impl serde::Serialize for AnchorByHeightRequest {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.AnchorByHeightRequest", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AnchorByHeightRequest {
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
            type Value = AnchorByHeightRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.AnchorByHeightRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AnchorByHeightRequest, V::Error>
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
                Ok(AnchorByHeightRequest {
                    height: height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.AnchorByHeightRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AnchorByHeightResponse {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.AnchorByHeightResponse", len)?;
        if let Some(v) = self.anchor.as_ref() {
            struct_ser.serialize_field("anchor", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AnchorByHeightResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "anchor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Anchor,
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
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AnchorByHeightResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.AnchorByHeightResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AnchorByHeightResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut anchor__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Anchor => {
                            if anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anchor"));
                            }
                            anchor__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AnchorByHeightResponse {
                    anchor: anchor__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.AnchorByHeightResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ArchivedNullifierProofRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.generation_index != 0 {
            len += 1;
        }
        if self.nullifier.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.ArchivedNullifierProofRequest", len)?;
        if self.generation_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationIndex", ToString::to_string(&self.generation_index).as_str())?;
        }
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ArchivedNullifierProofRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "generation_index",
            "generationIndex",
            "nullifier",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GenerationIndex,
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
                            "generationIndex" | "generation_index" => Ok(GeneratedField::GenerationIndex),
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
            type Value = ArchivedNullifierProofRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.ArchivedNullifierProofRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ArchivedNullifierProofRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut generation_index__ = None;
                let mut nullifier__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GenerationIndex => {
                            if generation_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationIndex"));
                            }
                            generation_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
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
                Ok(ArchivedNullifierProofRequest {
                    generation_index: generation_index__.unwrap_or_default(),
                    nullifier: nullifier__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.ArchivedNullifierProofRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ArchivedNullifierProofResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.protocol_version != 0 {
            len += 1;
        }
        if self.generation_index != 0 {
            len += 1;
        }
        if !self.generation_root.is_empty() {
            len += 1;
        }
        if self.generation_start_position != 0 {
            len += 1;
        }
        if self.generation_end_position != 0 {
            len += 1;
        }
        if self.witness.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.ArchivedNullifierProofResponse", len)?;
        if self.protocol_version != 0 {
            struct_ser.serialize_field("protocolVersion", &self.protocol_version)?;
        }
        if self.generation_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationIndex", ToString::to_string(&self.generation_index).as_str())?;
        }
        if !self.generation_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationRoot", pbjson::private::base64::encode(&self.generation_root).as_str())?;
        }
        if self.generation_start_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationStartPosition", ToString::to_string(&self.generation_start_position).as_str())?;
        }
        if self.generation_end_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationEndPosition", ToString::to_string(&self.generation_end_position).as_str())?;
        }
        if let Some(v) = self.witness.as_ref() {
            struct_ser.serialize_field("witness", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ArchivedNullifierProofResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "protocol_version",
            "protocolVersion",
            "generation_index",
            "generationIndex",
            "generation_root",
            "generationRoot",
            "generation_start_position",
            "generationStartPosition",
            "generation_end_position",
            "generationEndPosition",
            "witness",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProtocolVersion,
            GenerationIndex,
            GenerationRoot,
            GenerationStartPosition,
            GenerationEndPosition,
            Witness,
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
                            "protocolVersion" | "protocol_version" => Ok(GeneratedField::ProtocolVersion),
                            "generationIndex" | "generation_index" => Ok(GeneratedField::GenerationIndex),
                            "generationRoot" | "generation_root" => Ok(GeneratedField::GenerationRoot),
                            "generationStartPosition" | "generation_start_position" => Ok(GeneratedField::GenerationStartPosition),
                            "generationEndPosition" | "generation_end_position" => Ok(GeneratedField::GenerationEndPosition),
                            "witness" => Ok(GeneratedField::Witness),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ArchivedNullifierProofResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.ArchivedNullifierProofResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ArchivedNullifierProofResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut protocol_version__ = None;
                let mut generation_index__ = None;
                let mut generation_root__ = None;
                let mut generation_start_position__ = None;
                let mut generation_end_position__ = None;
                let mut witness__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProtocolVersion => {
                            if protocol_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocolVersion"));
                            }
                            protocol_version__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationIndex => {
                            if generation_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationIndex"));
                            }
                            generation_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationRoot => {
                            if generation_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationRoot"));
                            }
                            generation_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationStartPosition => {
                            if generation_start_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationStartPosition"));
                            }
                            generation_start_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationEndPosition => {
                            if generation_end_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationEndPosition"));
                            }
                            generation_end_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Witness => {
                            if witness__.is_some() {
                                return Err(serde::de::Error::duplicate_field("witness"));
                            }
                            witness__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ArchivedNullifierProofResponse {
                    protocol_version: protocol_version__.unwrap_or_default(),
                    generation_index: generation_index__.unwrap_or_default(),
                    generation_root: generation_root__.unwrap_or_default(),
                    generation_start_position: generation_start_position__.unwrap_or_default(),
                    generation_end_position: generation_end_position__.unwrap_or_default(),
                    witness: witness__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.ArchivedNullifierProofResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CommitmentSource {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.CommitmentSource", len)?;
        if let Some(v) = self.source.as_ref() {
            match v {
                commitment_source::Source::Transaction(v) => {
                    struct_ser.serialize_field("transaction", v)?;
                }
                commitment_source::Source::Ics20Transfer(v) => {
                    struct_ser.serialize_field("ics20Transfer", v)?;
                }
                commitment_source::Source::Genesis(v) => {
                    struct_ser.serialize_field("genesis", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CommitmentSource {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction",
            "ics_20_transfer",
            "ics20Transfer",
            "genesis",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transaction,
            Ics20Transfer,
            Genesis,
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
                            "ics20Transfer" | "ics_20_transfer" => Ok(GeneratedField::Ics20Transfer),
                            "genesis" => Ok(GeneratedField::Genesis),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CommitmentSource;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.CommitmentSource")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CommitmentSource, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut source__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transaction => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transaction"));
                            }
                            source__ = map_.next_value::<::std::option::Option<_>>()?.map(commitment_source::Source::Transaction)
;
                        }
                        GeneratedField::Ics20Transfer => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ics20Transfer"));
                            }
                            source__ = map_.next_value::<::std::option::Option<_>>()?.map(commitment_source::Source::Ics20Transfer)
;
                        }
                        GeneratedField::Genesis => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("genesis"));
                            }
                            source__ = map_.next_value::<::std::option::Option<_>>()?.map(commitment_source::Source::Genesis)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CommitmentSource {
                    source: source__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.CommitmentSource", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for commitment_source::Genesis {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Genesis", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for commitment_source::Genesis {
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
            type Value = commitment_source::Genesis;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.CommitmentSource.Genesis")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<commitment_source::Genesis, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(commitment_source::Genesis {
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Genesis", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for commitment_source::Ics20Transfer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.packet_seq != 0 {
            len += 1;
        }
        if !self.channel_id.is_empty() {
            len += 1;
        }
        if !self.sender.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Ics20Transfer", len)?;
        if self.packet_seq != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("packetSeq", ToString::to_string(&self.packet_seq).as_str())?;
        }
        if !self.channel_id.is_empty() {
            struct_ser.serialize_field("channelId", &self.channel_id)?;
        }
        if !self.sender.is_empty() {
            struct_ser.serialize_field("sender", &self.sender)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for commitment_source::Ics20Transfer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "packet_seq",
            "packetSeq",
            "channel_id",
            "channelId",
            "sender",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PacketSeq,
            ChannelId,
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
                            "packetSeq" | "packet_seq" => Ok(GeneratedField::PacketSeq),
                            "channelId" | "channel_id" => Ok(GeneratedField::ChannelId),
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
            type Value = commitment_source::Ics20Transfer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.CommitmentSource.Ics20Transfer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<commitment_source::Ics20Transfer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut packet_seq__ = None;
                let mut channel_id__ = None;
                let mut sender__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PacketSeq => {
                            if packet_seq__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetSeq"));
                            }
                            packet_seq__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ChannelId => {
                            if channel_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("channelId"));
                            }
                            channel_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Sender => {
                            if sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender"));
                            }
                            sender__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(commitment_source::Ics20Transfer {
                    packet_seq: packet_seq__.unwrap_or_default(),
                    channel_id: channel_id__.unwrap_or_default(),
                    sender: sender__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Ics20Transfer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for commitment_source::Transaction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Transaction", len)?;
        if !self.id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("id", pbjson::private::base64::encode(&self.id).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for commitment_source::Transaction {
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
            type Value = commitment_source::Transaction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.CommitmentSource.Transaction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<commitment_source::Transaction, V::Error>
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
                            id__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(commitment_source::Transaction {
                    id: id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.CommitmentSource.Transaction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Epoch {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.index != 0 {
            len += 1;
        }
        if self.start_height != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.Epoch", len)?;
        if self.index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("index", ToString::to_string(&self.index).as_str())?;
        }
        if self.start_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startHeight", ToString::to_string(&self.start_height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Epoch {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "index",
            "start_height",
            "startHeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Index,
            StartHeight,
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
                            "index" => Ok(GeneratedField::Index),
                            "startHeight" | "start_height" => Ok(GeneratedField::StartHeight),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Epoch;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.Epoch")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Epoch, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut index__ = None;
                let mut start_height__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Index => {
                            if index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("index"));
                            }
                            index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartHeight => {
                            if start_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startHeight"));
                            }
                            start_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Epoch {
                    index: index__.unwrap_or_default(),
                    start_height: start_height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.Epoch", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EpochByHeightRequest {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EpochByHeightRequest", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EpochByHeightRequest {
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
            type Value = EpochByHeightRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EpochByHeightRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EpochByHeightRequest, V::Error>
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
                Ok(EpochByHeightRequest {
                    height: height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EpochByHeightRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EpochByHeightResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.epoch.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EpochByHeightResponse", len)?;
        if let Some(v) = self.epoch.as_ref() {
            struct_ser.serialize_field("epoch", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EpochByHeightResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "epoch",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Epoch,
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
                            "epoch" => Ok(GeneratedField::Epoch),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EpochByHeightResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EpochByHeightResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EpochByHeightResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut epoch__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Epoch => {
                            if epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epoch"));
                            }
                            epoch__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EpochByHeightResponse {
                    epoch: epoch__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EpochByHeightResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventAnchor {
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
        if self.height != 0 {
            len += 1;
        }
        if self.timestamp.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventAnchor", len)?;
        if let Some(v) = self.anchor.as_ref() {
            struct_ser.serialize_field("anchor", v)?;
        }
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.timestamp.as_ref() {
            struct_ser.serialize_field("timestamp", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventAnchor {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "anchor",
            "height",
            "timestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Anchor,
            Height,
            Timestamp,
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
                            "height" => Ok(GeneratedField::Height),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventAnchor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventAnchor")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventAnchor, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut anchor__ = None;
                let mut height__ = None;
                let mut timestamp__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Anchor => {
                            if anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anchor"));
                            }
                            anchor__ = map_.next_value()?;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventAnchor {
                    anchor: anchor__,
                    height: height__.unwrap_or_default(),
                    timestamp: timestamp__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventAnchor", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventBlockRoot {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.root.is_some() {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.timestamp.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventBlockRoot", len)?;
        if let Some(v) = self.root.as_ref() {
            struct_ser.serialize_field("root", v)?;
        }
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.timestamp.as_ref() {
            struct_ser.serialize_field("timestamp", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventBlockRoot {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "root",
            "height",
            "timestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Root,
            Height,
            Timestamp,
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
                            "root" => Ok(GeneratedField::Root),
                            "height" => Ok(GeneratedField::Height),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventBlockRoot;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventBlockRoot")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventBlockRoot, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut root__ = None;
                let mut height__ = None;
                let mut timestamp__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Root => {
                            if root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("root"));
                            }
                            root__ = map_.next_value()?;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventBlockRoot {
                    root: root__,
                    height: height__.unwrap_or_default(),
                    timestamp: timestamp__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventBlockRoot", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventCommitment {
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
        if self.position != 0 {
            len += 1;
        }
        if self.source.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventCommitment", len)?;
        if let Some(v) = self.commitment.as_ref() {
            struct_ser.serialize_field("commitment", v)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if let Some(v) = self.source.as_ref() {
            struct_ser.serialize_field("source", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventCommitment {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "commitment",
            "position",
            "source",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Commitment,
            Position,
            Source,
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
                            "position" => Ok(GeneratedField::Position),
                            "source" => Ok(GeneratedField::Source),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventCommitment;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventCommitment")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventCommitment, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut commitment__ = None;
                let mut position__ = None;
                let mut source__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
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
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventCommitment {
                    commitment: commitment__,
                    position: position__.unwrap_or_default(),
                    source: source__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventCommitment", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventEpochRoot {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.root.is_some() {
            len += 1;
        }
        if self.index != 0 {
            len += 1;
        }
        if self.timestamp.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventEpochRoot", len)?;
        if let Some(v) = self.root.as_ref() {
            struct_ser.serialize_field("root", v)?;
        }
        if self.index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("index", ToString::to_string(&self.index).as_str())?;
        }
        if let Some(v) = self.timestamp.as_ref() {
            struct_ser.serialize_field("timestamp", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventEpochRoot {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "root",
            "index",
            "timestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Root,
            Index,
            Timestamp,
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
                            "root" => Ok(GeneratedField::Root),
                            "index" => Ok(GeneratedField::Index),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventEpochRoot;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventEpochRoot")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventEpochRoot, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut root__ = None;
                let mut index__ = None;
                let mut timestamp__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Root => {
                            if root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("root"));
                            }
                            root__ = map_.next_value()?;
                        }
                        GeneratedField::Index => {
                            if index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("index"));
                            }
                            index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventEpochRoot {
                    root: root__,
                    index: index__.unwrap_or_default(),
                    timestamp: timestamp__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventEpochRoot", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventNullifierGenerationArchived {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.generation_index != 0 {
            len += 1;
        }
        if self.generation_start_position != 0 {
            len += 1;
        }
        if self.generation_end_position != 0 {
            len += 1;
        }
        if !self.generation_root.is_empty() {
            len += 1;
        }
        if self.archived_generation_count != 0 {
            len += 1;
        }
        if !self.archived_history_head.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventNullifierGenerationArchived", len)?;
        if self.generation_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationIndex", ToString::to_string(&self.generation_index).as_str())?;
        }
        if self.generation_start_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationStartPosition", ToString::to_string(&self.generation_start_position).as_str())?;
        }
        if self.generation_end_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationEndPosition", ToString::to_string(&self.generation_end_position).as_str())?;
        }
        if !self.generation_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationRoot", pbjson::private::base64::encode(&self.generation_root).as_str())?;
        }
        if self.archived_generation_count != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedGenerationCount", ToString::to_string(&self.archived_generation_count).as_str())?;
        }
        if !self.archived_history_head.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedHistoryHead", pbjson::private::base64::encode(&self.archived_history_head).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventNullifierGenerationArchived {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "generation_index",
            "generationIndex",
            "generation_start_position",
            "generationStartPosition",
            "generation_end_position",
            "generationEndPosition",
            "generation_root",
            "generationRoot",
            "archived_generation_count",
            "archivedGenerationCount",
            "archived_history_head",
            "archivedHistoryHead",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GenerationIndex,
            GenerationStartPosition,
            GenerationEndPosition,
            GenerationRoot,
            ArchivedGenerationCount,
            ArchivedHistoryHead,
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
                            "generationIndex" | "generation_index" => Ok(GeneratedField::GenerationIndex),
                            "generationStartPosition" | "generation_start_position" => Ok(GeneratedField::GenerationStartPosition),
                            "generationEndPosition" | "generation_end_position" => Ok(GeneratedField::GenerationEndPosition),
                            "generationRoot" | "generation_root" => Ok(GeneratedField::GenerationRoot),
                            "archivedGenerationCount" | "archived_generation_count" => Ok(GeneratedField::ArchivedGenerationCount),
                            "archivedHistoryHead" | "archived_history_head" => Ok(GeneratedField::ArchivedHistoryHead),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventNullifierGenerationArchived;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventNullifierGenerationArchived")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventNullifierGenerationArchived, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut generation_index__ = None;
                let mut generation_start_position__ = None;
                let mut generation_end_position__ = None;
                let mut generation_root__ = None;
                let mut archived_generation_count__ = None;
                let mut archived_history_head__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GenerationIndex => {
                            if generation_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationIndex"));
                            }
                            generation_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationStartPosition => {
                            if generation_start_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationStartPosition"));
                            }
                            generation_start_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationEndPosition => {
                            if generation_end_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationEndPosition"));
                            }
                            generation_end_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationRoot => {
                            if generation_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationRoot"));
                            }
                            generation_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedGenerationCount => {
                            if archived_generation_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedGenerationCount"));
                            }
                            archived_generation_count__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedHistoryHead => {
                            if archived_history_head__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedHistoryHead"));
                            }
                            archived_history_head__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventNullifierGenerationArchived {
                    generation_index: generation_index__.unwrap_or_default(),
                    generation_start_position: generation_start_position__.unwrap_or_default(),
                    generation_end_position: generation_end_position__.unwrap_or_default(),
                    generation_root: generation_root__.unwrap_or_default(),
                    archived_generation_count: archived_generation_count__.unwrap_or_default(),
                    archived_history_head: archived_history_head__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventNullifierGenerationArchived", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventNullifierGenerationFrozen {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.generation_index != 0 {
            len += 1;
        }
        if self.application_epoch_start != 0 {
            len += 1;
        }
        if self.application_epoch_end != 0 {
            len += 1;
        }
        if self.generation_start_position != 0 {
            len += 1;
        }
        if self.generation_end_position != 0 {
            len += 1;
        }
        if !self.generation_root.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.EventNullifierGenerationFrozen", len)?;
        if self.generation_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationIndex", ToString::to_string(&self.generation_index).as_str())?;
        }
        if self.application_epoch_start != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("applicationEpochStart", ToString::to_string(&self.application_epoch_start).as_str())?;
        }
        if self.application_epoch_end != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("applicationEpochEnd", ToString::to_string(&self.application_epoch_end).as_str())?;
        }
        if self.generation_start_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationStartPosition", ToString::to_string(&self.generation_start_position).as_str())?;
        }
        if self.generation_end_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationEndPosition", ToString::to_string(&self.generation_end_position).as_str())?;
        }
        if !self.generation_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationRoot", pbjson::private::base64::encode(&self.generation_root).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventNullifierGenerationFrozen {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "generation_index",
            "generationIndex",
            "application_epoch_start",
            "applicationEpochStart",
            "application_epoch_end",
            "applicationEpochEnd",
            "generation_start_position",
            "generationStartPosition",
            "generation_end_position",
            "generationEndPosition",
            "generation_root",
            "generationRoot",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GenerationIndex,
            ApplicationEpochStart,
            ApplicationEpochEnd,
            GenerationStartPosition,
            GenerationEndPosition,
            GenerationRoot,
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
                            "generationIndex" | "generation_index" => Ok(GeneratedField::GenerationIndex),
                            "applicationEpochStart" | "application_epoch_start" => Ok(GeneratedField::ApplicationEpochStart),
                            "applicationEpochEnd" | "application_epoch_end" => Ok(GeneratedField::ApplicationEpochEnd),
                            "generationStartPosition" | "generation_start_position" => Ok(GeneratedField::GenerationStartPosition),
                            "generationEndPosition" | "generation_end_position" => Ok(GeneratedField::GenerationEndPosition),
                            "generationRoot" | "generation_root" => Ok(GeneratedField::GenerationRoot),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventNullifierGenerationFrozen;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.EventNullifierGenerationFrozen")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventNullifierGenerationFrozen, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut generation_index__ = None;
                let mut application_epoch_start__ = None;
                let mut application_epoch_end__ = None;
                let mut generation_start_position__ = None;
                let mut generation_end_position__ = None;
                let mut generation_root__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GenerationIndex => {
                            if generation_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationIndex"));
                            }
                            generation_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ApplicationEpochStart => {
                            if application_epoch_start__.is_some() {
                                return Err(serde::de::Error::duplicate_field("applicationEpochStart"));
                            }
                            application_epoch_start__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ApplicationEpochEnd => {
                            if application_epoch_end__.is_some() {
                                return Err(serde::de::Error::duplicate_field("applicationEpochEnd"));
                            }
                            application_epoch_end__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationStartPosition => {
                            if generation_start_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationStartPosition"));
                            }
                            generation_start_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationEndPosition => {
                            if generation_end_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationEndPosition"));
                            }
                            generation_end_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationRoot => {
                            if generation_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationRoot"));
                            }
                            generation_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventNullifierGenerationFrozen {
                    generation_index: generation_index__.unwrap_or_default(),
                    application_epoch_start: application_epoch_start__.unwrap_or_default(),
                    application_epoch_end: application_epoch_end__.unwrap_or_default(),
                    generation_start_position: generation_start_position__.unwrap_or_default(),
                    generation_end_position: generation_end_position__.unwrap_or_default(),
                    generation_root: generation_root__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.EventNullifierGenerationFrozen", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GenerationNonmembershipProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.generation_index != 0 {
            len += 1;
        }
        if !self.generation_root.is_empty() {
            len += 1;
        }
        if self.generation_start_position != 0 {
            len += 1;
        }
        if self.generation_end_position != 0 {
            len += 1;
        }
        if !self.groth16_proof.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.GenerationNonmembershipProof", len)?;
        if self.generation_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationIndex", ToString::to_string(&self.generation_index).as_str())?;
        }
        if !self.generation_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationRoot", pbjson::private::base64::encode(&self.generation_root).as_str())?;
        }
        if self.generation_start_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationStartPosition", ToString::to_string(&self.generation_start_position).as_str())?;
        }
        if self.generation_end_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generationEndPosition", ToString::to_string(&self.generation_end_position).as_str())?;
        }
        if !self.groth16_proof.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("groth16Proof", pbjson::private::base64::encode(&self.groth16_proof).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GenerationNonmembershipProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "generation_index",
            "generationIndex",
            "generation_root",
            "generationRoot",
            "generation_start_position",
            "generationStartPosition",
            "generation_end_position",
            "generationEndPosition",
            "groth16_proof",
            "groth16Proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GenerationIndex,
            GenerationRoot,
            GenerationStartPosition,
            GenerationEndPosition,
            Groth16Proof,
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
                            "generationIndex" | "generation_index" => Ok(GeneratedField::GenerationIndex),
                            "generationRoot" | "generation_root" => Ok(GeneratedField::GenerationRoot),
                            "generationStartPosition" | "generation_start_position" => Ok(GeneratedField::GenerationStartPosition),
                            "generationEndPosition" | "generation_end_position" => Ok(GeneratedField::GenerationEndPosition),
                            "groth16Proof" | "groth16_proof" => Ok(GeneratedField::Groth16Proof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GenerationNonmembershipProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.GenerationNonmembershipProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenerationNonmembershipProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut generation_index__ = None;
                let mut generation_root__ = None;
                let mut generation_start_position__ = None;
                let mut generation_end_position__ = None;
                let mut groth16_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GenerationIndex => {
                            if generation_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationIndex"));
                            }
                            generation_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationRoot => {
                            if generation_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationRoot"));
                            }
                            generation_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationStartPosition => {
                            if generation_start_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationStartPosition"));
                            }
                            generation_start_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GenerationEndPosition => {
                            if generation_end_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generationEndPosition"));
                            }
                            generation_end_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Groth16Proof => {
                            if groth16_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groth16Proof"));
                            }
                            groth16_proof__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenerationNonmembershipProof {
                    generation_index: generation_index__.unwrap_or_default(),
                    generation_root: generation_root__.unwrap_or_default(),
                    generation_start_position: generation_start_position__.unwrap_or_default(),
                    generation_end_position: generation_end_position__.unwrap_or_default(),
                    groth16_proof: groth16_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.GenerationNonmembershipProof", FIELDS, GeneratedVisitor)
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
        if self.sct_params.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.GenesisContent", len)?;
        if let Some(v) = self.sct_params.as_ref() {
            struct_ser.serialize_field("sctParams", v)?;
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
            "sct_params",
            "sctParams",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SctParams,
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
                            "sctParams" | "sct_params" => Ok(GeneratedField::SctParams),
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
                formatter.write_str("struct shieldd.core.component.sct.v1.GenesisContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sct_params__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SctParams => {
                            if sct_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sctParams"));
                            }
                            sct_params__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisContent {
                    sct_params: sct_params__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.GenesisContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HistoricalChunkProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.chunk_index != 0 {
            len += 1;
        }
        if !self.end_history_head.is_empty() {
            len += 1;
        }
        if !self.groth16_proof.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.HistoricalChunkProof", len)?;
        if self.chunk_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("chunkIndex", ToString::to_string(&self.chunk_index).as_str())?;
        }
        if !self.end_history_head.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("endHistoryHead", pbjson::private::base64::encode(&self.end_history_head).as_str())?;
        }
        if !self.groth16_proof.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("groth16Proof", pbjson::private::base64::encode(&self.groth16_proof).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HistoricalChunkProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "chunk_index",
            "chunkIndex",
            "end_history_head",
            "endHistoryHead",
            "groth16_proof",
            "groth16Proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ChunkIndex,
            EndHistoryHead,
            Groth16Proof,
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
                            "chunkIndex" | "chunk_index" => Ok(GeneratedField::ChunkIndex),
                            "endHistoryHead" | "end_history_head" => Ok(GeneratedField::EndHistoryHead),
                            "groth16Proof" | "groth16_proof" => Ok(GeneratedField::Groth16Proof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = HistoricalChunkProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.HistoricalChunkProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HistoricalChunkProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut chunk_index__ = None;
                let mut end_history_head__ = None;
                let mut groth16_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ChunkIndex => {
                            if chunk_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chunkIndex"));
                            }
                            chunk_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndHistoryHead => {
                            if end_history_head__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endHistoryHead"));
                            }
                            end_history_head__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Groth16Proof => {
                            if groth16_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groth16Proof"));
                            }
                            groth16_proof__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(HistoricalChunkProof {
                    chunk_index: chunk_index__.unwrap_or_default(),
                    end_history_head: end_history_head__.unwrap_or_default(),
                    groth16_proof: groth16_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.HistoricalChunkProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HistoricalNullifierProof {
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
        if !self.completed_chunks.is_empty() {
            len += 1;
        }
        if !self.tail.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.HistoricalNullifierProof", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if !self.completed_chunks.is_empty() {
            struct_ser.serialize_field("completedChunks", &self.completed_chunks)?;
        }
        if !self.tail.is_empty() {
            struct_ser.serialize_field("tail", &self.tail)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HistoricalNullifierProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nullifier",
            "completed_chunks",
            "completedChunks",
            "tail",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            CompletedChunks,
            Tail,
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
                            "completedChunks" | "completed_chunks" => Ok(GeneratedField::CompletedChunks),
                            "tail" => Ok(GeneratedField::Tail),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = HistoricalNullifierProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.HistoricalNullifierProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HistoricalNullifierProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut completed_chunks__ = None;
                let mut tail__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::CompletedChunks => {
                            if completed_chunks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("completedChunks"));
                            }
                            completed_chunks__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Tail => {
                            if tail__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tail"));
                            }
                            tail__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(HistoricalNullifierProof {
                    nullifier: nullifier__,
                    completed_chunks: completed_chunks__.unwrap_or_default(),
                    tail: tail__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.HistoricalNullifierProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexedNullifierLeaf {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.value.is_empty() {
            len += 1;
        }
        if self.next_index != 0 {
            len += 1;
        }
        if !self.next_value.is_empty() {
            len += 1;
        }
        if self.is_lower_sentinel {
            len += 1;
        }
        if self.is_terminal {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.IndexedNullifierLeaf", len)?;
        if !self.value.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("value", pbjson::private::base64::encode(&self.value).as_str())?;
        }
        if self.next_index != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nextIndex", ToString::to_string(&self.next_index).as_str())?;
        }
        if !self.next_value.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nextValue", pbjson::private::base64::encode(&self.next_value).as_str())?;
        }
        if self.is_lower_sentinel {
            struct_ser.serialize_field("isLowerSentinel", &self.is_lower_sentinel)?;
        }
        if self.is_terminal {
            struct_ser.serialize_field("isTerminal", &self.is_terminal)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexedNullifierLeaf {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "value",
            "next_index",
            "nextIndex",
            "next_value",
            "nextValue",
            "is_lower_sentinel",
            "isLowerSentinel",
            "is_terminal",
            "isTerminal",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            NextIndex,
            NextValue,
            IsLowerSentinel,
            IsTerminal,
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
                            "nextIndex" | "next_index" => Ok(GeneratedField::NextIndex),
                            "nextValue" | "next_value" => Ok(GeneratedField::NextValue),
                            "isLowerSentinel" | "is_lower_sentinel" => Ok(GeneratedField::IsLowerSentinel),
                            "isTerminal" | "is_terminal" => Ok(GeneratedField::IsTerminal),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexedNullifierLeaf;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.IndexedNullifierLeaf")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexedNullifierLeaf, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut next_index__ = None;
                let mut next_value__ = None;
                let mut is_lower_sentinel__ = None;
                let mut is_terminal__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NextIndex => {
                            if next_index__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nextIndex"));
                            }
                            next_index__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NextValue => {
                            if next_value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nextValue"));
                            }
                            next_value__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsLowerSentinel => {
                            if is_lower_sentinel__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isLowerSentinel"));
                            }
                            is_lower_sentinel__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsTerminal => {
                            if is_terminal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isTerminal"));
                            }
                            is_terminal__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexedNullifierLeaf {
                    value: value__.unwrap_or_default(),
                    next_index: next_index__.unwrap_or_default(),
                    next_value: next_value__.unwrap_or_default(),
                    is_lower_sentinel: is_lower_sentinel__.unwrap_or_default(),
                    is_terminal: is_terminal__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.IndexedNullifierLeaf", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexedNullifierPathLayer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.siblings.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.IndexedNullifierPathLayer", len)?;
        if !self.siblings.is_empty() {
            struct_ser.serialize_field("siblings", &self.siblings.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexedNullifierPathLayer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "siblings",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Siblings,
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
                            "siblings" => Ok(GeneratedField::Siblings),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexedNullifierPathLayer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.IndexedNullifierPathLayer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexedNullifierPathLayer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut siblings__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Siblings => {
                            if siblings__.is_some() {
                                return Err(serde::de::Error::duplicate_field("siblings"));
                            }
                            siblings__ =
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexedNullifierPathLayer {
                    siblings: siblings__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.IndexedNullifierPathLayer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexedNullifierWitness {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.leaf_position != 0 {
            len += 1;
        }
        if self.leaf.is_some() {
            len += 1;
        }
        if !self.auth_path.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.IndexedNullifierWitness", len)?;
        if self.leaf_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("leafPosition", ToString::to_string(&self.leaf_position).as_str())?;
        }
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if !self.auth_path.is_empty() {
            struct_ser.serialize_field("authPath", &self.auth_path)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexedNullifierWitness {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "leaf_position",
            "leafPosition",
            "leaf",
            "auth_path",
            "authPath",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LeafPosition,
            Leaf,
            AuthPath,
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
                            "leafPosition" | "leaf_position" => Ok(GeneratedField::LeafPosition),
                            "leaf" => Ok(GeneratedField::Leaf),
                            "authPath" | "auth_path" => Ok(GeneratedField::AuthPath),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexedNullifierWitness;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.IndexedNullifierWitness")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexedNullifierWitness, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut leaf_position__ = None;
                let mut leaf__ = None;
                let mut auth_path__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LeafPosition => {
                            if leaf_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leafPosition"));
                            }
                            leaf_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::AuthPath => {
                            if auth_path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authPath"));
                            }
                            auth_path__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexedNullifierWitness {
                    leaf_position: leaf_position__.unwrap_or_default(),
                    leaf: leaf__,
                    auth_path: auth_path__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.IndexedNullifierWitness", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Nullifier {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.Nullifier", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Nullifier {
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
            type Value = Nullifier;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.Nullifier")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Nullifier, V::Error>
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
                Ok(Nullifier {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.Nullifier", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierGenerationState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.protocol_version != 0 {
            len += 1;
        }
        if self.activation_epoch != 0 {
            len += 1;
        }
        if self.current_generation != 0 {
            len += 1;
        }
        if self.current_tree.is_some() {
            len += 1;
        }
        if !self.current_root.is_empty() {
            len += 1;
        }
        if self.current_start_position != 0 {
            len += 1;
        }
        if self.previous_tree.is_some() {
            len += 1;
        }
        if !self.previous_root.is_empty() {
            len += 1;
        }
        if self.recent_position_floor != 0 {
            len += 1;
        }
        if self.archived_generation_count != 0 {
            len += 1;
        }
        if !self.archived_history_head.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierGenerationState", len)?;
        if self.protocol_version != 0 {
            struct_ser.serialize_field("protocolVersion", &self.protocol_version)?;
        }
        if self.activation_epoch != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("activationEpoch", ToString::to_string(&self.activation_epoch).as_str())?;
        }
        if self.current_generation != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("currentGeneration", ToString::to_string(&self.current_generation).as_str())?;
        }
        if let Some(v) = self.current_tree.as_ref() {
            struct_ser.serialize_field("currentTree", v)?;
        }
        if !self.current_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("currentRoot", pbjson::private::base64::encode(&self.current_root).as_str())?;
        }
        if self.current_start_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("currentStartPosition", ToString::to_string(&self.current_start_position).as_str())?;
        }
        if let Some(v) = self.previous_tree.as_ref() {
            struct_ser.serialize_field("previousTree", v)?;
        }
        if !self.previous_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("previousRoot", pbjson::private::base64::encode(&self.previous_root).as_str())?;
        }
        if self.recent_position_floor != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("recentPositionFloor", ToString::to_string(&self.recent_position_floor).as_str())?;
        }
        if self.archived_generation_count != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedGenerationCount", ToString::to_string(&self.archived_generation_count).as_str())?;
        }
        if !self.archived_history_head.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedHistoryHead", pbjson::private::base64::encode(&self.archived_history_head).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierGenerationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "protocol_version",
            "protocolVersion",
            "activation_epoch",
            "activationEpoch",
            "current_generation",
            "currentGeneration",
            "current_tree",
            "currentTree",
            "current_root",
            "currentRoot",
            "current_start_position",
            "currentStartPosition",
            "previous_tree",
            "previousTree",
            "previous_root",
            "previousRoot",
            "recent_position_floor",
            "recentPositionFloor",
            "archived_generation_count",
            "archivedGenerationCount",
            "archived_history_head",
            "archivedHistoryHead",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProtocolVersion,
            ActivationEpoch,
            CurrentGeneration,
            CurrentTree,
            CurrentRoot,
            CurrentStartPosition,
            PreviousTree,
            PreviousRoot,
            RecentPositionFloor,
            ArchivedGenerationCount,
            ArchivedHistoryHead,
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
                            "protocolVersion" | "protocol_version" => Ok(GeneratedField::ProtocolVersion),
                            "activationEpoch" | "activation_epoch" => Ok(GeneratedField::ActivationEpoch),
                            "currentGeneration" | "current_generation" => Ok(GeneratedField::CurrentGeneration),
                            "currentTree" | "current_tree" => Ok(GeneratedField::CurrentTree),
                            "currentRoot" | "current_root" => Ok(GeneratedField::CurrentRoot),
                            "currentStartPosition" | "current_start_position" => Ok(GeneratedField::CurrentStartPosition),
                            "previousTree" | "previous_tree" => Ok(GeneratedField::PreviousTree),
                            "previousRoot" | "previous_root" => Ok(GeneratedField::PreviousRoot),
                            "recentPositionFloor" | "recent_position_floor" => Ok(GeneratedField::RecentPositionFloor),
                            "archivedGenerationCount" | "archived_generation_count" => Ok(GeneratedField::ArchivedGenerationCount),
                            "archivedHistoryHead" | "archived_history_head" => Ok(GeneratedField::ArchivedHistoryHead),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierGenerationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierGenerationState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierGenerationState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut protocol_version__ = None;
                let mut activation_epoch__ = None;
                let mut current_generation__ = None;
                let mut current_tree__ = None;
                let mut current_root__ = None;
                let mut current_start_position__ = None;
                let mut previous_tree__ = None;
                let mut previous_root__ = None;
                let mut recent_position_floor__ = None;
                let mut archived_generation_count__ = None;
                let mut archived_history_head__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProtocolVersion => {
                            if protocol_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocolVersion"));
                            }
                            protocol_version__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ActivationEpoch => {
                            if activation_epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activationEpoch"));
                            }
                            activation_epoch__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CurrentGeneration => {
                            if current_generation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentGeneration"));
                            }
                            current_generation__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CurrentTree => {
                            if current_tree__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentTree"));
                            }
                            current_tree__ = map_.next_value()?;
                        }
                        GeneratedField::CurrentRoot => {
                            if current_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentRoot"));
                            }
                            current_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CurrentStartPosition => {
                            if current_start_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentStartPosition"));
                            }
                            current_start_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PreviousTree => {
                            if previous_tree__.is_some() {
                                return Err(serde::de::Error::duplicate_field("previousTree"));
                            }
                            previous_tree__ = map_.next_value()?;
                        }
                        GeneratedField::PreviousRoot => {
                            if previous_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("previousRoot"));
                            }
                            previous_root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RecentPositionFloor => {
                            if recent_position_floor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recentPositionFloor"));
                            }
                            recent_position_floor__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedGenerationCount => {
                            if archived_generation_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedGenerationCount"));
                            }
                            archived_generation_count__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedHistoryHead => {
                            if archived_history_head__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedHistoryHead"));
                            }
                            archived_history_head__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierGenerationState {
                    protocol_version: protocol_version__.unwrap_or_default(),
                    activation_epoch: activation_epoch__.unwrap_or_default(),
                    current_generation: current_generation__.unwrap_or_default(),
                    current_tree: current_tree__,
                    current_root: current_root__.unwrap_or_default(),
                    current_start_position: current_start_position__.unwrap_or_default(),
                    previous_tree: previous_tree__,
                    previous_root: previous_root__.unwrap_or_default(),
                    recent_position_floor: recent_position_floor__.unwrap_or_default(),
                    archived_generation_count: archived_generation_count__.unwrap_or_default(),
                    archived_history_head: archived_history_head__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierGenerationState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierRequest {
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
        if self.with_proof {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierRequest", len)?;
        if let Some(v) = self.nullifier.as_ref() {
            struct_ser.serialize_field("nullifier", v)?;
        }
        if self.with_proof {
            struct_ser.serialize_field("withProof", &self.with_proof)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nullifier",
            "with_proof",
            "withProof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Nullifier,
            WithProof,
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
                            "withProof" | "with_proof" => Ok(GeneratedField::WithProof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nullifier__ = None;
                let mut with_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Nullifier => {
                            if nullifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nullifier"));
                            }
                            nullifier__ = map_.next_value()?;
                        }
                        GeneratedField::WithProof => {
                            if with_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withProof"));
                            }
                            with_proof__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierRequest {
                    nullifier: nullifier__,
                    with_proof: with_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierResponse {
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
        if self.window.is_some() {
            len += 1;
        }
        if !self.active_lookups.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierResponse", len)?;
        if self.spent {
            struct_ser.serialize_field("spent", &self.spent)?;
        }
        if let Some(v) = self.window.as_ref() {
            struct_ser.serialize_field("window", v)?;
        }
        if !self.active_lookups.is_empty() {
            struct_ser.serialize_field("activeLookups", &self.active_lookups)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "spent",
            "window",
            "active_lookups",
            "activeLookups",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Spent,
            Window,
            ActiveLookups,
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
                            "window" => Ok(GeneratedField::Window),
                            "activeLookups" | "active_lookups" => Ok(GeneratedField::ActiveLookups),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut spent__ = None;
                let mut window__ = None;
                let mut active_lookups__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Spent => {
                            if spent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spent"));
                            }
                            spent__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Window => {
                            if window__.is_some() {
                                return Err(serde::de::Error::duplicate_field("window"));
                            }
                            window__ = map_.next_value()?;
                        }
                        GeneratedField::ActiveLookups => {
                            if active_lookups__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activeLookups"));
                            }
                            active_lookups__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierResponse {
                    spent: spent__.unwrap_or_default(),
                    window: window__,
                    active_lookups: active_lookups__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierTreeId {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.generation != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierTreeId", len)?;
        if self.generation != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("generation", ToString::to_string(&self.generation).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierTreeId {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "generation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Generation,
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
                            "generation" => Ok(GeneratedField::Generation),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierTreeId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierTreeId")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierTreeId, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut generation__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Generation => {
                            if generation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("generation"));
                            }
                            generation__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierTreeId {
                    generation: generation__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierTreeId", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierTreeLookup {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.tree.is_some() {
            len += 1;
        }
        if !self.root.is_empty() {
            len += 1;
        }
        if self.spent {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierTreeLookup", len)?;
        if let Some(v) = self.tree.as_ref() {
            struct_ser.serialize_field("tree", v)?;
        }
        if !self.root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("root", pbjson::private::base64::encode(&self.root).as_str())?;
        }
        if self.spent {
            struct_ser.serialize_field("spent", &self.spent)?;
        }
        if let Some(v) = self.proof.as_ref() {
            struct_ser.serialize_field("proof", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierTreeLookup {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "tree",
            "root",
            "spent",
            "proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Tree,
            Root,
            Spent,
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
                            "tree" => Ok(GeneratedField::Tree),
                            "root" => Ok(GeneratedField::Root),
                            "spent" => Ok(GeneratedField::Spent),
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
            type Value = NullifierTreeLookup;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierTreeLookup")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierTreeLookup, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut tree__ = None;
                let mut root__ = None;
                let mut spent__ = None;
                let mut proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Tree => {
                            if tree__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tree"));
                            }
                            tree__ = map_.next_value()?;
                        }
                        GeneratedField::Root => {
                            if root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("root"));
                            }
                            root__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Spent => {
                            if spent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("spent"));
                            }
                            spent__ = Some(map_.next_value()?);
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
                Ok(NullifierTreeLookup {
                    tree: tree__,
                    root: root__.unwrap_or_default(),
                    spent: spent__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierTreeLookup", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NullifierWindow {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.protocol_version != 0 {
            len += 1;
        }
        if self.current_generation != 0 {
            len += 1;
        }
        if self.recent_position_floor != 0 {
            len += 1;
        }
        if self.archived_generation_count != 0 {
            len += 1;
        }
        if !self.archived_history_head.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierWindow", len)?;
        if self.protocol_version != 0 {
            struct_ser.serialize_field("protocolVersion", &self.protocol_version)?;
        }
        if self.current_generation != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("currentGeneration", ToString::to_string(&self.current_generation).as_str())?;
        }
        if self.recent_position_floor != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("recentPositionFloor", ToString::to_string(&self.recent_position_floor).as_str())?;
        }
        if self.archived_generation_count != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedGenerationCount", ToString::to_string(&self.archived_generation_count).as_str())?;
        }
        if !self.archived_history_head.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("archivedHistoryHead", pbjson::private::base64::encode(&self.archived_history_head).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NullifierWindow {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "protocol_version",
            "protocolVersion",
            "current_generation",
            "currentGeneration",
            "recent_position_floor",
            "recentPositionFloor",
            "archived_generation_count",
            "archivedGenerationCount",
            "archived_history_head",
            "archivedHistoryHead",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProtocolVersion,
            CurrentGeneration,
            RecentPositionFloor,
            ArchivedGenerationCount,
            ArchivedHistoryHead,
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
                            "protocolVersion" | "protocol_version" => Ok(GeneratedField::ProtocolVersion),
                            "currentGeneration" | "current_generation" => Ok(GeneratedField::CurrentGeneration),
                            "recentPositionFloor" | "recent_position_floor" => Ok(GeneratedField::RecentPositionFloor),
                            "archivedGenerationCount" | "archived_generation_count" => Ok(GeneratedField::ArchivedGenerationCount),
                            "archivedHistoryHead" | "archived_history_head" => Ok(GeneratedField::ArchivedHistoryHead),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NullifierWindow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierWindow")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NullifierWindow, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut protocol_version__ = None;
                let mut current_generation__ = None;
                let mut recent_position_floor__ = None;
                let mut archived_generation_count__ = None;
                let mut archived_history_head__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProtocolVersion => {
                            if protocol_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocolVersion"));
                            }
                            protocol_version__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CurrentGeneration => {
                            if current_generation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentGeneration"));
                            }
                            current_generation__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RecentPositionFloor => {
                            if recent_position_floor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recentPositionFloor"));
                            }
                            recent_position_floor__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedGenerationCount => {
                            if archived_generation_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedGenerationCount"));
                            }
                            archived_generation_count__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ArchivedHistoryHead => {
                            if archived_history_head__.is_some() {
                                return Err(serde::de::Error::duplicate_field("archivedHistoryHead"));
                            }
                            archived_history_head__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NullifierWindow {
                    protocol_version: protocol_version__.unwrap_or_default(),
                    current_generation: current_generation__.unwrap_or_default(),
                    recent_position_floor: recent_position_floor__.unwrap_or_default(),
                    archived_generation_count: archived_generation_count__.unwrap_or_default(),
                    archived_history_head: archived_history_head__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierWindow", FIELDS, GeneratedVisitor)
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
        let struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierWindowRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierWindowRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierWindowRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.NullifierWindowResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.sct.v1.NullifierWindowResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.NullifierWindowResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SctFrontierRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.with_proof {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.SctFrontierRequest", len)?;
        if self.with_proof {
            struct_ser.serialize_field("withProof", &self.with_proof)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SctFrontierRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "with_proof",
            "withProof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WithProof,
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
                            "withProof" | "with_proof" => Ok(GeneratedField::WithProof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SctFrontierRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.SctFrontierRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SctFrontierRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut with_proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WithProof => {
                            if with_proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("withProof"));
                            }
                            with_proof__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SctFrontierRequest {
                    with_proof: with_proof__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.SctFrontierRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SctFrontierResponse {
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
        if self.anchor.is_some() {
            len += 1;
        }
        if !self.compact_frontier.is_empty() {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.SctFrontierResponse", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if let Some(v) = self.anchor.as_ref() {
            struct_ser.serialize_field("anchor", v)?;
        }
        if !self.compact_frontier.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("compactFrontier", pbjson::private::base64::encode(&self.compact_frontier).as_str())?;
        }
        if let Some(v) = self.proof.as_ref() {
            struct_ser.serialize_field("proof", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SctFrontierResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
            "anchor",
            "compact_frontier",
            "compactFrontier",
            "proof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
            Anchor,
            CompactFrontier,
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
                            "height" => Ok(GeneratedField::Height),
                            "anchor" => Ok(GeneratedField::Anchor),
                            "compactFrontier" | "compact_frontier" => Ok(GeneratedField::CompactFrontier),
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
            type Value = SctFrontierResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.SctFrontierResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SctFrontierResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
                let mut anchor__ = None;
                let mut compact_frontier__ = None;
                let mut proof__ = None;
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
                        GeneratedField::Anchor => {
                            if anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anchor"));
                            }
                            anchor__ = map_.next_value()?;
                        }
                        GeneratedField::CompactFrontier => {
                            if compact_frontier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compactFrontier"));
                            }
                            compact_frontier__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                Ok(SctFrontierResponse {
                    height: height__.unwrap_or_default(),
                    anchor: anchor__,
                    compact_frontier: compact_frontier__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.SctFrontierResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SctParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.epoch_duration != 0 {
            len += 1;
        }
        if self.sct_anchor_retention_blocks != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.SctParameters", len)?;
        if self.epoch_duration != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("epochDuration", ToString::to_string(&self.epoch_duration).as_str())?;
        }
        if self.sct_anchor_retention_blocks != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("sctAnchorRetentionBlocks", ToString::to_string(&self.sct_anchor_retention_blocks).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SctParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "epoch_duration",
            "epochDuration",
            "sct_anchor_retention_blocks",
            "sctAnchorRetentionBlocks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EpochDuration,
            SctAnchorRetentionBlocks,
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
                            "epochDuration" | "epoch_duration" => Ok(GeneratedField::EpochDuration),
                            "sctAnchorRetentionBlocks" | "sct_anchor_retention_blocks" => Ok(GeneratedField::SctAnchorRetentionBlocks),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SctParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.SctParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SctParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut epoch_duration__ = None;
                let mut sct_anchor_retention_blocks__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EpochDuration => {
                            if epoch_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epochDuration"));
                            }
                            epoch_duration__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SctAnchorRetentionBlocks => {
                            if sct_anchor_retention_blocks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sctAnchorRetentionBlocks"));
                            }
                            sct_anchor_retention_blocks__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SctParameters {
                    epoch_duration: epoch_duration__.unwrap_or_default(),
                    sct_anchor_retention_blocks: sct_anchor_retention_blocks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.SctParameters", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimestampByHeightRequest {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.TimestampByHeightRequest", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimestampByHeightRequest {
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
            type Value = TimestampByHeightRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.TimestampByHeightRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimestampByHeightRequest, V::Error>
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
                Ok(TimestampByHeightRequest {
                    height: height__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.TimestampByHeightRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimestampByHeightResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timestamp.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.sct.v1.TimestampByHeightResponse", len)?;
        if let Some(v) = self.timestamp.as_ref() {
            struct_ser.serialize_field("timestamp", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimestampByHeightResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Timestamp,
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
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimestampByHeightResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.sct.v1.TimestampByHeightResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimestampByHeightResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timestamp__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TimestampByHeightResponse {
                    timestamp: timestamp__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.sct.v1.TimestampByHeightResponse", FIELDS, GeneratedVisitor)
    }
}
