impl serde::Serialize for AssetPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.daily_volume_limit.is_empty() {
            len += 1;
        }
        if !self.allowed_ibc_routes.is_empty() {
            len += 1;
        }
        if !self.ring_id.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.policy_id.is_empty() {
            len += 1;
        }
        if !self.permission.is_empty() {
            len += 1;
        }
        if !self.resource.is_empty() {
            len += 1;
        }
        if self.registration_authority_vk.is_some() {
            len += 1;
        }
        if self.ibc_origin.is_some() {
            len += 1;
        }
        if self.seizure_authority_vk.is_some() {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.AssetPolicy", len)?;
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
        if !self.allowed_ibc_routes.is_empty() {
            struct_ser.serialize_field("allowedIbcRoutes", &self.allowed_ibc_routes)?;
        }
        if !self.ring_id.is_empty() {
            struct_ser.serialize_field("ringId", &self.ring_id)?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.policy_id.is_empty() {
            struct_ser.serialize_field("policyId", &self.policy_id)?;
        }
        if !self.permission.is_empty() {
            struct_ser.serialize_field("permission", &self.permission)?;
        }
        if !self.resource.is_empty() {
            struct_ser.serialize_field("resource", &self.resource)?;
        }
        if let Some(v) = self.registration_authority_vk.as_ref() {
            struct_ser.serialize_field("registrationAuthorityVk", v)?;
        }
        if let Some(v) = self.ibc_origin.as_ref() {
            struct_ser.serialize_field("ibcOrigin", v)?;
        }
        if let Some(v) = self.seizure_authority_vk.as_ref() {
            struct_ser.serialize_field("seizureAuthorityVk", v)?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "dk_pub",
            "dkPub",
            "daily_volume_limit",
            "dailyVolumeLimit",
            "allowed_ibc_routes",
            "allowedIbcRoutes",
            "ring_id",
            "ringId",
            "ring_pk",
            "ringPk",
            "policy_id",
            "policyId",
            "permission",
            "resource",
            "registration_authority_vk",
            "registrationAuthorityVk",
            "ibc_origin",
            "ibcOrigin",
            "seizure_authority_vk",
            "seizureAuthorityVk",
            "audit_keys",
            "auditKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DkPub,
            DailyVolumeLimit,
            AllowedIbcRoutes,
            RingId,
            RingPk,
            PolicyId,
            Permission,
            Resource,
            RegistrationAuthorityVk,
            IbcOrigin,
            SeizureAuthorityVk,
            AuditKeys,
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
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "dailyVolumeLimit" | "daily_volume_limit" => Ok(GeneratedField::DailyVolumeLimit),
                            "allowedIbcRoutes" | "allowed_ibc_routes" => Ok(GeneratedField::AllowedIbcRoutes),
                            "ringId" | "ring_id" => Ok(GeneratedField::RingId),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "policyId" | "policy_id" => Ok(GeneratedField::PolicyId),
                            "permission" => Ok(GeneratedField::Permission),
                            "resource" => Ok(GeneratedField::Resource),
                            "registrationAuthorityVk" | "registration_authority_vk" => Ok(GeneratedField::RegistrationAuthorityVk),
                            "ibcOrigin" | "ibc_origin" => Ok(GeneratedField::IbcOrigin),
                            "seizureAuthorityVk" | "seizure_authority_vk" => Ok(GeneratedField::SeizureAuthorityVk),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.AssetPolicy")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetPolicy, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut dk_pub__ = None;
                let mut daily_volume_limit__ = None;
                let mut allowed_ibc_routes__ = None;
                let mut ring_id__ = None;
                let mut ring_pk__ = None;
                let mut policy_id__ = None;
                let mut permission__ = None;
                let mut resource__ = None;
                let mut registration_authority_vk__ = None;
                let mut ibc_origin__ = None;
                let mut seizure_authority_vk__ = None;
                let mut audit_keys__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
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
                        GeneratedField::AllowedIbcRoutes => {
                            if allowed_ibc_routes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allowedIbcRoutes"));
                            }
                            allowed_ibc_routes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RingId => {
                            if ring_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringId"));
                            }
                            ring_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PolicyId => {
                            if policy_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyId"));
                            }
                            policy_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Resource => {
                            if resource__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resource"));
                            }
                            resource__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RegistrationAuthorityVk => {
                            if registration_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registrationAuthorityVk"));
                            }
                            registration_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::IbcOrigin => {
                            if ibc_origin__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcOrigin"));
                            }
                            ibc_origin__ = map_.next_value()?;
                        }
                        GeneratedField::SeizureAuthorityVk => {
                            if seizure_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("seizureAuthorityVk"));
                            }
                            seizure_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetPolicy {
                    dk_pub: dk_pub__.unwrap_or_default(),
                    daily_volume_limit: daily_volume_limit__.unwrap_or_default(),
                    allowed_ibc_routes: allowed_ibc_routes__.unwrap_or_default(),
                    ring_id: ring_id__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    policy_id: policy_id__.unwrap_or_default(),
                    permission: permission__.unwrap_or_default(),
                    resource: resource__.unwrap_or_default(),
                    registration_authority_vk: registration_authority_vk__,
                    ibc_origin: ibc_origin__,
                    seizure_authority_vk: seizure_authority_vk__,
                    audit_keys: audit_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.AssetPolicy", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetRegistrationGrant {
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
        if self.registrar_vk.is_some() {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.AssetRegistrationGrant", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if let Some(v) = self.registrar_vk.as_ref() {
            struct_ser.serialize_field("registrarVk", v)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetRegistrationGrant {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "registrar_vk",
            "registrarVk",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            RegistrarVk,
            Signature,
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
                            "registrarVk" | "registrar_vk" => Ok(GeneratedField::RegistrarVk),
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetRegistrationGrant;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.AssetRegistrationGrant")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetRegistrationGrant, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut registrar_vk__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::RegistrarVk => {
                            if registrar_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registrarVk"));
                            }
                            registrar_vk__ = map_.next_value()?;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetRegistrationGrant {
                    body: body__,
                    registrar_vk: registrar_vk__,
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.AssetRegistrationGrant", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssetRegistrationGrantBody {
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
        if self.is_regulated {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.daily_volume_limit.is_empty() {
            len += 1;
        }
        if !self.allowed_ibc_routes.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.ring_id.is_empty() {
            len += 1;
        }
        if !self.policy_id.is_empty() {
            len += 1;
        }
        if !self.permission.is_empty() {
            len += 1;
        }
        if !self.resource.is_empty() {
            len += 1;
        }
        if self.registration_authority_vk.is_some() {
            len += 1;
        }
        if self.valid_until_unix != 0 {
            len += 1;
        }
        if self.ibc_origin.is_some() {
            len += 1;
        }
        if self.seizure_authority_vk.is_some() {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.AssetRegistrationGrantBody", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
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
        if !self.allowed_ibc_routes.is_empty() {
            struct_ser.serialize_field("allowedIbcRoutes", &self.allowed_ibc_routes)?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.ring_id.is_empty() {
            struct_ser.serialize_field("ringId", &self.ring_id)?;
        }
        if !self.policy_id.is_empty() {
            struct_ser.serialize_field("policyId", &self.policy_id)?;
        }
        if !self.permission.is_empty() {
            struct_ser.serialize_field("permission", &self.permission)?;
        }
        if !self.resource.is_empty() {
            struct_ser.serialize_field("resource", &self.resource)?;
        }
        if let Some(v) = self.registration_authority_vk.as_ref() {
            struct_ser.serialize_field("registrationAuthorityVk", v)?;
        }
        if self.valid_until_unix != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("validUntilUnix", ToString::to_string(&self.valid_until_unix).as_str())?;
        }
        if let Some(v) = self.ibc_origin.as_ref() {
            struct_ser.serialize_field("ibcOrigin", v)?;
        }
        if let Some(v) = self.seizure_authority_vk.as_ref() {
            struct_ser.serialize_field("seizureAuthorityVk", v)?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssetRegistrationGrantBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "is_regulated",
            "isRegulated",
            "dk_pub",
            "dkPub",
            "daily_volume_limit",
            "dailyVolumeLimit",
            "allowed_ibc_routes",
            "allowedIbcRoutes",
            "ring_pk",
            "ringPk",
            "ring_id",
            "ringId",
            "policy_id",
            "policyId",
            "permission",
            "resource",
            "registration_authority_vk",
            "registrationAuthorityVk",
            "valid_until_unix",
            "validUntilUnix",
            "ibc_origin",
            "ibcOrigin",
            "seizure_authority_vk",
            "seizureAuthorityVk",
            "audit_keys",
            "auditKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            IsRegulated,
            DkPub,
            DailyVolumeLimit,
            AllowedIbcRoutes,
            RingPk,
            RingId,
            PolicyId,
            Permission,
            Resource,
            RegistrationAuthorityVk,
            ValidUntilUnix,
            IbcOrigin,
            SeizureAuthorityVk,
            AuditKeys,
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
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "dailyVolumeLimit" | "daily_volume_limit" => Ok(GeneratedField::DailyVolumeLimit),
                            "allowedIbcRoutes" | "allowed_ibc_routes" => Ok(GeneratedField::AllowedIbcRoutes),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "ringId" | "ring_id" => Ok(GeneratedField::RingId),
                            "policyId" | "policy_id" => Ok(GeneratedField::PolicyId),
                            "permission" => Ok(GeneratedField::Permission),
                            "resource" => Ok(GeneratedField::Resource),
                            "registrationAuthorityVk" | "registration_authority_vk" => Ok(GeneratedField::RegistrationAuthorityVk),
                            "validUntilUnix" | "valid_until_unix" => Ok(GeneratedField::ValidUntilUnix),
                            "ibcOrigin" | "ibc_origin" => Ok(GeneratedField::IbcOrigin),
                            "seizureAuthorityVk" | "seizure_authority_vk" => Ok(GeneratedField::SeizureAuthorityVk),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssetRegistrationGrantBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.AssetRegistrationGrantBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssetRegistrationGrantBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut is_regulated__ = None;
                let mut dk_pub__ = None;
                let mut daily_volume_limit__ = None;
                let mut allowed_ibc_routes__ = None;
                let mut ring_pk__ = None;
                let mut ring_id__ = None;
                let mut policy_id__ = None;
                let mut permission__ = None;
                let mut resource__ = None;
                let mut registration_authority_vk__ = None;
                let mut valid_until_unix__ = None;
                let mut ibc_origin__ = None;
                let mut seizure_authority_vk__ = None;
                let mut audit_keys__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
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
                        GeneratedField::AllowedIbcRoutes => {
                            if allowed_ibc_routes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allowedIbcRoutes"));
                            }
                            allowed_ibc_routes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RingId => {
                            if ring_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringId"));
                            }
                            ring_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::PolicyId => {
                            if policy_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyId"));
                            }
                            policy_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Resource => {
                            if resource__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resource"));
                            }
                            resource__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RegistrationAuthorityVk => {
                            if registration_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registrationAuthorityVk"));
                            }
                            registration_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::ValidUntilUnix => {
                            if valid_until_unix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("validUntilUnix"));
                            }
                            valid_until_unix__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IbcOrigin => {
                            if ibc_origin__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcOrigin"));
                            }
                            ibc_origin__ = map_.next_value()?;
                        }
                        GeneratedField::SeizureAuthorityVk => {
                            if seizure_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("seizureAuthorityVk"));
                            }
                            seizure_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AssetRegistrationGrantBody {
                    asset_id: asset_id__,
                    is_regulated: is_regulated__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    daily_volume_limit: daily_volume_limit__.unwrap_or_default(),
                    allowed_ibc_routes: allowed_ibc_routes__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    ring_id: ring_id__.unwrap_or_default(),
                    policy_id: policy_id__.unwrap_or_default(),
                    permission: permission__.unwrap_or_default(),
                    resource: resource__.unwrap_or_default(),
                    registration_authority_vk: registration_authority_vk__,
                    valid_until_unix: valid_until_unix__.unwrap_or_default(),
                    ibc_origin: ibc_origin__,
                    seizure_authority_vk: seizure_authority_vk__,
                    audit_keys: audit_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.AssetRegistrationGrantBody", FIELDS, GeneratedVisitor)
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
        let struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceAnchorsRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceAnchorsRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceAnchorsRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceAnchorsResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceAnchorsResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceAnchorsResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceAssetStatusRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceAssetStatusRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceAssetStatusRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceAssetStatusResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceAssetStatusResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceAssetStatusResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchMerkleProofsResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchQuery", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceBatchQuery")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceBatchQuery", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceLeaf {
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
        if !self.capk.is_empty() {
            len += 1;
        }
        if !self.rnk_dh_pk.is_empty() {
            len += 1;
        }
        if !self.rnk_commitment.is_empty() {
            len += 1;
        }
        if self.status != 0 {
            len += 1;
        }
        if self.freeze_generation != 0 {
            len += 1;
        }
        if self.frozen_since_height != 0 {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceLeaf", len)?;
        if let Some(v) = self.address.as_ref() {
            struct_ser.serialize_field("address", v)?;
        }
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if !self.capk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("capk", pbjson::private::base64::encode(&self.capk).as_str())?;
        }
        if !self.rnk_dh_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rnkDhPk", pbjson::private::base64::encode(&self.rnk_dh_pk).as_str())?;
        }
        if !self.rnk_commitment.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rnkCommitment", pbjson::private::base64::encode(&self.rnk_commitment).as_str())?;
        }
        if self.status != 0 {
            let v = UserAssetStatus::try_from(self.status)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        if self.freeze_generation != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("freezeGeneration", ToString::to_string(&self.freeze_generation).as_str())?;
        }
        if self.frozen_since_height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("frozenSinceHeight", ToString::to_string(&self.frozen_since_height).as_str())?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceLeaf {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "asset_id",
            "assetId",
            "capk",
            "rnk_dh_pk",
            "rnkDhPk",
            "rnk_commitment",
            "rnkCommitment",
            "status",
            "freeze_generation",
            "freezeGeneration",
            "frozen_since_height",
            "frozenSinceHeight",
            "audit_keys",
            "auditKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            AssetId,
            Capk,
            RnkDhPk,
            RnkCommitment,
            Status,
            FreezeGeneration,
            FrozenSinceHeight,
            AuditKeys,
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
                            "capk" => Ok(GeneratedField::Capk),
                            "rnkDhPk" | "rnk_dh_pk" => Ok(GeneratedField::RnkDhPk),
                            "rnkCommitment" | "rnk_commitment" => Ok(GeneratedField::RnkCommitment),
                            "status" => Ok(GeneratedField::Status),
                            "freezeGeneration" | "freeze_generation" => Ok(GeneratedField::FreezeGeneration),
                            "frozenSinceHeight" | "frozen_since_height" => Ok(GeneratedField::FrozenSinceHeight),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceLeaf;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceLeaf")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceLeaf, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut asset_id__ = None;
                let mut capk__ = None;
                let mut rnk_dh_pk__ = None;
                let mut rnk_commitment__ = None;
                let mut status__ = None;
                let mut freeze_generation__ = None;
                let mut frozen_since_height__ = None;
                let mut audit_keys__ = None;
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
                        GeneratedField::Capk => {
                            if capk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("capk"));
                            }
                            capk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RnkDhPk => {
                            if rnk_dh_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rnkDhPk"));
                            }
                            rnk_dh_pk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RnkCommitment => {
                            if rnk_commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rnkCommitment"));
                            }
                            rnk_commitment__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map_.next_value::<UserAssetStatus>()? as i32);
                        }
                        GeneratedField::FreezeGeneration => {
                            if freeze_generation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("freezeGeneration"));
                            }
                            freeze_generation__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FrozenSinceHeight => {
                            if frozen_since_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("frozenSinceHeight"));
                            }
                            frozen_since_height__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceLeaf {
                    address: address__,
                    asset_id: asset_id__,
                    capk: capk__.unwrap_or_default(),
                    rnk_dh_pk: rnk_dh_pk__.unwrap_or_default(),
                    rnk_commitment: rnk_commitment__.unwrap_or_default(),
                    status: status__.unwrap_or_default(),
                    freeze_generation: freeze_generation__.unwrap_or_default(),
                    frozen_since_height: frozen_since_height__.unwrap_or_default(),
                    audit_keys: audit_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceLeaf", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceMerkleProofsRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceMerkleProofsRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceMerkleProofsRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceMerkleProofsResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceMerkleProofsResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceMerkleProofsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceParameters {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.anchor_validation_window_blocks != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceParameters", len)?;
        if self.anchor_validation_window_blocks != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("anchorValidationWindowBlocks", ToString::to_string(&self.anchor_validation_window_blocks).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceParameters {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "anchor_validation_window_blocks",
            "anchorValidationWindowBlocks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AnchorValidationWindowBlocks,
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
                            "anchorValidationWindowBlocks" | "anchor_validation_window_blocks" => Ok(GeneratedField::AnchorValidationWindowBlocks),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ComplianceParameters;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceParameters")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceParameters, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut anchor_validation_window_blocks__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AnchorValidationWindowBlocks => {
                            if anchor_validation_window_blocks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anchorValidationWindowBlocks"));
                            }
                            anchor_validation_window_blocks__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ComplianceParameters {
                    anchor_validation_window_blocks: anchor_validation_window_blocks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceParameters", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceUserLeafRequest", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceUserLeafRequest")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceUserLeafRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceUserLeafResponse", len)?;
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceUserLeafResponse")
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
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceUserLeafResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ComplianceViewingKey {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.ComplianceViewingKey", len)?;
        if !self.inner.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("inner", pbjson::private::base64::encode(&self.inner).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ComplianceViewingKey {
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
            type Value = ComplianceViewingKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.ComplianceViewingKey")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ComplianceViewingKey, V::Error>
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
                Ok(ComplianceViewingKey {
                    inner: inner__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.ComplianceViewingKey", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DleqProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.commitment_g.is_empty() {
            len += 1;
        }
        if !self.commitment_h.is_empty() {
            len += 1;
        }
        if !self.response.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.DleqProof", len)?;
        if !self.commitment_g.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("commitmentG", pbjson::private::base64::encode(&self.commitment_g).as_str())?;
        }
        if !self.commitment_h.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("commitmentH", pbjson::private::base64::encode(&self.commitment_h).as_str())?;
        }
        if !self.response.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("response", pbjson::private::base64::encode(&self.response).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DleqProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "commitment_g",
            "commitmentG",
            "commitment_h",
            "commitmentH",
            "response",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CommitmentG,
            CommitmentH,
            Response,
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
                            "commitmentG" | "commitment_g" => Ok(GeneratedField::CommitmentG),
                            "commitmentH" | "commitment_h" => Ok(GeneratedField::CommitmentH),
                            "response" => Ok(GeneratedField::Response),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DleqProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.DleqProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DleqProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut commitment_g__ = None;
                let mut commitment_h__ = None;
                let mut response__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CommitmentG => {
                            if commitment_g__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitmentG"));
                            }
                            commitment_g__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CommitmentH => {
                            if commitment_h__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitmentH"));
                            }
                            commitment_h__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Response => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("response"));
                            }
                            response__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(DleqProof {
                    commitment_g: commitment_g__.unwrap_or_default(),
                    commitment_h: commitment_h__.unwrap_or_default(),
                    response: response__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.DleqProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventAssetRegistered {
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
        if self.is_regulated {
            len += 1;
        }
        if self.position != 0 {
            len += 1;
        }
        if self.indexed_leaf.is_some() {
            len += 1;
        }
        if self.low_leaf_position != 0 {
            len += 1;
        }
        if self.updated_low_leaf.is_some() {
            len += 1;
        }
        if self.asset_policy.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.EventAssetRegistered", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if let Some(v) = self.indexed_leaf.as_ref() {
            struct_ser.serialize_field("indexedLeaf", v)?;
        }
        if self.low_leaf_position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("lowLeafPosition", ToString::to_string(&self.low_leaf_position).as_str())?;
        }
        if let Some(v) = self.updated_low_leaf.as_ref() {
            struct_ser.serialize_field("updatedLowLeaf", v)?;
        }
        if let Some(v) = self.asset_policy.as_ref() {
            struct_ser.serialize_field("assetPolicy", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventAssetRegistered {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "is_regulated",
            "isRegulated",
            "position",
            "indexed_leaf",
            "indexedLeaf",
            "low_leaf_position",
            "lowLeafPosition",
            "updated_low_leaf",
            "updatedLowLeaf",
            "asset_policy",
            "assetPolicy",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            IsRegulated,
            Position,
            IndexedLeaf,
            LowLeafPosition,
            UpdatedLowLeaf,
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
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "position" => Ok(GeneratedField::Position),
                            "indexedLeaf" | "indexed_leaf" => Ok(GeneratedField::IndexedLeaf),
                            "lowLeafPosition" | "low_leaf_position" => Ok(GeneratedField::LowLeafPosition),
                            "updatedLowLeaf" | "updated_low_leaf" => Ok(GeneratedField::UpdatedLowLeaf),
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
            type Value = EventAssetRegistered;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.EventAssetRegistered")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventAssetRegistered, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut is_regulated__ = None;
                let mut position__ = None;
                let mut indexed_leaf__ = None;
                let mut low_leaf_position__ = None;
                let mut updated_low_leaf__ = None;
                let mut asset_policy__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
                        }
                        GeneratedField::IsRegulated => {
                            if is_regulated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isRegulated"));
                            }
                            is_regulated__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IndexedLeaf => {
                            if indexed_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("indexedLeaf"));
                            }
                            indexed_leaf__ = map_.next_value()?;
                        }
                        GeneratedField::LowLeafPosition => {
                            if low_leaf_position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lowLeafPosition"));
                            }
                            low_leaf_position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UpdatedLowLeaf => {
                            if updated_low_leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updatedLowLeaf"));
                            }
                            updated_low_leaf__ = map_.next_value()?;
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
                Ok(EventAssetRegistered {
                    asset_id: asset_id__,
                    is_regulated: is_regulated__.unwrap_or_default(),
                    position: position__.unwrap_or_default(),
                    indexed_leaf: indexed_leaf__,
                    low_leaf_position: low_leaf_position__.unwrap_or_default(),
                    updated_low_leaf: updated_low_leaf__,
                    asset_policy: asset_policy__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.EventAssetRegistered", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventComplianceAnchor {
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
        if !self.user_anchor.is_empty() {
            len += 1;
        }
        if !self.asset_anchor.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.EventComplianceAnchor", len)?;
        if self.height != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("height", ToString::to_string(&self.height).as_str())?;
        }
        if !self.user_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("userAnchor", pbjson::private::base64::encode(&self.user_anchor).as_str())?;
        }
        if !self.asset_anchor.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("assetAnchor", pbjson::private::base64::encode(&self.asset_anchor).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventComplianceAnchor {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "height",
            "user_anchor",
            "userAnchor",
            "asset_anchor",
            "assetAnchor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Height,
            UserAnchor,
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
                            "height" => Ok(GeneratedField::Height),
                            "userAnchor" | "user_anchor" => Ok(GeneratedField::UserAnchor),
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
            type Value = EventComplianceAnchor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.EventComplianceAnchor")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventComplianceAnchor, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut height__ = None;
                let mut user_anchor__ = None;
                let mut asset_anchor__ = None;
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
                        GeneratedField::UserAnchor => {
                            if user_anchor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userAnchor"));
                            }
                            user_anchor__ =
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventComplianceAnchor {
                    height: height__.unwrap_or_default(),
                    user_anchor: user_anchor__.unwrap_or_default(),
                    asset_anchor: asset_anchor__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.EventComplianceAnchor", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventUserAssetStatusChanged {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.position != 0 {
            len += 1;
        }
        if !self.commitment.is_empty() {
            len += 1;
        }
        if self.leaf.is_some() {
            len += 1;
        }
        if self.previous_status != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.EventUserAssetStatusChanged", len)?;
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if !self.commitment.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("commitment", pbjson::private::base64::encode(&self.commitment).as_str())?;
        }
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if self.previous_status != 0 {
            let v = UserAssetStatus::try_from(self.previous_status)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.previous_status)))?;
            struct_ser.serialize_field("previousStatus", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventUserAssetStatusChanged {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "position",
            "commitment",
            "leaf",
            "previous_status",
            "previousStatus",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Position,
            Commitment,
            Leaf,
            PreviousStatus,
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
                            "position" => Ok(GeneratedField::Position),
                            "commitment" => Ok(GeneratedField::Commitment),
                            "leaf" => Ok(GeneratedField::Leaf),
                            "previousStatus" | "previous_status" => Ok(GeneratedField::PreviousStatus),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventUserAssetStatusChanged;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.EventUserAssetStatusChanged")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventUserAssetStatusChanged, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut position__ = None;
                let mut commitment__ = None;
                let mut leaf__ = None;
                let mut previous_status__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Commitment => {
                            if commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitment"));
                            }
                            commitment__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::PreviousStatus => {
                            if previous_status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("previousStatus"));
                            }
                            previous_status__ = Some(map_.next_value::<UserAssetStatus>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventUserAssetStatusChanged {
                    position: position__.unwrap_or_default(),
                    commitment: commitment__.unwrap_or_default(),
                    leaf: leaf__,
                    previous_status: previous_status__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.EventUserAssetStatusChanged", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EventUserRegistered {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.position != 0 {
            len += 1;
        }
        if !self.commitment.is_empty() {
            len += 1;
        }
        if self.leaf.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.EventUserRegistered", len)?;
        if self.position != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("position", ToString::to_string(&self.position).as_str())?;
        }
        if !self.commitment.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("commitment", pbjson::private::base64::encode(&self.commitment).as_str())?;
        }
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventUserRegistered {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "position",
            "commitment",
            "leaf",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Position,
            Commitment,
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
                            "position" => Ok(GeneratedField::Position),
                            "commitment" => Ok(GeneratedField::Commitment),
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
            type Value = EventUserRegistered;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.EventUserRegistered")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventUserRegistered, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut position__ = None;
                let mut commitment__ = None;
                let mut leaf__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Position => {
                            if position__.is_some() {
                                return Err(serde::de::Error::duplicate_field("position"));
                            }
                            position__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Commitment => {
                            if commitment__.is_some() {
                                return Err(serde::de::Error::duplicate_field("commitment"));
                            }
                            commitment__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                Ok(EventUserRegistered {
                    position: position__.unwrap_or_default(),
                    commitment: commitment__.unwrap_or_default(),
                    leaf: leaf__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.EventUserRegistered", FIELDS, GeneratedVisitor)
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
        if !self.native_assets.is_empty() {
            len += 1;
        }
        if !self.compliance_registrar_vk.is_empty() {
            len += 1;
        }
        if self.compliance_params.is_some() {
            len += 1;
        }
        if !self.user_registrations.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.GenesisContent", len)?;
        if !self.native_assets.is_empty() {
            struct_ser.serialize_field("nativeAssets", &self.native_assets)?;
        }
        if !self.compliance_registrar_vk.is_empty() {
            struct_ser.serialize_field("complianceRegistrarVk", &self.compliance_registrar_vk)?;
        }
        if let Some(v) = self.compliance_params.as_ref() {
            struct_ser.serialize_field("complianceParams", v)?;
        }
        if !self.user_registrations.is_empty() {
            struct_ser.serialize_field("userRegistrations", &self.user_registrations)?;
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
            "native_assets",
            "nativeAssets",
            "compliance_registrar_vk",
            "complianceRegistrarVk",
            "compliance_params",
            "complianceParams",
            "user_registrations",
            "userRegistrations",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NativeAssets,
            ComplianceRegistrarVk,
            ComplianceParams,
            UserRegistrations,
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
                            "nativeAssets" | "native_assets" => Ok(GeneratedField::NativeAssets),
                            "complianceRegistrarVk" | "compliance_registrar_vk" => Ok(GeneratedField::ComplianceRegistrarVk),
                            "complianceParams" | "compliance_params" => Ok(GeneratedField::ComplianceParams),
                            "userRegistrations" | "user_registrations" => Ok(GeneratedField::UserRegistrations),
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
                formatter.write_str("struct shieldd.core.component.compliance.v1.GenesisContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut native_assets__ = None;
                let mut compliance_registrar_vk__ = None;
                let mut compliance_params__ = None;
                let mut user_registrations__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NativeAssets => {
                            if native_assets__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nativeAssets"));
                            }
                            native_assets__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceRegistrarVk => {
                            if compliance_registrar_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceRegistrarVk"));
                            }
                            compliance_registrar_vk__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ComplianceParams => {
                            if compliance_params__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complianceParams"));
                            }
                            compliance_params__ = map_.next_value()?;
                        }
                        GeneratedField::UserRegistrations => {
                            if user_registrations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userRegistrations"));
                            }
                            user_registrations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisContent {
                    native_assets: native_assets__.unwrap_or_default(),
                    compliance_registrar_vk: compliance_registrar_vk__.unwrap_or_default(),
                    compliance_params: compliance_params__,
                    user_registrations: user_registrations__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.GenesisContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GenesisUserRegistration {
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
        if self.capability_certificate.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.GenesisUserRegistration", len)?;
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if let Some(v) = self.capability_certificate.as_ref() {
            struct_ser.serialize_field("capabilityCertificate", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GenesisUserRegistration {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "leaf",
            "capability_certificate",
            "capabilityCertificate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Leaf,
            CapabilityCertificate,
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
                            "capabilityCertificate" | "capability_certificate" => Ok(GeneratedField::CapabilityCertificate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GenesisUserRegistration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.GenesisUserRegistration")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GenesisUserRegistration, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut leaf__ = None;
                let mut capability_certificate__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::CapabilityCertificate => {
                            if capability_certificate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("capabilityCertificate"));
                            }
                            capability_certificate__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GenesisUserRegistration {
                    leaf: leaf__,
                    capability_certificate: capability_certificate__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.GenesisUserRegistration", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IbcAssetOrigin {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.route.is_some() {
            len += 1;
        }
        if !self.base_denom.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.IbcAssetOrigin", len)?;
        if let Some(v) = self.route.as_ref() {
            struct_ser.serialize_field("route", v)?;
        }
        if !self.base_denom.is_empty() {
            struct_ser.serialize_field("baseDenom", &self.base_denom)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IbcAssetOrigin {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "route",
            "base_denom",
            "baseDenom",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Route,
            BaseDenom,
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
                            "route" => Ok(GeneratedField::Route),
                            "baseDenom" | "base_denom" => Ok(GeneratedField::BaseDenom),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IbcAssetOrigin;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.IbcAssetOrigin")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IbcAssetOrigin, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut route__ = None;
                let mut base_denom__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Route => {
                            if route__.is_some() {
                                return Err(serde::de::Error::duplicate_field("route"));
                            }
                            route__ = map_.next_value()?;
                        }
                        GeneratedField::BaseDenom => {
                            if base_denom__.is_some() {
                                return Err(serde::de::Error::duplicate_field("baseDenom"));
                            }
                            base_denom__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IbcAssetOrigin {
                    route: route__,
                    base_denom: base_denom__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.IbcAssetOrigin", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IbcRoute {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.local_port.is_empty() {
            len += 1;
        }
        if !self.local_channel.is_empty() {
            len += 1;
        }
        if !self.connection_id.is_empty() {
            len += 1;
        }
        if !self.counterparty_port.is_empty() {
            len += 1;
        }
        if !self.counterparty_channel.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.IbcRoute", len)?;
        if !self.local_port.is_empty() {
            struct_ser.serialize_field("localPort", &self.local_port)?;
        }
        if !self.local_channel.is_empty() {
            struct_ser.serialize_field("localChannel", &self.local_channel)?;
        }
        if !self.connection_id.is_empty() {
            struct_ser.serialize_field("connectionId", &self.connection_id)?;
        }
        if !self.counterparty_port.is_empty() {
            struct_ser.serialize_field("counterpartyPort", &self.counterparty_port)?;
        }
        if !self.counterparty_channel.is_empty() {
            struct_ser.serialize_field("counterpartyChannel", &self.counterparty_channel)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IbcRoute {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "local_port",
            "localPort",
            "local_channel",
            "localChannel",
            "connection_id",
            "connectionId",
            "counterparty_port",
            "counterpartyPort",
            "counterparty_channel",
            "counterpartyChannel",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LocalPort,
            LocalChannel,
            ConnectionId,
            CounterpartyPort,
            CounterpartyChannel,
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
                            "localPort" | "local_port" => Ok(GeneratedField::LocalPort),
                            "localChannel" | "local_channel" => Ok(GeneratedField::LocalChannel),
                            "connectionId" | "connection_id" => Ok(GeneratedField::ConnectionId),
                            "counterpartyPort" | "counterparty_port" => Ok(GeneratedField::CounterpartyPort),
                            "counterpartyChannel" | "counterparty_channel" => Ok(GeneratedField::CounterpartyChannel),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IbcRoute;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.IbcRoute")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IbcRoute, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut local_port__ = None;
                let mut local_channel__ = None;
                let mut connection_id__ = None;
                let mut counterparty_port__ = None;
                let mut counterparty_channel__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LocalPort => {
                            if local_port__.is_some() {
                                return Err(serde::de::Error::duplicate_field("localPort"));
                            }
                            local_port__ = Some(map_.next_value()?);
                        }
                        GeneratedField::LocalChannel => {
                            if local_channel__.is_some() {
                                return Err(serde::de::Error::duplicate_field("localChannel"));
                            }
                            local_channel__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ConnectionId => {
                            if connection_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("connectionId"));
                            }
                            connection_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CounterpartyPort => {
                            if counterparty_port__.is_some() {
                                return Err(serde::de::Error::duplicate_field("counterpartyPort"));
                            }
                            counterparty_port__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CounterpartyChannel => {
                            if counterparty_channel__.is_some() {
                                return Err(serde::de::Error::duplicate_field("counterpartyChannel"));
                            }
                            counterparty_channel__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IbcRoute {
                    local_port: local_port__.unwrap_or_default(),
                    local_channel: local_channel__.unwrap_or_default(),
                    connection_id: connection_id__.unwrap_or_default(),
                    counterparty_port: counterparty_port__.unwrap_or_default(),
                    counterparty_channel: counterparty_channel__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.IbcRoute", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IndexedLeafData {
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
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.daily_volume_limit.is_empty() {
            len += 1;
        }
        if !self.route_policy_hash.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.ring_id_hash.is_empty() {
            len += 1;
        }
        if !self.policy_id_hash.is_empty() {
            len += 1;
        }
        if !self.permission_hash.is_empty() {
            len += 1;
        }
        if !self.resource_hash.is_empty() {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.IndexedLeafData", len)?;
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
        if !self.route_policy_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("routePolicyHash", pbjson::private::base64::encode(&self.route_policy_hash).as_str())?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.ring_id_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringIdHash", pbjson::private::base64::encode(&self.ring_id_hash).as_str())?;
        }
        if !self.policy_id_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("policyIdHash", pbjson::private::base64::encode(&self.policy_id_hash).as_str())?;
        }
        if !self.permission_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("permissionHash", pbjson::private::base64::encode(&self.permission_hash).as_str())?;
        }
        if !self.resource_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("resourceHash", pbjson::private::base64::encode(&self.resource_hash).as_str())?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IndexedLeafData {
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
            "dk_pub",
            "dkPub",
            "daily_volume_limit",
            "dailyVolumeLimit",
            "route_policy_hash",
            "routePolicyHash",
            "ring_pk",
            "ringPk",
            "ring_id_hash",
            "ringIdHash",
            "policy_id_hash",
            "policyIdHash",
            "permission_hash",
            "permissionHash",
            "resource_hash",
            "resourceHash",
            "audit_keys",
            "auditKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Value,
            NextIndex,
            NextValue,
            DkPub,
            DailyVolumeLimit,
            RoutePolicyHash,
            RingPk,
            RingIdHash,
            PolicyIdHash,
            PermissionHash,
            ResourceHash,
            AuditKeys,
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
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "dailyVolumeLimit" | "daily_volume_limit" => Ok(GeneratedField::DailyVolumeLimit),
                            "routePolicyHash" | "route_policy_hash" => Ok(GeneratedField::RoutePolicyHash),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "ringIdHash" | "ring_id_hash" => Ok(GeneratedField::RingIdHash),
                            "policyIdHash" | "policy_id_hash" => Ok(GeneratedField::PolicyIdHash),
                            "permissionHash" | "permission_hash" => Ok(GeneratedField::PermissionHash),
                            "resourceHash" | "resource_hash" => Ok(GeneratedField::ResourceHash),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IndexedLeafData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.IndexedLeafData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IndexedLeafData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut value__ = None;
                let mut next_index__ = None;
                let mut next_value__ = None;
                let mut dk_pub__ = None;
                let mut daily_volume_limit__ = None;
                let mut route_policy_hash__ = None;
                let mut ring_pk__ = None;
                let mut ring_id_hash__ = None;
                let mut policy_id_hash__ = None;
                let mut permission_hash__ = None;
                let mut resource_hash__ = None;
                let mut audit_keys__ = None;
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
                        GeneratedField::RoutePolicyHash => {
                            if route_policy_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("routePolicyHash"));
                            }
                            route_policy_hash__ =
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
                        GeneratedField::RingIdHash => {
                            if ring_id_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringIdHash"));
                            }
                            ring_id_hash__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PolicyIdHash => {
                            if policy_id_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyIdHash"));
                            }
                            policy_id_hash__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PermissionHash => {
                            if permission_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permissionHash"));
                            }
                            permission_hash__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ResourceHash => {
                            if resource_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resourceHash"));
                            }
                            resource_hash__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(IndexedLeafData {
                    value: value__.unwrap_or_default(),
                    next_index: next_index__.unwrap_or_default(),
                    next_value: next_value__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    daily_volume_limit: daily_volume_limit__.unwrap_or_default(),
                    route_policy_hash: route_policy_hash__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    ring_id_hash: ring_id_hash__.unwrap_or_default(),
                    policy_id_hash: policy_id_hash__.unwrap_or_default(),
                    permission_hash: permission_hash__.unwrap_or_default(),
                    resource_hash: resource_hash__.unwrap_or_default(),
                    audit_keys: audit_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.IndexedLeafData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MerklePath {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.layers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.MerklePath", len)?;
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MerklePath {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "layers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Layers,
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
                            "layers" => Ok(GeneratedField::Layers),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MerklePath;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.MerklePath")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MerklePath, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut layers__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(MerklePath {
                    layers: layers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.MerklePath", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MerklePathLayer {
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
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.MerklePathLayer", len)?;
        if !self.siblings.is_empty() {
            struct_ser.serialize_field("siblings", &self.siblings.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MerklePathLayer {
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
            type Value = MerklePathLayer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.MerklePathLayer")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MerklePathLayer, V::Error>
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
                Ok(MerklePathLayer {
                    siblings: siblings__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.MerklePathLayer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MsgRegisterAsset {
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
        if self.is_regulated {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if !self.daily_volume_limit.is_empty() {
            len += 1;
        }
        if !self.allowed_ibc_routes.is_empty() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.ring_id.is_empty() {
            len += 1;
        }
        if !self.policy_id.is_empty() {
            len += 1;
        }
        if !self.permission.is_empty() {
            len += 1;
        }
        if !self.resource.is_empty() {
            len += 1;
        }
        if self.registration_authority_vk.is_some() {
            len += 1;
        }
        if self.asset_registration_grant.is_some() {
            len += 1;
        }
        if self.ibc_origin.is_some() {
            len += 1;
        }
        if self.seizure_authority_vk.is_some() {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        if self.audit_certificate.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.MsgRegisterAsset", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
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
        if !self.allowed_ibc_routes.is_empty() {
            struct_ser.serialize_field("allowedIbcRoutes", &self.allowed_ibc_routes)?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.ring_id.is_empty() {
            struct_ser.serialize_field("ringId", &self.ring_id)?;
        }
        if !self.policy_id.is_empty() {
            struct_ser.serialize_field("policyId", &self.policy_id)?;
        }
        if !self.permission.is_empty() {
            struct_ser.serialize_field("permission", &self.permission)?;
        }
        if !self.resource.is_empty() {
            struct_ser.serialize_field("resource", &self.resource)?;
        }
        if let Some(v) = self.registration_authority_vk.as_ref() {
            struct_ser.serialize_field("registrationAuthorityVk", v)?;
        }
        if let Some(v) = self.asset_registration_grant.as_ref() {
            struct_ser.serialize_field("assetRegistrationGrant", v)?;
        }
        if let Some(v) = self.ibc_origin.as_ref() {
            struct_ser.serialize_field("ibcOrigin", v)?;
        }
        if let Some(v) = self.seizure_authority_vk.as_ref() {
            struct_ser.serialize_field("seizureAuthorityVk", v)?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        if let Some(v) = self.audit_certificate.as_ref() {
            struct_ser.serialize_field("auditCertificate", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MsgRegisterAsset {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "is_regulated",
            "isRegulated",
            "dk_pub",
            "dkPub",
            "daily_volume_limit",
            "dailyVolumeLimit",
            "allowed_ibc_routes",
            "allowedIbcRoutes",
            "ring_pk",
            "ringPk",
            "ring_id",
            "ringId",
            "policy_id",
            "policyId",
            "permission",
            "resource",
            "registration_authority_vk",
            "registrationAuthorityVk",
            "asset_registration_grant",
            "assetRegistrationGrant",
            "ibc_origin",
            "ibcOrigin",
            "seizure_authority_vk",
            "seizureAuthorityVk",
            "audit_keys",
            "auditKeys",
            "audit_certificate",
            "auditCertificate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            IsRegulated,
            DkPub,
            DailyVolumeLimit,
            AllowedIbcRoutes,
            RingPk,
            RingId,
            PolicyId,
            Permission,
            Resource,
            RegistrationAuthorityVk,
            AssetRegistrationGrant,
            IbcOrigin,
            SeizureAuthorityVk,
            AuditKeys,
            AuditCertificate,
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
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "dailyVolumeLimit" | "daily_volume_limit" => Ok(GeneratedField::DailyVolumeLimit),
                            "allowedIbcRoutes" | "allowed_ibc_routes" => Ok(GeneratedField::AllowedIbcRoutes),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "ringId" | "ring_id" => Ok(GeneratedField::RingId),
                            "policyId" | "policy_id" => Ok(GeneratedField::PolicyId),
                            "permission" => Ok(GeneratedField::Permission),
                            "resource" => Ok(GeneratedField::Resource),
                            "registrationAuthorityVk" | "registration_authority_vk" => Ok(GeneratedField::RegistrationAuthorityVk),
                            "assetRegistrationGrant" | "asset_registration_grant" => Ok(GeneratedField::AssetRegistrationGrant),
                            "ibcOrigin" | "ibc_origin" => Ok(GeneratedField::IbcOrigin),
                            "seizureAuthorityVk" | "seizure_authority_vk" => Ok(GeneratedField::SeizureAuthorityVk),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            "auditCertificate" | "audit_certificate" => Ok(GeneratedField::AuditCertificate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MsgRegisterAsset;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.MsgRegisterAsset")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MsgRegisterAsset, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut is_regulated__ = None;
                let mut dk_pub__ = None;
                let mut daily_volume_limit__ = None;
                let mut allowed_ibc_routes__ = None;
                let mut ring_pk__ = None;
                let mut ring_id__ = None;
                let mut policy_id__ = None;
                let mut permission__ = None;
                let mut resource__ = None;
                let mut registration_authority_vk__ = None;
                let mut asset_registration_grant__ = None;
                let mut ibc_origin__ = None;
                let mut seizure_authority_vk__ = None;
                let mut audit_keys__ = None;
                let mut audit_certificate__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
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
                        GeneratedField::AllowedIbcRoutes => {
                            if allowed_ibc_routes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allowedIbcRoutes"));
                            }
                            allowed_ibc_routes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RingId => {
                            if ring_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringId"));
                            }
                            ring_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::PolicyId => {
                            if policy_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyId"));
                            }
                            policy_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Resource => {
                            if resource__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resource"));
                            }
                            resource__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RegistrationAuthorityVk => {
                            if registration_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registrationAuthorityVk"));
                            }
                            registration_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::AssetRegistrationGrant => {
                            if asset_registration_grant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetRegistrationGrant"));
                            }
                            asset_registration_grant__ = map_.next_value()?;
                        }
                        GeneratedField::IbcOrigin => {
                            if ibc_origin__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ibcOrigin"));
                            }
                            ibc_origin__ = map_.next_value()?;
                        }
                        GeneratedField::SeizureAuthorityVk => {
                            if seizure_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("seizureAuthorityVk"));
                            }
                            seizure_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AuditCertificate => {
                            if audit_certificate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditCertificate"));
                            }
                            audit_certificate__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(MsgRegisterAsset {
                    asset_id: asset_id__,
                    is_regulated: is_regulated__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    daily_volume_limit: daily_volume_limit__.unwrap_or_default(),
                    allowed_ibc_routes: allowed_ibc_routes__.unwrap_or_default(),
                    ring_pk: ring_pk__.unwrap_or_default(),
                    ring_id: ring_id__.unwrap_or_default(),
                    policy_id: policy_id__.unwrap_or_default(),
                    permission: permission__.unwrap_or_default(),
                    resource: resource__.unwrap_or_default(),
                    registration_authority_vk: registration_authority_vk__,
                    asset_registration_grant: asset_registration_grant__,
                    ibc_origin: ibc_origin__,
                    seizure_authority_vk: seizure_authority_vk__,
                    audit_keys: audit_keys__.unwrap_or_default(),
                    audit_certificate: audit_certificate__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.MsgRegisterAsset", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MsgRegisterUser {
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
        if self.grant.is_some() {
            len += 1;
        }
        if self.capability_certificate.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.MsgRegisterUser", len)?;
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if let Some(v) = self.grant.as_ref() {
            struct_ser.serialize_field("grant", v)?;
        }
        if let Some(v) = self.capability_certificate.as_ref() {
            struct_ser.serialize_field("capabilityCertificate", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MsgRegisterUser {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "leaf",
            "grant",
            "capability_certificate",
            "capabilityCertificate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Leaf,
            Grant,
            CapabilityCertificate,
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
                            "grant" => Ok(GeneratedField::Grant),
                            "capabilityCertificate" | "capability_certificate" => Ok(GeneratedField::CapabilityCertificate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MsgRegisterUser;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.MsgRegisterUser")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MsgRegisterUser, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut leaf__ = None;
                let mut grant__ = None;
                let mut capability_certificate__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::Grant => {
                            if grant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("grant"));
                            }
                            grant__ = map_.next_value()?;
                        }
                        GeneratedField::CapabilityCertificate => {
                            if capability_certificate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("capabilityCertificate"));
                            }
                            capability_certificate__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(MsgRegisterUser {
                    leaf: leaf__,
                    grant: grant__,
                    capability_certificate: capability_certificate__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.MsgRegisterUser", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NativeAssetRegistration {
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
        if self.is_regulated {
            len += 1;
        }
        if !self.dk_pub.is_empty() {
            len += 1;
        }
        if self.registration_authority_vk.is_some() {
            len += 1;
        }
        if self.seizure_authority_vk.is_some() {
            len += 1;
        }
        if !self.ring_pk.is_empty() {
            len += 1;
        }
        if !self.ring_id.is_empty() {
            len += 1;
        }
        if !self.policy_id.is_empty() {
            len += 1;
        }
        if !self.permission.is_empty() {
            len += 1;
        }
        if !self.resource.is_empty() {
            len += 1;
        }
        if !self.audit_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.NativeAssetRegistration", len)?;
        if let Some(v) = self.asset_id.as_ref() {
            struct_ser.serialize_field("assetId", v)?;
        }
        if self.is_regulated {
            struct_ser.serialize_field("isRegulated", &self.is_regulated)?;
        }
        if !self.dk_pub.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("dkPub", pbjson::private::base64::encode(&self.dk_pub).as_str())?;
        }
        if let Some(v) = self.registration_authority_vk.as_ref() {
            struct_ser.serialize_field("registrationAuthorityVk", v)?;
        }
        if let Some(v) = self.seizure_authority_vk.as_ref() {
            struct_ser.serialize_field("seizureAuthorityVk", v)?;
        }
        if !self.ring_pk.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("ringPk", pbjson::private::base64::encode(&self.ring_pk).as_str())?;
        }
        if !self.ring_id.is_empty() {
            struct_ser.serialize_field("ringId", &self.ring_id)?;
        }
        if !self.policy_id.is_empty() {
            struct_ser.serialize_field("policyId", &self.policy_id)?;
        }
        if !self.permission.is_empty() {
            struct_ser.serialize_field("permission", &self.permission)?;
        }
        if !self.resource.is_empty() {
            struct_ser.serialize_field("resource", &self.resource)?;
        }
        if !self.audit_keys.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("auditKeys", pbjson::private::base64::encode(&self.audit_keys).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NativeAssetRegistration {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "asset_id",
            "assetId",
            "is_regulated",
            "isRegulated",
            "dk_pub",
            "dkPub",
            "registration_authority_vk",
            "registrationAuthorityVk",
            "seizure_authority_vk",
            "seizureAuthorityVk",
            "ring_pk",
            "ringPk",
            "ring_id",
            "ringId",
            "policy_id",
            "policyId",
            "permission",
            "resource",
            "audit_keys",
            "auditKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssetId,
            IsRegulated,
            DkPub,
            RegistrationAuthorityVk,
            SeizureAuthorityVk,
            RingPk,
            RingId,
            PolicyId,
            Permission,
            Resource,
            AuditKeys,
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
                            "isRegulated" | "is_regulated" => Ok(GeneratedField::IsRegulated),
                            "dkPub" | "dk_pub" => Ok(GeneratedField::DkPub),
                            "registrationAuthorityVk" | "registration_authority_vk" => Ok(GeneratedField::RegistrationAuthorityVk),
                            "seizureAuthorityVk" | "seizure_authority_vk" => Ok(GeneratedField::SeizureAuthorityVk),
                            "ringPk" | "ring_pk" => Ok(GeneratedField::RingPk),
                            "ringId" | "ring_id" => Ok(GeneratedField::RingId),
                            "policyId" | "policy_id" => Ok(GeneratedField::PolicyId),
                            "permission" => Ok(GeneratedField::Permission),
                            "resource" => Ok(GeneratedField::Resource),
                            "auditKeys" | "audit_keys" => Ok(GeneratedField::AuditKeys),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NativeAssetRegistration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.NativeAssetRegistration")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NativeAssetRegistration, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut asset_id__ = None;
                let mut is_regulated__ = None;
                let mut dk_pub__ = None;
                let mut registration_authority_vk__ = None;
                let mut seizure_authority_vk__ = None;
                let mut ring_pk__ = None;
                let mut ring_id__ = None;
                let mut policy_id__ = None;
                let mut permission__ = None;
                let mut resource__ = None;
                let mut audit_keys__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssetId => {
                            if asset_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assetId"));
                            }
                            asset_id__ = map_.next_value()?;
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
                        GeneratedField::RegistrationAuthorityVk => {
                            if registration_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registrationAuthorityVk"));
                            }
                            registration_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::SeizureAuthorityVk => {
                            if seizure_authority_vk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("seizureAuthorityVk"));
                            }
                            seizure_authority_vk__ = map_.next_value()?;
                        }
                        GeneratedField::RingPk => {
                            if ring_pk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringPk"));
                            }
                            ring_pk__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RingId => {
                            if ring_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ringId"));
                            }
                            ring_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::PolicyId => {
                            if policy_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyId"));
                            }
                            policy_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Resource => {
                            if resource__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resource"));
                            }
                            resource__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AuditKeys => {
                            if audit_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("auditKeys"));
                            }
                            audit_keys__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NativeAssetRegistration {
                    asset_id: asset_id__,
                    is_regulated: is_regulated__.unwrap_or_default(),
                    dk_pub: dk_pub__.unwrap_or_default(),
                    registration_authority_vk: registration_authority_vk__,
                    seizure_authority_vk: seizure_authority_vk__,
                    ring_pk: ring_pk__.unwrap_or_default(),
                    ring_id: ring_id__.unwrap_or_default(),
                    policy_id: policy_id__.unwrap_or_default(),
                    permission: permission__.unwrap_or_default(),
                    resource: resource__.unwrap_or_default(),
                    audit_keys: audit_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.NativeAssetRegistration", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OrbisCapabilityCertificate {
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
        if !self.r_point.is_empty() {
            len += 1;
        }
        if !self.response.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.OrbisCapabilityCertificate", len)?;
        if !self.chain_id.is_empty() {
            struct_ser.serialize_field("chainId", &self.chain_id)?;
        }
        if !self.r_point.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rPoint", pbjson::private::base64::encode(&self.r_point).as_str())?;
        }
        if !self.response.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("response", pbjson::private::base64::encode(&self.response).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OrbisCapabilityCertificate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "chain_id",
            "chainId",
            "r_point",
            "rPoint",
            "response",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ChainId,
            RPoint,
            Response,
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
                            "rPoint" | "r_point" => Ok(GeneratedField::RPoint),
                            "response" => Ok(GeneratedField::Response),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OrbisCapabilityCertificate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.OrbisCapabilityCertificate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OrbisCapabilityCertificate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut chain_id__ = None;
                let mut r_point__ = None;
                let mut response__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ChainId => {
                            if chain_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chainId"));
                            }
                            chain_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RPoint => {
                            if r_point__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rPoint"));
                            }
                            r_point__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Response => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("response"));
                            }
                            response__ =
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(OrbisCapabilityCertificate {
                    chain_id: chain_id__.unwrap_or_default(),
                    r_point: r_point__.unwrap_or_default(),
                    response: response__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.OrbisCapabilityCertificate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UserAssetStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "USER_ASSET_STATUS_UNSPECIFIED",
            Self::Active => "USER_ASSET_STATUS_ACTIVE",
            Self::Frozen => "USER_ASSET_STATUS_FROZEN",
            Self::Seized => "USER_ASSET_STATUS_SEIZED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for UserAssetStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "USER_ASSET_STATUS_UNSPECIFIED",
            "USER_ASSET_STATUS_ACTIVE",
            "USER_ASSET_STATUS_FROZEN",
            "USER_ASSET_STATUS_SEIZED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserAssetStatus;

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
                    "USER_ASSET_STATUS_UNSPECIFIED" => Ok(UserAssetStatus::Unspecified),
                    "USER_ASSET_STATUS_ACTIVE" => Ok(UserAssetStatus::Active),
                    "USER_ASSET_STATUS_FROZEN" => Ok(UserAssetStatus::Frozen),
                    "USER_ASSET_STATUS_SEIZED" => Ok(UserAssetStatus::Seized),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for UserRegistrationGrant {
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
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.UserRegistrationGrant", len)?;
        if let Some(v) = self.body.as_ref() {
            struct_ser.serialize_field("body", v)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UserRegistrationGrant {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "body",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Body,
            Signature,
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
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserRegistrationGrant;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.UserRegistrationGrant")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UserRegistrationGrant, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut body__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Body => {
                            if body__.is_some() {
                                return Err(serde::de::Error::duplicate_field("body"));
                            }
                            body__ = map_.next_value()?;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UserRegistrationGrant {
                    body: body__,
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.UserRegistrationGrant", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UserRegistrationGrantBody {
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
        if !self.policy_id.is_empty() {
            len += 1;
        }
        if self.valid_until_unix != 0 {
            len += 1;
        }
        if !self.nonce.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("shieldd.core.component.compliance.v1.UserRegistrationGrantBody", len)?;
        if let Some(v) = self.leaf.as_ref() {
            struct_ser.serialize_field("leaf", v)?;
        }
        if !self.policy_id.is_empty() {
            struct_ser.serialize_field("policyId", &self.policy_id)?;
        }
        if self.valid_until_unix != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("validUntilUnix", ToString::to_string(&self.valid_until_unix).as_str())?;
        }
        if !self.nonce.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nonce", pbjson::private::base64::encode(&self.nonce).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UserRegistrationGrantBody {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "leaf",
            "policy_id",
            "policyId",
            "valid_until_unix",
            "validUntilUnix",
            "nonce",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Leaf,
            PolicyId,
            ValidUntilUnix,
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
                            "leaf" => Ok(GeneratedField::Leaf),
                            "policyId" | "policy_id" => Ok(GeneratedField::PolicyId),
                            "validUntilUnix" | "valid_until_unix" => Ok(GeneratedField::ValidUntilUnix),
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
            type Value = UserRegistrationGrantBody;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct shieldd.core.component.compliance.v1.UserRegistrationGrantBody")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UserRegistrationGrantBody, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut leaf__ = None;
                let mut policy_id__ = None;
                let mut valid_until_unix__ = None;
                let mut nonce__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Leaf => {
                            if leaf__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leaf"));
                            }
                            leaf__ = map_.next_value()?;
                        }
                        GeneratedField::PolicyId => {
                            if policy_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policyId"));
                            }
                            policy_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ValidUntilUnix => {
                            if valid_until_unix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("validUntilUnix"));
                            }
                            valid_until_unix__ =
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
                Ok(UserRegistrationGrantBody {
                    leaf: leaf__,
                    policy_id: policy_id__.unwrap_or_default(),
                    valid_until_unix: valid_until_unix__.unwrap_or_default(),
                    nonce: nonce__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("shieldd.core.component.compliance.v1.UserRegistrationGrantBody", FIELDS, GeneratedVisitor)
    }
}
