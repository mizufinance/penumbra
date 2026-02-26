//! Decrypted user data type for compliance scanning.

use crate::scanning::{CoreData, ExtensionData};

/// Decrypted compliance data (core + extension tiers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecryptedUserData {
    /// Core data (amount + self address)
    pub core: CoreData,
    /// Extension data (counterparty address)
    pub extension: ExtensionData,
}
