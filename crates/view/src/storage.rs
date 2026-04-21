use std::{collections::BTreeMap, num::NonZeroU64, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use camino::Utf8Path;
use decaf377::Fq;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use r2d2_sqlite::{
    rusqlite::{OpenFlags, OptionalExtension},
    SqliteConnectionManager,
};
use sha2::{Digest, Sha256};
use tap::{Tap, TapFallible};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::spawn_blocking,
};
use tracing::{error_span, Instrument};
use url::Url;

use penumbra_sdk_app::params::AppParameters;
use penumbra_sdk_asset::{asset, asset::Id, asset::Metadata, Value};
use penumbra_sdk_fee::GasPrices;
use penumbra_sdk_keys::{keys::AddressIndex, Address, FullViewingKey};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{
    core::app::v1::{
        query_service_client::QueryServiceClient as AppQueryServiceClient, AppParametersRequest,
    },
    DomainType,
};
use penumbra_sdk_sct::{CommitmentSource, Nullifier};
use penumbra_sdk_shielded_pool::{fmd, note, Note, Rseed};
use penumbra_sdk_tct::{self as tct, builder::epoch::Root};
use penumbra_sdk_transaction::Transaction;
use sct::TreeStore;
use tct::StateCommitment;

use crate::{sync::FilteredBlock, SpendableNoteRecord};

pub(crate) mod compliance;
mod sct;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BalanceEntry {
    pub id: Id,
    pub amount: u128,
    pub address_index: AddressIndex,
}

/// The hash of the schema for the database.
static SCHEMA_HASH: Lazy<String> =
    Lazy::new(|| hex::encode(Sha256::digest(include_str!("storage/schema.sql"))));

#[derive(Clone)]
pub struct Storage {
    pool: r2d2::Pool<SqliteConnectionManager>,

    /// This allows an optimization where we only commit to the database after
    /// scanning a nonempty block.
    ///
    /// If this is `Some`, we have uncommitted empty blocks up to the inner height.
    /// If this is `None`, we don't.
    ///
    /// Using a `NonZeroU64` ensures that `Option<NonZeroU64>` fits in 8 bytes.
    uncommitted_height: Arc<Mutex<Option<NonZeroU64>>>,

    scanned_notes_tx: tokio::sync::broadcast::Sender<SpendableNoteRecord>,
    scanned_nullifiers_tx: tokio::sync::broadcast::Sender<Nullifier>,
}

impl Storage {
    /// If the database at `storage_path` exists, [`Self::load`] it, otherwise, [`Self::initialize`] it.
    #[tracing::instrument(
        skip_all,
        fields(
            path = ?storage_path.as_ref().map(|p| p.as_ref().as_str()),
            url = %node,
        )
    )]
    pub async fn load_or_initialize(
        storage_path: Option<impl AsRef<Utf8Path>>,
        fvk: &FullViewingKey,
        node: Url,
    ) -> anyhow::Result<Self> {
        if let Some(path) = storage_path.as_ref().map(AsRef::as_ref) {
            if path.exists() {
                tracing::debug!(?path, "database exists");
                return Self::load(path).await;
            } else {
                tracing::debug!(?path, "database does not exist");
            }
        };

        let mut client = AppQueryServiceClient::connect(node.to_string())
            .instrument(error_span!("connecting_to_endpoint"))
            .await
            .tap_err(|error| {
                tracing::error!(?error, "failed to connect to app query service endpoint")
            })?
            .tap(|_| tracing::debug!("connected to app query service endpoint"));
        let params = client
            .app_parameters(tonic::Request::new(AppParametersRequest {}))
            .instrument(error_span!("getting_app_parameters"))
            .await?
            .into_inner()
            .try_into()?;

        Self::initialize(storage_path, fvk.clone(), params).await
    }

    fn connect(
        path: Option<impl AsRef<Utf8Path>>,
    ) -> anyhow::Result<r2d2::Pool<SqliteConnectionManager>> {
        if let Some(path) = path {
            let manager = SqliteConnectionManager::file(path.as_ref())
                .with_flags(
                    // Don't allow opening URIs, because they can change the behavior of the database; we
                    // just want to open normal filepaths.
                    OpenFlags::default() & !OpenFlags::SQLITE_OPEN_URI,
                )
                .with_init(|conn| {
                    // "NORMAL" will be consistent, but maybe not durable -- this is fine,
                    // since all our data is being synced from the chain, so if we lose a dbtx,
                    // it's like we're resuming sync from a previous height.
                    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                    // We use `prepare_cached` a fair amount: this is an overestimate of the number
                    // of cached prepared statements likely to be used.
                    conn.set_prepared_statement_cache_capacity(32);
                    Ok(())
                });
            Ok(r2d2::Pool::builder()
                // We set max_size=1 to avoid "database is locked" sqlite errors,
                // when accessing across multiple threads.
                .max_size(1)
                .build(manager)?)
        } else {
            let manager = SqliteConnectionManager::memory();
            // Max size needs to be set to 1, otherwise a new in-memory database is created for each
            // connection to the pool, which results in very confusing errors.
            //
            // Lifetimes and timeouts are likewise configured to their maximum values, since
            // the in-memory database will disappear on connection close.
            Ok(r2d2::Pool::builder()
                .max_size(1)
                .min_idle(Some(1))
                .max_lifetime(Some(Duration::MAX))
                .idle_timeout(Some(Duration::MAX))
                .build(manager)?)
        }
    }

    pub async fn load(path: impl AsRef<Utf8Path>) -> anyhow::Result<Self> {
        let storage = Self {
            pool: Self::connect(Some(path))?,
            uncommitted_height: Arc::new(Mutex::new(None)),
            scanned_notes_tx: broadcast::channel(128).0,
            scanned_nullifiers_tx: broadcast::channel(512).0,
        };

        spawn_blocking(move || {
            // Check the version of the software used when first initializing this database.
            // If it doesn't match the current version, we should report the error to the user.
            let actual_schema_hash: String = storage
                .pool
                .get()?
                .query_row("SELECT schema_hash FROM schema_hash", (), |row| {
                    row.get("schema_hash")
                })
                .context("failed to query database schema version: the database was probably created by an old client version, and needs to be reset and resynchronized")?;

            if actual_schema_hash != *SCHEMA_HASH {
                let database_client_version: String = storage
                    .pool
                    .get()?
                    .query_row("SELECT client_version FROM client_version", (), |row| {
                        row.get("client_version")
                    })
                    .context("failed to query client version: the database was probably created by an old client version, and needs to be reset and resynchronized")?;

                anyhow::bail!(
                    "can't load view database created by client version {} using client version {}: they have different schemata, so you need to reset your view database and resynchronize by running pcli view reset",
                    database_client_version,
                    env!("CARGO_PKG_VERSION"),
                );
            }

            Ok(storage)
        })
            .await?
    }

    pub async fn initialize(
        storage_path: Option<impl AsRef<Utf8Path>>,
        fvk: FullViewingKey,
        params: AppParameters,
    ) -> anyhow::Result<Self> {
        tracing::debug!(storage_path = ?storage_path.as_ref().map(AsRef::as_ref), ?fvk, ?params);

        // Connect to the database (or create it)
        let pool = Self::connect(storage_path)?;

        let out = spawn_blocking(move || {
            // In one database transaction, populate everything
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;

            // Create the tables
            tx.execute_batch(include_str!("storage/schema.sql"))?;

            let params_bytes = params.encode_to_vec();
            tx.execute(
                "INSERT INTO kv (k, v) VALUES ('app_params', ?1)",
                [&params_bytes[..]],
            )?;

            let fvk_bytes = fvk.encode_to_vec();
            tx.execute("INSERT INTO kv (k, v) VALUES ('fvk', ?1)", [&fvk_bytes[..]])?;

            // Insert -1 as a signaling value for pre-genesis.
            // We just have to be careful to treat negative values as None
            // in last_sync_height.
            tx.execute("INSERT INTO sync_height (height) VALUES (-1)", ())?;

            // Insert the schema hash into the database
            tx.execute(
                "INSERT INTO schema_hash (schema_hash) VALUES (?1)",
                [&*SCHEMA_HASH],
            )?;

            // Insert the client version into the database
            tx.execute(
                "INSERT INTO client_version (client_version) VALUES (?1)",
                [env!("CARGO_PKG_VERSION")],
            )?;

            tx.commit()?;
            drop(conn);

            anyhow::Ok(Storage {
                pool,
                uncommitted_height: Arc::new(Mutex::new(None)),
                scanned_notes_tx: broadcast::channel(128).0,
                scanned_nullifiers_tx: broadcast::channel(512).0,
            })
        })
        .await??;

        out.update_epoch(0, None, Some(0)).await?;

        Ok(out)
    }

    /// Loads asset metadata from a JSON file and use to update the database.
    pub async fn load_asset_metadata(
        &self,
        registry_path: impl AsRef<Utf8Path>,
    ) -> anyhow::Result<()> {
        tracing::debug!(registry_path = ?registry_path.as_ref(), "loading asset metadata");
        let registry_path = registry_path.as_ref();
        // Parse into a serde_json::Value first so we can get the bits we care about
        let mut registry_json: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(registry_path)
                .context("failed to read file")?
                .as_str(),
        )
        .context("failed to parse JSON")?;

        let registry: BTreeMap<String, Metadata> = serde_json::value::from_value(
            registry_json
                .get_mut("assetById")
                .ok_or_else(|| anyhow::anyhow!("missing assetById"))?
                .take(),
        )
        .context("could not parse asset registry")?;

        for metadata in registry.into_values() {
            self.record_asset(metadata).await?;
        }

        Ok(())
    }

    /// Query for account balance by address
    pub async fn balances(
        &self,
        address_index: Option<AddressIndex>,
        asset_id: Option<asset::Id>,
    ) -> anyhow::Result<Vec<BalanceEntry>> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let query = "SELECT notes.asset_id, notes.amount, spendable_notes.address_index
                FROM    notes
                JOIN    spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                WHERE   spendable_notes.height_spent IS NULL";

            tracing::debug!(?query);

            // Combine notes of the same asset/address index together
            let mut balances: BTreeMap<AddressIndex, BTreeMap<asset::Id, Amount>> = BTreeMap::new();

            for result in pool.get()?.prepare_cached(query)?.query_map([], |row| {
                let asset_id = row.get::<&str, Vec<u8>>("asset_id")?;
                let amount = row.get::<&str, Vec<u8>>("amount")?;
                let address_index = row.get::<&str, Vec<u8>>("address_index")?;

                Ok((asset_id, amount, address_index))
            })? {
                let (id, amount, index) = result?;

                let id = Id::try_from(id.as_slice())?;

                let amount: Amount = Amount::from_be_bytes(
                    amount
                        .as_slice()
                        .try_into()
                        .expect("amount slice of incorrect length"),
                );

                let index = AddressIndex::try_from(index.as_slice())?;

                // Skip this entry if not captured by address index filter
                if let Some(address_index) = address_index {
                    if address_index != index {
                        continue;
                    }
                }
                if let Some(asset_id) = asset_id {
                    if asset_id != id {
                        continue;
                    }
                }

                balances
                    .entry(index)
                    .or_insert_with(BTreeMap::new)
                    .entry(id)
                    .and_modify(|e| *e += amount)
                    .or_insert(amount);
            }

            let entries = balances
                .into_iter()
                .flat_map(|(index, assets)| {
                    assets.into_iter().map(move |(id, amount)| BalanceEntry {
                        id,
                        amount: amount.into(),
                        address_index: index,
                    })
                })
                .collect::<Vec<_>>();
            Ok(entries)
        })
        .await?
    }

    /// Query for a note by its note commitment, optionally waiting until the note is detected.
    pub async fn note_by_commitment(
        &self,
        note_commitment: tct::StateCommitment,
        await_detection: bool,
    ) -> anyhow::Result<SpendableNoteRecord> {
        // Start subscribing now, before querying for whether we already
        // have the record, so that we can't miss it if we race a write.
        let mut rx = self.scanned_notes_tx.subscribe();

        let pool = self.pool.clone();

        if let Some(record) = spawn_blocking(move || {
            // Check if we already have the record
            pool.get()?
                .prepare(&format!(
                    "SELECT
                        notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                    FROM notes
                    JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                    WHERE notes.note_commitment = x'{}'",
                    hex::encode(note_commitment.0.to_bytes())
                ))?
                .query_and_then((), |record| record.try_into())?
                .next()
                .transpose()
        })
        .await??
        {
            return Ok(record);
        }

        if !await_detection {
            anyhow::bail!("Note commitment {} not found", note_commitment);
        }

        // Otherwise, wait for newly detected notes and check whether they're
        // the requested one.

        loop {
            match rx.recv().await {
                Ok(record) => {
                    if record.note_commitment == note_commitment {
                        return Ok(record);
                    }
                }

                Err(e) => match e {
                    RecvError::Closed => {
                        anyhow::bail!(
                            "Receiver error during note detection: closed (no more active senders)"
                        );
                    }
                    RecvError::Lagged(count) => {
                        anyhow::bail!(
                            "Receiver error during note detection: lagged (by {:?} messages)",
                            count
                        );
                    }
                },
            };
        }
    }

    /// Query for a nullifier's status, optionally waiting until the nullifier is detected.
    pub async fn nullifier_status(
        &self,
        nullifier: Nullifier,
        await_detection: bool,
    ) -> anyhow::Result<bool> {
        // Start subscribing now, before querying for whether we already have the nullifier, so we
        // can't miss it if we race a write.
        let mut rx = self.scanned_nullifiers_tx.subscribe();

        // Clone the pool handle so that the returned future is 'static
        let pool = self.pool.clone();

        let nullifier_bytes = nullifier.0.to_bytes().to_vec();

        // Check if we already have the nullifier in the set of spent notes
        if let Some(height_spent) = spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT height_spent FROM spendable_notes WHERE nullifier = ?1")?
                .query_and_then([nullifier_bytes], |row| {
                    let height_spent: Option<u64> = row.get("height_spent")?;
                    anyhow::Ok(height_spent)
                })?
                .next()
                .transpose()
        })
        .await??
        {
            let spent = height_spent.is_some();

            // If we're awaiting detection and the nullifier isn't yet spent, don't return just yet
            if !await_detection || spent {
                return Ok(spent);
            }
        }

        // After checking the database, if we didn't find it, return `false` unless we are to
        // await detection
        if !await_detection {
            return Ok(false);
        }

        // Otherwise, wait for newly detected nullifiers and check whether they're the requested
        // one.
        loop {
            let new_nullifier = rx.recv().await.context("change subscriber failed")?;

            if new_nullifier == nullifier {
                return Ok(true);
            }
        }
    }

    /// The last block height we've scanned to, if any.
    pub async fn last_sync_height(&self) -> anyhow::Result<Option<u64>> {
        // Check if we have uncommitted blocks beyond the database height.
        if let Some(height) = *self.uncommitted_height.lock() {
            return Ok(Some(height.get()));
        }

        let pool = self.pool.clone();

        spawn_blocking(move || {
            let height: Option<i64> = pool
                .get()?
                .prepare_cached("SELECT height FROM sync_height ORDER BY height DESC LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<i64>>(0))?;

            anyhow::Ok(u64::try_from(height.ok_or_else(|| anyhow!("missing sync height"))?).ok())
        })
        .await?
    }

    pub async fn app_params(&self) -> anyhow::Result<AppParameters> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let params_bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'app_params' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing app_params in kv table"))?;

            AppParameters::decode(params_bytes.as_slice())
        })
        .await?
    }

    pub async fn gas_prices(&self) -> anyhow::Result<GasPrices> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'gas_prices' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing gas_prices in kv table"))?;

            GasPrices::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn fmd_parameters(&self) -> anyhow::Result<fmd::Parameters> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'fmd_params' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing fmd_params in kv table"))?;

            fmd::Parameters::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn full_viewing_key(&self) -> anyhow::Result<FullViewingKey> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k is 'fvk' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing fvk in kv table"))?;

            FullViewingKey::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn state_commitment_tree(&self) -> anyhow::Result<tct::Tree> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            tct::Tree::from_reader(&mut TreeStore(&mut pool.get()?.transaction()?))
        })
        .await?
    }

    /// Returns a tuple of (block height, transaction hash) for all transactions in a given range of block heights.
    pub async fn transaction_hashes(
        &self,
        start_height: Option<u64>,
        end_height: Option<u64>,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>)>> {
        let starting_block = start_height.unwrap_or(0) as i64;
        let ending_block = end_height.unwrap_or(self.last_sync_height().await?.unwrap_or(0)) as i64;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached(
                    "SELECT block_height, tx_hash
                    FROM tx
                    WHERE block_height BETWEEN ?1 AND ?2",
                )?
                .query_and_then([starting_block, ending_block], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    anyhow::Ok((block_height, tx_hash))
                })?
                .collect()
        })
        .await?
    }

    /// Returns a tuple of (block height, transaction hash, transaction) for all transactions in a given range of block heights.
    pub async fn transactions(
        &self,
        start_height: Option<u64>,
        end_height: Option<u64>,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>, Transaction)>> {
        let starting_block = start_height.unwrap_or(0) as i64;
        let ending_block = end_height.unwrap_or(self.last_sync_height().await?.unwrap_or(0)) as i64;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached(
                    "SELECT block_height, tx_hash, tx_bytes
                    FROM tx
                    WHERE block_height BETWEEN ?1 AND ?2",
                )?
                .query_and_then([starting_block, ending_block], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    let tx = Transaction::decode(tx_bytes.as_slice())?;
                    anyhow::Ok((block_height, tx_hash, tx))
                })?
                .collect()
        })
        .await?
    }

    pub async fn transaction_by_hash(
        &self,
        tx_hash: &[u8],
    ) -> anyhow::Result<Option<(u64, Transaction)>> {
        let pool = self.pool.clone();
        let tx_hash = tx_hash.to_vec();

        spawn_blocking(move || {
            if let Some((block_height, tx_bytes)) = pool
                .get()?
                .prepare_cached("SELECT block_height, tx_bytes FROM tx WHERE tx_hash = ?1")?
                .query_row([tx_hash], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    Ok((block_height, tx_bytes))
                })
                .optional()?
            {
                let tx = Transaction::decode(tx_bytes.as_slice())?;
                Ok(Some((block_height, tx)))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    // Query for a note by its note commitment, optionally waiting until the note is detected.
    pub async fn note_by_nullifier(
        &self,
        nullifier: Nullifier,
        await_detection: bool,
    ) -> anyhow::Result<SpendableNoteRecord> {
        // Start subscribing now, before querying for whether we already
        // have the record, so that we can't miss it if we race a write.
        let mut rx = self.scanned_notes_tx.subscribe();

        // Clone the pool handle so that the returned future is 'static
        let pool = self.pool.clone();

        let nullifier_bytes = nullifier.to_bytes().to_vec();

        if let Some(record) = spawn_blocking(move || {
            let record = pool
                .get()?
                .prepare(&format!(
                    "SELECT
                        notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                    FROM notes
                    JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                    WHERE hex(spendable_notes.nullifier) = \"{}\"",
                    hex::encode_upper(nullifier_bytes)
                ))?
                .query_and_then((), |row| SpendableNoteRecord::try_from(row))?
                .next()
                .transpose()?;

            anyhow::Ok(record)
        })
        .await??
        {
            return Ok(record);
        }

        if !await_detection {
            anyhow::bail!("Note commitment for nullifier {:?} not found", nullifier);
        }

        // Otherwise, wait for newly detected notes and check whether they're
        // the requested one.

        loop {
            match rx.recv().await {
                Ok(record) => {
                    if record.nullifier == nullifier {
                        return Ok(record);
                    }
                }

                Err(e) => match e {
                    RecvError::Closed => {
                        anyhow::bail!(
                            "Receiver error during note detection: closed (no more active senders)"
                        );
                    }
                    RecvError::Lagged(count) => {
                        anyhow::bail!(
                            "Receiver error during note detection: lagged (by {:?} messages)",
                            count
                        );
                    }
                },
            };
        }
    }

    pub async fn all_assets(&self) -> anyhow::Result<Vec<Metadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets")?
                .query_and_then([], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;

                    anyhow::Ok(denom_metadata)
                })?
                .collect()
        })
        .await?
    }

    pub async fn asset_by_id(&self, id: &Id) -> anyhow::Result<Option<Metadata>> {
        let id = id.to_bytes().to_vec();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets WHERE asset_id = ?1")?
                .query_and_then([id], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;
                    anyhow::Ok(denom_metadata)
                })?
                .next()
                .transpose()
        })
        .await?
    }

    // Get assets whose denoms match the given SQL LIKE pattern, with the `_` and `%` wildcards,
    // where `\` is the escape character.
    pub async fn assets_matching(&self, pattern: String) -> anyhow::Result<Vec<Metadata>> {
        let pattern = pattern.to_owned();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets WHERE denom LIKE ?1 ESCAPE '\\'")?
                .query_and_then([pattern], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;
                    anyhow::Ok(denom_metadata)
                })?
                .collect()
        })
        .await?
    }

    pub async fn notes(
        &self,
        include_spent: bool,
        asset_id: Option<asset::Id>,
        address_index: Option<penumbra_sdk_keys::keys::AddressIndex>,
        amount_to_spend: Option<Amount>,
    ) -> anyhow::Result<Vec<SpendableNoteRecord>> {
        // If set, return spent notes as well as unspent notes.
        // bool include_spent = 2;
        let spent_clause = match include_spent {
            false => "NULL",
            true => "height_spent",
        };

        // If set, only return notes with the specified asset id.
        // core.crypto.v1.AssetId asset_id = 3;
        let asset_clause = asset_id
            .map(|id| format!("x'{}'", hex::encode(id.to_bytes())))
            .unwrap_or_else(|| "asset_id".to_string());

        // If set, only return notes with the specified address index.
        // crypto.AddressIndex address_index = 4;
        // This isn't what we want any more, we need to be indexing notes
        // by *account*, not just by address index.
        // For now, just do filtering in software.
        /*
        let address_clause = address_index
            .map(|d| format!("x'{}'", hex::encode(d.to_bytes())))
            .unwrap_or_else(|| "address_index".to_string());
         */
        let address_clause = "address_index".to_string();

        // If set, stop returning notes once the total exceeds this amount.
        //
        // Ignored if `asset_id` is unset or if `include_spent` is set.
        // uint64 amount_to_spend = 5;
        //TODO: figure out a clever way to only return notes up to the sum using SQL
        let amount_cutoff = (amount_to_spend.is_some()) && !(include_spent || asset_id.is_none());
        let mut amount_total = Amount::zero();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut output: Vec<SpendableNoteRecord> = Vec::new();

            for result in pool
                .get()?
                .prepare(&format!(
                    "SELECT notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                FROM notes
                JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                WHERE spendable_notes.height_spent IS {spent_clause}
                AND notes.asset_id IS {asset_clause}
                AND spendable_notes.address_index IS {address_clause}"
                ))?
                .query_and_then((), |row| SpendableNoteRecord::try_from(row))?
            {
                let record = result?;

                // Skip notes that don't match the account, since we're
                // not doing account filtering in SQL as a temporary hack (see above)
                if let Some(address_index) = address_index {
                    if record.address_index.account != address_index.account {
                        continue;
                    }
                }
                let amount = record.note.amount();

                // Only display notes of value > 0

                if amount.value() > 0 {
                    output.push(record);
                }

                // If we're tracking amounts, accumulate the value of the note
                // and check if we should break out of the loop.
                if amount_cutoff {
                    // We know all the notes are of the same type, so adding raw quantities makes sense.
                    amount_total += amount;
                    if amount_total >= amount_to_spend.unwrap_or_default() {
                        break;
                    }
                }
            }

            if amount_total < amount_to_spend.unwrap_or_default() {
                anyhow::bail!(
                    "requested amount of {} exceeds total of {}",
                    amount_to_spend.unwrap_or_default(),
                    amount_total
                );
            }

            anyhow::Ok(output)
        })
        .await?
    }

    #[tracing::instrument(skip(self))]
    pub async fn record_asset(&self, asset: Metadata) -> anyhow::Result<()> {
        tracing::debug!(?asset);

        let asset_id = asset.id().to_bytes().to_vec();
        let denom = asset.base_denom().denom;
        let metadata_json = serde_json::to_string(&asset)?;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .execute(
                    "INSERT OR REPLACE INTO assets (asset_id, denom, metadata) VALUES (?1, ?2, ?3)",
                    (asset_id, denom, metadata_json),
                )
                .map_err(anyhow::Error::from)
        })
        .await??;

        Ok(())
    }

    pub async fn record_empty_block(&self, height: u64) -> anyhow::Result<()> {
        // Check that the incoming block height follows the latest recorded height
        let last_sync_height = self.last_sync_height().await?.ok_or_else(|| {
            anyhow::anyhow!("invalid: tried to record empty block as genesis block")
        })?;

        if height != last_sync_height + 1 {
            anyhow::bail!(
                "Wrong block height {} for latest sync height {}",
                height,
                last_sync_height
            );
        }

        *self.uncommitted_height.lock() = Some(height.try_into()?);
        Ok(())
    }

    fn record_note_inner(
        dbtx: &r2d2_sqlite::rusqlite::Transaction<'_>,
        note: &Note,
    ) -> anyhow::Result<()> {
        let note_commitment = note.commit().0.to_bytes().to_vec();
        let address = note.address().to_vec();
        let amount = u128::from(note.amount()).to_be_bytes().to_vec();
        let asset_id = note.asset_id().to_bytes().to_vec();
        let rseed = note.rseed().to_bytes().to_vec();

        dbtx.execute(
            "INSERT INTO notes (note_commitment, address, amount, asset_id, rseed)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT (note_commitment)
                DO UPDATE SET
                address = excluded.address,
                amount = excluded.amount,
                asset_id = excluded.asset_id,
                rseed = excluded.rseed",
            (note_commitment, address, amount, asset_id, rseed),
        )?;

        Ok(())
    }

    pub async fn give_advice(&self, note: Note) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let mut lock = pool.get()?;
        let dbtx = lock.transaction()?;

        Storage::record_note_inner(&dbtx, &note)?;

        dbtx.commit()?;

        Ok(())
    }

    /// Return advice about note contents for use in scanning.
    ///
    /// Given a list of note commitments, this method checks whether any of them
    /// correspond to notes that have been recorded in the database but not yet
    /// observed during scanning.
    pub async fn scan_advice(
        &self,
        note_commitments: Vec<note::StateCommitment>,
    ) -> anyhow::Result<BTreeMap<note::StateCommitment, Note>> {
        if note_commitments.is_empty() {
            return Ok(BTreeMap::new());
        }

        let pool = self.pool.clone();

        // This query gives advice about notes which are known but which have not already been recorded as spendable,
        // in part to avoid revealing information about which notes have been spent.

        spawn_blocking(move || {
            pool.get()?
                .prepare(&format!(
                    "SELECT notes.note_commitment,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed
                    FROM notes
                    LEFT OUTER JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    WHERE (spendable_notes.note_commitment IS NULL) AND (notes.note_commitment IN ({}))",
                    note_commitments
                        .iter()
                        .map(|cm| format!("x'{}'", hex::encode(cm.0.to_bytes())))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))?
                .query_and_then((), |row| {
                    let address = Address::try_from(row.get::<_, Vec<u8>>("address")?)?;
                    let amount = row.get::<_, [u8; 16]>("amount")?;
                    let amount_u128: u128 = u128::from_be_bytes(amount);
                    let asset_id = asset::Id(Fq::from_bytes_checked(&row.get::<_, [u8; 32]>("asset_id")?).expect("asset id malformed"));
                    let rseed = Rseed(row.get::<_, [u8; 32]>("rseed")?);
                    let note = Note::from_parts(
                        address,
                        Value {
                            amount: amount_u128.into(),
                            asset_id,
                        },
                        rseed,
                    )?;
                    anyhow::Ok((note.commit(), note))
                })?
                .collect::<anyhow::Result<BTreeMap<_, _>>>()
        }).await?
    }

    /// Filters for nullifiers whose notes we control
    pub async fn filter_nullifiers(
        &self,
        nullifiers: Vec<Nullifier>,
    ) -> anyhow::Result<Vec<Nullifier>> {
        if nullifiers.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare(&format!(
                    "SELECT nullifier FROM (SELECT nullifier FROM spendable_notes UNION SELECT nullifier FROM tx_by_nullifier) WHERE nullifier IN ({})",
                    nullifiers
                        .iter()
                        .map(|x| format!("x'{}'", hex::encode(x.0.to_bytes())))
                        .collect::<Vec<String>>()
                        .join(",")
                ))?
                .query_and_then((), |row| {
                    let nullifier: Vec<u8> = row.get("nullifier")?;
                    nullifier.as_slice().try_into()
                })?
                .collect()
        })
            .await?
    }

    pub async fn record_block(
        &self,
        filtered_block: FilteredBlock,
        transactions: Vec<Transaction>,
        sct: &mut tct::Tree,
        channel: tonic::transport::Channel,
    ) -> anyhow::Result<()> {
        //Check that the incoming block height follows the latest recorded height
        let last_sync_height = self.last_sync_height().await?;

        let correct_height = match last_sync_height {
            // Require that the new block follows the last one we scanned.
            Some(cur_height) => filtered_block.height == cur_height + 1,
            // Require that the new block represents the initial chain state.
            None => filtered_block.height == 0,
        };

        if !correct_height {
            anyhow::bail!(
                "Wrong block height {} for latest sync height {:?}",
                filtered_block.height,
                last_sync_height
            );
        }

        let pool = self.pool.clone();
        let uncommitted_height = self.uncommitted_height.clone();
        let scanned_notes_tx = self.scanned_notes_tx.clone();
        let scanned_nullifiers_tx = self.scanned_nullifiers_tx.clone();

        let fvk = self.full_viewing_key().await?;

        // If the app parameters have changed, update them.
        let new_app_parameters: Option<AppParameters> = if filtered_block.app_parameters_updated {
            // Fetch the latest parameters
            let mut client = AppQueryServiceClient::new(channel);
            Some(
                client
                    .app_parameters(tonic::Request::new(AppParametersRequest {}))
                    .await?
                    .into_inner()
                    .try_into()?,
            )
        } else {
            None
        };

        // Cloning the SCT is cheap because it's a copy-on-write structure, so we move an owned copy
        // into the spawned thread. This means that if for any reason the thread panics or throws an
        // error, the changes to the SCT will be discarded, just like any changes to the database,
        // so the two stay transactionally in sync, even in the case of errors. This would not be
        // the case if we `std::mem::take` the SCT and move it into the spawned thread, because then
        // an error would mean the updated version would never be put back, and the outcome would be
        // a cleared SCT but a non-empty database.
        let mut new_sct = sct.clone();

        *sct = spawn_blocking(move || {
            let mut lock = pool.get()?;
            let mut dbtx = lock.transaction()?;

            if let Some(params) = new_app_parameters {
                let params_bytes = params.encode_to_vec();
                // We expect app_params to be present already but may as well use an upsert
                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('app_params', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&params_bytes[..]],
                )?;
            }

            // Insert new note records into storage
            for note_record in filtered_block.new_notes.values() {
                let note_commitment = note_record.note_commitment.0.to_bytes().to_vec();
                let height_created = filtered_block.height as i64;
                let address_index = note_record.address_index.to_bytes().to_vec();
                let nullifier = note_record.nullifier.to_bytes().to_vec();
                let position = (u64::from(note_record.position)) as i64;
                let source = note_record.source.encode_to_vec();
                // Check if the note is from a transaction, if so, include the tx hash (id)
                let tx_hash = match note_record.source {
                    CommitmentSource::Transaction { id } => id,
                    _ => None,
                };

                // Record the inner note data in the notes table
                Storage::record_note_inner(&dbtx, &note_record.note)?;

                dbtx.execute(
                    "INSERT INTO spendable_notes
                    (note_commitment, nullifier, position, height_created, address_index, source, height_spent, tx_hash)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)
                    ON CONFLICT (note_commitment)
                    DO UPDATE SET nullifier = excluded.nullifier,
                    position = excluded.position,
                    height_created = excluded.height_created,
                    address_index = excluded.address_index,
                    source = excluded.source,
                    height_spent = excluded.height_spent,
                    tx_hash = excluded.tx_hash",
                    (
                        &note_commitment,
                        &nullifier,
                        &position,
                        &height_created,
                        &address_index,
                        &source,
                        // height_spent is NULL because the note is newly discovered
                        &tx_hash,
                    ),
                )?;
            }

            // Update any rows of the table with matching nullifiers to have height_spent
            for nullifier in &filtered_block.spent_nullifiers {
                let height_spent = filtered_block.height as i64;
                let nullifier_bytes = nullifier.to_bytes().to_vec();

                let spent_commitment: Option<StateCommitment> = dbtx.prepare_cached(
                    "UPDATE spendable_notes SET height_spent = ?1 WHERE nullifier = ?2 RETURNING note_commitment"
                )?
                    .query_and_then(
                        (height_spent, &nullifier_bytes),
                        |row| {
                            let bytes: Vec<u8> = row.get("note_commitment")?;
                            StateCommitment::try_from(&bytes[..]).context("invalid commitment bytes")
                        },
                    )?
                    .next()
                    .transpose()?;

                // Mark spent notes as spent
                if let Some(spent_commitment) = spent_commitment {
                    tracing::debug!(?nullifier, ?spent_commitment, "detected spent note commitment");
                    tracing::debug!(?nullifier, ?spent_commitment, "forgetting spent note commitment");
                    new_sct.forget(spent_commitment);
                };
            }

            // Update SCT table with current SCT state
            new_sct.to_writer(&mut TreeStore(&mut dbtx))?;

            // Record all transactions
            for transaction in transactions {
                let tx_bytes = transaction.encode_to_vec();
                // We have to create an explicit temporary borrow, because the sqlx api is bad (see above)
                let tx_hash_owned = sha2::Sha256::digest(&tx_bytes);
                let tx_hash = tx_hash_owned.as_slice();
                let tx_block_height = filtered_block.height as i64;
                let decrypted_memo = transaction.decrypt_memo(&fvk).ok();
                let memo_text = decrypted_memo.clone().map_or(None,|x| Some(x.text().to_string()));
                let return_address = decrypted_memo.map_or(None, |x| Some(x.return_address().to_vec()));

                tracing::debug!(tx_hash = ?hex::encode(tx_hash), "recording extended transaction");

                dbtx.execute(
                    "INSERT OR IGNORE INTO tx (tx_hash, tx_bytes, block_height, return_address, memo_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (&tx_hash, &tx_bytes, tx_block_height, return_address, memo_text),
                )?;

                // Associate all of the spent nullifiers with the transaction by hash.
                for nf in transaction.spent_nullifiers() {
                    let nf_bytes = nf.0.to_bytes().to_vec();
                    dbtx.execute(
                        "INSERT OR IGNORE INTO tx_by_nullifier (nullifier, tx_hash) VALUES (?1, ?2)",
                        (&nf_bytes, &tx_hash),
                    )?;
                }
            }

            // Update FMD parameters if they've changed.
            if filtered_block.fmd_parameters.is_some() {
                let fmd_parameters_bytes =
                    &fmd::Parameters::encode_to_vec(&filtered_block.fmd_parameters.ok_or_else(|| anyhow::anyhow!("missing fmd parameters in filtered block"))?)[..];

                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('fmd_params', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&fmd_parameters_bytes],
                )?;
            }

            // Update gas prices if they've changed.
            if filtered_block.gas_prices.is_some() {
                let gas_prices_bytes =
                    &GasPrices::encode_to_vec(&filtered_block.gas_prices.ok_or_else(|| anyhow::anyhow!("missing gas prices in filtered block"))?)[..];

                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('gas_prices', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&gas_prices_bytes],
                )?;
            }

            // Record block height as latest synced height
            let latest_sync_height = filtered_block.height as i64;
            dbtx.execute("UPDATE sync_height SET height = ?1", [latest_sync_height])?;

            // Commit the changes to the database
            dbtx.commit()?;

            // IMPORTANT: NO PANICS OR ERRORS PAST THIS POINT
            // If there is a panic or error past this point, the database will be left in out of
            // sync with the in-memory copy of the SCT, which means that it will become corrupted as
            // synchronization continues.

            // It's critical to reset the uncommitted height here, since we've just
            // invalidated it by committing.
            uncommitted_height.lock().take();

            // Broadcast all committed note records to channel
            // Done following tx.commit() to avoid notifying of a new SpendableNoteRecord before it is actually committed to the database

            for note_record in filtered_block.new_notes.values() {
                // This will fail to be broadcast if there is no active receiver (such as on initial
                // sync) The error is ignored, as this isn't a problem, because if there is no
                // active receiver there is nothing to do
                let _ = scanned_notes_tx.send(note_record.clone());
            }

            for nullifier in filtered_block.spent_nullifiers.iter() {
                // This will fail to be broadcast if there is no active receiver (such as on initial
                // sync) The error is ignored, as this isn't a problem, because if there is no
                // active receiver there is nothing to do
                let _ = scanned_nullifiers_tx.send(*nullifier);
            }

            anyhow::Ok(new_sct)
        })
            .await??;

        Ok(())
    }

    pub async fn notes_by_sender(
        &self,
        return_address: &Address,
    ) -> anyhow::Result<Vec<SpendableNoteRecord>> {
        let pool = self.pool.clone();

        let query = "SELECT notes.note_commitment,
            spendable_notes.height_created,
            notes.address,
            notes.amount,
            notes.asset_id,
            notes.rseed,
            spendable_notes.address_index,
            spendable_notes.source,
            spendable_notes.height_spent,
            spendable_notes.nullifier,
            spendable_notes.position
            FROM notes
            JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
            JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
            WHERE tx.return_address = ?1";

        let return_address = return_address.to_vec();

        let records = spawn_blocking(move || {
            pool.get()?
                .prepare(query)?
                .query_and_then([return_address], |record| record.try_into())?
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .await??;

        Ok(records)
    }

    /// Get all transactions with a matching memo text. The `pattern` argument
    /// should include SQL wildcards, such as `%` and `_`, to match substrings,
    /// e.g. `%foo%`.
    pub async fn transactions_matching_memo(
        &self,
        pattern: String,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>, Transaction, String)>> {
        let pattern = pattern.to_owned();
        tracing::trace!(?pattern, "searching for memos matching");
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT block_height, tx_hash, tx_bytes, memo_text FROM tx WHERE memo_text LIKE ?1 ESCAPE '\\'")?
                .query_and_then([pattern], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    let tx = Transaction::decode(tx_bytes.as_slice())?;
                    let memo_text: String = row.get("memo_text")?;
                    anyhow::Ok((block_height, tx_hash, tx, memo_text))
                })?
                .collect()
        })
        .await?
    }

    /// Update information about an epoch.
    pub async fn update_epoch(
        &self,
        epoch: u64,
        root: Option<Root>,
        start_height: Option<u64>,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .execute(
                    r#"
                    INSERT INTO epochs(epoch_index, root, start_height)
                    VALUES (?1, ?2, ?3)
                    ON CONFLICT(epoch_index)
                    DO UPDATE SET
                        root = COALESCE(?2, root),
                        start_height = COALESCE(?3, start_height)
                    "#,
                    (epoch, root.map(|x| x.encode_to_vec()), start_height),
                )
                .map_err(anyhow::Error::from)
        })
        .await??;

        Ok(())
    }

    /// Fetch information about the current epoch.
    ///
    /// This will return the root of the epoch, if present,
    /// and the start height of the epoch, if present.
    pub async fn get_epoch(&self, epoch: u64) -> anyhow::Result<(Option<Root>, Option<u64>)> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .query_row_and_then(
                    r#"
                    SELECT root, start_height
                    FROM epochs
                    WHERE epoch_index = ?1
                    "#,
                    (epoch,),
                    |row| {
                        let root_raw: Option<Vec<u8>> = row.get("root")?;
                        let start_height: Option<u64> = row.get("start_height")?;
                        let root = root_raw.map(|x| Root::decode(x.as_slice())).transpose()?;
                        anyhow::Ok((root, start_height))
                    },
                )
                .map_err(anyhow::Error::from)
        })
        .await?
    }

    /// Load the compliance user tree from storage.
    pub async fn compliance_user_tree(
        &self,
    ) -> anyhow::Result<crate::compliance_tree::ComplianceUserTree> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let mut store = compliance::ComplianceTreeStore(&mut tx);
            crate::compliance_tree::ComplianceUserTree::from_store(&mut store)
        })
        .await?
    }

    /// Load the compliance asset tree from storage.
    pub async fn compliance_asset_tree(
        &self,
    ) -> anyhow::Result<crate::compliance_tree::ComplianceAssetTree> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let mut store = compliance::ComplianceTreeStore(&mut tx);
            crate::compliance_tree::ComplianceAssetTree::from_store(&mut store)
        })
        .await?
    }

    /// Record compliance tree changes for a block.
    pub async fn record_compliance_block(
        &self,
        height: u64,
        user_tree: &crate::compliance_tree::ComplianceUserTree,
        asset_tree: &crate::compliance_tree::ComplianceAssetTree,
        user_start_position: u64,
        asset_start_position: u64,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let user_root = user_tree.root();
        let asset_root = asset_tree.root();

        // Clone tree state for persistence
        let user_tree = user_tree.clone();
        let asset_tree = asset_tree.clone();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;

            {
                let mut store = compliance::ComplianceTreeStore(&mut tx);

                // Persist user tree changes
                user_tree.persist(&mut store, user_start_position)?;

                // Persist asset tree changes
                asset_tree.persist(&mut store, asset_start_position)?;

                // Store anchors for this block
                store.add_anchor(height, user_root, asset_root)?;
            }

            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Record compliance leaf data for an address in the sync scope.
    pub async fn record_compliance_leaf_data(
        &self,
        address: &penumbra_sdk_keys::Address,
        asset_id: &asset::Id,
        position: u64,
        ack: &[u8],
        ack_orbis: &[u8],
        commitment: StateCommitment,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();
        let asset_bytes = asset_id.to_bytes().to_vec();
        let ack_bytes = ack.to_vec();
        let ack_orbis_bytes = ack_orbis.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.add_leaf_data(
                    &address_bytes,
                    &asset_bytes,
                    position,
                    &ack_bytes,
                    &ack_orbis_bytes,
                    commitment,
                )?;
            }
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Record a counterparty address for tracking.
    pub async fn record_counterparty(
        &self,
        address: &penumbra_sdk_keys::Address,
        height: u64,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.add_counterparty(&address_bytes, height)?;
            }
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Check if an address is in the compliance sync scope (own or counterparty).
    pub async fn is_address_in_compliance_scope(
        &self,
        fvk: &FullViewingKey,
        address: &penumbra_sdk_keys::Address,
    ) -> anyhow::Result<bool> {
        // First check if it's one of our own addresses
        if fvk.address_index(address).is_some() {
            return Ok(true);
        }

        // Otherwise check if it's a tracked counterparty
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let result = {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.is_counterparty(&address_bytes)?
            };
            Ok::<bool, anyhow::Error>(result)
        })
        .await?
    }

    /// Get compliance leaf data for an address and asset from local storage.
    ///
    /// Returns (position, ack_bytes, ack_orbis_bytes, commitment) if available, None if not in scope.
    pub async fn get_compliance_leaf_data(
        &self,
        address: &penumbra_sdk_keys::Address,
        asset_id: &asset::Id,
    ) -> anyhow::Result<Option<(u64, [u8; 32], [u8; 32], StateCommitment)>> {
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();
        let asset_bytes = asset_id.to_bytes().to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let result = {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.get_leaf_data(&address_bytes, &asset_bytes)?
            };
            // Convert Vec<u8> to [u8; 32] if present
            let converted = result
                .map(
                    |(pos, ack_vec, ack_orbis_vec, commitment)| -> anyhow::Result<_> {
                        let ack_bytes: [u8; 32] = ack_vec.try_into().map_err(|v: Vec<u8>| {
                            anyhow::anyhow!("ACK must be 32 bytes, got {}", v.len())
                        })?;
                        let ack_orbis_bytes: [u8; 32] =
                            ack_orbis_vec.try_into().map_err(|v: Vec<u8>| {
                                anyhow::anyhow!("ACK_orbis must be 32 bytes, got {}", v.len())
                            })?;
                        Ok((pos, ack_bytes, ack_orbis_bytes, commitment))
                    },
                )
                .transpose()?;
            Ok::<Option<(u64, [u8; 32], [u8; 32], StateCommitment)>, anyhow::Error>(converted)
        })
        .await?
    }

    /// Store an asset policy (threshold and DK_pub).
    pub async fn store_asset_policy(
        &self,
        asset_id: &asset::Id,
        dk_pub: &decaf377::Element,
        threshold: u128,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let asset_bytes = asset_id.to_bytes().to_vec();
        let dk_pub_bytes = dk_pub.vartime_compress().0.to_vec();
        let threshold_bytes = threshold.to_le_bytes().to_vec();

        spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO compliance_asset_policies (asset_id, dk_pub, threshold) VALUES (?1, ?2, ?3)",
                (asset_bytes.as_slice(), dk_pub_bytes.as_slice(), threshold_bytes.as_slice()),
            )?;
            Ok::<(), anyhow::Error>(())
        })
        .await?
    }

    /// Get an asset policy (threshold and DK_pub) if one exists.
    pub async fn get_asset_policy(
        &self,
        asset_id: &asset::Id,
    ) -> anyhow::Result<Option<penumbra_sdk_compliance::structs::AssetPolicy>> {
        let pool = self.pool.clone();
        let asset_bytes = asset_id.to_bytes().to_vec();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let result: Option<(Vec<u8>, Vec<u8>)> = conn
                .query_row(
                    "SELECT dk_pub, threshold FROM compliance_asset_policies WHERE asset_id = ?1",
                    [asset_bytes.as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            match result {
                Some((dk_pub_bytes, threshold_bytes)) => {
                    let dk_pub_arr: [u8; 32] = dk_pub_bytes.try_into().map_err(|v: Vec<u8>| {
                        anyhow::anyhow!("dk_pub must be 32 bytes, got {}", v.len())
                    })?;
                    let dk_pub = decaf377::Encoding(dk_pub_arr)
                        .vartime_decompress()
                        .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?;
                    let threshold_arr: [u8; 16] =
                        threshold_bytes.try_into().map_err(|v: Vec<u8>| {
                            anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
                        })?;
                    let threshold = u128::from_le_bytes(threshold_arr);
                    Ok(Some(penumbra_sdk_compliance::structs::AssetPolicy::new(
                        dk_pub,
                        threshold,
                        vec![],
                        String::new(),
                        decaf377::Element::default(),
                        String::new(),
                        String::new(),
                        String::new(),
                    )))
                }
                None => Ok(None),
            }
        })
        .await?
    }
}
