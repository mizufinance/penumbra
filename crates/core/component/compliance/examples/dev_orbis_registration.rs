use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::structs::OrbisCapabilityCertificate;
use shieldd_sdk_compliance::{derive_regulated_nullifier_key, AssetPolicy, ComplianceLeaf};
use shieldd_sdk_keys::test_keys;
use shieldd_sdk_proto::DomainType;

fn bundle(address_index: u32) -> anyhow::Result<(String, String, String, String)> {
    let fvk = &*test_keys::FULL_VIEWING_KEY;
    let address = fvk.payment_address(address_index.into());
    let ring_sk = decaf377::Fr::from(1u64);
    let ring_pk = decaf377::Element::GENERATOR * ring_sk;
    let rnk_dh_pk = address.diversified_generator() * ring_sk;
    let asset_id = asset::REGISTRY
        .parse_denom("wregulated_usd")
        .expect("regulated smoke asset is registered")
        .id();
    let policy = AssetPolicy::new(
        decaf377::Element::GENERATOR,
        u128::MAX,
        Vec::new(),
        None,
        "shieldd-dev-ring".to_owned(),
        ring_pk,
        "shieldd-dev-policy".to_owned(),
        "read".to_owned(),
        "document".to_owned(),
    );
    let rnk =
        derive_regulated_nullifier_key(fvk.incoming(), &address, asset_id, ring_pk, rnk_dh_pk)?;
    let leaf =
        ComplianceLeaf::registered_from_rnk(address.clone(), asset_id, ring_pk, rnk_dh_pk, rnk)?;
    let certificate =
        OrbisCapabilityCertificate::sign_for_test("shieldd-local-devnet", &leaf, &policy, ring_sk)?;
    Ok((
        address.to_string(),
        hex::encode(rnk_dh_pk.vartime_compress().0),
        hex::encode(leaf.rnk_commitment.to_bytes()),
        hex::encode(certificate.encode_to_vec()),
    ))
}

fn main() -> anyhow::Result<()> {
    let first = bundle(0)?;
    let second = bundle(1)?;
    let third = bundle(2)?;
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        first.0,
        first.1,
        first.2,
        first.3,
        second.0,
        second.1,
        second.2,
        second.3,
        third.0,
        third.1,
        third.2,
        third.3,
    );
    Ok(())
}
