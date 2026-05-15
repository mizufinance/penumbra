# Orbis Runtime Contract

Penumbra vendors the Orbis integration runtime contract here so local and CI
flows do not depend on a manually maintained neighboring `orbis-rs` checkout or
on `cli-tool`.

Current contract line:

- Orbis source ref: pinned in [crates/util/orbis-client/Cargo.toml](../../crates/util/orbis-client/Cargo.toml) (`orbis-rs` git revs). `scripts/lib/common.sh::orbis_pinned_rev_from_cargo` extracts this and feeds it to the Docker build context, so Cargo.toml is the single source of truth.
- Crypto feature: `decaf377`
- SourceHub image: pinned by digest in [docker-compose.yml](docker-compose.yml) and as `SOURCEHUB_IMAGE_DEFAULT` in [scripts/lib/common.sh](../../scripts/lib/common.sh). Override with `SOURCEHUB_IMAGE=<image>` for ad-hoc testing.

`./scripts/orbis-stack.sh up` prepares a local checkout of the pinned upstream
`orbis-rs` ref under `tmp/orbis-rs` and points Docker Compose at that local
build context. This avoids Docker BuildKit incompatibilities with remote git
contexts on older CI runners while keeping Penumbra's supported runtime pinned
in repo.
