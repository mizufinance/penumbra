//! Protobuf wire types for Shieldd.
//! Convert into domain types through [`DomainType`] to validate decoded data.

// The autogen code is not clippy-clean, so we disable some clippy warnings for this crate.
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::unwrap_used)]
#![allow(non_snake_case)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use prost::{Message, Name};

/// Helper methods used for shaping the JSON (and other Serde) formats derived from the protos.
pub mod serializers;

#[cfg(feature = "box-grpc")]
pub mod box_grpc_svc;

/// Helper trait for using Protobuf messages as ABCI events.
pub mod event;
mod protobuf;
pub use protobuf::DomainType;

#[cfg(feature = "cnidarium")]
pub mod state;
#[cfg(feature = "cnidarium")]
pub use state::StateReadProto;
#[cfg(feature = "cnidarium")]
pub use state::StateWriteProto;

pub use shieldd::*;

pub mod shieldd {
    /// Shieldd storage query structures.
    pub mod cnidarium {
        pub mod v1 {
            include!("gen/shieldd.cnidarium.v1.rs");
            include!("gen/shieldd.cnidarium.v1.serde.rs");
        }
    }

    /// Core protocol structures.
    pub mod core {
        /// Top-level structures for the Shieldd application.
        pub mod app {
            pub mod v1 {
                include!("gen/shieldd.core.app.v1.rs");
                include!("gen/shieldd.core.app.v1.serde.rs");
            }
        }

        pub mod asset {
            pub mod v1 {
                include!("gen/shieldd.core.asset.v1.rs");
                include!("gen/shieldd.core.asset.v1.serde.rs");
            }
        }

        pub mod txhash {
            pub mod v1 {
                include!("gen/shieldd.core.txhash.v1.rs");
                include!("gen/shieldd.core.txhash.v1.serde.rs");
            }
        }

        /// Components of the Shieldd application.
        pub mod component {
            pub mod compact_block {
                pub mod v1 {
                    include!("gen/shieldd.core.component.compact_block.v1.rs");
                    include!("gen/shieldd.core.component.compact_block.v1.serde.rs");
                }
            }

            pub mod compliance {
                pub mod v1 {
                    include!("gen/shieldd.core.component.compliance.v1.rs");
                    include!("gen/shieldd.core.component.compliance.v1.serde.rs");
                }
            }

            pub mod fee {
                pub mod v1 {
                    include!("gen/shieldd.core.component.fee.v1.rs");
                    include!("gen/shieldd.core.component.fee.v1.serde.rs");
                }
            }

            pub mod ibc {
                pub mod v1 {
                    include!("gen/shieldd.core.component.ibc.v1.rs");
                    include!("gen/shieldd.core.component.ibc.v1.serde.rs");
                }
            }

            pub mod sct {
                pub mod v1 {
                    include!("gen/shieldd.core.component.sct.v1.rs");
                    include!("gen/shieldd.core.component.sct.v1.serde.rs");
                }
            }

            pub mod shielded_pool {
                pub mod v1 {
                    include!("gen/shieldd.core.component.shielded_pool.v1.rs");
                    include!("gen/shieldd.core.component.shielded_pool.v1.serde.rs");
                }
            }
        }

        pub mod keys {
            pub mod v1 {
                include!("gen/shieldd.core.keys.v1.rs");
                include!("gen/shieldd.core.keys.v1.serde.rs");
            }
        }

        pub mod num {
            pub mod v1 {
                include!("gen/shieldd.core.num.v1.rs");
                include!("gen/shieldd.core.num.v1.serde.rs");
            }
        }

        /// Transaction structures.
        pub mod transaction {
            pub mod v1 {
                include!("gen/shieldd.core.transaction.v1.rs");
                include!("gen/shieldd.core.transaction.v1.serde.rs");
            }
        }
    }

    /// Cryptography primitives used by Shieldd.
    pub mod crypto {
        pub mod decaf377_frost {
            pub mod v1 {
                include!("gen/shieldd.crypto.decaf377_frost.v1.rs");
                include!("gen/shieldd.crypto.decaf377_frost.v1.serde.rs");
            }
        }

        pub mod decaf377_rdsa {
            pub mod v1 {
                include!("gen/shieldd.crypto.decaf377_rdsa.v1.rs");
                include!("gen/shieldd.crypto.decaf377_rdsa.v1.serde.rs");
            }
        }

        pub mod tct {
            pub mod v1 {
                include!("gen/shieldd.crypto.tct.v1.rs");
                include!("gen/shieldd.crypto.tct.v1.serde.rs");
            }
        }
    }

    /// Custody protocol structures.
    pub mod custody {
        pub mod threshold {
            pub mod v1 {
                include!("gen/shieldd.custody.threshold.v1.rs");
                include!("gen/shieldd.custody.threshold.v1.serde.rs");
            }
        }

        pub mod v1 {
            include!("gen/shieldd.custody.v1.rs");
            include!("gen/shieldd.custody.v1.serde.rs");
        }
    }

    /// Execution-client RPC surface.
    pub mod execution_client {
        pub mod v1 {
            include!("gen/shieldd.execution_client.v1.rs");
            include!("gen/shieldd.execution_client.v1.serde.rs");
        }
    }

    pub mod util {
        pub mod node {
            pub mod v1 {
                include!("gen/shieldd.util.node.v1.rs");
                include!("gen/shieldd.util.node.v1.serde.rs");
            }
        }

        pub mod tendermint_proxy {
            pub mod v1 {
                include!("gen/shieldd.util.tendermint_proxy.v1.rs");
                include!("gen/shieldd.util.tendermint_proxy.v1.serde.rs");

                /// gRPC metadata set only after the proxy attempted an
                /// upstream transaction broadcast whose outcome is unknown.
                pub const BROADCAST_OUTCOME_METADATA_KEY: &str = "shieldd-broadcast-outcome";
                pub const BROADCAST_OUTCOME_UNKNOWN: &str = "unknown";
                pub const BROADCAST_OUTCOME_NOT_SUBMITTED: &str = "not-submitted";
            }
        }
    }

    /// View protocol structures.
    pub mod view {
        pub mod v1 {
            include!("gen/shieldd.view.v1.rs");
            include!("gen/shieldd.view.v1.serde.rs");
        }
    }
}

pub mod tendermint {
    pub mod crypto {
        include!("gen/tendermint.crypto.rs");
    }

    #[allow(clippy::large_enum_variant)]
    pub mod types {
        include!("gen/tendermint.types.rs");
    }

    pub mod version {
        include!("gen/tendermint.version.rs");
    }

    pub mod p2p {
        include!("gen/tendermint.p2p.rs");
    }

    pub mod abci {
        include!("gen/tendermint.abci.rs");
    }
}

pub mod noble {
    pub mod forwarding {
        pub mod v1 {
            include!("gen/noble.forwarding.v1.rs");
        }
    }
}

pub mod cosmos {
    pub mod base {
        pub mod v1beta1 {
            include!("gen/cosmos.base.v1beta1.rs");
        }

        pub mod query {
            pub mod v1beta1 {
                include!("gen/cosmos.base.query.v1beta1.rs");
            }
        }

        pub mod abci {
            pub mod v1beta1 {
                include!("gen/cosmos.base.abci.v1beta1.rs");
            }
        }
    }

    pub mod auth {
        pub mod v1beta1 {
            include!("gen/cosmos.auth.v1beta1.rs");
        }
    }

    pub mod bank {
        pub mod v1beta1 {
            include!("gen/cosmos.bank.v1beta1.rs");
        }
    }

    pub mod tx {
        pub mod v1beta1 {
            include!("gen/cosmos.tx.v1beta1.rs");
        }

        pub mod config {
            pub mod v1 {
                include!("gen/cosmos.tx.config.v1.rs");
            }
        }

        pub mod signing {
            pub mod v1beta1 {
                include!("gen/cosmos.tx.signing.v1beta1.rs");
            }
        }
    }

    pub mod crypto {
        pub mod multisig {
            pub mod v1beta1 {
                include!("gen/cosmos.crypto.multisig.v1beta1.rs");
            }
        }
    }
}

#[cfg(feature = "rpc")]
// https://github.com/mizufinance/shieldd/issues/3038#issuecomment-1722534133
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("gen/proto_descriptor.bin.no_lfs");
