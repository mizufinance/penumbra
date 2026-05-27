# SnarkPack Formal Handoff

Status: proof obligations for the next phase. No hax extraction is implemented
in this pass.

Preferred extraction order: hax to F* first, Lean 4 when the extracted subset
is supported, Coq fallback. EasyCrypt is reserved for later game-based
soundness work.

## Obligations

```text
fn encode_statement(input: StatementEncodingInput) -> Vec<u8>
property injective:
  forall a b.
    encode_statement(a) == encode_statement(b) -> a == b
```

```text
fn statement_digest(input: StatementEncodingInput) -> [u8; 32]
assumption sha256_collision_resistance:
  forall a b.
    statement_digest(a) == statement_digest(b) ->
      encode_statement(a) == encode_statement(b)
```

```text
fn challenge_context_from_statement_digest(statement_digest: [u8; 32]) -> ChallengeContext
assumption sha256_domain_separated_collision_resistance:
  forall a b.
    challenge_context_from_statement_digest(statement_digest(a)) ==
      challenge_context_from_statement_digest(statement_digest(b)) ->
        statement_digest(a) == statement_digest(b)
```

```text
fn challenge_preimage(
  stage: Vec<u8>,
  context: [u8; 32],
  nonce: u64,
  messages: Vec<u8>,
) -> Vec<u8>
property injective:
  forall a b.
    challenge_preimage(a.stage, a.context, a.nonce, a.messages) ==
      challenge_preimage(b.stage, b.context, b.nonce, b.messages) ->
        a == b

layout:
  output contains length-prefixed or fixed-width fields in order:
    domain("penumbra.snarkpack.challenge.v1\0"),
    len(stage), stage,
    context,
    nonce.to_le_bytes(),
    messages
```

Challenge binding is explicit: RIPP challenge derivation receives a
`ChallengeContext` argument and has no thread-local fallback state.

```text
fn validate_counts<T>(real: u32, padded: u32, rows: &[T]) -> Result<(), Error>
property count_invariant:
  Ok iff 0 < real <= padded
    and padded == rows.len()
    and padded is a power of two
```

```text
fn validate_row_arity(rows: &[Vec<Fq>], expected: usize) -> Result<(), Error>
property row_arity:
  Ok iff forall row in rows. row.len() == expected
```
