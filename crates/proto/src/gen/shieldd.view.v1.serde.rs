impl serde::Serialize for AddressByIndexRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address_index.is_some() {
            len += 1;
        }
        if self.purpose.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AddressByIndexRequest", len)?;
        if let Some(v) = self.address_index.as_ref() {
            struct_ser.serialize_field("addressIndex", v)?;
        }
        if let Some(v) = self.purpose.as_ref() {
            struct_ser.serialize_field("purpose", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddressByIndexRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address_index",
            "addressIndex",
            "purpose",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddressIndex,
            Purpose,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "addressIndex" | "address_index" => Ok(GeneratedField::AddressIndex),
                            "purpose" => Ok(GeneratedField::Purpose),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddressByIndexRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AddressByIndexRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddressByIndexRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address_index__ = None;
                let mut purpose__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddressIndex => {
                            if address_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressIndex"));
                            }
                            address_index__ = map_.next_value()?;
                        }
                        GeneratedField::Purpose => {
                            if purpose__.is_some() {
                                return Err(serde::de::Error::duplicate_field("purpose"));
                            }
                            purpose__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AddressByIndexRequest {
                    address_index: address_index__,
                    purpose: purpose__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AddressByIndexRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AddressByIndexResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AddressByIndexResponse", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddressByIndexResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddressByIndexResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AddressByIndexResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddressByIndexResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AddressByIndexResponse {
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AddressByIndexResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AddressPurpose {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.regulated_asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AddressPurpose", len)?;
        if let Some(v) = self.regulated_asset_id.as_ref() {
            struct_ser.serialize_field("regulatedAssetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddressPurpose {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "regulated_asset_id",
            "regulatedAssetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RegulatedAssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "regulatedAssetId" | "regulated_asset_id" => Ok(GeneratedField::RegulatedAssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddressPurpose;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AddressPurpose")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddressPurpose, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut regulated_asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RegulatedAssetId => {
                            if regulated_asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("regulatedAssetId"));
                            }
                            regulated_asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AddressPurpose {
                    regulated_asset_id: regulated_asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AddressPurpose", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AppParametersRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.AppParametersRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AppParametersRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AppParametersRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AppParametersRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AppParametersRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(AppParametersRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AppParametersRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AppParametersResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.parameters.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AppParametersResponse", len)?;
        if let Some(v) = self.parameters.as_ref() {
            struct_ser.serialize_field("parameters", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AppParametersResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "parameters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Parameters,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "parameters" => Ok(GeneratedField::Parameters),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AppParametersResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AppParametersResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AppParametersResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut parameters__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Parameters => {
                            if parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("parameters"));
                            }
                            parameters__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AppParametersResponse {
                    parameters: parameters__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AppParametersResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetMetadataByIdRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AssetMetadataByIdRequest", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetMetadataByIdRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetMetadataByIdRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AssetMetadataByIdRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetMetadataByIdRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetMetadataByIdRequest {
                    asset_id: asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AssetMetadataByIdRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetMetadataByIdResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.denom_metadata.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AssetMetadataByIdResponse", len)?;
        if let Some(v) = self.denom_metadata.as_ref() {
            struct_ser.serialize_field("denomMetadata", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetMetadataByIdResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "denom_metadata",
            "denomMetadata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DenomMetadata,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "denomMetadata" | "denom_metadata" => Ok(GeneratedField::DenomMetadata),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetMetadataByIdResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AssetMetadataByIdResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetMetadataByIdResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut denom_metadata__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DenomMetadata => {
                            if denom_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("denomMetadata"));
                            }
                            denom_metadata__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetMetadataByIdResponse {
                    denom_metadata: denom_metadata__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AssetMetadataByIdResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.filtered {
            len += 1;
        }
        if !self.include_specific_denominations.is_empty() {
            len += 1;
        }
        if self.include_proposal_nfts {
            len += 1;
        }
        if self.include_voting_receipt_tokens {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AssetsRequest", len)?;
        if self.filtered {
            struct_ser.serialize_field("filtered", &self.filtered)?;
        }
        if !self.include_specific_denominations.is_empty() {
            struct_ser.serialize_field("includeSpecificDenominations", &self.include_specific_denominations)?;
        }
        if self.include_proposal_nfts {
            struct_ser.serialize_field("includeProposalNfts", &self.include_proposal_nfts)?;
        }
        if self.include_voting_receipt_tokens {
            struct_ser.serialize_field("includeVotingReceiptTokens", &self.include_voting_receipt_tokens)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filtered",
            "include_specific_denominations",
            "includeSpecificDenominations",
            "include_proposal_nfts",
            "includeProposalNfts",
            "include_voting_receipt_tokens",
            "includeVotingReceiptTokens",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filtered,
            IncludeSpecificDenominations,
            IncludeProposalNfts,
            IncludeVotingReceiptTokens,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "filtered" => Ok(GeneratedField::Filtered),
                            "includeSpecificDenominations" | "include_specific_denominations" => Ok(GeneratedField::IncludeSpecificDenominations),
                            "includeProposalNfts" | "include_proposal_nfts" => Ok(GeneratedField::IncludeProposalNfts),
                            "includeVotingReceiptTokens" | "include_voting_receipt_tokens" => Ok(GeneratedField::IncludeVotingReceiptTokens),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AssetsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filtered__ = None;
                let mut include_specific_denominations__ = None;
                let mut include_proposal_nfts__ = None;
                let mut include_voting_receipt_tokens__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Filtered => {
                            if filtered__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filtered"));
                            }
                            filtered__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IncludeSpecificDenominations => {
                            if include_specific_denominations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("includeSpecificDenominations"));
                            }
                            include_specific_denominations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IncludeProposalNfts => {
                            if include_proposal_nfts__.is_some() {
                                return Err(serde::de::Error::duplicate_field("includeProposalNfts"));
                            }
                            include_proposal_nfts__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IncludeVotingReceiptTokens => {
                            if include_voting_receipt_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("includeVotingReceiptTokens"));
                            }
                            include_voting_receipt_tokens__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetsRequest {
                    filtered: filtered__.unwrap_or_default(),
                    include_specific_denominations: include_specific_denominations__.unwrap_or_default(),
                    include_proposal_nfts: include_proposal_nfts__.unwrap_or_default(),
                    include_voting_receipt_tokens: include_voting_receipt_tokens__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AssetsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.denom_metadata.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AssetsResponse", len)?;
        if let Some(v) = self.denom_metadata.as_ref() {
            struct_ser.serialize_field("denomMetadata", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "denom_metadata",
            "denomMetadata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DenomMetadata,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "denomMetadata" | "denom_metadata" => Ok(GeneratedField::DenomMetadata),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AssetsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut denom_metadata__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DenomMetadata => {
                            if denom_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("denomMetadata"));
                            }
                            denom_metadata__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetsResponse {
                    denom_metadata: denom_metadata__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AssetsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AuthorizeAndBuildRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction_plan.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AuthorizeAndBuildRequest", len)?;
        if let Some(v) = self.transaction_plan.as_ref() {
            struct_ser.serialize_field("transactionPlan", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AuthorizeAndBuildRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction_plan",
            "transactionPlan",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TransactionPlan,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transactionPlan" | "transaction_plan" => Ok(GeneratedField::TransactionPlan),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AuthorizeAndBuildRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AuthorizeAndBuildRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AuthorizeAndBuildRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction_plan__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TransactionPlan => {
                            if transaction_plan__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionPlan"));
                            }
                            transaction_plan__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AuthorizeAndBuildRequest {
                    transaction_plan: transaction_plan__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AuthorizeAndBuildRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AuthorizeAndBuildResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.status.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse", len)?;
        if let Some(v) = self.status.as_ref() {
            match v {
                authorize_and_build_response::Status::BuildProgress(v) => {
                    struct_ser.serialize_field("buildProgress", v)?;
                }
                authorize_and_build_response::Status::Complete(v) => {
                    struct_ser.serialize_field("complete", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AuthorizeAndBuildResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "build_progress",
            "buildProgress",
            "complete",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BuildProgress,
            Complete,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "buildProgress" | "build_progress" => Ok(GeneratedField::BuildProgress),
                            "complete" => Ok(GeneratedField::Complete),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AuthorizeAndBuildResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AuthorizeAndBuildResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AuthorizeAndBuildResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut status__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BuildProgress => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("buildProgress"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(authorize_and_build_response::Status::BuildProgress)
;
                        }
                        GeneratedField::Complete => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complete"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(authorize_and_build_response::Status::Complete)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AuthorizeAndBuildResponse {
                    status: status__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for authorize_and_build_response::BuildProgress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.progress != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse.BuildProgress", len)?;
        if self.progress != 0. {
            struct_ser.serialize_field("progress", &self.progress)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for authorize_and_build_response::BuildProgress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "progress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Progress,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "progress" => Ok(GeneratedField::Progress),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = authorize_and_build_response::BuildProgress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AuthorizeAndBuildResponse.BuildProgress")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<authorize_and_build_response::BuildProgress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut progress__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Progress => {
                            if progress__.is_some() {
                                return Err(serde::de::Error::duplicate_field("progress"));
                            }
                            progress__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(authorize_and_build_response::BuildProgress {
                    progress: progress__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse.BuildProgress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for authorize_and_build_response::Complete {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse.Complete", len)?;
        if let Some(v) = self.transaction.as_ref() {
            struct_ser.serialize_field("transaction", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for authorize_and_build_response::Complete {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transaction,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transaction" => Ok(GeneratedField::Transaction),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = authorize_and_build_response::Complete;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.AuthorizeAndBuildResponse.Complete")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<authorize_and_build_response::Complete, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transaction => {
                            if transaction__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transaction"));
                            }
                            transaction__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(authorize_and_build_response::Complete {
                    transaction: transaction__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.AuthorizeAndBuildResponse.Complete", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BalancesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.account_filter.is_some() {
            len += 1;
        }
        if self.asset_id_filter.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BalancesRequest", len)?;
        if let Some(v) = self.account_filter.as_ref() {
            struct_ser.serialize_field("accountFilter", v)?;
        }
        if let Some(v) = self.asset_id_filter.as_ref() {
            struct_ser.serialize_field("assetIdFilter", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BalancesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_filter",
            "accountFilter",
            "asset_id_filter",
            "assetIdFilter",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountFilter,
            AssetIdFilter,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "accountFilter" | "account_filter" => Ok(GeneratedField::AccountFilter),
                            "assetIdFilter" | "asset_id_filter" => Ok(GeneratedField::AssetIdFilter),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BalancesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BalancesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BalancesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account_filter__ = None;
                let mut asset_id_filter__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AccountFilter => {
                            if account_filter__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountFilter"));
                            }
                            account_filter__ = map_.next_value()?;
                        }
                        GeneratedField::AssetIdFilter => {
                            if asset_id_filter__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetIdFilter"));
                            }
                            asset_id_filter__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BalancesRequest {
                    account_filter: account_filter__,
                    asset_id_filter: asset_id_filter__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BalancesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BalancesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.account.is_some() {
            len += 1;
        }
        if self.balance.is_some() {
            len += 1;
        }
        if self.account_address.is_some() {
            len += 1;
        }
        if self.balance_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BalancesResponse", len)?;
        if let Some(v) = self.account.as_ref() {
            struct_ser.serialize_field("account", v)?;
        }
        if let Some(v) = self.balance.as_ref() {
            struct_ser.serialize_field("balance", v)?;
        }
        if let Some(v) = self.account_address.as_ref() {
            struct_ser.serialize_field("accountAddress", v)?;
        }
        if let Some(v) = self.balance_view.as_ref() {
            struct_ser.serialize_field("balanceView", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BalancesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account",
            "balance",
            "account_address",
            "accountAddress",
            "balance_view",
            "balanceView",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Account,
            Balance,
            AccountAddress,
            BalanceView,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "account" => Ok(GeneratedField::Account),
                            "balance" => Ok(GeneratedField::Balance),
                            "accountAddress" | "account_address" => Ok(GeneratedField::AccountAddress),
                            "balanceView" | "balance_view" => Ok(GeneratedField::BalanceView),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BalancesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BalancesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BalancesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account__ = None;
                let mut balance__ = None;
                let mut account_address__ = None;
                let mut balance_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Account => {
                            if account__.is_some() {
                                return Err(serde::de::Error::duplicate_field("account"));
                            }
                            account__ = map_.next_value()?;
                        }
                        GeneratedField::Balance => {
                            if balance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balance"));
                            }
                            balance__ = map_.next_value()?;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = map_.next_value()?;
                        }
                        GeneratedField::BalanceView => {
                            if balance_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balanceView"));
                            }
                            balance_view__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BalancesResponse {
                    account: account__,
                    balance: balance__,
                    account_address: account_address__,
                    balance_view: balance_view__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BalancesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BroadcastTransactionRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction.is_some() {
            len += 1;
        }
        if self.await_detection {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BroadcastTransactionRequest", len)?;
        if let Some(v) = self.transaction.as_ref() {
            struct_ser.serialize_field("transaction", v)?;
        }
        if self.await_detection {
            struct_ser.serialize_field("awaitDetection", &self.await_detection)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BroadcastTransactionRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction",
            "await_detection",
            "awaitDetection",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transaction,
            AwaitDetection,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transaction" => Ok(GeneratedField::Transaction),
                            "awaitDetection" | "await_detection" => Ok(GeneratedField::AwaitDetection),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BroadcastTransactionRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BroadcastTransactionRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BroadcastTransactionRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction__ = None;
                let mut await_detection__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transaction => {
                            if transaction__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transaction"));
                            }
                            transaction__ = map_.next_value()?;
                        }
                        GeneratedField::AwaitDetection => {
                            if await_detection__.is_some() {
                                return Err(serde::de::Error::duplicate_field("awaitDetection"));
                            }
                            await_detection__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BroadcastTransactionRequest {
                    transaction: transaction__,
                    await_detection: await_detection__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BroadcastTransactionRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BroadcastTransactionResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.status.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BroadcastTransactionResponse", len)?;
        if let Some(v) = self.status.as_ref() {
            match v {
                broadcast_transaction_response::Status::BroadcastSuccess(v) => {
                    struct_ser.serialize_field("broadcastSuccess", v)?;
                }
                broadcast_transaction_response::Status::Confirmed(v) => {
                    struct_ser.serialize_field("confirmed", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BroadcastTransactionResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "broadcast_success",
            "broadcastSuccess",
            "confirmed",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BroadcastSuccess,
            Confirmed,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "broadcastSuccess" | "broadcast_success" => Ok(GeneratedField::BroadcastSuccess),
                            "confirmed" => Ok(GeneratedField::Confirmed),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BroadcastTransactionResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BroadcastTransactionResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BroadcastTransactionResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut status__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BroadcastSuccess => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("broadcastSuccess"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(broadcast_transaction_response::Status::BroadcastSuccess)
;
                        }
                        GeneratedField::Confirmed => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("confirmed"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(broadcast_transaction_response::Status::Confirmed)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BroadcastTransactionResponse {
                    status: status__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BroadcastTransactionResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for broadcast_transaction_response::BroadcastSuccess {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BroadcastTransactionResponse.BroadcastSuccess", len)?;
        if let Some(v) = self.id.as_ref() {
            struct_ser.serialize_field("id", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for broadcast_transaction_response::BroadcastSuccess {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = broadcast_transaction_response::BroadcastSuccess;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BroadcastTransactionResponse.BroadcastSuccess")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<broadcast_transaction_response::BroadcastSuccess, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(broadcast_transaction_response::BroadcastSuccess {
                    id: id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BroadcastTransactionResponse.BroadcastSuccess", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for broadcast_transaction_response::Confirmed {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.id.is_some() {
            len += 1;
        }
        if self.detection_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.BroadcastTransactionResponse.Confirmed", len)?;
        if let Some(v) = self.id.as_ref() {
            struct_ser.serialize_field("id", v)?;
        }
        if self.detection_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("detectionHeight", ToString::to_string(&self.detection_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for broadcast_transaction_response::Confirmed {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "detection_height",
            "detectionHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            DetectionHeight,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "detectionHeight" | "detection_height" => Ok(GeneratedField::DetectionHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = broadcast_transaction_response::Confirmed;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.BroadcastTransactionResponse.Confirmed")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<broadcast_transaction_response::Confirmed, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut detection_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = map_.next_value()?;
                        }
                        GeneratedField::DetectionHeight => {
                            if detection_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("detectionHeight"));
                            }
                            detection_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(broadcast_transaction_response::Confirmed {
                    id: id__,
                    detection_height: detection_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.BroadcastTransactionResponse.Confirmed", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceAnchorsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceAnchorsRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceAnchorsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceAnchorsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceAnchorsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceAnchorsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(ComplianceAnchorsRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceAnchorsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceAnchorsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.user_tree_root.is_empty() {
            len += 1;
        }
        if !self.asset_tree_root.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceAnchorsResponse", len)?;
        if !self.user_tree_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("userTreeRoot", pbjson::private::base64::encode(&self.user_tree_root).as_str())?;
        }
        if !self.asset_tree_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("assetTreeRoot", pbjson::private::base64::encode(&self.asset_tree_root).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceAnchorsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "user_tree_root",
            "userTreeRoot",
            "asset_tree_root",
            "assetTreeRoot",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UserTreeRoot,
            AssetTreeRoot,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "userTreeRoot" | "user_tree_root" => Ok(GeneratedField::UserTreeRoot),
                            "assetTreeRoot" | "asset_tree_root" => Ok(GeneratedField::AssetTreeRoot),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceAnchorsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceAnchorsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceAnchorsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut user_tree_root__ = None;
                let mut asset_tree_root__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UserTreeRoot => {
                            if user_tree_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userTreeRoot"));
                            }
                            user_tree_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetTreeRoot => {
                            if asset_tree_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetTreeRoot"));
                            }
                            asset_tree_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceAnchorsResponse {
                    user_tree_root: user_tree_root__.unwrap_or_default(),
                    asset_tree_root: asset_tree_root__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceAnchorsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceAssetStatusRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceAssetStatusRequest", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceAssetStatusRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceAssetStatusRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceAssetStatusRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceAssetStatusRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceAssetStatusRequest {
                    asset_id: asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceAssetStatusRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceAssetStatusResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.asset_id.is_some() {
            len += 1;
        }
        if self.is_registered {
            len += 1;
        }
        if self.is_regulated {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.daily_volume_limit.is_empty() {
            len += 1;
        }
        if self.asset_policy.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceAssetStatusResponse", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if self.is_registered {
            struct_ser.serialize_field("isRegistered", &self.is_registered)?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if !self.dk_pub.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dkPub", pbjson::private::base64::encode(&self.dk_pub).as_str())?;
        }
        if !self.daily_volume_limit.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dailyVolumeLimit", pbjson::private::base64::encode(&self.daily_volume_limit).as_str())?;
        }
        if let Some(v) = self.asset_policy.as_ref() {
            struct_ser.serialize_field("assetPolicy", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceAssetStatusResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "is_registered",
            "isRegistered",
            "is_regulated",
            "isRegulated",
            "dk_pub",
            "dkPub",
            "daily_volume_limit",
            "dailyVolumeLimit",
            "asset_policy",
            "assetPolicy",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            IsRegistered,
            IsRegulated,
            DkPub,
            DailyVolumeLimit,
            AssetPolicy,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            "isRegistered" | "is_registered" => Ok(GeneratedField::IsRegistered),
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "dailyVolumeLimit" | "daily_volume_limit" => Ok(GeneratedField::DailyVolumeLimit),
                            "assetPolicy" | "asset_policy" => Ok(GeneratedField::AssetPolicy),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceAssetStatusResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceAssetStatusResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceAssetStatusResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut is_registered__ = None;
                let mut is_regulated__ = None;
                let mut dk_pub__ = None;
                let mut daily_volume_limit__ = None;
                let mut asset_policy__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::IsRegistered => {
                            if is_registered__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegistered"));
                            }
                            is_registered__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DkPub => {
                            if dk_pub__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dkPub"));
                            }
                            dk_pub__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DailyVolumeLimit => {
                            if daily_volume_limit__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dailyVolumeLimit"));
                            }
                            daily_volume_limit__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetPolicy => {
                            if asset_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetPolicy"));
                            }
                            asset_policy__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceAssetStatusResponse {
                    asset_id: asset_id__,
                    is_registered: is_registered__.unwrap_or_default(),
                    is_regulated: is_regulated__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    daily_volume_limit: daily_volume_limit__.unwrap_or_default(),
                    asset_policy: asset_policy__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceAssetStatusResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceBatchMerkleProofsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.queries.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceBatchMerkleProofsRequest", len)?;
        if !self.queries.is_empty() {
            struct_ser.serialize_field("queries", &self.queries)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceBatchMerkleProofsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "queries",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Queries,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "queries" => Ok(GeneratedField::Queries),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceBatchMerkleProofsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceBatchMerkleProofsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceBatchMerkleProofsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut queries__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Queries => {
                            if queries__.is_some() {
                                return Err(serde::de::Error::duplicate_field("queries"));
                            }
                            queries__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceBatchMerkleProofsRequest {
                    queries: queries__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceBatchMerkleProofsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceBatchMerkleProofsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.compliance_anchor.is_empty() {
            len += 1;
        }
        if !self.asset_anchor.is_empty() {
            len += 1;
        }
        if !self.results.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceBatchMerkleProofsResponse", len)?;
        if !self.compliance_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceAnchor", pbjson::private::base64::encode(&self.compliance_anchor).as_str())?;
        }
        if !self.asset_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("assetAnchor", pbjson::private::base64::encode(&self.asset_anchor).as_str())?;
        }
        if !self.results.is_empty() {
            struct_ser.serialize_field("results", &self.results)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceBatchMerkleProofsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "results",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ComplianceAnchor,
            AssetAnchor,
            Results,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "results" => Ok(GeneratedField::Results),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceBatchMerkleProofsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceBatchMerkleProofsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceBatchMerkleProofsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut results__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Results => {
                            if results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("results"));
                            }
                            results__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceBatchMerkleProofsResponse {
                    compliance_anchor: compliance_anchor__.unwrap_or_default(),
                    asset_anchor: asset_anchor__.unwrap_or_default(),
                    results: results__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceBatchMerkleProofsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceBatchQuery {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        if self.asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceBatchQuery", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceBatchQuery {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "asset_id",
            "assetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            AssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceBatchQuery;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceBatchQuery")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceBatchQuery, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceBatchQuery {
                    address: address__,
                    asset_id: asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceBatchQuery", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceMerkleProofsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        if self.asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceMerkleProofsRequest", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceMerkleProofsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "asset_id",
            "assetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            AssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceMerkleProofsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceMerkleProofsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceMerkleProofsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceMerkleProofsRequest {
                    address: address__,
                    asset_id: asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceMerkleProofsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceMerkleProofsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.user_registered {
            len += 1;
        }
        if self.asset_registered {
            len += 1;
        }
        if self.is_regulated {
            len += 1;
        }
        if self.compliance_path.is_some() {
            len += 1;
        }
        if self.compliance_position != 0 {
            len += 1;
        }
        if self.asset_path.is_some() {
            len += 1;
        }
        if self.asset_position != 0 {
            len += 1;
        }
        if !self.compliance_anchor.is_empty() {
            len += 1;
        }
        if !self.asset_anchor.is_empty() {
            len += 1;
        }
        if self.asset_indexed_leaf.is_some() {
            len += 1;
        }
        if self.compliance_leaf.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceMerkleProofsResponse", len)?;
        if self.user_registered {
            struct_ser.serialize_field("userRegistered", &self.user_registered)?;
        }
        if self.asset_registered {
            struct_ser.serialize_field("assetRegistered", &self.asset_registered)?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if let Some(v) = self.compliance_path.as_ref() {
            struct_ser.serialize_field("compliancePath", v)?;
        }
        if self.compliance_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("compliancePosition", ToString::to_string(&self.compliance_position).as_str())?;
        }
        if let Some(v) = self.asset_path.as_ref() {
            struct_ser.serialize_field("assetPath", v)?;
        }
        if self.asset_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("assetPosition", ToString::to_string(&self.asset_position).as_str())?;
        }
        if !self.compliance_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceAnchor", pbjson::private::base64::encode(&self.compliance_anchor).as_str())?;
        }
        if !self.asset_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("assetAnchor", pbjson::private::base64::encode(&self.asset_anchor).as_str())?;
        }
        if let Some(v) = self.asset_indexed_leaf.as_ref() {
            struct_ser.serialize_field("assetIndexedLeaf", v)?;
        }
        if let Some(v) = self.compliance_leaf.as_ref() {
            struct_ser.serialize_field("complianceLeaf", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceMerkleProofsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "user_registered",
            "userRegistered",
            "asset_registered",
            "assetRegistered",
            "is_regulated",
            "isRegulated",
            "compliance_path",
            "compliancePath",
            "compliance_position",
            "compliancePosition",
            "asset_path",
            "assetPath",
            "asset_position",
            "assetPosition",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "asset_indexed_leaf",
            "assetIndexedLeaf",
            "compliance_leaf",
            "complianceLeaf",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UserRegistered,
            AssetRegistered,
            IsRegulated,
            CompliancePath,
            CompliancePosition,
            AssetPath,
            AssetPosition,
            ComplianceAnchor,
            AssetAnchor,
            AssetIndexedLeaf,
            ComplianceLeaf,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "userRegistered" | "user_registered" => Ok(GeneratedField::UserRegistered),
                            "assetRegistered" | "asset_registered" => Ok(GeneratedField::AssetRegistered),
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "compliancePath" | "compliance_path" => Ok(GeneratedField::CompliancePath),
                            "compliancePosition" | "compliance_position" => Ok(GeneratedField::CompliancePosition),
                            "assetPath" | "asset_path" => Ok(GeneratedField::AssetPath),
                            "assetPosition" | "asset_position" => Ok(GeneratedField::AssetPosition),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "assetIndexedLeaf" | "asset_indexed_leaf" => Ok(GeneratedField::AssetIndexedLeaf),
                            "complianceLeaf" | "compliance_leaf" => Ok(GeneratedField::ComplianceLeaf),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceMerkleProofsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceMerkleProofsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceMerkleProofsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut user_registered__ = None;
                let mut asset_registered__ = None;
                let mut is_regulated__ = None;
                let mut compliance_path__ = None;
                let mut compliance_position__ = None;
                let mut asset_path__ = None;
                let mut asset_position__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut asset_indexed_leaf__ = None;
                let mut compliance_leaf__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UserRegistered => {
                            if user_registered__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userRegistered"));
                            }
                            user_registered__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AssetRegistered => {
                            if asset_registered__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetRegistered"));
                            }
                            asset_registered__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CompliancePath => {
                            if compliance_path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliancePath"));
                            }
                            compliance_path__ = map_.next_value()?;
                        }
                        GeneratedField::CompliancePosition => {
                            if compliance_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliancePosition"));
                            }
                            compliance_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetPath => {
                            if asset_path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetPath"));
                            }
                            asset_path__ = map_.next_value()?;
                        }
                        GeneratedField::AssetPosition => {
                            if asset_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetPosition"));
                            }
                            asset_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetIndexedLeaf => {
                            if asset_indexed_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetIndexedLeaf"));
                            }
                            asset_indexed_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceLeaf => {
                            if compliance_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceLeaf"));
                            }
                            compliance_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceMerkleProofsResponse {
                    user_registered: user_registered__.unwrap_or_default(),
                    asset_registered: asset_registered__.unwrap_or_default(),
                    is_regulated: is_regulated__.unwrap_or_default(),
                    compliance_path: compliance_path__,
                    compliance_position: compliance_position__.unwrap_or_default(),
                    asset_path: asset_path__,
                    asset_position: asset_position__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__.unwrap_or_default(),
                    asset_anchor: asset_anchor__.unwrap_or_default(),
                    asset_indexed_leaf: asset_indexed_leaf__,
                    compliance_leaf: compliance_leaf__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceMerkleProofsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceUserLeafRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        if self.asset_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceUserLeafRequest", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceUserLeafRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "asset_id",
            "assetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            AssetId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceUserLeafRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceUserLeafRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceUserLeafRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut asset_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceUserLeafRequest {
                    address: address__,
                    asset_id: asset_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceUserLeafRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceUserLeafResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_registered {
            len += 1;
        }
        if self.leaf.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.ComplianceUserLeafResponse", len)?;
        if self.is_registered {
            struct_ser.serialize_field("isRegistered", &self.is_registered)?;
        }
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceUserLeafResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_registered",
            "isRegistered",
            "leaf",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsRegistered,
            Leaf,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "isRegistered" | "is_registered" => Ok(GeneratedField::IsRegistered),
                            "leaf" => Ok(GeneratedField::Leaf),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceUserLeafResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.ComplianceUserLeafResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceUserLeafResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_registered__ = None;
                let mut leaf__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsRegistered => {
                            if is_registered__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegistered"));
                            }
                            is_registered__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceUserLeafResponse {
                    is_registered: is_registered__.unwrap_or_default(),
                    leaf: leaf__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.ComplianceUserLeafResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DiscoveryParametersRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.DiscoveryParametersRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DiscoveryParametersRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DiscoveryParametersRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.DiscoveryParametersRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DiscoveryParametersRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(DiscoveryParametersRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.DiscoveryParametersRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DiscoveryParametersResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.parameters.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.DiscoveryParametersResponse", len)?;
        if let Some(v) = self.parameters.as_ref() {
            struct_ser.serialize_field("parameters", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DiscoveryParametersResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "parameters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Parameters,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "parameters" => Ok(GeneratedField::Parameters),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DiscoveryParametersResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.DiscoveryParametersResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DiscoveryParametersResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut parameters__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Parameters => {
                            if parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("parameters"));
                            }
                            parameters__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(DiscoveryParametersResponse {
                    parameters: parameters__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.DiscoveryParametersResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EphemeralAddressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address_index.is_some() {
            len += 1;
        }
        if self.purpose.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.EphemeralAddressRequest", len)?;
        if let Some(v) = self.address_index.as_ref() {
            struct_ser.serialize_field("addressIndex", v)?;
        }
        if let Some(v) = self.purpose.as_ref() {
            struct_ser.serialize_field("purpose", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EphemeralAddressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address_index",
            "addressIndex",
            "purpose",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddressIndex,
            Purpose,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "addressIndex" | "address_index" => Ok(GeneratedField::AddressIndex),
                            "purpose" => Ok(GeneratedField::Purpose),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EphemeralAddressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.EphemeralAddressRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EphemeralAddressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address_index__ = None;
                let mut purpose__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddressIndex => {
                            if address_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressIndex"));
                            }
                            address_index__ = map_.next_value()?;
                        }
                        GeneratedField::Purpose => {
                            if purpose__.is_some() {
                                return Err(serde::de::Error::duplicate_field("purpose"));
                            }
                            purpose__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EphemeralAddressRequest {
                    address_index: address_index__,
                    purpose: purpose__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.EphemeralAddressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EphemeralAddressResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.EphemeralAddressResponse", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EphemeralAddressResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EphemeralAddressResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.EphemeralAddressResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EphemeralAddressResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EphemeralAddressResponse {
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.EphemeralAddressResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GasPricesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.GasPricesRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GasPricesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GasPricesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.GasPricesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GasPricesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(GasPricesRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.GasPricesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GasPricesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.gas_prices.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.GasPricesResponse", len)?;
        if let Some(v) = self.gas_prices.as_ref() {
            struct_ser.serialize_field("gasPrices", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GasPricesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "gas_prices",
            "gasPrices",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GasPrices,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "gasPrices" | "gas_prices" => Ok(GeneratedField::GasPrices),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GasPricesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.GasPricesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GasPricesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut gas_prices__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GasPrices => {
                            if gas_prices__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gasPrices"));
                            }
                            gas_prices__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GasPricesResponse {
                    gas_prices: gas_prices__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.GasPricesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexByAddressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.IndexByAddressRequest", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexByAddressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexByAddressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.IndexByAddressRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexByAddressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexByAddressRequest {
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.IndexByAddressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexByAddressResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address_index.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.IndexByAddressResponse", len)?;
        if let Some(v) = self.address_index.as_ref() {
            struct_ser.serialize_field("addressIndex", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexByAddressResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address_index",
            "addressIndex",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddressIndex,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "addressIndex" | "address_index" => Ok(GeneratedField::AddressIndex),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexByAddressResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.IndexByAddressResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexByAddressResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address_index__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddressIndex => {
                            if address_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressIndex"));
                            }
                            address_index__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexByAddressResponse {
                    address_index: address_index__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.IndexByAddressResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteByCommitmentRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_commitment.is_some() {
            len += 1;
        }
        if self.await_detection {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NoteByCommitmentRequest", len)?;
        if let Some(v) = self.note_commitment.as_ref() {
            struct_ser.serialize_field("noteCommitment", v)?;
        }
        if self.await_detection {
            struct_ser.serialize_field("awaitDetection", &self.await_detection)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteByCommitmentRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_commitment",
            "noteCommitment",
            "await_detection",
            "awaitDetection",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteCommitment,
            AwaitDetection,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "noteCommitment" | "note_commitment" => Ok(GeneratedField::NoteCommitment),
                            "awaitDetection" | "await_detection" => Ok(GeneratedField::AwaitDetection),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteByCommitmentRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NoteByCommitmentRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteByCommitmentRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_commitment__ = None;
                let mut await_detection__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteCommitment => {
                            if note_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteCommitment"));
                            }
                            note_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::AwaitDetection => {
                            if await_detection__.is_some() {
                                return Err(serde::de::Error::duplicate_field("awaitDetection"));
                            }
                            await_detection__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteByCommitmentRequest {
                    note_commitment: note_commitment__,
                    await_detection: await_detection__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NoteByCommitmentRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteByCommitmentResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.spendable_note.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NoteByCommitmentResponse", len)?;
        if let Some(v) = self.spendable_note.as_ref() {
            struct_ser.serialize_field("spendableNote", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteByCommitmentResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "spendable_note",
            "spendableNote",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SpendableNote,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "spendableNote" | "spendable_note" => Ok(GeneratedField::SpendableNote),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteByCommitmentResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NoteByCommitmentResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteByCommitmentResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spendable_note__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SpendableNote => {
                            if spendable_note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spendableNote"));
                            }
                            spendable_note__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteByCommitmentResponse {
                    spendable_note: spendable_note__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NoteByCommitmentResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NotesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.include_spent {
            len += 1;
        }
        if self.asset_id.is_some() {
            len += 1;
        }
        if self.address_index.is_some() {
            len += 1;
        }
        if self.amount_to_spend.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NotesRequest", len)?;
        if self.include_spent {
            struct_ser.serialize_field("includeSpent", &self.include_spent)?;
        }
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if let Some(v) = self.address_index.as_ref() {
            struct_ser.serialize_field("addressIndex", v)?;
        }
        if let Some(v) = self.amount_to_spend.as_ref() {
            struct_ser.serialize_field("amountToSpend", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NotesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "include_spent",
            "includeSpent",
            "asset_id",
            "assetId",
            "address_index",
            "addressIndex",
            "amount_to_spend",
            "amountToSpend",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IncludeSpent,
            AssetId,
            AddressIndex,
            AmountToSpend,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "includeSpent" | "include_spent" => Ok(GeneratedField::IncludeSpent),
                            "assetId" | "asset_id" => Ok(GeneratedField::AssetId),
                            "addressIndex" | "address_index" => Ok(GeneratedField::AddressIndex),
                            "amountToSpend" | "amount_to_spend" => Ok(GeneratedField::AmountToSpend),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NotesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NotesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NotesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut include_spent__ = None;
                let mut asset_id__ = None;
                let mut address_index__ = None;
                let mut amount_to_spend__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IncludeSpent => {
                            if include_spent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("includeSpent"));
                            }
                            include_spent__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::AddressIndex => {
                            if address_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressIndex"));
                            }
                            address_index__ = map_.next_value()?;
                        }
                        GeneratedField::AmountToSpend => {
                            if amount_to_spend__.is_some() {
                                return Err(serde::de::Error::duplicate_field("amountToSpend"));
                            }
                            amount_to_spend__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NotesRequest {
                    include_spent: include_spent__.unwrap_or_default(),
                    asset_id: asset_id__,
                    address_index: address_index__,
                    amount_to_spend: amount_to_spend__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NotesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NotesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_record.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NotesResponse", len)?;
        if let Some(v) = self.note_record.as_ref() {
            struct_ser.serialize_field("noteRecord", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NotesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_record",
            "noteRecord",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteRecord,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "noteRecord" | "note_record" => Ok(GeneratedField::NoteRecord),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NotesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NotesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NotesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_record__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteRecord => {
                            if note_record__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteRecord"));
                            }
                            note_record__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NotesResponse {
                    note_record: note_record__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NotesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierStatusRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.nullifier.is_some() {
            len += 1;
        }
        if self.await_detection {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NullifierStatusRequest", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if self.await_detection {
            struct_ser.serialize_field("awaitDetection", &self.await_detection)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierStatusRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nullifier",
            "await_detection",
            "awaitDetection",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            AwaitDetection,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "nullifier" => Ok(GeneratedField::Nullifier),
                            "awaitDetection" | "await_detection" => Ok(GeneratedField::AwaitDetection),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierStatusRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NullifierStatusRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierStatusRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut await_detection__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::AwaitDetection => {
                            if await_detection__.is_some() {
                                return Err(serde::de::Error::duplicate_field("awaitDetection"));
                            }
                            await_detection__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierStatusRequest {
                    nullifier: nullifier__,
                    await_detection: await_detection__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NullifierStatusRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierStatusResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.spent {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NullifierStatusResponse", len)?;
        if self.spent {
            struct_ser.serialize_field("spent", &self.spent)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierStatusResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "spent",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Spent,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "spent" => Ok(GeneratedField::Spent),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierStatusResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NullifierStatusResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierStatusResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spent__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Spent => {
                            if spent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spent"));
                            }
                            spent__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierStatusResponse {
                    spent: spent__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NullifierStatusResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierWindowRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.NullifierWindowRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierWindowRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierWindowRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NullifierWindowRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierWindowRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(NullifierWindowRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NullifierWindowRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierWindowResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.window.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.NullifierWindowResponse", len)?;
        if let Some(v) = self.window.as_ref() {
            struct_ser.serialize_field("window", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierWindowResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "window",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Window,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "window" => Ok(GeneratedField::Window),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierWindowResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.NullifierWindowResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierWindowResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut window__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Window => {
                            if window__.is_some() {
                                return Err(serde::de::Error::duplicate_field("window"));
                            }
                            window__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierWindowResponse {
                    window: window__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.NullifierWindowResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpendableNoteRecord {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_commitment.is_some() {
            len += 1;
        }
        if self.note.is_some() {
            len += 1;
        }
        if self.address_index.is_some() {
            len += 1;
        }
        if self.nullifier.is_some() {
            len += 1;
        }
        if self.height_created != 0 {
            len += 1;
        }
        if self.height_spent != 0 {
            len += 1;
        }
        if self.position != 0 {
            len += 1;
        }
        if self.source.is_some() {
            len += 1;
        }
        if self.return_address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.SpendableNoteRecord", len)?;
        if let Some(v) = self.note_commitment.as_ref() {
            struct_ser.serialize_field("noteCommitment", v)?;
        }
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        if let Some(v) = self.address_index.as_ref() {
            struct_ser.serialize_field("addressIndex", v)?;
        }
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if self.height_created != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("heightCreated", ToString::to_string(&self.height_created).as_str())?;
        }
        if self.height_spent != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("heightSpent", ToString::to_string(&self.height_spent).as_str())?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if let Some(v) = self.source.as_ref() {
            struct_ser.serialize_field("source", v)?;
        }
        if let Some(v) = self.return_address.as_ref() {
            struct_ser.serialize_field("returnAddress", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpendableNoteRecord {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_commitment",
            "noteCommitment",
            "note",
            "address_index",
            "addressIndex",
            "nullifier",
            "height_created",
            "heightCreated",
            "height_spent",
            "heightSpent",
            "position",
            "source",
            "return_address",
            "returnAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteCommitment,
            Note,
            AddressIndex,
            Nullifier,
            HeightCreated,
            HeightSpent,
            Position,
            Source,
            ReturnAddress,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "noteCommitment" | "note_commitment" => Ok(GeneratedField::NoteCommitment),
                            "note" => Ok(GeneratedField::Note),
                            "addressIndex" | "address_index" => Ok(GeneratedField::AddressIndex),
                            "nullifier" => Ok(GeneratedField::Nullifier),
                            "heightCreated" | "height_created" => Ok(GeneratedField::HeightCreated),
                            "heightSpent" | "height_spent" => Ok(GeneratedField::HeightSpent),
                            "position" => Ok(GeneratedField::Position),
                            "source" => Ok(GeneratedField::Source),
                            "returnAddress" | "return_address" => Ok(GeneratedField::ReturnAddress),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpendableNoteRecord;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.SpendableNoteRecord")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SpendableNoteRecord, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_commitment__ = None;
                let mut note__ = None;
                let mut address_index__ = None;
                let mut nullifier__ = None;
                let mut height_created__ = None;
                let mut height_spent__ = None;
                let mut position__ = None;
                let mut source__ = None;
                let mut return_address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteCommitment => {
                            if note_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteCommitment"));
                            }
                            note_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::Note => {
                            if note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            note__ = map_.next_value()?;
                        }
                        GeneratedField::AddressIndex => {
                            if address_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressIndex"));
                            }
                            address_index__ = map_.next_value()?;
                        }
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::HeightCreated => {
                            if height_created__.is_some() {
                                return Err(serde::de::Error::duplicate_field("heightCreated"));
                            }
                            height_created__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeightSpent => {
                            if height_spent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("heightSpent"));
                            }
                            height_spent__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = map_.next_value()?;
                        }
                        GeneratedField::ReturnAddress => {
                            if return_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("returnAddress"));
                            }
                            return_address__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SpendableNoteRecord {
                    note_commitment: note_commitment__,
                    note: note__,
                    address_index: address_index__,
                    nullifier: nullifier__,
                    height_created: height_created__.unwrap_or_default(),
                    height_spent: height_spent__.unwrap_or_default(),
                    position: position__.unwrap_or_default(),
                    source: source__,
                    return_address: return_address__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.SpendableNoteRecord", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StatusRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.StatusRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StatusRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StatusRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.StatusRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<StatusRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(StatusRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.StatusRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StatusResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.sync_height != 0 {
            len += 1;
        }
        if self.catching_up {
            len += 1;
        }
        if self.latest_block_timestamp != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.StatusResponse", len)?;
        if self.sync_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("syncHeight", ToString::to_string(&self.sync_height).as_str())?;
        }
        if self.catching_up {
            struct_ser.serialize_field("catchingUp", &self.catching_up)?;
        }
        if self.latest_block_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("latestBlockTimestamp", ToString::to_string(&self.latest_block_timestamp).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StatusResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sync_height",
            "syncHeight",
            "catching_up",
            "catchingUp",
            "latest_block_timestamp",
            "latestBlockTimestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SyncHeight,
            CatchingUp,
            LatestBlockTimestamp,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "syncHeight" | "sync_height" => Ok(GeneratedField::SyncHeight),
                            "catchingUp" | "catching_up" => Ok(GeneratedField::CatchingUp),
                            "latestBlockTimestamp" | "latest_block_timestamp" => Ok(GeneratedField::LatestBlockTimestamp),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StatusResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.StatusResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<StatusResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sync_height__ = None;
                let mut catching_up__ = None;
                let mut latest_block_timestamp__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SyncHeight => {
                            if sync_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncHeight"));
                            }
                            sync_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CatchingUp => {
                            if catching_up__.is_some() {
                                return Err(serde::de::Error::duplicate_field("catchingUp"));
                            }
                            catching_up__ = Some(map_.next_value()?);
                        }
                        GeneratedField::LatestBlockTimestamp => {
                            if latest_block_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("latestBlockTimestamp"));
                            }
                            latest_block_timestamp__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(StatusResponse {
                    sync_height: sync_height__.unwrap_or_default(),
                    catching_up: catching_up__.unwrap_or_default(),
                    latest_block_timestamp: latest_block_timestamp__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.StatusResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StatusStreamRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.StatusStreamRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StatusStreamRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StatusStreamRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.StatusStreamRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<StatusStreamRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(StatusStreamRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.StatusStreamRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StatusStreamResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.latest_known_block_height != 0 {
            len += 1;
        }
        if self.sync_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.StatusStreamResponse", len)?;
        if self.latest_known_block_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("latestKnownBlockHeight", ToString::to_string(&self.latest_known_block_height).as_str())?;
        }
        if self.sync_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("syncHeight", ToString::to_string(&self.sync_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StatusStreamResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "latest_known_block_height",
            "latestKnownBlockHeight",
            "sync_height",
            "syncHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LatestKnownBlockHeight,
            SyncHeight,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "latestKnownBlockHeight" | "latest_known_block_height" => Ok(GeneratedField::LatestKnownBlockHeight),
                            "syncHeight" | "sync_height" => Ok(GeneratedField::SyncHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StatusStreamResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.StatusStreamResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<StatusStreamResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut latest_known_block_height__ = None;
                let mut sync_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LatestKnownBlockHeight => {
                            if latest_known_block_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("latestKnownBlockHeight"));
                            }
                            latest_known_block_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SyncHeight => {
                            if sync_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncHeight"));
                            }
                            sync_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(StatusStreamResponse {
                    latest_known_block_height: latest_known_block_height__.unwrap_or_default(),
                    sync_height: sync_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.StatusStreamResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.height != 0 {
            len += 1;
        }
        if self.id.is_some() {
            len += 1;
        }
        if self.transaction.is_some() {
            len += 1;
        }
        if self.perspective.is_some() {
            len += 1;
        }
        if self.view.is_some() {
            len += 1;
        }
        if self.summary.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionInfo", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.id.as_ref() {
            struct_ser.serialize_field("id", v)?;
        }
        if let Some(v) = self.transaction.as_ref() {
            struct_ser.serialize_field("transaction", v)?;
        }
        if let Some(v) = self.perspective.as_ref() {
            struct_ser.serialize_field("perspective", v)?;
        }
        if let Some(v) = self.view.as_ref() {
            struct_ser.serialize_field("view", v)?;
        }
        if let Some(v) = self.summary.as_ref() {
            struct_ser.serialize_field("summary", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
            "id",
            "transaction",
            "perspective",
            "view",
            "summary",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
            Id,
            Transaction,
            Perspective,
            View,
            Summary,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "height" => Ok(GeneratedField::Height),
                            "id" => Ok(GeneratedField::Id),
                            "transaction" => Ok(GeneratedField::Transaction),
                            "perspective" => Ok(GeneratedField::Perspective),
                            "view" => Ok(GeneratedField::View),
                            "summary" => Ok(GeneratedField::Summary),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
                let mut id__ = None;
                let mut transaction__ = None;
                let mut perspective__ = None;
                let mut view__ = None;
                let mut summary__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = map_.next_value()?;
                        }
                        GeneratedField::Transaction => {
                            if transaction__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transaction"));
                            }
                            transaction__ = map_.next_value()?;
                        }
                        GeneratedField::Perspective => {
                            if perspective__.is_some() {
                                return Err(serde::de::Error::duplicate_field("perspective"));
                            }
                            perspective__ = map_.next_value()?;
                        }
                        GeneratedField::View => {
                            if view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("view"));
                            }
                            view__ = map_.next_value()?;
                        }
                        GeneratedField::Summary => {
                            if summary__.is_some() {
                                return Err(serde::de::Error::duplicate_field("summary"));
                            }
                            summary__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionInfo {
                    height: height__.unwrap_or_default(),
                    id: id__,
                    transaction: transaction__,
                    perspective: perspective__,
                    view: view__,
                    summary: summary__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionInfoByHashRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionInfoByHashRequest", len)?;
        if let Some(v) = self.id.as_ref() {
            struct_ser.serialize_field("id", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionInfoByHashRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionInfoByHashRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionInfoByHashRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionInfoByHashRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionInfoByHashRequest {
                    id: id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionInfoByHashRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionInfoByHashResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.tx_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionInfoByHashResponse", len)?;
        if let Some(v) = self.tx_info.as_ref() {
            struct_ser.serialize_field("txInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionInfoByHashResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "tx_info",
            "txInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TxInfo,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "txInfo" | "tx_info" => Ok(GeneratedField::TxInfo),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionInfoByHashResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionInfoByHashResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionInfoByHashResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut tx_info__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TxInfo => {
                            if tx_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("txInfo"));
                            }
                            tx_info__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionInfoByHashResponse {
                    tx_info: tx_info__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionInfoByHashResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionInfoRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.start_height != 0 {
            len += 1;
        }
        if self.end_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionInfoRequest", len)?;
        if self.start_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startHeight", ToString::to_string(&self.start_height).as_str())?;
        }
        if self.end_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("endHeight", ToString::to_string(&self.end_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionInfoRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "start_height",
            "startHeight",
            "end_height",
            "endHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartHeight,
            EndHeight,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "startHeight" | "start_height" => Ok(GeneratedField::StartHeight),
                            "endHeight" | "end_height" => Ok(GeneratedField::EndHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionInfoRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionInfoRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionInfoRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut start_height__ = None;
                let mut end_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::StartHeight => {
                            if start_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startHeight"));
                            }
                            start_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndHeight => {
                            if end_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endHeight"));
                            }
                            end_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionInfoRequest {
                    start_height: start_height__.unwrap_or_default(),
                    end_height: end_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionInfoRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionInfoResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.tx_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionInfoResponse", len)?;
        if let Some(v) = self.tx_info.as_ref() {
            struct_ser.serialize_field("txInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionInfoResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "tx_info",
            "txInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TxInfo,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "txInfo" | "tx_info" => Ok(GeneratedField::TxInfo),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionInfoResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionInfoResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionInfoResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut tx_info__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TxInfo => {
                            if tx_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("txInfo"));
                            }
                            tx_info__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionInfoResponse {
                    tx_info: tx_info__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionInfoResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionPlannerRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.expiry_height != 0 {
            len += 1;
        }
        if self.memo.is_some() {
            len += 1;
        }
        if self.source.is_some() {
            len += 1;
        }
        if !self.outputs.is_empty() {
            len += 1;
        }
        if !self.ibc_relay_actions.is_empty() {
            len += 1;
        }
        if !self.ics20_withdrawals.is_empty() {
            len += 1;
        }
        if !self.host_withdrawals.is_empty() {
            len += 1;
        }
        if self.epoch_index != 0 {
            len += 1;
        }
        if self.epoch.is_some() {
            len += 1;
        }
        if self.disclose_to_issuer {
            len += 1;
        }
        if self.fee_mode.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionPlannerRequest", len)?;
        if self.expiry_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("expiryHeight", ToString::to_string(&self.expiry_height).as_str())?;
        }
        if let Some(v) = self.memo.as_ref() {
            struct_ser.serialize_field("memo", v)?;
        }
        if let Some(v) = self.source.as_ref() {
            struct_ser.serialize_field("source", v)?;
        }
        if !self.outputs.is_empty() {
            struct_ser.serialize_field("outputs", &self.outputs)?;
        }
        if !self.ibc_relay_actions.is_empty() {
            struct_ser.serialize_field("ibcRelayActions", &self.ibc_relay_actions)?;
        }
        if !self.ics20_withdrawals.is_empty() {
            struct_ser.serialize_field("ics20Withdrawals", &self.ics20_withdrawals)?;
        }
        if !self.host_withdrawals.is_empty() {
            struct_ser.serialize_field("hostWithdrawals", &self.host_withdrawals)?;
        }
        if self.epoch_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("epochIndex", ToString::to_string(&self.epoch_index).as_str())?;
        }
        if let Some(v) = self.epoch.as_ref() {
            struct_ser.serialize_field("epoch", v)?;
        }
        if self.disclose_to_issuer {
            struct_ser.serialize_field("discloseToIssuer", &self.disclose_to_issuer)?;
        }
        if let Some(v) = self.fee_mode.as_ref() {
            match v {
                transaction_planner_request::FeeMode::AutoFee(v) => {
                    struct_ser.serialize_field("autoFee", v)?;
                }
                transaction_planner_request::FeeMode::ManualFee(v) => {
                    struct_ser.serialize_field("manualFee", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionPlannerRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "expiry_height",
            "expiryHeight",
            "memo",
            "source",
            "outputs",
            "ibc_relay_actions",
            "ibcRelayActions",
            "ics20_withdrawals",
            "ics20Withdrawals",
            "host_withdrawals",
            "hostWithdrawals",
            "epoch_index",
            "epochIndex",
            "epoch",
            "disclose_to_issuer",
            "discloseToIssuer",
            "auto_fee",
            "autoFee",
            "manual_fee",
            "manualFee",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ExpiryHeight,
            Memo,
            Source,
            Outputs,
            IbcRelayActions,
            Ics20Withdrawals,
            HostWithdrawals,
            EpochIndex,
            Epoch,
            DiscloseToIssuer,
            AutoFee,
            ManualFee,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "expiryHeight" | "expiry_height" => Ok(GeneratedField::ExpiryHeight),
                            "memo" => Ok(GeneratedField::Memo),
                            "source" => Ok(GeneratedField::Source),
                            "outputs" => Ok(GeneratedField::Outputs),
                            "ibcRelayActions" | "ibc_relay_actions" => Ok(GeneratedField::IbcRelayActions),
                            "ics20Withdrawals" | "ics20_withdrawals" => Ok(GeneratedField::Ics20Withdrawals),
                            "hostWithdrawals" | "host_withdrawals" => Ok(GeneratedField::HostWithdrawals),
                            "epochIndex" | "epoch_index" => Ok(GeneratedField::EpochIndex),
                            "epoch" => Ok(GeneratedField::Epoch),
                            "discloseToIssuer" | "disclose_to_issuer" => Ok(GeneratedField::DiscloseToIssuer),
                            "autoFee" | "auto_fee" => Ok(GeneratedField::AutoFee),
                            "manualFee" | "manual_fee" => Ok(GeneratedField::ManualFee),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionPlannerRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionPlannerRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionPlannerRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut expiry_height__ = None;
                let mut memo__ = None;
                let mut source__ = None;
                let mut outputs__ = None;
                let mut ibc_relay_actions__ = None;
                let mut ics20_withdrawals__ = None;
                let mut host_withdrawals__ = None;
                let mut epoch_index__ = None;
                let mut epoch__ = None;
                let mut disclose_to_issuer__ = None;
                let mut fee_mode__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ExpiryHeight => {
                            if expiry_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("expiryHeight"));
                            }
                            expiry_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Memo => {
                            if memo__.is_some() {
                                return Err(serde::de::Error::duplicate_field("memo"));
                            }
                            memo__ = map_.next_value()?;
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = map_.next_value()?;
                        }
                        GeneratedField::Outputs => {
                            if outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputs"));
                            }
                            outputs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IbcRelayActions => {
                            if ibc_relay_actions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcRelayActions"));
                            }
                            ibc_relay_actions__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Ics20Withdrawals => {
                            if ics20_withdrawals__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ics20Withdrawals"));
                            }
                            ics20_withdrawals__ = Some(map_.next_value()?);
                        }
                        GeneratedField::HostWithdrawals => {
                            if host_withdrawals__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hostWithdrawals"));
                            }
                            host_withdrawals__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EpochIndex => {
                            if epoch_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epochIndex"));
                            }
                            epoch_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Epoch => {
                            if epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epoch"));
                            }
                            epoch__ = map_.next_value()?;
                        }
                        GeneratedField::DiscloseToIssuer => {
                            if disclose_to_issuer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("discloseToIssuer"));
                            }
                            disclose_to_issuer__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AutoFee => {
                            if fee_mode__.is_some() {
                                return Err(serde::de::Error::duplicate_field("autoFee"));
                            }
                            fee_mode__ = map_.next_value::<::std::option::Option<_>>()?.map(transaction_planner_request::FeeMode::AutoFee)
;
                        }
                        GeneratedField::ManualFee => {
                            if fee_mode__.is_some() {
                                return Err(serde::de::Error::duplicate_field("manualFee"));
                            }
                            fee_mode__ = map_.next_value::<::std::option::Option<_>>()?.map(transaction_planner_request::FeeMode::ManualFee)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionPlannerRequest {
                    expiry_height: expiry_height__.unwrap_or_default(),
                    memo: memo__,
                    source: source__,
                    outputs: outputs__.unwrap_or_default(),
                    ibc_relay_actions: ibc_relay_actions__.unwrap_or_default(),
                    ics20_withdrawals: ics20_withdrawals__.unwrap_or_default(),
                    host_withdrawals: host_withdrawals__.unwrap_or_default(),
                    epoch_index: epoch_index__.unwrap_or_default(),
                    epoch: epoch__,
                    disclose_to_issuer: disclose_to_issuer__.unwrap_or_default(),
                    fee_mode: fee_mode__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionPlannerRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for transaction_planner_request::TransferOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.value.is_some() {
            len += 1;
        }
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionPlannerRequest.TransferOutput", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for transaction_planner_request::TransferOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Address,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "value" => Ok(GeneratedField::Value),
                            "address" => Ok(GeneratedField::Address),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = transaction_planner_request::TransferOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionPlannerRequest.TransferOutput")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<transaction_planner_request::TransferOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(transaction_planner_request::TransferOutput {
                    value: value__,
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionPlannerRequest.TransferOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionPlannerResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.plan.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransactionPlannerResponse", len)?;
        if let Some(v) = self.plan.as_ref() {
            struct_ser.serialize_field("plan", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionPlannerResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "plan",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Plan,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "plan" => Ok(GeneratedField::Plan),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionPlannerResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransactionPlannerResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionPlannerResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut plan__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Plan => {
                            if plan__.is_some() {
                                return Err(serde::de::Error::duplicate_field("plan"));
                            }
                            plan__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionPlannerResponse {
                    plan: plan__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransactionPlannerResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransparentAddressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.TransparentAddressRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransparentAddressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransparentAddressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransparentAddressRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransparentAddressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(TransparentAddressRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransparentAddressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransparentAddressResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address.is_some() {
            len += 1;
        }
        if !self.encoding.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.TransparentAddressResponse", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        if !self.encoding.is_empty() {
            struct_ser.serialize_field("encoding", &self.encoding)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransparentAddressResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "encoding",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            Encoding,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address" => Ok(GeneratedField::Address),
                            "encoding" => Ok(GeneratedField::Encoding),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransparentAddressResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.TransparentAddressResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransparentAddressResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut encoding__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = map_.next_value()?;
                        }
                        GeneratedField::Encoding => {
                            if encoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encoding"));
                            }
                            encoding__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransparentAddressResponse {
                    address: address__,
                    encoding: encoding__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.TransparentAddressResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VolumeAccumulatorRecoveryRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.subject.is_empty() {
            len += 1;
        }
        if self.day_start != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryRequest", len)?;
        if !self.subject.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("subject", pbjson::private::base64::encode(&self.subject).as_str())?;
        }
        if self.day_start != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dayStart", ToString::to_string(&self.day_start).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VolumeAccumulatorRecoveryRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "subject",
            "day_start",
            "dayStart",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Subject,
            DayStart,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "subject" => Ok(GeneratedField::Subject),
                            "dayStart" | "day_start" => Ok(GeneratedField::DayStart),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VolumeAccumulatorRecoveryRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.VolumeAccumulatorRecoveryRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<VolumeAccumulatorRecoveryRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut subject__ = None;
                let mut day_start__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Subject => {
                            if subject__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subject"));
                            }
                            subject__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DayStart => {
                            if day_start__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dayStart"));
                            }
                            day_start__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(VolumeAccumulatorRecoveryRequest {
                    subject: subject__.unwrap_or_default(),
                    day_start: day_start__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VolumeAccumulatorRecoveryResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.outcome.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse", len)?;
        if let Some(v) = self.outcome.as_ref() {
            match v {
                volume_accumulator_recovery_response::Outcome::Absent(v) => {
                    struct_ser.serialize_field("absent", v)?;
                }
                volume_accumulator_recovery_response::Outcome::Complete(v) => {
                    struct_ser.serialize_field("complete", v)?;
                }
                volume_accumulator_recovery_response::Outcome::Incomplete(v) => {
                    struct_ser.serialize_field("incomplete", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VolumeAccumulatorRecoveryResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "absent",
            "complete",
            "incomplete",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Absent,
            Complete,
            Incomplete,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "absent" => Ok(GeneratedField::Absent),
                            "complete" => Ok(GeneratedField::Complete),
                            "incomplete" => Ok(GeneratedField::Incomplete),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VolumeAccumulatorRecoveryResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.VolumeAccumulatorRecoveryResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<VolumeAccumulatorRecoveryResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut outcome__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Absent => {
                            if outcome__.is_some() {
                                return Err(serde::de::Error::duplicate_field("absent"));
                            }
                            outcome__ = map_.next_value::<::std::option::Option<_>>()?.map(volume_accumulator_recovery_response::Outcome::Absent)
;
                        }
                        GeneratedField::Complete => {
                            if outcome__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complete"));
                            }
                            outcome__ = map_.next_value::<::std::option::Option<_>>()?.map(volume_accumulator_recovery_response::Outcome::Complete)
;
                        }
                        GeneratedField::Incomplete => {
                            if outcome__.is_some() {
                                return Err(serde::de::Error::duplicate_field("incomplete"));
                            }
                            outcome__ = map_.next_value::<::std::option::Option<_>>()?.map(volume_accumulator_recovery_response::Outcome::Incomplete)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(VolumeAccumulatorRecoveryResponse {
                    outcome: outcome__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for volume_accumulator_recovery_response::Absent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Absent", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for volume_accumulator_recovery_response::Absent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = volume_accumulator_recovery_response::Absent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Absent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<volume_accumulator_recovery_response::Absent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(volume_accumulator_recovery_response::Absent {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Absent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for volume_accumulator_recovery_response::Complete {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.subject.is_empty() {
            len += 1;
        }
        if self.day_start != 0 {
            len += 1;
        }
        if !self.undisclosed_volume.is_empty() {
            len += 1;
        }
        if !self.blinding.is_empty() {
            len += 1;
        }
        if self.commitment.is_some() {
            len += 1;
        }
        if self.position != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Complete", len)?;
        if !self.subject.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("subject", pbjson::private::base64::encode(&self.subject).as_str())?;
        }
        if self.day_start != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dayStart", ToString::to_string(&self.day_start).as_str())?;
        }
        if !self.undisclosed_volume.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("undisclosedVolume", pbjson::private::base64::encode(&self.undisclosed_volume).as_str())?;
        }
        if !self.blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blinding", pbjson::private::base64::encode(&self.blinding).as_str())?;
        }
        if let Some(v) = self.commitment.as_ref() {
            struct_ser.serialize_field("commitment", v)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for volume_accumulator_recovery_response::Complete {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "subject",
            "day_start",
            "dayStart",
            "undisclosed_volume",
            "undisclosedVolume",
            "blinding",
            "commitment",
            "position",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Subject,
            DayStart,
            UndisclosedVolume,
            Blinding,
            Commitment,
            Position,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "subject" => Ok(GeneratedField::Subject),
                            "dayStart" | "day_start" => Ok(GeneratedField::DayStart),
                            "undisclosedVolume" | "undisclosed_volume" => Ok(GeneratedField::UndisclosedVolume),
                            "blinding" => Ok(GeneratedField::Blinding),
                            "commitment" => Ok(GeneratedField::Commitment),
                            "position" => Ok(GeneratedField::Position),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = volume_accumulator_recovery_response::Complete;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Complete")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<volume_accumulator_recovery_response::Complete, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut subject__ = None;
                let mut day_start__ = None;
                let mut undisclosed_volume__ = None;
                let mut blinding__ = None;
                let mut commitment__ = None;
                let mut position__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Subject => {
                            if subject__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subject"));
                            }
                            subject__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DayStart => {
                            if day_start__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dayStart"));
                            }
                            day_start__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UndisclosedVolume => {
                            if undisclosed_volume__.is_some() {
                                return Err(serde::de::Error::duplicate_field("undisclosedVolume"));
                            }
                            undisclosed_volume__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Blinding => {
                            if blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blinding"));
                            }
                            blinding__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Commitment => {
                            if commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitment"));
                            }
                            commitment__ = map_.next_value()?;
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(volume_accumulator_recovery_response::Complete {
                    subject: subject__.unwrap_or_default(),
                    day_start: day_start__.unwrap_or_default(),
                    undisclosed_volume: undisclosed_volume__.unwrap_or_default(),
                    blinding: blinding__.unwrap_or_default(),
                    commitment: commitment__,
                    position: position__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Complete", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for volume_accumulator_recovery_response::Incomplete {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Incomplete", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for volume_accumulator_recovery_response::Incomplete {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = volume_accumulator_recovery_response::Incomplete;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Incomplete")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<volume_accumulator_recovery_response::Incomplete, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(volume_accumulator_recovery_response::Incomplete {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.VolumeAccumulatorRecoveryResponse.Incomplete", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WalletIdRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.view.v1.WalletIdRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WalletIdRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WalletIdRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WalletIdRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WalletIdRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(WalletIdRequest {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WalletIdRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WalletIdResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.wallet_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WalletIdResponse", len)?;
        if let Some(v) = self.wallet_id.as_ref() {
            struct_ser.serialize_field("walletId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WalletIdResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "wallet_id",
            "walletId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WalletId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "walletId" | "wallet_id" => Ok(GeneratedField::WalletId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WalletIdResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WalletIdResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WalletIdResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut wallet_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WalletId => {
                            if wallet_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletId"));
                            }
                            wallet_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WalletIdResponse {
                    wallet_id: wallet_id__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WalletIdResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WitnessAndBuildRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction_plan.is_some() {
            len += 1;
        }
        if self.authorization_data.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessAndBuildRequest", len)?;
        if let Some(v) = self.transaction_plan.as_ref() {
            struct_ser.serialize_field("transactionPlan", v)?;
        }
        if let Some(v) = self.authorization_data.as_ref() {
            struct_ser.serialize_field("authorizationData", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WitnessAndBuildRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction_plan",
            "transactionPlan",
            "authorization_data",
            "authorizationData",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TransactionPlan,
            AuthorizationData,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transactionPlan" | "transaction_plan" => Ok(GeneratedField::TransactionPlan),
                            "authorizationData" | "authorization_data" => Ok(GeneratedField::AuthorizationData),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WitnessAndBuildRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessAndBuildRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WitnessAndBuildRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction_plan__ = None;
                let mut authorization_data__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TransactionPlan => {
                            if transaction_plan__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionPlan"));
                            }
                            transaction_plan__ = map_.next_value()?;
                        }
                        GeneratedField::AuthorizationData => {
                            if authorization_data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authorizationData"));
                            }
                            authorization_data__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WitnessAndBuildRequest {
                    transaction_plan: transaction_plan__,
                    authorization_data: authorization_data__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessAndBuildRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WitnessAndBuildResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.status.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessAndBuildResponse", len)?;
        if let Some(v) = self.status.as_ref() {
            match v {
                witness_and_build_response::Status::BuildProgress(v) => {
                    struct_ser.serialize_field("buildProgress", v)?;
                }
                witness_and_build_response::Status::Complete(v) => {
                    struct_ser.serialize_field("complete", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WitnessAndBuildResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "build_progress",
            "buildProgress",
            "complete",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BuildProgress,
            Complete,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "buildProgress" | "build_progress" => Ok(GeneratedField::BuildProgress),
                            "complete" => Ok(GeneratedField::Complete),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WitnessAndBuildResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessAndBuildResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WitnessAndBuildResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut status__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BuildProgress => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("buildProgress"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(witness_and_build_response::Status::BuildProgress)
;
                        }
                        GeneratedField::Complete => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complete"));
                            }
                            status__ = map_.next_value::<::std::option::Option<_>>()?.map(witness_and_build_response::Status::Complete)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WitnessAndBuildResponse {
                    status: status__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessAndBuildResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for witness_and_build_response::BuildProgress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.progress != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessAndBuildResponse.BuildProgress", len)?;
        if self.progress != 0. {
            struct_ser.serialize_field("progress", &self.progress)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for witness_and_build_response::BuildProgress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "progress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Progress,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "progress" => Ok(GeneratedField::Progress),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = witness_and_build_response::BuildProgress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessAndBuildResponse.BuildProgress")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<witness_and_build_response::BuildProgress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut progress__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Progress => {
                            if progress__.is_some() {
                                return Err(serde::de::Error::duplicate_field("progress"));
                            }
                            progress__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(witness_and_build_response::BuildProgress {
                    progress: progress__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessAndBuildResponse.BuildProgress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for witness_and_build_response::Complete {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessAndBuildResponse.Complete", len)?;
        if let Some(v) = self.transaction.as_ref() {
            struct_ser.serialize_field("transaction", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for witness_and_build_response::Complete {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transaction,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transaction" => Ok(GeneratedField::Transaction),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = witness_and_build_response::Complete;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessAndBuildResponse.Complete")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<witness_and_build_response::Complete, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transaction => {
                            if transaction__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transaction"));
                            }
                            transaction__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(witness_and_build_response::Complete {
                    transaction: transaction__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessAndBuildResponse.Complete", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WitnessRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction_plan.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessRequest", len)?;
        if let Some(v) = self.transaction_plan.as_ref() {
            struct_ser.serialize_field("transactionPlan", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WitnessRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction_plan",
            "transactionPlan",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TransactionPlan,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transactionPlan" | "transaction_plan" => Ok(GeneratedField::TransactionPlan),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WitnessRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WitnessRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction_plan__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TransactionPlan => {
                            if transaction_plan__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionPlan"));
                            }
                            transaction_plan__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WitnessRequest {
                    transaction_plan: transaction_plan__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WitnessResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.witness_data.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.view.v1.WitnessResponse", len)?;
        if let Some(v) = self.witness_data.as_ref() {
            struct_ser.serialize_field("witnessData", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WitnessResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "witness_data",
            "witnessData",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WitnessData,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "witnessData" | "witness_data" => Ok(GeneratedField::WitnessData),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WitnessResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.view.v1.WitnessResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WitnessResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut witness_data__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WitnessData => {
                            if witness_data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("witnessData"));
                            }
                            witness_data__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WitnessResponse {
                    witness_data: witness_data__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.view.v1.WitnessResponse", FIELDS, GeneratedVisitor)
    }
}
