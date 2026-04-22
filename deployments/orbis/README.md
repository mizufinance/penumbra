# Orbis Runtime Contract

Penumbra vendors the Orbis integration runtime contract here so local and CI
flows do not depend on a neighboring `orbis-rs` checkout or on `cli-tool`.

Current contract line:

- Orbis source ref: `d5889bd777bbac7bf97a8e89a2556116f2740ceb`
- Crypto feature: `decaf377`
- SourceHub image default: `ghcr.io/sourcenetwork/sourcehub:dev`

The compose file builds `orbis-node` from the pinned upstream git ref via a
remote Docker build context. That keeps Penumbra's supported runtime pinned in
repo without checking out `orbis-rs` in CI.
