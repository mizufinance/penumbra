# Protobuf compiler

Run `just proto` from the repository root; `just proto-check` checks drift.
The wrapper reads `proto/codegen.json`, stages the pinned import closure, and
writes Rust to `crates/proto/src/gen` and Go to `proto/go/gen`.

The Rust compiler takes input directory, output directory and explicit root
schemas. Use the wrapper for reproducible generation and Go compilation.
See [development](../../docs/development.md).
