/-!
Paper/spec-derived SnarkPack transcript-shape oracle.

This executable Lean model intentionally keeps pairing and field arithmetic
abstract. It derives only the trace/folding structure from
`docs/snarkpack/ripp-spec.md`: one statement-bound context event, the abstract
RIPP/TIPA/Groth16 equation roles, and the Fiat-Shamir challenge sequence for the
Groth16 randomizer, AB GIPA rounds, AB KZG opening, C/SSM GIPA rounds, and C/SSM
KZG opening.
-/

inductive Level where
  | penumbraByte
  | abstractTrace
deriving Repr, BEq

inductive Kind where
  | challengeContext
  | challengePreimage
  | challengeDigest
  | equationRole
deriving Repr, BEq

structure Event where
  specRowId : String
  level : Level
  kind : Kind
  stageLabel : String
  nonce : Option Nat
  abstractPayload : Option String
deriving Repr, BEq

def Level.render : Level → String
  | Level.penumbraByte => "penumbra-byte"
  | Level.abstractTrace => "abstract-trace"

def Kind.render : Kind → String
  | Kind.challengeContext => "ChallengeContext"
  | Kind.challengePreimage => "ChallengePreimage"
  | Kind.challengeDigest => "ChallengeDigest"
  | Kind.equationRole => "EquationRole"

def renderOptionNat : Option Nat → String
  | none => "-"
  | some n => toString n

def renderOptionString : Option String → String
  | none => "-"
  | some s => s

def Event.render (count : Nat) (event : Event) : String :=
  "count=" ++ toString count ++
  "|" ++ event.specRowId ++
  "|" ++ event.level.render ++
  "|" ++ event.kind.render ++
  "|" ++ event.stageLabel ++
  "|" ++ renderOptionNat event.nonce ++
  "|" ++ renderOptionString event.abstractPayload

partial def nextPowerOfTwoFrom (acc target : Nat) : Nat :=
  if acc >= target then acc else nextPowerOfTwoFrom (acc * 2) target

def nextPowerOfTwo (count : Nat) : Nat :=
  nextPowerOfTwoFrom 1 (if count == 0 then 1 else count)

partial def log2PowerOfTwoFrom (rounds power target : Nat) : Nat :=
  if power >= target then rounds else log2PowerOfTwoFrom (rounds + 1) (power * 2) target

def roundCount (count : Nat) : Nat :=
  log2PowerOfTwoFrom 0 1 (nextPowerOfTwo count)

def contextEvent : Event :=
  {
    specRowId := "fs.context-constructor",
    level := Level.penumbraByte,
    kind := Kind.challengeContext,
    stageLabel := "statement",
    nonce := none,
    abstractPayload := none
  }

def abstractEvent (row : String) : Event :=
  {
    specRowId := row,
    level := Level.abstractTrace,
    kind := Kind.equationRole,
    stageLabel := row,
    nonce := none,
    abstractPayload := some row
  }

def staticAbstractTrace : List Event :=
  [
    abstractEvent "gipa.input-relation",
    abstractEvent "gipa.round-folding",
    abstractEvent "gipa.verifier-folding",
    abstractEvent "tipa.srs",
    abstractEvent "tipa.ab.gipa",
    abstractEvent "tipa.ab.kzg-equations",
    abstractEvent "ssm.power-sequence",
    abstractEvent "ssm.base-equation",
    abstractEvent "groth16.folded-inputs",
    abstractEvent "groth16.ppe"
  ]

def challengeEvents (specRow stage : String) (nonce : Nat) : List Event :=
  [
    {
      specRowId := "fs.stage-labels",
      level := Level.penumbraByte,
      kind := Kind.challengePreimage,
      stageLabel := stage,
      nonce := some nonce,
      abstractPayload := none
    },
    {
      specRowId := "fs.challenge-preimage",
      level := Level.penumbraByte,
      kind := Kind.challengePreimage,
      stageLabel := stage,
      nonce := some nonce,
      abstractPayload := none
    },
    {
      specRowId := specRow,
      level := Level.penumbraByte,
      kind := Kind.challengeDigest,
      stageLabel := stage,
      nonce := some nonce,
      abstractPayload := none
    }
  ]

def roundChallenges (rounds : Nat) (stage : String) : List Event :=
  (List.range rounds).bind fun _round =>
    challengeEvents "gipa.challenge-dependency" stage 0

def traceForCount (count : Nat) : List Event :=
  let rounds := roundCount count
  [contextEvent] ++
  staticAbstractTrace ++
  challengeEvents "groth16.randomizer" "aggregate.randomizer" 0 ++
  roundChallenges rounds "tipa.ab.gipa.round" ++
  challengeEvents "tipa.ab.kzg-challenge" "tipa.ab.kzg" 0 ++
  roundChallenges rounds "tipa.c.gipa.round" ++
  challengeEvents "ssm.kzg-challenge" "tipa.c.kzg" 0

def emitCount (count : Nat) : IO Unit := do
  for event in traceForCount count do
    IO.println (event.render count)

def main (args : List String) : IO UInt32 := do
  if args.isEmpty then
    IO.eprintln "usage: lean --run SnarkpackOracle.lean <count>..."
    return 64
  for arg in args do
    match arg.toNat? with
    | none =>
        IO.eprintln s!"invalid count: {arg}"
        return 64
    | some count =>
        emitCount count
  return 0
