# SnarkPack Hax Extraction Boundary

Status: reviewed metadata for the current implementation-boundary extraction
set. This file bounds the hax semantic-preservation assumption by target.

Every target listed in `hax-targets.txt` must have exactly one row here.
Every compatibility `assume val` introduced by `scripts/snarkpack-formal.sh`
must have a shim row with a semantic postcondition and removal path.

## Extracted Targets

| target | Rust features used | precondition | arithmetic mode | control flow | panic/expect | unsafe | hax shims | status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `penumbra_sdk_proof_aggregation::statement::StatementFieldBytes` | owned `Vec<u8>` newtype | `requires true` | none | constructors/accessors only | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::StatementPublicInputRow` | owned `Vec<StatementFieldBytes>` newtype | `requires true` | `usize` length observation only | iterator accessors | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::StatementPaddedRows` | owned `Vec<StatementPublicInputRow>` newtype | `requires true` | `usize` length observation only | iterator accessors | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::StatementEncodingInput` | plain owned fields and typed rows | `requires true` | fixed-width `u32` fields | data carrier | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::encode_statement` | `Vec` allocation, slice appends, typed row iteration | every byte-field length fits `u32` | `u32` little-endian writes; length conversion checked | bounded `for` loops over rows and fields | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::validate_counts` | slice length observation | `requires true` | checked `usize`/`u32` comparison via conversion | branch-only | none | none | `impl_u32__is_power_of_two` | reviewed |
| `penumbra_sdk_proof_aggregation::statement::validate_row_arity` | nested `Vec` row length observation | `requires true` | `usize` equality | bounded `for` loop over rows | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::statement::validate_repeat_final_padding` | nested `Vec` row equality and suffix iteration | `requires true` | checked `u32`/`usize` conversion | bounded `for` loop over padded suffix | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::aggregate_proof_wrapper::encode_wrapped_aggregate_proof` | `Vec` allocation and slice appends | inner proof length fits `u32` | checked `u32` length conversion | branch-only | none | none | none | reviewed |
| `penumbra_sdk_proof_aggregation::aggregate_proof_wrapper::decode_wrapped_aggregate_proof_inner_range` | slice indexing, length checks, `Range<usize>` | `requires true` | checked addition for proof end | branch-only | none | none | `impl__starts_with` | reviewed |
| `ark_ip_proofs::challenge::ChallengeContext` | private 32-byte array newtype | constructor input is a 32-byte statement digest | none | constructor/accessor only | none | none | none | reviewed |
| `ark_ip_proofs::challenge::challenge_context_preimage` | `Vec` allocation and slice appends | input is a 32-byte statement digest | none | branch-free | none | none | none | reviewed |
| `ark_ip_proofs::challenge::challenge_preimage` | `Vec` allocation and slice appends | stage label length fits `u32` | `u64` little-endian nonce; checked stage length | branch-free after length conversion | `expect` on static stage-label length; accepted because all labels are compile-time constants and invariant-reviewed | none | none | reviewed |

## Support Shims

| shim | semantic postcondition | affected proof row | owner | reviewer | removal path | status |
| --- | --- | --- | --- | --- | --- | --- |
| `impl_u32__is_power_of_two` | returns true iff the `u32` input is a nonzero power of two under Rust `u32::is_power_of_two` semantics | `validate_counts` iff row | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* support library exposes a compatible definition accepted by pinned F* | assumed |
| `impl__starts_with` | returns true iff the first slice begins with the second slice element-wise | wrapper malformed-domain rejection and round-trip rows | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* support library exposes a compatible definition accepted by pinned F* | assumed |
| `impl__starts_with_append` | if a slice is constructed as `prefix || rest`, `starts_with(prefix || rest, prefix)` returns true | wrapper round-trip row | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* support library exposes a compatible definition accepted by pinned F* | assumed |
| `impl__get_middle_append3` | for `prefix || field || suffix`, range-getting `[len(prefix), len(prefix) + len(field))` returns `field` when the total length fits `usize` | wrapper round-trip and digest-mismatch rows | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* support library exposes range-get slice lemmas accepted by pinned F* | assumed |
| `impl__try_into_array_self_slice` | converting a fixed-size array viewed as an equal-length slice back to that array succeeds unchanged | wrapper round-trip row | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* support library exposes slice-to-array round-trip lemmas accepted by pinned F* | assumed |
| `impl_u32__to_le_bytes_injective` | the 4-byte little-endian encoding of a `u32` is injective (distinct values give distinct byte arrays); `to_le_bytes` itself is an opaque hax model with no roundtrip, so injectivity is recorded as an axiom | statement encoding injectivity, frame-lemma value/length-prefix recovery | proof-aggregation maintainers | pending security/crypto review | remove when the hax/F* support library exposes a `from_le_bytes ∘ to_le_bytes = id` roundtrip accepted by pinned F* | assumed |
| `impl_u32__from_to_le_bytes` | `u32::from_le_bytes(u32::to_le_bytes(x)) == x` under Rust little-endian integer encoding semantics | wrapper round-trip row | proof-aggregation maintainers | pending security/crypto review | remove when the hax/F* support library exposes the roundtrip accepted by pinned F* | assumed |
| `impl_usize_u32_cast_roundtrip` | if a `usize` value fits in `u32`, casting to `u32` and back to `usize` preserves the value | wrapper round-trip row | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* exposes machine-integer cast lemmas accepted by pinned F* | assumed |
| `impl_usize__checked_add_ok` | `usize::checked_add(x, y)` returns `Some(x + y)` whenever `x + y` fits in `usize` | wrapper round-trip row | proof-aggregation maintainers | pending security/crypto review | remove when hax/F* exposes checked-arithmetic lemmas accepted by pinned F* | assumed |
| `impl_u64__to_le_bytes_injective` | the 8-byte little-endian encoding of a `u64` is injective (distinct values give distinct byte arrays); same opaque-model rationale as the `u32` axiom | challenge-preimage nonce frame recovery | proof-aggregation maintainers | pending security/crypto review | remove when the hax/F* support library exposes a `from_le_bytes ∘ to_le_bytes = id` roundtrip accepted by pinned F* | assumed |

## Risk Rules

New extraction targets must declare their preconditions before proof work starts.
New `unsafe`, `while`, `loop`, unchecked arithmetic, panics, or support shims in
extracted targets are blockers until this file records the exact semantics and
`scripts/check-snarkpack-invariants.sh` is updated when the pattern is
script-checkable.
