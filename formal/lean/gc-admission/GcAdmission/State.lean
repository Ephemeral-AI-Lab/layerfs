/-
  GcAdmission/State.lean

  The Level-2 model of the shipped LayerFS GC admission / liveness scope:
  ordered identity (machine incarnation, Sandbox generation) with the Sandbox
  tombstone, the `gc_intent` done-row/audit path and the compact barrier.
  Lease/fence, the cross-home `PlaceView` merge and archive/prune are deferred
  (Ephemeral-AI-Lab/layerfs#775) and are not modelled here.

  Everything is pure, computable and Mathlib-free: the only import is the
  Lean 4 standard library.

  Modelling note.  The plan's §5.3 uses `Finset` for the enumerated
  collections.  Lean 4 core (and `Std`) does not ship `Finset` and the task
  forbids Mathlib, so the enumerated collections are `List`s with deduplicated
  updates.  The audit set is *derived* from the live intent map as its `done`
  rows, which is the structural form of the SQLite compact's `Copy::Done`:
  the done rows are exactly the retained rows.  Admission reads a single
  machine home; a machine home is either readable or absent, and an absent
  home is no proof.

  Review round 1 (2026-11-24): the ordered token is compared by milliseconds
  only -- the product's `compare_place` treats a same-millisecond pair with
  different nonces as `Unordered`, never as an order.  The live intent map is
  total (`Store.intents` is the map), and the finite `entries` list is the
  support invariant `EntriesSound`.  `placeOf` reads the place authority only
  when the intent recorded one of the two authority columns; a place-null row
  goes to the owner evidence.

  Review round 2 (2026-11-24): `Ordered` keeps the raw token text, so `.equal`
  is raw-token equality and two spellings of the same (label, ms, nonce) are
  `.unordered`; `parseToken` accepts only non-empty ASCII decimal digits below
  `2^64`; `ownerVerdict` takes the Store kind (`sharedStore`), so a place-null
  other-boot row is Live on a PostgreSQL Store and Dead only on a local SQLite
  Store, matching `OwnerView::owner_dead`.
-/
import Std

namespace GcAdmission

abbrev OpId      := Nat
abbrev BranchId  := Nat
abbrev ProjectId := Nat
abbrev MachineId := String
abbrev SandboxId := String
abbrev HomeId    := String
abbrev BootId    := String

/-! ## 5.1 Ordered token and place relation -/

/-- A place token `p1:<label>:<ms>:<nonce>`.  `raw` is the exact token text:
the product's `compare_place` compares the raw strings first, so the raw value
is what makes two spellings of the same `(label, ms, nonce)` -- a leading zero,
say -- distinct values rather than an equality.  The label is part of the token
but not of the within-kind order: admission compares a worker token with a
worker token and a Sandbox token with a Sandbox token, never across kinds. -/
structure Ordered where
  label : String
  ms    : Nat
  nonce : String
  raw   : String
  deriving DecidableEq, Repr

/-- `a` is strictly older than `b` (same-label tokens), by milliseconds only. -/
def Ordered.lt (a b : Ordered) : Prop := a.ms < b.ms

instance : LT Ordered := ⟨Ordered.lt⟩

instance (a b : Ordered) : Decidable (a < b) := by
  change Decidable (a.ms < b.ms)
  infer_instance

/-- The recorded/current relation, mirroring `contract::authority::PlaceOrder`.
At least one value predates `p1:` or the two differ without a millisecond
order (including a same-millisecond pair with different nonces). -/
inductive PlaceOrder
  | equal
  | recordedOlder   -- recorded < current: the current place is newer
  | recordedNewer   -- recorded > current: a restore/rollback
  | unordered       -- no millisecond order, or a value predates `p1:`
  deriving DecidableEq, Repr

/-- A place value as admission can see it. `legacy` is any value that does not
parse; it compares equal only to itself and never proves death. -/
inductive PlaceValue
  | ordered (o : Ordered)
  | legacy  (raw : String)
  deriving DecidableEq, Repr

/-- The product's `compare_place` for two parsed values: the **raw** token
strings first (equal only when they are equal; two spellings of the same
label/milliseconds/nonce are not equal), then the label guard (admission never
orders across kinds), then the milliseconds.  A same-label, same-millisecond,
different-raw pair is `unordered`, never an order. -/
def compareOrdered (a b : Ordered) : PlaceOrder :=
  if a.raw = b.raw then .equal
  else if a.label ≠ b.label then .unordered
  else if a.ms < b.ms then .recordedOlder
  else if b.ms < a.ms then .recordedNewer
  else .unordered

/-- Cross-label is no proof: the design never compares a worker token with
a Sandbox token, and widening the model to do so would be unsound. -/
def comparePlace : PlaceValue → PlaceValue → PlaceOrder
  | .ordered a, .ordered b => compareOrdered a b
  | .legacy a, .legacy b => if a = b then .equal else .unordered
  | _, _ => .unordered

/-- The `u64` range of the product's `milliseconds.parse()`: a value at or
above `2^64` overflows and is not an ordered token. -/
def msBound : Nat := 2 ^ 64

/-- The milliseconds field is non-empty ASCII decimal digits only: no sign, no
whitespace, no `_` or other separator.  (`u64::from_str` also accepts a leading
`+`; no minted token carries one and the model is deliberately stricter.) -/
def asciiDigits (s : String) : Bool :=
  !s.isEmpty && s.toList.all (fun c => '0' ≤ c && c ≤ '9')

/-- Parse the `ms` field: decimal digits, then the `u64` bound. -/
def parseMs (s : String) : Option Nat :=
  if asciiDigits s then
    let n := s.toList.foldl (fun acc c => acc * 10 + (c.toNat - '0'.toNat)) 0
    if n < msBound then some n else none
  else none

/-- Parse the real `p1:<label>:<ms>:<nonce>` shape into the ordered model,
keeping the raw text.  Anything else is absent evidence, never death. -/
def parseToken (raw : String) : Option Ordered :=
  match raw.splitOn ":" with
  | ["p1", label, ms, nonce] =>
      match parseMs ms with
      | some ms => if nonce.isEmpty then none else some ⟨label, ms, nonce, raw⟩
      | none    => none
  | _ => none

/-! ## 5.2 Liveness, evidence, and the verdict -/

inductive Liveness
  | live
  | uncertain
  | dead
  deriving DecidableEq, Repr

inductive RowState
  | live
  | uncertain
  | done
  deriving DecidableEq, Repr

/-- The principal that created a done row.  The shipped scope has only the
explicit operator escape; there is no automatic release. -/
inductive Principal
  | operator
  deriving DecidableEq, Repr

/-- The operator actions the audit records: the shipped `preview`, `settle`
and `release`. -/
inductive Action
  | preview | settle | release
  deriving DecidableEq, Repr

/-- The owner evidence the intent itself carries; the fallback for a private or
place-null row. Missing/another-namespace evidence is never death. -/
inductive OwnerEvidence
  | sameBootLive
  | oldBoot
  | otherNamespace
  | none
  deriving DecidableEq, Repr

/-- The owner fallback, matching `OwnerView::owner_dead`: a different boot is
death only for a **local SQLite Store** (`sharedStore = false`).  A shared
PostgreSQL Store may be admitted from another machine on that boot, so a
place-null other-boot row stays conservatively Live there. -/
def ownerVerdict (sharedStore : Bool) : OwnerEvidence → Liveness
  | .sameBootLive   => .live
  | .oldBoot        => if sharedStore then .live else .dead
  | .otherNamespace => .live
  | .none           => .live

/-- The two `gc_intent` authority columns, as the intent records them:
the machine incarnation and the Sandbox generation. -/
structure RecordedPlace where
  machine : Option PlaceValue
  sandbox : Option PlaceValue
  deriving DecidableEq, Repr

/-- The current values one admission compares against: the single readable
home's machine incarnation, the Sandbox generation and the Sandbox tombstone. -/
structure PlaceRecord where
  machine : Option PlaceValue
  sandbox : Option PlaceValue
  retired : Option PlaceValue   -- the machine tombstone's generation
  deriving DecidableEq, Repr

/-- Whether a witness is a place decision or the owner fallback. -/
inductive WitnessKind
  | place
  | owner
  deriving DecidableEq, Repr

/-- The snapshot admission decided; stored on the row so a retained done row
is auditable after the home moved on.  A place witness carries the
`PlaceRecord`; an owner witness carries the `OwnerEvidence` fallback. -/
structure Witness where
  kind          : WitnessKind
  recorded      : RecordedPlace
  place         : Option PlaceRecord
  ownerEvidence : OwnerEvidence
  sharedStore   : Bool := false  -- the Store kind the owner fallback was read with
  verdict       : Liveness
  deriving DecidableEq, Repr

/-- Machine-incarnation comparison: the machine wins before the Sandbox. -/
def compareMachine (recorded current : Option PlaceValue) : Option Liveness :=
  match recorded, current with
  | some r, some c =>
      match comparePlace r c with
      | .recordedOlder => some .dead
      | .recordedNewer => some .uncertain
      | .unordered     => some .uncertain
      | .equal         => none
  | _, _ => none

/-- Sandbox generation comparison, including the retained tombstone. -/
def compareSandbox (recorded current retired : Option PlaceValue) :
    Option Liveness :=
  match recorded with
  | none => none
  | some r =>
      match current with
      | some c =>
          match comparePlace r c with
          | .recordedOlder => some .dead
          | .recordedNewer => some .uncertain
          | .unordered     => some .uncertain
          | .equal =>
              match retired with
              | some t =>
                  match comparePlace r t with
                  | .equal          => some .dead
                  | .recordedOlder  => some .dead
                  | .recordedNewer  => none          -- an earlier Sandbox
                  | .unordered      => some .uncertain
              | none => none
      | none =>
          match retired with
          | some t =>
              match comparePlace r t with
              | .equal         => some .dead
              | .recordedOlder => some .dead
              | .recordedNewer => some .uncertain     -- a restore
              | .unordered     => some .uncertain
          | none => none

/-- The hand translation of `PlaceAuthority::liveness` for the shipped scope:
machine incarnation first, then the Sandbox generation (record first, then the
tombstone), then live.  No lease step. -/
def placeVerdict (recorded : RecordedPlace) (place : PlaceRecord) : Liveness :=
  match compareMachine recorded.machine place.machine with
  | some v => v
  | none =>
      match compareSandbox recorded.sandbox place.sandbox place.retired with
      | some v => v
      | none   => .live

/-- The verdict the row's stored `Witness` must agree with. -/
def WitnessConsistent (w : Witness) : Prop :=
  match w.kind with
  | .place =>
      ∃ p, w.place = some p ∧ w.verdict = placeVerdict w.recorded p
  | .owner =>
      w.place = none ∧ w.verdict = ownerVerdict w.sharedStore w.ownerEvidence

/-! ## 5.3 State -/

structure Intent where
  op             : OpId
  state          : RowState
  branch         : Option BranchId
  project        : Option ProjectId
  machine        : Option MachineId
  sandbox        : Option SandboxId
  ownerBoot      : Option BootId
  ownerEvidence  : OwnerEvidence
  recorded       : RecordedPlace
  evidence       : String
  doneBy         : Option Principal
  uncertain      : Bool          -- the DB `uncertain` column: true marks the
                                 -- row unresolved and blocks any automatic
                                 -- adoption or compact deletion
  settled        : Bool          -- an explicit operator settle marked the row;
                                 -- retained on the done row for the audit
  action         : Option Action
  witness        : Option Witness
  deriving DecidableEq, Repr

/-- The evidence string shape the operator transitions write:
`action=<a>;operator=<o>;op=<id>`.  The preview row is the exception: the
product mints a fresh random op id and writes `action=preview;operator=<o>`
with no `op=` (see `previewEvidence`). -/
def evidenceFor (action operator : String) (op : OpId) : String :=
  "action=" ++ action ++ ";operator=" ++ operator ++ ";op=" ++ toString op

/-- The product's preview evidence: a fresh random op id is the row key, and
the evidence text carries only the action and operator label. -/
def previewEvidence (operator : String) : String :=
  "action=preview;operator=" ++ operator

/-- The two `gc_intent` authority columns, as the product's `PlaceView::liveness`
gates the place path: only a row that recorded a machine incarnation or a
Sandbox generation is a placed row.  The machine *name* alone is not
authority. -/
def Intent.hasPlaceEvidenceB (i : Intent) : Bool :=
  i.recorded.machine.isSome || i.recorded.sandbox.isSome

structure Sandbox where
  machine    : MachineId
  generation : Option PlaceValue
  deriving DecidableEq, Repr

/-- One machine's readable home record: its ordered incarnation, its current
Sandbox records, and the machine-level retired Sandboxes.  The `retired` list
is the model of `machine/<machine>/sandbox-retired/<name>.json`; it survives
the Sandbox record's deletion. -/
structure Machine where
  machine     : MachineId
  incarnation : Option PlaceValue
  sandboxes   : List (SandboxId × Sandbox)
  retired     : List (SandboxId × PlaceValue)
  deriving DecidableEq, Repr

/-- The one readable machine home. -/
abbrev Home := Machine

inductive BranchState
  | live
  | removed
  deriving DecidableEq, Repr

/-- The Store record.  `intentMap` is the live map (total), and `entries` is
its finite support list.  `EntriesSound` states that a key has a map value
exactly when it is listed, so the executable guards can scan `entries` and
still see every row; the DB read that builds `entries` must return every map
row, retained done rows included (H4). -/
structure Store where
  branches  : BranchId → Option BranchState
  intentMap : OpId → Option Intent
  entries   : List OpId
  compacted : Bool

/-- The live intent map (total): the map value, with `entries` the support. -/
def Store.intents (s : Store) (op : OpId) : Option Intent :=
  s.intentMap op

/-- The empty Store. -/
def Store.empty : Store :=
  { branches := fun _ => none
  , intentMap := fun _ => none
  , entries := []
  , compacted := false }

structure State where
  store         : Store
  home          : Option Home
  branchProject : BranchId → Option ProjectId   -- static project ownership
  sharedStore   : Bool := false   -- PostgreSQL (true) vs local SQLite (false)

/-- The one readable home for machine `m`, if it exists.  A missing home is
`none` (no proof, never death). -/
def State.homeFor (s : State) (m : MachineId) : Option Home :=
  match s.home with
  | some h => if h.machine = m then some h else none
  | none   => none

def homeIncarnation (s : State) (m : MachineId) : Option PlaceValue :=
  (s.homeFor m).bind (·.incarnation)

def tombstone (s : State) (m : MachineId) (sb : SandboxId) : Option PlaceValue :=
  (s.homeFor m).bind fun h =>
    (h.retired.find? (fun t => t.1 = sb)).map (·.2)

/-! ## 5.4 The admission verdict -/

/-- The intent's own recorded place, the fallback when the readable home
carries no value for this intent.  A truly place-null intent has no fallback. -/
def Intent.ownPlace (i : Intent) : Option PlaceRecord :=
  match i.recorded.machine, i.recorded.sandbox with
  | none, none => none
  | m, sb => some { machine := m, sandbox := sb, retired := none }

/-- The current place one admission compares against.  The place path is
read only for a row that recorded authority (`machine_incarnation` or
`sandbox_generation`); a place-null row returns `none` and goes to the owner
evidence.  When the single readable home exists, each field falls back to the
intent's own recorded value independently (the `PlaceView::record` per-field
merge), with the tombstone from the home.  With no readable home the intent's
own recorded place is the conservative fallback (F2, plan §14). -/
def placeOf (s : State) (i : Intent) : Option PlaceRecord :=
  if !i.hasPlaceEvidenceB then none
  else
    match i.machine with
    | none => i.ownPlace
    | some m =>
        match s.homeFor m with
        | none => i.ownPlace
        | some h =>
            let homeSandbox :=
              match i.sandbox with
              | some sb =>
                  (h.sandboxes.find? (fun p : SandboxId × Sandbox => p.1 = sb)).bind
                    (fun p => p.2.generation)
              | none => none
            let homeRetired :=
              match i.sandbox with
              | some sb =>
                  (h.retired.find? (fun p : SandboxId × PlaceValue => p.1 = sb)).map
                    (fun p => p.2)
              | none => none
            let machine :=
              match h.incarnation with
              | some v => some v
              | none   => i.recorded.machine
            let sandbox :=
              match homeSandbox with
              | some v => some v
              | none   => i.recorded.sandbox
            some { machine := machine, sandbox := sandbox, retired := homeRetired }

def verdict (s : State) (i : Intent) : Liveness :=
  match placeOf s i with
  | some p => placeVerdict i.recorded p
  | none   => ownerVerdict s.sharedStore i.ownerEvidence

def Dead (s : State) (i : Intent) : Prop := verdict s i = .dead
def Uncertain (s : State) (i : Intent) : Prop := verdict s i = .uncertain
def Live (s : State) (i : Intent) : Prop := verdict s i = .live

/-- Death from the place authority alone (the automatic path). -/
def PlaceDead (s : State) (i : Intent) : Prop :=
  match placeOf s i with
  | some p => placeVerdict i.recorded p = .dead
  | none   => False

/-- The witness of a direct audit row (preview) that names no place and no
owner evidence; its verdict is the owner fallback `.live`, so it is
consistent but never proves death. -/
def auditWitness : Witness :=
  { kind := .owner
  , recorded := { machine := none, sandbox := none }
  , place := none
  , ownerEvidence := .none
  , sharedStore := false
  , verdict := .live }

/-- The snapshot a mount/admission stores. -/
def witnessFor (s : State) (i : Intent) : Witness :=
  match placeOf s i with
  | some p =>
      { kind := .place, recorded := i.recorded,
        place := some p, ownerEvidence := i.ownerEvidence,
        sharedStore := s.sharedStore,
        verdict := placeVerdict i.recorded p }
  | none =>
      { kind := .owner, recorded := i.recorded,
        place := none, ownerEvidence := i.ownerEvidence,
        sharedStore := s.sharedStore,
        verdict := ownerVerdict s.sharedStore i.ownerEvidence }

end GcAdmission
