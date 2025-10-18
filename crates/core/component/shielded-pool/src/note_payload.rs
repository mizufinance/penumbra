use anyhow::{Context, Error};

use penumbra_sdk_keys::keys::FullViewingKey;
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{penumbra::core::component::shielded_pool::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};

use crate::{note, Note, NoteCiphertext};
use decaf377_ka as ka;

#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::NotePayload", into = "pb::NotePayload")]
pub struct NotePayload {
    pub note_commitment: note::StateCommitment,
    pub ephemeral_key: ka::Public,
    pub encrypted_note: NoteCiphertext,
}

impl NotePayload {
    pub fn trial_decrypt(&self, fvk: &FullViewingKey) -> Option<Note> {
        // Notes are now encrypted with asset-specific transmission keys for cryptographic
        // enforcement of asset-level viewing permissions. To decrypt, we need to try
        // asset-specific IVKs for all known assets until one succeeds.

        // First, try the base IVK for backward compatibility with old notes
        let base_ivk = fvk.incoming();
        if let Ok(note) = Note::decrypt(&self.encrypted_note, base_ivk, &self.ephemeral_key) {
            tracing::debug!(
                note_commitment = ?note.commit(),
                ?note,
                "found note while scanning (base IVK)"
            );
            return self.verify_note(note, fvk);
        }

        // Get all known assets and try decrypting with each asset-specific IVK
        let asset_cache = penumbra_sdk_asset::asset::Cache::with_known_assets();

        // The Cache implements Deref to BTreeMap<Id, Metadata>, so we can iterate directly
        for (asset_id, _metadata) in asset_cache.iter() {
            // Derive the asset-specific IVK
            let asset_ivk = base_ivk.derive_asset_specific(&asset_id);

            // Try to decrypt with this asset-specific IVK
            if let Ok(note) =
                Note::decrypt_with_asset(&self.encrypted_note, &asset_ivk, &self.ephemeral_key)
            {
                tracing::debug!(
                    note_commitment = ?note.commit(),
                    ?note,
                    ?asset_id,
                    "found note while scanning (asset-specific IVK)"
                );
                return self.verify_note(note, fvk);
            }
        }

        // Could not decrypt with any known asset IVK
        None
    }

    /// Verify a decrypted note's validity.
    ///
    /// This is extracted as a separate method to avoid code duplication between
    /// the base IVK and asset-specific IVK decryption paths.
    fn verify_note(&self, note: Note, fvk: &FullViewingKey) -> Option<Note> {
        // Verification logic (if any fails, return None & log error)
        // Reject notes with zero amount
        if note.amount() == Amount::zero() {
            // This is only debug-level because it can happen honestly (e.g., swap claims, dummy spends).
            tracing::debug!("ignoring note recording zero assets");
            return None;
        }
        // Make sure spendable by keys
        if !note.controlled_by(fvk) {
            // This should be a warning, because no honestly generated note plaintext should
            // mismatch the FVK that can detect and decrypt it.
            tracing::warn!("decrypted note that is not spendable by provided full viewing key");
            return None;
        }
        // Make sure note commitment matches
        if note.commit() != self.note_commitment {
            // This should be a warning, because no honestly generated note plaintext should
            // fail to match the note commitment actually included in the chain.
            tracing::warn!("decrypted note does not match provided note commitment");
            return None;
        }

        // NOTE: We intentionally return `Option` here instead of `Result`
        // such that we gracefully drop malformed notes instead of returning an error
        // that may propagate up the call stack and cause a panic.
        // All errors in parsing notes must not cause a panic in the view service.
        // A panic when parsing a specific note could link the fact that the malformed
        // note can be successfully decrypted with a specific IP.
        //
        // See "REJECT" attack (CVE-2019-16930) for a similar attack in ZCash
        // Section 4.1 in https://crypto.stanford.edu/timings/pingreject.pdf
        Some(note)
    }
}

impl std::fmt::Debug for NotePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotePayload")
            .field("note_commitment", &self.note_commitment)
            .field("ephemeral_key", &self.ephemeral_key)
            .field("encrypted_note", &"...")
            .finish()
    }
}

impl DomainType for NotePayload {
    type Proto = pb::NotePayload;
}

impl From<NotePayload> for pb::NotePayload {
    fn from(msg: NotePayload) -> Self {
        pb::NotePayload {
            note_commitment: Some(msg.note_commitment.into()),
            ephemeral_key: msg.ephemeral_key.0.to_vec(),
            encrypted_note: Some(msg.encrypted_note.into()),
        }
    }
}

impl TryFrom<pb::NotePayload> for NotePayload {
    type Error = Error;

    fn try_from(proto: pb::NotePayload) -> anyhow::Result<Self, Self::Error> {
        Ok(NotePayload {
            note_commitment: proto
                .note_commitment
                .ok_or_else(|| anyhow::anyhow!("missing note commitment"))?
                .try_into()?,
            ephemeral_key: ka::Public::try_from(&proto.ephemeral_key[..])
                .context("ephemeral key malformed")?,
            encrypted_note: proto
                .encrypted_note
                .ok_or_else(|| anyhow::anyhow!("missing encrypted note"))?
                .try_into()?,
        })
    }
}
