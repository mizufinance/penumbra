module StatementEncodingProofs
#set-options "--fuel 1 --ifuel 1 --z3rlimit 100"

module S = Penumbra_sdk_proof_aggregation.Statement
module V = Alloc.Vec
module Seq = FStar.Seq
module Num = Core_models.Num
module Int = Rust_primitives.Integers
module F = FrameLemmas
open Core_models

(*
  Proof target, extracted from Rust with hax: encode_statement is injective
  over typed StatementEncodingInput values, so the statement digest binds the
  typed statement (modulo SHA-256 collision resistance, an assumed row).

  Strategy: each append helper writes a determined byte suffix (content
  lemmas). encode_statement is therefore a concatenation of self-delimiting
  frames; injectivity follows from FrameLemmas by peeling frame by frame.
  Length-prefixed frames need the field length to fit u32, which holds exactly
  on the success path that produces the bytes we reason about.
*)

let bo (b: V.t_Vec u8 Alloc.Alloc.t_Global) : Seq.seq u8 = b._0

(* ------------------------------------------------------------------ *)
(* Leaf content lemmas                                                 *)
(* ------------------------------------------------------------------ *)

/// append_u32_field appends the constant 4-length prefix then the value.
let lemma_append_u32_field_content (bytes: V.t_Vec u8 Alloc.Alloc.t_Global) (value: u32)
    : Lemma
      (ensures
        bo (S.append_u32_field bytes value) ==
        Seq.append (bo bytes)
          (Seq.append (Num.impl_u32__to_le_bytes (mk_u32 4)) (Num.impl_u32__to_le_bytes value)))
= Seq.append_assoc (bo bytes)
    (Num.impl_u32__to_le_bytes (mk_u32 4))
    (Num.impl_u32__to_le_bytes value)

/// append_len succeeds when the length fits u32 and appends to_le_bytes(len).
let lemma_append_len_ok (bytes: V.t_Vec u8 Alloc.Alloc.t_Global) (len: usize) (field: string)
    : Lemma
      (requires Int.v len <= 4294967295)
      (ensures
        (let res = S.append_len bytes len field in
          Core_models.Result.Result_Ok? (snd res) /\
          Int.v (cast len <: u32) == Int.v len /\
          bo (fst res) == Seq.append (bo bytes) (Num.impl_u32__to_le_bytes (cast len <: u32))))
= ()

/// append_bytes_field succeeds when the field fits u32 and appends a
/// length-prefixed frame to_le_bytes(len) || field.
let lemma_append_bytes_field_ok (bytes: V.t_Vec u8 Alloc.Alloc.t_Global) (field: t_Slice u8)
    : Lemma
      (requires Seq.length field <= 4294967295)
      (ensures
        (let res = S.append_bytes_field bytes field in
          Core_models.Result.Result_Ok? (snd res) /\
          Int.v (cast (Core_models.Slice.impl__len #u8 field) <: u32) == Seq.length field /\
          bo (fst res) ==
          Seq.append (bo bytes)
            (Seq.append (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 field) <: u32))
              field)))
= lemma_append_len_ok bytes (Core_models.Slice.impl__len #u8 field) "bytes_field";
  Seq.append_assoc (bo bytes)
    (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 field) <: u32))
    field

/// A u8 Vec is determined by its underlying byte sequence.
let vec_eq_from_seq (a b: V.t_Vec u8 Alloc.Alloc.t_Global)
    : Lemma (requires a._0 == b._0) (ensures a == b)
= ()

(* ------------------------------------------------------------------ *)
(* Field sequence: spec, content, injectivity                          *)
(* ------------------------------------------------------------------ *)

let field_bytes (f: S.t_StatementFieldBytes) : t_Slice u8 = f.f_bytes._0

let wf_field (f: S.t_StatementFieldBytes) : prop = Seq.length (field_bytes f) <= 4294967295

let rec wf_fields (fields: t_Slice S.t_StatementFieldBytes)
    : Tot prop (decreases Seq.length fields) =
  if Seq.length fields = 0
  then True
  else wf_field (Seq.index fields 0) /\ wf_fields (Seq.slice fields 1 (Seq.length fields))

/// Byte suffix appended by encode_fields: a sequence of length-prefixed frames.
let rec spec_fields (fields: t_Slice S.t_StatementFieldBytes {wf_fields fields})
    : Tot (Seq.seq u8) (decreases Seq.length fields) =
  if Seq.length fields = 0
  then Seq.empty
  else
    let head = Seq.index fields 0 in
    let rest = Seq.slice fields 1 (Seq.length fields) in
    Seq.append
      (Seq.append
        (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 (field_bytes head)) <: u32))
        (field_bytes head))
      (spec_fields rest)

let rec lemma_encode_fields_content
      (bytes: V.t_Vec u8 Alloc.Alloc.t_Global)
      (fields: t_Slice S.t_StatementFieldBytes {wf_fields fields})
    : Lemma
      (ensures
        Core_models.Result.Result_Ok? (snd (S.encode_fields bytes fields)) /\
        bo (fst (S.encode_fields bytes fields)) == Seq.append (bo bytes) (spec_fields fields))
      (decreases Seq.length fields)
= if Seq.length fields = 0
  then Seq.append_empty_r (bo bytes)
  else begin
    let head = Seq.index fields 0 in
    let rest = Seq.slice fields 1 (Seq.length fields) in
    lemma_append_bytes_field_ok bytes (field_bytes head);
    let b1 = fst (S.append_bytes_field bytes (field_bytes head)) in
    lemma_encode_fields_content b1 rest;
    Seq.append_assoc (bo bytes)
      (Seq.append
        (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 (field_bytes head)) <: u32))
        (field_bytes head))
      (spec_fields rest)
  end

/// Reconstruct a field slice from its head and tail (split at 1).
let lemma_fields_cons (fields: t_Slice S.t_StatementFieldBytes)
    : Lemma
      (requires Seq.length fields > 0)
      (ensures
        fields ==
        Seq.append (Seq.slice fields 0 1) (Seq.slice fields 1 (Seq.length fields)))
= Seq.lemma_split fields 1

let rec lemma_spec_fields_inj
      (fa: t_Slice S.t_StatementFieldBytes {wf_fields fa})
      (fb: t_Slice S.t_StatementFieldBytes {wf_fields fb})
      (ra rb: Seq.seq u8)
    : Lemma
      (requires
        Seq.length fa == Seq.length fb /\
        Seq.append (spec_fields fa) ra == Seq.append (spec_fields fb) rb)
      (ensures fa == fb /\ ra == rb)
      (decreases Seq.length fa)
= if Seq.length fa = 0
  then begin
    Seq.append_empty_l ra; Seq.append_empty_l rb;
    assert (Seq.equal fa fb)
  end
  else begin
    let ha = Seq.index fa 0 in
    let hb = Seq.index fb 0 in
    let resta = Seq.slice fa 1 (Seq.length fa) in
    let restb = Seq.slice fb 1 (Seq.length fb) in
    // spec_fa = (to_le(len ha) @ bytes ha) @ spec resta ; regroup with ra
    Seq.append_assoc
      (Seq.append (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 (field_bytes ha)) <: u32)) (field_bytes ha))
      (spec_fields resta) ra;
    Seq.append_assoc
      (Seq.append (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 (field_bytes hb)) <: u32)) (field_bytes hb))
      (spec_fields restb) rb;
    F.lemma_lenpref_frame_inj
      (cast (Core_models.Slice.impl__len #u8 (field_bytes ha)) <: u32) (field_bytes ha) (Seq.append (spec_fields resta) ra)
      (cast (Core_models.Slice.impl__len #u8 (field_bytes hb)) <: u32) (field_bytes hb) (Seq.append (spec_fields restb) rb);
    // field_bytes ha == field_bytes hb  =>  ha == hb
    vec_eq_from_seq ha.f_bytes hb.f_bytes;
    lemma_spec_fields_inj resta restb ra rb;
    lemma_fields_cons fa; lemma_fields_cons fb;
    assert (Seq.equal (Seq.slice fa 0 1) (Seq.slice fb 0 1))
  end

(* ------------------------------------------------------------------ *)
(* Row sequence: spec, content, injectivity                            *)
(* ------------------------------------------------------------------ *)

let row_fields (r: S.t_StatementPublicInputRow) : t_Slice S.t_StatementFieldBytes = r.f_fields._0

let wf_row (r: S.t_StatementPublicInputRow) : prop =
  Seq.length (row_fields r) <= 4294967295 /\ wf_fields (row_fields r)

let rec wf_rows (rows: t_Slice S.t_StatementPublicInputRow)
    : Tot prop (decreases Seq.length rows) =
  if Seq.length rows = 0
  then True
  else wf_row (Seq.index rows 0) /\ wf_rows (Seq.slice rows 1 (Seq.length rows))

let row_arity_le (r: S.t_StatementPublicInputRow {wf_row r}) : u32 =
  cast (Core_models.Slice.impl__len #S.t_StatementFieldBytes (row_fields r)) <: u32

let spec_row (r: S.t_StatementPublicInputRow {wf_row r}) : Seq.seq u8 =
  Seq.append (Num.impl_u32__to_le_bytes (row_arity_le r)) (spec_fields (row_fields r))

let rec spec_rows (rows: t_Slice S.t_StatementPublicInputRow {wf_rows rows})
    : Tot (Seq.seq u8) (decreases Seq.length rows) =
  if Seq.length rows = 0
  then Seq.empty
  else
    let head = Seq.index rows 0 in
    let rest = Seq.slice rows 1 (Seq.length rows) in
    Seq.append (spec_row head) (spec_rows rest)

/// A field-Vec is determined by its underlying sequence; hence so is a row.
let row_eq_from_fields (a b: S.t_StatementPublicInputRow)
    : Lemma (requires a.f_fields._0 == b.f_fields._0) (ensures a == b)
= ()

let lemma_encode_row_content
      (bytes: V.t_Vec u8 Alloc.Alloc.t_Global)
      (row: S.t_StatementPublicInputRow {wf_row row})
    : Lemma
      (ensures
        Core_models.Result.Result_Ok? (snd (S.encode_row bytes row)) /\
        bo (fst (S.encode_row bytes row)) == Seq.append (bo bytes) (spec_row row))
= lemma_append_len_ok bytes
    (Core_models.Slice.impl__len #S.t_StatementFieldBytes (row_fields row)) "row_arity";
  let b1 = fst (S.append_len bytes
    (Core_models.Slice.impl__len #S.t_StatementFieldBytes (row_fields row)) "row_arity") in
  lemma_encode_fields_content b1 (row_fields row);
  Seq.append_assoc (bo bytes)
    (Num.impl_u32__to_le_bytes (row_arity_le row))
    (spec_fields (row_fields row))

let rec lemma_encode_rows_content
      (bytes: V.t_Vec u8 Alloc.Alloc.t_Global)
      (rows: t_Slice S.t_StatementPublicInputRow {wf_rows rows})
    : Lemma
      (ensures
        Core_models.Result.Result_Ok? (snd (S.encode_rows bytes rows)) /\
        bo (fst (S.encode_rows bytes rows)) == Seq.append (bo bytes) (spec_rows rows))
      (decreases Seq.length rows)
= if Seq.length rows = 0
  then Seq.append_empty_r (bo bytes)
  else begin
    let head = Seq.index rows 0 in
    let rest = Seq.slice rows 1 (Seq.length rows) in
    lemma_encode_row_content bytes head;
    let b1 = fst (S.encode_row bytes head) in
    lemma_encode_rows_content b1 rest;
    Seq.append_assoc (bo bytes) (spec_row head) (spec_rows rest)
  end

let lemma_rows_cons (rows: t_Slice S.t_StatementPublicInputRow)
    : Lemma
      (requires Seq.length rows > 0)
      (ensures rows == Seq.append (Seq.slice rows 0 1) (Seq.slice rows 1 (Seq.length rows)))
= Seq.lemma_split rows 1

/// A single row frame is self-delimiting (arity is length-prefixed).
let lemma_spec_row_inj
      (ra: S.t_StatementPublicInputRow {wf_row ra})
      (rb: S.t_StatementPublicInputRow {wf_row rb})
      (ta tb: Seq.seq u8)
    : Lemma
      (requires Seq.append (spec_row ra) ta == Seq.append (spec_row rb) tb)
      (ensures ra == rb /\ ta == tb)
= Seq.append_assoc (Num.impl_u32__to_le_bytes (row_arity_le ra)) (spec_fields (row_fields ra)) ta;
  Seq.append_assoc (Num.impl_u32__to_le_bytes (row_arity_le rb)) (spec_fields (row_fields rb)) tb;
  F.lemma_u32_value_frame_inj
    (row_arity_le ra) (Seq.append (spec_fields (row_fields ra)) ta)
    (row_arity_le rb) (Seq.append (spec_fields (row_fields rb)) tb);
  lemma_spec_fields_inj (row_fields ra) (row_fields rb) ta tb;
  row_eq_from_fields ra rb

let rec lemma_spec_rows_inj
      (rowsa: t_Slice S.t_StatementPublicInputRow {wf_rows rowsa})
      (rowsb: t_Slice S.t_StatementPublicInputRow {wf_rows rowsb})
      (ta tb: Seq.seq u8)
    : Lemma
      (requires
        Seq.length rowsa == Seq.length rowsb /\
        Seq.append (spec_rows rowsa) ta == Seq.append (spec_rows rowsb) tb)
      (ensures rowsa == rowsb /\ ta == tb)
      (decreases Seq.length rowsa)
= if Seq.length rowsa = 0
  then begin
    Seq.append_empty_l ta; Seq.append_empty_l tb;
    assert (Seq.equal rowsa rowsb)
  end
  else begin
    let ha = Seq.index rowsa 0 in
    let hb = Seq.index rowsb 0 in
    let resta = Seq.slice rowsa 1 (Seq.length rowsa) in
    let restb = Seq.slice rowsb 1 (Seq.length rowsb) in
    Seq.append_assoc (spec_row ha) (spec_rows resta) ta;
    Seq.append_assoc (spec_row hb) (spec_rows restb) tb;
    lemma_spec_row_inj ha hb (Seq.append (spec_rows resta) ta) (Seq.append (spec_rows restb) tb);
    lemma_spec_rows_inj resta restb ta tb;
    lemma_rows_cons rowsa; lemma_rows_cons rowsb;
    assert (Seq.equal (Seq.slice rowsa 0 1) (Seq.slice rowsb 0 1))
  end

(* ------------------------------------------------------------------ *)
(* Statement: well-formedness, spec, content, injectivity              *)
(* ------------------------------------------------------------------ *)

let stmt_rows (x: S.t_StatementEncodingInput) : t_Slice S.t_StatementPublicInputRow =
  x.f_padded_public_inputs.f_rows._0

let wf_input (x: S.t_StatementEncodingInput) : prop =
  Seq.length (x.f_curve_id._0) <= 4294967295 /\
  Seq.length (x.f_backend_id._0) <= 4294967295 /\
  Seq.length (stmt_rows x) <= 4294967295 /\
  wf_rows (stmt_rows x)

let atom_u32 (value: u32) : Seq.seq u8 =
  Seq.append (Num.impl_u32__to_le_bytes (mk_u32 4)) (Num.impl_u32__to_le_bytes value)

let atom_bytes (f: t_Slice u8 {Seq.length f <= 4294967295}) : Seq.seq u8 =
  Seq.append (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #u8 f) <: u32)) f

/// A fixed 32-byte array is a slice whose length trivially fits u32.
let arr32_as_slice (a: t_Array u8 (mk_usize 32)) : (f: t_Slice u8 {Seq.length f <= 4294967295}) = a

/// The 47 bytes of the padding-rule domain separator, matching the hax model
/// of v_PADDING_RULE_DOMAIN (`array_of_list 47 padding_list`).
let padding_list : list u8 =
  [
    mk_u8 112; mk_u8 101; mk_u8 110; mk_u8 117; mk_u8 109; mk_u8 98; mk_u8 114; mk_u8 97;
    mk_u8 46; mk_u8 115; mk_u8 110; mk_u8 97; mk_u8 114; mk_u8 107; mk_u8 112; mk_u8 97;
    mk_u8 99; mk_u8 107; mk_u8 46; mk_u8 112; mk_u8 97; mk_u8 100; mk_u8 100; mk_u8 105;
    mk_u8 110; mk_u8 103; mk_u8 46; mk_u8 114; mk_u8 101; mk_u8 112; mk_u8 101; mk_u8 97;
    mk_u8 116; mk_u8 45; mk_u8 102; mk_u8 105; mk_u8 110; mk_u8 97; mk_u8 108; mk_u8 45;
    mk_u8 114; mk_u8 111; mk_u8 119; mk_u8 46; mk_u8 118; mk_u8 49; mk_u8 0
  ]

/// The padding-rule domain separator is a fixed 47-byte slice. Its hax model
/// unfolds (the let is reducible, array_of_list is `unfold`) to
/// `seq_of_list padding_list`, whose return type gives
/// `length s == List.length padding_list`; the latter normalizes to 47.
let lemma_padding_len ()
    : Lemma (Seq.length Penumbra_sdk_proof_aggregation.Padding.v_PADDING_RULE_DOMAIN == 47)
= assert_norm (FStar.List.Tot.length padding_list == 47);
  assert (Penumbra_sdk_proof_aggregation.Padding.v_PADDING_RULE_DOMAIN ==
          Seq.seq_of_list padding_list)

let padding_domain : (f: t_Slice u8 {Seq.length f <= 4294967295}) =
  lemma_padding_len ();
  Penumbra_sdk_proof_aggregation.Padding.v_PADDING_RULE_DOMAIN

/// Right-nested concatenation of a list of byte segments.
let rec nest_right (l: list (Seq.seq u8)) : Seq.seq u8 =
  match l with
  | [] -> Seq.empty
  | a :: tl -> Seq.append a (nest_right tl)

/// Left-accumulated concatenation, matching the encoder's append threading.
let rec fold_left_app (acc: Seq.seq u8) (l: list (Seq.seq u8)) : Tot (Seq.seq u8) (decreases l) =
  match l with
  | [] -> acc
  | a :: tl -> fold_left_app (Seq.append acc a) tl

/// The encoder's left accumulation equals the right-nested concatenation.
let rec lemma_fold_nest (acc: Seq.seq u8) (l: list (Seq.seq u8))
    : Lemma (ensures fold_left_app acc l == Seq.append acc (nest_right l)) (decreases l)
= match l with
  | [] -> Seq.append_empty_r acc
  | a :: tl ->
    Seq.append_assoc acc a (nest_right tl);
    lemma_fold_nest (Seq.append acc a) tl

/// The self-delimiting frames encode_statement appends, in order.
let atoms (x: S.t_StatementEncodingInput {wf_input x}) : list (Seq.seq u8) =
  [
    atom_u32 x.f_version;
    atom_bytes x.f_curve_id._0;
    atom_bytes x.f_backend_id._0;
    atom_bytes padding_domain;
    atom_u32 x.f_proof_family_id;
    atom_u32 x.f_consolidate_family_id;
    atom_u32 x.f_split_family_id;
    atom_u32 x.f_shielded_ics20_withdrawal_family_id;
    atom_bytes (arr32_as_slice x.f_srs_id);
    atom_bytes (arr32_as_slice x.f_vk_digest);
    atom_u32 x.f_real_count;
    atom_u32 x.f_padded_count;
    atom_u32 x.f_public_input_arity;
    Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #S.t_StatementPublicInputRow (stmt_rows x)) <: u32);
    spec_rows (stmt_rows x)
  ]

let spec_statement (x: S.t_StatementEncodingInput {wf_input x}) : Seq.seq u8 =
  Seq.append (atom_u32 x.f_version)
  (Seq.append (atom_bytes x.f_curve_id._0)
  (Seq.append (atom_bytes x.f_backend_id._0)
  (Seq.append (atom_bytes padding_domain)
  (Seq.append (atom_u32 x.f_proof_family_id)
  (Seq.append (atom_u32 x.f_consolidate_family_id)
  (Seq.append (atom_u32 x.f_split_family_id)
  (Seq.append (atom_u32 x.f_shielded_ics20_withdrawal_family_id)
  (Seq.append (atom_bytes (arr32_as_slice x.f_srs_id))
  (Seq.append (atom_bytes (arr32_as_slice x.f_vk_digest))
  (Seq.append (atom_u32 x.f_real_count)
  (Seq.append (atom_u32 x.f_padded_count)
  (Seq.append (atom_u32 x.f_public_input_arity)
  (Seq.append
    (Num.impl_u32__to_le_bytes (cast (Core_models.Slice.impl__len #S.t_StatementPublicInputRow (stmt_rows x)) <: u32))
    (spec_rows (stmt_rows x)))))))))))))))

(* ------------------------------------------------------------------ *)
(* Statement content and injectivity                                   *)
(* ------------------------------------------------------------------ *)

#push-options "--fuel 20 --ifuel 2 --z3rlimit 200"

/// spec_statement is the right-nested concatenation of the atom frames.
let lemma_spec_is_nest (x: S.t_StatementEncodingInput {wf_input x})
    : Lemma (spec_statement x == nest_right (atoms x))
= Seq.append_empty_r (spec_rows (stmt_rows x))

/// encode_statement succeeds on well-formed input and emits spec_statement.
let lemma_encode_statement_content (x: S.t_StatementEncodingInput {wf_input x})
    : Lemma
      (ensures
        Core_models.Result.Result_Ok? (S.encode_statement x) /\
        bo (Core_models.Result.Result_Ok?._0 (S.encode_statement x)) == spec_statement x)
= let b0 = Alloc.Vec.impl__new #u8 () in
  let b1 = S.append_u32_field b0 x.f_version in
  lemma_append_u32_field_content b0 x.f_version;
  let b2 = fst (S.append_bytes_field b1 (Alloc.Vec.impl_1__as_slice x.f_curve_id)) in
  lemma_append_bytes_field_ok b1 (Alloc.Vec.impl_1__as_slice x.f_curve_id);
  let b3 = fst (S.append_bytes_field b2 (Alloc.Vec.impl_1__as_slice x.f_backend_id)) in
  lemma_append_bytes_field_ok b2 (Alloc.Vec.impl_1__as_slice x.f_backend_id);
  lemma_padding_len ();
  let b4 = fst (S.append_bytes_field b3 Penumbra_sdk_proof_aggregation.Padding.v_PADDING_RULE_DOMAIN) in
  lemma_append_bytes_field_ok b3 Penumbra_sdk_proof_aggregation.Padding.v_PADDING_RULE_DOMAIN;
  let b5 = S.append_u32_field b4 x.f_proof_family_id in
  lemma_append_u32_field_content b4 x.f_proof_family_id;
  let b6 = S.append_u32_field b5 x.f_consolidate_family_id in
  lemma_append_u32_field_content b5 x.f_consolidate_family_id;
  let b7 = S.append_u32_field b6 x.f_split_family_id in
  lemma_append_u32_field_content b6 x.f_split_family_id;
  let b8 = S.append_u32_field b7 x.f_shielded_ics20_withdrawal_family_id in
  lemma_append_u32_field_content b7 x.f_shielded_ics20_withdrawal_family_id;
  let b9 = fst (S.append_bytes_field b8 x.f_srs_id) in
  lemma_append_bytes_field_ok b8 x.f_srs_id;
  let b10 = fst (S.append_bytes_field b9 x.f_vk_digest) in
  lemma_append_bytes_field_ok b9 x.f_vk_digest;
  let b11 = S.append_u32_field b10 x.f_real_count in
  lemma_append_u32_field_content b10 x.f_real_count;
  let b12 = S.append_u32_field b11 x.f_padded_count in
  lemma_append_u32_field_content b11 x.f_padded_count;
  let b13 = S.append_u32_field b12 x.f_public_input_arity in
  lemma_append_u32_field_content b12 x.f_public_input_arity;
  let b14 = fst (S.append_len b13
    (S.impl_StatementPaddedRows__len x.f_padded_public_inputs) "row_count") in
  lemma_append_len_ok b13 (S.impl_StatementPaddedRows__len x.f_padded_public_inputs) "row_count";
  lemma_encode_rows_content b14 (S.impl_StatementPaddedRows__as_slice x.f_padded_public_inputs);
  lemma_fold_nest Seq.empty (atoms x);
  Seq.append_empty_l (nest_right (atoms x));
  lemma_spec_is_nest x

#pop-options

(* Peel one fixed 8-byte u32 atom (4-byte length tag + value) from the front. *)
let peel_u32 (va: u32) (ra: Seq.seq u8) (vb: u32) (rb: Seq.seq u8)
    : Lemma
      (requires Seq.append (atom_u32 va) ra == Seq.append (atom_u32 vb) rb)
      (ensures va == vb /\ ra == rb)
= Seq.append_assoc (Num.impl_u32__to_le_bytes (mk_u32 4)) (Num.impl_u32__to_le_bytes va) ra;
  Seq.append_assoc (Num.impl_u32__to_le_bytes (mk_u32 4)) (Num.impl_u32__to_le_bytes vb) rb;
  F.lemma_u32_value_frame_inj (mk_u32 4) (Seq.append (Num.impl_u32__to_le_bytes va) ra)
                              (mk_u32 4) (Seq.append (Num.impl_u32__to_le_bytes vb) rb);
  F.lemma_u32_value_frame_inj va ra vb rb

(* Peel one length-prefixed bytes atom from the front. *)
let peel_bytes
      (fa: t_Slice u8 {Seq.length fa <= 4294967295}) (ra: Seq.seq u8)
      (fb: t_Slice u8 {Seq.length fb <= 4294967295}) (rb: Seq.seq u8)
    : Lemma
      (requires Seq.append (atom_bytes fa) ra == Seq.append (atom_bytes fb) rb)
      (ensures fa == fb /\ ra == rb)
= F.lemma_lenpref_frame_inj
    (cast (Core_models.Slice.impl__len #u8 fa) <: u32) fa ra
    (cast (Core_models.Slice.impl__len #u8 fb) <: u32) fb rb

let paddedrows_eq (a b: S.t_StatementPaddedRows)
    : Lemma (requires a.f_rows._0 == b.f_rows._0) (ensures a == b)
= ()

#push-options "--fuel 2 --ifuel 1 --z3rlimit 400"

/// encode_statement is injective: equal byte encodings force equal typed
/// statements. This is the byte-level binding the digest relies on.
let lemma_encode_statement_injective (x y: S.t_StatementEncodingInput)
    : Lemma
      (requires wf_input x /\ wf_input y /\ spec_statement x == spec_statement y)
      (ensures x == y)
= let rcx : u32 = cast (Core_models.Slice.impl__len #S.t_StatementPublicInputRow (stmt_rows x)) in
  let rcy : u32 = cast (Core_models.Slice.impl__len #S.t_StatementPublicInputRow (stmt_rows y)) in
  // right-nested tails, atom by atom (15 == spec_rows tail, 1 == whole)
  let t15x = spec_rows (stmt_rows x) in
  let t15y = spec_rows (stmt_rows y) in
  let t14x = Seq.append (Num.impl_u32__to_le_bytes rcx) t15x in
  let t14y = Seq.append (Num.impl_u32__to_le_bytes rcy) t15y in
  let t13x = Seq.append (atom_u32 x.f_public_input_arity) t14x in
  let t13y = Seq.append (atom_u32 y.f_public_input_arity) t14y in
  let t12x = Seq.append (atom_u32 x.f_padded_count) t13x in
  let t12y = Seq.append (atom_u32 y.f_padded_count) t13y in
  let t11x = Seq.append (atom_u32 x.f_real_count) t12x in
  let t11y = Seq.append (atom_u32 y.f_real_count) t12y in
  let t10x = Seq.append (atom_bytes (arr32_as_slice x.f_vk_digest)) t11x in
  let t10y = Seq.append (atom_bytes (arr32_as_slice y.f_vk_digest)) t11y in
  let t9x = Seq.append (atom_bytes (arr32_as_slice x.f_srs_id)) t10x in
  let t9y = Seq.append (atom_bytes (arr32_as_slice y.f_srs_id)) t10y in
  let t8x = Seq.append (atom_u32 x.f_shielded_ics20_withdrawal_family_id) t9x in
  let t8y = Seq.append (atom_u32 y.f_shielded_ics20_withdrawal_family_id) t9y in
  let t7x = Seq.append (atom_u32 x.f_split_family_id) t8x in
  let t7y = Seq.append (atom_u32 y.f_split_family_id) t8y in
  let t6x = Seq.append (atom_u32 x.f_consolidate_family_id) t7x in
  let t6y = Seq.append (atom_u32 y.f_consolidate_family_id) t7y in
  let t5x = Seq.append (atom_u32 x.f_proof_family_id) t6x in
  let t5y = Seq.append (atom_u32 y.f_proof_family_id) t6y in
  let t4x = Seq.append (atom_bytes padding_domain) t5x in
  let t4y = Seq.append (atom_bytes padding_domain) t5y in
  let t3x = Seq.append (atom_bytes x.f_backend_id._0) t4x in
  let t3y = Seq.append (atom_bytes y.f_backend_id._0) t4y in
  let t2x = Seq.append (atom_bytes x.f_curve_id._0) t3x in
  let t2y = Seq.append (atom_bytes y.f_curve_id._0) t3y in
  // spec_statement x == t1x definitionally; peel front to back
  peel_u32 x.f_version t2x y.f_version t2y;
  peel_bytes x.f_curve_id._0 t3x y.f_curve_id._0 t3y;
  peel_bytes x.f_backend_id._0 t4x y.f_backend_id._0 t4y;
  peel_bytes padding_domain t5x padding_domain t5y;
  peel_u32 x.f_proof_family_id t6x y.f_proof_family_id t6y;
  peel_u32 x.f_consolidate_family_id t7x y.f_consolidate_family_id t7y;
  peel_u32 x.f_split_family_id t8x y.f_split_family_id t8y;
  peel_u32 x.f_shielded_ics20_withdrawal_family_id t9x y.f_shielded_ics20_withdrawal_family_id t9y;
  peel_bytes (arr32_as_slice x.f_srs_id) t10x (arr32_as_slice y.f_srs_id) t10y;
  peel_bytes (arr32_as_slice x.f_vk_digest) t11x (arr32_as_slice y.f_vk_digest) t11y;
  peel_u32 x.f_real_count t12x y.f_real_count t12y;
  peel_u32 x.f_padded_count t13x y.f_padded_count t13y;
  peel_u32 x.f_public_input_arity t14x y.f_public_input_arity t14y;
  // now t14x == t14y: peel the bare row-count u32, leaving spec_rows == spec_rows
  F.lemma_u32_value_frame_inj rcx t15x rcy t15y;
  // rcx == rcy and lengths fit u32, so the row counts match
  assert (Seq.length (stmt_rows x) == Seq.length (stmt_rows y));
  Seq.append_empty_r t15x;
  Seq.append_empty_r t15y;
  lemma_spec_rows_inj (stmt_rows x) (stmt_rows y) Seq.empty Seq.empty;
  // reassemble typed equality
  vec_eq_from_seq x.f_curve_id y.f_curve_id;
  vec_eq_from_seq x.f_backend_id y.f_backend_id;
  paddedrows_eq x.f_padded_public_inputs y.f_padded_public_inputs

#pop-options
