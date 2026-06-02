module FrameLemmas
#set-options "--fuel 0 --ifuel 0 --z3rlimit 60"

module Seq = FStar.Seq
module Num = Core_models.Num
module Int = Rust_primitives.Integers
open Core_models

(*
  Foundation lemmas for the SnarkPack byte-encoding injectivity proofs.

  Every encoder in the boundary (statement, wrapper, challenge preimage) is a
  left-associated concatenation of two frame shapes:

    - fixed-width value frames:  to_le_bytes(value) || rest
    - length-prefixed frames:    to_le_bytes(len)   || field || rest

  These lemmas show each frame shape recovers its boundary, so a sequence of
  frames is injective. They reduce entirely to FStar.Seq plus the recorded
  to_le_bytes injectivity axioms (Core_models.Num, see hax-extraction-boundary.md).

  hax models seq_concat / extend_from_slice as FStar.Seq.append and
  to_le_bytes as a length-4 / length-8 array, so consumers' (encode_x ..)._0
  values reduce to the FStar.Seq.append terms reasoned about here.
*)

/// Fixed-width split: equal-length left segments are determined by the concat.
let lemma_fixed_split_inj (#t: Type) (a r1 b r2: Seq.seq t)
    : Lemma
      (requires Seq.length a == Seq.length b /\ Seq.append a r1 == Seq.append b r2)
      (ensures a == b /\ r1 == r2)
= Seq.lemma_append_inj a r1 b r2

/// A 4-byte little-endian u32 value frame recovers its value and tail.
let lemma_u32_value_frame_inj (va: u32) (ra: Seq.seq u8) (vb: u32) (rb: Seq.seq u8)
    : Lemma
      (requires
        Seq.append (Num.impl_u32__to_le_bytes va) ra ==
        Seq.append (Num.impl_u32__to_le_bytes vb) rb)
      (ensures va == vb /\ ra == rb)
= let pa = Num.impl_u32__to_le_bytes va in
  let pb = Num.impl_u32__to_le_bytes vb in
  assert (Seq.length pa == 4);
  assert (Seq.length pb == 4);
  Seq.lemma_append_inj pa ra pb rb;
  Num.impl_u32__to_le_bytes_injective va vb

/// An 8-byte little-endian u64 value frame recovers its value and tail.
let lemma_u64_value_frame_inj (va: u64) (ra: Seq.seq u8) (vb: u64) (rb: Seq.seq u8)
    : Lemma
      (requires
        Seq.append (Num.impl_u64__to_le_bytes va) ra ==
        Seq.append (Num.impl_u64__to_le_bytes vb) rb)
      (ensures va == vb /\ ra == rb)
= let pa = Num.impl_u64__to_le_bytes va in
  let pb = Num.impl_u64__to_le_bytes vb in
  assert (Seq.length pa == 8);
  assert (Seq.length pb == 8);
  Seq.lemma_append_inj pa ra pb rb;
  Num.impl_u64__to_le_bytes_injective va vb

/// A length-prefixed frame `to_le_bytes(len) || field || rest` recovers field
/// and rest, given the prefixed length actually equals the field length.
let lemma_lenpref_frame_inj
      (la: u32) (fa ra: Seq.seq u8)
      (lb: u32) (fb rb: Seq.seq u8)
    : Lemma
      (requires
        Int.v la == Seq.length fa /\ Int.v lb == Seq.length fb /\
        Seq.append (Seq.append (Num.impl_u32__to_le_bytes la) fa) ra ==
        Seq.append (Seq.append (Num.impl_u32__to_le_bytes lb) fb) rb)
      (ensures la == lb /\ fa == fb /\ ra == rb)
= let pa = Num.impl_u32__to_le_bytes la in
  let pb = Num.impl_u32__to_le_bytes lb in
  assert (Seq.length pa == 4);
  assert (Seq.length pb == 4);
  Seq.append_assoc pa fa ra;
  Seq.append_assoc pb fb rb;
  // now pa @ (fa @ ra) == pb @ (fb @ rb)
  Seq.lemma_append_inj pa (Seq.append fa ra) pb (Seq.append fb rb);
  // pa == pb gives la == lb, hence length fa == length fb
  Num.impl_u32__to_le_bytes_injective la lb;
  Seq.lemma_append_inj fa ra fb rb
