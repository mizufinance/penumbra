# Aggregate decoder corpus

The valid inner proofs are extracted from the runtime aggregate byte baseline:
rows 0–3 cover Transfer counts 1, 2, 4, and 8; rows 7, 11, and 15 cover both
reshape families and withdrawal at count 8. Runtime tests check acceptance.
Malformed seeds exercise truncation, trailing bytes, and oversized input.

The target compares the production decoder with the independently defined
reference wire tree, checked group decoding, singleton identity-output checks,
and canonical reserialization. It does not verify cryptographic proof soundness.
Long campaigns and their measured results live in `mizufinance/shieldd-security`.
