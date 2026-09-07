use anyhow::Context as _;
use r2d2_sqlite::rusqlite::{OptionalExtension, Transaction};

use shieldd_sdk_tct::StateCommitment;

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
    pub daily_volume_limit: u128,
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
    pub capk: [u8; 32],
    pub rnk_dh_pk: [u8; 32],
    pub rnk_commitment: [u8; 32],
    pub status: shieldd_sdk_compliance::UserAssetStatus,
    pub freeze_generation: u64,
    pub frozen_since_height: u64,
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

/// Convert a freeze generation to SQLite's signed integer representation.
#[inline]
fn freeze_generation_to_i64(generation: u64) -> anyhow::Result<i64> {
    i64::try_from(generation)
        .map_err(|_| anyhow::anyhow!("freeze generation {} exceeds i64::MAX", generation))
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
                "INSERT OR REPLACE INTO compliance_user_positions (position, commitment) VALUES (?1, ?2)",
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
                "SELECT value, next_index, next_value, dk_pub, daily_volume_limit, \
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
                    row.get::<_, Vec<u8>>("daily_volume_limit")?,
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
                daily_volume_limit,
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
                let daily_volume_limit: [u8; 16] =
                    daily_volume_limit.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "asset leaf daily_volume_limit must be 16 bytes, got {} at position {}",
                            v.len(),
                            position
                        )
                    })?;
                let daily_volume_limit = u128::from_le_bytes(daily_volume_limit);
                Ok(Some(IndexedLeafData {
                    value,
                    next_index,
                    next_value,
                    dk_pub,
                    daily_volume_limit,
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
        // Store daily_volume_limit as BLOB (16 bytes little-endian u128)
        let daily_volume_limit_bytes = leaf.daily_volume_limit.to_le_bytes().to_vec();

        // Use INSERT OR REPLACE to update existing leaves (critical for low leaf updates)
        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_asset_leaves \
                 (position, value, next_index, next_value, dk_pub, daily_volume_limit, \
                  route_policy_hash, ring_pk, ring_id_hash, policy_id_hash, permission_hash, resource_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )
            .context("failed to prepare asset leaf insert")?
            .execute((
                &position,
                &leaf.value.to_vec(),
                &next_index,
                &leaf.next_value.to_vec(),
                &leaf.dk_pub.to_vec(),
                &daily_volume_limit_bytes,
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
        if let Some((existing_user, existing_asset)) = self.get_anchor(height)? {
            anyhow::ensure!(
                existing_user == user_anchor && existing_asset == asset_anchor,
                "conflicting compliance anchors already exist at height {height}"
            );
            return Ok(());
        }
        let height = height_to_i64(height)?;
        let user_root = <[u8; 32]>::from(user_anchor).to_vec();
        let asset_root = <[u8; 32]>::from(asset_anchor).to_vec();

        self.0
            .prepare_cached(
                "INSERT INTO compliance_anchors (height, user_root, asset_root) VALUES (?1, ?2, ?3)",
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
        capk: &[u8],
        rnk_dh_pk: &[u8],
        rnk_commitment: &[u8],
        status: shieldd_sdk_compliance::UserAssetStatus,
        freeze_generation: u64,
        frozen_since_height: u64,
        commitment: StateCommitment,
    ) -> anyhow::Result<()> {
        let position = position_to_i64(position)?;
        let freeze_generation = freeze_generation_to_i64(freeze_generation)?;
        let frozen_since_height = height_to_i64(frozen_since_height)?;
        let commitment = <[u8; 32]>::from(commitment).to_vec();

        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_user_leaf_data \
                 (address, asset_id, position, capk, rnk_dh_pk, rnk_commitment, status, freeze_generation, frozen_since_height, commitment) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .context("failed to prepare leaf data insert")?
            .execute((
                address,
                asset_id,
                &position,
                capk,
                rnk_dh_pk,
                rnk_commitment,
                &(shieldd_sdk_proto::core::component::compliance::v1::UserAssetStatus::from(status)
                    as i32),
                &freeze_generation,
                &frozen_since_height,
                &commitment,
            ))
            .context("failed to insert leaf data")?;

        Ok(())
    }

    pub fn add_asset_policy(&mut self, asset_id: &[u8], policy: &[u8]) -> anyhow::Result<()> {
        self.0
            .prepare_cached(
                "INSERT OR REPLACE INTO compliance_asset_policies (asset_id, policy) VALUES (?1, ?2)",
            )
            .context("failed to prepare asset policy insert")?
            .execute((asset_id, policy))
            .context("failed to insert asset policy")?;
        Ok(())
    }

    /// Get full compliance leaf data for an address/asset pair.
    pub fn get_leaf_data(
        &mut self,
        address: &[u8],
        asset_id: &[u8],
    ) -> anyhow::Result<Option<UserLeafData>> {
        let mut stmt = self
            .0
            .prepare_cached(
                "SELECT position, capk, rnk_dh_pk, rnk_commitment, status, freeze_generation, frozen_since_height, commitment \
                 FROM compliance_user_leaf_data \
                 WHERE address = ?1 AND asset_id = ?2",
            )
            .context("failed to prepare leaf data query")?;

        let result = stmt
            .query_row((address, asset_id), |row| {
                let position: i64 = row.get("position")?;
                let capk: Vec<u8> = row.get("capk")?;
                let rnk_dh_pk: Vec<u8> = row.get("rnk_dh_pk")?;
                let rnk_commitment: Vec<u8> = row.get("rnk_commitment")?;
                let status: i32 = row.get("status")?;
                let freeze_generation: i64 = row.get("freeze_generation")?;
                let frozen_since_height: i64 = row.get("frozen_since_height")?;
                let commitment: Vec<u8> = row.get("commitment")?;
                Ok((
                    position,
                    capk,
                    rnk_dh_pk,
                    rnk_commitment,
                    status,
                    freeze_generation,
                    frozen_since_height,
                    commitment,
                ))
            })
            .optional()
            .context("failed to query leaf data")?;

        match result {
            Some((
                position,
                capk,
                rnk_dh_pk,
                rnk_commitment,
                status,
                freeze_generation,
                frozen_since_height,
                commitment,
            )) => {
                let capk: [u8; 32] = capk.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("leaf data capk must be 32 bytes, got {}", v.len())
                })?;
                let rnk_dh_pk: [u8; 32] = rnk_dh_pk.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("leaf data rnk_dh_pk must be 32 bytes, got {}", v.len())
                })?;
                let rnk_commitment: [u8; 32] =
                    rnk_commitment.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!(
                            "leaf data rnk commitment must be 32 bytes, got {}",
                            v.len()
                        )
                    })?;
                let commitment: [u8; 32] = commitment.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "leaf data commitment must be 32 bytes, got {} (database may be corrupted)",
                        v.len()
                    )
                })?;
                Ok(Some(UserLeafData {
                    position: position as u64,
                    capk,
                    rnk_dh_pk,
                    rnk_commitment,
                    status: status.try_into()?,
                    freeze_generation: u64::try_from(freeze_generation)
                        .context("stored freeze generation is negative")?,
                    frozen_since_height: u64::try_from(frozen_since_height)
                        .context("stored frozen-since height is negative")?,
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
            daily_volume_limit: 1000,
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
        assert_eq!(retrieved.daily_volume_limit, 1000);
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
        store.add_anchor(100, user_anchor, asset_anchor).unwrap();
        let conflicting = StateCommitment::try_from([7u8; 32]).unwrap();
        assert!(store.add_anchor(100, conflicting, asset_anchor).is_err());
        let (user, asset) = store.get_anchor(100).unwrap().unwrap();
        assert_eq!(<[u8; 32]>::from(user), [5u8; 32]);
        assert_eq!(<[u8; 32]>::from(asset), [6u8; 32]);

        // Test latest anchor
        let (height, user, asset) = store.get_latest_anchor().unwrap().unwrap();
        assert_eq!(height, 100);
        assert_eq!(<[u8; 32]>::from(user), [5u8; 32]);
        assert_eq!(<[u8; 32]>::from(asset), [6u8; 32]);

        // Freeze provenance must survive view persistence because it is part
        // of the authenticated compliance leaf.
        let leaf_commitment = StateCommitment::try_from([7u8; 32]).unwrap();
        store
            .add_leaf_data(
                &[8u8; 80],
                &[9u8; 32],
                3,
                &[10u8; 32],
                &[11u8; 32],
                &[12u8; 32],
                shieldd_sdk_compliance::UserAssetStatus::Frozen,
                4,
                120,
                leaf_commitment,
            )
            .unwrap();
        let leaf = store
            .get_leaf_data(&[8u8; 80], &[9u8; 32])
            .unwrap()
            .unwrap();
        assert_eq!(leaf.position, 3);
        assert_eq!(leaf.freeze_generation, 4);
        assert_eq!(leaf.frozen_since_height, 120);
        assert_eq!(leaf.status, shieldd_sdk_compliance::UserAssetStatus::Frozen);
    }
}
