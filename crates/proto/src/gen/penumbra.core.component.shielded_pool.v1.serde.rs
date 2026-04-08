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
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdRequest", len)?;
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
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.AssetMetadataByIdRequest")
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
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdResponse", len)?;
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
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.AssetMetadataByIdResponse")
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
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetMetadataByIdsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.asset_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsRequest", len)?;
        if !self.asset_id.is_empty() {
            struct_ser.serialize_field("assetId", &self.asset_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetMetadataByIdsRequest {
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
            type Value = AssetMetadataByIdsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetMetadataByIdsRequest, V::Error>
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
                            asset_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetMetadataByIdsRequest {
                    asset_id: asset_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetMetadataByIdsResponse {
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
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsResponse", len)?;
        if let Some(v) = self.denom_metadata.as_ref() {
            struct_ser.serialize_field("denomMetadata", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetMetadataByIdsResponse {
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
            type Value = AssetMetadataByIdsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetMetadataByIdsResponse, V::Error>
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
                Ok(AssetMetadataByIdsResponse {
                    denom_metadata: denom_metadata__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.AssetMetadataByIdsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventBroadcastClue {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.clue.is_some() {
            len += 1;
        }
        if self.tx.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventBroadcastClue", len)?;
        if let Some(v) = self.clue.as_ref() {
            struct_ser.serialize_field("clue", v)?;
        }
        if let Some(v) = self.tx.as_ref() {
            struct_ser.serialize_field("tx", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventBroadcastClue {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "clue",
            "tx",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Clue,
            Tx,
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
                            "clue" => Ok(GeneratedField::Clue),
                            "tx" => Ok(GeneratedField::Tx),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventBroadcastClue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventBroadcastClue")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventBroadcastClue, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut clue__ = None;
                let mut tx__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Clue => {
                            if clue__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clue"));
                            }
                            clue__ = map_.next_value()?;
                        }
                        GeneratedField::Tx => {
                            if tx__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tx"));
                            }
                            tx__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventBroadcastClue {
                    clue: clue__,
                    tx: tx__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventBroadcastClue", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventInboundFungibleTokenTransfer {
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
        if !self.sender.is_empty() {
            len += 1;
        }
        if self.receiver.is_some() {
            len += 1;
        }
        if self.meta.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if !self.sender.is_empty() {
            struct_ser.serialize_field("sender", &self.sender)?;
        }
        if let Some(v) = self.receiver.as_ref() {
            struct_ser.serialize_field("receiver", v)?;
        }
        if let Some(v) = self.meta.as_ref() {
            struct_ser.serialize_field("meta", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventInboundFungibleTokenTransfer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "sender",
            "receiver",
            "meta",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Sender,
            Receiver,
            Meta,
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
                            "sender" => Ok(GeneratedField::Sender),
                            "receiver" => Ok(GeneratedField::Receiver),
                            "meta" => Ok(GeneratedField::Meta),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventInboundFungibleTokenTransfer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventInboundFungibleTokenTransfer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut sender__ = None;
                let mut receiver__ = None;
                let mut meta__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Sender => {
                            if sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender"));
                            }
                            sender__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Receiver => {
                            if receiver__.is_some() {
                                return Err(serde::de::Error::duplicate_field("receiver"));
                            }
                            receiver__ = map_.next_value()?;
                        }
                        GeneratedField::Meta => {
                            if meta__.is_some() {
                                return Err(serde::de::Error::duplicate_field("meta"));
                            }
                            meta__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventInboundFungibleTokenTransfer {
                    value: value__,
                    sender: sender__.unwrap_or_default(),
                    receiver: receiver__,
                    meta: meta__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventOutboundFungibleTokenRefund {
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
        if self.sender.is_some() {
            len += 1;
        }
        if !self.receiver.is_empty() {
            len += 1;
        }
        if self.reason != 0 {
            len += 1;
        }
        if self.meta.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if let Some(v) = self.sender.as_ref() {
            struct_ser.serialize_field("sender", v)?;
        }
        if !self.receiver.is_empty() {
            struct_ser.serialize_field("receiver", &self.receiver)?;
        }
        if self.reason != 0 {
            let v = event_outbound_fungible_token_refund::Reason::try_from(self.reason)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.reason)))?;
            struct_ser.serialize_field("reason", &v)?;
        }
        if let Some(v) = self.meta.as_ref() {
            struct_ser.serialize_field("meta", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventOutboundFungibleTokenRefund {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "sender",
            "receiver",
            "reason",
            "meta",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Sender,
            Receiver,
            Reason,
            Meta,
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
                            "sender" => Ok(GeneratedField::Sender),
                            "receiver" => Ok(GeneratedField::Receiver),
                            "reason" => Ok(GeneratedField::Reason),
                            "meta" => Ok(GeneratedField::Meta),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventOutboundFungibleTokenRefund;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventOutboundFungibleTokenRefund, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut sender__ = None;
                let mut receiver__ = None;
                let mut reason__ = None;
                let mut meta__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Sender => {
                            if sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender"));
                            }
                            sender__ = map_.next_value()?;
                        }
                        GeneratedField::Receiver => {
                            if receiver__.is_some() {
                                return Err(serde::de::Error::duplicate_field("receiver"));
                            }
                            receiver__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Reason => {
                            if reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reason"));
                            }
                            reason__ = Some(map_.next_value::<event_outbound_fungible_token_refund::Reason>()? as i32);
                        }
                        GeneratedField::Meta => {
                            if meta__.is_some() {
                                return Err(serde::de::Error::duplicate_field("meta"));
                            }
                            meta__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventOutboundFungibleTokenRefund {
                    value: value__,
                    sender: sender__,
                    receiver: receiver__.unwrap_or_default(),
                    reason: reason__.unwrap_or_default(),
                    meta: meta__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for event_outbound_fungible_token_refund::Reason {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "REASON_UNSPECIFIED",
            Self::Timeout => "REASON_TIMEOUT",
            Self::Error => "REASON_ERROR",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for event_outbound_fungible_token_refund::Reason {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "REASON_UNSPECIFIED",
            "REASON_TIMEOUT",
            "REASON_ERROR",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = event_outbound_fungible_token_refund::Reason;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "REASON_UNSPECIFIED" => Ok(event_outbound_fungible_token_refund::Reason::Unspecified),
                    "REASON_TIMEOUT" => Ok(event_outbound_fungible_token_refund::Reason::Timeout),
                    "REASON_ERROR" => Ok(event_outbound_fungible_token_refund::Reason::Error),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EventOutboundFungibleTokenTransfer {
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
        if self.sender.is_some() {
            len += 1;
        }
        if !self.receiver.is_empty() {
            len += 1;
        }
        if self.meta.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if let Some(v) = self.sender.as_ref() {
            struct_ser.serialize_field("sender", v)?;
        }
        if !self.receiver.is_empty() {
            struct_ser.serialize_field("receiver", &self.receiver)?;
        }
        if let Some(v) = self.meta.as_ref() {
            struct_ser.serialize_field("meta", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventOutboundFungibleTokenTransfer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "sender",
            "receiver",
            "meta",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Sender,
            Receiver,
            Meta,
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
                            "sender" => Ok(GeneratedField::Sender),
                            "receiver" => Ok(GeneratedField::Receiver),
                            "meta" => Ok(GeneratedField::Meta),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventOutboundFungibleTokenTransfer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventOutboundFungibleTokenTransfer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut sender__ = None;
                let mut receiver__ = None;
                let mut meta__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Sender => {
                            if sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender"));
                            }
                            sender__ = map_.next_value()?;
                        }
                        GeneratedField::Receiver => {
                            if receiver__.is_some() {
                                return Err(serde::de::Error::duplicate_field("receiver"));
                            }
                            receiver__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Meta => {
                            if meta__.is_some() {
                                return Err(serde::de::Error::duplicate_field("meta"));
                            }
                            meta__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventOutboundFungibleTokenTransfer {
                    value: value__,
                    sender: sender__,
                    receiver: receiver__.unwrap_or_default(),
                    meta: meta__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventOutput {
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
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventOutput", len)?;
        if let Some(v) = self.note_commitment.as_ref() {
            struct_ser.serialize_field("noteCommitment", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_commitment",
            "noteCommitment",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteCommitment,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventOutput")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_commitment__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteCommitment => {
                            if note_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteCommitment"));
                            }
                            note_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventOutput {
                    note_commitment: note_commitment__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventSpend {
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
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.EventSpend", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventSpend {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nullifier",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventSpend;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.EventSpend")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventSpend, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventSpend {
                    nullifier: nullifier__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.EventSpend", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FmdMetaParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.fmd_grace_period_blocks != 0 {
            len += 1;
        }
        if self.algorithm.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParameters", len)?;
        if self.fmd_grace_period_blocks != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("fmdGracePeriodBlocks", ToString::to_string(&self.fmd_grace_period_blocks).as_str())?;
        }
        if let Some(v) = self.algorithm.as_ref() {
            match v {
                fmd_meta_parameters::Algorithm::FixedPrecisionBits(v) => {
                    struct_ser.serialize_field("fixedPrecisionBits", v)?;
                }
                fmd_meta_parameters::Algorithm::SlidingWindow(v) => {
                    struct_ser.serialize_field("slidingWindow", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FmdMetaParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fmd_grace_period_blocks",
            "fmdGracePeriodBlocks",
            "fixed_precision_bits",
            "fixedPrecisionBits",
            "sliding_window",
            "slidingWindow",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FmdGracePeriodBlocks,
            FixedPrecisionBits,
            SlidingWindow,
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
                            "fmdGracePeriodBlocks" | "fmd_grace_period_blocks" => Ok(GeneratedField::FmdGracePeriodBlocks),
                            "fixedPrecisionBits" | "fixed_precision_bits" => Ok(GeneratedField::FixedPrecisionBits),
                            "slidingWindow" | "sliding_window" => Ok(GeneratedField::SlidingWindow),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FmdMetaParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdMetaParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FmdMetaParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut fmd_grace_period_blocks__ = None;
                let mut algorithm__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FmdGracePeriodBlocks => {
                            if fmd_grace_period_blocks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fmdGracePeriodBlocks"));
                            }
                            fmd_grace_period_blocks__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FixedPrecisionBits => {
                            if algorithm__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fixedPrecisionBits"));
                            }
                            algorithm__ = map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| fmd_meta_parameters::Algorithm::FixedPrecisionBits(x.0));
                        }
                        GeneratedField::SlidingWindow => {
                            if algorithm__.is_some() {
                                return Err(serde::de::Error::duplicate_field("slidingWindow"));
                            }
                            algorithm__ = map_.next_value::<::std::option::Option<_>>()?.map(fmd_meta_parameters::Algorithm::SlidingWindow)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FmdMetaParameters {
                    fmd_grace_period_blocks: fmd_grace_period_blocks__.unwrap_or_default(),
                    algorithm: algorithm__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParameters", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for fmd_meta_parameters::AlgorithmSlidingWindow {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.window_update_periods != 0 {
            len += 1;
        }
        if self.targeted_detections_per_window != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParameters.AlgorithmSlidingWindow", len)?;
        if self.window_update_periods != 0 {
            struct_ser.serialize_field("windowUpdatePeriods", &self.window_update_periods)?;
        }
        if self.targeted_detections_per_window != 0 {
            struct_ser.serialize_field("targetedDetectionsPerWindow", &self.targeted_detections_per_window)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for fmd_meta_parameters::AlgorithmSlidingWindow {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "window_update_periods",
            "windowUpdatePeriods",
            "targeted_detections_per_window",
            "targetedDetectionsPerWindow",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WindowUpdatePeriods,
            TargetedDetectionsPerWindow,
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
                            "windowUpdatePeriods" | "window_update_periods" => Ok(GeneratedField::WindowUpdatePeriods),
                            "targetedDetectionsPerWindow" | "targeted_detections_per_window" => Ok(GeneratedField::TargetedDetectionsPerWindow),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = fmd_meta_parameters::AlgorithmSlidingWindow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdMetaParameters.AlgorithmSlidingWindow")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<fmd_meta_parameters::AlgorithmSlidingWindow, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut window_update_periods__ = None;
                let mut targeted_detections_per_window__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WindowUpdatePeriods => {
                            if window_update_periods__.is_some() {
                                return Err(serde::de::Error::duplicate_field("windowUpdatePeriods"));
                            }
                            window_update_periods__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetedDetectionsPerWindow => {
                            if targeted_detections_per_window__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetedDetectionsPerWindow"));
                            }
                            targeted_detections_per_window__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(fmd_meta_parameters::AlgorithmSlidingWindow {
                    window_update_periods: window_update_periods__.unwrap_or_default(),
                    targeted_detections_per_window: targeted_detections_per_window__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParameters.AlgorithmSlidingWindow", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FmdMetaParametersAlgorithmState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.state.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState", len)?;
        if let Some(v) = self.state.as_ref() {
            match v {
                fmd_meta_parameters_algorithm_state::State::Fixed(v) => {
                    struct_ser.serialize_field("fixed", v)?;
                }
                fmd_meta_parameters_algorithm_state::State::SlidingWindow(v) => {
                    struct_ser.serialize_field("slidingWindow", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FmdMetaParametersAlgorithmState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fixed",
            "sliding_window",
            "slidingWindow",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Fixed,
            SlidingWindow,
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
                            "fixed" => Ok(GeneratedField::Fixed),
                            "slidingWindow" | "sliding_window" => Ok(GeneratedField::SlidingWindow),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FmdMetaParametersAlgorithmState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FmdMetaParametersAlgorithmState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Fixed => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fixed"));
                            }
                            state__ = map_.next_value::<::std::option::Option<_>>()?.map(fmd_meta_parameters_algorithm_state::State::Fixed)
;
                        }
                        GeneratedField::SlidingWindow => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("slidingWindow"));
                            }
                            state__ = map_.next_value::<::std::option::Option<_>>()?.map(fmd_meta_parameters_algorithm_state::State::SlidingWindow)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FmdMetaParametersAlgorithmState {
                    state: state__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for fmd_meta_parameters_algorithm_state::FixedState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.FixedState", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for fmd_meta_parameters_algorithm_state::FixedState {
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
            type Value = fmd_meta_parameters_algorithm_state::FixedState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.FixedState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<fmd_meta_parameters_algorithm_state::FixedState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(fmd_meta_parameters_algorithm_state::FixedState {
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.FixedState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for fmd_meta_parameters_algorithm_state::SlidingWindowState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.approximate_clue_count != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.SlidingWindowState", len)?;
        if self.approximate_clue_count != 0 {
            struct_ser.serialize_field("approximateClueCount", &self.approximate_clue_count)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for fmd_meta_parameters_algorithm_state::SlidingWindowState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "approximate_clue_count",
            "approximateClueCount",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ApproximateClueCount,
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
                            "approximateClueCount" | "approximate_clue_count" => Ok(GeneratedField::ApproximateClueCount),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = fmd_meta_parameters_algorithm_state::SlidingWindowState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.SlidingWindowState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<fmd_meta_parameters_algorithm_state::SlidingWindowState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut approximate_clue_count__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ApproximateClueCount => {
                            if approximate_clue_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("approximateClueCount"));
                            }
                            approximate_clue_count__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(fmd_meta_parameters_algorithm_state::SlidingWindowState {
                    approximate_clue_count: approximate_clue_count__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdMetaParametersAlgorithmState.SlidingWindowState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FmdParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.precision_bits != 0 {
            len += 1;
        }
        if self.as_of_block_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FmdParameters", len)?;
        if self.precision_bits != 0 {
            struct_ser.serialize_field("precisionBits", &self.precision_bits)?;
        }
        if self.as_of_block_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("asOfBlockHeight", ToString::to_string(&self.as_of_block_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FmdParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "precision_bits",
            "precisionBits",
            "as_of_block_height",
            "asOfBlockHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PrecisionBits,
            AsOfBlockHeight,
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
                            "precisionBits" | "precision_bits" => Ok(GeneratedField::PrecisionBits),
                            "asOfBlockHeight" | "as_of_block_height" => Ok(GeneratedField::AsOfBlockHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FmdParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FmdParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FmdParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut precision_bits__ = None;
                let mut as_of_block_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PrecisionBits => {
                            if precision_bits__.is_some() {
                                return Err(serde::de::Error::duplicate_field("precisionBits"));
                            }
                            precision_bits__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AsOfBlockHeight => {
                            if as_of_block_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("asOfBlockHeight"));
                            }
                            as_of_block_height__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FmdParameters {
                    precision_bits: precision_bits__.unwrap_or_default(),
                    as_of_block_height: as_of_block_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FmdParameters", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FungibleTokenTransferPacketMetadata {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.channel.is_empty() {
            len += 1;
        }
        if self.sequence != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata", len)?;
        if !self.channel.is_empty() {
            struct_ser.serialize_field("channel", &self.channel)?;
        }
        if self.sequence != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("sequence", ToString::to_string(&self.sequence).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FungibleTokenTransferPacketMetadata {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "channel",
            "sequence",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Channel,
            Sequence,
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
                            "channel" => Ok(GeneratedField::Channel),
                            "sequence" => Ok(GeneratedField::Sequence),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FungibleTokenTransferPacketMetadata;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FungibleTokenTransferPacketMetadata, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut channel__ = None;
                let mut sequence__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Channel => {
                            if channel__.is_some() {
                                return Err(serde::de::Error::duplicate_field("channel"));
                            }
                            channel__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Sequence => {
                            if sequence__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sequence"));
                            }
                            sequence__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FungibleTokenTransferPacketMetadata {
                    channel: channel__.unwrap_or_default(),
                    sequence: sequence__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata", FIELDS, GeneratedVisitor)
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
        if self.shielded_pool_params.is_some() {
            len += 1;
        }
        if !self.allocations.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.GenesisContent", len)?;
        if let Some(v) = self.shielded_pool_params.as_ref() {
            struct_ser.serialize_field("shieldedPoolParams", v)?;
        }
        if !self.allocations.is_empty() {
            struct_ser.serialize_field("allocations", &self.allocations)?;
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
            "shielded_pool_params",
            "shieldedPoolParams",
            "allocations",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ShieldedPoolParams,
            Allocations,
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
                            "shieldedPoolParams" | "shielded_pool_params" => Ok(GeneratedField::ShieldedPoolParams),
                            "allocations" => Ok(GeneratedField::Allocations),
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
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.GenesisContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut shielded_pool_params__ = None;
                let mut allocations__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ShieldedPoolParams => {
                            if shielded_pool_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("shieldedPoolParams"));
                            }
                            shielded_pool_params__ = map_.next_value()?;
                        }
                        GeneratedField::Allocations => {
                            if allocations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allocations"));
                            }
                            allocations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisContent {
                    shielded_pool_params: shielded_pool_params__,
                    allocations: allocations__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.GenesisContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for genesis_content::Allocation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.amount.is_some() {
            len += 1;
        }
        if !self.denom.is_empty() {
            len += 1;
        }
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.GenesisContent.Allocation", len)?;
        if let Some(v) = self.amount.as_ref() {
            struct_ser.serialize_field("amount", v)?;
        }
        if !self.denom.is_empty() {
            struct_ser.serialize_field("denom", &self.denom)?;
        }
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for genesis_content::Allocation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "amount",
            "denom",
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Amount,
            Denom,
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
                            "amount" => Ok(GeneratedField::Amount),
                            "denom" => Ok(GeneratedField::Denom),
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
            type Value = genesis_content::Allocation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.GenesisContent.Allocation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<genesis_content::Allocation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut amount__ = None;
                let mut denom__ = None;
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Amount => {
                            if amount__.is_some() {
                                return Err(serde::de::Error::duplicate_field("amount"));
                            }
                            amount__ = map_.next_value()?;
                        }
                        GeneratedField::Denom => {
                            if denom__.is_some() {
                                return Err(serde::de::Error::duplicate_field("denom"));
                            }
                            denom__ = Some(map_.next_value()?);
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
                Ok(genesis_content::Allocation {
                    amount: amount__,
                    denom: denom__.unwrap_or_default(),
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.GenesisContent.Allocation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Note {
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
        if !self.rseed.is_empty() {
            len += 1;
        }
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.Note", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if !self.rseed.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rseed", pbjson::private::base64::encode(&self.rseed).as_str())?;
        }
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Note {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "rseed",
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Rseed,
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
                            "rseed" => Ok(GeneratedField::Rseed),
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
            type Value = Note;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.Note")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Note, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut rseed__ = None;
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Rseed => {
                            if rseed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rseed"));
                            }
                            rseed__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                Ok(Note {
                    value: value__,
                    rseed: rseed__.unwrap_or_default(),
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.Note", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteCiphertext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inner.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.NoteCiphertext", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteCiphertext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inner,
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
                            "inner" => Ok(GeneratedField::Inner),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteCiphertext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.NoteCiphertext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteCiphertext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Inner => {
                            if inner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inner"));
                            }
                            inner__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteCiphertext {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.NoteCiphertext", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NotePayload {
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
        if !self.ephemeral_key.is_empty() {
            len += 1;
        }
        if self.encrypted_note.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.NotePayload", len)?;
        if let Some(v) = self.note_commitment.as_ref() {
            struct_ser.serialize_field("noteCommitment", v)?;
        }
        if !self.ephemeral_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ephemeralKey", pbjson::private::base64::encode(&self.ephemeral_key).as_str())?;
        }
        if let Some(v) = self.encrypted_note.as_ref() {
            struct_ser.serialize_field("encryptedNote", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NotePayload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_commitment",
            "noteCommitment",
            "ephemeral_key",
            "ephemeralKey",
            "encrypted_note",
            "encryptedNote",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteCommitment,
            EphemeralKey,
            EncryptedNote,
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
                            "ephemeralKey" | "ephemeral_key" => Ok(GeneratedField::EphemeralKey),
                            "encryptedNote" | "encrypted_note" => Ok(GeneratedField::EncryptedNote),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NotePayload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.NotePayload")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NotePayload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_commitment__ = None;
                let mut ephemeral_key__ = None;
                let mut encrypted_note__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteCommitment => {
                            if note_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteCommitment"));
                            }
                            note_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::EphemeralKey => {
                            if ephemeral_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ephemeralKey"));
                            }
                            ephemeral_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EncryptedNote => {
                            if encrypted_note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryptedNote"));
                            }
                            encrypted_note__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NotePayload {
                    note_commitment: note_commitment__,
                    ephemeral_key: ephemeral_key__.unwrap_or_default(),
                    encrypted_note: encrypted_note__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.NotePayload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteView {
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
        if !self.rseed.is_empty() {
            len += 1;
        }
        if self.address.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.NoteView", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if !self.rseed.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rseed", pbjson::private::base64::encode(&self.rseed).as_str())?;
        }
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteView {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "rseed",
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Rseed,
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
                            "rseed" => Ok(GeneratedField::Rseed),
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
            type Value = NoteView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.NoteView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut rseed__ = None;
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Rseed => {
                            if rseed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rseed"));
                            }
                            rseed__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                Ok(NoteView {
                    value: value__,
                    rseed: rseed__.unwrap_or_default(),
                    address: address__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.NoteView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Output {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.body.is_some() {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.Output", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if let Some(v) = self.proof.as_ref() {
            struct_ser.serialize_field("proof", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Output {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            Proof,
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
                            "body" => Ok(GeneratedField::Body),
                            "proof" => Ok(GeneratedField::Proof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Output;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.Output")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Output, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::Proof => {
                            if proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proof"));
                            }
                            proof__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Output {
                    body: body__,
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.Output", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OutputBody {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_payload.is_some() {
            len += 1;
        }
        if self.balance_commitment.is_some() {
            len += 1;
        }
        if !self.wrapped_memo_key.is_empty() {
            len += 1;
        }
        if !self.ovk_wrapped_key.is_empty() {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if self.target_timestamp != 0 {
            len += 1;
        }
        if self.counterparty_leaf_hash.is_some() {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.threshold.is_empty() {
            len += 1;
        }
        if !self.sender_ciphertext.is_empty() {
            len += 1;
        }
        if !self.dleq_proofs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.OutputBody", len)?;
        if let Some(v) = self.note_payload.as_ref() {
            struct_ser.serialize_field("notePayload", v)?;
        }
        if let Some(v) = self.balance_commitment.as_ref() {
            struct_ser.serialize_field("balanceCommitment", v)?;
        }
        if !self.wrapped_memo_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("wrappedMemoKey", pbjson::private::base64::encode(&self.wrapped_memo_key).as_str())?;
        }
        if !self.ovk_wrapped_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ovkWrappedKey", pbjson::private::base64::encode(&self.ovk_wrapped_key).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if self.target_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTimestamp", ToString::to_string(&self.target_timestamp).as_str())?;
        }
        if let Some(v) = self.counterparty_leaf_hash.as_ref() {
            struct_ser.serialize_field("counterpartyLeafHash", v)?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
        }
        if !self.dk_pub.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dkPub", pbjson::private::base64::encode(&self.dk_pub).as_str())?;
        }
        if !self.threshold.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("threshold", pbjson::private::base64::encode(&self.threshold).as_str())?;
        }
        if !self.sender_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("senderCiphertext", pbjson::private::base64::encode(&self.sender_ciphertext).as_str())?;
        }
        if !self.dleq_proofs.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqProofs", pbjson::private::base64::encode(&self.dleq_proofs).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OutputBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_payload",
            "notePayload",
            "balance_commitment",
            "balanceCommitment",
            "wrapped_memo_key",
            "wrappedMemoKey",
            "ovk_wrapped_key",
            "ovkWrappedKey",
            "compliance_ciphertext",
            "complianceCiphertext",
            "target_timestamp",
            "targetTimestamp",
            "counterparty_leaf_hash",
            "counterpartyLeafHash",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "dk_pub",
            "dkPub",
            "threshold",
            "sender_ciphertext",
            "senderCiphertext",
            "dleq_proofs",
            "dleqProofs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NotePayload,
            BalanceCommitment,
            WrappedMemoKey,
            OvkWrappedKey,
            ComplianceCiphertext,
            TargetTimestamp,
            CounterpartyLeafHash,
            ComplianceAnchor,
            AssetAnchor,
            DkPub,
            Threshold,
            SenderCiphertext,
            DleqProofs,
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
                            "notePayload" | "note_payload" => Ok(GeneratedField::NotePayload),
                            "balanceCommitment" | "balance_commitment" => Ok(GeneratedField::BalanceCommitment),
                            "wrappedMemoKey" | "wrapped_memo_key" => Ok(GeneratedField::WrappedMemoKey),
                            "ovkWrappedKey" | "ovk_wrapped_key" => Ok(GeneratedField::OvkWrappedKey),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "counterpartyLeafHash" | "counterparty_leaf_hash" => Ok(GeneratedField::CounterpartyLeafHash),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "threshold" => Ok(GeneratedField::Threshold),
                            "senderCiphertext" | "sender_ciphertext" => Ok(GeneratedField::SenderCiphertext),
                            "dleqProofs" | "dleq_proofs" => Ok(GeneratedField::DleqProofs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OutputBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.OutputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OutputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_payload__ = None;
                let mut balance_commitment__ = None;
                let mut wrapped_memo_key__ = None;
                let mut ovk_wrapped_key__ = None;
                let mut compliance_ciphertext__ = None;
                let mut target_timestamp__ = None;
                let mut counterparty_leaf_hash__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut dk_pub__ = None;
                let mut threshold__ = None;
                let mut sender_ciphertext__ = None;
                let mut dleq_proofs__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NotePayload => {
                            if note_payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("notePayload"));
                            }
                            note_payload__ = map_.next_value()?;
                        }
                        GeneratedField::BalanceCommitment => {
                            if balance_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balanceCommitment"));
                            }
                            balance_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::WrappedMemoKey => {
                            if wrapped_memo_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("wrappedMemoKey"));
                            }
                            wrapped_memo_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OvkWrappedKey => {
                            if ovk_wrapped_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ovkWrappedKey"));
                            }
                            ovk_wrapped_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetTimestamp => {
                            if target_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTimestamp"));
                            }
                            target_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CounterpartyLeafHash => {
                            if counterparty_leaf_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("counterpartyLeafHash"));
                            }
                            counterparty_leaf_hash__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::DkPub => {
                            if dk_pub__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dkPub"));
                            }
                            dk_pub__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Threshold => {
                            if threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("threshold"));
                            }
                            threshold__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SenderCiphertext => {
                            if sender_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderCiphertext"));
                            }
                            sender_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqProofs => {
                            if dleq_proofs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqProofs"));
                            }
                            dleq_proofs__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(OutputBody {
                    note_payload: note_payload__,
                    balance_commitment: balance_commitment__,
                    wrapped_memo_key: wrapped_memo_key__.unwrap_or_default(),
                    ovk_wrapped_key: ovk_wrapped_key__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    counterparty_leaf_hash: counterparty_leaf_hash__,
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    dk_pub: dk_pub__.unwrap_or_default(),
                    threshold: threshold__.unwrap_or_default(),
                    sender_ciphertext: sender_ciphertext__.unwrap_or_default(),
                    dleq_proofs: dleq_proofs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.OutputBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OutputPlan {
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
        if self.dest_address.is_some() {
            len += 1;
        }
        if !self.rseed.is_empty() {
            len += 1;
        }
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.proof_blinding_r.is_empty() {
            len += 1;
        }
        if !self.proof_blinding_s.is_empty() {
            len += 1;
        }
        if self.target_timestamp != 0 {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if self.is_regulated {
            len += 1;
        }
        if self.compliance_leaf.is_some() {
            len += 1;
        }
        if self.counterparty_leaf.is_some() {
            len += 1;
        }
        if !self.compliance_ephemeral_secret.is_empty() {
            len += 1;
        }
        if self.counterparty_address.is_some() {
            len += 1;
        }
        if !self.tx_blinding_nonce.is_empty() {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
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
        if self.asset_indexed_leaf.is_some() {
            len += 1;
        }
        if !self.sender_ciphertext.is_empty() {
            len += 1;
        }
        if !self.salt.is_empty() {
            len += 1;
        }
        if !self.dleq_k_1.is_empty() {
            len += 1;
        }
        if !self.dleq_k_2.is_empty() {
            len += 1;
        }
        if !self.dleq_k_3.is_empty() {
            len += 1;
        }
        if !self.dleq_c_1.is_empty() {
            len += 1;
        }
        if !self.dleq_s_1.is_empty() {
            len += 1;
        }
        if !self.dleq_c_2.is_empty() {
            len += 1;
        }
        if !self.dleq_s_2.is_empty() {
            len += 1;
        }
        if !self.dleq_c_3.is_empty() {
            len += 1;
        }
        if !self.dleq_s_3.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.threshold_bytes.is_empty() {
            len += 1;
        }
        if self.is_flagged {
            len += 1;
        }
        if !self.r_2.is_empty() {
            len += 1;
        }
        if !self.r_3.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.OutputPlan", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if let Some(v) = self.dest_address.as_ref() {
            struct_ser.serialize_field("destAddress", v)?;
        }
        if !self.rseed.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rseed", pbjson::private::base64::encode(&self.rseed).as_str())?;
        }
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.proof_blinding_r.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("proofBlindingR", pbjson::private::base64::encode(&self.proof_blinding_r).as_str())?;
        }
        if !self.proof_blinding_s.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("proofBlindingS", pbjson::private::base64::encode(&self.proof_blinding_s).as_str())?;
        }
        if self.target_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTimestamp", ToString::to_string(&self.target_timestamp).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if let Some(v) = self.compliance_leaf.as_ref() {
            struct_ser.serialize_field("complianceLeaf", v)?;
        }
        if let Some(v) = self.counterparty_leaf.as_ref() {
            struct_ser.serialize_field("counterpartyLeaf", v)?;
        }
        if !self.compliance_ephemeral_secret.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceEphemeralSecret", pbjson::private::base64::encode(&self.compliance_ephemeral_secret).as_str())?;
        }
        if let Some(v) = self.counterparty_address.as_ref() {
            struct_ser.serialize_field("counterpartyAddress", v)?;
        }
        if !self.tx_blinding_nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("txBlindingNonce", pbjson::private::base64::encode(&self.tx_blinding_nonce).as_str())?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
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
        if let Some(v) = self.asset_indexed_leaf.as_ref() {
            struct_ser.serialize_field("assetIndexedLeaf", v)?;
        }
        if !self.sender_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("senderCiphertext", pbjson::private::base64::encode(&self.sender_ciphertext).as_str())?;
        }
        if !self.salt.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("salt", pbjson::private::base64::encode(&self.salt).as_str())?;
        }
        if !self.dleq_k_1.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqK1", pbjson::private::base64::encode(&self.dleq_k_1).as_str())?;
        }
        if !self.dleq_k_2.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqK2", pbjson::private::base64::encode(&self.dleq_k_2).as_str())?;
        }
        if !self.dleq_k_3.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqK3", pbjson::private::base64::encode(&self.dleq_k_3).as_str())?;
        }
        if !self.dleq_c_1.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqC1", pbjson::private::base64::encode(&self.dleq_c_1).as_str())?;
        }
        if !self.dleq_s_1.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqS1", pbjson::private::base64::encode(&self.dleq_s_1).as_str())?;
        }
        if !self.dleq_c_2.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqC2", pbjson::private::base64::encode(&self.dleq_c_2).as_str())?;
        }
        if !self.dleq_s_2.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqS2", pbjson::private::base64::encode(&self.dleq_s_2).as_str())?;
        }
        if !self.dleq_c_3.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqC3", pbjson::private::base64::encode(&self.dleq_c_3).as_str())?;
        }
        if !self.dleq_s_3.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqS3", pbjson::private::base64::encode(&self.dleq_s_3).as_str())?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.dk_pub.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dkPub", pbjson::private::base64::encode(&self.dk_pub).as_str())?;
        }
        if !self.threshold_bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("thresholdBytes", pbjson::private::base64::encode(&self.threshold_bytes).as_str())?;
        }
        if self.is_flagged {
            struct_ser.serialize_field("isFlagged", &self.is_flagged)?;
        }
        if !self.r_2.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("r2", pbjson::private::base64::encode(&self.r_2).as_str())?;
        }
        if !self.r_3.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("r3", pbjson::private::base64::encode(&self.r_3).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OutputPlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "dest_address",
            "destAddress",
            "rseed",
            "value_blinding",
            "valueBlinding",
            "proof_blinding_r",
            "proofBlindingR",
            "proof_blinding_s",
            "proofBlindingS",
            "target_timestamp",
            "targetTimestamp",
            "compliance_ciphertext",
            "complianceCiphertext",
            "is_regulated",
            "isRegulated",
            "compliance_leaf",
            "complianceLeaf",
            "counterparty_leaf",
            "counterpartyLeaf",
            "compliance_ephemeral_secret",
            "complianceEphemeralSecret",
            "counterparty_address",
            "counterpartyAddress",
            "tx_blinding_nonce",
            "txBlindingNonce",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "compliance_path",
            "compliancePath",
            "compliance_position",
            "compliancePosition",
            "asset_path",
            "assetPath",
            "asset_position",
            "assetPosition",
            "asset_indexed_leaf",
            "assetIndexedLeaf",
            "sender_ciphertext",
            "senderCiphertext",
            "salt",
            "dleq_k_1",
            "dleqK1",
            "dleq_k_2",
            "dleqK2",
            "dleq_k_3",
            "dleqK3",
            "dleq_c_1",
            "dleqC1",
            "dleq_s_1",
            "dleqS1",
            "dleq_c_2",
            "dleqC2",
            "dleq_s_2",
            "dleqS2",
            "dleq_c_3",
            "dleqC3",
            "dleq_s_3",
            "dleqS3",
            "ring_pk",
            "ringPk",
            "dk_pub",
            "dkPub",
            "threshold_bytes",
            "thresholdBytes",
            "is_flagged",
            "isFlagged",
            "r_2",
            "r2",
            "r_3",
            "r3",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            DestAddress,
            Rseed,
            ValueBlinding,
            ProofBlindingR,
            ProofBlindingS,
            TargetTimestamp,
            ComplianceCiphertext,
            IsRegulated,
            ComplianceLeaf,
            CounterpartyLeaf,
            ComplianceEphemeralSecret,
            CounterpartyAddress,
            TxBlindingNonce,
            ComplianceAnchor,
            AssetAnchor,
            CompliancePath,
            CompliancePosition,
            AssetPath,
            AssetPosition,
            AssetIndexedLeaf,
            SenderCiphertext,
            Salt,
            DleqK1,
            DleqK2,
            DleqK3,
            DleqC1,
            DleqS1,
            DleqC2,
            DleqS2,
            DleqC3,
            DleqS3,
            RingPk,
            DkPub,
            ThresholdBytes,
            IsFlagged,
            R2,
            R3,
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
                            "destAddress" | "dest_address" => Ok(GeneratedField::DestAddress),
                            "rseed" => Ok(GeneratedField::Rseed),
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "proofBlindingR" | "proof_blinding_r" => Ok(GeneratedField::ProofBlindingR),
                            "proofBlindingS" | "proof_blinding_s" => Ok(GeneratedField::ProofBlindingS),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "complianceLeaf" | "compliance_leaf" => Ok(GeneratedField::ComplianceLeaf),
                            "counterpartyLeaf" | "counterparty_leaf" => Ok(GeneratedField::CounterpartyLeaf),
                            "complianceEphemeralSecret" | "compliance_ephemeral_secret" => Ok(GeneratedField::ComplianceEphemeralSecret),
                            "counterpartyAddress" | "counterparty_address" => Ok(GeneratedField::CounterpartyAddress),
                            "txBlindingNonce" | "tx_blinding_nonce" => Ok(GeneratedField::TxBlindingNonce),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "compliancePath" | "compliance_path" => Ok(GeneratedField::CompliancePath),
                            "compliancePosition" | "compliance_position" => Ok(GeneratedField::CompliancePosition),
                            "assetPath" | "asset_path" => Ok(GeneratedField::AssetPath),
                            "assetPosition" | "asset_position" => Ok(GeneratedField::AssetPosition),
                            "assetIndexedLeaf" | "asset_indexed_leaf" => Ok(GeneratedField::AssetIndexedLeaf),
                            "senderCiphertext" | "sender_ciphertext" => Ok(GeneratedField::SenderCiphertext),
                            "salt" => Ok(GeneratedField::Salt),
                            "dleqK1" | "dleq_k_1" => Ok(GeneratedField::DleqK1),
                            "dleqK2" | "dleq_k_2" => Ok(GeneratedField::DleqK2),
                            "dleqK3" | "dleq_k_3" => Ok(GeneratedField::DleqK3),
                            "dleqC1" | "dleq_c_1" => Ok(GeneratedField::DleqC1),
                            "dleqS1" | "dleq_s_1" => Ok(GeneratedField::DleqS1),
                            "dleqC2" | "dleq_c_2" => Ok(GeneratedField::DleqC2),
                            "dleqS2" | "dleq_s_2" => Ok(GeneratedField::DleqS2),
                            "dleqC3" | "dleq_c_3" => Ok(GeneratedField::DleqC3),
                            "dleqS3" | "dleq_s_3" => Ok(GeneratedField::DleqS3),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "thresholdBytes" | "threshold_bytes" => Ok(GeneratedField::ThresholdBytes),
                            "isFlagged" | "is_flagged" => Ok(GeneratedField::IsFlagged),
                            "r2" | "r_2" => Ok(GeneratedField::R2),
                            "r3" | "r_3" => Ok(GeneratedField::R3),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OutputPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.OutputPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OutputPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut dest_address__ = None;
                let mut rseed__ = None;
                let mut value_blinding__ = None;
                let mut proof_blinding_r__ = None;
                let mut proof_blinding_s__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_ciphertext__ = None;
                let mut is_regulated__ = None;
                let mut compliance_leaf__ = None;
                let mut counterparty_leaf__ = None;
                let mut compliance_ephemeral_secret__ = None;
                let mut counterparty_address__ = None;
                let mut tx_blinding_nonce__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut compliance_path__ = None;
                let mut compliance_position__ = None;
                let mut asset_path__ = None;
                let mut asset_position__ = None;
                let mut asset_indexed_leaf__ = None;
                let mut sender_ciphertext__ = None;
                let mut salt__ = None;
                let mut dleq_k_1__ = None;
                let mut dleq_k_2__ = None;
                let mut dleq_k_3__ = None;
                let mut dleq_c_1__ = None;
                let mut dleq_s_1__ = None;
                let mut dleq_c_2__ = None;
                let mut dleq_s_2__ = None;
                let mut dleq_c_3__ = None;
                let mut dleq_s_3__ = None;
                let mut ring_pk__ = None;
                let mut dk_pub__ = None;
                let mut threshold_bytes__ = None;
                let mut is_flagged__ = None;
                let mut r_2__ = None;
                let mut r_3__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::DestAddress => {
                            if dest_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("destAddress"));
                            }
                            dest_address__ = map_.next_value()?;
                        }
                        GeneratedField::Rseed => {
                            if rseed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rseed"));
                            }
                            rseed__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ProofBlindingR => {
                            if proof_blinding_r__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proofBlindingR"));
                            }
                            proof_blinding_r__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ProofBlindingS => {
                            if proof_blinding_s__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proofBlindingS"));
                            }
                            proof_blinding_s__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetTimestamp => {
                            if target_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTimestamp"));
                            }
                            target_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceLeaf => {
                            if compliance_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceLeaf"));
                            }
                            compliance_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::CounterpartyLeaf => {
                            if counterparty_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("counterpartyLeaf"));
                            }
                            counterparty_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceEphemeralSecret => {
                            if compliance_ephemeral_secret__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceEphemeralSecret"));
                            }
                            compliance_ephemeral_secret__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CounterpartyAddress => {
                            if counterparty_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("counterpartyAddress"));
                            }
                            counterparty_address__ = map_.next_value()?;
                        }
                        GeneratedField::TxBlindingNonce => {
                            if tx_blinding_nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("txBlindingNonce"));
                            }
                            tx_blinding_nonce__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
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
                        GeneratedField::AssetIndexedLeaf => {
                            if asset_indexed_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetIndexedLeaf"));
                            }
                            asset_indexed_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::SenderCiphertext => {
                            if sender_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderCiphertext"));
                            }
                            sender_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Salt => {
                            if salt__.is_some() {
                                return Err(serde::de::Error::duplicate_field("salt"));
                            }
                            salt__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqK1 => {
                            if dleq_k_1__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqK1"));
                            }
                            dleq_k_1__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqK2 => {
                            if dleq_k_2__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqK2"));
                            }
                            dleq_k_2__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqK3 => {
                            if dleq_k_3__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqK3"));
                            }
                            dleq_k_3__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqC1 => {
                            if dleq_c_1__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqC1"));
                            }
                            dleq_c_1__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqS1 => {
                            if dleq_s_1__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqS1"));
                            }
                            dleq_s_1__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqC2 => {
                            if dleq_c_2__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqC2"));
                            }
                            dleq_c_2__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqS2 => {
                            if dleq_s_2__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqS2"));
                            }
                            dleq_s_2__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqC3 => {
                            if dleq_c_3__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqC3"));
                            }
                            dleq_c_3__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqS3 => {
                            if dleq_s_3__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqS3"));
                            }
                            dleq_s_3__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DkPub => {
                            if dk_pub__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dkPub"));
                            }
                            dk_pub__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ThresholdBytes => {
                            if threshold_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("thresholdBytes"));
                            }
                            threshold_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsFlagged => {
                            if is_flagged__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isFlagged"));
                            }
                            is_flagged__ = Some(map_.next_value()?);
                        }
                        GeneratedField::R2 => {
                            if r_2__.is_some() {
                                return Err(serde::de::Error::duplicate_field("r2"));
                            }
                            r_2__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::R3 => {
                            if r_3__.is_some() {
                                return Err(serde::de::Error::duplicate_field("r3"));
                            }
                            r_3__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(OutputPlan {
                    value: value__,
                    dest_address: dest_address__,
                    rseed: rseed__.unwrap_or_default(),
                    value_blinding: value_blinding__.unwrap_or_default(),
                    proof_blinding_r: proof_blinding_r__.unwrap_or_default(),
                    proof_blinding_s: proof_blinding_s__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    is_regulated: is_regulated__.unwrap_or_default(),
                    compliance_leaf: compliance_leaf__,
                    counterparty_leaf: counterparty_leaf__,
                    compliance_ephemeral_secret: compliance_ephemeral_secret__.unwrap_or_default(),
                    counterparty_address: counterparty_address__,
                    tx_blinding_nonce: tx_blinding_nonce__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    compliance_path: compliance_path__,
                    compliance_position: compliance_position__.unwrap_or_default(),
                    asset_path: asset_path__,
                    asset_position: asset_position__.unwrap_or_default(),
                    asset_indexed_leaf: asset_indexed_leaf__,
                    sender_ciphertext: sender_ciphertext__.unwrap_or_default(),
                    salt: salt__.unwrap_or_default(),
                    dleq_k_1: dleq_k_1__.unwrap_or_default(),
                    dleq_k_2: dleq_k_2__.unwrap_or_default(),
                    dleq_k_3: dleq_k_3__.unwrap_or_default(),
                    dleq_c_1: dleq_c_1__.unwrap_or_default(),
                    dleq_s_1: dleq_s_1__.unwrap_or_default(),
                    dleq_c_2: dleq_c_2__.unwrap_or_default(),
                    dleq_s_2: dleq_s_2__.unwrap_or_default(),
                    dleq_c_3: dleq_c_3__.unwrap_or_default(),
                    dleq_s_3: dleq_s_3__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    threshold_bytes: threshold_bytes__.unwrap_or_default(),
                    is_flagged: is_flagged__.unwrap_or_default(),
                    r_2: r_2__.unwrap_or_default(),
                    r_3: r_3__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.OutputPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OutputView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.output_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.OutputView", len)?;
        if let Some(v) = self.output_view.as_ref() {
            match v {
                output_view::OutputView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                output_view::OutputView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OutputView {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "visible",
            "opaque",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Visible,
            Opaque,
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
                            "visible" => Ok(GeneratedField::Visible),
                            "opaque" => Ok(GeneratedField::Opaque),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OutputView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.OutputView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OutputView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut output_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if output_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            output_view__ = map_.next_value::<::std::option::Option<_>>()?.map(output_view::OutputView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if output_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            output_view__ = map_.next_value::<::std::option::Option<_>>()?.map(output_view::OutputView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(OutputView {
                    output_view: output_view__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.OutputView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for output_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.OutputView.Opaque", len)?;
        if let Some(v) = self.output.as_ref() {
            struct_ser.serialize_field("output", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for output_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "output",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Output,
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
                            "output" => Ok(GeneratedField::Output),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = output_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.OutputView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<output_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut output__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Output => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("output"));
                            }
                            output__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(output_view::Opaque {
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.OutputView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for output_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.output.is_some() {
            len += 1;
        }
        if self.note.is_some() {
            len += 1;
        }
        if self.payload_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.OutputView.Visible", len)?;
        if let Some(v) = self.output.as_ref() {
            struct_ser.serialize_field("output", v)?;
        }
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        if let Some(v) = self.payload_key.as_ref() {
            struct_ser.serialize_field("payloadKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for output_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "output",
            "note",
            "payload_key",
            "payloadKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Output,
            Note,
            PayloadKey,
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
                            "output" => Ok(GeneratedField::Output),
                            "note" => Ok(GeneratedField::Note),
                            "payloadKey" | "payload_key" => Ok(GeneratedField::PayloadKey),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = output_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.OutputView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<output_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut output__ = None;
                let mut note__ = None;
                let mut payload_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Output => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("output"));
                            }
                            output__ = map_.next_value()?;
                        }
                        GeneratedField::Note => {
                            if note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            note__ = map_.next_value()?;
                        }
                        GeneratedField::PayloadKey => {
                            if payload_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payloadKey"));
                            }
                            payload_key__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(output_view::Visible {
                    output: output__,
                    note: note__,
                    payload_key: payload_key__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.OutputView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedPoolParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.fixed_fmd_params.is_some() {
            len += 1;
        }
        if self.fmd_meta_params.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.ShieldedPoolParameters", len)?;
        if let Some(v) = self.fixed_fmd_params.as_ref() {
            struct_ser.serialize_field("fixedFmdParams", v)?;
        }
        if let Some(v) = self.fmd_meta_params.as_ref() {
            struct_ser.serialize_field("fmdMetaParams", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedPoolParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fixed_fmd_params",
            "fixedFmdParams",
            "fmd_meta_params",
            "fmdMetaParams",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FixedFmdParams,
            FmdMetaParams,
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
                            "fixedFmdParams" | "fixed_fmd_params" => Ok(GeneratedField::FixedFmdParams),
                            "fmdMetaParams" | "fmd_meta_params" => Ok(GeneratedField::FmdMetaParams),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedPoolParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.ShieldedPoolParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedPoolParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut fixed_fmd_params__ = None;
                let mut fmd_meta_params__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FixedFmdParams => {
                            if fixed_fmd_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fixedFmdParams"));
                            }
                            fixed_fmd_params__ = map_.next_value()?;
                        }
                        GeneratedField::FmdMetaParams => {
                            if fmd_meta_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fmdMetaParams"));
                            }
                            fmd_meta_params__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedPoolParameters {
                    fixed_fmd_params: fixed_fmd_params__,
                    fmd_meta_params: fmd_meta_params__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.ShieldedPoolParameters", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Spend {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.body.is_some() {
            len += 1;
        }
        if self.auth_sig.is_some() {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.Spend", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if let Some(v) = self.auth_sig.as_ref() {
            struct_ser.serialize_field("authSig", v)?;
        }
        if let Some(v) = self.proof.as_ref() {
            struct_ser.serialize_field("proof", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Spend {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "auth_sig",
            "authSig",
            "proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            AuthSig,
            Proof,
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
                            "body" => Ok(GeneratedField::Body),
                            "authSig" | "auth_sig" => Ok(GeneratedField::AuthSig),
                            "proof" => Ok(GeneratedField::Proof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Spend;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.Spend")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Spend, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut auth_sig__ = None;
                let mut proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::AuthSig => {
                            if auth_sig__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authSig"));
                            }
                            auth_sig__ = map_.next_value()?;
                        }
                        GeneratedField::Proof => {
                            if proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proof"));
                            }
                            proof__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Spend {
                    body: body__,
                    auth_sig: auth_sig__,
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.Spend", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpendBody {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.balance_commitment.is_some() {
            len += 1;
        }
        if self.nullifier.is_some() {
            len += 1;
        }
        if self.rk.is_some() {
            len += 1;
        }
        if !self.encrypted_backref.is_empty() {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if self.target_timestamp != 0 {
            len += 1;
        }
        if self.sender_leaf_hash.is_some() {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
            len += 1;
        }
        if !self.dleq_proof.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.SpendBody", len)?;
        if let Some(v) = self.balance_commitment.as_ref() {
            struct_ser.serialize_field("balanceCommitment", v)?;
        }
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if let Some(v) = self.rk.as_ref() {
            struct_ser.serialize_field("rk", v)?;
        }
        if !self.encrypted_backref.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("encryptedBackref", pbjson::private::base64::encode(&self.encrypted_backref).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if self.target_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTimestamp", ToString::to_string(&self.target_timestamp).as_str())?;
        }
        if let Some(v) = self.sender_leaf_hash.as_ref() {
            struct_ser.serialize_field("senderLeafHash", v)?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
        }
        if !self.dleq_proof.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqProof", pbjson::private::base64::encode(&self.dleq_proof).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpendBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "balance_commitment",
            "balanceCommitment",
            "nullifier",
            "rk",
            "encrypted_backref",
            "encryptedBackref",
            "compliance_ciphertext",
            "complianceCiphertext",
            "target_timestamp",
            "targetTimestamp",
            "sender_leaf_hash",
            "senderLeafHash",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "dleq_proof",
            "dleqProof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BalanceCommitment,
            Nullifier,
            Rk,
            EncryptedBackref,
            ComplianceCiphertext,
            TargetTimestamp,
            SenderLeafHash,
            ComplianceAnchor,
            AssetAnchor,
            DleqProof,
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
                            "balanceCommitment" | "balance_commitment" => Ok(GeneratedField::BalanceCommitment),
                            "nullifier" => Ok(GeneratedField::Nullifier),
                            "rk" => Ok(GeneratedField::Rk),
                            "encryptedBackref" | "encrypted_backref" => Ok(GeneratedField::EncryptedBackref),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "senderLeafHash" | "sender_leaf_hash" => Ok(GeneratedField::SenderLeafHash),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "dleqProof" | "dleq_proof" => Ok(GeneratedField::DleqProof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpendBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.SpendBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SpendBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut balance_commitment__ = None;
                let mut nullifier__ = None;
                let mut rk__ = None;
                let mut encrypted_backref__ = None;
                let mut compliance_ciphertext__ = None;
                let mut target_timestamp__ = None;
                let mut sender_leaf_hash__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut dleq_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BalanceCommitment => {
                            if balance_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balanceCommitment"));
                            }
                            balance_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::Rk => {
                            if rk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rk"));
                            }
                            rk__ = map_.next_value()?;
                        }
                        GeneratedField::EncryptedBackref => {
                            if encrypted_backref__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryptedBackref"));
                            }
                            encrypted_backref__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetTimestamp => {
                            if target_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTimestamp"));
                            }
                            target_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SenderLeafHash => {
                            if sender_leaf_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderLeafHash"));
                            }
                            sender_leaf_hash__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::DleqProof => {
                            if dleq_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqProof"));
                            }
                            dleq_proof__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SpendBody {
                    balance_commitment: balance_commitment__,
                    nullifier: nullifier__,
                    rk: rk__,
                    encrypted_backref: encrypted_backref__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    sender_leaf_hash: sender_leaf_hash__,
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    dleq_proof: dleq_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.SpendBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpendPlan {
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
        if self.position != 0 {
            len += 1;
        }
        if !self.randomizer.is_empty() {
            len += 1;
        }
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.proof_blinding_r.is_empty() {
            len += 1;
        }
        if !self.proof_blinding_s.is_empty() {
            len += 1;
        }
        if self.target_timestamp != 0 {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if self.is_regulated {
            len += 1;
        }
        if self.compliance_leaf.is_some() {
            len += 1;
        }
        if !self.compliance_ephemeral_secret.is_empty() {
            len += 1;
        }
        if !self.tx_blinding_nonce.is_empty() {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
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
        if self.asset_indexed_leaf.is_some() {
            len += 1;
        }
        if self.is_flagged {
            len += 1;
        }
        if !self.salt.is_empty() {
            len += 1;
        }
        if !self.dleq_k.is_empty() {
            len += 1;
        }
        if !self.dleq_c.is_empty() {
            len += 1;
        }
        if !self.dleq_s.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.threshold.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.SpendPlan", len)?;
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if !self.randomizer.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("randomizer", pbjson::private::base64::encode(&self.randomizer).as_str())?;
        }
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.proof_blinding_r.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("proofBlindingR", pbjson::private::base64::encode(&self.proof_blinding_r).as_str())?;
        }
        if !self.proof_blinding_s.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("proofBlindingS", pbjson::private::base64::encode(&self.proof_blinding_s).as_str())?;
        }
        if self.target_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTimestamp", ToString::to_string(&self.target_timestamp).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if let Some(v) = self.compliance_leaf.as_ref() {
            struct_ser.serialize_field("complianceLeaf", v)?;
        }
        if !self.compliance_ephemeral_secret.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceEphemeralSecret", pbjson::private::base64::encode(&self.compliance_ephemeral_secret).as_str())?;
        }
        if !self.tx_blinding_nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("txBlindingNonce", pbjson::private::base64::encode(&self.tx_blinding_nonce).as_str())?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
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
        if let Some(v) = self.asset_indexed_leaf.as_ref() {
            struct_ser.serialize_field("assetIndexedLeaf", v)?;
        }
        if self.is_flagged {
            struct_ser.serialize_field("isFlagged", &self.is_flagged)?;
        }
        if !self.salt.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("salt", pbjson::private::base64::encode(&self.salt).as_str())?;
        }
        if !self.dleq_k.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqK", pbjson::private::base64::encode(&self.dleq_k).as_str())?;
        }
        if !self.dleq_c.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqC", pbjson::private::base64::encode(&self.dleq_c).as_str())?;
        }
        if !self.dleq_s.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqS", pbjson::private::base64::encode(&self.dleq_s).as_str())?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.dk_pub.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dkPub", pbjson::private::base64::encode(&self.dk_pub).as_str())?;
        }
        if !self.threshold.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("threshold", pbjson::private::base64::encode(&self.threshold).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpendPlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note",
            "position",
            "randomizer",
            "value_blinding",
            "valueBlinding",
            "proof_blinding_r",
            "proofBlindingR",
            "proof_blinding_s",
            "proofBlindingS",
            "target_timestamp",
            "targetTimestamp",
            "compliance_ciphertext",
            "complianceCiphertext",
            "is_regulated",
            "isRegulated",
            "compliance_leaf",
            "complianceLeaf",
            "compliance_ephemeral_secret",
            "complianceEphemeralSecret",
            "tx_blinding_nonce",
            "txBlindingNonce",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "compliance_path",
            "compliancePath",
            "compliance_position",
            "compliancePosition",
            "asset_path",
            "assetPath",
            "asset_position",
            "assetPosition",
            "asset_indexed_leaf",
            "assetIndexedLeaf",
            "is_flagged",
            "isFlagged",
            "salt",
            "dleq_k",
            "dleqK",
            "dleq_c",
            "dleqC",
            "dleq_s",
            "dleqS",
            "ring_pk",
            "ringPk",
            "dk_pub",
            "dkPub",
            "threshold",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Note,
            Position,
            Randomizer,
            ValueBlinding,
            ProofBlindingR,
            ProofBlindingS,
            TargetTimestamp,
            ComplianceCiphertext,
            IsRegulated,
            ComplianceLeaf,
            ComplianceEphemeralSecret,
            TxBlindingNonce,
            ComplianceAnchor,
            AssetAnchor,
            CompliancePath,
            CompliancePosition,
            AssetPath,
            AssetPosition,
            AssetIndexedLeaf,
            IsFlagged,
            Salt,
            DleqK,
            DleqC,
            DleqS,
            RingPk,
            DkPub,
            Threshold,
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
                            "position" => Ok(GeneratedField::Position),
                            "randomizer" => Ok(GeneratedField::Randomizer),
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "proofBlindingR" | "proof_blinding_r" => Ok(GeneratedField::ProofBlindingR),
                            "proofBlindingS" | "proof_blinding_s" => Ok(GeneratedField::ProofBlindingS),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "complianceLeaf" | "compliance_leaf" => Ok(GeneratedField::ComplianceLeaf),
                            "complianceEphemeralSecret" | "compliance_ephemeral_secret" => Ok(GeneratedField::ComplianceEphemeralSecret),
                            "txBlindingNonce" | "tx_blinding_nonce" => Ok(GeneratedField::TxBlindingNonce),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "compliancePath" | "compliance_path" => Ok(GeneratedField::CompliancePath),
                            "compliancePosition" | "compliance_position" => Ok(GeneratedField::CompliancePosition),
                            "assetPath" | "asset_path" => Ok(GeneratedField::AssetPath),
                            "assetPosition" | "asset_position" => Ok(GeneratedField::AssetPosition),
                            "assetIndexedLeaf" | "asset_indexed_leaf" => Ok(GeneratedField::AssetIndexedLeaf),
                            "isFlagged" | "is_flagged" => Ok(GeneratedField::IsFlagged),
                            "salt" => Ok(GeneratedField::Salt),
                            "dleqK" | "dleq_k" => Ok(GeneratedField::DleqK),
                            "dleqC" | "dleq_c" => Ok(GeneratedField::DleqC),
                            "dleqS" | "dleq_s" => Ok(GeneratedField::DleqS),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "threshold" => Ok(GeneratedField::Threshold),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpendPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.SpendPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SpendPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note__ = None;
                let mut position__ = None;
                let mut randomizer__ = None;
                let mut value_blinding__ = None;
                let mut proof_blinding_r__ = None;
                let mut proof_blinding_s__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_ciphertext__ = None;
                let mut is_regulated__ = None;
                let mut compliance_leaf__ = None;
                let mut compliance_ephemeral_secret__ = None;
                let mut tx_blinding_nonce__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut compliance_path__ = None;
                let mut compliance_position__ = None;
                let mut asset_path__ = None;
                let mut asset_position__ = None;
                let mut asset_indexed_leaf__ = None;
                let mut is_flagged__ = None;
                let mut salt__ = None;
                let mut dleq_k__ = None;
                let mut dleq_c__ = None;
                let mut dleq_s__ = None;
                let mut ring_pk__ = None;
                let mut dk_pub__ = None;
                let mut threshold__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Note => {
                            if note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            note__ = map_.next_value()?;
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Randomizer => {
                            if randomizer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("randomizer"));
                            }
                            randomizer__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ProofBlindingR => {
                            if proof_blinding_r__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proofBlindingR"));
                            }
                            proof_blinding_r__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ProofBlindingS => {
                            if proof_blinding_s__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proofBlindingS"));
                            }
                            proof_blinding_s__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetTimestamp => {
                            if target_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTimestamp"));
                            }
                            target_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceLeaf => {
                            if compliance_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceLeaf"));
                            }
                            compliance_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceEphemeralSecret => {
                            if compliance_ephemeral_secret__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceEphemeralSecret"));
                            }
                            compliance_ephemeral_secret__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TxBlindingNonce => {
                            if tx_blinding_nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("txBlindingNonce"));
                            }
                            tx_blinding_nonce__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
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
                        GeneratedField::AssetIndexedLeaf => {
                            if asset_indexed_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetIndexedLeaf"));
                            }
                            asset_indexed_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::IsFlagged => {
                            if is_flagged__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isFlagged"));
                            }
                            is_flagged__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Salt => {
                            if salt__.is_some() {
                                return Err(serde::de::Error::duplicate_field("salt"));
                            }
                            salt__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqK => {
                            if dleq_k__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqK"));
                            }
                            dleq_k__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqC => {
                            if dleq_c__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqC"));
                            }
                            dleq_c__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqS => {
                            if dleq_s__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqS"));
                            }
                            dleq_s__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DkPub => {
                            if dk_pub__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dkPub"));
                            }
                            dk_pub__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Threshold => {
                            if threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("threshold"));
                            }
                            threshold__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SpendPlan {
                    note: note__,
                    position: position__.unwrap_or_default(),
                    randomizer: randomizer__.unwrap_or_default(),
                    value_blinding: value_blinding__.unwrap_or_default(),
                    proof_blinding_r: proof_blinding_r__.unwrap_or_default(),
                    proof_blinding_s: proof_blinding_s__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    is_regulated: is_regulated__.unwrap_or_default(),
                    compliance_leaf: compliance_leaf__,
                    compliance_ephemeral_secret: compliance_ephemeral_secret__.unwrap_or_default(),
                    tx_blinding_nonce: tx_blinding_nonce__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    compliance_path: compliance_path__,
                    compliance_position: compliance_position__.unwrap_or_default(),
                    asset_path: asset_path__,
                    asset_position: asset_position__.unwrap_or_default(),
                    asset_indexed_leaf: asset_indexed_leaf__,
                    is_flagged: is_flagged__.unwrap_or_default(),
                    salt: salt__.unwrap_or_default(),
                    dleq_k: dleq_k__.unwrap_or_default(),
                    dleq_c: dleq_c__.unwrap_or_default(),
                    dleq_s: dleq_s__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    threshold: threshold__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.SpendPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpendView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.spend_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.SpendView", len)?;
        if let Some(v) = self.spend_view.as_ref() {
            match v {
                spend_view::SpendView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                spend_view::SpendView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpendView {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "visible",
            "opaque",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Visible,
            Opaque,
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
                            "visible" => Ok(GeneratedField::Visible),
                            "opaque" => Ok(GeneratedField::Opaque),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpendView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.SpendView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SpendView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spend_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if spend_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            spend_view__ = map_.next_value::<::std::option::Option<_>>()?.map(spend_view::SpendView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if spend_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            spend_view__ = map_.next_value::<::std::option::Option<_>>()?.map(spend_view::SpendView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SpendView {
                    spend_view: spend_view__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.SpendView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for spend_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.spend.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.SpendView.Opaque", len)?;
        if let Some(v) = self.spend.as_ref() {
            struct_ser.serialize_field("spend", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for spend_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "spend",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Spend,
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
                            "spend" => Ok(GeneratedField::Spend),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = spend_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.SpendView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<spend_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spend__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Spend => {
                            if spend__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spend"));
                            }
                            spend__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(spend_view::Opaque {
                    spend: spend__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.SpendView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for spend_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.spend.is_some() {
            len += 1;
        }
        if self.note.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.SpendView.Visible", len)?;
        if let Some(v) = self.spend.as_ref() {
            struct_ser.serialize_field("spend", v)?;
        }
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for spend_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "spend",
            "note",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Spend,
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
                            "spend" => Ok(GeneratedField::Spend),
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
            type Value = spend_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.SpendView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<spend_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spend__ = None;
                let mut note__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Spend => {
                            if spend__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spend"));
                            }
                            spend__ = map_.next_value()?;
                        }
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
                Ok(spend_view::Visible {
                    spend: spend__,
                    note: note__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.SpendView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Transfer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.body.is_some() {
            len += 1;
        }
        if !self.auth_sigs.is_empty() {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.Transfer", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if !self.auth_sigs.is_empty() {
            struct_ser.serialize_field("authSigs", &self.auth_sigs)?;
        }
        if let Some(v) = self.proof.as_ref() {
            struct_ser.serialize_field("proof", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Transfer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "auth_sigs",
            "authSigs",
            "proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            AuthSigs,
            Proof,
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
                            "body" => Ok(GeneratedField::Body),
                            "authSigs" | "auth_sigs" => Ok(GeneratedField::AuthSigs),
                            "proof" => Ok(GeneratedField::Proof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Transfer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.Transfer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Transfer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut auth_sigs__ = None;
                let mut proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::AuthSigs => {
                            if auth_sigs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authSigs"));
                            }
                            auth_sigs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Proof => {
                            if proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proof"));
                            }
                            proof__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Transfer {
                    body: body__,
                    auth_sigs: auth_sigs__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.Transfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferBody {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.family_id != 0 {
            len += 1;
        }
        if self.anchor.is_some() {
            len += 1;
        }
        if self.balance_commitment.is_some() {
            len += 1;
        }
        if !self.inputs.is_empty() {
            len += 1;
        }
        if !self.outputs.is_empty() {
            len += 1;
        }
        if self.target_timestamp != 0 {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferBody", len)?;
        if self.family_id != 0 {
            struct_ser.serialize_field("familyId", &self.family_id)?;
        }
        if let Some(v) = self.anchor.as_ref() {
            struct_ser.serialize_field("anchor", v)?;
        }
        if let Some(v) = self.balance_commitment.as_ref() {
            struct_ser.serialize_field("balanceCommitment", v)?;
        }
        if !self.inputs.is_empty() {
            struct_ser.serialize_field("inputs", &self.inputs)?;
        }
        if !self.outputs.is_empty() {
            struct_ser.serialize_field("outputs", &self.outputs)?;
        }
        if self.target_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTimestamp", ToString::to_string(&self.target_timestamp).as_str())?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "family_id",
            "familyId",
            "anchor",
            "balance_commitment",
            "balanceCommitment",
            "inputs",
            "outputs",
            "target_timestamp",
            "targetTimestamp",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FamilyId,
            Anchor,
            BalanceCommitment,
            Inputs,
            Outputs,
            TargetTimestamp,
            ComplianceAnchor,
            AssetAnchor,
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
                            "familyId" | "family_id" => Ok(GeneratedField::FamilyId),
                            "anchor" => Ok(GeneratedField::Anchor),
                            "balanceCommitment" | "balance_commitment" => Ok(GeneratedField::BalanceCommitment),
                            "inputs" => Ok(GeneratedField::Inputs),
                            "outputs" => Ok(GeneratedField::Outputs),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut family_id__ = None;
                let mut anchor__ = None;
                let mut balance_commitment__ = None;
                let mut inputs__ = None;
                let mut outputs__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FamilyId => {
                            if family_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("familyId"));
                            }
                            family_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Anchor => {
                            if anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anchor"));
                            }
                            anchor__ = map_.next_value()?;
                        }
                        GeneratedField::BalanceCommitment => {
                            if balance_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balanceCommitment"));
                            }
                            balance_commitment__ = map_.next_value()?;
                        }
                        GeneratedField::Inputs => {
                            if inputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputs"));
                            }
                            inputs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Outputs => {
                            if outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputs"));
                            }
                            outputs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TargetTimestamp => {
                            if target_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTimestamp"));
                            }
                            target_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferBody {
                    family_id: family_id__.unwrap_or_default(),
                    anchor: anchor__,
                    balance_commitment: balance_commitment__,
                    inputs: inputs__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferInputBody {
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
        if self.rk.is_some() {
            len += 1;
        }
        if !self.encrypted_backref.is_empty() {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if !self.dleq_proof.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferInputBody", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if let Some(v) = self.rk.as_ref() {
            struct_ser.serialize_field("rk", v)?;
        }
        if !self.encrypted_backref.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("encryptedBackref", pbjson::private::base64::encode(&self.encrypted_backref).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if !self.dleq_proof.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqProof", pbjson::private::base64::encode(&self.dleq_proof).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferInputBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nullifier",
            "rk",
            "encrypted_backref",
            "encryptedBackref",
            "compliance_ciphertext",
            "complianceCiphertext",
            "dleq_proof",
            "dleqProof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            Rk,
            EncryptedBackref,
            ComplianceCiphertext,
            DleqProof,
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
                            "rk" => Ok(GeneratedField::Rk),
                            "encryptedBackref" | "encrypted_backref" => Ok(GeneratedField::EncryptedBackref),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "dleqProof" | "dleq_proof" => Ok(GeneratedField::DleqProof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferInputBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferInputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferInputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut rk__ = None;
                let mut encrypted_backref__ = None;
                let mut compliance_ciphertext__ = None;
                let mut dleq_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::Rk => {
                            if rk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rk"));
                            }
                            rk__ = map_.next_value()?;
                        }
                        GeneratedField::EncryptedBackref => {
                            if encrypted_backref__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryptedBackref"));
                            }
                            encrypted_backref__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqProof => {
                            if dleq_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqProof"));
                            }
                            dleq_proof__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferInputBody {
                    nullifier: nullifier__,
                    rk: rk__,
                    encrypted_backref: encrypted_backref__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    dleq_proof: dleq_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferInputBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferOutputBody {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_payload.is_some() {
            len += 1;
        }
        if !self.wrapped_memo_key.is_empty() {
            len += 1;
        }
        if !self.ovk_wrapped_key.is_empty() {
            len += 1;
        }
        if !self.compliance_ciphertext.is_empty() {
            len += 1;
        }
        if !self.dleq_proofs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferOutputBody", len)?;
        if let Some(v) = self.note_payload.as_ref() {
            struct_ser.serialize_field("notePayload", v)?;
        }
        if !self.wrapped_memo_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("wrappedMemoKey", pbjson::private::base64::encode(&self.wrapped_memo_key).as_str())?;
        }
        if !self.ovk_wrapped_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ovkWrappedKey", pbjson::private::base64::encode(&self.ovk_wrapped_key).as_str())?;
        }
        if !self.compliance_ciphertext.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceCiphertext", pbjson::private::base64::encode(&self.compliance_ciphertext).as_str())?;
        }
        if !self.dleq_proofs.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dleqProofs", pbjson::private::base64::encode(&self.dleq_proofs).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferOutputBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_payload",
            "notePayload",
            "wrapped_memo_key",
            "wrappedMemoKey",
            "ovk_wrapped_key",
            "ovkWrappedKey",
            "compliance_ciphertext",
            "complianceCiphertext",
            "dleq_proofs",
            "dleqProofs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NotePayload,
            WrappedMemoKey,
            OvkWrappedKey,
            ComplianceCiphertext,
            DleqProofs,
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
                            "notePayload" | "note_payload" => Ok(GeneratedField::NotePayload),
                            "wrappedMemoKey" | "wrapped_memo_key" => Ok(GeneratedField::WrappedMemoKey),
                            "ovkWrappedKey" | "ovk_wrapped_key" => Ok(GeneratedField::OvkWrappedKey),
                            "complianceCiphertext" | "compliance_ciphertext" => Ok(GeneratedField::ComplianceCiphertext),
                            "dleqProofs" | "dleq_proofs" => Ok(GeneratedField::DleqProofs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferOutputBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferOutputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferOutputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_payload__ = None;
                let mut wrapped_memo_key__ = None;
                let mut ovk_wrapped_key__ = None;
                let mut compliance_ciphertext__ = None;
                let mut dleq_proofs__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NotePayload => {
                            if note_payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("notePayload"));
                            }
                            note_payload__ = map_.next_value()?;
                        }
                        GeneratedField::WrappedMemoKey => {
                            if wrapped_memo_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("wrappedMemoKey"));
                            }
                            wrapped_memo_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OvkWrappedKey => {
                            if ovk_wrapped_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ovkWrappedKey"));
                            }
                            ovk_wrapped_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ComplianceCiphertext => {
                            if compliance_ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceCiphertext"));
                            }
                            compliance_ciphertext__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DleqProofs => {
                            if dleq_proofs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dleqProofs"));
                            }
                            dleq_proofs__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferOutputBody {
                    note_payload: note_payload__,
                    wrapped_memo_key: wrapped_memo_key__.unwrap_or_default(),
                    ovk_wrapped_key: ovk_wrapped_key__.unwrap_or_default(),
                    compliance_ciphertext: compliance_ciphertext__.unwrap_or_default(),
                    dleq_proofs: dleq_proofs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferOutputBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferPlan {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.body.is_some() {
            len += 1;
        }
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if self.balance.is_some() {
            len += 1;
        }
        if !self.spends.is_empty() {
            len += 1;
        }
        if !self.outputs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferPlan", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if let Some(v) = self.balance.as_ref() {
            struct_ser.serialize_field("balance", v)?;
        }
        if !self.spends.is_empty() {
            struct_ser.serialize_field("spends", &self.spends)?;
        }
        if !self.outputs.is_empty() {
            struct_ser.serialize_field("outputs", &self.outputs)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferPlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "value_blinding",
            "valueBlinding",
            "balance",
            "spends",
            "outputs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            ValueBlinding,
            Balance,
            Spends,
            Outputs,
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
                            "body" => Ok(GeneratedField::Body),
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "balance" => Ok(GeneratedField::Balance),
                            "spends" => Ok(GeneratedField::Spends),
                            "outputs" => Ok(GeneratedField::Outputs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut value_blinding__ = None;
                let mut balance__ = None;
                let mut spends__ = None;
                let mut outputs__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Balance => {
                            if balance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("balance"));
                            }
                            balance__ = map_.next_value()?;
                        }
                        GeneratedField::Spends => {
                            if spends__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spends"));
                            }
                            spends__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Outputs => {
                            if outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputs"));
                            }
                            outputs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferPlan {
                    body: body__,
                    value_blinding: value_blinding__.unwrap_or_default(),
                    balance: balance__,
                    spends: spends__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transfer_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferView", len)?;
        if let Some(v) = self.transfer_view.as_ref() {
            match v {
                transfer_view::TransferView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                transfer_view::TransferView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferView {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "visible",
            "opaque",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Visible,
            Opaque,
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
                            "visible" => Ok(GeneratedField::Visible),
                            "opaque" => Ok(GeneratedField::Opaque),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transfer_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if transfer_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            transfer_view__ = map_.next_value::<::std::option::Option<_>>()?.map(transfer_view::TransferView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if transfer_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            transfer_view__ = map_.next_value::<::std::option::Option<_>>()?.map(transfer_view::TransferView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferView {
                    transfer_view: transfer_view__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for transfer_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transfer.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferView.Opaque", len)?;
        if let Some(v) = self.transfer.as_ref() {
            struct_ser.serialize_field("transfer", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for transfer_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transfer",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transfer,
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
                            "transfer" => Ok(GeneratedField::Transfer),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = transfer_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<transfer_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transfer__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transfer => {
                            if transfer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transfer"));
                            }
                            transfer__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(transfer_view::Opaque {
                    transfer: transfer__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for transfer_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.transfer.is_some() {
            len += 1;
        }
        if !self.spent_notes.is_empty() {
            len += 1;
        }
        if !self.created_notes.is_empty() {
            len += 1;
        }
        if self.payload_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.TransferView.Visible", len)?;
        if let Some(v) = self.transfer.as_ref() {
            struct_ser.serialize_field("transfer", v)?;
        }
        if !self.spent_notes.is_empty() {
            struct_ser.serialize_field("spentNotes", &self.spent_notes)?;
        }
        if !self.created_notes.is_empty() {
            struct_ser.serialize_field("createdNotes", &self.created_notes)?;
        }
        if let Some(v) = self.payload_key.as_ref() {
            struct_ser.serialize_field("payloadKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for transfer_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transfer",
            "spent_notes",
            "spentNotes",
            "created_notes",
            "createdNotes",
            "payload_key",
            "payloadKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transfer,
            SpentNotes,
            CreatedNotes,
            PayloadKey,
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
                            "transfer" => Ok(GeneratedField::Transfer),
                            "spentNotes" | "spent_notes" => Ok(GeneratedField::SpentNotes),
                            "createdNotes" | "created_notes" => Ok(GeneratedField::CreatedNotes),
                            "payloadKey" | "payload_key" => Ok(GeneratedField::PayloadKey),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = transfer_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.TransferView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<transfer_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transfer__ = None;
                let mut spent_notes__ = None;
                let mut created_notes__ = None;
                let mut payload_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transfer => {
                            if transfer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transfer"));
                            }
                            transfer__ = map_.next_value()?;
                        }
                        GeneratedField::SpentNotes => {
                            if spent_notes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spentNotes"));
                            }
                            spent_notes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedNotes => {
                            if created_notes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNotes"));
                            }
                            created_notes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::PayloadKey => {
                            if payload_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payloadKey"));
                            }
                            payload_key__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(transfer_view::Visible {
                    transfer: transfer__,
                    spent_notes: spent_notes__.unwrap_or_default(),
                    created_notes: created_notes__.unwrap_or_default(),
                    payload_key: payload_key__,
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.TransferView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkNullifierDerivationProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inner.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.ZKNullifierDerivationProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkNullifierDerivationProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inner,
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
                            "inner" => Ok(GeneratedField::Inner),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ZkNullifierDerivationProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.ZKNullifierDerivationProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkNullifierDerivationProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Inner => {
                            if inner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inner"));
                            }
                            inner__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ZkNullifierDerivationProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.ZKNullifierDerivationProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkOutputProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inner.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.ZKOutputProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkOutputProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inner,
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
                            "inner" => Ok(GeneratedField::Inner),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ZkOutputProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.ZKOutputProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkOutputProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Inner => {
                            if inner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inner"));
                            }
                            inner__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ZkOutputProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.ZKOutputProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkSpendProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inner.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.ZKSpendProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkSpendProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inner,
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
                            "inner" => Ok(GeneratedField::Inner),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ZkSpendProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.ZKSpendProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkSpendProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Inner => {
                            if inner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inner"));
                            }
                            inner__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ZkSpendProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.ZKSpendProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkTransferProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inner.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("penumbra.core.component.shielded_pool.v1.ZKTransferProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkTransferProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inner,
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
                            "inner" => Ok(GeneratedField::Inner),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ZkTransferProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct penumbra.core.component.shielded_pool.v1.ZKTransferProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkTransferProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Inner => {
                            if inner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inner"));
                            }
                            inner__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ZkTransferProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("penumbra.core.component.shielded_pool.v1.ZKTransferProof", FIELDS, GeneratedVisitor)
    }
}
