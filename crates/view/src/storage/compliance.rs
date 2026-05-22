use anyhow::Context as _;
use r2d2_sqlite::rusqlite::{OptionalExtension, Transaction};

use penumbra_sdk_tct::StateCommitment;

/// Asset indexed leaf data (mirrors IndexedLeaf struct with full policy).
///
/// All fields must be persisted for correct tree reconstruction — the leaf
/// commitment hash depends on every policy field.
#[derive(Debug, Clone)]
pub struct IndexedLeafData {
    pub value: [u8; 32],
    pub next_index: u64,
    pub next_value: [u8; 32],
    pub dk_pub: [u8; 32],
    pub threshold: u128,
    pub slot_count: u32,
    pub route_policy_hash: [u8; 32],
    pub ring_pk: [u8; 32],
    pub ring_id_hash: [u8; 32],
    pub policy_id_hash: [u8; 32],
    pub permission_hash: [u8; 32],
    pub resource_hash: [u8; 32],
}

/// Full compliance user leaf data kept for addresses in sync scope.
#[derive(Debug, Clone)]
pub struct UserLeafData {
    pub position: u64,
    pub slot_id: u32,
    pub slot_derivation: [u8; 32],
    pub d: [u8; 32],
    pub commitment: StateCommitment,
}

/// Convert u64 position to i64 for SQLite storage, with overflow check.
#[inline]
fn position_to_i64(position: u64) -> anyhow::Result<i64> {
    i64::try_from(position).map_err(|_| {
        anyhow::anyhow!(
            "compliance tree position {} exceeds i64::MAX (tree too large for SQLite storage)",
            position
        )
    })
}

/// Convert u64 height to i64 for SQLite storage, with overflow check.
#[inline]
fn height_to_i64(height: u64) -> anyhow::Result<i64> {
    i64::try_from(height).map_err(|_| {
        anyhow::anyhow!(
            "block height {} exceeds i64::MAX (chain too long for SQLite storage)",
            height
        )
    })
}

/// Storage wrapper for compliance tree operations in SQLite.
#[derive(Debug)]
pub struct ComplianceTreeStore<'a, 'c: 'a>(pub &'a mut Transaction<'c>);

impl ComplianceTreeStore<'_, '_> {
    // ========== User Tree Operations ==========

    /// Get a user tree position's commitment.
    pub fn get_user_position(&mut self, position: u64) -> anyhow::Result<Option<StateCommitment>> {
        let position = position_to_i64(position)?;

        let mut stmt = self
            .0
            .prepare_cached("SELECT commitment FROM compliance_user_positions WHERE position = ?1")
            .context("failed to prepare user position query")?;

        let bytes = stmt
            .query_row::<Vec<u8>, _, _>((&position,), |row| row.get("commitment"))
            .optional()
            .context("failed to query user position")?;

        bytes
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes)
                    .map_err(|b: Vec<u8>| {
                        anyhow::anyhow!(
                            "user tree commitment must be 32 bytes, got {} (database may be corrupted)",
                            b.len()
                        )
                    })
                    .and_then(|array| StateCommitment::try_from(array).map_err(Into::into))
            })
            .transpose()
    }

    /// Add a user tree position.
    pub fn add_user_position(
        &mut self,
        position: u64,
        commitment: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let commitment = <[u8; 32]>::from(commitment).to_vec();

        self.0
            .prepare_cached(
                "INSERT INTO compliance_user_positions (position, commitment) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            )
            .context("failed to prepare user position insert")?
            .execute((&position, &commitment))
            .context("failed to insert user position")?;

        Ok(())
    }

    /// Get a user tree internal hash.
    pub fn get_user_hash(
        &mut self,
        position: u64,
        height: u8,
    ) -> anyhow::Result<Option<StateCommitment>> {
        let position = position_to_i64(position)?;

        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT hash FROM compliance_user_hashes WHERE position = ?1 AND height = ?2",
            )
            .context("failed to prepare user hash query")?;

        let bytes = stmt
            .query_row::<Vec<u8>, _, _>((&position, &height), |row| row.get("hash"))
            .optional()
            .context("failed to query user hash")?;

        bytes
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes)
                    .map_err(|b: Vec<u8>| {
                        anyhow::anyhow!(
                            "user tree hash must be 32 bytes, got {} (database may be corrupted)",
                            b.len()
                        )
                    })
                    .and_then(|array| StateCommitment::try_from(array).map_err(Into::into))
            })
            .transpose()
    }

    /// Add a user tree internal hash.
    pub fn add_user_hash(
        &mut self,
        position: u64,
        height: u8,
        hash: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let hash = <[u8; 32]>::from(hash).to_vec();

        self.0
            .prepare_cached(
                "INSERT INTO compliance_user_hashes (position, height, hash) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
            )
            .context("failed to prepare user hash insert")?
            .execute((&position, &height, &hash))
            .context("failed to insert user hash")?;

        Ok(())
    }

    // ========== Asset Tree (IMT) Operations ==========

    /// Get an asset tree indexed leaf.
    pub fn get_asset_leaf(&mut self, position: u64) -> anyhow::Result<Option<IndexedLeafData>> {
        let position = position_to_i64(position)?;

        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT value, next_index, next_value, dk_pub, threshold, slot_count, \
                 route_policy_hash, ring_pk, ring_id_hash, policy_id_hash, permission_hash, resource_hash \
                 FROM compliance_asset_leaves WHERE position = ?1",
            )
            .context("failed to prepare asset leaf query")?;

        let result = stmt
            .query_row((&position,), |row| {
                Ok((
                    row.get::<_, Vec<u8>>("value")?,
                    row.get::<_, i64>("next_index")?,
                    row.get::<_, Vec<u8>>("next_value")?,
                    row.get::<_, Vec<u8>>("dk_pub")?,
                    row.get::<_, Vec<u8>>("threshold")?,
                    row.get::<_, i64>("slot_count")?,
                    row.get::<_, Vec<u8>>("route_policy_hash")?,
                    row.get::<_, Vec<u8>>("ring_pk")?,
                    row.get::<_, Vec<u8>>("ring_id_hash")?,
                    row.get::<_, Vec<u8>>("policy_id_hash")?,
                    row.get::<_, Vec<u8>>("permission_hash")?,
                    row.get::<_, Vec<u8>>("resource_hash")?,
                ))
            })
            .optional()
            .context("failed to query asset leaf")?;

        match result {
            Some((
                value,
                next_index,
                next_value,
                dk_pub,
                threshold,
                slot_count,
                route_policy_hash,
                ring_pk,
                ring_id_hash,
                policy_id_hash,
                permission_hash,
                resource_hash,
            )) => {
                let to_arr = |v: Vec<u8>, name: &str| -> anyhow::Result<[u8; 32]> {
                    v.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "asset leaf {} must be 32 bytes, got {} at position {}",
                            name,
                            v.len(),
                            position
                        )
                    })
                };
                let value = to_arr(value, "value")?;
                let next_value = to_arr(next_value, "next_value")?;
                let dk_pub = to_arr(dk_pub, "dk_pub")?;
                let route_policy_hash = to_arr(route_policy_hash, "route_policy_hash")?;
                let ring_pk = to_arr(ring_pk, "ring_pk")?;
                let ring_id_hash = to_arr(ring_id_hash, "ring_id_hash")?;
                let policy_id_hash = to_arr(policy_id_hash, "policy_id_hash")?;
                let permission_hash = to_arr(permission_hash, "permission_hash")?;
                let resource_hash = to_arr(resource_hash, "resource_hash")?;
                let next_index = u64::try_from(next_index).map_err(|_| {
                    anyhow::anyhow!(
                        "asset leaf next_index is negative ({}) at position {}",
                        next_index,
                        position
                    )
                })?;
                // Support both 8-byte (legacy u64) and 16-byte (u128) threshold storage
                let threshold = if threshold.len() == 8 {
                    let bytes: [u8; 8] = threshold.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "asset leaf threshold must be 8 bytes, got {} at position {}",
                            v.len(),
                            position
                        )
                    })?;
                    u64::from_le_bytes(bytes) as u128
                } else if threshold.len() == 16 {
                    let bytes: [u8; 16] = threshold.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "asset leaf threshold must be 16 bytes, got {} at position {}",
                            v.len(),
                            position
                        )
                    })?;
                    u128::from_le_bytes(bytes)
                } else {
                    anyhow::bail!(
                        "asset leaf threshold must be 8 or 16 bytes, got {} at position {}",
                        threshold.len(),
                        position
                    )
                };
                let slot_count = u32::try_from(slot_count).map_err(|_| {
                    anyhow::anyhow!(
                        "asset leaf slot_count is negative or too large ({}) at position {}",
                        slot_count,
                        position
                    )
                })?;
                Ok(Some(IndexedLeafData {
                    value,
                    next_index,
                    next_value,
                    dk_pub,
                    threshold,
                    slot_count,
                    route_policy_hash,
                    ring_pk,
                    ring_id_hash,
                    policy_id_hash,
                    permission_hash,
                    resource_hash,
                }))
            }
            None => Ok(None),
        }
    }

    /// Add or update an asset tree indexed leaf.
    ///
    /// Uses INSERT OR REPLACE to handle both new leaves and updates to existing leaves
    /// (e.g., when updating a low leaf's next_index/next_value during IMT insertion).
    pub fn add_asset_leaf(&mut self, position: u64, leaf: IndexedLeafData) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let next_index = i64::try_from(leaf.next_index).map_err(|_| {
            anyhow::anyhow!(
                "asset leaf next_index {} exceeds i64::MAX (value too large for SQLite storage)",
                leaf.next_index
            )
        })?;
        // Store threshold as BLOB (16 bytes little-endian u128)
        let threshold_bytes = leaf.threshold.to_le_bytes().to_vec();

        // Use INSERT OR REPLACE to update existing leaves (critical for low leaf updates)
        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_asset_leaves \
                 (position, value, next_index, next_value, dk_pub, threshold, slot_count, \
                  route_policy_hash, ring_pk, ring_id_hash, policy_id_hash, permission_hash, resource_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .context("failed to prepare asset leaf insert")?
            .execute((
                &position,
                &leaf.value.to_vec(),
                &next_index,
                &leaf.next_value.to_vec(),
                &leaf.dk_pub.to_vec(),
                &threshold_bytes,
                &i64::from(leaf.slot_count),
                &leaf.route_policy_hash.to_vec(),
                &leaf.ring_pk.to_vec(),
                &leaf.ring_id_hash.to_vec(),
                &leaf.policy_id_hash.to_vec(),
                &leaf.permission_hash.to_vec(),
                &leaf.resource_hash.to_vec(),
            ))
            .context("failed to insert asset leaf")?;

        Ok(())
    }

    /// Get an asset tree internal hash.
    pub fn get_asset_hash(
        &mut self,
        position: u64,
        height: u8,
    ) -> anyhow::Result<Option<StateCommitment>> {
        let position = position_to_i64(position)?;

        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT hash FROM compliance_asset_hashes WHERE position = ?1 AND height = ?2",
            )
            .context("failed to prepare asset hash query")?;

        let bytes = stmt
            .query_row::<Vec<u8>, _, _>((&position, &height), |row| row.get("hash"))
            .optional()
            .context("failed to query asset hash")?;

        bytes
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes)
                    .map_err(|b: Vec<u8>| {
                        anyhow::anyhow!(
                            "asset tree hash must be 32 bytes, got {} (database may be corrupted)",
                            b.len()
                        )
                    })
                    .and_then(|array| StateCommitment::try_from(array).map_err(Into::into))
            })
            .transpose()
    }

    /// Add an asset tree internal hash.
    pub fn add_asset_hash(
        &mut self,
        position: u64,
        height: u8,
        hash: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let hash = <[u8; 32]>::from(hash).to_vec();

        self.0
            .prepare_cached(
                "INSERT INTO compliance_asset_hashes (position, height, hash) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
            )
            .context("failed to prepare asset hash insert")?
            .execute((&position, &height, &hash))
            .context("failed to insert asset hash")?;

        Ok(())
    }

    // ========== Anchor Operations ==========

    /// Get compliance anchors at a specific height.
    pub fn get_anchor(
        &mut self,
        height: u64,
    ) -> anyhow::Result<Option<(StateCommitment, StateCommitment)>> {
        let height = height_to_i64(height)?;

        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT user_root, asset_root FROM compliance_anchors WHERE height = ?1",
            )
            .context("failed to prepare anchor query")?;

        let result = stmt
            .query_row((&height,), |row| {
                let user_root: Vec<u8> = row.get("user_root")?;
                let asset_root: Vec<u8> = row.get("asset_root")?;
                Ok((user_root, asset_root))
            })
            .optional()
            .context("failed to query anchor")?;

        match result {
            Some((user_root, asset_root)) => {
                let user_root: [u8; 32] = user_root.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "anchor user_root must be 32 bytes, got {} at height {} (database may be corrupted)",
                        v.len(),
                        height
                    )
                })?;
                let asset_root: [u8; 32] = asset_root.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "anchor asset_root must be 32 bytes, got {} at height {} (database may be corrupted)",
                        v.len(),
                        height
                    )
                })?;
                Ok(Some((
                    StateCommitment::try_from(user_root)?,
                    StateCommitment::try_from(asset_root)?,
                )))
            }
            None => Ok(None),
        }
    }

    /// Add compliance anchors for a block height.
    pub fn add_anchor(
        &mut self,
        height: u64,
        user_anchor: StateCommitment,
        asset_anchor: StateCommitment,
    ) -> anyhow::Result<()> {
        let height = height_to_i64(height)?;
        let user_root = <[u8; 32]>::from(user_anchor).to_vec();
        let asset_root = <[u8; 32]>::from(asset_anchor).to_vec();

        self.0
            .prepare_cached(
                "INSERT INTO compliance_anchors (height, user_root, asset_root) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
            )
            .context("failed to prepare anchor insert")?
            .execute((&height, &user_root, &asset_root))
            .context("failed to insert anchor")?;

        Ok(())
    }

    /// Get the latest compliance anchors.
    pub fn get_latest_anchor(
        &mut self,
    ) -> anyhow::Result<Option<(u64, StateCommitment, StateCommitment)>> {
        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT height, user_root, asset_root FROM compliance_anchors ORDER BY height DESC LIMIT 1",
            )
            .context("failed to prepare latest anchor query")?;

        let result = stmt
            .query_row([], |row| {
                let height: i64 = row.get("height")?;
                let user_root: Vec<u8> = row.get("user_root")?;
                let asset_root: Vec<u8> = row.get("asset_root")?;
                Ok((height, user_root, asset_root))
            })
            .optional()
            .context("failed to query latest anchor")?;

        match result {
            Some((height, user_root, asset_root)) => {
                let user_root: [u8; 32] = user_root.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "latest anchor user_root must be 32 bytes, got {} (database may be corrupted)",
                        v.len()
                    )
                })?;
                let asset_root: [u8; 32] = asset_root.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "latest anchor asset_root must be 32 bytes, got {} (database may be corrupted)",
                        v.len()
                    )
                })?;
                Ok(Some((
                    height as u64,
                    StateCommitment::try_from(user_root)?,
                    StateCommitment::try_from(asset_root)?,
                )))
            }
            None => Ok(None),
        }
    }

    // ========== Leaf Data Operations (for addresses in sync scope) ==========

    /// Store full compliance leaf data for an address in the sync scope.
    pub fn add_leaf_data(
        &mut self,
        address: &[u8],
        asset_id: &[u8],
        position: u64,
        slot_id: u32,
        slot_derivation: &[u8],
        d: &[u8],
        commitment: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let commitment = <[u8; 32]>::from(commitment).to_vec();

        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_user_leaf_data \
                 (address, asset_id, position, slot_id, slot_derivation, d, commitment) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .context("failed to prepare leaf data insert")?
            .execute((
                address,
                asset_id,
                &position,
                &i64::from(slot_id),
                slot_derivation,
                d,
                &commitment,
            ))
            .context("failed to insert leaf data")?;

        Ok(())
    }

    /// Get full compliance leaf data for an address/asset pair.
    /// Returns full slot derivation data if found.
    pub fn get_leaf_data(
        &mut self,
        address: &[u8],
        asset_id: &[u8],
    ) -> anyhow::Result<Option<UserLeafData>> {
        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT position, slot_id, slot_derivation, d, commitment \
                 FROM compliance_user_leaf_data \
                 WHERE address = ?1 AND asset_id = ?2",
            )
            .context("failed to prepare leaf data query")?;

        let result = stmt
            .query_row((address, asset_id), |row| {
                let position: i64 = row.get("position")?;
                let slot_id: i64 = row.get("slot_id")?;
                let slot_derivation: Vec<u8> = row.get("slot_derivation")?;
                let d: Vec<u8> = row.get("d")?;
                let commitment: Vec<u8> = row.get("commitment")?;
                Ok((position, slot_id, slot_derivation, d, commitment))
            })
            .optional()
            .context("failed to query leaf data")?;

        match result {
            Some((position, slot_id, slot_derivation, d, commitment)) => {
                let slot_id = u32::try_from(slot_id).map_err(|_| {
                    anyhow::anyhow!("leaf data slot_id is negative or too large: {slot_id}")
                })?;
                let slot_derivation: [u8; 32] =
                    slot_derivation.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "leaf data slot_derivation must be 32 bytes, got {}",
                            v.len()
                        )
                    })?;
                let d: [u8; 32] = d.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("leaf data d must be 32 bytes, got {}", v.len())
                })?;
                let commitment: [u8; 32] = commitment.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "leaf data commitment must be 32 bytes, got {} (database may be corrupted)",
                        v.len()
                    )
                })?;
                Ok(Some(UserLeafData {
                    position: position as u64,
                    slot_id,
                    slot_derivation,
                    d,
                    commitment: StateCommitment::try_from(commitment)?,
                }))
            }
            None => Ok(None),
        }
    }

    // ========== Counterparty Tracking ==========

    /// Add a counterparty address to track.
    pub fn add_counterparty(&mut self, address: &[u8], height: u64) -> anyhow::Result<()> {
        let height = height_to_i64(height)?;

        self.0
            .prepare_cached(
                "INSERT OR IGNORE INTO compliance_counterparties \
                 (address, first_seen_height) VALUES (?1, ?2)",
            )
            .context("failed to prepare counterparty insert")?
            .execute((address, &height))
            .context("failed to insert counterparty")?;

        Ok(())
    }

    /// Check if an address is a tracked counterparty.
    pub fn is_counterparty(&mut self, address: &[u8]) -> anyhow::Result<bool> {
        let count: i64 = self
            .0
            .prepare_cached("SELECT COUNT(*) FROM compliance_counterparties WHERE address = ?1")
            .context("failed to prepare counterparty check")?
            .query_row((address,), |row| row.get(0))
            .context("failed to check counterparty")?;

        Ok(count > 0)
    }

    // ========== Tree Position Cursors ==========

    /// Get the current user tree position cursor.
    pub fn get_user_tree_position(&mut self) -> anyhow::Result<u64> {
        let position: i64 = self
            .0
            .prepare_cached("SELECT position FROM compliance_user_tree_position WHERE id = 0")
            .context("failed to prepare user tree position query")?
            .query_row([], |row| row.get(0))
            .context("failed to query user tree position")?;

        Ok(position as u64)
    }

    /// Set the user tree position cursor.
    pub fn set_user_tree_position(&mut self, position: u64) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;

        self.0
            .prepare_cached("UPDATE compliance_user_tree_position SET position = ?1 WHERE id = 0")
            .context("failed to prepare user tree position update")?
            .execute((&position,))
            .context("failed to update user tree position")?;

        Ok(())
    }

    /// Get the current asset tree leaf count.
    pub fn get_asset_tree_leaf_count(&mut self) -> anyhow::Result<u64> {
        let count: i64 = self
            .0
            .prepare_cached("SELECT leaf_count FROM compliance_asset_tree_position WHERE id = 0")
            .context("failed to prepare asset tree leaf count query")?
            .query_row([], |row| row.get(0))
            .context("failed to query asset tree leaf count")?;

        u64::try_from(count).map_err(|_| {
            anyhow::anyhow!(
                "asset tree leaf count {} is negative (database corruption)",
                count
            )
        })
    }

    /// Set the asset tree leaf count.
    pub fn set_asset_tree_leaf_count(&mut self, leaf_count: u64) -> anyhow::Result<()> {
        let count = i64::try_from(leaf_count).map_err(|_| {
            anyhow::anyhow!(
                "asset tree leaf count {} exceeds i64::MAX (tree too large for SQLite storage)",
                leaf_count
            )
        })?;

        self.0
            .prepare_cached(
                "UPDATE compliance_asset_tree_position SET leaf_count = ?1 WHERE id = 0",
            )
            .context("failed to prepare asset tree leaf count update")?
            .execute((&count,))
            .context("failed to update asset tree leaf count")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_store_spot_check() {
        // Set up the database
        let mut db = r2d2_sqlite::rusqlite::Connection::open_in_memory().unwrap();
        let mut tx = db.transaction().unwrap();
        tx.execute_batch(include_str!("schema.sql")).unwrap();

        // Use the compliance store
        let mut store = ComplianceTreeStore(&mut tx);

        // Test user position operations
        let commitment = StateCommitment::try_from([1u8; 32]).unwrap();
        store.add_user_position(0, commitment).unwrap();
        let retrieved = store.get_user_position(0).unwrap().unwrap();
        assert_eq!(<[u8; 32]>::from(retrieved), [1u8; 32]);

        // Test user hash operations
        let hash = StateCommitment::try_from([2u8; 32]).unwrap();
        store.add_user_hash(0, 1, hash).unwrap();
        let retrieved = store.get_user_hash(0, 1).unwrap().unwrap();
        assert_eq!(<[u8; 32]>::from(retrieved), [2u8; 32]);

        // Test asset leaf operations
        let leaf = IndexedLeafData {
            value: [3u8; 32],
            next_index: 1,
            next_value: [4u8; 32],
            dk_pub: [7u8; 32],
            threshold: 1000,
            slot_count: 10,
            route_policy_hash: [10u8; 32],
            ring_pk: [11u8; 32],
            ring_id_hash: [12u8; 32],
            policy_id_hash: [13u8; 32],
            permission_hash: [14u8; 32],
            resource_hash: [15u8; 32],
        };
        store.add_asset_leaf(0, leaf).unwrap();
        let retrieved = store.get_asset_leaf(0).unwrap().unwrap();
        assert_eq!(retrieved.value, [3u8; 32]);
        assert_eq!(retrieved.next_index, 1);
        assert_eq!(retrieved.next_value, [4u8; 32]);
        assert_eq!(retrieved.dk_pub, [7u8; 32]);
        assert_eq!(retrieved.slot_count, 10);
        assert_eq!(retrieved.threshold, 1000);
        assert_eq!(retrieved.route_policy_hash, [10u8; 32]);
        assert_eq!(retrieved.ring_pk, [11u8; 32]);
        assert_eq!(retrieved.ring_id_hash, [12u8; 32]);
        assert_eq!(retrieved.policy_id_hash, [13u8; 32]);
        assert_eq!(retrieved.permission_hash, [14u8; 32]);
        assert_eq!(retrieved.resource_hash, [15u8; 32]);

        // Test anchor operations
        let user_anchor = StateCommitment::try_from([5u8; 32]).unwrap();
        let asset_anchor = StateCommitment::try_from([6u8; 32]).unwrap();
        store.add_anchor(100, user_anchor, asset_anchor).unwrap();
        let (user, asset) = store.get_anchor(100).unwrap().unwrap();
        assert_eq!(<[u8; 32]>::from(user), [5u8; 32]);
        assert_eq!(<[u8; 32]>::from(asset), [6u8; 32]);

        // Test latest anchor
        let (height, user, asset) = store.get_latest_anchor().unwrap().unwrap();
        assert_eq!(height, 100);
        assert_eq!(<[u8; 32]>::from(user), [5u8; 32]);
        assert_eq!(<[u8; 32]>::from(asset), [6u8; 32]);
    }
}
