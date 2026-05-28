module ChallengePreimageProofs

module C = Ark_ip_proofs.Challenge
open Core_models

(*
  Proof target, extracted from Rust with hax:

  - challenge_preimage(stage, context, nonce, messages) is injective.
  - challenge preimages contain the fixed challenge domain, stage length,
    stage bytes, context bytes, u64 little-endian nonce, and messages in order.

  Status: deferred until lemmas are implemented against the extracted F* modules.
*)

let smoke_challenge_context_is_extracted
      (context:C.t_ChallengeContext)
    : t_Array u8 (mk_usize 32) =
  C.impl_ChallengeContext__as_bytes context

let smoke_challenge_preimage_is_extracted
      (context:C.t_ChallengeContext)
      (stage_label:t_Slice u8)
      (nonce:u64)
      (messages:t_Slice u8)
    : Alloc.Vec.t_Vec u8 Alloc.Alloc.t_Global =
  C.challenge_preimage context stage_label nonce messages
