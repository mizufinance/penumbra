impl serde::Serialize for CompactBlock {
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
        if !self.state_payloads.is_empty() {
            len += 1;
        }
        if !self.nullifiers.is_empty() {
            len += 1;
        }
        if self.block_root.is_some() {
            len += 1;
        }
        if self.epoch_root.is_some() {
            len += 1;
        }
        if self.discovery_parameters.is_some() {
            len += 1;
        }
        if !self.routing_records.is_empty() {
            len += 1;
        }
        if self.app_parameters_updated {
            len += 1;
        }
        if self.gas_prices.is_some() {
            len += 1;
        }
        if !self.alt_gas_prices.is_empty() {
            len += 1;
        }
        if self.epoch_index != 0 {
            len += 1;
        }
        if !self.compliance_user_anchor.is_empty() {
            len += 1;
        }
        if !self.compliance_asset_anchor.is_empty() {
            len += 1;
        }
        if !self.compliance_user_registrations.is_empty() {
            len += 1;
        }
        if !self.compliance_user_status_changes.is_empty() {
            len += 1;
        }
        if !self.compliance_asset_registrations.is_empty() {
            len += 1;
        }
        if !self.routing_action_payloads.is_empty() {
            len += 1;
        }
        if self.nullifier_window.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.CompactBlock", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if !self.state_payloads.is_empty() {
            struct_ser.serialize_field("statePayloads", &self.state_payloads)?;
        }
        if !self.nullifiers.is_empty() {
            struct_ser.serialize_field("nullifiers", &self.nullifiers)?;
        }
        if let Some(v) = self.block_root.as_ref() {
            struct_ser.serialize_field("blockRoot", v)?;
        }
        if let Some(v) = self.epoch_root.as_ref() {
            struct_ser.serialize_field("epochRoot", v)?;
        }
        if let Some(v) = self.discovery_parameters.as_ref() {
            struct_ser.serialize_field("discoveryParameters", v)?;
        }
        if !self.routing_records.is_empty() {
            struct_ser.serialize_field("routingRecords", &self.routing_records)?;
        }
        if self.app_parameters_updated {
            struct_ser.serialize_field("appParametersUpdated", &self.app_parameters_updated)?;
        }
        if let Some(v) = self.gas_prices.as_ref() {
            struct_ser.serialize_field("gasPrices", v)?;
        }
        if !self.alt_gas_prices.is_empty() {
            struct_ser.serialize_field("altGasPrices", &self.alt_gas_prices)?;
        }
        if self.epoch_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("epochIndex", ToString::to_string(&self.epoch_index).as_str())?;
        }
        if !self.compliance_user_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceUserAnchor", pbjson::private::base64::encode(&self.compliance_user_anchor).as_str())?;
        }
        if !self.compliance_asset_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceAssetAnchor", pbjson::private::base64::encode(&self.compliance_asset_anchor).as_str())?;
        }
        if !self.compliance_user_registrations.is_empty() {
            struct_ser.serialize_field("complianceUserRegistrations", &self.compliance_user_registrations)?;
        }
        if !self.compliance_user_status_changes.is_empty() {
            struct_ser.serialize_field("complianceUserStatusChanges", &self.compliance_user_status_changes)?;
        }
        if !self.compliance_asset_registrations.is_empty() {
            struct_ser.serialize_field("complianceAssetRegistrations", &self.compliance_asset_registrations)?;
        }
        if !self.routing_action_payloads.is_empty() {
            struct_ser.serialize_field("routingActionPayloads", &self.routing_action_payloads)?;
        }
        if let Some(v) = self.nullifier_window.as_ref() {
            struct_ser.serialize_field("nullifierWindow", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CompactBlock {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
            "state_payloads",
            "statePayloads",
            "nullifiers",
            "block_root",
            "blockRoot",
            "epoch_root",
            "epochRoot",
            "discovery_parameters",
            "discoveryParameters",
            "routing_records",
            "routingRecords",
            "app_parameters_updated",
            "appParametersUpdated",
            "gas_prices",
            "gasPrices",
            "alt_gas_prices",
            "altGasPrices",
            "epoch_index",
            "epochIndex",
            "compliance_user_anchor",
            "complianceUserAnchor",
            "compliance_asset_anchor",
            "complianceAssetAnchor",
            "compliance_user_registrations",
            "complianceUserRegistrations",
            "compliance_user_status_changes",
            "complianceUserStatusChanges",
            "compliance_asset_registrations",
            "complianceAssetRegistrations",
            "routing_action_payloads",
            "routingActionPayloads",
            "nullifier_window",
            "nullifierWindow",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
            StatePayloads,
            Nullifiers,
            BlockRoot,
            EpochRoot,
            DiscoveryParameters,
            RoutingRecords,
            AppParametersUpdated,
            GasPrices,
            AltGasPrices,
            EpochIndex,
            ComplianceUserAnchor,
            ComplianceAssetAnchor,
            ComplianceUserRegistrations,
            ComplianceUserStatusChanges,
            ComplianceAssetRegistrations,
            RoutingActionPayloads,
            NullifierWindow,
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
                            "statePayloads" | "state_payloads" => Ok(GeneratedField::StatePayloads),
                            "nullifiers" => Ok(GeneratedField::Nullifiers),
                            "blockRoot" | "block_root" => Ok(GeneratedField::BlockRoot),
                            "epochRoot" | "epoch_root" => Ok(GeneratedField::EpochRoot),
                            "discoveryParameters" | "discovery_parameters" => Ok(GeneratedField::DiscoveryParameters),
                            "routingRecords" | "routing_records" => Ok(GeneratedField::RoutingRecords),
                            "appParametersUpdated" | "app_parameters_updated" => Ok(GeneratedField::AppParametersUpdated),
                            "gasPrices" | "gas_prices" => Ok(GeneratedField::GasPrices),
                            "altGasPrices" | "alt_gas_prices" => Ok(GeneratedField::AltGasPrices),
                            "epochIndex" | "epoch_index" => Ok(GeneratedField::EpochIndex),
                            "complianceUserAnchor" | "compliance_user_anchor" => Ok(GeneratedField::ComplianceUserAnchor),
                            "complianceAssetAnchor" | "compliance_asset_anchor" => Ok(GeneratedField::ComplianceAssetAnchor),
                            "complianceUserRegistrations" | "compliance_user_registrations" => Ok(GeneratedField::ComplianceUserRegistrations),
                            "complianceUserStatusChanges" | "compliance_user_status_changes" => Ok(GeneratedField::ComplianceUserStatusChanges),
                            "complianceAssetRegistrations" | "compliance_asset_registrations" => Ok(GeneratedField::ComplianceAssetRegistrations),
                            "routingActionPayloads" | "routing_action_payloads" => Ok(GeneratedField::RoutingActionPayloads),
                            "nullifierWindow" | "nullifier_window" => Ok(GeneratedField::NullifierWindow),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CompactBlock;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.CompactBlock")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CompactBlock, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
                let mut state_payloads__ = None;
                let mut nullifiers__ = None;
                let mut block_root__ = None;
                let mut epoch_root__ = None;
                let mut discovery_parameters__ = None;
                let mut routing_records__ = None;
                let mut app_parameters_updated__ = None;
                let mut gas_prices__ = None;
                let mut alt_gas_prices__ = None;
                let mut epoch_index__ = None;
                let mut compliance_user_anchor__ = None;
                let mut compliance_asset_anchor__ = None;
                let mut compliance_user_registrations__ = None;
                let mut compliance_user_status_changes__ = None;
                let mut compliance_asset_registrations__ = None;
                let mut routing_action_payloads__ = None;
                let mut nullifier_window__ = None;
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
                        GeneratedField::StatePayloads => {
                            if state_payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("statePayloads"));
                            }
                            state_payloads__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Nullifiers => {
                            if nullifiers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifiers"));
                            }
                            nullifiers__ = Some(map_.next_value()?);
                        }
                        GeneratedField::BlockRoot => {
                            if block_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockRoot"));
                            }
                            block_root__ = map_.next_value()?;
                        }
                        GeneratedField::EpochRoot => {
                            if epoch_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epochRoot"));
                            }
                            epoch_root__ = map_.next_value()?;
                        }
                        GeneratedField::DiscoveryParameters => {
                            if discovery_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("discoveryParameters"));
                            }
                            discovery_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingRecords => {
                            if routing_records__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingRecords"));
                            }
                            routing_records__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AppParametersUpdated => {
                            if app_parameters_updated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("appParametersUpdated"));
                            }
                            app_parameters_updated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::GasPrices => {
                            if gas_prices__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gasPrices"));
                            }
                            gas_prices__ = map_.next_value()?;
                        }
                        GeneratedField::AltGasPrices => {
                            if alt_gas_prices__.is_some() {
                                return Err(serde::de::Error::duplicate_field("altGasPrices"));
                            }
                            alt_gas_prices__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EpochIndex => {
                            if epoch_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epochIndex"));
                            }
                            epoch_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceUserAnchor => {
                            if compliance_user_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceUserAnchor"));
                            }
                            compliance_user_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceAssetAnchor => {
                            if compliance_asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAssetAnchor"));
                            }
                            compliance_asset_anchor__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceUserRegistrations => {
                            if compliance_user_registrations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceUserRegistrations"));
                            }
                            compliance_user_registrations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceUserStatusChanges => {
                            if compliance_user_status_changes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceUserStatusChanges"));
                            }
                            compliance_user_status_changes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceAssetRegistrations => {
                            if compliance_asset_registrations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAssetRegistrations"));
                            }
                            compliance_asset_registrations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RoutingActionPayloads => {
                            if routing_action_payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingActionPayloads"));
                            }
                            routing_action_payloads__ = Some(map_.next_value()?);
                        }
                        GeneratedField::NullifierWindow => {
                            if nullifier_window__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifierWindow"));
                            }
                            nullifier_window__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CompactBlock {
                    height: height__.unwrap_or_default(),
                    state_payloads: state_payloads__.unwrap_or_default(),
                    nullifiers: nullifiers__.unwrap_or_default(),
                    block_root: block_root__,
                    epoch_root: epoch_root__,
                    discovery_parameters: discovery_parameters__,
                    routing_records: routing_records__.unwrap_or_default(),
                    app_parameters_updated: app_parameters_updated__.unwrap_or_default(),
                    gas_prices: gas_prices__,
                    alt_gas_prices: alt_gas_prices__.unwrap_or_default(),
                    epoch_index: epoch_index__.unwrap_or_default(),
                    compliance_user_anchor: compliance_user_anchor__.unwrap_or_default(),
                    compliance_asset_anchor: compliance_asset_anchor__.unwrap_or_default(),
                    compliance_user_registrations: compliance_user_registrations__.unwrap_or_default(),
                    compliance_user_status_changes: compliance_user_status_changes__.unwrap_or_default(),
                    compliance_asset_registrations: compliance_asset_registrations__.unwrap_or_default(),
                    routing_action_payloads: routing_action_payloads__.unwrap_or_default(),
                    nullifier_window: nullifier_window__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.CompactBlock", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CompactBlockRangeRequest {
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
        if self.keep_alive {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRangeRequest", len)?;
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
        if self.keep_alive {
            struct_ser.serialize_field("keepAlive", &self.keep_alive)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CompactBlockRangeRequest {
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
            "keep_alive",
            "keepAlive",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartHeight,
            EndHeight,
            KeepAlive,
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
                            "keepAlive" | "keep_alive" => Ok(GeneratedField::KeepAlive),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CompactBlockRangeRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.CompactBlockRangeRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CompactBlockRangeRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut start_height__ = None;
                let mut end_height__ = None;
                let mut keep_alive__ = None;
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
                        GeneratedField::KeepAlive => {
                            if keep_alive__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keepAlive"));
                            }
                            keep_alive__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CompactBlockRangeRequest {
                    start_height: start_height__.unwrap_or_default(),
                    end_height: end_height__.unwrap_or_default(),
                    keep_alive: keep_alive__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRangeRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CompactBlockRangeResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.compact_block.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRangeResponse", len)?;
        if let Some(v) = self.compact_block.as_ref() {
            struct_ser.serialize_field("compactBlock", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CompactBlockRangeResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "compact_block",
            "compactBlock",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CompactBlock,
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
                            "compactBlock" | "compact_block" => Ok(GeneratedField::CompactBlock),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CompactBlockRangeResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.CompactBlockRangeResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CompactBlockRangeResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut compact_block__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CompactBlock => {
                            if compact_block__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compactBlock"));
                            }
                            compact_block__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CompactBlockRangeResponse {
                    compact_block: compact_block__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRangeResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CompactBlockRequest {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRequest", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CompactBlockRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CompactBlockRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.CompactBlockRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CompactBlockRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CompactBlockRequest {
                    height: height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.CompactBlockRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CompactBlockResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.compact_block.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.CompactBlockResponse", len)?;
        if let Some(v) = self.compact_block.as_ref() {
            struct_ser.serialize_field("compactBlock", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CompactBlockResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "compact_block",
            "compactBlock",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CompactBlock,
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
                            "compactBlock" | "compact_block" => Ok(GeneratedField::CompactBlock),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CompactBlockResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.CompactBlockResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CompactBlockResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut compact_block__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CompactBlock => {
                            if compact_block__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compactBlock"));
                            }
                            compact_block__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CompactBlockResponse {
                    compact_block: compact_block__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.CompactBlockResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingActionPayloads {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transaction_id.is_some() {
            len += 1;
        }
        if self.action_index != 0 {
            len += 1;
        }
        if !self.note_payloads.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingActionPayloads", len)?;
        if let Some(v) = self.transaction_id.as_ref() {
            struct_ser.serialize_field("transactionId", v)?;
        }
        if self.action_index != 0 {
            struct_ser.serialize_field("actionIndex", &self.action_index)?;
        }
        if !self.note_payloads.is_empty() {
            struct_ser.serialize_field("notePayloads", &self.note_payloads)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingActionPayloads {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction_id",
            "transactionId",
            "action_index",
            "actionIndex",
            "note_payloads",
            "notePayloads",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TransactionId,
            ActionIndex,
            NotePayloads,
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
                            "transactionId" | "transaction_id" => Ok(GeneratedField::TransactionId),
                            "actionIndex" | "action_index" => Ok(GeneratedField::ActionIndex),
                            "notePayloads" | "note_payloads" => Ok(GeneratedField::NotePayloads),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingActionPayloads;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingActionPayloads")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingActionPayloads, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction_id__ = None;
                let mut action_index__ = None;
                let mut note_payloads__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TransactionId => {
                            if transaction_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionId"));
                            }
                            transaction_id__ = map_.next_value()?;
                        }
                        GeneratedField::ActionIndex => {
                            if action_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("actionIndex"));
                            }
                            action_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NotePayloads => {
                            if note_payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("notePayloads"));
                            }
                            note_payloads__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingActionPayloads {
                    transaction_id: transaction_id__,
                    action_index: action_index__.unwrap_or_default(),
                    note_payloads: note_payloads__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingActionPayloads", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingBlock {
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
        if self.block_root.is_some() {
            len += 1;
        }
        if self.epoch_root.is_some() {
            len += 1;
        }
        if !self.records.is_empty() {
            len += 1;
        }
        if self.discovery_parameters.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingBlock", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.block_root.as_ref() {
            struct_ser.serialize_field("blockRoot", v)?;
        }
        if let Some(v) = self.epoch_root.as_ref() {
            struct_ser.serialize_field("epochRoot", v)?;
        }
        if !self.records.is_empty() {
            struct_ser.serialize_field("records", &self.records)?;
        }
        if let Some(v) = self.discovery_parameters.as_ref() {
            struct_ser.serialize_field("discoveryParameters", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingBlock {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
            "block_root",
            "blockRoot",
            "epoch_root",
            "epochRoot",
            "records",
            "discovery_parameters",
            "discoveryParameters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
            BlockRoot,
            EpochRoot,
            Records,
            DiscoveryParameters,
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
                            "blockRoot" | "block_root" => Ok(GeneratedField::BlockRoot),
                            "epochRoot" | "epoch_root" => Ok(GeneratedField::EpochRoot),
                            "records" => Ok(GeneratedField::Records),
                            "discoveryParameters" | "discovery_parameters" => Ok(GeneratedField::DiscoveryParameters),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingBlock;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingBlock")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingBlock, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
                let mut block_root__ = None;
                let mut epoch_root__ = None;
                let mut records__ = None;
                let mut discovery_parameters__ = None;
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
                        GeneratedField::BlockRoot => {
                            if block_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockRoot"));
                            }
                            block_root__ = map_.next_value()?;
                        }
                        GeneratedField::EpochRoot => {
                            if epoch_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epochRoot"));
                            }
                            epoch_root__ = map_.next_value()?;
                        }
                        GeneratedField::Records => {
                            if records__.is_some() {
                                return Err(serde::de::Error::duplicate_field("records"));
                            }
                            records__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DiscoveryParameters => {
                            if discovery_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("discoveryParameters"));
                            }
                            discovery_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingBlock {
                    height: height__.unwrap_or_default(),
                    block_root: block_root__,
                    epoch_root: epoch_root__,
                    records: records__.unwrap_or_default(),
                    discovery_parameters: discovery_parameters__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingBlock", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingBlockRangeRequest {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingBlockRangeRequest", len)?;
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
impl<'de> serde::Deserialize<'de> for RoutingBlockRangeRequest {
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
            type Value = RoutingBlockRangeRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingBlockRangeRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingBlockRangeRequest, V::Error>
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
                Ok(RoutingBlockRangeRequest {
                    start_height: start_height__.unwrap_or_default(),
                    end_height: end_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingBlockRangeRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingBlockRangeResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.routing_block.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingBlockRangeResponse", len)?;
        if let Some(v) = self.routing_block.as_ref() {
            struct_ser.serialize_field("routingBlock", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingBlockRangeResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "routing_block",
            "routingBlock",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoutingBlock,
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
                            "routingBlock" | "routing_block" => Ok(GeneratedField::RoutingBlock),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingBlockRangeResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingBlockRangeResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingBlockRangeResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut routing_block__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RoutingBlock => {
                            if routing_block__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingBlock"));
                            }
                            routing_block__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingBlockRangeResponse {
                    routing_block: routing_block__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingBlockRangeResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingCandidatesRequest {
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
        if !self.selectors.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingCandidatesRequest", len)?;
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
        if !self.selectors.is_empty() {
            struct_ser.serialize_field("selectors", &self.selectors)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingCandidatesRequest {
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
            "selectors",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartHeight,
            EndHeight,
            Selectors,
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
                            "selectors" => Ok(GeneratedField::Selectors),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingCandidatesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingCandidatesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingCandidatesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut start_height__ = None;
                let mut end_height__ = None;
                let mut selectors__ = None;
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
                        GeneratedField::Selectors => {
                            if selectors__.is_some() {
                                return Err(serde::de::Error::duplicate_field("selectors"));
                            }
                            selectors__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingCandidatesRequest {
                    start_height: start_height__.unwrap_or_default(),
                    end_height: end_height__.unwrap_or_default(),
                    selectors: selectors__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingCandidatesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingCandidatesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.record.is_some() {
            len += 1;
        }
        if !self.note_payloads.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingCandidatesResponse", len)?;
        if let Some(v) = self.record.as_ref() {
            struct_ser.serialize_field("record", v)?;
        }
        if !self.note_payloads.is_empty() {
            struct_ser.serialize_field("notePayloads", &self.note_payloads)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingCandidatesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "record",
            "note_payloads",
            "notePayloads",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Record,
            NotePayloads,
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
                            "record" => Ok(GeneratedField::Record),
                            "notePayloads" | "note_payloads" => Ok(GeneratedField::NotePayloads),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingCandidatesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingCandidatesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingCandidatesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut record__ = None;
                let mut note_payloads__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Record => {
                            if record__.is_some() {
                                return Err(serde::de::Error::duplicate_field("record"));
                            }
                            record__ = map_.next_value()?;
                        }
                        GeneratedField::NotePayloads => {
                            if note_payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("notePayloads"));
                            }
                            note_payloads__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingCandidatesResponse {
                    record: record__,
                    note_payloads: note_payloads__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingCandidatesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingRecord {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.tag.is_some() {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.transaction_id.is_some() {
            len += 1;
        }
        if self.action_index != 0 {
            len += 1;
        }
        if self.tag_slot != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.RoutingRecord", len)?;
        if let Some(v) = self.tag.as_ref() {
            struct_ser.serialize_field("tag", v)?;
        }
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.transaction_id.as_ref() {
            struct_ser.serialize_field("transactionId", v)?;
        }
        if self.action_index != 0 {
            struct_ser.serialize_field("actionIndex", &self.action_index)?;
        }
        if self.tag_slot != 0 {
            struct_ser.serialize_field("tagSlot", &self.tag_slot)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingRecord {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "tag",
            "height",
            "transaction_id",
            "transactionId",
            "action_index",
            "actionIndex",
            "tag_slot",
            "tagSlot",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Tag,
            Height,
            TransactionId,
            ActionIndex,
            TagSlot,
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
                            "tag" => Ok(GeneratedField::Tag),
                            "height" => Ok(GeneratedField::Height),
                            "transactionId" | "transaction_id" => Ok(GeneratedField::TransactionId),
                            "actionIndex" | "action_index" => Ok(GeneratedField::ActionIndex),
                            "tagSlot" | "tag_slot" => Ok(GeneratedField::TagSlot),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingRecord;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.RoutingRecord")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingRecord, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut tag__ = None;
                let mut height__ = None;
                let mut transaction_id__ = None;
                let mut action_index__ = None;
                let mut tag_slot__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Tag => {
                            if tag__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tag"));
                            }
                            tag__ = map_.next_value()?;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TransactionId => {
                            if transaction_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionId"));
                            }
                            transaction_id__ = map_.next_value()?;
                        }
                        GeneratedField::ActionIndex => {
                            if action_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("actionIndex"));
                            }
                            action_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TagSlot => {
                            if tag_slot__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tagSlot"));
                            }
                            tag_slot__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingRecord {
                    tag: tag__,
                    height: height__.unwrap_or_default(),
                    transaction_id: transaction_id__,
                    action_index: action_index__.unwrap_or_default(),
                    tag_slot: tag_slot__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.RoutingRecord", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StatePayload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.source.is_some() {
            len += 1;
        }
        if self.state_payload.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.StatePayload", len)?;
        if let Some(v) = self.source.as_ref() {
            struct_ser.serialize_field("source", v)?;
        }
        if let Some(v) = self.state_payload.as_ref() {
            match v {
                state_payload::StatePayload::RolledUp(v) => {
                    struct_ser.serialize_field("rolledUp", v)?;
                }
                state_payload::StatePayload::Note(v) => {
                    struct_ser.serialize_field("note", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StatePayload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "source",
            "rolled_up",
            "rolledUp",
            "note",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Source,
            RolledUp,
            Note,
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
                            "source" => Ok(GeneratedField::Source),
                            "rolledUp" | "rolled_up" => Ok(GeneratedField::RolledUp),
                            "note" => Ok(GeneratedField::Note),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StatePayload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.StatePayload")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<StatePayload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut source__ = None;
                let mut state_payload__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = map_.next_value()?;
                        }
                        GeneratedField::RolledUp => {
                            if state_payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rolledUp"));
                            }
                            state_payload__ = map_.next_value::<::std::option::Option<_>>()?.map(state_payload::StatePayload::RolledUp)
;
                        }
                        GeneratedField::Note => {
                            if state_payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            state_payload__ = map_.next_value::<::std::option::Option<_>>()?.map(state_payload::StatePayload::Note)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(StatePayload {
                    source: source__,
                    state_payload: state_payload__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.StatePayload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for state_payload::Note {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.StatePayload.Note", len)?;
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for state_payload::Note {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Note,
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
                            "note" => Ok(GeneratedField::Note),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = state_payload::Note;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.StatePayload.Note")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<state_payload::Note, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Note => {
                            if note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            note__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(state_payload::Note {
                    note: note__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.StatePayload.Note", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for state_payload::RolledUp {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.commitment.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compact_block.v1.StatePayload.RolledUp", len)?;
        if let Some(v) = self.commitment.as_ref() {
            struct_ser.serialize_field("commitment", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for state_payload::RolledUp {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "commitment",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Commitment,
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
                            "commitment" => Ok(GeneratedField::Commitment),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = state_payload::RolledUp;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compact_block.v1.StatePayload.RolledUp")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<state_payload::RolledUp, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut commitment__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Commitment => {
                            if commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitment"));
                            }
                            commitment__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(state_payload::RolledUp {
                    commitment: commitment__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compact_block.v1.StatePayload.RolledUp", FIELDS, GeneratedVisitor)
    }
}
