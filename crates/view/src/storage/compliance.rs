use anyhow::Context as _;
use r2d2_sqlite::rusqlite::{OptionalExtension, Transaction};

use penumbra_sdk_tct::StateCommitment;

/// Asset indexed leaf data.
#[derive(Debug, Clone)]
pub struct IndexedLeafData {
    pub value: [u8; 32],
    pub next_index: u64,
    pub next_value: [u8; 32],
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
                "SELECT value, next_index, next_value FROM compliance_asset_leaves WHERE position = ?1",
            )
            .context("failed to prepare asset leaf query")?;

        let result = stmt
            .query_row((&position,), |row| {
                let value: Vec<u8> = row.get("value")?;
                let next_index: i64 = row.get("next_index")?;
                let next_value: Vec<u8> = row.get("next_value")?;
                Ok((value, next_index, next_value))
            })
            .optional()
            .context("failed to query asset leaf")?;

        match result {
            Some((value, next_index, next_value)) => {
                let value: [u8; 32] = value.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "asset leaf value must be 32 bytes, got {} at position {} (database may be corrupted)",
                        v.len(),
                        position
                    )
                })?;
                let next_value: [u8; 32] = next_value.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "asset leaf next_value must be 32 bytes, got {} at position {} (database may be corrupted)",
                        v.len(),
                        position
                    )
                })?;
                let next_index = u64::try_from(next_index).map_err(|_| {
                    anyhow::anyhow!(
                        "asset leaf next_index is negative ({}) at position {} (database may be corrupted)",
                        next_index,
                        position
                    )
                })?;
                Ok(Some(IndexedLeafData {
                    value,
                    next_index,
                    next_value,
                }))
            }
            None => Ok(None),
        }
    }

    /// Add an asset tree indexed leaf.
    pub fn add_asset_leaf(&mut self, position: u64, leaf: IndexedLeafData) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let next_index = i64::try_from(leaf.next_index).map_err(|_| {
            anyhow::anyhow!(
                "asset leaf next_index {} exceeds i64::MAX (value too large for SQLite storage)",
                leaf.next_index
            )
        })?;

        self.0
            .prepare_cached(
                "INSERT INTO compliance_asset_leaves (position, value, next_index, next_value) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
            )
            .context("failed to prepare asset leaf insert")?
            .execute((
                &position,
                &leaf.value.to_vec(),
                &next_index,
                &leaf.next_value.to_vec(),
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
        let height = height as i64;

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
        let height = height as i64;
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
        ack: &[u8],
        commitment: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let commitment = <[u8; 32]>::from(commitment).to_vec();

        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_user_leaf_data \
                 (address, asset_id, position, address_compliance_key, commitment) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .context("failed to prepare leaf data insert")?
            .execute((address, asset_id, &position, ack, &commitment))
            .context("failed to insert leaf data")?;

        Ok(())
    }

    /// Get full compliance leaf data for an address/asset pair.
    pub fn get_leaf_data(
        &mut self,
        address: &[u8],
        asset_id: &[u8],
    ) -> anyhow::Result<Option<(u64, Vec<u8>, StateCommitment)>> {
        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT position, address_compliance_key, commitment \
                 FROM compliance_user_leaf_data \
                 WHERE address = ?1 AND asset_id = ?2",
            )
            .context("failed to prepare leaf data query")?;

        let result = stmt
            .query_row((address, asset_id), |row| {
                let position: i64 = row.get("position")?;
                let ack: Vec<u8> = row.get("address_compliance_key")?;
                let commitment: Vec<u8> = row.get("commitment")?;
                Ok((position, ack, commitment))
            })
            .optional()
            .context("failed to query leaf data")?;

        match result {
            Some((position, ack, commitment)) => {
                let commitment: [u8; 32] = commitment.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "leaf data commitment must be 32 bytes, got {} (database may be corrupted)",
                        v.len()
                    )
                })?;
                Ok(Some((
                    position as u64,
                    ack,
                    StateCommitment::try_from(commitment)?,
                )))
            }
            None => Ok(None),
        }
    }

    // ========== Counterparty Tracking ==========

    /// Add a counterparty address to track.
    pub fn add_counterparty(&mut self, address: &[u8], height: u64) -> anyhow::Result<()> {
        let height = height as i64;

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

        Ok(count as u64)
    }

    /// Set the asset tree leaf count.
    pub fn set_asset_tree_leaf_count(&mut self, leaf_count: u64) -> anyhow::Result<()> {
        let count = leaf_count as i64;

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
        };
        store.add_asset_leaf(0, leaf).unwrap();
        let retrieved = store.get_asset_leaf(0).unwrap().unwrap();
        assert_eq!(retrieved.value, [3u8; 32]);
        assert_eq!(retrieved.next_index, 1);
        assert_eq!(retrieved.next_value, [4u8; 32]);

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
