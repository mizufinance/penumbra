module WrapperProofs
#set-options "--fuel 1 --ifuel 1 --z3rlimit 100"

module W = Penumbra_sdk_proof_aggregation.Aggregate_proof_wrapper
module V = Alloc.Vec
module Seq = FStar.Seq
module Num = Core_models.Num
module Int = Rust_primitives.Integers
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - decode_wrapped_aggregate_proof_inner_range accepts the output of
    encode_wrapped_aggregate_proof when statement digest and size cap match.
  - digest mismatch is rejected before the inner proof range is returned.
*)

let bo (b: V.t_Vec u8 Alloc.Alloc.t_Global) : Seq.seq u8 = b._0

let wrapper_header_len () : usize = mk_usize 74

let wrapper_spec
      (statement_digest:t_Array u8 (mk_usize 32))
      (inner_len:u32)
      (inner_proof_bytes:t_Slice u8)
    : Seq.seq u8 =
  Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN
      (Seq.append statement_digest
        (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes))

let wrapper_domain_list : list u8 =
  [
    mk_u8 112; mk_u8 101; mk_u8 110; mk_u8 117; mk_u8 109; mk_u8 98; mk_u8 114; mk_u8 97;
    mk_u8 46; mk_u8 115; mk_u8 110; mk_u8 97; mk_u8 114; mk_u8 107; mk_u8 112; mk_u8 97;
    mk_u8 99; mk_u8 107; mk_u8 46; mk_u8 97; mk_u8 103; mk_u8 103; mk_u8 114; mk_u8 101;
    mk_u8 103; mk_u8 97; mk_u8 116; mk_u8 101; mk_u8 95; mk_u8 112; mk_u8 114; mk_u8 111;
    mk_u8 111; mk_u8 102; mk_u8 46; mk_u8 118; mk_u8 49; mk_u8 0
  ]

let lemma_wrapper_domain_len ()
    : Lemma (Seq.length W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN == 38)
= assert_norm (FStar.List.Tot.length wrapper_domain_list == 38);
  assert (W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN == Seq.seq_of_list wrapper_domain_list)

let lemma_usize_to_u32_ok (len:usize)
    : Lemma
      (requires Int.v len <= 4294967295)
      (ensures
        Core_models.Convert.f_try_from #u32 #usize #FStar.Tactics.Typeclasses.solve len ==
          Core_models.Result.Result_Ok (cast len <: u32) /\
        Int.v (cast len <: u32) == Int.v len)
= ()

#push-options "--fuel 20 --ifuel 2 --z3rlimit 400"

let lemma_encode_wrapper_content
      (statement_digest:t_Array u8 (mk_usize 32))
      (inner_proof_bytes:t_Slice u8)
    : Lemma
      (requires
        Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= 4294967295 /\
        Seq.length W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN + 32 + 4 +
          Seq.length inner_proof_bytes <= Int.max_usize /\
        Int.v (wrapper_header_len ()) +
          Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= Int.max_usize)
      (ensures
        Core_models.Result.Result_Ok?
          (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes) /\
        Int.v (cast (Core_models.Slice.impl__len #u8 inner_proof_bytes) <: u32) ==
          Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) /\
        bo
          (Core_models.Result.Result_Ok?._0
            (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes)) ==
          wrapper_spec
            statement_digest
            (cast (Core_models.Slice.impl__len #u8 inner_proof_bytes) <: u32)
            inner_proof_bytes)
= lemma_wrapper_domain_len ()
  ; lemma_usize_to_u32_ok (Core_models.Slice.impl__len #u8 inner_proof_bytes);
  let inner_len = cast (Core_models.Slice.impl__len #u8 inner_proof_bytes) <: u32 in
  let cap =
    ((((Core_models.Slice.impl__len #u8 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN <: usize)
        +! mk_usize 32 <: usize)
      +! mk_usize 4 <: usize)
    +! (Core_models.Slice.impl__len #u8 inner_proof_bytes <: usize) <: usize)
  in
  let b0 = V.impl__with_capacity #u8 cap in
  let b1 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b0 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN in
  let b2 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b1 (statement_digest <: t_Slice u8) in
  let b3 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b2 (Num.impl_u32__to_le_bytes inner_len <: t_Slice u8) in
  let b4 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b3 inner_proof_bytes in
  Seq.append_empty_l W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN;
  Seq.append_assoc W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN
    statement_digest
    (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes);
  Seq.append_assoc
    (Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN statement_digest)
    (Num.impl_u32__to_le_bytes inner_len)
    inner_proof_bytes;
  assert (bo b4 == wrapper_spec statement_digest inner_len inner_proof_bytes)

#pop-options

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

let lemma_wrapper_roundtrip
      (statement_digest:t_Array u8 (mk_usize 32))
      (inner_proof_bytes:t_Slice u8)
    : Lemma
      (requires
        Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= 4294967295 /\
        Seq.length W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN + 32 + 4 +
          Seq.length inner_proof_bytes <= Int.max_usize /\
        Int.v (wrapper_header_len ()) +
          Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= Int.max_usize)
      (ensures
        W.decode_wrapped_aggregate_proof_inner_range
          (bo
            (Core_models.Result.Result_Ok?._0
              (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes)))
          statement_digest
          (Core_models.Option.Option_None <: Core_models.Option.t_Option usize)
        ==
        Core_models.Result.Result_Ok
          ({
              Core_models.Ops.Range.f_start = wrapper_header_len ();
              Core_models.Ops.Range.f_end =
                (wrapper_header_len () +!
                  (Core_models.Slice.impl__len #u8 inner_proof_bytes <: usize) <: usize)
            }
            <:
            Core_models.Ops.Range.t_Range usize))
= lemma_encode_wrapper_content statement_digest inner_proof_bytes;
  let inner_len = cast (Core_models.Slice.impl__len #u8 inner_proof_bytes) <: u32 in
  let rest =
    Seq.append statement_digest
      (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes)
  in
  let encoded =
    Core_models.Result.Result_Ok?._0
      (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes)
  in
  let bytes = bo encoded in
  assert (bytes == Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN rest);
  lemma_wrapper_domain_len ();
  assert (Core_models.Slice.impl__len #u8 bytes == wrapper_header_len () +!
    (Core_models.Slice.impl__len #u8 inner_proof_bytes <: usize));
  Core_models.Slice.impl__starts_with_append #u8 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN rest;
  let digest_start = Core_models.Slice.impl__len #u8 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN in
  let digest_end = digest_start +! mk_usize 32 in
  assert (digest_start == mk_usize 38);
  assert (digest_end == mk_usize 70);
  Core_models.Slice.impl__get_middle_append3 #u8
    W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN
    statement_digest
    (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes);
  assert (
    Core_models.Slice.impl__get #u8
      #(Core_models.Ops.Range.t_Range usize)
      bytes
      ({ Core_models.Ops.Range.f_start = digest_start;
         Core_models.Ops.Range.f_end = digest_end }
       <: Core_models.Ops.Range.t_Range usize)
    ==
    Core_models.Option.Option_Some (statement_digest <: t_Slice u8));
  let len_start = digest_end in
  let len_end = len_start +! mk_usize 4 in
  assert (len_end == wrapper_header_len ());
  Seq.append_assoc W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN
    statement_digest
    (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes);
  assert (
    bytes ==
    Seq.append
      (Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN statement_digest)
      (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes));
  Core_models.Slice.impl__get_middle_append3 #u8
    (Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN statement_digest)
    (Num.impl_u32__to_le_bytes inner_len)
    inner_proof_bytes;
  assert (
    Core_models.Slice.impl__get #u8
      #(Core_models.Ops.Range.t_Range usize)
      bytes
      ({ Core_models.Ops.Range.f_start = len_start;
         Core_models.Ops.Range.f_end = len_end }
       <: Core_models.Ops.Range.t_Range usize)
    ==
      Core_models.Option.Option_Some
      ((Num.impl_u32__to_le_bytes inner_len <: t_Slice u8)));
  Core_models.Convert.impl__try_into_array_self_slice #u8
    #(mk_usize 4)
    (Num.impl_u32__to_le_bytes inner_len);
  assert (
    Core_models.Convert.f_try_into #(t_Slice u8)
      #(t_Array u8 (mk_usize 4))
      #FStar.Tactics.Typeclasses.solve
      (Num.impl_u32__to_le_bytes inner_len <: t_Slice u8)
    ==
    Core_models.Result.Result_Ok (Num.impl_u32__to_le_bytes inner_len));
  Num.impl_u32__from_to_le_bytes inner_len;
  Num.impl_usize_u32_cast_roundtrip (Core_models.Slice.impl__len #u8 inner_proof_bytes);
  assert ((cast (inner_len <: u32) <: usize) == Core_models.Slice.impl__len #u8 inner_proof_bytes);
  Num.impl_usize__checked_add_ok (wrapper_header_len ())
    (Core_models.Slice.impl__len #u8 inner_proof_bytes);
  assert (
    Num.impl_usize__checked_add (wrapper_header_len ())
      (Core_models.Slice.impl__len #u8 inner_proof_bytes)
    ==
    Core_models.Option.Option_Some
      (wrapper_header_len () +!
        (Core_models.Slice.impl__len #u8 inner_proof_bytes <: usize)))

let lemma_wrapper_digest_mismatch_before_range
      (statement_digest expected_statement_digest:t_Array u8 (mk_usize 32))
      (inner_proof_bytes:t_Slice u8)
    : Lemma
      (requires
        statement_digest <> expected_statement_digest /\
        Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= 4294967295 /\
        Seq.length W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN + 32 + 4 +
          Seq.length inner_proof_bytes <= Int.max_usize /\
        Int.v (wrapper_header_len ()) +
          Int.v (Core_models.Slice.impl__len #u8 inner_proof_bytes) <= Int.max_usize)
      (ensures
        W.decode_wrapped_aggregate_proof_inner_range
          (bo
            (Core_models.Result.Result_Ok?._0
              (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes)))
          expected_statement_digest
          (Core_models.Option.Option_None <: Core_models.Option.t_Option usize)
        ==
        Core_models.Result.Result_Err
          (W.AggregateProofBytesError_StatementDigestMismatch <: W.t_AggregateProofBytesError))
= lemma_encode_wrapper_content statement_digest inner_proof_bytes;
  let inner_len = cast (Core_models.Slice.impl__len #u8 inner_proof_bytes) <: u32 in
  let rest =
    Seq.append statement_digest
      (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes)
  in
  let encoded =
    Core_models.Result.Result_Ok?._0
      (W.encode_wrapped_aggregate_proof statement_digest inner_proof_bytes)
  in
  let bytes = bo encoded in
  assert (bytes == Seq.append W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN rest);
  lemma_wrapper_domain_len ();
  Core_models.Slice.impl__starts_with_append #u8 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN rest;
  let digest_start = Core_models.Slice.impl__len #u8 W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN in
  let digest_end = digest_start +! mk_usize 32 in
  assert (digest_start == mk_usize 38);
  assert (digest_end == mk_usize 70);
  Core_models.Slice.impl__get_middle_append3 #u8
    W.v_AGGREGATE_PROOF_WRAPPER_DOMAIN
    statement_digest
    (Seq.append (Num.impl_u32__to_le_bytes inner_len) inner_proof_bytes);
  assert (
    Core_models.Slice.impl__get #u8
      #(Core_models.Ops.Range.t_Range usize)
      bytes
      ({ Core_models.Ops.Range.f_start = digest_start;
         Core_models.Ops.Range.f_end = digest_end }
       <: Core_models.Ops.Range.t_Range usize)
    ==
    Core_models.Option.Option_Some (statement_digest <: t_Slice u8))
