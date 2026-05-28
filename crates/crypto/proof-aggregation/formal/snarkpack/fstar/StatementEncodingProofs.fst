module StatementEncodingProofs

module S = Penumbra_sdk_proof_aggregation.Statement
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - encode_statement is injective over typed StatementEncodingInput values.
  - top-level byte fields are separated by u32 little-endian length prefixes.
  - public-input rows and fields are separated by row count, row arity, and
    field length prefixes.

  Status: deferred until lemmas are implemented against the extracted F* modules.
*)

let smoke_encode_statement_is_extracted
      (input:S.t_StatementEncodingInput)
    : Core_models.Result.t_Result (Alloc.Vec.t_Vec u8 Alloc.Alloc.t_Global)
        S.t_AggregateStatementError =
  S.encode_statement input

let smoke_statement_row_wrapper_is_extracted
      (rows:S.t_StatementPaddedRows)
    : usize =
  S.impl_StatementPaddedRows__len rows
