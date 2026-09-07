impl serde::Serialize for ActionWitness {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.asset.is_some() {
            len += 1;
        }
        if self.user_root.is_some() {
            len += 1;
        }
        if self.sender.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ActionWitness", len)?;
        if let Some(v) = self.asset.as_ref() {
            struct_ser.serialize_field("asset", v)?;
        }
        if let Some(v) = self.user_root.as_ref() {
            struct_ser.serialize_field("userRoot", v)?;
        }
        if let Some(v) = self.sender.as_ref() {
            struct_ser.serialize_field("sender", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ActionWitness {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset",
            "user_root",
            "userRoot",
            "sender",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Asset,
            UserRoot,
            Sender,
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
                            "asset" => Ok(GeneratedField::Asset),
                            "userRoot" | "user_root" => Ok(GeneratedField::UserRoot),
                            "sender" => Ok(GeneratedField::Sender),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ActionWitness;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ActionWitness")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ActionWitness, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset__ = None;
                let mut user_root__ = None;
                let mut sender__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Asset => {
                            if asset__.is_some() {
                                return Err(serde::de::Error::duplicate_field("asset"));
                            }
                            asset__ = map_.next_value()?;
                        }
                        GeneratedField::UserRoot => {
                            if user_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userRoot"));
                            }
                            user_root__ = map_.next_value()?;
                        }
                        GeneratedField::Sender => {
                            if sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender"));
                            }
                            sender__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ActionWitness {
                    asset: asset__,
                    user_root: user_root__,
                    sender: sender__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ActionWitness", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.AssetMetadataByIdRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.AssetMetadataByIdResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.AssetMetadataByIdsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetWitness {
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
        if self.root.is_some() {
            len += 1;
        }
        if self.leaf.is_some() {
            len += 1;
        }
        if self.position != 0 {
            len += 1;
        }
        if self.path.is_some() {
            len += 1;
        }
        if self.is_regulated {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.AssetWitness", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if let Some(v) = self.root.as_ref() {
            struct_ser.serialize_field("root", v)?;
        }
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if let Some(v) = self.path.as_ref() {
            struct_ser.serialize_field("path", v)?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetWitness {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "root",
            "leaf",
            "position",
            "path",
            "is_regulated",
            "isRegulated",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            Root,
            Leaf,
            Position,
            Path,
            IsRegulated,
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
                            "root" => Ok(GeneratedField::Root),
                            "leaf" => Ok(GeneratedField::Leaf),
                            "position" => Ok(GeneratedField::Position),
                            "path" => Ok(GeneratedField::Path),
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetWitness;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.AssetWitness")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetWitness, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut root__ = None;
                let mut leaf__ = None;
                let mut position__ = None;
                let mut path__ = None;
                let mut is_regulated__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::Root => {
                            if root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("root"));
                            }
                            root__ = map_.next_value()?;
                        }
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Path => {
                            if path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("path"));
                            }
                            path__ = map_.next_value()?;
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetWitness {
                    asset_id: asset_id__,
                    root: root__,
                    leaf: leaf__,
                    position: position__.unwrap_or_default(),
                    path: path__,
                    is_regulated: is_regulated__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.AssetWitness", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DiscoveryParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.regulated_precision_bits != 0 {
            len += 1;
        }
        if self.unregulated_precision_bits != 0 {
            len += 1;
        }
        if self.as_of_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.DiscoveryParameters", len)?;
        if self.regulated_precision_bits != 0 {
            struct_ser.serialize_field("regulatedPrecisionBits", &self.regulated_precision_bits)?;
        }
        if self.unregulated_precision_bits != 0 {
            struct_ser.serialize_field("unregulatedPrecisionBits", &self.unregulated_precision_bits)?;
        }
        if self.as_of_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("asOfHeight", ToString::to_string(&self.as_of_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DiscoveryParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "regulated_precision_bits",
            "regulatedPrecisionBits",
            "unregulated_precision_bits",
            "unregulatedPrecisionBits",
            "as_of_height",
            "asOfHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RegulatedPrecisionBits,
            UnregulatedPrecisionBits,
            AsOfHeight,
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
                            "regulatedPrecisionBits" | "regulated_precision_bits" => Ok(GeneratedField::RegulatedPrecisionBits),
                            "unregulatedPrecisionBits" | "unregulated_precision_bits" => Ok(GeneratedField::UnregulatedPrecisionBits),
                            "asOfHeight" | "as_of_height" => Ok(GeneratedField::AsOfHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DiscoveryParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.DiscoveryParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DiscoveryParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut regulated_precision_bits__ = None;
                let mut unregulated_precision_bits__ = None;
                let mut as_of_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RegulatedPrecisionBits => {
                            if regulated_precision_bits__.is_some() {
                                return Err(serde::de::Error::duplicate_field("regulatedPrecisionBits"));
                            }
                            regulated_precision_bits__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UnregulatedPrecisionBits => {
                            if unregulated_precision_bits__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unregulatedPrecisionBits"));
                            }
                            unregulated_precision_bits__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AsOfHeight => {
                            if as_of_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("asOfHeight"));
                            }
                            as_of_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(DiscoveryParameters {
                    regulated_precision_bits: regulated_precision_bits__.unwrap_or_default(),
                    unregulated_precision_bits: unregulated_precision_bits__.unwrap_or_default(),
                    as_of_height: as_of_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.DiscoveryParameters", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EventInboundFungibleTokenTransfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventNoteCreated {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EventNoteCreated", len)?;
        if let Some(v) = self.note_commitment.as_ref() {
            struct_ser.serialize_field("noteCommitment", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventNoteCreated {
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
            type Value = EventNoteCreated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EventNoteCreated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventNoteCreated, V::Error>
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
                Ok(EventNoteCreated {
                    note_commitment: note_commitment__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EventNoteCreated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventNullifierSpent {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EventNullifierSpent", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventNullifierSpent {
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
            type Value = EventNullifierSpent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EventNullifierSpent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventNullifierSpent, V::Error>
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
                Ok(EventNullifierSpent {
                    nullifier: nullifier__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EventNullifierSpent", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenRefund", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EventOutboundFungibleTokenTransfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EvmCall {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.contract.is_empty() {
            len += 1;
        }
        if !self.calldata.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.EvmCall", len)?;
        if !self.contract.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("contract", pbjson::private::base64::encode(&self.contract).as_str())?;
        }
        if !self.calldata.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("calldata", pbjson::private::base64::encode(&self.calldata).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EvmCall {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "contract",
            "calldata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Contract,
            Calldata,
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
                            "contract" => Ok(GeneratedField::Contract),
                            "calldata" => Ok(GeneratedField::Calldata),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EvmCall;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.EvmCall")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EvmCall, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut contract__ = None;
                let mut calldata__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Contract => {
                            if contract__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contract"));
                            }
                            contract__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Calldata => {
                            if calldata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("calldata"));
                            }
                            calldata__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EvmCall {
                    contract: contract__.unwrap_or_default(),
                    calldata: calldata__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.EvmCall", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.FungibleTokenTransferPacketMetadata", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.GenesisContent", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.GenesisContent")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.GenesisContent", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.GenesisContent.Allocation", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.GenesisContent.Allocation")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.GenesisContent.Allocation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HostExecution {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.refund_address.is_empty() {
            len += 1;
        }
        if self.gas_limit != 0 {
            len += 1;
        }
        if !self.calls.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.HostExecution", len)?;
        if !self.refund_address.is_empty() {
            struct_ser.serialize_field("refundAddress", &self.refund_address)?;
        }
        if self.gas_limit != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("gasLimit", ToString::to_string(&self.gas_limit).as_str())?;
        }
        if !self.calls.is_empty() {
            struct_ser.serialize_field("calls", &self.calls)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HostExecution {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "refund_address",
            "refundAddress",
            "gas_limit",
            "gasLimit",
            "calls",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RefundAddress,
            GasLimit,
            Calls,
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
                            "refundAddress" | "refund_address" => Ok(GeneratedField::RefundAddress),
                            "gasLimit" | "gas_limit" => Ok(GeneratedField::GasLimit),
                            "calls" => Ok(GeneratedField::Calls),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = HostExecution;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.HostExecution")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HostExecution, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut refund_address__ = None;
                let mut gas_limit__ = None;
                let mut calls__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RefundAddress => {
                            if refund_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("refundAddress"));
                            }
                            refund_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::GasLimit => {
                            if gas_limit__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gasLimit"));
                            }
                            gas_limit__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Calls => {
                            if calls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("calls"));
                            }
                            calls__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(HostExecution {
                    refund_address: refund_address__.unwrap_or_default(),
                    gas_limit: gas_limit__.unwrap_or_default(),
                    calls: calls__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.HostExecution", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HostTransfer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.recipient.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.HostTransfer", len)?;
        if !self.recipient.is_empty() {
            struct_ser.serialize_field("recipient", &self.recipient)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HostTransfer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "recipient",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Recipient,
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
                            "recipient" => Ok(GeneratedField::Recipient),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = HostTransfer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.HostTransfer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HostTransfer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut recipient__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Recipient => {
                            if recipient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipient"));
                            }
                            recipient__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(HostTransfer {
                    recipient: recipient__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.HostTransfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HostWithdrawal {
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
        if self.destination.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.HostWithdrawal", len)?;
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        if let Some(v) = self.destination.as_ref() {
            match v {
                host_withdrawal::Destination::Transfer(v) => {
                    struct_ser.serialize_field("transfer", v)?;
                }
                host_withdrawal::Destination::Execution(v) => {
                    struct_ser.serialize_field("execution", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HostWithdrawal {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "transfer",
            "execution",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            Transfer,
            Execution,
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
                            "transfer" => Ok(GeneratedField::Transfer),
                            "execution" => Ok(GeneratedField::Execution),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = HostWithdrawal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.HostWithdrawal")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HostWithdrawal, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut destination__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                        GeneratedField::Transfer => {
                            if destination__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transfer"));
                            }
                            destination__ = map_.next_value::<::std::option::Option<_>>()?.map(host_withdrawal::Destination::Transfer)
;
                        }
                        GeneratedField::Execution => {
                            if destination__.is_some() {
                                return Err(serde::de::Error::duplicate_field("execution"));
                            }
                            destination__ = map_.next_value::<::std::option::Option<_>>()?.map(host_withdrawal::Destination::Execution)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(HostWithdrawal {
                    value: value__,
                    destination: destination__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.HostWithdrawal", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.Note", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.Note")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.Note", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteCiphertext", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteCiphertext")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteCiphertext", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NotePayload", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NotePayload")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NotePayload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshape {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshape", len)?;
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
impl<'de> serde::Deserialize<'de> for NoteReshape {
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
            type Value = NoteReshape;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshape")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshape, V::Error>
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
                Ok(NoteReshape {
                    body: body__,
                    auth_sigs: auth_sigs__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshape", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapeBody {
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
        if self.routing_tag.is_some() {
            len += 1;
        }
        if !self.routing_parameter_set_id.is_empty() {
            len += 1;
        }
        if self.asset_anchor.is_some() {
            len += 1;
        }
        if self.compliance_anchor.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeBody", len)?;
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
        if let Some(v) = self.routing_tag.as_ref() {
            struct_ser.serialize_field("routingTag", v)?;
        }
        if !self.routing_parameter_set_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("routingParameterSetId", pbjson::private::base64::encode(&self.routing_parameter_set_id).as_str())?;
        }
        if let Some(v) = self.asset_anchor.as_ref() {
            struct_ser.serialize_field("assetAnchor", v)?;
        }
        if let Some(v) = self.compliance_anchor.as_ref() {
            struct_ser.serialize_field("complianceAnchor", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapeBody {
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
            "routing_tag",
            "routingTag",
            "routing_parameter_set_id",
            "routingParameterSetId",
            "asset_anchor",
            "assetAnchor",
            "compliance_anchor",
            "complianceAnchor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FamilyId,
            Anchor,
            BalanceCommitment,
            Inputs,
            Outputs,
            RoutingTag,
            RoutingParameterSetId,
            AssetAnchor,
            ComplianceAnchor,
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
                            "routingTag" | "routing_tag" => Ok(GeneratedField::RoutingTag),
                            "routingParameterSetId" | "routing_parameter_set_id" => Ok(GeneratedField::RoutingParameterSetId),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteReshapeBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapeBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut family_id__ = None;
                let mut anchor__ = None;
                let mut balance_commitment__ = None;
                let mut inputs__ = None;
                let mut outputs__ = None;
                let mut routing_tag__ = None;
                let mut routing_parameter_set_id__ = None;
                let mut asset_anchor__ = None;
                let mut compliance_anchor__ = None;
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
                        GeneratedField::RoutingTag => {
                            if routing_tag__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingTag"));
                            }
                            routing_tag__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameterSetId => {
                            if routing_parameter_set_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameterSetId"));
                            }
                            routing_parameter_set_id__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssetAnchor => {
                            if asset_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetAnchor"));
                            }
                            asset_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::ComplianceAnchor => {
                            if compliance_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceAnchor"));
                            }
                            compliance_anchor__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapeBody {
                    family_id: family_id__.unwrap_or_default(),
                    anchor: anchor__,
                    balance_commitment: balance_commitment__,
                    inputs: inputs__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                    routing_tag: routing_tag__,
                    routing_parameter_set_id: routing_parameter_set_id__.unwrap_or_default(),
                    asset_anchor: asset_anchor__,
                    compliance_anchor: compliance_anchor__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapeContext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.witness.is_some() {
            len += 1;
        }
        if !self.nonce.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeContext", len)?;
        if let Some(v) = self.witness.as_ref() {
            struct_ser.serialize_field("witness", v)?;
        }
        if !self.nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nonce", pbjson::private::base64::encode(&self.nonce).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapeContext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "witness",
            "nonce",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Witness,
            Nonce,
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
                            "witness" => Ok(GeneratedField::Witness),
                            "nonce" => Ok(GeneratedField::Nonce),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteReshapeContext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeContext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapeContext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut witness__ = None;
                let mut nonce__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Witness => {
                            if witness__.is_some() {
                                return Err(serde::de::Error::duplicate_field("witness"));
                            }
                            witness__ = map_.next_value()?;
                        }
                        GeneratedField::Nonce => {
                            if nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonce"));
                            }
                            nonce__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapeContext {
                    witness: witness__,
                    nonce: nonce__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeContext", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapeInputBody {
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
        if self.history_required {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeInputBody", len)?;
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
        if self.history_required {
            struct_ser.serialize_field("historyRequired", &self.history_required)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapeInputBody {
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
            "history_required",
            "historyRequired",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            Rk,
            EncryptedBackref,
            HistoryRequired,
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
                            "historyRequired" | "history_required" => Ok(GeneratedField::HistoryRequired),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteReshapeInputBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeInputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapeInputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut rk__ = None;
                let mut encrypted_backref__ = None;
                let mut history_required__ = None;
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
                        GeneratedField::HistoryRequired => {
                            if history_required__.is_some() {
                                return Err(serde::de::Error::duplicate_field("historyRequired"));
                            }
                            history_required__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapeInputBody {
                    nullifier: nullifier__,
                    rk: rk__,
                    encrypted_backref: encrypted_backref__.unwrap_or_default(),
                    history_required: history_required__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeInputBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapeOutputBody {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeOutputBody", len)?;
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
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapeOutputBody {
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NotePayload,
            WrappedMemoKey,
            OvkWrappedKey,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteReshapeOutputBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeOutputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapeOutputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_payload__ = None;
                let mut wrapped_memo_key__ = None;
                let mut ovk_wrapped_key__ = None;
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapeOutputBody {
                    note_payload: note_payload__,
                    wrapped_memo_key: wrapped_memo_key__.unwrap_or_default(),
                    ovk_wrapped_key: ovk_wrapped_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeOutputBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapePlan {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.spends.is_empty() {
            len += 1;
        }
        if !self.outputs.is_empty() {
            len += 1;
        }
        if self.family_id != 0 {
            len += 1;
        }
        if self.routing_parameters.is_some() {
            len += 1;
        }
        if self.compliance.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapePlan", len)?;
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.spends.is_empty() {
            struct_ser.serialize_field("spends", &self.spends)?;
        }
        if !self.outputs.is_empty() {
            struct_ser.serialize_field("outputs", &self.outputs)?;
        }
        if self.family_id != 0 {
            struct_ser.serialize_field("familyId", &self.family_id)?;
        }
        if let Some(v) = self.routing_parameters.as_ref() {
            struct_ser.serialize_field("routingParameters", v)?;
        }
        if let Some(v) = self.compliance.as_ref() {
            struct_ser.serialize_field("compliance", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapePlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value_blinding",
            "valueBlinding",
            "spends",
            "outputs",
            "family_id",
            "familyId",
            "routing_parameters",
            "routingParameters",
            "compliance",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ValueBlinding,
            Spends,
            Outputs,
            FamilyId,
            RoutingParameters,
            Compliance,
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
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "spends" => Ok(GeneratedField::Spends),
                            "outputs" => Ok(GeneratedField::Outputs),
                            "familyId" | "family_id" => Ok(GeneratedField::FamilyId),
                            "routingParameters" | "routing_parameters" => Ok(GeneratedField::RoutingParameters),
                            "compliance" => Ok(GeneratedField::Compliance),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NoteReshapePlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapePlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapePlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value_blinding__ = None;
                let mut spends__ = None;
                let mut outputs__ = None;
                let mut family_id__ = None;
                let mut routing_parameters__ = None;
                let mut compliance__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                        GeneratedField::FamilyId => {
                            if family_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("familyId"));
                            }
                            family_id__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RoutingParameters => {
                            if routing_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameters"));
                            }
                            routing_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::Compliance => {
                            if compliance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliance"));
                            }
                            compliance__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapePlan {
                    value_blinding: value_blinding__.unwrap_or_default(),
                    spends: spends__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                    family_id: family_id__.unwrap_or_default(),
                    routing_parameters: routing_parameters__,
                    compliance: compliance__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapePlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NoteReshapeView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_reshape_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView", len)?;
        if let Some(v) = self.note_reshape_view.as_ref() {
            match v {
                note_reshape_view::NoteReshapeView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                note_reshape_view::NoteReshapeView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NoteReshapeView {
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
            type Value = NoteReshapeView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NoteReshapeView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_reshape_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if note_reshape_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            note_reshape_view__ = map_.next_value::<::std::option::Option<_>>()?.map(note_reshape_view::NoteReshapeView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if note_reshape_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            note_reshape_view__ = map_.next_value::<::std::option::Option<_>>()?.map(note_reshape_view::NoteReshapeView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NoteReshapeView {
                    note_reshape_view: note_reshape_view__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for note_reshape_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_reshape.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView.Opaque", len)?;
        if let Some(v) = self.note_reshape.as_ref() {
            struct_ser.serialize_field("noteReshape", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for note_reshape_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_reshape",
            "noteReshape",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteReshape,
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
                            "noteReshape" | "note_reshape" => Ok(GeneratedField::NoteReshape),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = note_reshape_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<note_reshape_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_reshape__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteReshape => {
                            if note_reshape__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteReshape"));
                            }
                            note_reshape__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(note_reshape_view::Opaque {
                    note_reshape: note_reshape__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for note_reshape_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.note_reshape.is_some() {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView.Visible", len)?;
        if let Some(v) = self.note_reshape.as_ref() {
            struct_ser.serialize_field("noteReshape", v)?;
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
impl<'de> serde::Deserialize<'de> for note_reshape_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "note_reshape",
            "noteReshape",
            "spent_notes",
            "spentNotes",
            "created_notes",
            "createdNotes",
            "payload_key",
            "payloadKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NoteReshape,
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
                            "noteReshape" | "note_reshape" => Ok(GeneratedField::NoteReshape),
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
            type Value = note_reshape_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteReshapeView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<note_reshape_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_reshape__ = None;
                let mut spent_notes__ = None;
                let mut created_notes__ = None;
                let mut payload_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NoteReshape => {
                            if note_reshape__.is_some() {
                                return Err(serde::de::Error::duplicate_field("noteReshape"));
                            }
                            note_reshape__ = map_.next_value()?;
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
                Ok(note_reshape_view::Visible {
                    note_reshape: note_reshape__,
                    spent_notes: spent_notes__.unwrap_or_default(),
                    created_notes: created_notes__.unwrap_or_default(),
                    payload_key: payload_key__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteReshapeView.Visible", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.NoteView", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.NoteView")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.NoteView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingSelector {
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
        if self.prefix != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.RoutingSelector", len)?;
        if self.precision_bits != 0 {
            struct_ser.serialize_field("precisionBits", &self.precision_bits)?;
        }
        if self.prefix != 0 {
            struct_ser.serialize_field("prefix", &self.prefix)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingSelector {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "precision_bits",
            "precisionBits",
            "prefix",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PrecisionBits,
            Prefix,
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
                            "prefix" => Ok(GeneratedField::Prefix),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingSelector;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.RoutingSelector")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingSelector, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut precision_bits__ = None;
                let mut prefix__ = None;
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
                        GeneratedField::Prefix => {
                            if prefix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("prefix"));
                            }
                            prefix__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingSelector {
                    precision_bits: precision_bits__.unwrap_or_default(),
                    prefix: prefix__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.RoutingSelector", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingTag {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.value != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.RoutingTag", len)?;
        if self.value != 0 {
            struct_ser.serialize_field("value", &self.value)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoutingTag {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoutingTag;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.RoutingTag")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RoutingTag, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RoutingTag {
                    value: value__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.RoutingTag", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedHostWithdrawal {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawal", len)?;
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
impl<'de> serde::Deserialize<'de> for ShieldedHostWithdrawal {
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
            type Value = ShieldedHostWithdrawal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawal")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedHostWithdrawal, V::Error>
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
                Ok(ShieldedHostWithdrawal {
                    body: body__,
                    auth_sigs: auth_sigs__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawal", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedHostWithdrawalBody {
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
        if self.withdrawal.is_some() {
            len += 1;
        }
        if self.change_output.is_some() {
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
        if self.routing_tag.is_some() {
            len += 1;
        }
        if !self.routing_parameter_set_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalBody", len)?;
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
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if let Some(v) = self.change_output.as_ref() {
            struct_ser.serialize_field("changeOutput", v)?;
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
        if let Some(v) = self.routing_tag.as_ref() {
            struct_ser.serialize_field("routingTag", v)?;
        }
        if !self.routing_parameter_set_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("routingParameterSetId", pbjson::private::base64::encode(&self.routing_parameter_set_id).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedHostWithdrawalBody {
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
            "withdrawal",
            "change_output",
            "changeOutput",
            "target_timestamp",
            "targetTimestamp",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "routing_tag",
            "routingTag",
            "routing_parameter_set_id",
            "routingParameterSetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FamilyId,
            Anchor,
            BalanceCommitment,
            Inputs,
            Withdrawal,
            ChangeOutput,
            TargetTimestamp,
            ComplianceAnchor,
            AssetAnchor,
            RoutingTag,
            RoutingParameterSetId,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "changeOutput" | "change_output" => Ok(GeneratedField::ChangeOutput),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "routingTag" | "routing_tag" => Ok(GeneratedField::RoutingTag),
                            "routingParameterSetId" | "routing_parameter_set_id" => Ok(GeneratedField::RoutingParameterSetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedHostWithdrawalBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedHostWithdrawalBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut family_id__ = None;
                let mut anchor__ = None;
                let mut balance_commitment__ = None;
                let mut inputs__ = None;
                let mut withdrawal__ = None;
                let mut change_output__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut routing_tag__ = None;
                let mut routing_parameter_set_id__ = None;
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
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::ChangeOutput => {
                            if change_output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeOutput"));
                            }
                            change_output__ = map_.next_value()?;
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
                        GeneratedField::RoutingTag => {
                            if routing_tag__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingTag"));
                            }
                            routing_tag__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameterSetId => {
                            if routing_parameter_set_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameterSetId"));
                            }
                            routing_parameter_set_id__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedHostWithdrawalBody {
                    family_id: family_id__.unwrap_or_default(),
                    anchor: anchor__,
                    balance_commitment: balance_commitment__,
                    inputs: inputs__.unwrap_or_default(),
                    withdrawal: withdrawal__,
                    change_output: change_output__,
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    routing_tag: routing_tag__,
                    routing_parameter_set_id: routing_parameter_set_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedHostWithdrawalPlan {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.spends.is_empty() {
            len += 1;
        }
        if self.change_output.is_some() {
            len += 1;
        }
        if self.withdrawal.is_some() {
            len += 1;
        }
        if self.routing_parameters.is_some() {
            len += 1;
        }
        if self.compliance.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalPlan", len)?;
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.spends.is_empty() {
            struct_ser.serialize_field("spends", &self.spends)?;
        }
        if let Some(v) = self.change_output.as_ref() {
            struct_ser.serialize_field("changeOutput", v)?;
        }
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if let Some(v) = self.routing_parameters.as_ref() {
            struct_ser.serialize_field("routingParameters", v)?;
        }
        if let Some(v) = self.compliance.as_ref() {
            struct_ser.serialize_field("compliance", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedHostWithdrawalPlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value_blinding",
            "valueBlinding",
            "spends",
            "change_output",
            "changeOutput",
            "withdrawal",
            "routing_parameters",
            "routingParameters",
            "compliance",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ValueBlinding,
            Spends,
            ChangeOutput,
            Withdrawal,
            RoutingParameters,
            Compliance,
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
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "spends" => Ok(GeneratedField::Spends),
                            "changeOutput" | "change_output" => Ok(GeneratedField::ChangeOutput),
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "routingParameters" | "routing_parameters" => Ok(GeneratedField::RoutingParameters),
                            "compliance" => Ok(GeneratedField::Compliance),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedHostWithdrawalPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedHostWithdrawalPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value_blinding__ = None;
                let mut spends__ = None;
                let mut change_output__ = None;
                let mut withdrawal__ = None;
                let mut routing_parameters__ = None;
                let mut compliance__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Spends => {
                            if spends__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spends"));
                            }
                            spends__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ChangeOutput => {
                            if change_output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeOutput"));
                            }
                            change_output__ = map_.next_value()?;
                        }
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameters => {
                            if routing_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameters"));
                            }
                            routing_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::Compliance => {
                            if compliance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliance"));
                            }
                            compliance__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedHostWithdrawalPlan {
                    value_blinding: value_blinding__.unwrap_or_default(),
                    spends: spends__.unwrap_or_default(),
                    change_output: change_output__,
                    withdrawal: withdrawal__,
                    routing_parameters: routing_parameters__,
                    compliance: compliance__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedHostWithdrawalView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.shielded_host_withdrawal_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView", len)?;
        if let Some(v) = self.shielded_host_withdrawal_view.as_ref() {
            match v {
                shielded_host_withdrawal_view::ShieldedHostWithdrawalView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                shielded_host_withdrawal_view::ShieldedHostWithdrawalView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedHostWithdrawalView {
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
            type Value = ShieldedHostWithdrawalView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedHostWithdrawalView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut shielded_host_withdrawal_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if shielded_host_withdrawal_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            shielded_host_withdrawal_view__ = map_.next_value::<::std::option::Option<_>>()?.map(shielded_host_withdrawal_view::ShieldedHostWithdrawalView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if shielded_host_withdrawal_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            shielded_host_withdrawal_view__ = map_.next_value::<::std::option::Option<_>>()?.map(shielded_host_withdrawal_view::ShieldedHostWithdrawalView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedHostWithdrawalView {
                    shielded_host_withdrawal_view: shielded_host_withdrawal_view__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for shielded_host_withdrawal_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.withdrawal.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Opaque", len)?;
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for shielded_host_withdrawal_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "withdrawal",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Withdrawal,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = shielded_host_withdrawal_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<shielded_host_withdrawal_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut withdrawal__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(shielded_host_withdrawal_view::Opaque {
                    withdrawal: withdrawal__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for shielded_host_withdrawal_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.withdrawal.is_some() {
            len += 1;
        }
        if !self.spent_notes.is_empty() {
            len += 1;
        }
        if self.change_note.is_some() {
            len += 1;
        }
        if self.payload_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Visible", len)?;
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if !self.spent_notes.is_empty() {
            struct_ser.serialize_field("spentNotes", &self.spent_notes)?;
        }
        if let Some(v) = self.change_note.as_ref() {
            struct_ser.serialize_field("changeNote", v)?;
        }
        if let Some(v) = self.payload_key.as_ref() {
            struct_ser.serialize_field("payloadKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for shielded_host_withdrawal_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "withdrawal",
            "spent_notes",
            "spentNotes",
            "change_note",
            "changeNote",
            "payload_key",
            "payloadKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Withdrawal,
            SpentNotes,
            ChangeNote,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "spentNotes" | "spent_notes" => Ok(GeneratedField::SpentNotes),
                            "changeNote" | "change_note" => Ok(GeneratedField::ChangeNote),
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
            type Value = shielded_host_withdrawal_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<shielded_host_withdrawal_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut withdrawal__ = None;
                let mut spent_notes__ = None;
                let mut change_note__ = None;
                let mut payload_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::SpentNotes => {
                            if spent_notes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spentNotes"));
                            }
                            spent_notes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ChangeNote => {
                            if change_note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeNote"));
                            }
                            change_note__ = map_.next_value()?;
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
                Ok(shielded_host_withdrawal_view::Visible {
                    withdrawal: withdrawal__,
                    spent_notes: spent_notes__.unwrap_or_default(),
                    change_note: change_note__,
                    payload_key: payload_key__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedHostWithdrawalView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedIcs20Withdrawal {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20Withdrawal", len)?;
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
impl<'de> serde::Deserialize<'de> for ShieldedIcs20Withdrawal {
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
            type Value = ShieldedIcs20Withdrawal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20Withdrawal")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedIcs20Withdrawal, V::Error>
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
                Ok(ShieldedIcs20Withdrawal {
                    body: body__,
                    auth_sigs: auth_sigs__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20Withdrawal", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedIcs20WithdrawalBody {
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
        if self.withdrawal.is_some() {
            len += 1;
        }
        if self.change_output.is_some() {
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
        if self.routing_tag.is_some() {
            len += 1;
        }
        if !self.routing_parameter_set_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalBody", len)?;
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
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if let Some(v) = self.change_output.as_ref() {
            struct_ser.serialize_field("changeOutput", v)?;
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
        if let Some(v) = self.routing_tag.as_ref() {
            struct_ser.serialize_field("routingTag", v)?;
        }
        if !self.routing_parameter_set_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("routingParameterSetId", pbjson::private::base64::encode(&self.routing_parameter_set_id).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedIcs20WithdrawalBody {
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
            "withdrawal",
            "change_output",
            "changeOutput",
            "target_timestamp",
            "targetTimestamp",
            "compliance_anchor",
            "complianceAnchor",
            "asset_anchor",
            "assetAnchor",
            "routing_tag",
            "routingTag",
            "routing_parameter_set_id",
            "routingParameterSetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FamilyId,
            Anchor,
            BalanceCommitment,
            Inputs,
            Withdrawal,
            ChangeOutput,
            TargetTimestamp,
            ComplianceAnchor,
            AssetAnchor,
            RoutingTag,
            RoutingParameterSetId,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "changeOutput" | "change_output" => Ok(GeneratedField::ChangeOutput),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "routingTag" | "routing_tag" => Ok(GeneratedField::RoutingTag),
                            "routingParameterSetId" | "routing_parameter_set_id" => Ok(GeneratedField::RoutingParameterSetId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedIcs20WithdrawalBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedIcs20WithdrawalBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut family_id__ = None;
                let mut anchor__ = None;
                let mut balance_commitment__ = None;
                let mut inputs__ = None;
                let mut withdrawal__ = None;
                let mut change_output__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut routing_tag__ = None;
                let mut routing_parameter_set_id__ = None;
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
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::ChangeOutput => {
                            if change_output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeOutput"));
                            }
                            change_output__ = map_.next_value()?;
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
                        GeneratedField::RoutingTag => {
                            if routing_tag__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingTag"));
                            }
                            routing_tag__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameterSetId => {
                            if routing_parameter_set_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameterSetId"));
                            }
                            routing_parameter_set_id__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedIcs20WithdrawalBody {
                    family_id: family_id__.unwrap_or_default(),
                    anchor: anchor__,
                    balance_commitment: balance_commitment__,
                    inputs: inputs__.unwrap_or_default(),
                    withdrawal: withdrawal__,
                    change_output: change_output__,
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    routing_tag: routing_tag__,
                    routing_parameter_set_id: routing_parameter_set_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedIcs20WithdrawalChangeBody {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalChangeBody", len)?;
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
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedIcs20WithdrawalChangeBody {
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NotePayload,
            WrappedMemoKey,
            OvkWrappedKey,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedIcs20WithdrawalChangeBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalChangeBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedIcs20WithdrawalChangeBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_payload__ = None;
                let mut wrapped_memo_key__ = None;
                let mut ovk_wrapped_key__ = None;
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedIcs20WithdrawalChangeBody {
                    note_payload: note_payload__,
                    wrapped_memo_key: wrapped_memo_key__.unwrap_or_default(),
                    ovk_wrapped_key: ovk_wrapped_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalChangeBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedIcs20WithdrawalPlan {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.spends.is_empty() {
            len += 1;
        }
        if self.change_output.is_some() {
            len += 1;
        }
        if self.withdrawal.is_some() {
            len += 1;
        }
        if self.routing_parameters.is_some() {
            len += 1;
        }
        if self.compliance.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalPlan", len)?;
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.spends.is_empty() {
            struct_ser.serialize_field("spends", &self.spends)?;
        }
        if let Some(v) = self.change_output.as_ref() {
            struct_ser.serialize_field("changeOutput", v)?;
        }
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if let Some(v) = self.routing_parameters.as_ref() {
            struct_ser.serialize_field("routingParameters", v)?;
        }
        if let Some(v) = self.compliance.as_ref() {
            struct_ser.serialize_field("compliance", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedIcs20WithdrawalPlan {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value_blinding",
            "valueBlinding",
            "spends",
            "change_output",
            "changeOutput",
            "withdrawal",
            "routing_parameters",
            "routingParameters",
            "compliance",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ValueBlinding,
            Spends,
            ChangeOutput,
            Withdrawal,
            RoutingParameters,
            Compliance,
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
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "spends" => Ok(GeneratedField::Spends),
                            "changeOutput" | "change_output" => Ok(GeneratedField::ChangeOutput),
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "routingParameters" | "routing_parameters" => Ok(GeneratedField::RoutingParameters),
                            "compliance" => Ok(GeneratedField::Compliance),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedIcs20WithdrawalPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedIcs20WithdrawalPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value_blinding__ = None;
                let mut spends__ = None;
                let mut change_output__ = None;
                let mut withdrawal__ = None;
                let mut routing_parameters__ = None;
                let mut compliance__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Spends => {
                            if spends__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spends"));
                            }
                            spends__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ChangeOutput => {
                            if change_output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeOutput"));
                            }
                            change_output__ = map_.next_value()?;
                        }
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameters => {
                            if routing_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameters"));
                            }
                            routing_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::Compliance => {
                            if compliance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliance"));
                            }
                            compliance__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedIcs20WithdrawalPlan {
                    value_blinding: value_blinding__.unwrap_or_default(),
                    spends: spends__.unwrap_or_default(),
                    change_output: change_output__,
                    withdrawal: withdrawal__,
                    routing_parameters: routing_parameters__,
                    compliance: compliance__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedIcs20WithdrawalView {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.shielded_ics20_withdrawal_view.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView", len)?;
        if let Some(v) = self.shielded_ics20_withdrawal_view.as_ref() {
            match v {
                shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView::Visible(v) => {
                    struct_ser.serialize_field("visible", v)?;
                }
                shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView::Opaque(v) => {
                    struct_ser.serialize_field("opaque", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedIcs20WithdrawalView {
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
            type Value = ShieldedIcs20WithdrawalView;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedIcs20WithdrawalView, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut shielded_ics20_withdrawal_view__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Visible => {
                            if shielded_ics20_withdrawal_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("visible"));
                            }
                            shielded_ics20_withdrawal_view__ = map_.next_value::<::std::option::Option<_>>()?.map(shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView::Visible)
;
                        }
                        GeneratedField::Opaque => {
                            if shielded_ics20_withdrawal_view__.is_some() {
                                return Err(serde::de::Error::duplicate_field("opaque"));
                            }
                            shielded_ics20_withdrawal_view__ = map_.next_value::<::std::option::Option<_>>()?.map(shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView::Opaque)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedIcs20WithdrawalView {
                    shielded_ics20_withdrawal_view: shielded_ics20_withdrawal_view__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for shielded_ics20_withdrawal_view::Opaque {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.withdrawal.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Opaque", len)?;
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for shielded_ics20_withdrawal_view::Opaque {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "withdrawal",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Withdrawal,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = shielded_ics20_withdrawal_view::Opaque;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Opaque")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<shielded_ics20_withdrawal_view::Opaque, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut withdrawal__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(shielded_ics20_withdrawal_view::Opaque {
                    withdrawal: withdrawal__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Opaque", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for shielded_ics20_withdrawal_view::Visible {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.withdrawal.is_some() {
            len += 1;
        }
        if !self.spent_notes.is_empty() {
            len += 1;
        }
        if self.change_note.is_some() {
            len += 1;
        }
        if self.payload_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Visible", len)?;
        if let Some(v) = self.withdrawal.as_ref() {
            struct_ser.serialize_field("withdrawal", v)?;
        }
        if !self.spent_notes.is_empty() {
            struct_ser.serialize_field("spentNotes", &self.spent_notes)?;
        }
        if let Some(v) = self.change_note.as_ref() {
            struct_ser.serialize_field("changeNote", v)?;
        }
        if let Some(v) = self.payload_key.as_ref() {
            struct_ser.serialize_field("payloadKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for shielded_ics20_withdrawal_view::Visible {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "withdrawal",
            "spent_notes",
            "spentNotes",
            "change_note",
            "changeNote",
            "payload_key",
            "payloadKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Withdrawal,
            SpentNotes,
            ChangeNote,
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
                            "withdrawal" => Ok(GeneratedField::Withdrawal),
                            "spentNotes" | "spent_notes" => Ok(GeneratedField::SpentNotes),
                            "changeNote" | "change_note" => Ok(GeneratedField::ChangeNote),
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
            type Value = shielded_ics20_withdrawal_view::Visible;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Visible")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<shielded_ics20_withdrawal_view::Visible, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut withdrawal__ = None;
                let mut spent_notes__ = None;
                let mut change_note__ = None;
                let mut payload_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Withdrawal => {
                            if withdrawal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withdrawal"));
                            }
                            withdrawal__ = map_.next_value()?;
                        }
                        GeneratedField::SpentNotes => {
                            if spent_notes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spentNotes"));
                            }
                            spent_notes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ChangeNote => {
                            if change_note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeNote"));
                            }
                            change_note__ = map_.next_value()?;
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
                Ok(shielded_ics20_withdrawal_view::Visible {
                    withdrawal: withdrawal__,
                    spent_notes: spent_notes__.unwrap_or_default(),
                    change_note: change_note__,
                    payload_key: payload_key__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedIcs20WithdrawalView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedInputPlan {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedInputPlan", len)?;
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
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedInputPlan {
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Note,
            Position,
            Randomizer,
            ValueBlinding,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedInputPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedInputPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedInputPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note__ = None;
                let mut position__ = None;
                let mut randomizer__ = None;
                let mut value_blinding__ = None;
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedInputPlan {
                    note: note__,
                    position: position__.unwrap_or_default(),
                    randomizer: randomizer__.unwrap_or_default(),
                    value_blinding: value_blinding__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedInputPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ShieldedOutputPlan {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedOutputPlan", len)?;
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
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ShieldedOutputPlan {
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            DestAddress,
            Rseed,
            ValueBlinding,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ShieldedOutputPlan;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedOutputPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedOutputPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut dest_address__ = None;
                let mut rseed__ = None;
                let mut value_blinding__ = None;
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedOutputPlan {
                    value: value__,
                    dest_address: dest_address__,
                    rseed: rseed__.unwrap_or_default(),
                    value_blinding: value_blinding__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedOutputPlan", FIELDS, GeneratedVisitor)
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
        if self.discovery_params.is_some() {
            len += 1;
        }
        if self.discovery_grace_period_blocks != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedPoolParameters", len)?;
        if let Some(v) = self.discovery_params.as_ref() {
            struct_ser.serialize_field("discoveryParams", v)?;
        }
        if self.discovery_grace_period_blocks != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("discoveryGracePeriodBlocks", ToString::to_string(&self.discovery_grace_period_blocks).as_str())?;
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
            "discovery_params",
            "discoveryParams",
            "discovery_grace_period_blocks",
            "discoveryGracePeriodBlocks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DiscoveryParams,
            DiscoveryGracePeriodBlocks,
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
                            "discoveryParams" | "discovery_params" => Ok(GeneratedField::DiscoveryParams),
                            "discoveryGracePeriodBlocks" | "discovery_grace_period_blocks" => Ok(GeneratedField::DiscoveryGracePeriodBlocks),
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ShieldedPoolParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ShieldedPoolParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut discovery_params__ = None;
                let mut discovery_grace_period_blocks__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DiscoveryParams => {
                            if discovery_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("discoveryParams"));
                            }
                            discovery_params__ = map_.next_value()?;
                        }
                        GeneratedField::DiscoveryGracePeriodBlocks => {
                            if discovery_grace_period_blocks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("discoveryGracePeriodBlocks"));
                            }
                            discovery_grace_period_blocks__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ShieldedPoolParameters {
                    discovery_params: discovery_params__,
                    discovery_grace_period_blocks: discovery_grace_period_blocks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ShieldedPoolParameters", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.Transfer", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.Transfer")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.Transfer", FIELDS, GeneratedVisitor)
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
        if self.routing.is_some() {
            len += 1;
        }
        if !self.routing_parameter_set_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferBody", len)?;
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
        if let Some(v) = self.routing.as_ref() {
            struct_ser.serialize_field("routing", v)?;
        }
        if !self.routing_parameter_set_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("routingParameterSetId", pbjson::private::base64::encode(&self.routing_parameter_set_id).as_str())?;
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
            "routing",
            "routing_parameter_set_id",
            "routingParameterSetId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Anchor,
            BalanceCommitment,
            Inputs,
            Outputs,
            TargetTimestamp,
            ComplianceAnchor,
            AssetAnchor,
            Routing,
            RoutingParameterSetId,
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
                            "anchor" => Ok(GeneratedField::Anchor),
                            "balanceCommitment" | "balance_commitment" => Ok(GeneratedField::BalanceCommitment),
                            "inputs" => Ok(GeneratedField::Inputs),
                            "outputs" => Ok(GeneratedField::Outputs),
                            "targetTimestamp" | "target_timestamp" => Ok(GeneratedField::TargetTimestamp),
                            "complianceAnchor" | "compliance_anchor" => Ok(GeneratedField::ComplianceAnchor),
                            "assetAnchor" | "asset_anchor" => Ok(GeneratedField::AssetAnchor),
                            "routing" => Ok(GeneratedField::Routing),
                            "routingParameterSetId" | "routing_parameter_set_id" => Ok(GeneratedField::RoutingParameterSetId),
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut anchor__ = None;
                let mut balance_commitment__ = None;
                let mut inputs__ = None;
                let mut outputs__ = None;
                let mut target_timestamp__ = None;
                let mut compliance_anchor__ = None;
                let mut asset_anchor__ = None;
                let mut routing__ = None;
                let mut routing_parameter_set_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
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
                        GeneratedField::Routing => {
                            if routing__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routing"));
                            }
                            routing__ = map_.next_value()?;
                        }
                        GeneratedField::RoutingParameterSetId => {
                            if routing_parameter_set_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameterSetId"));
                            }
                            routing_parameter_set_id__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferBody {
                    anchor: anchor__,
                    balance_commitment: balance_commitment__,
                    inputs: inputs__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                    target_timestamp: target_timestamp__.unwrap_or_default(),
                    compliance_anchor: compliance_anchor__,
                    asset_anchor: asset_anchor__,
                    routing: routing__,
                    routing_parameter_set_id: routing_parameter_set_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferBody", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferContext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.witness.is_some() {
            len += 1;
        }
        if self.recipient.is_some() {
            len += 1;
        }
        if self.policy.is_some() {
            len += 1;
        }
        if self.timestamp != 0 {
            len += 1;
        }
        if !self.nonce.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferContext", len)?;
        if let Some(v) = self.witness.as_ref() {
            struct_ser.serialize_field("witness", v)?;
        }
        if let Some(v) = self.recipient.as_ref() {
            struct_ser.serialize_field("recipient", v)?;
        }
        if let Some(v) = self.policy.as_ref() {
            struct_ser.serialize_field("policy", v)?;
        }
        if self.timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("timestamp", ToString::to_string(&self.timestamp).as_str())?;
        }
        if !self.nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nonce", pbjson::private::base64::encode(&self.nonce).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferContext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "witness",
            "recipient",
            "policy",
            "timestamp",
            "nonce",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Witness,
            Recipient,
            Policy,
            Timestamp,
            Nonce,
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
                            "witness" => Ok(GeneratedField::Witness),
                            "recipient" => Ok(GeneratedField::Recipient),
                            "policy" => Ok(GeneratedField::Policy),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            "nonce" => Ok(GeneratedField::Nonce),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferContext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferContext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferContext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut witness__ = None;
                let mut recipient__ = None;
                let mut policy__ = None;
                let mut timestamp__ = None;
                let mut nonce__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Witness => {
                            if witness__.is_some() {
                                return Err(serde::de::Error::duplicate_field("witness"));
                            }
                            witness__ = map_.next_value()?;
                        }
                        GeneratedField::Recipient => {
                            if recipient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipient"));
                            }
                            recipient__ = map_.next_value()?;
                        }
                        GeneratedField::Policy => {
                            if policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policy"));
                            }
                            policy__ = map_.next_value()?;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Nonce => {
                            if nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonce"));
                            }
                            nonce__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferContext {
                    witness: witness__,
                    recipient: recipient__,
                    policy: policy__,
                    timestamp: timestamp__.unwrap_or_default(),
                    nonce: nonce__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferContext", FIELDS, GeneratedVisitor)
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
        if self.history_required {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferInputBody", len)?;
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
        if self.history_required {
            struct_ser.serialize_field("historyRequired", &self.history_required)?;
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
            "history_required",
            "historyRequired",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            Rk,
            EncryptedBackref,
            ComplianceCiphertext,
            HistoryRequired,
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
                            "historyRequired" | "history_required" => Ok(GeneratedField::HistoryRequired),
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferInputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferInputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut rk__ = None;
                let mut encrypted_backref__ = None;
                let mut compliance_ciphertext__ = None;
                let mut history_required__ = None;
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
                        GeneratedField::HistoryRequired => {
                            if history_required__.is_some() {
                                return Err(serde::de::Error::duplicate_field("historyRequired"));
                            }
                            history_required__ = Some(map_.next_value()?);
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
                    history_required: history_required__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferInputBody", FIELDS, GeneratedVisitor)
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
        if !self.compliance_metadata.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferOutputBody", len)?;
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
        if !self.compliance_metadata.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("complianceMetadata", pbjson::private::base64::encode(&self.compliance_metadata).as_str())?;
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
            "compliance_metadata",
            "complianceMetadata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NotePayload,
            WrappedMemoKey,
            OvkWrappedKey,
            ComplianceCiphertext,
            ComplianceMetadata,
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
                            "complianceMetadata" | "compliance_metadata" => Ok(GeneratedField::ComplianceMetadata),
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferOutputBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferOutputBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut note_payload__ = None;
                let mut wrapped_memo_key__ = None;
                let mut ovk_wrapped_key__ = None;
                let mut compliance_ciphertext__ = None;
                let mut compliance_metadata__ = None;
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
                        GeneratedField::ComplianceMetadata => {
                            if compliance_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceMetadata"));
                            }
                            compliance_metadata__ =
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
                    compliance_metadata: compliance_metadata__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferOutputBody", FIELDS, GeneratedVisitor)
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
        if !self.value_blinding.is_empty() {
            len += 1;
        }
        if !self.spends.is_empty() {
            len += 1;
        }
        if !self.outputs.is_empty() {
            len += 1;
        }
        if self.routing_parameters.is_some() {
            len += 1;
        }
        if self.compliance.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferPlan", len)?;
        if !self.value_blinding.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("valueBlinding", pbjson::private::base64::encode(&self.value_blinding).as_str())?;
        }
        if !self.spends.is_empty() {
            struct_ser.serialize_field("spends", &self.spends)?;
        }
        if !self.outputs.is_empty() {
            struct_ser.serialize_field("outputs", &self.outputs)?;
        }
        if let Some(v) = self.routing_parameters.as_ref() {
            struct_ser.serialize_field("routingParameters", v)?;
        }
        if let Some(v) = self.compliance.as_ref() {
            struct_ser.serialize_field("compliance", v)?;
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
            "value_blinding",
            "valueBlinding",
            "spends",
            "outputs",
            "routing_parameters",
            "routingParameters",
            "compliance",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ValueBlinding,
            Spends,
            Outputs,
            RoutingParameters,
            Compliance,
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
                            "valueBlinding" | "value_blinding" => Ok(GeneratedField::ValueBlinding),
                            "spends" => Ok(GeneratedField::Spends),
                            "outputs" => Ok(GeneratedField::Outputs),
                            "routingParameters" | "routing_parameters" => Ok(GeneratedField::RoutingParameters),
                            "compliance" => Ok(GeneratedField::Compliance),
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferPlan")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferPlan, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value_blinding__ = None;
                let mut spends__ = None;
                let mut outputs__ = None;
                let mut routing_parameters__ = None;
                let mut compliance__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ValueBlinding => {
                            if value_blinding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("valueBlinding"));
                            }
                            value_blinding__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                        GeneratedField::RoutingParameters => {
                            if routing_parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routingParameters"));
                            }
                            routing_parameters__ = map_.next_value()?;
                        }
                        GeneratedField::Compliance => {
                            if compliance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compliance"));
                            }
                            compliance__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferPlan {
                    value_blinding: value_blinding__.unwrap_or_default(),
                    spends: spends__.unwrap_or_default(),
                    outputs: outputs__.unwrap_or_default(),
                    routing_parameters: routing_parameters__,
                    compliance: compliance__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferPlan", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TransferRouting {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.tags.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferRouting", len)?;
        if !self.tags.is_empty() {
            struct_ser.serialize_field("tags", &self.tags)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TransferRouting {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "tags",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Tags,
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
                            "tags" => Ok(GeneratedField::Tags),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TransferRouting;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferRouting")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TransferRouting, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut tags__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Tags => {
                            if tags__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tags"));
                            }
                            tags__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TransferRouting {
                    tags: tags__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferRouting", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferView", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferView")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferView", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferView.Opaque", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferView.Opaque")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferView.Opaque", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.TransferView.Visible", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.TransferView.Visible")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.TransferView.Visible", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UserWitness {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.leaf.is_some() {
            len += 1;
        }
        if self.position != 0 {
            len += 1;
        }
        if self.path.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.UserWitness", len)?;
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if let Some(v) = self.path.as_ref() {
            struct_ser.serialize_field("path", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UserWitness {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "leaf",
            "position",
            "path",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Leaf,
            Position,
            Path,
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
                            "leaf" => Ok(GeneratedField::Leaf),
                            "position" => Ok(GeneratedField::Position),
                            "path" => Ok(GeneratedField::Path),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserWitness;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.UserWitness")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UserWitness, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut leaf__ = None;
                let mut position__ = None;
                let mut path__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Path => {
                            if path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("path"));
                            }
                            path__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UserWitness {
                    leaf: leaf__,
                    position: position__.unwrap_or_default(),
                    path: path__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.UserWitness", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WithdrawalContext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.witness.is_some() {
            len += 1;
        }
        if self.timestamp != 0 {
            len += 1;
        }
        if !self.nonce.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.WithdrawalContext", len)?;
        if let Some(v) = self.witness.as_ref() {
            struct_ser.serialize_field("witness", v)?;
        }
        if self.timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("timestamp", ToString::to_string(&self.timestamp).as_str())?;
        }
        if !self.nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nonce", pbjson::private::base64::encode(&self.nonce).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WithdrawalContext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "witness",
            "timestamp",
            "nonce",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Witness,
            Timestamp,
            Nonce,
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
                            "witness" => Ok(GeneratedField::Witness),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            "nonce" => Ok(GeneratedField::Nonce),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WithdrawalContext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.WithdrawalContext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WithdrawalContext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut witness__ = None;
                let mut timestamp__ = None;
                let mut nonce__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Witness => {
                            if witness__.is_some() {
                                return Err(serde::de::Error::duplicate_field("witness"));
                            }
                            witness__ = map_.next_value()?;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Nonce => {
                            if nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonce"));
                            }
                            nonce__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WithdrawalContext {
                    witness: witness__,
                    timestamp: timestamp__.unwrap_or_default(),
                    nonce: nonce__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.WithdrawalContext", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkNoteReshapeProof {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ZKNoteReshapeProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkNoteReshapeProof {
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
            type Value = ZkNoteReshapeProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ZKNoteReshapeProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkNoteReshapeProof, V::Error>
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
                Ok(ZkNoteReshapeProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ZKNoteReshapeProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ZkShieldedIcs20WithdrawalProof {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ZKShieldedIcs20WithdrawalProof", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ZkShieldedIcs20WithdrawalProof {
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
            type Value = ZkShieldedIcs20WithdrawalProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ZKShieldedIcs20WithdrawalProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ZkShieldedIcs20WithdrawalProof, V::Error>
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
                Ok(ZkShieldedIcs20WithdrawalProof {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ZKShieldedIcs20WithdrawalProof", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.shielded_pool.v1.ZKTransferProof", len)?;
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
                formatter.write_str("struct shieldd.core.component.shielded_pool.v1.ZKTransferProof")
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
        deserializer.deserialize_struct("shieldd.core.component.shielded_pool.v1.ZKTransferProof", FIELDS, GeneratedVisitor)
    }
}
