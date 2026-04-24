# Orbis Runtime Contract

Penumbra vendors the Orbis integration runtime contract here so local and CI
flows do not depend on a manually maintained neighboring `orbis-rs` checkout or
on `cli-tool`.

Current contract line:

- Orbis source ref: `d5889bd777bbac7bf97a8e89a2556116f2740ceb`
- Crypto feature: `decaf377`
- SourceHub image default: `ghcr.io/sourcenetwork/sourcehub:dev`

`./scripts/orbis-stack.sh up` prepares a local checkout of the pinned upstream
`orbis-rs` ref under `tmp/orbis-rs` and points Docker Compose at that local
build context. This avoids Docker BuildKit incompatibilities with remote git
contexts on older CI runners while keeping Penumbra's supported runtime pinned
in repo.
