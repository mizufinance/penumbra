use did_key::{generate, Ed25519KeyPair as DidEd25519KeyPair};
use orbis_authn::JwtSigner;
use sha2::{Digest, Sha256};

const DEFAULT_READER_DID_PK: &str = "test_jwt";

pub(crate) fn default_reader_did_pk() -> &'static str {
    DEFAULT_READER_DID_PK
}

pub(crate) fn deterministic_jwt_signer(reader_did_pk: &str) -> JwtSigner {
    let key_pair = generate::<DidEd25519KeyPair>(Some(&did_seed(reader_did_pk)));
    JwtSigner::from_key_pair(key_pair)
}

fn did_seed(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}
