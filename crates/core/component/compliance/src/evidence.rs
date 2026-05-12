use anyhow::{anyhow, bail, ensure, Context, Result};
use decaf377::Fq;
use penumbra_sdk_asset::asset;
use sha2::{Digest, Sha256};

use crate::{
    decode_object::{
        PublicTransferTierDecodeObject, TransferTierKind, TransferTierMetadataStatement,
    },
    structs::DleqProof,
    transfer::{
        TransferComplianceCiphertext, TransferComplianceDleqProofs, TRANSFER_DLEQ_BYTES,
        TRANSFER_WIRE_BYTES,
    },
    ActionRef, BlockRef, OutputRef, TransferOrbisUploadBundle, TxRef,
};

pub const COMPLIANCE_EVIDENCE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceObjectType {
    Transfer = 1,
}

impl EvidenceObjectType {
    fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            1 => Ok(Self::Transfer),
            other => bail!("unknown evidence object type {other}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComplianceEvidenceObject {
    pub version: u32,
    pub object_type: EvidenceObjectType,
    pub output_ref: OutputRef,
    pub asset_id: asset::Id,
    pub is_flagged: bool,
    pub detection_salt: Fq,
    pub transfer_ciphertext: TransferComplianceCiphertext,
    pub transfer_dleq_bundle: TransferComplianceDleqProofs,
    pub tier_objects: Vec<PublicTransferTierDecodeObject>,
    pub orbis_upload_bundle_hash: Option<[u8; 32]>,
    pub payload_hash: [u8; 32],
}

impl ComplianceEvidenceObject {
    pub fn new_transfer(
        output_ref: OutputRef,
        asset_id: asset::Id,
        is_flagged: bool,
        detection_salt: Fq,
        transfer_ciphertext: TransferComplianceCiphertext,
        transfer_dleq_bundle: TransferComplianceDleqProofs,
        tier_objects: Vec<PublicTransferTierDecodeObject>,
        upload_bundle: Option<&TransferOrbisUploadBundle>,
    ) -> Result<Self> {
        let orbis_upload_bundle_hash = upload_bundle
            .map(|bundle| bundle.to_bytes().map(|bytes| Sha256::digest(bytes).into()))
            .transpose()?;
        let mut evidence = Self {
            version: COMPLIANCE_EVIDENCE_VERSION,
            object_type: EvidenceObjectType::Transfer,
            output_ref,
            asset_id,
            is_flagged,
            detection_salt,
            transfer_ciphertext,
            transfer_dleq_bundle,
            tier_objects,
            orbis_upload_bundle_hash,
            payload_hash: [0u8; 32],
        };
        evidence.payload_hash = evidence.compute_payload_hash();
        Ok(evidence)
    }

    pub fn from_upload_bundle(
        output_ref: OutputRef,
        asset_id: asset::Id,
        is_flagged: bool,
        detection_salt: Fq,
        transfer_ciphertext: TransferComplianceCiphertext,
        upload_bundle: &TransferOrbisUploadBundle,
    ) -> Result<Self> {
        let proofs = TransferComplianceDleqProofs {
            sender_core: dleq_from_upload_package(&upload_bundle.sender_core)?,
            sender_ext: dleq_from_upload_package(&upload_bundle.sender_ext)?,
            output_core: dleq_from_upload_package(&upload_bundle.output_core)?,
            output_ext: dleq_from_upload_package(&upload_bundle.output_ext)?,
        };
        let tier_objects = vec![
            PublicTransferTierDecodeObject::new(
                upload_bundle.sender_core.statement.clone(),
                &transfer_ciphertext.sender_core_epk,
                transfer_ciphertext.sender_core_c2,
                &upload_bundle.sender_core.shared_point()?,
                &proofs.sender_core,
            ),
            PublicTransferTierDecodeObject::new(
                upload_bundle.sender_ext.statement.clone(),
                &transfer_ciphertext.sender_ext_epk,
                transfer_ciphertext.sender_ext_c2,
                &upload_bundle.sender_ext.shared_point()?,
                &proofs.sender_ext,
            ),
            PublicTransferTierDecodeObject::new(
                upload_bundle.output_core.statement.clone(),
                &transfer_ciphertext.output_core_epk,
                transfer_ciphertext.output_core_c2,
                &upload_bundle.output_core.shared_point()?,
                &proofs.output_core,
            ),
            PublicTransferTierDecodeObject::new(
                upload_bundle.output_ext.statement.clone(),
                &transfer_ciphertext.output_ext_epk,
                transfer_ciphertext.output_ext_c2,
                &upload_bundle.output_ext.shared_point()?,
                &proofs.output_ext,
            ),
        ];

        Self::new_transfer(
            output_ref,
            asset_id,
            is_flagged,
            detection_salt,
            transfer_ciphertext,
            proofs,
            tier_objects,
            Some(upload_bundle),
        )
    }

    pub fn object_hash(&self) -> [u8; 32] {
        Sha256::digest(self.to_bytes()).into()
    }

    pub fn validate_payload_hash(&self) -> Result<()> {
        ensure!(
            self.version == COMPLIANCE_EVIDENCE_VERSION,
            "unsupported evidence version {}",
            self.version
        );
        ensure!(
            self.payload_hash == self.compute_payload_hash(),
            "evidence payload_hash mismatch"
        );
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.payload_bytes_without_hash();
        out.extend_from_slice(&self.payload_hash);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = EvidenceReader::new(bytes);
        let version = reader.read_u32()?;
        let object_type = EvidenceObjectType::from_byte(reader.read_u8()?)?;
        let height = reader.read_u64()?;
        let block_hash = reader.read_array::<32>()?;
        let parent_hash = reader.read_array::<32>()?;
        let block_time_unix = if reader.read_u8()? == 1 {
            Some(reader.read_i64()?)
        } else {
            None
        };
        let tx_index = reader.read_u32()?;
        let tx_hash = penumbra_sdk_txhash::TransactionId(reader.read_array::<32>()?);
        let action_index = reader.read_u32()?;
        let output_index = reader.read_u32()?;
        let asset_id = asset::Id(
            Fq::from_bytes_checked(&reader.read_array::<32>()?)
                .map_err(|_| anyhow!("invalid evidence asset_id"))?,
        );
        let is_flagged = reader.read_u8()? != 0;
        let detection_salt = Fq::from_bytes_checked(&reader.read_array::<32>()?)
            .map_err(|_| anyhow!("invalid evidence detection_salt"))?;
        let transfer_ciphertext =
            TransferComplianceCiphertext::from_bytes(reader.read_slice(TRANSFER_WIRE_BYTES)?)?;
        let transfer_dleq_bundle =
            TransferComplianceDleqProofs::from_bytes(reader.read_slice(TRANSFER_DLEQ_BYTES)?)?;
        let tier_count = reader.read_u32()? as usize;
        let mut tier_objects = Vec::with_capacity(tier_count);
        for _ in 0..tier_count {
            tier_objects.push(read_tier_object(&mut reader)?);
        }
        let orbis_upload_bundle_hash = if reader.read_u8()? == 1 {
            Some(reader.read_array::<32>()?)
        } else {
            None
        };
        let payload_hash = reader.read_array::<32>()?;
        reader.finish()?;

        Ok(Self {
            version,
            object_type,
            output_ref: OutputRef {
                action: ActionRef {
                    tx: TxRef {
                        block: BlockRef {
                            height,
                            block_hash,
                            parent_hash,
                            block_time_unix,
                        },
                        tx_index,
                        tx_hash,
                    },
                    action_index,
                },
                output_index,
            },
            asset_id,
            is_flagged,
            detection_salt,
            transfer_ciphertext,
            transfer_dleq_bundle,
            tier_objects,
            orbis_upload_bundle_hash,
            payload_hash,
        })
    }

    fn compute_payload_hash(&self) -> [u8; 32] {
        Sha256::digest(self.payload_bytes_without_hash()).into()
    }

    fn payload_bytes_without_hash(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.version.to_le_bytes());
        out.push(self.object_type as u8);
        let tx_ref = &self.output_ref.action.tx;
        out.extend_from_slice(&tx_ref.block.height.to_le_bytes());
        out.extend_from_slice(&tx_ref.block.block_hash);
        out.extend_from_slice(&tx_ref.block.parent_hash);
        match tx_ref.block.block_time_unix {
            Some(block_time) => {
                out.push(1);
                out.extend_from_slice(&block_time.to_le_bytes());
            }
            None => out.push(0),
        }
        out.extend_from_slice(&tx_ref.tx_index.to_le_bytes());
        out.extend_from_slice(tx_ref.tx_hash.as_ref());
        out.extend_from_slice(&self.output_ref.action.action_index.to_le_bytes());
        out.extend_from_slice(&self.output_ref.output_index.to_le_bytes());
        out.extend_from_slice(&self.asset_id.0.to_bytes());
        out.push(u8::from(self.is_flagged));
        out.extend_from_slice(&self.detection_salt.to_bytes());
        out.extend_from_slice(&self.transfer_ciphertext.to_bytes());
        out.extend_from_slice(&self.transfer_dleq_bundle.to_bytes());
        out.extend_from_slice(&(self.tier_objects.len() as u32).to_le_bytes());
        for tier in &self.tier_objects {
            write_tier_object(&mut out, tier);
        }
        match self.orbis_upload_bundle_hash {
            Some(hash) => {
                out.push(1);
                out.extend_from_slice(&hash);
            }
            None => out.push(0),
        }
        out
    }
}

fn dleq_from_upload_package(package: &crate::OrbisEncryptedSeedUploadPackage) -> Result<DleqProof> {
    Ok(DleqProof {
        c: package.challenge_scalar(),
        s: package.response_scalar()?,
    })
}

fn write_tier_object(out: &mut Vec<u8>, tier: &PublicTransferTierDecodeObject) {
    let statement = &tier.statement;
    out.extend_from_slice(&statement.subject_b_d_bytes);
    out.extend_from_slice(&statement.ring_id_hash_bytes);
    out.extend_from_slice(&statement.policy_id_hash_bytes);
    out.extend_from_slice(&statement.resource_hash_bytes);
    out.extend_from_slice(&statement.permission_hash_bytes);
    out.push(statement.tier as u8);
    out.extend_from_slice(&statement.target_timestamp.to_le_bytes());
    out.extend_from_slice(&statement.salt_bytes);
    out.extend_from_slice(&tier.epk_bytes);
    out.extend_from_slice(&tier.c2_bytes);
    out.extend_from_slice(&tier.shared_point_bytes);
    out.extend_from_slice(&tier.dleq_challenge_bytes);
    out.extend_from_slice(&tier.dleq_response_bytes);
}

fn read_tier_object(reader: &mut EvidenceReader<'_>) -> Result<PublicTransferTierDecodeObject> {
    let statement = TransferTierMetadataStatement {
        subject_b_d_bytes: reader.read_array::<32>()?,
        ring_id_hash_bytes: reader.read_array::<32>()?,
        policy_id_hash_bytes: reader.read_array::<32>()?,
        resource_hash_bytes: reader.read_array::<32>()?,
        permission_hash_bytes: reader.read_array::<32>()?,
        tier: tier_kind_from_byte(reader.read_u8()?)?,
        target_timestamp: reader.read_u64()?,
        salt_bytes: reader.read_array::<32>()?,
    };
    Ok(PublicTransferTierDecodeObject {
        statement,
        epk_bytes: reader.read_array::<32>()?,
        c2_bytes: reader.read_array::<32>()?,
        shared_point_bytes: reader.read_array::<32>()?,
        dleq_challenge_bytes: reader.read_array::<32>()?,
        dleq_response_bytes: reader.read_array::<32>()?,
    })
}

fn tier_kind_from_byte(byte: u8) -> Result<TransferTierKind> {
    match byte {
        1 => Ok(TransferTierKind::SenderCore),
        2 => Ok(TransferTierKind::SenderExt),
        3 => Ok(TransferTierKind::OutputCore),
        4 => Ok(TransferTierKind::OutputExt),
        other => bail!("invalid transfer tier kind {other}"),
    }
}

struct EvidenceReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> EvidenceReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_slice(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| anyhow!("evidence object offset overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| anyhow!("truncated evidence object"))?;
        self.offset = end;
        Ok(slice)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.read_slice(N)?
            .try_into()
            .context("read fixed evidence object field")
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_slice(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_array::<4>()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_array::<8>()?))
    }

    fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.read_array::<8>()?))
    }

    fn finish(&self) -> Result<()> {
        ensure!(
            self.offset == self.bytes.len(),
            "evidence object has trailing bytes"
        );
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use decaf377::{Element, Fr};
    use penumbra_sdk_asset::Value;
    use penumbra_sdk_num::Amount;
    use rand_core::OsRng;

    use crate::{
        crypto::derive_compliance_scalar,
        decode_object::TransferTierMetadataStatement,
        indexed_tree::string_to_fq,
        test_helpers::make_address,
        transfer::{compute_transfer_dleqs, encrypt_transfer},
        upload_package::build_orbis_encrypted_seed_upload_package_with_randomness,
    };

    fn derive_ack(ring_pk: &Element, address: &penumbra_sdk_keys::Address) -> Element {
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    pub(crate) fn valid_evidence_fixture(
    ) -> (ComplianceEvidenceObject, TransferOrbisUploadBundle, Element) {
        let dk = crate::DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_sk = Fr::rand(&mut OsRng);
        let ring_pk = Element::GENERATOR * ring_sk;
        let sender = make_address(9);
        let receiver = make_address(10);
        let ack_sender = derive_ack(&ring_pk, &sender);
        let ack_receiver = derive_ack(&ring_pk, &receiver);
        let asset_id = asset::Id(Fq::from(444u64));
        let salt = Fq::from(77u64);
        let encrypted = encrypt_transfer(
            &mut OsRng,
            &ack_sender,
            &ack_receiver,
            &dk_pub,
            &receiver,
            &sender,
            Value {
                amount: Amount::from(1234u128),
                asset_id,
            },
            false,
            salt,
        )
        .unwrap();

        let target_timestamp = 1_700_000_000;
        let ring_id = "ring-id";
        let policy_id = "policy-id";
        let resource = "document";
        let permission = "read";
        let tier_salts = [
            Fq::from(11u64),
            Fq::from(12u64),
            Fq::from(13u64),
            Fq::from(14u64),
        ];
        let proofs = compute_transfer_dleqs(
            &encrypted.sender,
            &encrypted.output,
            Fr::rand(&mut OsRng),
            Fr::rand(&mut OsRng),
            Fr::rand(&mut OsRng),
            Fr::rand(&mut OsRng),
            &ack_sender,
            &ack_receiver,
            string_to_fq(policy_id),
            string_to_fq(resource),
            string_to_fq(permission),
            tier_salts[0],
            tier_salts[1],
            tier_salts[2],
            tier_salts[3],
            target_timestamp,
        );

        let sender_bd = sender.diversified_generator().vartime_compress_to_field();
        let receiver_bd = receiver.diversified_generator().vartime_compress_to_field();
        let statements = [
            TransferTierMetadataStatement::from_identifiers(
                sender_bd,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderCore,
                target_timestamp,
                tier_salts[0],
            ),
            TransferTierMetadataStatement::from_identifiers(
                sender_bd,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderExt,
                target_timestamp,
                tier_salts[1],
            ),
            TransferTierMetadataStatement::from_identifiers(
                receiver_bd,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputCore,
                target_timestamp,
                tier_salts[2],
            ),
            TransferTierMetadataStatement::from_identifiers(
                receiver_bd,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputExt,
                target_timestamp,
                tier_salts[3],
            ),
        ];

        let ciphertext = &encrypted.ciphertext;
        let tier_objects = vec![
            PublicTransferTierDecodeObject::new(
                statements[0].clone(),
                &ciphertext.sender_core_epk,
                ciphertext.sender_core_c2,
                &(ack_sender * encrypted.sender.core.r),
                &proofs.sender_core,
            ),
            PublicTransferTierDecodeObject::new(
                statements[1].clone(),
                &ciphertext.sender_ext_epk,
                ciphertext.sender_ext_c2,
                &(ack_sender * encrypted.sender.ext.r),
                &proofs.sender_ext,
            ),
            PublicTransferTierDecodeObject::new(
                statements[2].clone(),
                &ciphertext.output_core_epk,
                ciphertext.output_core_c2,
                &(ack_receiver * encrypted.output.core.r),
                &proofs.output_core,
            ),
            PublicTransferTierDecodeObject::new(
                statements[3].clone(),
                &ciphertext.output_ext_epk,
                ciphertext.output_ext_c2,
                &(ack_receiver * encrypted.output.ext.r),
                &proofs.output_ext,
            ),
        ];

        let upload_bundle = TransferOrbisUploadBundle {
            sender_core: build_orbis_encrypted_seed_upload_package_with_randomness(
                &mut OsRng,
                &ring_pk,
                encrypted.sender.core.seed,
                encrypted.sender.core.r,
                statements[0].clone(),
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderCore,
                target_timestamp,
                tier_salts[0],
            )
            .unwrap(),
            sender_ext: build_orbis_encrypted_seed_upload_package_with_randomness(
                &mut OsRng,
                &ring_pk,
                encrypted.sender.ext.seed,
                encrypted.sender.ext.r,
                statements[1].clone(),
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderExt,
                target_timestamp,
                tier_salts[1],
            )
            .unwrap(),
            output_core: build_orbis_encrypted_seed_upload_package_with_randomness(
                &mut OsRng,
                &ring_pk,
                encrypted.output.core.seed,
                encrypted.output.core.r,
                statements[2].clone(),
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputCore,
                target_timestamp,
                tier_salts[2],
            )
            .unwrap(),
            output_ext: build_orbis_encrypted_seed_upload_package_with_randomness(
                &mut OsRng,
                &ring_pk,
                encrypted.output.ext.seed,
                encrypted.output.ext.r,
                statements[3].clone(),
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputExt,
                target_timestamp,
                tier_salts[3],
            )
            .unwrap(),
        };

        let output_ref = OutputRef {
            action: ActionRef {
                tx: TxRef {
                    block: BlockRef {
                        height: 7,
                        block_hash: [7u8; 32],
                        parent_hash: [6u8; 32],
                        block_time_unix: Some(12345),
                    },
                    tx_index: 1,
                    tx_hash: penumbra_sdk_txhash::TransactionId([8u8; 32]),
                },
                action_index: 2,
            },
            output_index: 0,
        };
        let evidence = ComplianceEvidenceObject::new_transfer(
            output_ref,
            asset_id,
            false,
            salt,
            encrypted.ciphertext,
            proofs,
            tier_objects,
            Some(&upload_bundle),
        )
        .unwrap();

        (evidence, upload_bundle, ring_pk)
    }

    #[test]
    fn evidence_round_trip_preserves_payload_hash() {
        let (evidence, _bundle, _ring_pk) = valid_evidence_fixture();
        let encoded = evidence.to_bytes();
        let decoded = ComplianceEvidenceObject::from_bytes(&encoded).unwrap();
        decoded.validate_payload_hash().unwrap();
        assert_eq!(decoded.object_hash(), evidence.object_hash());
        assert_eq!(decoded.output_ref, evidence.output_ref);
        assert_eq!(decoded.tier_objects.len(), 4);
    }

    #[test]
    fn evidence_builds_from_real_upload_bundle() {
        let (evidence, bundle, ring_pk) = valid_evidence_fixture();
        let rebuilt = ComplianceEvidenceObject::from_upload_bundle(
            evidence.output_ref.clone(),
            evidence.asset_id,
            evidence.is_flagged,
            evidence.detection_salt,
            evidence.transfer_ciphertext.clone(),
            &bundle,
        )
        .unwrap();

        assert_eq!(rebuilt.output_ref, evidence.output_ref);
        assert_eq!(rebuilt.asset_id, evidence.asset_id);
        assert_eq!(rebuilt.detection_salt, evidence.detection_salt);
        assert_eq!(rebuilt.tier_objects.len(), 4);
        assert_eq!(
            crate::validate_audit_evidence(crate::AuditValidationInput {
                evidence: rebuilt,
                upload_bundle: Some(bundle),
                ring_pk,
            }),
            crate::AuditValidationStatus::Valid
        );
    }

    #[test]
    fn evidence_rejects_trailing_bytes() {
        let (evidence, _bundle, _ring_pk) = valid_evidence_fixture();
        let mut encoded = evidence.to_bytes();
        encoded.push(0);
        assert!(ComplianceEvidenceObject::from_bytes(&encoded).is_err());
    }
}
