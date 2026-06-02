module ChallengePreimageProofs
#set-options "--fuel 1 --ifuel 1 --z3rlimit 100"

module C = Ark_ip_proofs.Challenge
module RSeq = Rust_primitives.Sequence
module V = Alloc.Vec
module Seq = FStar.Seq
module Num = Core_models.Num
module Int = Rust_primitives.Integers
module F = FrameLemmas
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - challenge_preimage(stage, context, nonce, messages) is injective.
  - challenge preimages contain the fixed challenge domain, stage length,
    stage bytes, context bytes, u64 little-endian nonce, and messages in order.

  These lemmas prove the extracted preimage constructors' byte layout. Hash
  binding is a reduction to SHA-256 assumptions recorded in formal-handoff.md.
*)

let lemma_seq_from_slice_id (#a:Type0) (s:t_Slice a)
    : Lemma (RSeq.seq_from_slice #a s == s)
= ()

let bo (b: V.t_Vec u8 Alloc.Alloc.t_Global) : Seq.seq u8 = b._0

let challenge_context_spec (statement_digest:t_Array u8 (mk_usize 32)) : Seq.seq u8 =
  Seq.append C.v_CHALLENGE_CONTEXT_DOMAIN statement_digest

let challenge_domain_list : list u8 =
  [
    mk_u8 112; mk_u8 101; mk_u8 110; mk_u8 117; mk_u8 109; mk_u8 98; mk_u8 114; mk_u8 97;
    mk_u8 46; mk_u8 115; mk_u8 110; mk_u8 97; mk_u8 114; mk_u8 107; mk_u8 112; mk_u8 97;
    mk_u8 99; mk_u8 107; mk_u8 46; mk_u8 99; mk_u8 104; mk_u8 97; mk_u8 108; mk_u8 108;
    mk_u8 101; mk_u8 110; mk_u8 103; mk_u8 101; mk_u8 46; mk_u8 118; mk_u8 49; mk_u8 0
  ]

let challenge_context_domain_list : list u8 =
  [
    mk_u8 112; mk_u8 101; mk_u8 110; mk_u8 117; mk_u8 109; mk_u8 98; mk_u8 114; mk_u8 97;
    mk_u8 46; mk_u8 115; mk_u8 110; mk_u8 97; mk_u8 114; mk_u8 107; mk_u8 112; mk_u8 97;
    mk_u8 99; mk_u8 107; mk_u8 46; mk_u8 99; mk_u8 104; mk_u8 97; mk_u8 108; mk_u8 108;
    mk_u8 101; mk_u8 110; mk_u8 103; mk_u8 101; mk_u8 95; mk_u8 99; mk_u8 111; mk_u8 110;
    mk_u8 116; mk_u8 101; mk_u8 120; mk_u8 116; mk_u8 46; mk_u8 118; mk_u8 49; mk_u8 0
  ]

let lemma_challenge_domain_len ()
    : Lemma (Seq.length C.v_CHALLENGE_DOMAIN == 32)
= assert_norm (FStar.List.Tot.length challenge_domain_list == 32);
  assert (C.v_CHALLENGE_DOMAIN == Seq.seq_of_list challenge_domain_list)

let lemma_challenge_context_domain_len ()
    : Lemma (Seq.length C.v_CHALLENGE_CONTEXT_DOMAIN == 40)
= assert_norm (FStar.List.Tot.length challenge_context_domain_list == 40);
  assert (C.v_CHALLENGE_CONTEXT_DOMAIN == Seq.seq_of_list challenge_context_domain_list)

let label_len (stage_label:t_Slice u8) : u32 =
  cast (Core_models.Slice.impl__len #u8 stage_label) <: u32

let challenge_spec
      (context:C.t_ChallengeContext)
      (stage_label:t_Slice u8)
      (nonce:u64)
      (messages:t_Slice u8)
    : Seq.seq u8 =
  Seq.append C.v_CHALLENGE_DOMAIN
    (Seq.append (Num.impl_u32__to_le_bytes (label_len stage_label))
      (Seq.append stage_label
        (Seq.append (C.impl_ChallengeContext__as_bytes context)
          (Seq.append (Num.impl_u64__to_le_bytes nonce) messages))))

let lemma_usize_to_u32_ok (len:usize)
    : Lemma
      (requires Int.v len <= 4294967295)
      (ensures
        Core_models.Convert.f_try_from #u32 #usize #FStar.Tactics.Typeclasses.solve len ==
          Core_models.Result.Result_Ok (cast len <: u32) /\
        Int.v (cast len <: u32) == Int.v len)
= ()

let smoke_challenge_context_is_extracted
      (context:C.t_ChallengeContext)
    : t_Array u8 (mk_usize 32) =
  C.impl_ChallengeContext__as_bytes context

let smoke_challenge_context_preimage_is_extracted
      (statement_digest:t_Array u8 (mk_usize 32))
    : Alloc.Vec.t_Vec u8 Alloc.Alloc.t_Global =
  C.challenge_context_preimage statement_digest

let smoke_challenge_preimage_is_extracted
      (context:C.t_ChallengeContext)
      (stage_label:t_Slice u8)
      (nonce:u64)
      (messages:t_Slice u8)
    : Alloc.Vec.t_Vec u8 Alloc.Alloc.t_Global =
  C.challenge_preimage context stage_label nonce messages

let lemma_challenge_context_bytes_injective
      (left:C.t_ChallengeContext)
      (right:C.t_ChallengeContext)
    : Lemma
      (requires (C.impl_ChallengeContext__as_bytes left == C.impl_ChallengeContext__as_bytes right))
      (ensures (left == right))
= ()

let lemma_challenge_context_preimage_layout
      (statement_digest:t_Array u8 (mk_usize 32))
    : Lemma
      (ensures
        bo (C.challenge_context_preimage statement_digest) ==
        challenge_context_spec statement_digest)
= let b0 = V.impl__new #u8 () in
  lemma_challenge_context_domain_len ();
  let b1 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b0 C.v_CHALLENGE_CONTEXT_DOMAIN in
  let b2 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b1 (statement_digest <: t_Slice u8) in
  Seq.append_empty_l C.v_CHALLENGE_CONTEXT_DOMAIN;
  assert (bo b2 == challenge_context_spec statement_digest)

#push-options "--fuel 20 --ifuel 2 --z3rlimit 400"

let lemma_challenge_preimage_layout
      (context:C.t_ChallengeContext)
      (stage_label:t_Slice u8)
      (nonce:u64)
      (messages:t_Slice u8)
    : Lemma
      (requires
        Int.v (Core_models.Slice.impl__len #u8 stage_label) <= 4294967295 /\
        Seq.length C.v_CHALLENGE_DOMAIN + 4 + Seq.length stage_label + 32 + 8 +
          Seq.length messages <= Int.max_usize)
      (ensures
        bo (C.challenge_preimage context stage_label nonce messages) ==
        challenge_spec context stage_label nonce messages)
= lemma_usize_to_u32_ok (Core_models.Slice.impl__len #u8 stage_label);
  let stage_label_len = label_len stage_label in
  let cap =
    ((((((Core_models.Slice.impl__len #u8 C.v_CHALLENGE_DOMAIN <: usize)
        +! mk_usize 4 <: usize)
      +! (Core_models.Slice.impl__len #u8 stage_label <: usize) <: usize)
      +! mk_usize 32 <: usize)
      +! mk_usize 8 <: usize)
      +! (Core_models.Slice.impl__len #u8 messages <: usize) <: usize)
  in
  let b0 = V.impl__with_capacity #u8 cap in
  let b1 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global b0 C.v_CHALLENGE_DOMAIN in
  let b2 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b1 (Num.impl_u32__to_le_bytes stage_label_len <: t_Slice u8) in
  let b3 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global b2 stage_label in
  let b4 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b3 (C.impl_ChallengeContext__as_bytes context <: t_Slice u8) in
  let b5 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global
    b4 (Num.impl_u64__to_le_bytes nonce <: t_Slice u8) in
  let b6 = V.impl_2__extend_from_slice #u8 #Alloc.Alloc.t_Global b5 messages in
  Seq.append_empty_l C.v_CHALLENGE_DOMAIN;
  Seq.append_assoc C.v_CHALLENGE_DOMAIN
    (Num.impl_u32__to_le_bytes stage_label_len)
    (Seq.append stage_label
      (Seq.append (C.impl_ChallengeContext__as_bytes context)
        (Seq.append (Num.impl_u64__to_le_bytes nonce) messages)));
  Seq.append_assoc
    (Seq.append C.v_CHALLENGE_DOMAIN (Num.impl_u32__to_le_bytes stage_label_len))
    stage_label
    (Seq.append (C.impl_ChallengeContext__as_bytes context)
      (Seq.append (Num.impl_u64__to_le_bytes nonce) messages));
  Seq.append_assoc
    (Seq.append
      (Seq.append C.v_CHALLENGE_DOMAIN (Num.impl_u32__to_le_bytes stage_label_len))
      stage_label)
    (C.impl_ChallengeContext__as_bytes context)
    (Seq.append (Num.impl_u64__to_le_bytes nonce) messages);
  Seq.append_assoc
    (Seq.append
      (Seq.append
        (Seq.append C.v_CHALLENGE_DOMAIN (Num.impl_u32__to_le_bytes stage_label_len))
        stage_label)
      (C.impl_ChallengeContext__as_bytes context))
    (Num.impl_u64__to_le_bytes nonce)
    messages;
  assert (bo b6 == challenge_spec context stage_label nonce messages)

#pop-options

let lemma_challenge_spec_injective
      (left_context:C.t_ChallengeContext)
      (left_label:t_Slice u8)
      (left_nonce:u64)
      (left_messages:t_Slice u8)
      (right_context:C.t_ChallengeContext)
      (right_label:t_Slice u8)
      (right_nonce:u64)
      (right_messages:t_Slice u8)
    : Lemma
      (requires
        Int.v (Core_models.Slice.impl__len #u8 left_label) <= 4294967295 /\
        Int.v (Core_models.Slice.impl__len #u8 right_label) <= 4294967295 /\
        challenge_spec left_context left_label left_nonce left_messages ==
        challenge_spec right_context right_label right_nonce right_messages)
      (ensures
        left_context == right_context /\
        left_label == right_label /\
        left_nonce == right_nonce /\
        left_messages == right_messages)
= let left_tail =
    Seq.append (Num.impl_u32__to_le_bytes (label_len left_label))
      (Seq.append left_label
        (Seq.append (C.impl_ChallengeContext__as_bytes left_context)
          (Seq.append (Num.impl_u64__to_le_bytes left_nonce) left_messages)))
  in
  let right_tail =
    Seq.append (Num.impl_u32__to_le_bytes (label_len right_label))
      (Seq.append right_label
        (Seq.append (C.impl_ChallengeContext__as_bytes right_context)
          (Seq.append (Num.impl_u64__to_le_bytes right_nonce) right_messages)))
  in
  Seq.lemma_append_inj C.v_CHALLENGE_DOMAIN left_tail C.v_CHALLENGE_DOMAIN right_tail;
  lemma_usize_to_u32_ok (Core_models.Slice.impl__len #u8 left_label);
  lemma_usize_to_u32_ok (Core_models.Slice.impl__len #u8 right_label);
  Seq.append_assoc
    (Num.impl_u32__to_le_bytes (label_len left_label))
    left_label
    (Seq.append (C.impl_ChallengeContext__as_bytes left_context)
      (Seq.append (Num.impl_u64__to_le_bytes left_nonce) left_messages));
  Seq.append_assoc
    (Num.impl_u32__to_le_bytes (label_len right_label))
    right_label
    (Seq.append (C.impl_ChallengeContext__as_bytes right_context)
      (Seq.append (Num.impl_u64__to_le_bytes right_nonce) right_messages));
  F.lemma_lenpref_frame_inj
    (label_len left_label)
    left_label
    (Seq.append (C.impl_ChallengeContext__as_bytes left_context)
      (Seq.append (Num.impl_u64__to_le_bytes left_nonce) left_messages))
    (label_len right_label)
    right_label
    (Seq.append (C.impl_ChallengeContext__as_bytes right_context)
      (Seq.append (Num.impl_u64__to_le_bytes right_nonce) right_messages));
  F.lemma_fixed_split_inj
    (C.impl_ChallengeContext__as_bytes left_context)
    (Seq.append (Num.impl_u64__to_le_bytes left_nonce) left_messages)
    (C.impl_ChallengeContext__as_bytes right_context)
    (Seq.append (Num.impl_u64__to_le_bytes right_nonce) right_messages);
  lemma_challenge_context_bytes_injective left_context right_context;
  F.lemma_u64_value_frame_inj left_nonce left_messages right_nonce right_messages

let lemma_challenge_preimage_injective
      (left_context:C.t_ChallengeContext)
      (left_label:t_Slice u8)
      (left_nonce:u64)
      (left_messages:t_Slice u8)
      (right_context:C.t_ChallengeContext)
      (right_label:t_Slice u8)
      (right_nonce:u64)
      (right_messages:t_Slice u8)
    : Lemma
      (requires
        Int.v (Core_models.Slice.impl__len #u8 left_label) <= 4294967295 /\
        Int.v (Core_models.Slice.impl__len #u8 right_label) <= 4294967295 /\
        Seq.length C.v_CHALLENGE_DOMAIN + 4 + Seq.length left_label + 32 + 8 +
          Seq.length left_messages <= Int.max_usize /\
        Seq.length C.v_CHALLENGE_DOMAIN + 4 + Seq.length right_label + 32 + 8 +
          Seq.length right_messages <= Int.max_usize /\
        bo (C.challenge_preimage left_context left_label left_nonce left_messages) ==
        bo (C.challenge_preimage right_context right_label right_nonce right_messages))
      (ensures
        left_context == right_context /\
        left_label == right_label /\
        left_nonce == right_nonce /\
        left_messages == right_messages)
= lemma_challenge_preimage_layout left_context left_label left_nonce left_messages;
  lemma_challenge_preimage_layout right_context right_label right_nonce right_messages;
  lemma_challenge_spec_injective
    left_context left_label left_nonce left_messages
    right_context right_label right_nonce right_messages
