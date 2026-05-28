# SnarkPack RIPP Refinement Map

Status: open refinement artifact. This document is the required audit map
between Penumbra-owned RIPP/SnarkPack Rust and the algorithm whose algebraic
soundness is assumed. It is not a mechanized proof.

Review each row against `docs/snarkpack/ripp-spec.md`. That file states the
local algorithm spec, transcript rules, and minimum evidence required for each
deviation class.

Rows are keyed by `symbol_id`, and every symbol in
`crates/crypto/proof-aggregation/formal/snarkpack/ripp-refinement-scope.txt`
must appear here exactly once.

Deviation classes:

- `mechanical`: naming, profiling, plumbing, or type-shape difference.
- `performance`: same semantics, different execution strategy.
- `security-binding`: transcript, statement, SRS, VK, or domain-binding logic.
- `semantic`: any behavior that may change accepted or rejected proofs.

Statuses:

- `refined`: reviewed against the algorithm, with cited evidence.
- `proved-equivalent`: mechanically proved equivalent to the algorithmic step.
- `assumed`: accepted by security/crypto review as an explicit assumption.
- `open`: blocker for completion.

Blocking rule: any `security-binding` or `semantic` deviation stays `open`
unless it is `proved-equivalent` or accepted as an `assumed` row by
security/crypto review. Disputed classification defaults to the higher-risk
class until resolved.

## Coverage

| symbol_id | algorithm reference | local behavior summary | deviation class | evidence | status | reviewer |
| --- | --- | --- | --- | --- | --- | --- |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs:ChallengeContext::from_statement_digest` | Penumbra statement-bound Fiat-Shamir adaptation | Derives context from statement digest with a fixed domain. | security-binding | `challenge_preimage_changes_on_stage_context_nonce_or_messages`; formal handoff row | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs:ChallengeContext::as_bytes` | Penumbra statement-bound Fiat-Shamir adaptation | Exposes the fixed 32-byte context to challenge framing. | security-binding | invariant guard against alternate constructors | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs:challenge_digest` | Fiat-Shamir challenge derivation | Hashes domain, stage, context, nonce, and messages before scalar conversion. | security-binding | challenge-trace tests; formal handoff row | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs:challenge_preimage` | Fiat-Shamir challenge framing | Canonically frames challenge query bytes. | security-binding | `challenge_preimage_layout_golden`; formal handoff row | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::setup` | GIPA setup | Produces commitment keys for the selected inner-product relation. | mechanical | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::prove` | GIPA prover | Entry point for GIPA proof generation. | mechanical | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::verify` | GIPA verifier | Entry point for GIPA proof verification. | mechanical | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::prove_with_aux` | GIPA prover with transcript output | Produces proof plus recursive challenge transcript. | mechanical | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::prove_with_aux_profiled_with_stage_with_trace` | GIPA prover with Fiat-Shamir trace | Adds profiling and explicit stage labels. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::_prove_profiled` | GIPA recursive prover core | Implements recursive folding, commitments, and challenge sampling. | semantic | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::verify_recursive_challenge_transcript_with_stage_with_trace` | GIPA verifier transcript reconstruction | Reconstructs recursive challenges with explicit labels and trace sink. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::_compute_recursive_challenges` | GIPA verifier challenge computation | Computes challenges from proof commitments. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::_compute_final_commitment_keys` | GIPA verifier folded key computation | Folds commitment keys using the verifier transcript. | semantic | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs:GIPA::_verify_base_commitment` | GIPA base check | Checks final base commitment relation. | semantic | existing GIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:SRS::get_commitment_keys` | TIPA SRS commitment key extraction | Returns G1/G2 commitment keys from SRS. | mechanical | `prepared_proving_srs_matches_commitment_keys` | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:SRS::prepare_for_proving` | TIPA prover SRS preparation | Prepares affine/prepared point views for proving. | performance | `prepared_proving_srs_matches_commitment_keys` | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:SRS::get_verifier_key` | TIPA verifier SRS extraction | Returns verifier SRS from full SRS. | mechanical | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:PreparedProvingSrs::new` | TIPA prover SRS preparation | Constructs prepared prover SRS. | performance | `prepared_proving_srs_matches_commitment_keys` | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:prove_pairing_inner_product_with_prepared_srs_shift_profiled` | TIPA AB prover shifted SRS path | Proves pairing inner-product relation using prepared shifted SRS. | performance | specialized/generic proof byte parity test | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:prove_pairing_inner_product_gipa_with_aux_profiled` | TIPA AB GIPA reduction | Runs GIPA reduction for pairing inner-product relation. | semantic | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:TIPA::setup` | TIPA setup | Generates TIPA SRS and target commitment key. | mechanical | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:TIPA::prove_with_prepared_srs_shift_profiled_with_trace` | TIPA prover with shifted prepared SRS | Builds TIPA proof, KZG openings, and trace events. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:TIPA::verify_with_srs_shift_and_labels_with_trace` | TIPA verifier with explicit labels | Verifies TIPA proof and KZG openings with explicit stage labels. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:prove_commitment_key_kzg_opening_with_affine_profiled` | KZG opening prover | Produces commitment-key KZG opening. | semantic | affine/projective parity test | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:verify_commitment_key_g2_kzg_opening` | KZG opening verifier over G2 keys | Verifies G2 commitment-key opening. | semantic | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:verify_commitment_key_g1_kzg_opening` | KZG opening verifier over G1 keys | Verifies G1 commitment-key opening. | semantic | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:structured_generators_scalar_power` | Structured generator scalar powers | Builds structured scalar powers for shifted generators. | semantic | existing shifted-SRS tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:polynomial_evaluation_product_form_from_transcript` | KZG transcript polynomial evaluation | Computes product-form polynomial evaluation from transcript. | semantic | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs:polynomial_coefficients_from_transcript` | KZG transcript polynomial coefficients | Computes polynomial coefficients from transcript and shift. | semantic | existing TIPA unit tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:GIPAWithSSM::setup` | Structured-scalar GIPA setup | Creates commitment keys for SSM path. | mechanical | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:GIPAWithSSM::prove_with_structured_scalar_message` | Structured-scalar GIPA prover | Proves inner product with one structured clear message. | semantic | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:GIPAWithSSM::verify_with_structured_scalar_message` | Structured-scalar GIPA verifier | Verifies SSM GIPA relation. | semantic | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:TIPAWithSSM::setup` | TIPA SSM setup | Creates SRS and commitment key for SSM TIPA. | mechanical | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:TIPAWithSSM::prove_with_prepared_structured_scalar_message_profiled` | TIPA SSM prover | Proves structured-scalar TIPA relation. | semantic | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:TIPAWithSSM::verify_with_structured_scalar_message_with_trace` | TIPA SSM verifier | Verifies SSM TIPA relation with explicit trace. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs:structured_scalar_power` | Structured scalar powers | Builds geometric power vector for SSM path. | semantic | existing SSM tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:setup_inner_product` | Groth16 aggregation SRS setup | Creates inner-product SRS for aggregation. | mechanical | SRS id/report tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:aggregate_proofs_profiled_with_trace` | Groth16 aggregate prover | Aggregates Groth16 proofs with profiling and trace. | security-binding | aggregate parity/property tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:verify_aggregate_proof_profiled_with_trace` | Groth16 aggregate verifier | Verifies aggregate proof with profiling and trace. | security-binding | aggregate parity/property tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:derive_randomizer` | Groth16 aggregate randomizer | Derives aggregation randomizer from statement-bound Fiat-Shamir context. | security-binding | challenge trace parity tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:verify_tipa_ab` | Groth16 AB TIPA verifier | Verifies AB TIPA subproof. | semantic | aggregate mutation tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:verify_tipa_c` | Groth16 C TIPA verifier | Verifies C/SSM TIPA subproof. | semantic | aggregate mutation tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:fold_public_inputs` | Public input folding | Folds public inputs into aggregate verifier equation. | semantic | public-input mutation tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:verify_ppe` | Pairing product equation check | Checks final aggregate pairing product equation. | semantic | aggregate mutation tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:build_shifted_ck_1` | Shifted commitment key construction | Builds shifted commitment key powers for C path. | semantic | shifted key tests | open | pending |
| `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs:inverse_powers` | Inverse powers for shifted SRS | Builds inverse powers used by shifted commitment key path. | semantic | inverse powers test | open | pending |
