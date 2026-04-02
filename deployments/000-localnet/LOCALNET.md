# Local 2-Validator Penumbra Network

## Quick Start

```bash
# From the repo root, start the 2-validator network
docker compose -f deployments/compose/docker-compose.yml up -d

# View logs
docker compose -f deployments/compose/docker-compose.yml logs -f

# Reset and start fresh
docker compose -f deployments/compose/docker-compose.yml down -v
docker compose -f deployments/compose/docker-compose.yml up -d
```

## Verified Working Configuration

### Genesis Account

**Address:**
```
penumbra19lf99grquvn8yufxswy9yasux5xhew2adjefmwdk7pnnd6za6ardtce0d9z7suzwz94cjclz6rwqqn5qeta4pn4j7y59k38za5yhmkt66tngw3txh90rm0vrhelx68lcsk9yqk
```

**Mnemonic:**
```
decorate bright ozone fork gallery riot bus exhaust worth way bone indoor calm squirrel merry zero scheme cotton until shop any excess stage laundry
```

### Verified Genesis Allocations

| Token | Amount |
|-------|--------|
| penumbra | 1,000,000 |
| gm | 20,000 |
| gn | 20,000 |
| test_usd | 500,000 |

### Validators

| Name | Status | Voting Power |
|------|--------|--------------|
| Local Validator 1 | Active, Bonded | 50% |
| Local Validator 2 | Active, Bonded | 50% |

### Network Parameters

| Parameter | Value |
|-----------|-------|
| Chain ID | penumbra-local-devnet |
| Validators | 2 |
| Epoch Duration | 100 blocks |
| Unbonding Delay | 100 blocks |
| Block Time | ~500ms |
| Gas Price | 0 |

## Exposed Ports

### Validator 0

| Service | Port |
|---------|------|
| gRPC | 8080 |
| ABCI | 26658 |
| CometBFT P2P | 26656 |
| CometBFT RPC | 26657 |

### Validator 1

| Service | Port |
|---------|------|
| gRPC | 8081 |
| ABCI | 26668 |
| CometBFT P2P | 26666 |
| CometBFT RPC | 26667 |

## Connecting with pcli

```bash
# Initialize wallet with the genesis mnemonic
mkdir -p ~/.local/share/pcli-localnet
pcli --home ~/.local/share/pcli-localnet init --grpc-url http://localhost:8080 soft-kms import-phrase <<EOF
decorate bright ozone fork gallery riot bus exhaust worth way bone indoor calm squirrel merry zero scheme cotton until shop any excess stage laundry
EOF

# Sync and check balance
pcli --home ~/.local/share/pcli-localnet view sync
pcli --home ~/.local/share/pcli-localnet view balance

# List validators
pcli --home ~/.local/share/pcli-localnet query validator list
```

## Files

- `deployments/000-localnet/validators.json` - 2 validator definitions
- `deployments/000-localnet/allocations.csv` - Genesis allocations
- `deployments/compose/docker-compose.yml` - Docker Compose config

---

## Improvements for Mainnet Deployment

### 1. Folder Structure (Current)

```
deployments/
├── 000-localnet/
│   ├── validators.json
│   └── allocations.csv
└── compose/
    └── docker-compose.yml
```

**Future:** Add `deployments/mainnet/` for mainnet genesis configs.

### 2. Allocation File Improvements

- Add validation for allocation amounts (ensure they sum correctly for tokenomics)
- Add support for vesting schedules in allocations
- Consider a TOML or JSON format for better readability vs CSV

### 3. Validator Configuration

- Add support for validator key generation with secure key management
- Include validator funding stream configuration in JSON
- Add commission rate configuration
- Support for validator metadata (logo, description, website)

### 4. Docker Compose Improvements

- Add healthchecks for pd and cometbft services
- Add prometheus/grafana for monitoring
- Support for log aggregation
- Add volume backup/restore scripts

### 5. Genesis Generation

- Create a `pd network generate` wrapper script that:
  - Validates allocations
  - Generates validator keys securely
  - Creates backup of keys
  - Outputs genesis summary

### 6. Documentation

- Add operator runbook
- Document backup/recovery procedures
- Add monitoring setup guide
- Create validator onboarding guide

### 7. Security Considerations

- Use Docker secrets for sensitive data
- Add firewall rules documentation
- Document key management best practices
- Add HTTPS/TLS configuration for gRPC

### 8. Staking Token Customization

The staking token (upenumbra) is hardcoded. To customize:

1. Edit `crates/core/asset/src/lib.rs` - change `"upenumbra"` lookup
2. Edit `crates/core/asset/src/asset/registry.rs` - add new asset
3. Rebuild: `cargo build --release -p pd`

---

## Troubleshooting

### "database already initialized" panic

This happens when cometbft restarts while pd is still running. Fix:
```bash
docker compose -f deployments/compose/docker-compose.yml down -v
docker compose -f deployments/compose/docker-compose.yml up -d
```

### Peer resolution failures

Both cometbft nodes need to resolve each other's hostnames. The docker-compose uses network aliases (`validator-0`, `validator-1`) for this. Ensure both nodes depend on both pd nodes so they start together.

### Balance not showing

After starting the network, sync your wallet:
```bash
pcli --home ~/.local/share/pcli-localnet view sync
```
