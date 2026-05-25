use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

pub const FLOW_TYPE_PRIVATE_TRANSFER: &str = "private_transfer";
pub const FLOW_TYPE_SHIELD: &str = "shield";
pub const FLOW_TYPE_WITHDRAW: &str = "withdraw";
pub const DECRYPTED_VIA_ISSUER_DK: &str = "issuer_dk";
pub const DECRYPTED_VIA_ORBIS_PRE: &str = "orbis_pre";
pub const DECRYPTED_VIA_PUBLIC: &str = "public";
pub const AUDIT_STATUS_PENDING: &str = "pending";
pub const AUDIT_STATUS_EVIDENCE_VALID: &str = "evidence_valid";
pub const AUDIT_STATUS_EVIDENCE_INVALID: &str = "evidence_invalid";
pub const AUDIT_STATUS_DECRYPT_FAILED: &str = "decrypt_failed";
pub const AUDIT_STATUS_AUDIT_COMPLETE: &str = "audit_complete";
pub const SCREEN_STATUS_PENDING: &str = "pending";
pub const SCREEN_STATUS_IRRELEVANT: &str = "irrelevant";
pub const SCREEN_STATUS_DETECTED: &str = "detected";
pub const SCREEN_STATUS_INVALID: &str = "invalid";
pub const DETECTION_STATUS_DETECTED: &str = "detected";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowType {
    #[serde(rename = "private_transfer")]
    PrivateTransfer,
    #[serde(rename = "shield")]
    Shield,
    #[serde(rename = "withdraw")]
    Withdraw,
}

impl FlowType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateTransfer => FLOW_TYPE_PRIVATE_TRANSFER,
            Self::Shield => FLOW_TYPE_SHIELD,
            Self::Withdraw => FLOW_TYPE_WITHDRAW,
        }
    }
}

impl fmt::Display for FlowType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FlowType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            FLOW_TYPE_PRIVATE_TRANSFER => Ok(Self::PrivateTransfer),
            FLOW_TYPE_SHIELD => Ok(Self::Shield),
            FLOW_TYPE_WITHDRAW => Ok(Self::Withdraw),
            _ => Err(anyhow!("unknown audit flow type {value:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecryptedVia {
    #[serde(rename = "issuer_dk")]
    IssuerDetectionKey,
    #[serde(rename = "orbis_pre")]
    OrbisPre,
    #[serde(rename = "public")]
    Public,
}

impl DecryptedVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IssuerDetectionKey => DECRYPTED_VIA_ISSUER_DK,
            Self::OrbisPre => DECRYPTED_VIA_ORBIS_PRE,
            Self::Public => DECRYPTED_VIA_PUBLIC,
        }
    }
}

impl fmt::Display for DecryptedVia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DecryptedVia {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            DECRYPTED_VIA_ISSUER_DK => Ok(Self::IssuerDetectionKey),
            DECRYPTED_VIA_ORBIS_PRE => Ok(Self::OrbisPre),
            DECRYPTED_VIA_PUBLIC => Ok(Self::Public),
            _ => Err(anyhow!("unknown audit decrypted_via value {value:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "irrelevant")]
    Irrelevant,
    #[serde(rename = "detected")]
    Detected,
    #[serde(rename = "invalid")]
    Invalid,
}

impl ScreenStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => SCREEN_STATUS_PENDING,
            Self::Irrelevant => SCREEN_STATUS_IRRELEVANT,
            Self::Detected => SCREEN_STATUS_DETECTED,
            Self::Invalid => SCREEN_STATUS_INVALID,
        }
    }

    pub fn try_advance(from: Self, to: Self) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let allowed = matches!(
            (from, to),
            (Self::Pending, Self::Irrelevant)
                | (Self::Pending, Self::Detected)
                | (Self::Pending, Self::Invalid)
        );
        anyhow::ensure!(
            allowed,
            "illegal screen status transition {} -> {}",
            from,
            to
        );
        Ok(())
    }
}

impl fmt::Display for ScreenStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScreenStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            SCREEN_STATUS_PENDING => Ok(Self::Pending),
            SCREEN_STATUS_IRRELEVANT => Ok(Self::Irrelevant),
            SCREEN_STATUS_DETECTED => Ok(Self::Detected),
            SCREEN_STATUS_INVALID => Ok(Self::Invalid),
            _ => Err(anyhow!("unknown screen status {value:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionStatus {
    #[serde(rename = "detected")]
    Detected,
}

impl DetectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Detected => DETECTION_STATUS_DETECTED,
        }
    }

    pub fn try_advance(from: Self, to: Self) -> Result<()> {
        if from == to {
            return Ok(());
        }
        anyhow::bail!("illegal detection status transition {} -> {}", from, to)
    }
}

impl fmt::Display for DetectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DetectionStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            DETECTION_STATUS_DETECTED => Ok(Self::Detected),
            _ => Err(anyhow!("unknown detection status {value:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "evidence_valid")]
    EvidenceValid,
    #[serde(rename = "evidence_invalid")]
    EvidenceInvalid,
    #[serde(rename = "decrypt_failed")]
    DecryptFailed,
    #[serde(rename = "audit_complete")]
    AuditComplete,
}

impl AuditStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => AUDIT_STATUS_PENDING,
            Self::EvidenceValid => AUDIT_STATUS_EVIDENCE_VALID,
            Self::EvidenceInvalid => AUDIT_STATUS_EVIDENCE_INVALID,
            Self::DecryptFailed => AUDIT_STATUS_DECRYPT_FAILED,
            Self::AuditComplete => AUDIT_STATUS_AUDIT_COMPLETE,
        }
    }

    pub fn try_advance(from: Self, to: Self) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let allowed = matches!(
            (from, to),
            (Self::Pending, Self::EvidenceValid)
                | (Self::Pending, Self::EvidenceInvalid)
                | (Self::EvidenceInvalid, Self::EvidenceValid)
                | (Self::EvidenceValid, Self::EvidenceInvalid)
                | (Self::EvidenceValid, Self::DecryptFailed)
                | (Self::EvidenceValid, Self::AuditComplete)
                | (Self::DecryptFailed, Self::AuditComplete)
        );
        anyhow::ensure!(
            allowed,
            "illegal audit status transition {} -> {}",
            from,
            to
        );
        Ok(())
    }
}

impl fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AuditStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            AUDIT_STATUS_PENDING => Ok(Self::Pending),
            AUDIT_STATUS_EVIDENCE_VALID => Ok(Self::EvidenceValid),
            AUDIT_STATUS_EVIDENCE_INVALID => Ok(Self::EvidenceInvalid),
            AUDIT_STATUS_DECRYPT_FAILED => Ok(Self::DecryptFailed),
            AUDIT_STATUS_AUDIT_COMPLETE => Ok(Self::AuditComplete),
            _ => Err(anyhow!("unknown audit status {value:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditStatus, DetectionStatus, ScreenStatus};

    #[test]
    fn audit_status_transition_table_is_explicit() {
        let statuses = [
            AuditStatus::Pending,
            AuditStatus::EvidenceValid,
            AuditStatus::EvidenceInvalid,
            AuditStatus::DecryptFailed,
            AuditStatus::AuditComplete,
        ];
        let allowed = [
            (AuditStatus::Pending, AuditStatus::Pending),
            (AuditStatus::Pending, AuditStatus::EvidenceValid),
            (AuditStatus::Pending, AuditStatus::EvidenceInvalid),
            (AuditStatus::EvidenceValid, AuditStatus::EvidenceValid),
            (AuditStatus::EvidenceValid, AuditStatus::EvidenceInvalid),
            (AuditStatus::EvidenceValid, AuditStatus::DecryptFailed),
            (AuditStatus::EvidenceValid, AuditStatus::AuditComplete),
            (AuditStatus::EvidenceInvalid, AuditStatus::EvidenceInvalid),
            (AuditStatus::EvidenceInvalid, AuditStatus::EvidenceValid),
            (AuditStatus::DecryptFailed, AuditStatus::DecryptFailed),
            (AuditStatus::DecryptFailed, AuditStatus::AuditComplete),
            (AuditStatus::AuditComplete, AuditStatus::AuditComplete),
        ];

        for from in statuses {
            for to in statuses {
                let is_allowed = allowed.contains(&(from, to));
                assert_eq!(
                    AuditStatus::try_advance(from, to).is_ok(),
                    is_allowed,
                    "transition {from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn screen_status_transition_table_is_explicit() {
        let statuses = [
            ScreenStatus::Pending,
            ScreenStatus::Irrelevant,
            ScreenStatus::Detected,
            ScreenStatus::Invalid,
        ];
        let allowed = [
            (ScreenStatus::Pending, ScreenStatus::Pending),
            (ScreenStatus::Pending, ScreenStatus::Irrelevant),
            (ScreenStatus::Pending, ScreenStatus::Detected),
            (ScreenStatus::Pending, ScreenStatus::Invalid),
            (ScreenStatus::Irrelevant, ScreenStatus::Irrelevant),
            (ScreenStatus::Detected, ScreenStatus::Detected),
            (ScreenStatus::Invalid, ScreenStatus::Invalid),
        ];

        for from in statuses {
            for to in statuses {
                let is_allowed = allowed.contains(&(from, to));
                assert_eq!(
                    ScreenStatus::try_advance(from, to).is_ok(),
                    is_allowed,
                    "transition {from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn detection_status_transition_table_is_explicit() {
        assert!(
            DetectionStatus::try_advance(DetectionStatus::Detected, DetectionStatus::Detected)
                .is_ok()
        );
    }
}
