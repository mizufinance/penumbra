module ValidationProofs

module S = Penumbra_sdk_proof_aggregation.Statement
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - validate_counts returns Ok iff 0 < real <= padded, padded == rows.len(),
    and padded is a power of two.
  - validate_row_arity returns Ok iff every row length equals expected.

  Status: deferred until lemmas are implemented against the extracted F* modules.
*)

let smoke_validate_counts_is_extracted
      (#a:Type0)
      (real_count padded_count:u32)
      (rows:t_Slice a)
    : Core_models.Result.t_Result Prims.unit S.t_AggregateStatementError =
  S.validate_counts #a real_count padded_count rows

let smoke_validate_row_arity_is_extracted
      (#a:Type0)
      (rows:t_Slice (Alloc.Vec.t_Vec a Alloc.Alloc.t_Global))
      (expected:usize)
    : Core_models.Result.t_Result Prims.unit S.t_AggregateStatementError =
  S.validate_row_arity #a rows expected

let lemma_validate_counts_rejects_zero
      (#a:Type0)
      (padded_count:u32)
      (rows:t_Slice a)
    : Lemma (
        S.validate_counts #a (mk_u32 0) padded_count rows ==
        Core_models.Result.Result_Err
          (S.AggregateStatementError_BadCount {
            S.f_real_count = mk_u32 0;
            S.f_padded_count = padded_count
          })
      )
= ()
