//! Asset types and identifiers.
mod cache;
mod denom;
mod denom_metadata;
mod id;
mod registry;

pub use cache::Cache;
pub use denom::Denom;
pub use denom_metadata::{Metadata, Unit};
pub use id::{Id, VALUE_GENERATOR_DOMAIN_SEP};
pub use registry::{Registry, REGISTRY};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn test_registry_native_token() {
        // We should be able to use `parse_base` with the valid base denomination.
        let base_denom = Cache::with_known_assets()
            .get_unit("ushieldd")
            .unwrap()
            .base();
        assert_eq!(format!("{base_denom}"), "ushieldd".to_string());

        // If we try to use `parse_base` with a display denomination, we should get `None`.
        let display_denoms = vec!["mshieldd", "shieldd"];
        for display_denom in &display_denoms {
            assert!(REGISTRY.parse_denom(display_denom).is_none());
        }

        // We should be able to use the display denominations with `parse_display` however.
        for display_denom in display_denoms {
            let parsed_display_denom = REGISTRY.parse_unit(display_denom);

            assert_eq!(
                format!("{}", parsed_display_denom.base()),
                "ushieldd".to_string()
            );

            assert_eq!(format!("{parsed_display_denom}"), display_denom)
        }

        // The base denomination (ushieldd) can also be used for display purposes.
        let parsed_display_denom = REGISTRY.parse_unit("ushieldd");
        assert_eq!(
            format!("{}", parsed_display_denom.base()),
            "ushieldd".to_string()
        );
    }

    #[test]
    fn test_displaydenom_format_value() {
        // with exponent 6, 1782000 formats to 1.782
        let shieldd_sdk_display_denom = REGISTRY.parse_unit("shieldd");
        assert_eq!(
            shieldd_sdk_display_denom.format_value(1782000u64.into()),
            "1.782"
        );
        assert_eq!(
            shieldd_sdk_display_denom.format_value(6700001u64.into()),
            "6.700001"
        );
        assert_eq!(
            shieldd_sdk_display_denom.format_value(1u64.into()),
            "0.000001"
        );

        // with exponent 3, 1782000 formats to 1782
        let mshieldd_sdk_display_denom = REGISTRY.parse_unit("mshieldd");
        assert_eq!(
            mshieldd_sdk_display_denom.format_value(1782000u64.into()),
            "1782"
        );

        // with exponent 0, 1782000 formats to 1782000
        let ushieldd_sdk_display_denom = REGISTRY.parse_unit("ushieldd");
        assert_eq!(
            ushieldd_sdk_display_denom.format_value(1782000u64.into()),
            "1782000"
        );
    }

    #[test]
    fn best_unit_for() {
        let base_denom = Cache::with_known_assets()
            .get_unit("ushieldd")
            .unwrap()
            .base();

        assert_eq!(base_denom.best_unit_for(0u64.into()).to_string(), "shieldd");
        assert_eq!(
            base_denom.best_unit_for(999u64.into()).to_string(),
            "ushieldd"
        );
        assert_eq!(
            base_denom.best_unit_for(1_000u64.into()).to_string(),
            "mshieldd"
        );
        assert_eq!(
            base_denom.best_unit_for(999_999u64.into()).to_string(),
            "mshieldd"
        );
        assert_eq!(
            base_denom.best_unit_for(1_000_000u64.into()).to_string(),
            "shieldd"
        );
    }

    #[test]
    fn test_displaydenom_parse_value() {
        let shieldd_sdk_display_denom = REGISTRY.parse_unit("shieldd");
        assert!(shieldd_sdk_display_denom.parse_value("1.2.3").is_err());

        assert_eq!(
            shieldd_sdk_display_denom.parse_value("1.782").unwrap(),
            1782000u64.into()
        );
        assert_eq!(
            shieldd_sdk_display_denom.parse_value("6.700001").unwrap(),
            6700001u64.into()
        );

        let mshieldd_sdk_display_denom = REGISTRY.parse_unit("mshieldd");
        assert_eq!(
            mshieldd_sdk_display_denom.parse_value("1782").unwrap(),
            1782000u64.into()
        );
        assert!(mshieldd_sdk_display_denom.parse_value("1782.0001").is_err());

        let ushieldd_sdk_display_denom = REGISTRY.parse_unit("ushieldd");
        assert_eq!(
            ushieldd_sdk_display_denom.parse_value("1782000").unwrap(),
            1782000u64.into()
        );
    }

    #[test]
    fn test_get_unit() {
        let unit = Cache::with_known_assets().get_unit("cube").unwrap();

        assert_eq!(format!("{unit}"), "cube".to_string());
    }

    proptest! {
        #[test]
        fn displaydenom_parsing_formatting_roundtrip(
            v: u128
        ) {
            let shieldd_sdk_display_denom = REGISTRY.parse_unit("shieldd");
            let formatted = shieldd_sdk_display_denom.format_value(v.into());
            let parsed = shieldd_sdk_display_denom.parse_value(&formatted);
            assert_eq!(v, u128::from(parsed.unwrap()));

            let mshieldd_sdk_display_denom = REGISTRY.parse_unit("mshieldd");
            let formatted = mshieldd_sdk_display_denom.format_value(v.into());
            let parsed = mshieldd_sdk_display_denom.parse_value(&formatted);
            assert_eq!(v, u128::from(parsed.unwrap()));
        }
    }
}
