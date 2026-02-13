use anyhow::{anyhow, bail};
use ibc_proto::google::protobuf::Any;
pub use ibc_types::lightclients::tendermint::misbehaviour::TENDERMINT_MISBEHAVIOUR_TYPE_URL;
use ibc_types::{
    core::client::Height,
    core::connection::ChainId,
    lightclients::tendermint::{
        client_state::{ClientState as TendermintClientState, TENDERMINT_CLIENT_STATE_TYPE_URL},
        consensus_state::{
            ConsensusState as TendermintConsensusState, TENDERMINT_CONSENSUS_STATE_TYPE_URL,
        },
        header::{Header as TendermintHeader, TENDERMINT_HEADER_TYPE_URL},
        TrustThreshold,
    },
};
use prost::Message;

pub const BANKD_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.bankd.v1.ClientState";
pub const BANKD_CONSENSUS_STATE_TYPE_URL: &str = "/ibc.lightclients.bankd.v1.ConsensusState";
pub const BANKD_HEADER_TYPE_URL: &str = "/ibc.lightclients.bankd.v1.Header";
pub const BANKD_MISBEHAVIOUR_TYPE_URL: &str = "/ibc.lightclients.bankd.v1.Misbehaviour";

pub type BankdClientState = penumbra_sdk_proto::ibc::lightclients::bankd::v1::ClientState;
pub type BankdConsensusState = penumbra_sdk_proto::ibc::lightclients::bankd::v1::ConsensusState;
pub type BankdHeader = penumbra_sdk_proto::ibc::lightclients::bankd::v1::Header;
pub type BankdMisbehaviour = penumbra_sdk_proto::ibc::lightclients::bankd::v1::Misbehaviour;

#[derive(Clone, Debug)]
pub enum AnyClientState {
    Tendermint(TendermintClientState),
    Bankd(BankdClientState),
}

#[derive(Clone, Debug)]
pub enum AnyConsensusState {
    Tendermint(TendermintConsensusState),
    Bankd(BankdConsensusState),
}

#[derive(Clone, Debug)]
pub enum AnyHeader {
    Tendermint(TendermintHeader),
    Bankd(BankdHeader),
}

// --- TryFrom<Any> implementations ---

impl TryFrom<Any> for AnyClientState {
    type Error = anyhow::Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        match any.type_url.as_str() {
            TENDERMINT_CLIENT_STATE_TYPE_URL => {
                let cs = TendermintClientState::try_from(any)
                    .map_err(|e| anyhow!("failed to deserialize tendermint client state: {e}"))?;
                Ok(AnyClientState::Tendermint(cs))
            }
            BANKD_CLIENT_STATE_TYPE_URL => {
                let cs = BankdClientState::decode(any.value.as_ref())
                    .map_err(|e| anyhow!("failed to deserialize bankd client state: {e}"))?;
                Ok(AnyClientState::Bankd(cs))
            }
            other => bail!("unknown client state type URL: {other}"),
        }
    }
}

impl TryFrom<Any> for AnyConsensusState {
    type Error = anyhow::Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        match any.type_url.as_str() {
            TENDERMINT_CONSENSUS_STATE_TYPE_URL => {
                let cs = TendermintConsensusState::try_from(any).map_err(|e| {
                    anyhow!("failed to deserialize tendermint consensus state: {e}")
                })?;
                Ok(AnyConsensusState::Tendermint(cs))
            }
            BANKD_CONSENSUS_STATE_TYPE_URL => {
                let cs = BankdConsensusState::decode(any.value.as_ref())
                    .map_err(|e| anyhow!("failed to deserialize bankd consensus state: {e}"))?;
                Ok(AnyConsensusState::Bankd(cs))
            }
            other => bail!("unknown consensus state type URL: {other}"),
        }
    }
}

impl TryFrom<Any> for AnyHeader {
    type Error = anyhow::Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        match any.type_url.as_str() {
            TENDERMINT_HEADER_TYPE_URL => {
                let h = TendermintHeader::try_from(any)
                    .map_err(|e| anyhow!("failed to deserialize tendermint header: {e}"))?;
                Ok(AnyHeader::Tendermint(h))
            }
            BANKD_HEADER_TYPE_URL => {
                let h = BankdHeader::decode(any.value.as_ref())
                    .map_err(|e| anyhow!("failed to deserialize bankd header: {e}"))?;
                Ok(AnyHeader::Bankd(h))
            }
            other => bail!("unknown header type URL: {other}"),
        }
    }
}

// --- From<Enum> for Any implementations ---

impl From<AnyClientState> for Any {
    fn from(cs: AnyClientState) -> Self {
        match cs {
            AnyClientState::Tendermint(cs) => cs.into(),
            AnyClientState::Bankd(cs) => Any {
                type_url: BANKD_CLIENT_STATE_TYPE_URL.to_string(),
                value: cs.encode_to_vec(),
            },
        }
    }
}

impl From<AnyConsensusState> for Any {
    fn from(cs: AnyConsensusState) -> Self {
        match cs {
            AnyConsensusState::Tendermint(cs) => cs.into(),
            AnyConsensusState::Bankd(cs) => Any {
                type_url: BANKD_CONSENSUS_STATE_TYPE_URL.to_string(),
                value: cs.encode_to_vec(),
            },
        }
    }
}

impl From<AnyHeader> for Any {
    fn from(h: AnyHeader) -> Self {
        match h {
            AnyHeader::Tendermint(h) => h.into(),
            AnyHeader::Bankd(h) => Any {
                type_url: BANKD_HEADER_TYPE_URL.to_string(),
                value: h.encode_to_vec(),
            },
        }
    }
}

// --- Accessor methods ---

impl AnyClientState {
    pub fn as_bankd(&self) -> anyhow::Result<&BankdClientState> {
        match self {
            AnyClientState::Bankd(cs) => Ok(cs),
            _ => bail!("expected bankd client state, got {}", self.client_type()),
        }
    }

    pub fn client_type(&self) -> &'static str {
        match self {
            AnyClientState::Tendermint(_) => "07-tendermint",
            AnyClientState::Bankd(_) => "08-commonware-bls",
        }
    }

    pub fn latest_height(&self) -> anyhow::Result<Height> {
        match self {
            AnyClientState::Tendermint(cs) => Ok(cs.latest_height()),
            AnyClientState::Bankd(cs) => {
                let h = cs
                    .latest_height
                    .as_ref()
                    .ok_or_else(|| anyhow!("bankd client state missing latest_height"))?;
                Ok(Height::new(h.revision_number, h.revision_height)?)
            }
        }
    }

    pub fn is_frozen(&self) -> bool {
        match self {
            AnyClientState::Tendermint(cs) => cs.is_frozen(),
            AnyClientState::Bankd(cs) => cs
                .frozen_height
                .as_ref()
                .map_or(false, |h| h.revision_number > 0 || h.revision_height > 0),
        }
    }

    pub fn proof_specs(&self) -> Vec<ics23::ProofSpec> {
        match self {
            AnyClientState::Tendermint(cs) => cs.proof_specs.clone(),
            // TODO: replace manual field-by-field mapping with a proper
            // From/Into conversion once ics23 exposes one. If ics23 adds
            // fields, this mapping will silently drop them.
            AnyClientState::Bankd(cs) => cs
                .proof_specs
                .iter()
                .map(|spec| ics23::ProofSpec {
                    leaf_spec: spec.leaf_spec.as_ref().map(|ls| ics23::LeafOp {
                        hash: ls.hash,
                        prehash_key: ls.prehash_key,
                        prehash_value: ls.prehash_value,
                        length: ls.length,
                        prefix: ls.prefix.clone(),
                    }),
                    inner_spec: spec.inner_spec.as_ref().map(|is| ics23::InnerSpec {
                        child_order: is.child_order.clone(),
                        child_size: is.child_size,
                        min_prefix_length: is.min_prefix_length,
                        max_prefix_length: is.max_prefix_length,
                        empty_child: is.empty_child.clone(),
                        hash: is.hash,
                    }),
                    max_depth: spec.max_depth,
                    min_depth: spec.min_depth,
                    prehash_key_before_comparison: spec.prehash_key_before_comparison,
                })
                .collect(),
        }
    }

    pub fn chain_id(&self) -> ChainId {
        match self {
            AnyClientState::Tendermint(cs) => cs.chain_id.clone(),
            AnyClientState::Bankd(cs) => ChainId::from_string(&cs.chain_id),
        }
    }

    pub fn trust_threshold(&self) -> Option<TrustThreshold> {
        match self {
            AnyClientState::Tendermint(cs) => Some(cs.trust_level),
            AnyClientState::Bankd(_) => None,
        }
    }

    /// Check if the client's trusting period has expired given the elapsed duration.
    /// Bankd clients with `trusting_period_secs == 0` never expire.
    pub fn expired(&self, elapsed: std::time::Duration) -> bool {
        match self {
            AnyClientState::Tendermint(cs) => cs.expired(elapsed),
            AnyClientState::Bankd(cs) => {
                cs.trusting_period_secs > 0 && elapsed.as_secs() >= cs.trusting_period_secs
            }
        }
    }

    /// Return the BLS12-381 group public key from the client state, if present.
    /// Tendermint clients do not have a group public key.
    pub fn group_public_key(&self) -> Option<&[u8]> {
        match self {
            AnyClientState::Tendermint(_) => None,
            AnyClientState::Bankd(cs) => {
                if cs.group_public_key.is_empty() {
                    None
                } else {
                    Some(&cs.group_public_key)
                }
            }
        }
    }

    /// Return the trusting period in seconds. Tendermint returns its trusting
    /// period converted to seconds; bankd returns the proto field directly.
    pub fn trusting_period_secs(&self) -> u64 {
        match self {
            AnyClientState::Tendermint(cs) => cs.trusting_period.as_secs(),
            AnyClientState::Bankd(cs) => cs.trusting_period_secs,
        }
    }

    /// Return a copy of the client state with the frozen height cleared.
    pub fn unfrozen(self) -> Self {
        match self {
            AnyClientState::Tendermint(cs) => AnyClientState::Tendermint(cs.unfrozen()),
            AnyClientState::Bankd(mut cs) => {
                cs.frozen_height = None;
                AnyClientState::Bankd(cs)
            }
        }
    }

    /// Return a copy of the client state with the given frozen height set.
    pub fn with_frozen_height(self, h: Height) -> Self {
        match self {
            AnyClientState::Tendermint(cs) => AnyClientState::Tendermint(cs.with_frozen_height(h)),
            AnyClientState::Bankd(mut cs) => {
                cs.frozen_height = Some(ibc_proto::ibc::core::client::v1::Height {
                    revision_number: h.revision_number,
                    revision_height: h.revision_height,
                });
                AnyClientState::Bankd(cs)
            }
        }
    }

    /// Check that the given height is not greater than the client's latest height.
    pub fn verify_height(&self, height: Height) -> anyhow::Result<()> {
        match self {
            AnyClientState::Tendermint(cs) => Ok(cs.verify_height(height)?),
            AnyClientState::Bankd(_) => {
                let latest = self
                    .latest_height()
                    .map_err(|e| anyhow!("bankd verify_height: {e}"))?;
                if latest < height {
                    anyhow::bail!(
                        "client height {} is less than verification height {}",
                        latest,
                        height
                    );
                }
                Ok(())
            }
        }
    }
}

impl AnyConsensusState {
    pub fn as_bankd(&self) -> anyhow::Result<&BankdConsensusState> {
        match self {
            AnyConsensusState::Bankd(cs) => Ok(cs),
            _ => bail!("expected bankd consensus state"),
        }
    }

    pub fn root(&self) -> Vec<u8> {
        match self {
            AnyConsensusState::Tendermint(cs) => cs.root.hash.clone(),
            AnyConsensusState::Bankd(cs) => cs.root.clone(),
        }
    }

    pub fn timestamp_as_unix_secs(&self) -> anyhow::Result<u64> {
        match self {
            AnyConsensusState::Tendermint(cs) => {
                let unix_ts = cs.timestamp.unix_timestamp();
                anyhow::ensure!(
                    unix_ts >= 0,
                    "negative consensus state timestamp: {}",
                    unix_ts
                );
                Ok(unix_ts as u64)
            }
            AnyConsensusState::Bankd(cs) => Ok(cs.timestamp),
        }
    }

    pub fn timestamp(&self) -> anyhow::Result<tendermint::Time> {
        match self {
            AnyConsensusState::Tendermint(cs) => Ok(cs.timestamp),
            AnyConsensusState::Bankd(cs) => {
                tendermint::Time::from_unix_timestamp(cs.timestamp as i64, 0)
                    .map_err(|e| anyhow!("bankd timestamp to Time: {e}"))
            }
        }
    }

    pub fn timestamp_nanos(&self) -> anyhow::Result<u64> {
        match self {
            AnyConsensusState::Tendermint(cs) => Ok(cs.timestamp.unix_timestamp_nanos() as u64),
            AnyConsensusState::Bankd(cs) => Ok(cs
                .timestamp
                .checked_mul(1_000_000_000)
                .ok_or_else(|| anyhow!("bankd timestamp overflow converting to nanos"))?),
        }
    }

    /// Return the BLS12-381 group public key from the consensus state, if present.
    /// Tendermint consensus states do not have a group public key.
    pub fn group_public_key(&self) -> Option<&[u8]> {
        match self {
            AnyConsensusState::Tendermint(_) => None,
            AnyConsensusState::Bankd(cs) => {
                if cs.group_public_key.is_empty() {
                    None
                } else {
                    Some(&cs.group_public_key)
                }
            }
        }
    }
}

impl AnyHeader {
    pub fn as_bankd(&self) -> anyhow::Result<&BankdHeader> {
        match self {
            AnyHeader::Bankd(h) => Ok(h),
            _ => bail!("expected bankd header"),
        }
    }

    pub fn height(&self) -> anyhow::Result<Height> {
        match self {
            AnyHeader::Tendermint(h) => Ok(h.height()),
            AnyHeader::Bankd(h) => {
                let ht = h
                    .height
                    .as_ref()
                    .ok_or_else(|| anyhow!("bankd header missing height"))?;
                Ok(Height::new(ht.revision_number, ht.revision_height)?)
            }
        }
    }

    pub fn trusted_height(&self) -> anyhow::Result<Height> {
        match self {
            AnyHeader::Tendermint(h) => Ok(h.trusted_height),
            AnyHeader::Bankd(h) => {
                let ht = h
                    .trusted_height
                    .as_ref()
                    .ok_or_else(|| anyhow!("bankd header missing trusted_height"))?;
                Ok(Height::new(ht.revision_number, ht.revision_height)?)
            }
        }
    }

    pub fn timestamp_as_unix_secs(&self) -> anyhow::Result<u64> {
        match self {
            AnyHeader::Tendermint(h) => {
                let unix_ts = h.signed_header.header.time.unix_timestamp();
                anyhow::ensure!(unix_ts >= 0, "negative header timestamp: {}", unix_ts);
                Ok(unix_ts as u64)
            }
            AnyHeader::Bankd(h) => Ok(h.timestamp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use ibc_proto::google::protobuf::Any;
    use ibc_types::DomainType as _;

    // ---------------------------------------------------------------
    // Helpers: construct minimal but valid bankd proto types
    // ---------------------------------------------------------------

    /// 48-byte fake BLS12-381 G1Affine compressed key for tests.
    fn fake_group_public_key() -> Vec<u8> {
        vec![0xBB; 48]
    }

    fn bankd_client_state(chain_id: &str, rev_number: u64, rev_height: u64) -> BankdClientState {
        BankdClientState {
            chain_id: chain_id.to_string(),
            latest_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: rev_number,
                revision_height: rev_height,
            }),
            frozen_height: None,
            proof_specs: vec![jmt_ics23_spec()],
            group_public_key: fake_group_public_key(),
            trusting_period_secs: 86_400, // 1 day
        }
    }

    fn bankd_consensus_state(root: Vec<u8>, timestamp: u64) -> BankdConsensusState {
        BankdConsensusState {
            root,
            timestamp,
            group_public_key: fake_group_public_key(),
        }
    }

    fn bankd_header(rev_number: u64, rev_height: u64, timestamp: u64) -> BankdHeader {
        BankdHeader {
            height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: rev_number,
                revision_height: rev_height,
            }),
            trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: rev_number,
                revision_height: rev_height.saturating_sub(1),
            }),
            timestamp,
            new_root: vec![0xab; 32],
            parent_hash: vec![0x01; 32],
            prevrandao: vec![0x02; 32],
            state_root: vec![0x03; 32],
            ibc_root: vec![0x04; 32],
            transactions: vec![vec![0x05; 64], vec![0x06; 64]],
            finalization_certificate: vec![0x07; 240],
        }
    }

    /// JMT proof spec matching the vendored spec in prefix.rs (single-spec for bankd).
    fn jmt_ics23_spec() -> ics23::ProofSpec {
        ics23::ProofSpec {
            leaf_spec: Some(ics23::LeafOp {
                hash: ics23::HashOp::Sha256.into(),
                prehash_key: ics23::HashOp::Sha256.into(),
                prehash_value: ics23::HashOp::Sha256.into(),
                length: ics23::LengthOp::NoPrefix.into(),
                prefix: b"JMT::LeafNode".to_vec(),
            }),
            inner_spec: Some(ics23::InnerSpec {
                hash: ics23::HashOp::Sha256.into(),
                child_order: vec![0, 1],
                min_prefix_length: b"JMT::IntrnalNode".len() as i32,
                max_prefix_length: b"JMT::IntrnalNode".len() as i32,
                child_size: 32,
                empty_child: b"SPARSE_MERKLE_PLACEHOLDER_HASH__".to_vec(),
            }),
            min_depth: 0,
            max_depth: 64,
            prehash_key_before_comparison: true,
        }
    }

    // ---------------------------------------------------------------
    // AnyClientState round-trip tests
    // ---------------------------------------------------------------

    #[test]
    fn any_client_state_bankd_round_trip() {
        let cs = bankd_client_state("bankd-testnet-1", 0, 42);
        let any_cs: Any = AnyClientState::Bankd(cs.clone()).into();
        assert_eq!(any_cs.type_url, BANKD_CLIENT_STATE_TYPE_URL);

        let recovered = AnyClientState::try_from(any_cs).expect("round-trip should succeed");
        match recovered {
            AnyClientState::Bankd(inner) => assert_eq!(inner, cs),
            _ => panic!("expected Bankd variant"),
        }
    }

    #[test]
    fn any_client_state_tendermint_round_trip() {
        // Use the real Stargaze create_client fixture to get a valid TendermintClientState
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("component/test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg = ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice())
            .expect("valid MsgCreateClient");

        let tm_cs = ibc_types::lightclients::tendermint::client_state::ClientState::try_from(
            msg.client_state,
        )
        .expect("valid TendermintClientState");

        let any_cs: Any = AnyClientState::Tendermint(tm_cs.clone()).into();
        assert_eq!(any_cs.type_url, TENDERMINT_CLIENT_STATE_TYPE_URL);

        let recovered = AnyClientState::try_from(any_cs).expect("round-trip should succeed");
        match recovered {
            AnyClientState::Tendermint(inner) => {
                assert_eq!(inner.chain_id, tm_cs.chain_id);
                assert_eq!(inner.latest_height(), tm_cs.latest_height());
            }
            _ => panic!("expected Tendermint variant"),
        }
    }

    #[test]
    fn any_client_state_unknown_type_url_rejected() {
        let bad = Any {
            type_url: "/ibc.lightclients.fake.v1.ClientState".to_string(),
            value: vec![1, 2, 3],
        };
        let err = AnyClientState::try_from(bad).unwrap_err();
        assert!(
            err.to_string().contains("unknown client state type URL"),
            "error should mention unknown type URL, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // AnyConsensusState round-trip tests
    // ---------------------------------------------------------------

    #[test]
    fn any_consensus_state_bankd_round_trip() {
        let cs = bankd_consensus_state(vec![0xaa; 32], 1_700_000_000);
        let any_cs: Any = AnyConsensusState::Bankd(cs.clone()).into();
        assert_eq!(any_cs.type_url, BANKD_CONSENSUS_STATE_TYPE_URL);

        let recovered = AnyConsensusState::try_from(any_cs).expect("round-trip should succeed");
        match recovered {
            AnyConsensusState::Bankd(inner) => assert_eq!(inner, cs),
            _ => panic!("expected Bankd variant"),
        }
    }

    #[test]
    fn any_consensus_state_tendermint_round_trip() {
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("component/test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg = ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice())
            .expect("valid MsgCreateClient");

        let tm_cons =
            ibc_types::lightclients::tendermint::consensus_state::ConsensusState::try_from(
                msg.consensus_state,
            )
            .expect("valid TendermintConsensusState");

        let any_cs: Any = AnyConsensusState::Tendermint(tm_cons.clone()).into();
        assert_eq!(any_cs.type_url, TENDERMINT_CONSENSUS_STATE_TYPE_URL);

        let recovered = AnyConsensusState::try_from(any_cs).expect("round-trip should succeed");
        match recovered {
            AnyConsensusState::Tendermint(inner) => {
                assert_eq!(inner.root, tm_cons.root);
                assert_eq!(inner.timestamp, tm_cons.timestamp);
            }
            _ => panic!("expected Tendermint variant"),
        }
    }

    #[test]
    fn any_consensus_state_unknown_type_url_rejected() {
        let bad = Any {
            type_url: "/ibc.lightclients.fake.v1.ConsensusState".to_string(),
            value: vec![1, 2, 3],
        };
        let err = AnyConsensusState::try_from(bad).unwrap_err();
        assert!(
            err.to_string().contains("unknown consensus state type URL"),
            "error should mention unknown type URL, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // AnyHeader round-trip tests
    // ---------------------------------------------------------------

    #[test]
    fn any_header_bankd_round_trip() {
        let h = bankd_header(0, 100, 1_700_000_000);
        let any_h: Any = AnyHeader::Bankd(h.clone()).into();
        assert_eq!(any_h.type_url, BANKD_HEADER_TYPE_URL);

        let recovered = AnyHeader::try_from(any_h).expect("round-trip should succeed");
        match recovered {
            AnyHeader::Bankd(inner) => assert_eq!(inner, h),
            _ => panic!("expected Bankd variant"),
        }
    }

    #[test]
    fn any_header_unknown_type_url_rejected() {
        let bad = Any {
            type_url: "/ibc.lightclients.fake.v1.Header".to_string(),
            value: vec![1, 2, 3],
        };
        let err = AnyHeader::try_from(bad).unwrap_err();
        assert!(
            err.to_string().contains("unknown header type URL"),
            "error should mention unknown type URL, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // AnyClientState accessor dispatch tests
    // ---------------------------------------------------------------

    #[test]
    fn bankd_client_state_latest_height() {
        let cs = AnyClientState::Bankd(bankd_client_state("test-chain", 1, 99));
        let h = cs.latest_height().expect("should have height");
        assert_eq!(h.revision_number, 1);
        assert_eq!(h.revision_height, 99);
    }

    #[test]
    fn bankd_client_state_missing_height_errors() {
        let mut cs = bankd_client_state("test-chain", 0, 1);
        cs.latest_height = None;
        let err = AnyClientState::Bankd(cs).latest_height().unwrap_err();
        assert!(err.to_string().contains("missing latest_height"));
    }

    #[test]
    fn bankd_client_state_client_type() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        assert_eq!(cs.client_type(), "08-commonware-bls");
    }

    #[test]
    fn tendermint_client_state_client_type() {
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("component/test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg = ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice())
            .expect("valid MsgCreateClient");
        let tm_cs = ibc_types::lightclients::tendermint::client_state::ClientState::try_from(
            msg.client_state,
        )
        .expect("valid");
        let cs = AnyClientState::Tendermint(tm_cs);
        assert_eq!(cs.client_type(), "07-tendermint");
    }

    #[test]
    fn bankd_client_state_is_frozen_false() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        assert!(!cs.is_frozen());
    }

    #[test]
    fn bankd_client_state_is_frozen_true() {
        let mut inner = bankd_client_state("test", 0, 1);
        inner.frozen_height = Some(ibc_proto::ibc::core::client::v1::Height {
            revision_number: 0,
            revision_height: 1,
        });
        assert!(AnyClientState::Bankd(inner).is_frozen());
    }

    #[test]
    fn bankd_client_state_frozen_zero_height_not_frozen() {
        let mut inner = bankd_client_state("test", 0, 1);
        inner.frozen_height = Some(ibc_proto::ibc::core::client::v1::Height {
            revision_number: 0,
            revision_height: 0,
        });
        assert!(!AnyClientState::Bankd(inner).is_frozen());
    }

    #[test]
    fn bankd_client_state_proof_specs_returns_jmt_spec() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        let specs = cs.proof_specs();
        assert_eq!(specs.len(), 1, "bankd should have 1 proof spec (JMT only)");
        assert_eq!(specs[0].max_depth, 64);
        assert!(specs[0].prehash_key_before_comparison);
    }

    #[test]
    fn bankd_client_state_chain_id() {
        let cs = AnyClientState::Bankd(bankd_client_state("bankd-mainnet-1", 0, 1));
        assert_eq!(cs.chain_id().as_str(), "bankd-mainnet-1");
    }

    #[test]
    fn bankd_client_state_trust_threshold_is_none() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        assert!(cs.trust_threshold().is_none());
    }

    #[test]
    fn bankd_client_state_expires_after_trusting_period() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        // Default trusting_period_secs is 86_400 (1 day)
        assert!(!cs.expired(std::time::Duration::from_secs(86_399)));
        assert!(cs.expired(std::time::Duration::from_secs(86_400)));
        assert!(cs.expired(std::time::Duration::from_secs(999_999_999)));
    }

    #[test]
    fn bankd_client_state_zero_trusting_period_never_expires() {
        let mut inner = bankd_client_state("test", 0, 1);
        inner.trusting_period_secs = 0;
        let cs = AnyClientState::Bankd(inner);
        assert!(!cs.expired(std::time::Duration::from_secs(999_999_999)));
    }

    #[test]
    fn bankd_client_state_freeze_and_unfreeze() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        assert!(!cs.is_frozen());

        let frozen = cs.with_frozen_height(Height::new(0, 5).expect("valid height"));
        assert!(frozen.is_frozen());

        let unfrozen = frozen.unfrozen();
        assert!(!unfrozen.is_frozen());
    }

    #[test]
    fn bankd_client_state_verify_height_ok() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 100));
        assert!(cs.verify_height(Height::new(0, 50).expect("valid")).is_ok());
        assert!(cs
            .verify_height(Height::new(0, 100).expect("valid"))
            .is_ok());
    }

    #[test]
    fn bankd_client_state_verify_height_too_high() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 100));
        let err = cs
            .verify_height(Height::new(0, 101).expect("valid"))
            .unwrap_err();
        assert!(err.to_string().contains("less than verification height"));
    }

    // ---------------------------------------------------------------
    // AnyConsensusState accessor dispatch tests
    // ---------------------------------------------------------------

    #[test]
    fn bankd_consensus_state_root() {
        let root_bytes = vec![0xde; 32];
        let cs = AnyConsensusState::Bankd(bankd_consensus_state(root_bytes.clone(), 100));
        assert_eq!(cs.root(), root_bytes);
    }

    #[test]
    fn bankd_consensus_state_timestamp_as_unix_secs() {
        let cs = AnyConsensusState::Bankd(bankd_consensus_state(vec![], 1_700_000_000));
        assert_eq!(
            cs.timestamp_as_unix_secs().expect("should succeed"),
            1_700_000_000
        );
    }

    #[test]
    fn bankd_consensus_state_timestamp() {
        let cs = AnyConsensusState::Bankd(bankd_consensus_state(vec![], 1_700_000_000));
        let t = cs.timestamp().expect("should succeed");
        assert_eq!(t.unix_timestamp(), 1_700_000_000);
    }

    #[test]
    fn bankd_consensus_state_timestamp_nanos() {
        let cs = AnyConsensusState::Bankd(bankd_consensus_state(vec![], 100));
        assert_eq!(
            cs.timestamp_nanos().expect("should succeed"),
            100_000_000_000
        );
    }

    // ---------------------------------------------------------------
    // AnyHeader accessor dispatch tests
    // ---------------------------------------------------------------

    #[test]
    fn bankd_header_height() {
        let h = AnyHeader::Bankd(bankd_header(0, 42, 100));
        let ht = h.height().expect("should succeed");
        assert_eq!(ht.revision_height, 42);
    }

    #[test]
    fn bankd_header_trusted_height() {
        let h = AnyHeader::Bankd(bankd_header(0, 42, 100));
        let ht = h.trusted_height().expect("should succeed");
        assert_eq!(ht.revision_height, 41);
    }

    #[test]
    fn bankd_header_timestamp_as_unix_secs() {
        let h = AnyHeader::Bankd(bankd_header(0, 1, 1_700_000_000));
        assert_eq!(
            h.timestamp_as_unix_secs().expect("should succeed"),
            1_700_000_000
        );
    }

    #[test]
    fn bankd_header_missing_height_errors() {
        let mut inner = bankd_header(0, 1, 100);
        inner.height = None;
        let err = AnyHeader::Bankd(inner).height().unwrap_err();
        assert!(err.to_string().contains("missing height"));
    }

    #[test]
    fn bankd_header_missing_trusted_height_errors() {
        let mut inner = bankd_header(0, 1, 100);
        inner.trusted_height = None;
        let err = AnyHeader::Bankd(inner).trusted_height().unwrap_err();
        assert!(err.to_string().contains("missing trusted_height"));
    }

    // ---------------------------------------------------------------
    // New field accessor tests
    // ---------------------------------------------------------------

    #[test]
    fn bankd_client_state_group_public_key() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        let gpk = cs.group_public_key().expect("should have group public key");
        assert_eq!(gpk.len(), 48);
        assert_eq!(gpk, &fake_group_public_key()[..]);
    }

    #[test]
    fn bankd_client_state_group_public_key_empty_is_none() {
        let mut inner = bankd_client_state("test", 0, 1);
        inner.group_public_key = vec![];
        let cs = AnyClientState::Bankd(inner);
        assert!(cs.group_public_key().is_none());
    }

    #[test]
    fn tendermint_client_state_group_public_key_is_none() {
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("component/test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg = ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice())
            .expect("valid MsgCreateClient");
        let tm_cs = ibc_types::lightclients::tendermint::client_state::ClientState::try_from(
            msg.client_state,
        )
        .expect("valid");
        assert!(AnyClientState::Tendermint(tm_cs)
            .group_public_key()
            .is_none());
    }

    #[test]
    fn bankd_client_state_trusting_period_secs() {
        let cs = AnyClientState::Bankd(bankd_client_state("test", 0, 1));
        assert_eq!(cs.trusting_period_secs(), 86_400);
    }

    #[test]
    fn bankd_consensus_state_group_public_key() {
        let cs = AnyConsensusState::Bankd(bankd_consensus_state(vec![0xaa; 32], 100));
        let gpk = cs.group_public_key().expect("should have group public key");
        assert_eq!(gpk.len(), 48);
        assert_eq!(gpk, &fake_group_public_key()[..]);
    }

    #[test]
    fn bankd_consensus_state_group_public_key_empty_is_none() {
        let mut inner = bankd_consensus_state(vec![0xaa; 32], 100);
        inner.group_public_key = vec![];
        let cs = AnyConsensusState::Bankd(inner);
        assert!(cs.group_public_key().is_none());
    }

    #[test]
    fn tendermint_consensus_state_group_public_key_is_none() {
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("component/test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg = ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice())
            .expect("valid MsgCreateClient");
        let tm_cons =
            ibc_types::lightclients::tendermint::consensus_state::ConsensusState::try_from(
                msg.consensus_state,
            )
            .expect("valid");
        assert!(AnyConsensusState::Tendermint(tm_cons)
            .group_public_key()
            .is_none());
    }

    // ---------------------------------------------------------------
    // Round-trip tests for new fields
    // ---------------------------------------------------------------

    #[test]
    fn bankd_client_state_new_fields_round_trip() {
        let cs = bankd_client_state("bankd-testnet-1", 0, 42);
        assert_eq!(cs.group_public_key.len(), 48);
        assert_eq!(cs.trusting_period_secs, 86_400);

        let any: Any = AnyClientState::Bankd(cs.clone()).into();
        let recovered = AnyClientState::try_from(any).expect("round-trip should succeed");
        match recovered {
            AnyClientState::Bankd(inner) => {
                assert_eq!(inner.group_public_key, cs.group_public_key);
                assert_eq!(inner.trusting_period_secs, cs.trusting_period_secs);
            }
            _ => panic!("expected Bankd variant"),
        }
    }

    #[test]
    fn bankd_consensus_state_new_fields_round_trip() {
        let cs = bankd_consensus_state(vec![0xaa; 32], 1_700_000_000);
        assert_eq!(cs.group_public_key.len(), 48);

        let any: Any = AnyConsensusState::Bankd(cs.clone()).into();
        let recovered = AnyConsensusState::try_from(any).expect("round-trip should succeed");
        match recovered {
            AnyConsensusState::Bankd(inner) => {
                assert_eq!(inner.group_public_key, cs.group_public_key);
            }
            _ => panic!("expected Bankd variant"),
        }
    }

    #[test]
    fn bankd_header_new_fields_round_trip() {
        let h = bankd_header(0, 100, 1_700_000_000);
        assert_eq!(h.parent_hash, vec![0x01; 32]);
        assert_eq!(h.prevrandao, vec![0x02; 32]);
        assert_eq!(h.state_root, vec![0x03; 32]);
        assert_eq!(h.ibc_root, vec![0x04; 32]);
        assert_eq!(h.transactions.len(), 2);
        assert_eq!(h.finalization_certificate.len(), 240);

        let any: Any = AnyHeader::Bankd(h.clone()).into();
        let recovered = AnyHeader::try_from(any).expect("round-trip should succeed");
        match recovered {
            AnyHeader::Bankd(inner) => {
                assert_eq!(inner.parent_hash, h.parent_hash);
                assert_eq!(inner.prevrandao, h.prevrandao);
                assert_eq!(inner.state_root, h.state_root);
                assert_eq!(inner.ibc_root, h.ibc_root);
                assert_eq!(inner.transactions, h.transactions);
                assert_eq!(inner.finalization_certificate, h.finalization_certificate);
            }
            _ => panic!("expected Bankd variant"),
        }
    }

    // ---------------------------------------------------------------
    // BankdMisbehaviour round-trip tests
    // ---------------------------------------------------------------

    #[test]
    fn bankd_misbehaviour_round_trip() {
        use prost::Message as _;

        let h1 = bankd_header(0, 42, 1_700_000_000);
        let mut h2 = bankd_header(0, 42, 1_700_000_000);
        h2.new_root = vec![0xcd; 32]; // equivocation: same height, different data

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1.clone()),
            header_2: Some(h2.clone()),
        };

        let encoded = prost::Message::encode_to_vec(&mb);
        let decoded =
            BankdMisbehaviour::decode(encoded.as_ref()).expect("round-trip decode should succeed");

        assert_eq!(decoded.client_id, "bankd-0");
        assert_eq!(decoded.header_1, Some(h1));
        assert_eq!(decoded.header_2, Some(h2));
    }

    #[test]
    fn bankd_misbehaviour_any_round_trip() {
        use prost::Message as _;

        let h1 = bankd_header(0, 42, 1_700_000_000);
        let mut h2 = bankd_header(0, 42, 1_700_000_000);
        h2.new_root = vec![0xcd; 32];

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1.clone()),
            header_2: Some(h2.clone()),
        };

        let any = Any {
            type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
            value: prost::Message::encode_to_vec(&mb),
        };

        assert_eq!(any.type_url, BANKD_MISBEHAVIOUR_TYPE_URL);

        let decoded = BankdMisbehaviour::decode(any.value.as_ref())
            .expect("round-trip via Any should succeed");
        assert_eq!(decoded.client_id, "bankd-0");
        assert_eq!(decoded.header_1, Some(h1));
        assert_eq!(decoded.header_2, Some(h2));
    }
}
