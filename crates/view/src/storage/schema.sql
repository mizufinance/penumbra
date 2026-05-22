-- The hash of this schema file
CREATE TABLE schema_hash (schema_hash TEXT NOT NULL);

-- The client version that created this database
CREATE TABLE client_version (client_version TEXT NOT NULL);

-- General-purpose blob storage
CREATE TABLE kv (
    k                       TEXT PRIMARY KEY NOT NULL,
    v                       BLOB NOT NULL
);

CREATE TABLE sync_height (height BIGINT NOT NULL);

-- used for storing a cache of known assets
CREATE TABLE assets (
    asset_id                BLOB PRIMARY KEY NOT NULL,
    denom                   TEXT NOT NULL,
    metadata                TEXT NOT NULL
);

-- the shape information about the sct
CREATE TABLE sct_position ( position BIGINT );
INSERT INTO sct_position VALUES ( 0 ); -- starting position is 0

CREATE TABLE sct_forgotten ( forgotten BIGINT NOT NULL );
INSERT INTO sct_forgotten VALUES ( 0 ); -- starting forgotten version is 0

-- the hashes for nodes in the sct
CREATE TABLE sct_hashes (
    position BIGINT NOT NULL,
    height   TINYINT NOT NULL,
    hash     BLOB NOT NULL
);

-- these indices may help with 2-dimensional range deletion
CREATE INDEX hash_position_idx ON sct_hashes ( position );
--CREATE INDEX hash_height_idx ON sct_hashes ( height );

-- all the commitments stored in the sct
CREATE TABLE sct_commitments (
    position BIGINT NOT NULL,
    commitment BLOB NOT NULL
);

-- look up transaction hashes by nullifier
CREATE TABLE tx_by_nullifier (
    nullifier               BLOB PRIMARY KEY NOT NULL,
    tx_hash                 BLOB NOT NULL
);

-- list of all known relevant transactions
CREATE TABLE tx (
    tx_hash                 BLOB PRIMARY KEY NOT NULL,
    tx_bytes                BLOB NOT NULL,
    block_height            BIGINT NOT NULL,
    return_address          BLOB,
    memo_text               TEXT
);

-- This table just records the mapping from note commitments to note plaintexts.
-- This is also used as a way to give advice about out-of-band notes during scanning,
-- by allowing the user to add notes to the database before they are scanned.
CREATE TABLE notes (
    note_commitment         BLOB PRIMARY KEY NOT NULL,
    address                 BLOB NOT NULL,
    amount                  BLOB NOT NULL,
    asset_id                BLOB NOT NULL,
    rseed                   BLOB NOT NULL
);

-- general purpose note queries
CREATE INDEX notes_idx ON notes (
    address,
    asset_id,
    amount
);

-- Minimal data required for balance tracking
-- Meant to represent notes which have been accepted into the note set
CREATE TABLE spendable_notes (
    note_commitment         BLOB PRIMARY KEY NOT NULL,
    -- the nullifier for this note, used to detect when it is spent
    nullifier               BLOB NOT NULL,
    -- the position of the note in the state commitment tree
    position                BIGINT NOT NULL,
    -- the height at which the note was created
    height_created          BIGINT NOT NULL,
    -- precomputed decryption of the diversifier
    address_index           BLOB NOT NULL,
    -- the source of the note (a tx hash or structured data jammed into one)
    source                  BLOB NOT NULL,
    -- null if unspent, otherwise spent at height_spent
    height_spent            BIGINT,
    -- null if note source is not a transaction, otherwise the tx hash
    tx_hash                 BLOB
);

CREATE INDEX spendable_notes_by_nullifier_idx ON spendable_notes (
    nullifier
);

CREATE INDEX spendable_notes_by_source_idx ON spendable_notes (
    source
);

-- general purpose note queries
CREATE INDEX spendable_notes_idx ON spendable_notes (
    address_index,
    height_created,
    height_spent       -- null if unspent, so spent/unspent is first
);

CREATE TABLE epochs (
    epoch_index BIGINT PRIMARY KEY,
    root BLOB,
    start_height BIGINT
);

-- ========== Compliance Trees (Local Sync) ==========

-- User compliance tree positions and commitments
CREATE TABLE compliance_user_positions (
    position BIGINT PRIMARY KEY,
    commitment BLOB NOT NULL
);

-- Internal hashes for user tree auth paths
CREATE TABLE compliance_user_hashes (
    position BIGINT NOT NULL,
    height TINYINT NOT NULL,
    hash BLOB NOT NULL,
    PRIMARY KEY (position, height)
);

-- Asset tree (IMT) indexed leaves (full policy for correct tree reconstruction)
CREATE TABLE compliance_asset_leaves (
    position BIGINT PRIMARY KEY,
    value BLOB NOT NULL,
    next_index BIGINT NOT NULL,
    next_value BLOB NOT NULL,
    dk_pub BLOB NOT NULL,          -- 32 bytes compressed curve point
    threshold BLOB NOT NULL,       -- 16 bytes little-endian u128
    slot_count BIGINT NOT NULL,
    route_policy_hash BLOB NOT NULL,   -- 32 bytes Fq
    ring_pk BLOB NOT NULL,         -- 32 bytes compressed curve point
    ring_id_hash BLOB NOT NULL,    -- 32 bytes Fq
    policy_id_hash BLOB NOT NULL,  -- 32 bytes Fq
    permission_hash BLOB NOT NULL, -- 32 bytes Fq
    resource_hash BLOB NOT NULL    -- 32 bytes Fq
);

-- Internal hashes for asset tree auth paths
CREATE TABLE compliance_asset_hashes (
    position BIGINT NOT NULL,
    height TINYINT NOT NULL,
    hash BLOB NOT NULL,
    PRIMARY KEY (position, height)
);

-- Compliance tree anchors per block
CREATE TABLE compliance_anchors (
    height BIGINT PRIMARY KEY,
    user_root BLOB NOT NULL,
    asset_root BLOB NOT NULL
);
-- Index for efficient "latest anchor" queries (ORDER BY height DESC)
CREATE INDEX compliance_anchors_height_desc ON compliance_anchors(height DESC);

-- Full compliance leaf data (for addresses in sync scope)
CREATE TABLE compliance_user_leaf_data (
    address BLOB NOT NULL,
    asset_id BLOB NOT NULL,
    position BIGINT NOT NULL,
    slot_id BIGINT NOT NULL,
    slot_derivation BLOB NOT NULL,     -- 32 bytes Fq
    d BLOB NOT NULL,                   -- 32 bytes Fq
    commitment BLOB NOT NULL,
    PRIMARY KEY (address, asset_id)
);
CREATE INDEX compliance_user_leaf_data_position ON compliance_user_leaf_data(position);

-- Tracked counterparty addresses (sparse sync scope)
CREATE TABLE compliance_counterparties (
    address BLOB PRIMARY KEY,
    first_seen_height BIGINT NOT NULL,
    last_tx_height BIGINT
);

-- Tree position cursors (for reconstruction on load)
CREATE TABLE compliance_user_tree_position (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    position BIGINT NOT NULL
);
INSERT INTO compliance_user_tree_position VALUES (0, 0);

CREATE TABLE compliance_asset_tree_position (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    leaf_count BIGINT NOT NULL
);
INSERT INTO compliance_asset_tree_position VALUES (0, 1); -- Starts with sentinel

-- Full asset policies used by compliance planning and Orbis upload packaging.
CREATE TABLE compliance_asset_policies (
    asset_id BLOB PRIMARY KEY,
    policy BLOB NOT NULL
);
