//! Consensus-authenticated log of accepted compliance-relevant effects.
//!
//! The verifiable state stores a Poseidon head and length. Canonical records
//! live in nonverifiable storage and are always checked back to that head.

use anyhow::{ensure, Context as _, Result};
use decaf377::Fq;
use once_cell::sync::Lazy;
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::Address;
use shieldd_sdk_tct::StateCommitment;

#[cfg(feature = "component")]
use async_trait::async_trait;
#[cfg(feature = "component")]
use cnidarium::{StateRead, StateWrite};

#[cfg(feature = "component")]
use crate::state_key;
use crate::{
    TransferComplianceCiphertext, TransferComplianceMetadata, UserAssetStatus,
    WithdrawalComplianceCiphertext,
};

pub const AUDIT_LOG_VERSION: u32 = 1;
pub const MAX_AUDIT_CHAIN_ID_BYTES: usize = 128;
pub const MAX_AUDIT_RECORD_BYTES: usize = 4096;

static AUDIT_BYTES_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.bytes.v1").as_bytes())
});
static AUDIT_SOURCE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.source.v1").as_bytes())
});
static AUDIT_EFFECT_HEAD_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.effect.head.v1").as_bytes())
});
static AUDIT_EFFECT_TAIL_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.effect.tail.v1").as_bytes())
});
static AUDIT_RECORD_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.record.v1").as_bytes())
});
static AUDIT_LOG_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.log.v1").as_bytes())
});
static AUDIT_TRANSFER_CANDIDATE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.audit.candidate.transfer.v1").as_bytes(),
    )
});
static AUDIT_WITHDRAWAL_CANDIDATE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.audit.candidate.withdrawal.v1").as_bytes(),
    )
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditSource {
    ShielddTransaction {
        height: u64,
        transaction_id: [u8; 32],
        action_index: u32,
        effect_index: u32,
    },
    Host {
        chain_id: String,
        height: u64,
        tx_hash: [u8; 32],
        tx_index: u32,
        message_index: u32,
        effect_index: u32,
    },
    Ibc {
        height: u64,
        channel_id: String,
        packet_sequence: u64,
        operation: IbcOperation,
        effect_index: u32,
    },
}

impl AuditSource {
    pub fn height(&self) -> u64 {
        match self {
            Self::ShielddTransaction { height, .. }
            | Self::Host { height, .. }
            | Self::Ibc { height, .. } => *height,
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.height() > 0, "audit source height must be nonzero");
        if let Self::Host { chain_id, .. } = self {
            ensure!(
                !chain_id.is_empty(),
                "audit host chain ID must not be empty"
            );
            ensure!(
                chain_id.len() <= MAX_AUDIT_CHAIN_ID_BYTES,
                "audit host chain ID exceeds size limit"
            );
        }
        if let Self::Ibc { channel_id, .. } = self {
            ensure!(
                !channel_id.is_empty(),
                "audit IBC channel ID must not be empty"
            );
            ensure!(
                channel_id.len() <= MAX_AUDIT_CHAIN_ID_BYTES,
                "audit IBC channel ID exceeds size limit"
            );
        }
        Ok(())
    }

    pub fn commitment(&self) -> Result<Fq> {
        self.validate()?;
        let (source_head, context) = match self {
            Self::ShielddTransaction {
                height,
                transaction_id,
                action_index,
                effect_index,
            } => {
                let (tx_low, tx_high) = split_256(transaction_id);
                (
                    poseidon377::hash_7(
                        &AUDIT_SOURCE_DOMAIN,
                        (
                            Fq::from(1u64),
                            Fq::from(*height),
                            Fq::from(tx_low),
                            Fq::from(tx_high),
                            Fq::from(*action_index),
                            Fq::from(*effect_index),
                            Fq::from(0u64),
                        ),
                    ),
                    Fq::from(0u64),
                )
            }
            Self::Host {
                chain_id,
                height,
                tx_hash,
                tx_index,
                message_index,
                effect_index,
            } => {
                let (tx_low, tx_high) = split_256(tx_hash);
                (
                    poseidon377::hash_7(
                        &AUDIT_SOURCE_DOMAIN,
                        (
                            Fq::from(2u64),
                            Fq::from(*height),
                            Fq::from(tx_low),
                            Fq::from(tx_high),
                            Fq::from(*tx_index),
                            Fq::from(*message_index),
                            Fq::from(*effect_index),
                        ),
                    ),
                    audit_bytes_commitment(chain_id.as_bytes()),
                )
            }
            Self::Ibc {
                height,
                channel_id,
                packet_sequence,
                operation,
                effect_index,
            } => (
                poseidon377::hash_7(
                    &AUDIT_SOURCE_DOMAIN,
                    (
                        Fq::from(3u64),
                        Fq::from(*height),
                        Fq::from(0u64),
                        Fq::from(0u64),
                        Fq::from(*packet_sequence),
                        Fq::from(*operation as u64),
                        Fq::from(*effect_index),
                    ),
                ),
                audit_bytes_commitment(channel_id.as_bytes()),
            ),
        };
        Ok(poseidon377::hash_2(
            &AUDIT_SOURCE_DOMAIN,
            (source_head, context),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithdrawalKind {
    Host,
    Ics20,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IbcOperation {
    Receive = 1,
    Refund = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditEffect {
    TransferOutput {
        asset_anchor: StateCommitment,
        compliance_ciphertext: Vec<u8>,
        compliance_metadata: Vec<u8>,
    },
    Withdrawal {
        kind: WithdrawalKind,
        asset_id: asset::Id,
        amount: u128,
        asset_anchor: StateCommitment,
        compliance_ciphertext: Vec<u8>,
    },
    NoteReshape {
        action_effect_hash: [u8; 64],
    },
    PublicDeposit {
        asset_id: asset::Id,
        amount: u128,
        recipient: Address,
    },
    UserRegistered {
        asset_id: asset::Id,
        address: Address,
    },
    UserStatusChanged {
        asset_id: asset::Id,
        address: Address,
        status: UserAssetStatus,
        freeze_generation: u64,
        frozen_since_height: u64,
    },
    AssetRegistered {
        asset_id: asset::Id,
        is_regulated: bool,
    },
    IbcRelay {
        action_effect_hash: [u8; 64],
    },
    NoteSeized {
        asset_id: asset::Id,
        address: Address,
        nullifier: [u8; 32],
        amount: u128,
        freeze_generation: u64,
        authorization_commitment: Fq,
    },
}

impl AuditEffect {
    /// Target-independent projections committed by the audit log.
    ///
    /// A transfer has two projections in canonical credit-then-debit order.
    /// Together they bind every tier used to decide whether the target was the
    /// receiver or sender. A withdrawal has one debit projection.
    fn candidate_commitments(&self) -> Result<Vec<Fq>> {
        match self {
            Self::TransferOutput {
                asset_anchor,
                compliance_ciphertext,
                compliance_metadata,
            } => {
                let ciphertext = TransferComplianceCiphertext::from_bytes(compliance_ciphertext)?;
                let metadata = TransferComplianceMetadata::from_bytes(compliance_metadata)?;
                let public = ciphertext.to_transfer_circuit_public_inputs();

                let credit = transfer_candidate_commitment(
                    1,
                    asset_anchor.0,
                    metadata.ring_id_hash()?,
                    metadata.policy_id_hash()?,
                    metadata.permission_hash()?,
                    metadata.resource_hash()?,
                    public.sender_core_epk.vartime_compress_to_field(),
                    public.detection_ciphertext,
                    public.output_core_epk.vartime_compress_to_field(),
                    public.output_core_c2,
                    public.output_core_key_confirmation,
                    public.output_core_ciphertext[0],
                    metadata.output_core_salt()?,
                    public.sender_ext_epk.vartime_compress_to_field(),
                    public.sender_ext_c2,
                    public.sender_ext_ciphertext,
                );
                let debit = transfer_candidate_commitment(
                    2,
                    asset_anchor.0,
                    metadata.ring_id_hash()?,
                    metadata.policy_id_hash()?,
                    metadata.permission_hash()?,
                    metadata.resource_hash()?,
                    public.sender_core_epk.vartime_compress_to_field(),
                    public.detection_ciphertext,
                    public.sender_core_epk.vartime_compress_to_field(),
                    public.sender_core_c2,
                    public.sender_core_key_confirmation,
                    public.sender_core_ciphertext[0],
                    metadata.sender_core_salt()?,
                    public.output_ext_epk.vartime_compress_to_field(),
                    public.output_ext_c2,
                    public.output_ext_ciphertext,
                );
                Ok(vec![credit, debit])
            }
            Self::Withdrawal {
                kind,
                asset_id,
                amount,
                asset_anchor,
                compliance_ciphertext,
            } => {
                let ciphertext = WithdrawalComplianceCiphertext::from_bytes(compliance_ciphertext)?;
                let encrypted_sender = decode_fq_words::<3>(&ciphertext.encrypted_sender_address)?;
                Ok(vec![withdrawal_candidate_commitment(
                    *kind,
                    *asset_id,
                    *amount,
                    asset_anchor.0,
                    ciphertext.epk.vartime_compress_to_field(),
                    ciphertext.c2,
                    ciphertext.key_confirmation,
                    encrypted_sender,
                )])
            }
            _ => Ok(Vec::new()),
        }
    }

    fn commitment_fields(&self) -> Result<(u64, [Fq; 12])> {
        let mut fields = [Fq::from(0u64); 12];
        let kind = match self {
            Self::TransferOutput {
                asset_anchor,
                compliance_ciphertext,
                compliance_metadata,
            } => {
                fields[0] = audit_bytes_commitment(compliance_ciphertext);
                fields[1] = audit_bytes_commitment(compliance_metadata);
                let candidates = self.candidate_commitments()?;
                fields[2] = candidates[0];
                fields[3] = candidates[1];
                fields[4] = asset_anchor.0;
                1
            }
            Self::Withdrawal {
                kind,
                asset_id,
                amount,
                asset_anchor,
                compliance_ciphertext,
            } => {
                fields[0] = Fq::from(match kind {
                    WithdrawalKind::Host => 1u64,
                    WithdrawalKind::Ics20 => 2u64,
                });
                fields[1] = asset_id.0;
                fields[2] = Fq::from(*amount);
                fields[3] = audit_bytes_commitment(compliance_ciphertext);
                fields[4] = self.candidate_commitments()?[0];
                fields[5] = asset_anchor.0;
                2
            }
            Self::NoteReshape { action_effect_hash } => {
                put_hash_limbs(&mut fields, action_effect_hash);
                3
            }
            Self::PublicDeposit {
                asset_id,
                amount,
                recipient,
            } => {
                fields[0] = asset_id.0;
                fields[1] = Fq::from(*amount);
                fields[2] = audit_bytes_commitment(&recipient.to_vec());
                4
            }
            Self::UserRegistered { asset_id, address } => {
                fields[0] = asset_id.0;
                fields[1] = audit_bytes_commitment(&address.to_vec());
                5
            }
            Self::UserStatusChanged {
                asset_id,
                address,
                status,
                freeze_generation,
                frozen_since_height,
            } => {
                fields[0] = asset_id.0;
                fields[1] = audit_bytes_commitment(&address.to_vec());
                fields[2] = status.as_field();
                fields[3] = Fq::from(*freeze_generation);
                fields[4] = Fq::from(*frozen_since_height);
                6
            }
            Self::AssetRegistered {
                asset_id,
                is_regulated,
            } => {
                fields[0] = asset_id.0;
                fields[1] = Fq::from(u64::from(*is_regulated));
                7
            }
            Self::IbcRelay { action_effect_hash } => {
                put_hash_limbs(&mut fields, action_effect_hash);
                8
            }
            Self::NoteSeized {
                asset_id,
                address,
                nullifier,
                amount,
                freeze_generation,
                authorization_commitment,
            } => {
                fields[0] = asset_id.0;
                fields[1] = audit_bytes_commitment(&address.to_vec());
                fields[2] = Fq::from_le_bytes_mod_order(nullifier);
                fields[3] = Fq::from(*amount);
                fields[4] = Fq::from(*freeze_generation);
                fields[5] = *authorization_commitment;
                9
            }
        };
        Ok((kind, fields))
    }

    pub fn commitment(&self) -> Result<Fq> {
        let (kind, fields) = self.commitment_fields()?;
        let head = poseidon377::hash_7(
            &AUDIT_EFFECT_HEAD_DOMAIN,
            (
                Fq::from(kind),
                fields[0],
                fields[1],
                fields[2],
                fields[3],
                fields[4],
                fields[5],
            ),
        );
        Ok(poseidon377::hash_7(
            &AUDIT_EFFECT_TAIL_DOMAIN,
            (
                head, fields[6], fields[7], fields[8], fields[9], fields[10], fields[11],
            ),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEffectRecord {
    pub source: AuditSource,
    pub effect: AuditEffect,
}

impl AuditEffectRecord {
    pub fn validate(&self) -> Result<()> {
        self.source.validate()?;
        match &self.effect {
            AuditEffect::Withdrawal {
                compliance_ciphertext,
                ..
            } => ensure!(
                !compliance_ciphertext.is_empty(),
                "audit compliance ciphertext must not be empty"
            ),
            AuditEffect::TransferOutput {
                compliance_ciphertext,
                ..
            } => ensure!(
                !compliance_ciphertext.is_empty(),
                "audit compliance ciphertext must not be empty"
            ),
            AuditEffect::UserStatusChanged {
                status,
                freeze_generation,
                frozen_since_height,
                ..
            } => match status {
                UserAssetStatus::Active => ensure!(
                    *frozen_since_height == 0,
                    "active audit status cannot have a frozen-since height"
                ),
                UserAssetStatus::Frozen | UserAssetStatus::Seized => {
                    ensure!(
                        *freeze_generation > 0 && *frozen_since_height > 0,
                        "frozen audit status requires generation and height"
                    )
                }
            },
            AuditEffect::NoteSeized {
                amount,
                freeze_generation,
                authorization_commitment,
                ..
            } => {
                ensure!(*amount > 0, "seized audit amount must be nonzero");
                ensure!(
                    *freeze_generation > 0,
                    "seized audit freeze generation must be nonzero"
                );
                ensure!(
                    *authorization_commitment != Fq::from(0u64),
                    "seized audit authorization commitment must be nonzero"
                );
            }
            _ => {}
        }
        self.effect.candidate_commitments()?;
        let encoded = self.encode_unchecked()?;
        ensure!(
            encoded.len() <= MAX_AUDIT_RECORD_BYTES,
            "audit effect record exceeds size limit"
        );
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        self.encode_unchecked()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_AUDIT_RECORD_BYTES,
            "audit effect record exceeds size limit"
        );
        let mut reader = AuditReader::new(bytes);
        ensure!(
            reader.read_u32()? == AUDIT_LOG_VERSION,
            "unsupported audit effect version"
        );
        let source = match reader.read_u8()? {
            1 => AuditSource::ShielddTransaction {
                height: reader.read_u64()?,
                transaction_id: reader.read_fixed()?,
                action_index: reader.read_u32()?,
                effect_index: reader.read_u32()?,
            },
            2 => AuditSource::Host {
                chain_id: String::from_utf8(reader.read_bytes(MAX_AUDIT_CHAIN_ID_BYTES)?)
                    .context("audit host chain ID is not UTF-8")?,
                height: reader.read_u64()?,
                tx_hash: reader.read_fixed()?,
                tx_index: reader.read_u32()?,
                message_index: reader.read_u32()?,
                effect_index: reader.read_u32()?,
            },
            3 => AuditSource::Ibc {
                height: reader.read_u64()?,
                channel_id: String::from_utf8(reader.read_bytes(MAX_AUDIT_CHAIN_ID_BYTES)?)
                    .context("audit IBC channel ID is not UTF-8")?,
                packet_sequence: reader.read_u64()?,
                operation: match reader.read_u8()? {
                    1 => IbcOperation::Receive,
                    2 => IbcOperation::Refund,
                    operation => anyhow::bail!("unknown audit IBC operation {operation}"),
                },
                effect_index: reader.read_u32()?,
            },
            tag => anyhow::bail!("unknown audit source tag {tag}"),
        };
        let effect = match reader.read_u8()? {
            1 => AuditEffect::TransferOutput {
                asset_anchor: StateCommitment(reader.read_fq()?),
                compliance_ciphertext: reader.read_bytes(MAX_AUDIT_RECORD_BYTES)?,
                compliance_metadata: reader.read_bytes(MAX_AUDIT_RECORD_BYTES)?,
            },
            2 => AuditEffect::Withdrawal {
                kind: reader.read_withdrawal_kind()?,
                asset_id: reader.read_asset_id()?,
                amount: u128::from_le_bytes(reader.read_fixed()?),
                asset_anchor: StateCommitment(reader.read_fq()?),
                compliance_ciphertext: reader.read_bytes(MAX_AUDIT_RECORD_BYTES)?,
            },
            3 => AuditEffect::NoteReshape {
                action_effect_hash: reader.read_fixed()?,
            },
            4 => AuditEffect::PublicDeposit {
                asset_id: reader.read_asset_id()?,
                amount: u128::from_le_bytes(reader.read_fixed()?),
                recipient: reader.read_address()?,
            },
            5 => AuditEffect::UserRegistered {
                asset_id: reader.read_asset_id()?,
                address: reader.read_address()?,
            },
            6 => AuditEffect::UserStatusChanged {
                asset_id: reader.read_asset_id()?,
                address: reader.read_address()?,
                status: match reader.read_u8()? {
                    1 => UserAssetStatus::Active,
                    2 => UserAssetStatus::Frozen,
                    3 => UserAssetStatus::Seized,
                    status => anyhow::bail!("unknown audit user status {status}"),
                },
                freeze_generation: reader.read_u64()?,
                frozen_since_height: reader.read_u64()?,
            },
            7 => AuditEffect::AssetRegistered {
                asset_id: reader.read_asset_id()?,
                is_regulated: match reader.read_u8()? {
                    0 => false,
                    1 => true,
                    value => anyhow::bail!("invalid audit regulation flag {value}"),
                },
            },
            8 => AuditEffect::IbcRelay {
                action_effect_hash: reader.read_fixed()?,
            },
            9 => AuditEffect::NoteSeized {
                asset_id: reader.read_asset_id()?,
                address: reader.read_address()?,
                nullifier: reader.read_fixed()?,
                amount: u128::from_le_bytes(reader.read_fixed()?),
                freeze_generation: reader.read_u64()?,
                authorization_commitment: reader.read_fq()?,
            },
            tag => anyhow::bail!("unknown audit effect tag {tag}"),
        };
        reader.finish()?;
        let record = Self { source, effect };
        record.validate()?;
        ensure!(
            record.encode_unchecked()? == bytes,
            "non-canonical audit record"
        );
        Ok(record)
    }

    fn encode_unchecked(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        put_u32(&mut out, AUDIT_LOG_VERSION);
        match &self.source {
            AuditSource::ShielddTransaction {
                height,
                transaction_id,
                action_index,
                effect_index,
            } => {
                out.push(1);
                put_u64(&mut out, *height);
                out.extend_from_slice(transaction_id);
                put_u32(&mut out, *action_index);
                put_u32(&mut out, *effect_index);
            }
            AuditSource::Host {
                chain_id,
                height,
                tx_hash,
                tx_index,
                message_index,
                effect_index,
            } => {
                out.push(2);
                put_bytes(&mut out, chain_id.as_bytes())?;
                put_u64(&mut out, *height);
                out.extend_from_slice(tx_hash);
                put_u32(&mut out, *tx_index);
                put_u32(&mut out, *message_index);
                put_u32(&mut out, *effect_index);
            }
            AuditSource::Ibc {
                height,
                channel_id,
                packet_sequence,
                operation,
                effect_index,
            } => {
                out.push(3);
                put_u64(&mut out, *height);
                put_bytes(&mut out, channel_id.as_bytes())?;
                put_u64(&mut out, *packet_sequence);
                out.push(*operation as u8);
                put_u32(&mut out, *effect_index);
            }
        }
        match &self.effect {
            AuditEffect::TransferOutput {
                asset_anchor,
                compliance_ciphertext,
                compliance_metadata,
            } => {
                out.push(1);
                out.extend_from_slice(&asset_anchor.0.to_bytes());
                put_bytes(&mut out, compliance_ciphertext)?;
                put_bytes(&mut out, compliance_metadata)?;
            }
            AuditEffect::Withdrawal {
                kind,
                asset_id,
                amount,
                asset_anchor,
                compliance_ciphertext,
            } => {
                out.push(2);
                out.push(match kind {
                    WithdrawalKind::Host => 1,
                    WithdrawalKind::Ics20 => 2,
                });
                out.extend_from_slice(&asset_id.0.to_bytes());
                out.extend_from_slice(&amount.to_le_bytes());
                out.extend_from_slice(&asset_anchor.0.to_bytes());
                put_bytes(&mut out, compliance_ciphertext)?;
            }
            AuditEffect::NoteReshape { action_effect_hash } => {
                out.push(3);
                out.extend_from_slice(action_effect_hash);
            }
            AuditEffect::PublicDeposit {
                asset_id,
                amount,
                recipient,
            } => {
                out.push(4);
                out.extend_from_slice(&asset_id.0.to_bytes());
                out.extend_from_slice(&amount.to_le_bytes());
                put_bytes(&mut out, &recipient.to_vec())?;
            }
            AuditEffect::UserRegistered { asset_id, address } => {
                out.push(5);
                out.extend_from_slice(&asset_id.0.to_bytes());
                put_bytes(&mut out, &address.to_vec())?;
            }
            AuditEffect::UserStatusChanged {
                asset_id,
                address,
                status,
                freeze_generation,
                frozen_since_height,
            } => {
                out.push(6);
                out.extend_from_slice(&asset_id.0.to_bytes());
                put_bytes(&mut out, &address.to_vec())?;
                out.push(match status {
                    UserAssetStatus::Active => 1,
                    UserAssetStatus::Frozen => 2,
                    UserAssetStatus::Seized => 3,
                });
                put_u64(&mut out, *freeze_generation);
                put_u64(&mut out, *frozen_since_height);
            }
            AuditEffect::AssetRegistered {
                asset_id,
                is_regulated,
            } => {
                out.push(7);
                out.extend_from_slice(&asset_id.0.to_bytes());
                out.push(u8::from(*is_regulated));
            }
            AuditEffect::IbcRelay { action_effect_hash } => {
                out.push(8);
                out.extend_from_slice(action_effect_hash);
            }
            AuditEffect::NoteSeized {
                asset_id,
                address,
                nullifier,
                amount,
                freeze_generation,
                authorization_commitment,
            } => {
                out.push(9);
                out.extend_from_slice(&asset_id.0.to_bytes());
                put_bytes(&mut out, &address.to_vec())?;
                out.extend_from_slice(nullifier);
                out.extend_from_slice(&amount.to_le_bytes());
                put_u64(&mut out, *freeze_generation);
                out.extend_from_slice(&authorization_commitment.to_bytes());
            }
        }
        Ok(out)
    }

    pub fn commitment(&self) -> Result<Fq> {
        self.validate()?;
        Ok(poseidon377::hash_3(
            &AUDIT_RECORD_DOMAIN,
            (
                Fq::from(AUDIT_LOG_VERSION),
                self.source.commitment()?,
                self.effect.commitment()?,
            ),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn transfer_candidate_commitment(
    role: u64,
    asset_anchor: Fq,
    ring_id_hash: Fq,
    policy_id_hash: Fq,
    permission_hash: Fq,
    resource_hash: Fq,
    detection_epk: Fq,
    detection_ciphertext: [Fq; 4],
    core_epk: Fq,
    core_c2: Fq,
    core_key_confirmation: Fq,
    core_ciphertext: Fq,
    core_salt: Fq,
    address_epk: Fq,
    address_c2: Fq,
    address_ciphertext: [Fq; 3],
) -> Fq {
    fold_candidate(
        &AUDIT_TRANSFER_CANDIDATE_DOMAIN,
        [
            Fq::from(role),
            asset_anchor,
            ring_id_hash,
            policy_id_hash,
            permission_hash,
            resource_hash,
            detection_epk,
            detection_ciphertext[0],
            detection_ciphertext[1],
            detection_ciphertext[2],
            detection_ciphertext[3],
            core_epk,
            core_c2,
            core_key_confirmation,
            core_ciphertext,
            core_salt,
            address_epk,
            address_c2,
            address_ciphertext[0],
            address_ciphertext[1],
            address_ciphertext[2],
        ],
    )
}

fn withdrawal_candidate_commitment(
    kind: WithdrawalKind,
    asset_id: asset::Id,
    amount: u128,
    asset_anchor: Fq,
    epk: Fq,
    c2: Fq,
    key_confirmation: Fq,
    address_ciphertext: [Fq; 3],
) -> Fq {
    fold_candidate(
        &AUDIT_WITHDRAWAL_CANDIDATE_DOMAIN,
        [
            Fq::from(2u64),
            Fq::from(match kind {
                WithdrawalKind::Host => 1u64,
                WithdrawalKind::Ics20 => 2u64,
            }),
            asset_id.0,
            Fq::from(amount),
            asset_anchor,
            epk,
            c2,
            key_confirmation,
            address_ciphertext[0],
            address_ciphertext[1],
            address_ciphertext[2],
        ],
    )
}

fn fold_candidate<const N: usize>(domain: &Fq, fields: [Fq; N]) -> Fq {
    fields.into_iter().fold(Fq::from(0u64), |head, field| {
        poseidon377::hash_2(domain, (head, field))
    })
}

fn decode_fq_words<const N: usize>(bytes: &[u8]) -> Result<[Fq; N]> {
    ensure!(
        bytes.len() == N * 32,
        "invalid audit ciphertext field count"
    );
    let fields = (0..N)
        .map(|index| {
            let start = index * 32;
            let encoded: [u8; 32] = bytes[start..start + 32]
                .try_into()
                .expect("bounded field word");
            Fq::from_bytes_checked(&encoded)
                .map_err(|_| anyhow::anyhow!("non-canonical audit ciphertext field {index}"))
        })
        .collect::<Result<Vec<_>>>()?;
    fields
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid audit ciphertext field count"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditLogState {
    pub length: u64,
    pub head: Fq,
    pub last_height: u64,
}

impl Default for AuditLogState {
    fn default() -> Self {
        Self {
            length: 0,
            head: Fq::from(0u64),
            last_height: 0,
        }
    }
}

impl AuditLogState {
    pub fn append(self, record: &AuditEffectRecord) -> Result<Self> {
        let height = record.source.height();
        ensure!(
            height >= self.last_height,
            "audit effect source height moved backwards"
        );
        let head = poseidon377::hash_3(
            &AUDIT_LOG_DOMAIN,
            (self.head, Fq::from(self.length), record.commitment()?),
        );
        Ok(Self {
            length: self
                .length
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("audit log length overflow"))?,
            head,
            last_height: height,
        })
    }

    #[cfg(feature = "component")]
    fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(48);
        bytes.extend_from_slice(&self.length.to_le_bytes());
        bytes.extend_from_slice(&self.head.to_bytes());
        bytes.extend_from_slice(&self.last_height.to_le_bytes());
        bytes
    }

    #[cfg(feature = "component")]
    fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() == 48, "audit log state must be 48 bytes");
        let length = u64::from_le_bytes(bytes[..8].try_into().expect("checked length"));
        let head = Fq::from_bytes_checked(&bytes[8..40].try_into().expect("checked length"))
            .map_err(|_| anyhow::anyhow!("audit log head must be canonical"))?;
        let last_height = u64::from_le_bytes(bytes[40..].try_into().expect("checked length"));
        Ok(Self {
            length,
            head,
            last_height,
        })
    }
}

#[cfg(feature = "component")]
#[async_trait]
pub trait AuditLogRead: StateRead {
    async fn get_audit_log_state(&self) -> Result<AuditLogState> {
        self.get_raw(state_key::audit_log::state())
            .await?
            .map(|bytes| AuditLogState::decode(&bytes).context("decoding audit log state"))
            .transpose()
            .map(|state| state.unwrap_or_default())
    }

    async fn get_audit_record(&self, index: u64) -> Result<Option<AuditEffectRecord>> {
        self.nonverifiable_get_raw(&state_key::audit_log::record(index))
            .await?
            .map(|bytes| AuditEffectRecord::decode(&bytes).context("decoding audit log record"))
            .transpose()
    }

    async fn verify_audit_log(&self) -> Result<AuditLogState> {
        let expected = self.get_audit_log_state().await?;
        let mut replayed = AuditLogState::default();
        for index in 0..expected.length {
            let record = self
                .get_audit_record(index)
                .await?
                .ok_or_else(|| anyhow::anyhow!("missing audit log record {index}"))?;
            replayed = replayed.append(&record)?;
        }
        ensure!(
            replayed == expected,
            "persisted audit records do not match the authenticated audit log state"
        );
        Ok(replayed)
    }
}

#[cfg(feature = "component")]
impl<T: StateRead + ?Sized> AuditLogRead for T {}

#[cfg(feature = "component")]
#[async_trait]
pub trait AuditLogWrite: StateWrite + AuditLogRead {
    async fn append_audit_effect(&mut self, record: AuditEffectRecord) -> Result<u64> {
        let current = self.get_audit_log_state().await?;
        let index = current.length;
        let record_key = state_key::audit_log::record(index);
        ensure!(
            self.nonverifiable_get_raw(&record_key).await?.is_none(),
            "audit log record {index} already exists"
        );
        let encoded = record.encode()?;
        let next = current.append(&record)?;
        self.nonverifiable_put_raw(record_key, encoded);
        self.put_raw(state_key::audit_log::state().to_owned(), next.encode());
        Ok(index)
    }
}

#[cfg(feature = "component")]
impl<T: StateWrite + ?Sized> AuditLogWrite for T {}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = u32::try_from(value.len()).context("audit field exceeds u32 length")?;
    put_u32(out, length);
    out.extend_from_slice(value);
    Ok(())
}

pub fn audit_bytes_commitment(bytes: &[u8]) -> Fq {
    let mut head = poseidon377::hash_2(
        &AUDIT_BYTES_DOMAIN,
        (
            Fq::from(bytes.len() as u64),
            Fq::from(bytes.len().div_ceil(31) as u64),
        ),
    );
    for (index, chunk) in bytes.chunks(31).enumerate() {
        head = poseidon377::hash_3(
            &AUDIT_BYTES_DOMAIN,
            (
                head,
                Fq::from(index as u64),
                Fq::from_le_bytes_mod_order(chunk),
            ),
        );
    }
    head
}

fn split_256(bytes: &[u8; 32]) -> (u128, u128) {
    (
        u128::from_le_bytes(bytes[..16].try_into().expect("fixed low limb")),
        u128::from_le_bytes(bytes[16..].try_into().expect("fixed high limb")),
    )
}

fn put_hash_limbs(fields: &mut [Fq; 12], bytes: &[u8; 64]) {
    for (field, chunk) in fields.iter_mut().zip(bytes.chunks_exact(16)) {
        *field = Fq::from(u128::from_le_bytes(
            chunk.try_into().expect("fixed hash limb"),
        ));
    }
}

struct AuditReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> AuditReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_fixed()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_fixed()?))
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.read_exact(N)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("audit record is truncated"))
    }

    fn read_bytes(&mut self, maximum: usize) -> Result<Vec<u8>> {
        let length = self.read_u32()? as usize;
        ensure!(length <= maximum, "audit byte field exceeds size limit");
        Ok(self.read_exact(length)?.to_vec())
    }

    fn read_asset_id(&mut self) -> Result<asset::Id> {
        Ok(asset::Id(self.read_fq()?))
    }

    fn read_fq(&mut self) -> Result<Fq> {
        let bytes = self.read_fixed()?;
        Fq::from_bytes_checked(&bytes)
            .map_err(|_| anyhow::anyhow!("audit field element must be canonical"))
    }

    fn read_address(&mut self) -> Result<Address> {
        let bytes = self.read_bytes(128)?;
        Address::try_from(bytes.as_slice()).context("invalid audit address")
    }

    fn read_withdrawal_kind(&mut self) -> Result<WithdrawalKind> {
        match self.read_u8()? {
            1 => Ok(WithdrawalKind::Host),
            2 => Ok(WithdrawalKind::Ics20),
            tag => anyhow::bail!("unknown audit withdrawal kind {tag}"),
        }
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| anyhow::anyhow!("audit record length overflow"))?;
        ensure!(end <= self.bytes.len(), "audit record is truncated");
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }

    fn finish(&self) -> Result<()> {
        ensure!(
            self.position == self.bytes.len(),
            "audit record has trailing bytes"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encrypt_transfer, encrypt_withdrawal};
    use rand_core::OsRng;
    use shieldd_sdk_asset::Value;

    fn record(height: u64, effect_index: u32, byte: u8) -> AuditEffectRecord {
        AuditEffectRecord {
            source: AuditSource::ShielddTransaction {
                height,
                transaction_id: [7; 32],
                action_index: 2,
                effect_index,
            },
            effect: AuditEffect::IbcRelay {
                action_effect_hash: [byte; 64],
            },
        }
    }

    #[test]
    fn log_chain_binds_order_and_every_record_byte() {
        let first = record(10, 0, 1);
        let second = record(10, 1, 2);
        let canonical = AuditLogState::default()
            .append(&first)
            .unwrap()
            .append(&second)
            .unwrap();
        let reversed = AuditLogState::default()
            .append(&second)
            .unwrap()
            .append(&first)
            .unwrap();
        let changed = AuditLogState::default()
            .append(&first)
            .unwrap()
            .append(&record(10, 1, 3))
            .unwrap();
        assert_ne!(canonical.head, reversed.head);
        assert_ne!(canonical.head, changed.head);
        assert_eq!(canonical.length, 2);
    }

    #[test]
    fn status_effect_commits_freeze_provenance() {
        let address = Address::dummy(&mut OsRng);
        let base = AuditEffectRecord {
            source: AuditSource::Host {
                chain_id: "bankd-test".to_owned(),
                height: 20,
                tx_hash: [8; 32],
                tx_index: 1,
                message_index: 2,
                effect_index: 0,
            },
            effect: AuditEffect::UserStatusChanged {
                asset_id: asset::Id(Fq::from(9u64)),
                address,
                status: UserAssetStatus::Frozen,
                freeze_generation: 1,
                frozen_since_height: 20,
            },
        };
        let mut refrozen = base.clone();
        if let AuditEffect::UserStatusChanged {
            freeze_generation,
            frozen_since_height,
            ..
        } = &mut refrozen.effect
        {
            *freeze_generation = 2;
            *frozen_since_height = 30;
        }
        assert_ne!(base.commitment().unwrap(), refrozen.commitment().unwrap());
        let encoded = base.encode().unwrap();
        assert_eq!(AuditEffectRecord::decode(&encoded).unwrap(), base);
    }

    #[test]
    fn note_seizure_effect_round_trips_and_binds_the_nullifier() {
        let base = AuditEffectRecord {
            source: AuditSource::Host {
                chain_id: "bankd-test".to_owned(),
                height: 50,
                tx_hash: [8; 32],
                tx_index: 1,
                message_index: 2,
                effect_index: 0,
            },
            effect: AuditEffect::NoteSeized {
                asset_id: asset::Id(Fq::from(9u64)),
                address: Address::dummy(&mut OsRng),
                nullifier: Fq::from(11u64).to_bytes(),
                amount: 7,
                freeze_generation: 2,
                authorization_commitment: Fq::from(13u64),
            },
        };
        let mut other = base.clone();
        if let AuditEffect::NoteSeized { nullifier, .. } = &mut other.effect {
            *nullifier = Fq::from(12u64).to_bytes();
        }
        assert_ne!(base.commitment().unwrap(), other.commitment().unwrap());
        let encoded = base.encode().unwrap();
        assert_eq!(AuditEffectRecord::decode(&encoded).unwrap(), base);
    }

    #[test]
    fn ibc_public_deposit_round_trips_and_binds_packet_provenance() {
        let base = AuditEffectRecord {
            source: AuditSource::Ibc {
                height: 20,
                channel_id: "channel-7".to_owned(),
                packet_sequence: 42,
                operation: IbcOperation::Receive,
                effect_index: 0,
            },
            effect: AuditEffect::PublicDeposit {
                asset_id: asset::Id(Fq::from(9u64)),
                amount: 7,
                recipient: Address::dummy(&mut OsRng),
            },
        };
        let encoded = base.encode().unwrap();
        assert_eq!(AuditEffectRecord::decode(&encoded).unwrap(), base);

        let mut refunded = base.clone();
        if let AuditSource::Ibc { operation, .. } = &mut refunded.source {
            *operation = IbcOperation::Refund;
        }
        assert_ne!(base.commitment().unwrap(), refunded.commitment().unwrap());

        let mut other_packet = base.clone();
        if let AuditSource::Ibc {
            packet_sequence, ..
        } = &mut other_packet.source
        {
            *packet_sequence += 1;
        }
        assert_ne!(
            base.commitment().unwrap(),
            other_packet.commitment().unwrap()
        );
    }

    #[test]
    fn candidate_commitments_bind_canonical_ciphertext_projections() {
        let asset_id = asset::Id(Fq::from(9u64));
        let sender = Address::dummy(&mut OsRng);
        let receiver = Address::dummy(&mut OsRng);
        let encrypted = encrypt_transfer(
            &mut OsRng,
            &decaf377::Element::GENERATOR,
            &decaf377::Element::GENERATOR,
            &decaf377::Element::GENERATOR,
            &receiver,
            &sender,
            Value {
                amount: 7u64.into(),
                asset_id,
            },
            false,
            Fq::from(10u64),
            Fq::from(11u64),
            Fq::from(12u64),
        )
        .unwrap();
        let metadata = TransferComplianceMetadata::new(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(4u64),
            5,
            Fq::from(11u64),
            Fq::from(13u64),
            Fq::from(12u64),
            Fq::from(14u64),
        );
        let effect = AuditEffect::TransferOutput {
            asset_anchor: StateCommitment(Fq::from(21u64)),
            compliance_ciphertext: encrypted.ciphertext.to_bytes(),
            compliance_metadata: metadata.to_bytes().unwrap(),
        };
        let candidates = effect.candidate_commitments().unwrap();
        assert_eq!(candidates.len(), 2);
        assert_ne!(candidates[0], candidates[1]);

        let changed_anchor = AuditEffect::TransferOutput {
            asset_anchor: StateCommitment(Fq::from(22u64)),
            compliance_ciphertext: encrypted.ciphertext.to_bytes(),
            compliance_metadata: metadata.to_bytes().unwrap(),
        }
        .candidate_commitments()
        .unwrap();
        assert_ne!(candidates[0], changed_anchor[0]);
        assert_ne!(candidates[1], changed_anchor[1]);

        let changed_metadata = TransferComplianceMetadata::new(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(4u64),
            5,
            Fq::from(11u64),
            Fq::from(13u64),
            Fq::from(99u64),
            Fq::from(14u64),
        );
        let changed = AuditEffect::TransferOutput {
            asset_anchor: StateCommitment(Fq::from(21u64)),
            compliance_ciphertext: encrypted.ciphertext.to_bytes(),
            compliance_metadata: changed_metadata.to_bytes().unwrap(),
        }
        .candidate_commitments()
        .unwrap();
        assert_ne!(candidates[0], changed[0]);
        assert_eq!(candidates[1], changed[1]);

        let withdrawal =
            encrypt_withdrawal(&mut OsRng, decaf377::Element::GENERATOR, &sender).unwrap();
        let withdrawal_effect = AuditEffect::Withdrawal {
            kind: WithdrawalKind::Host,
            asset_id,
            amount: 7,
            asset_anchor: StateCommitment(Fq::from(21u64)),
            compliance_ciphertext: withdrawal.ciphertext.to_bytes().to_vec(),
        };
        assert_eq!(withdrawal_effect.candidate_commitments().unwrap().len(), 1);
    }

    #[cfg(feature = "component")]
    #[tokio::test]
    async fn state_persists_records_and_replays_the_authenticated_head() {
        let storage = cnidarium::TempStorage::new().await.unwrap();
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        let first = record(10, 0, 1);
        let second = record(10, 1, 2);

        assert_eq!(state.append_audit_effect(first.clone()).await.unwrap(), 0);
        assert_eq!(state.append_audit_effect(second.clone()).await.unwrap(), 1);
        let authenticated = state.get_audit_log_state().await.unwrap();
        storage.commit(state).await.unwrap();

        let snapshot = storage.latest_snapshot();
        assert_eq!(snapshot.get_audit_record(0).await.unwrap(), Some(first));
        assert_eq!(snapshot.get_audit_record(1).await.unwrap(), Some(second));
        assert_eq!(snapshot.verify_audit_log().await.unwrap(), authenticated);
    }

    #[cfg(feature = "component")]
    #[tokio::test]
    async fn replay_rejects_missing_or_tampered_records() {
        use cnidarium::StateWrite as _;

        let storage = cnidarium::TempStorage::new().await.unwrap();
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        state.append_audit_effect(record(10, 0, 1)).await.unwrap();
        state.append_audit_effect(record(10, 1, 2)).await.unwrap();

        state.nonverifiable_delete(state_key::audit_log::record(1));
        assert!(state.verify_audit_log().await.is_err());

        state.nonverifiable_put_raw(
            state_key::audit_log::record(1),
            record(10, 1, 3).encode().unwrap(),
        );
        assert!(state.verify_audit_log().await.is_err());
    }
}
