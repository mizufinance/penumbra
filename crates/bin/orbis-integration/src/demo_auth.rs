use orbis_authn::JwtSigner;

pub fn dkg_signer() -> JwtSigner {
    JwtSigner::new()
}
