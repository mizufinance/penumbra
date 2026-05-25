-- A table containing updates to the total supply, and market cap.
CREATE TABLE IF NOT EXISTS insights_supply (
    -- The height where the supply was updated.
    height BIGINT PRIMARY KEY,
    -- The total supply of the staking token at this height.
    total BIGINT NOT NULL
);

-- Our internal representation of the shielded pool table.
CREATE TABLE IF NOT EXISTS insights_shielded_pool (
    -- The asset this concerns.
    asset_id BYTEA NOT NULL,
    height BIGINT NOT NULL,
    -- The total value shielded, in terms of that asset.
    total_value TEXT NOT NULL,
    -- The current value shielded, in terms of that asset.
    current_value TEXT NOT NULL,
    -- The number of unique depositors.
    unique_depositors INT NOT NULL,
    PRIMARY KEY (asset_id, height)
);

-- Unique depositors into the shielded pool
CREATE TABLE IF NOT EXISTS _insights_shielded_pool_depositors (
    asset_id BYTEA NOT NULL,
    address TEXT NOT NULL,
    PRIMARY KEY (asset_id, address)
);

CREATE OR REPLACE VIEW insights_shielded_pool_latest AS
    SELECT DISTINCT ON (asset_id) * FROM insights_shielded_pool ORDER BY asset_id, height DESC;
