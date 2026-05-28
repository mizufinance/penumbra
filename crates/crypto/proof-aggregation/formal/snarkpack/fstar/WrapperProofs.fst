module WrapperProofs

module W = Penumbra_sdk_proof_aggregation.Aggregate_proof_wrapper
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - decode_wrapped_aggregate_proof_inner_range accepts the output of
    encode_wrapped_aggregate_proof when statement digest and size cap match.
  - digest mismatch is rejected before the inner proof range is returned.

  Status: deferred until lemmas are implemented against the extracted F* modules.
*)

let smoke_encode_wrapper_is_extracted
      (statement_digest:t_Array u8 (mk_usize 32))
      (inner_proof_bytes:t_Slice u8)
    : Core_models.Result.t_Result (Alloc.Vec.t_Vec u8 Alloc.Alloc.t_Global)
        W.t_AggregateProofBytesError =
  W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes

let smoke_decode_inner_range_is_extracted
      (wrapped_proof_bytes:t_Slice u8)
      (expected_statement_digest:t_Array u8 (mk_usize 32))
      (cap:Core_models.Option.t_Option usize)
    : Core_models.Result.t_Result (Core_models.Ops.Range.t_Range usize)
        W.t_AggregateProofBytesError =
  W.decode_wrapped_aggregate_proof_inner_range wrapped_proof_bytes expected_statement_digest cap

let lemma_wrapper_rejects_oversize_before_parsing
      (wrapped_proof_bytes:t_Slice u8)
      (expected_statement_digest:t_Array u8 (mk_usize 32))
      (max:usize{(Core_models.Slice.impl__len #u8 wrapped_proof_bytes <: usize) >. max})
    : Lemma (
        W.decode_wrapped_aggregate_proof_inner_range
          wrapped_proof_bytes
          expected_statement_digest
          (Core_models.Option.Option_Some max <: Core_models.Option.t_Option usize)
        ==
        Core_models.Result.Result_Err
          (W.AggregateProofBytesError_OversizeBytes {
            W.f_max = max;
            W.f_got = Core_models.Slice.impl__len #u8 wrapped_proof_bytes
          })
      )
= ()
