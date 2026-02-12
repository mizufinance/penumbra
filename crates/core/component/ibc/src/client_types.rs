use anyhow::{anyhow, bail};
use ibc_proto::google::protobuf::Any;
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
pub use ibc_types::lightclients::tendermint::misbehaviour::TENDERMINT_MISBEHAVIOUR_TYPE_URL;
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
    pub fn client_type(&self) -> &'static str {
        match self {
            AnyClientState::Tendermint(_) => "07-tendermint",
            AnyClientState::Bankd(_) => "bankd",
        }
    }

    pub fn latest_height(&self) -> Height {
        match self {
            AnyClientState::Tendermint(cs) => cs.latest_height(),
            AnyClientState::Bankd(cs) => {
                let h = cs.latest_height.as_ref().expect("bankd client state missing latest_height");
                Height::new(h.revision_number, h.revision_height)
                    .expect("invalid bankd latest height")
            }
        }
    }

    pub fn is_frozen(&self) -> bool {
        match self {
            AnyClientState::Tendermint(cs) => cs.is_frozen(),
            AnyClientState::Bankd(cs) => {
                cs.frozen_height.as_ref().map_or(false, |h| {
                    h.revision_number > 0 || h.revision_height > 0
                })
            }
        }
    }

    pub fn proof_specs(&self) -> Vec<ics23::ProofSpec> {
        match self {
            AnyClientState::Tendermint(cs) => cs.proof_specs.clone(),
            AnyClientState::Bankd(cs) => {
                cs.proof_specs.iter().map(|spec| {
                    ics23::ProofSpec {
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
                    }
                }).collect()
            }
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
}

impl AnyConsensusState {
    pub fn root(&self) -> Vec<u8> {
        match self {
            AnyConsensusState::Tendermint(cs) => cs.root.hash.clone(),
            AnyConsensusState::Bankd(cs) => cs.root.clone(),
        }
    }

    pub fn timestamp_as_unix_secs(&self) -> anyhow::Result<u64> {
        match self {
            AnyConsensusState::Tendermint(cs) => {
                let t = cs.timestamp;
                Ok(t.unix_timestamp() as u64)
            }
            AnyConsensusState::Bankd(cs) => Ok(cs.timestamp),
        }
    }
}

impl AnyHeader {
    pub fn height(&self) -> Height {
        match self {
            AnyHeader::Tendermint(h) => h.height(),
            AnyHeader::Bankd(h) => {
                let ht = h.height.as_ref().expect("bankd header missing height");
                Height::new(ht.revision_number, ht.revision_height)
                    .expect("invalid bankd header height")
            }
        }
    }

    pub fn trusted_height(&self) -> Height {
        match self {
            AnyHeader::Tendermint(h) => h.trusted_height,
            AnyHeader::Bankd(h) => {
                let ht = h.trusted_height.as_ref().expect("bankd header missing trusted_height");
                Height::new(ht.revision_number, ht.revision_height)
                    .expect("invalid bankd trusted height")
            }
        }
    }

    pub fn timestamp_as_unix_secs(&self) -> anyhow::Result<u64> {
        match self {
            AnyHeader::Tendermint(h) => {
                let t = h.signed_header.header.time;
                Ok(t.unix_timestamp() as u64)
            }
            AnyHeader::Bankd(h) => Ok(h.timestamp),
        }
    }
}
