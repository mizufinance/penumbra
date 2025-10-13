Summary

  I've created a new CLI command pcli view asset-viewing-key (alias: avk) that allows you to export an asset-specific viewing key for scanning by
  specific asset.

  Key Features:

  1. AssetViewingKey Implementation (crates/core/keys/src/keys/asset_viewing_key.rs:89-154):
    - Serializes to/from 80 bytes (32 bytes asset ID + 32 bytes IVK + 16 bytes diversifier key)
    - Bech32m encoding with prefix penumbraassetviewingkey
    - Can decrypt notes at ANY address, but filtered to one asset
    - Preserves full privacy for other assets
  2. CLI Command (crates/bin/pcli/src/command/view/asset_viewing_key.rs):
    - Usage: pcli view asset-viewing-key --asset-id <ASSET_ID>
    - Accepts either:
        - Bech32m-encoded asset ID (starting with passet)
      - Raw denomination string (e.g., "upenumbra", "usdc")
    - Shows the derived asset ID and the asset viewing key
  3. Asset ID Derivation:
  To derive an asset ID from a denomination like "upenumbra":
    - It uses BLAKE2b with personalization string "Penumbra_AssetID"
    - The hash is converted to a field element
    - You can use either format:
        - --asset-id upenumbra (raw denomination - will be hashed)
      - --asset-id passet1... (already hashed bech32m format)

  Example Usage:

  # Install latest & run testnet
  ```
  just container
  cargo build --release -p pcli && cp target/release/pcli `which pcli`

  pd network unsafe-reset-all
  rm -rf ~/.penumbra
  rm -rf ~/.local/share/local0
  rm -rf ~/.local/share/local1
  just testnet
  ```

  ```bash
  # penumbra1eu5pnv6qptp2p0aevfc0adjrpd24glz5shey7wvwlg5sp3ffca2zk32cemjd90ughdh7xqplrej9lqzc06337w2scxykjajd2nrtttvmqt6tssr6pmzp283hhte7y4jf6sn2wh
  echo 'test test test test test test test test test test test junk' | pcli --home ~/.local/share/local0 init --grpc-url http://localhost:8080 soft-kms import-phrase
  ADDRESS0=$(pcli --home ~/.local/share/local0 view address); echo "ADDRESS0: $ADDRESS0"

  # used as the relayer
  echo 'rhythm marine super pact sketch burden link uncover alert hip fossil board' | pcli --home ~/.local/share/local1 init --grpc-url http://localhost:8080 soft-kms import-phrase
  ADDRESS1=$(pcli --home ~/.local/share/local1 view address); echo "ADDRESS1: $ADDRESS1"
  ```

  # Using raw denomination
  ```bash
  pcli --home ~/.local/share/local0 view balance
  pcli --home ~/.local/share/local1 view asset-viewing-key --asset-id test_usd
  ASSET_VIEWING_KEY_ACC1=penumbraassetviewingkey15xa8edy2ly97qp2mv6kwchpyvawxkxycsgmsrqagvr58l66n8qxa936f2engzk0tp4alv7pewr2tsckmjrzdl2c0euntkm96m6efgqz093n4sqesd86zced8r7mhkmc6yh0g0h

  # send test_usd from ADDR0 (local0) to ADDR1
  pcli --home ~/.local/share/local0 tx send --to $ADDRESS1 1test_usd
  pcli --home ~/.local/share/local0 tx send --to $ADDRESS1 1penumbra
  pcli --home ~/.local/share/local0 view balance
  ```

  # Query the transfer of the test_usd from acc0 viewing key provided
  ```bash
  pcli --home ~/.local/share/testviewingkey init --grpc-url http://localhost:8080 soft-kms generate

  pcli --home ~/.local/share/local1 view balance

  # this is not the right local account on PURPOSE (just so it's compatible so I can test it)
  pcli --home ~/.local/share/testviewingkey view balance --asset-viewing-key $ASSET_VIEWING_KEY_ACC1
  ```

  Architecture:

  The AssetViewingKey contains:
  - The full IVK (incoming viewing key) from the FVK
  - The specific asset ID to filter on
  - Can decrypt notes at any address (same as FVK)
  - Only reveals notes matching the specified asset_id

  This is perfect for compliance scenarios where you need to prove holdings/transactions for one asset without revealing other holdings!
