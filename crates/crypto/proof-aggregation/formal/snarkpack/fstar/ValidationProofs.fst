module ValidationProofs

module S = Penumbra_sdk_proof_aggregation.Statement
module Seq = FStar.Seq
module V = Alloc.Vec
module Int = Rust_primitives.Integers
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - validate_counts returns Ok iff 0 < real <= padded, padded == rows.len(),
    and padded is a power of two.
  - validate_row_arity returns Ok iff every row length equals expected.
  - validate_repeat_final_padding returns Ok iff every padding-suffix row
    repeats the final real row.
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

let lemma_validate_counts_rejects_real_gt_padded
      (#a:Type0)
      (real_count padded_count:u32)
      (rows:t_Slice a)
    : Lemma
      (requires (real_count >. padded_count))
      (ensures (
        S.validate_counts #a real_count padded_count rows ==
        Core_models.Result.Result_Err
          (S.AggregateStatementError_BadCount {
            S.f_real_count = real_count;
            S.f_padded_count = padded_count
          })
      ))
= ()

let lemma_validate_counts_rejects_bad_padding_after_count_guards
      (#a:Type0)
      (real_count padded_count:u32)
      (rows:t_Slice a)
    : Lemma
      (requires (
        real_count <>. mk_u32 0 /\
        real_count <=. padded_count /\
        ((padded_count =. mk_u32 0 ||
          ~.(Core_models.Num.impl_u32__is_power_of_two padded_count <: bool) ||
          (Core_models.Result.impl__ok #usize
             #Core_models.Num.Error.t_TryFromIntError
             (Core_models.Convert.f_try_from #usize #u32 #FStar.Tactics.Typeclasses.solve padded_count
               <:
               Core_models.Result.t_Result usize Core_models.Num.Error.t_TryFromIntError)
           <:
           Core_models.Option.t_Option usize) <>.
          (Core_models.Option.Option_Some (Core_models.Slice.impl__len #a rows <: usize)
           <:
           Core_models.Option.t_Option usize)) == true)))
      (ensures (
        S.validate_counts #a real_count padded_count rows ==
        Core_models.Result.Result_Err
          (S.AggregateStatementError_BadPadding {
            S.f_padded_count = padded_count;
            S.f_row_count = Core_models.Slice.impl__len #a rows
          })
      ))
= ()

let lemma_validate_counts_accepts_when_all_guards_pass
      (#a:Type0)
      (real_count padded_count:u32)
      (rows:t_Slice a)
    : Lemma
      (requires (
        real_count <>. mk_u32 0 /\
        real_count <=. padded_count /\
        padded_count <>. mk_u32 0 /\
        (Core_models.Num.impl_u32__is_power_of_two padded_count <: bool) == true /\
        (Core_models.Result.impl__ok #usize
          #Core_models.Num.Error.t_TryFromIntError
          (Core_models.Convert.f_try_from #usize #u32 #FStar.Tactics.Typeclasses.solve padded_count
            <:
            Core_models.Result.t_Result usize Core_models.Num.Error.t_TryFromIntError)
        <:
        Core_models.Option.t_Option usize) ==
        (Core_models.Option.Option_Some (Core_models.Slice.impl__len #a rows <: usize)
        <:
        Core_models.Option.t_Option usize)))
      (ensures (
        S.validate_counts #a real_count padded_count rows ==
        Core_models.Result.Result_Ok (() <: Prims.unit)
      ))
= ()

let counts_valid (#a:Type0) (real_count padded_count:u32) (rows:t_Slice a)
    : Tot prop =
  real_count <>. mk_u32 0 /\
  real_count <=. padded_count /\
  padded_count <>. mk_u32 0 /\
  (Core_models.Num.impl_u32__is_power_of_two padded_count <: bool) == true /\
  (Core_models.Result.impl__ok #usize
    #Core_models.Num.Error.t_TryFromIntError
    (Core_models.Convert.f_try_from #usize #u32 #FStar.Tactics.Typeclasses.solve padded_count
      <:
      Core_models.Result.t_Result usize Core_models.Num.Error.t_TryFromIntError)
  <:
  Core_models.Option.t_Option usize) ==
  (Core_models.Option.Option_Some (Core_models.Slice.impl__len #a rows <: usize)
  <:
  Core_models.Option.t_Option usize)

let lemma_validate_counts_iff
      (#a:Type0)
      (real_count padded_count:u32)
      (rows:t_Slice a)
    : Lemma (
        Core_models.Result.Result_Ok? (S.validate_counts #a real_count padded_count rows)
          <==> counts_valid real_count padded_count rows
      )
= ()

(* ------------------------------------------------------------------ *)
(* validate_row_arity: Ok iff every row has the expected length         *)
(* ------------------------------------------------------------------ *)

/// Every row's length equals `expected`, by structural recursion matching
/// validate_row_arity_from.
let rec all_arity (#a: Type0)
      (rows: t_Slice (V.t_Vec a Alloc.Alloc.t_Global)) (expected: usize)
    : Tot prop (decreases Seq.length rows) =
  if Seq.length rows = 0
  then True
  else
    (V.impl_1__len #a (Seq.index rows 0) == expected) /\
    all_arity (Seq.slice rows 1 (Seq.length rows)) expected

let rec lemma_validate_row_arity_iff (#a: Type0)
      (rows: t_Slice (V.t_Vec a Alloc.Alloc.t_Global)) (expected index: usize)
    : Lemma
      (requires Int.v index + Seq.length rows <= Int.max_usize)
      (ensures
        (Core_models.Result.Result_Ok? (S.validate_row_arity_from #a rows expected index)
          <==> all_arity rows expected))
      (decreases Seq.length rows)
= if Seq.length rows = 0
  then ()
  else begin
    let rest = Seq.slice rows 1 (Seq.length rows) in
    Seq.lemma_index_slice rows 0 1 0;
    lemma_validate_row_arity_iff rest expected (index +! mk_usize 1)
  end

let lemma_validate_row_arity_iff_top (#a: Type0)
      (rows: t_Slice (V.t_Vec a Alloc.Alloc.t_Global)) (expected: usize)
    : Lemma
      (requires Seq.length rows <= Int.max_usize)
      (ensures
        (Core_models.Result.Result_Ok? (S.validate_row_arity #a rows expected)
          <==> all_arity rows expected))
= lemma_validate_row_arity_iff rows expected (mk_usize 0)

(* ------------------------------------------------------------------ *)
(* repeat-final padding: Ok iff every suffix row repeats final_real     *)
(* ------------------------------------------------------------------ *)

let rec all_eq_final (#a: Type0)
      (suffix: t_Slice (V.t_Vec a Alloc.Alloc.t_Global))
      (final_real: V.t_Vec a Alloc.Alloc.t_Global)
    : Tot prop (decreases Seq.length suffix) =
  if Seq.length suffix = 0
  then True
  else
    Seq.index suffix 0 == final_real /\
    all_eq_final (Seq.slice suffix 1 (Seq.length suffix)) final_real

let rec lemma_check_repeat_suffix_iff (#a: Type0)
      (#[FStar.Tactics.Typeclasses.tcresolve ()] i0: Core_models.Cmp.t_Eq a)
      (suffix: t_Slice (V.t_Vec a Alloc.Alloc.t_Global))
      (final_real: V.t_Vec a Alloc.Alloc.t_Global)
      (padded_count: u32)
      (row_count: usize)
    : Lemma
      (ensures
        (Core_models.Result.Result_Ok?
          (S.check_repeat_suffix #a suffix final_real padded_count row_count)
          <==> all_eq_final suffix final_real))
      (decreases Seq.length suffix)
= if Seq.length suffix = 0
  then ()
  else begin
    let rest = Seq.slice suffix 1 (Seq.length suffix) in
    Seq.lemma_index_slice suffix 0 1 0;
    lemma_check_repeat_suffix_iff rest final_real padded_count row_count
  end

let repeat_final_padding_ok (#a: Type0)
      (#[FStar.Tactics.Typeclasses.tcresolve ()] i0: Core_models.Cmp.t_Eq a)
      (real_count: u32)
      (rows: t_Slice (V.t_Vec a Alloc.Alloc.t_Global))
    : Tot prop =
  match
    Core_models.Result.impl__map_err #u32
      #Core_models.Num.Error.t_TryFromIntError
      #S.t_AggregateStatementError
      #(Core_models.Num.Error.t_TryFromIntError -> S.t_AggregateStatementError)
      (Core_models.Convert.f_try_from #u32
          #usize
          #FStar.Tactics.Typeclasses.solve
          (Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows <: usize)
        <:
        Core_models.Result.t_Result u32 Core_models.Num.Error.t_TryFromIntError)
      (fun _ ->
          S.AggregateStatementError_OversizeBytes
          ({
              S.f_field = "padded_public_inputs.len";
              S.f_max = cast (Core_models.Num.impl_u32__MAX <: u32) <: usize;
              S.f_got = Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows
            })
          <:
          S.t_AggregateStatementError)
    <:
    Core_models.Result.t_Result u32 S.t_AggregateStatementError
  with
  | Core_models.Result.Result_Err _ -> False
  | Core_models.Result.Result_Ok padded_count ->
    (match
        Core_models.Result.impl__map_err #usize
          #Core_models.Num.Error.t_TryFromIntError
          #S.t_AggregateStatementError
          #(Core_models.Num.Error.t_TryFromIntError -> S.t_AggregateStatementError)
          (Core_models.Convert.f_try_from #usize #u32 #FStar.Tactics.Typeclasses.solve real_count
            <:
            Core_models.Result.t_Result usize Core_models.Num.Error.t_TryFromIntError)
          (fun _ ->
              S.AggregateStatementError_BadCount
              ({ S.f_real_count = real_count; S.f_padded_count = padded_count })
              <:
              S.t_AggregateStatementError)
        <:
        Core_models.Result.t_Result usize S.t_AggregateStatementError
      with
      | Core_models.Result.Result_Err _ -> False
      | Core_models.Result.Result_Ok real_count_usize ->
        if
          real_count_usize =. mk_usize 0 ||
          real_count_usize >.
          (Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows <: usize)
        then False
        else
          all_eq_final #a
            (rows.[ { Core_models.Ops.Range.f_start = real_count_usize }
              <:
              Core_models.Ops.Range.t_RangeFrom usize ])
            (rows.[ real_count_usize -! mk_usize 1 <: usize ]))

let lemma_validate_repeat_final_padding_iff (#a: Type0)
      (#[FStar.Tactics.Typeclasses.tcresolve ()] i0: Core_models.Cmp.t_Eq a)
      (real_count: u32)
      (rows: t_Slice (V.t_Vec a Alloc.Alloc.t_Global))
    : Lemma
      (ensures
        (Core_models.Result.Result_Ok?
          (S.validate_repeat_final_padding #a real_count rows)
          <==> repeat_final_padding_ok #a real_count rows))
= match
    Core_models.Result.impl__map_err #u32
      #Core_models.Num.Error.t_TryFromIntError
      #S.t_AggregateStatementError
      #(Core_models.Num.Error.t_TryFromIntError -> S.t_AggregateStatementError)
      (Core_models.Convert.f_try_from #u32
          #usize
          #FStar.Tactics.Typeclasses.solve
          (Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows <: usize)
        <:
        Core_models.Result.t_Result u32 Core_models.Num.Error.t_TryFromIntError)
      (fun _ ->
          S.AggregateStatementError_OversizeBytes
          ({
              S.f_field = "padded_public_inputs.len";
              S.f_max = cast (Core_models.Num.impl_u32__MAX <: u32) <: usize;
              S.f_got = Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows
            })
          <:
          S.t_AggregateStatementError)
    <:
    Core_models.Result.t_Result u32 S.t_AggregateStatementError
  with
  | Core_models.Result.Result_Err _ -> ()
  | Core_models.Result.Result_Ok padded_count ->
    (match
        Core_models.Result.impl__map_err #usize
          #Core_models.Num.Error.t_TryFromIntError
          #S.t_AggregateStatementError
          #(Core_models.Num.Error.t_TryFromIntError -> S.t_AggregateStatementError)
          (Core_models.Convert.f_try_from #usize #u32 #FStar.Tactics.Typeclasses.solve real_count
            <:
            Core_models.Result.t_Result usize Core_models.Num.Error.t_TryFromIntError)
          (fun _ ->
              S.AggregateStatementError_BadCount
              ({ S.f_real_count = real_count; S.f_padded_count = padded_count })
              <:
              S.t_AggregateStatementError)
        <:
        Core_models.Result.t_Result usize S.t_AggregateStatementError
      with
      | Core_models.Result.Result_Err _ -> ()
      | Core_models.Result.Result_Ok real_count_usize ->
        if
          real_count_usize =. mk_usize 0 ||
          real_count_usize >.
          (Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows <: usize)
        then ()
        else
          lemma_check_repeat_suffix_iff #a
            (rows.[ { Core_models.Ops.Range.f_start = real_count_usize }
              <:
              Core_models.Ops.Range.t_RangeFrom usize ])
            (rows.[ real_count_usize -! mk_usize 1 <: usize ])
            padded_count
            (Core_models.Slice.impl__len #(V.t_Vec a Alloc.Alloc.t_Global) rows <: usize))
