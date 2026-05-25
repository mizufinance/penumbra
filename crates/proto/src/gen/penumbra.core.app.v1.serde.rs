impl serde::Serialize for AppParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.chain_id.is_empty() {
            len += 1;
        }
        if self.sct_params.is_some() {
            len += 1;
        }
        if self.governance_params.is_some() {
            len += 1;
        }
        if self.ibc_params.is_some() {
            len += 1;
        }
        if self.validator_params.is_some() {
            len += 1;
        }
        if self.fee_params.is_some() {
            len += 1;
        }
        if self.shielded_pool_params.is_some() {
            len += 1;
        }
        if self.compliance_params.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.AppParameters", len)?;
        if !self.chain_id.is_empty() {
            struct_ser.serialize_field("chainId", &self.chain_id)?;
        }
        if let Some(v) = self.sct_params.as_ref() {
            struct_ser.serialize_field("sctParams", v)?;
        }
        if let Some(v) = self.governance_params.as_ref() {
            struct_ser.serialize_field("governanceParams", v)?;
        }
        if let Some(v) = self.ibc_params.as_ref() {
            struct_ser.serialize_field("ibcParams", v)?;
        }
        if let Some(v) = self.validator_params.as_ref() {
            struct_ser.serialize_field("validatorParams", v)?;
        }
        if let Some(v) = self.fee_params.as_ref() {
            struct_ser.serialize_field("feeParams", v)?;
        }
        if let Some(v) = self.shielded_pool_params.as_ref() {
            struct_ser.serialize_field("shieldedPoolParams", v)?;
        }
        if let Some(v) = self.compliance_params.as_ref() {
            struct_ser.serialize_field("complianceParams", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AppParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "chain_id",
            "chainId",
            "sct_params",
            "sctParams",
            "governance_params",
            "governanceParams",
            "ibc_params",
            "ibcParams",
            "validator_params",
            "validatorParams",
            "fee_params",
            "feeParams",
            "shielded_pool_params",
            "shieldedPoolParams",
            "compliance_params",
            "complianceParams",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ChainId,
            SctParams,
            GovernanceParams,
            IbcParams,
            ValidatorParams,
            FeeParams,
            ShieldedPoolParams,
            ComplianceParams,
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
                            "chainId" | "chain_id" => Ok(GeneratedField::ChainId),
                            "sctParams" | "sct_params" => Ok(GeneratedField::SctParams),
                            "governanceParams" | "governance_params" => Ok(GeneratedField::GovernanceParams),
                            "ibcParams" | "ibc_params" => Ok(GeneratedField::IbcParams),
                            "validatorParams" | "validator_params" => Ok(GeneratedField::ValidatorParams),
                            "feeParams" | "fee_params" => Ok(GeneratedField::FeeParams),
                            "shieldedPoolParams" | "shielded_pool_params" => Ok(GeneratedField::ShieldedPoolParams),
                            "complianceParams" | "compliance_params" => Ok(GeneratedField::ComplianceParams),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AppParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.AppParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AppParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut chain_id__ = None;
                let mut sct_params__ = None;
                let mut governance_params__ = None;
                let mut ibc_params__ = None;
                let mut validator_params__ = None;
                let mut fee_params__ = None;
                let mut shielded_pool_params__ = None;
                let mut compliance_params__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ChainId => {
                            if chain_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chainId"));
                            }
                            chain_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SctParams => {
                            if sct_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sctParams"));
                            }
                            sct_params__ = map_.next_value()?;
                        }
                        GeneratedField::GovernanceParams => {
                            if governance_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("governanceParams"));
                            }
                            governance_params__ = map_.next_value()?;
                        }
                        GeneratedField::IbcParams => {
                            if ibc_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcParams"));
                            }
                            ibc_params__ = map_.next_value()?;
                        }
                        GeneratedField::ValidatorParams => {
                            if validator_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("validatorParams"));
                            }
                            validator_params__ = map_.next_value()?;
                        }
                        GeneratedField::FeeParams => {
                            if fee_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("feeParams"));
                            }
                            fee_params__ = map_.next_value()?;
                        }
                        GeneratedField::ShieldedPoolParams => {
                            if shielded_pool_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("shieldedPoolParams"));
                            }
                            shielded_pool_params__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceParams => {
                            if compliance_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceParams"));
                            }
                            compliance_params__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AppParameters {
                    chain_id: chain_id__.unwrap_or_default(),
                    sct_params: sct_params__,
                    governance_params: governance_params__,
                    ibc_params: ibc_params__,
                    validator_params: validator_params__,
                    fee_params: fee_params__,
                    shielded_pool_params: shielded_pool_params__,
                    compliance_params: compliance_params__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.AppParameters", FIELDS, GeneratedVisitor)
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
        let struct_ser = serializer.serialize_struct("penumbra.core.app.v1.AppParametersRequest", len)?;
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
                formatter.write_str("struct penumbra.core.app.v1.AppParametersRequest")
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
        deserializer.deserialize_struct("penumbra.core.app.v1.AppParametersRequest", FIELDS, GeneratedVisitor)
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
        if self.app_parameters.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.AppParametersResponse", len)?;
        if let Some(v) = self.app_parameters.as_ref() {
            struct_ser.serialize_field("appParameters", v)?;
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
            "app_parameters",
            "appParameters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AppParameters,
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
                            "appParameters" | "app_parameters" => Ok(GeneratedField::AppParameters),
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
                formatter.write_str("struct penumbra.core.app.v1.AppParametersResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AppParametersResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut app_parameters__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AppParameters => {
                            if app_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("appParameters"));
                            }
                            app_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AppParametersResponse {
                    app_parameters: app_parameters__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.AppParametersResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventAppParametersChange {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.new_parameters.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.EventAppParametersChange", len)?;
        if let Some(v) = self.new_parameters.as_ref() {
            struct_ser.serialize_field("newParameters", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventAppParametersChange {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "new_parameters",
            "newParameters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewParameters,
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
                            "newParameters" | "new_parameters" => Ok(GeneratedField::NewParameters),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventAppParametersChange;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.EventAppParametersChange")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventAppParametersChange, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut new_parameters__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NewParameters => {
                            if new_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newParameters"));
                            }
                            new_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventAppParametersChange {
                    new_parameters: new_parameters__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.EventAppParametersChange", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GenesisAppState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.genesis_app_state.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.GenesisAppState", len)?;
        if let Some(v) = self.genesis_app_state.as_ref() {
            match v {
                genesis_app_state::GenesisAppState::GenesisContent(v) => {
                    struct_ser.serialize_field("genesisContent", v)?;
                }
                genesis_app_state::GenesisAppState::GenesisCheckpoint(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("genesisCheckpoint", pbjson::private::base64::encode(&v).as_str())?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GenesisAppState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "genesis_content",
            "genesisContent",
            "genesis_checkpoint",
            "genesisCheckpoint",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GenesisContent,
            GenesisCheckpoint,
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
                            "genesisContent" | "genesis_content" => Ok(GeneratedField::GenesisContent),
                            "genesisCheckpoint" | "genesis_checkpoint" => Ok(GeneratedField::GenesisCheckpoint),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GenesisAppState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.GenesisAppState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisAppState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut genesis_app_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GenesisContent => {
                            if genesis_app_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("genesisContent"));
                            }
                            genesis_app_state__ = map_.next_value::<::std::option::Option<_>>()?.map(genesis_app_state::GenesisAppState::GenesisContent)
;
                        }
                        GeneratedField::GenesisCheckpoint => {
                            if genesis_app_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("genesisCheckpoint"));
                            }
                            genesis_app_state__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| genesis_app_state::GenesisAppState::GenesisCheckpoint(x.0));
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisAppState {
                    genesis_app_state: genesis_app_state__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.GenesisAppState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GenesisContent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.chain_id.is_empty() {
            len += 1;
        }
        if self.validator_content.is_some() {
            len += 1;
        }
        if self.shielded_pool_content.is_some() {
            len += 1;
        }
        if self.governance_content.is_some() {
            len += 1;
        }
        if self.ibc_content.is_some() {
            len += 1;
        }
        if self.sct_content.is_some() {
            len += 1;
        }
        if self.fee_content.is_some() {
            len += 1;
        }
        if self.compliance_content.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.GenesisContent", len)?;
        if !self.chain_id.is_empty() {
            struct_ser.serialize_field("chainId", &self.chain_id)?;
        }
        if let Some(v) = self.validator_content.as_ref() {
            struct_ser.serialize_field("validatorContent", v)?;
        }
        if let Some(v) = self.shielded_pool_content.as_ref() {
            struct_ser.serialize_field("shieldedPoolContent", v)?;
        }
        if let Some(v) = self.governance_content.as_ref() {
            struct_ser.serialize_field("governanceContent", v)?;
        }
        if let Some(v) = self.ibc_content.as_ref() {
            struct_ser.serialize_field("ibcContent", v)?;
        }
        if let Some(v) = self.sct_content.as_ref() {
            struct_ser.serialize_field("sctContent", v)?;
        }
        if let Some(v) = self.fee_content.as_ref() {
            struct_ser.serialize_field("feeContent", v)?;
        }
        if let Some(v) = self.compliance_content.as_ref() {
            struct_ser.serialize_field("complianceContent", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GenesisContent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "chain_id",
            "chainId",
            "validator_content",
            "validatorContent",
            "shielded_pool_content",
            "shieldedPoolContent",
            "governance_content",
            "governanceContent",
            "ibc_content",
            "ibcContent",
            "sct_content",
            "sctContent",
            "fee_content",
            "feeContent",
            "compliance_content",
            "complianceContent",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ChainId,
            ValidatorContent,
            ShieldedPoolContent,
            GovernanceContent,
            IbcContent,
            SctContent,
            FeeContent,
            ComplianceContent,
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
                            "chainId" | "chain_id" => Ok(GeneratedField::ChainId),
                            "validatorContent" | "validator_content" => Ok(GeneratedField::ValidatorContent),
                            "shieldedPoolContent" | "shielded_pool_content" => Ok(GeneratedField::ShieldedPoolContent),
                            "governanceContent" | "governance_content" => Ok(GeneratedField::GovernanceContent),
                            "ibcContent" | "ibc_content" => Ok(GeneratedField::IbcContent),
                            "sctContent" | "sct_content" => Ok(GeneratedField::SctContent),
                            "feeContent" | "fee_content" => Ok(GeneratedField::FeeContent),
                            "complianceContent" | "compliance_content" => Ok(GeneratedField::ComplianceContent),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GenesisContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.GenesisContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut chain_id__ = None;
                let mut validator_content__ = None;
                let mut shielded_pool_content__ = None;
                let mut governance_content__ = None;
                let mut ibc_content__ = None;
                let mut sct_content__ = None;
                let mut fee_content__ = None;
                let mut compliance_content__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ChainId => {
                            if chain_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chainId"));
                            }
                            chain_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ValidatorContent => {
                            if validator_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("validatorContent"));
                            }
                            validator_content__ = map_.next_value()?;
                        }
                        GeneratedField::ShieldedPoolContent => {
                            if shielded_pool_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("shieldedPoolContent"));
                            }
                            shielded_pool_content__ = map_.next_value()?;
                        }
                        GeneratedField::GovernanceContent => {
                            if governance_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("governanceContent"));
                            }
                            governance_content__ = map_.next_value()?;
                        }
                        GeneratedField::IbcContent => {
                            if ibc_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcContent"));
                            }
                            ibc_content__ = map_.next_value()?;
                        }
                        GeneratedField::SctContent => {
                            if sct_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sctContent"));
                            }
                            sct_content__ = map_.next_value()?;
                        }
                        GeneratedField::FeeContent => {
                            if fee_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("feeContent"));
                            }
                            fee_content__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceContent => {
                            if compliance_content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceContent"));
                            }
                            compliance_content__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisContent {
                    chain_id: chain_id__.unwrap_or_default(),
                    validator_content: validator_content__,
                    shielded_pool_content: shielded_pool_content__,
                    governance_content: governance_content__,
                    ibc_content: ibc_content__,
                    sct_content: sct_content__,
                    fee_content: fee_content__,
                    compliance_content: compliance_content__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.GenesisContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionsByHeightRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.block_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.TransactionsByHeightRequest", len)?;
        if self.block_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blockHeight", ToString::to_string(&self.block_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionsByHeightRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "block_height",
            "blockHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BlockHeight,
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
                            "blockHeight" | "block_height" => Ok(GeneratedField::BlockHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionsByHeightRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.TransactionsByHeightRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionsByHeightRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut block_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BlockHeight => {
                            if block_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockHeight"));
                            }
                            block_height__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionsByHeightRequest {
                    block_height: block_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.TransactionsByHeightRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransactionsByHeightResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.transactions.is_empty() {
            len += 1;
        }
        if self.block_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.app.v1.TransactionsByHeightResponse", len)?;
        if !self.transactions.is_empty() {
            struct_ser.serialize_field("transactions", &self.transactions)?;
        }
        if self.block_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blockHeight", ToString::to_string(&self.block_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransactionsByHeightResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transactions",
            "block_height",
            "blockHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transactions,
            BlockHeight,
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
                            "transactions" => Ok(GeneratedField::Transactions),
                            "blockHeight" | "block_height" => Ok(GeneratedField::BlockHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransactionsByHeightResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.app.v1.TransactionsByHeightResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransactionsByHeightResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transactions__ = None;
                let mut block_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transactions => {
                            if transactions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactions"));
                            }
                            transactions__ = Some(map_.next_value()?);
                        }
                        GeneratedField::BlockHeight => {
                            if block_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockHeight"));
                            }
                            block_height__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransactionsByHeightResponse {
                    transactions: transactions__.unwrap_or_default(),
                    block_height: block_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.app.v1.TransactionsByHeightResponse", FIELDS, GeneratedVisitor)
    }
}
