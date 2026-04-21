//! IBC compliance bridging.
//!
//! Encodes compliance metadata in the ICS-20 memo field so regulated assets
//! can cross IBC channels. The metadata is base64-encoded protobuf wrapped
//! in a JSON object: `{"penumbra_compliance": "<base64>", "memo": "..."}`.

use anyhow::{Context, Result};
use base64::Engine as _;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::core::component::compliance::v1 as pb;
use penumbra_sdk_proto::Message as _;

/// Prefix key used to identify compliance metadata in the ICS-20 memo JSON.
const MEMO_KEY: &str = "penumbra_compliance";

/// Compliance metadata for IBC transfers of regulated assets.
///
/// Carries the transfer-input ciphertext and minimal context so the issuer can
/// track its regulated asset across IBC channels.
#[derive(Clone, Debug)]
pub struct IbcComplianceMetadata {
    /// The compliance ciphertext from the transfer-side input bundle (TRANSFER_INPUT_WIRE_BYTES bytes).
    pub compliance_ciphertext: Vec<u8>,
    /// The asset ID being transferred.
    pub asset_id: asset::Id,
}

impl IbcComplianceMetadata {
    /// Encode compliance metadata into an ICS-20 memo string.
    pub fn encode_to_memo(&self, existing_memo: &str) -> Result<String> {
        let proto = self.to_proto_public();
        let proto_bytes = proto.encode_to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&proto_bytes);

        let memo_obj = if existing_memo.is_empty() {
            serde_json::json!({ MEMO_KEY: b64 })
        } else {
            serde_json::json!({ MEMO_KEY: b64, "memo": existing_memo })
        };

        serde_json::to_string(&memo_obj).context("failed to encode compliance memo as JSON")
    }

    /// Try to parse compliance metadata from an ICS-20 memo string.
    ///
    /// Returns `None` if the memo doesn't contain compliance metadata.
    pub fn from_memo(memo: &str) -> Result<Option<Self>> {
        if memo.is_empty() {
            return Ok(None);
        }

        let parsed: serde_json::Value = match serde_json::from_str(memo) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        let b64 = match parsed.get(MEMO_KEY).and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return Ok(None),
        };

        let proto_bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .context("invalid base64 in compliance memo")?;

        let proto = pb::IbcComplianceMetadata::decode(proto_bytes.as_slice())
            .context("invalid protobuf in compliance memo")?;

        Ok(Some(Self::from_proto_public(proto)?))
    }

    /// Check if a memo string contains compliance metadata (cheap heuristic).
    pub fn is_compliance_memo(memo: &str) -> bool {
        memo.contains(MEMO_KEY)
    }

    /// Extract the user memo from a compliance-wrapped memo string.
    pub fn extract_user_memo(memo: &str) -> Option<String> {
        let parsed: serde_json::Value = serde_json::from_str(memo).ok()?;
        parsed
            .get("memo")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convert to protobuf representation.
    pub fn to_proto_public(&self) -> pb::IbcComplianceMetadata {
        pb::IbcComplianceMetadata {
            compliance_ciphertext: self.compliance_ciphertext.clone(),
            asset_id: Some(self.asset_id.into()),
        }
    }

    /// Parse from protobuf representation.
    pub fn from_proto_public(proto: pb::IbcComplianceMetadata) -> Result<Self> {
        use crate::structs::TRANSFER_INPUT_WIRE_BYTES;
        anyhow::ensure!(
            proto.compliance_ciphertext.len() == TRANSFER_INPUT_WIRE_BYTES,
            "IBC compliance ciphertext must be {TRANSFER_INPUT_WIRE_BYTES} bytes, got {}",
            proto.compliance_ciphertext.len()
        );

        let asset_id = proto
            .asset_id
            .ok_or_else(|| anyhow::anyhow!("missing asset_id in IBC compliance metadata"))?
            .try_into()
            .context("invalid asset_id")?;

        Ok(Self {
            compliance_ciphertext: proto.compliance_ciphertext,
            asset_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::TRANSFER_INPUT_WIRE_BYTES;
    use decaf377::Fq;

    #[test]
    fn roundtrip_encode_decode() {
        let metadata = IbcComplianceMetadata {
            compliance_ciphertext: vec![1u8; TRANSFER_INPUT_WIRE_BYTES],
            asset_id: asset::Id(Fq::from(12345u64)),
        };

        let memo = metadata.encode_to_memo("").unwrap();
        assert!(IbcComplianceMetadata::is_compliance_memo(&memo));

        let decoded = IbcComplianceMetadata::from_memo(&memo)
            .unwrap()
            .expect("should decode");
        assert_eq!(
            decoded.compliance_ciphertext,
            metadata.compliance_ciphertext
        );
        assert_eq!(decoded.asset_id, metadata.asset_id);
    }

    #[test]
    fn preserves_user_memo() {
        let metadata = IbcComplianceMetadata {
            compliance_ciphertext: vec![0u8; TRANSFER_INPUT_WIRE_BYTES],
            asset_id: asset::Id(Fq::from(1u64)),
        };

        let memo = metadata.encode_to_memo("hello world").unwrap();
        let user_memo = IbcComplianceMetadata::extract_user_memo(&memo);
        assert_eq!(user_memo.as_deref(), Some("hello world"));
    }

    #[test]
    fn non_compliance_memo_returns_none() {
        assert!(IbcComplianceMetadata::from_memo("").unwrap().is_none());
        assert!(IbcComplianceMetadata::from_memo("just a string")
            .unwrap()
            .is_none());
        assert!(IbcComplianceMetadata::from_memo("{}").unwrap().is_none());
        assert!(IbcComplianceMetadata::from_memo(r#"{"other": "data"}"#)
            .unwrap()
            .is_none());
    }

    #[test]
    fn wrong_ciphertext_size_rejected() {
        let proto = pb::IbcComplianceMetadata {
            compliance_ciphertext: vec![0u8; 100], // Wrong size
            asset_id: Some(asset::Id(Fq::from(1u64)).into()),
        };
        let result = IbcComplianceMetadata::from_proto_public(proto);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("224 bytes"),
            "error should mention expected size"
        );
    }

    #[test]
    fn missing_asset_id_rejected() {
        let proto = pb::IbcComplianceMetadata {
            compliance_ciphertext: vec![0u8; TRANSFER_INPUT_WIRE_BYTES],
            asset_id: None,
        };
        let result = IbcComplianceMetadata::from_proto_public(proto);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing asset_id"));
    }

    #[test]
    fn roundtrip_with_existing_memo() {
        let metadata = IbcComplianceMetadata {
            compliance_ciphertext: vec![0xAB; TRANSFER_INPUT_WIRE_BYTES],
            asset_id: asset::Id(Fq::from(999u64)),
        };

        let memo = metadata.encode_to_memo("transfer from Alice").unwrap();

        // Should contain both compliance data and user memo
        assert!(IbcComplianceMetadata::is_compliance_memo(&memo));
        let user_memo = IbcComplianceMetadata::extract_user_memo(&memo);
        assert_eq!(user_memo.as_deref(), Some("transfer from Alice"));

        // Should roundtrip compliance data
        let decoded = IbcComplianceMetadata::from_memo(&memo)
            .unwrap()
            .expect("should decode");
        assert_eq!(
            decoded.compliance_ciphertext,
            vec![0xAB; TRANSFER_INPUT_WIRE_BYTES]
        );
        assert_eq!(decoded.asset_id, asset::Id(Fq::from(999u64)));
    }

    #[test]
    fn extract_user_memo_no_memo_field() {
        // JSON with compliance data but no "memo" key
        let metadata = IbcComplianceMetadata {
            compliance_ciphertext: vec![0u8; TRANSFER_INPUT_WIRE_BYTES],
            asset_id: asset::Id(Fq::from(1u64)),
        };
        let memo = metadata.encode_to_memo("").unwrap();
        assert!(IbcComplianceMetadata::extract_user_memo(&memo).is_none());
    }

    #[test]
    fn is_compliance_memo_rejects_plain_text() {
        assert!(!IbcComplianceMetadata::is_compliance_memo("hello"));
        assert!(!IbcComplianceMetadata::is_compliance_memo("{}"));
        assert!(!IbcComplianceMetadata::is_compliance_memo(
            r#"{"memo": "test"}"#
        ));
    }
}
