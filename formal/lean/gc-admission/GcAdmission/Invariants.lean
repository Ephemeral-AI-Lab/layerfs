import GcAdmission.Transitions

/-!
  GcAdmission/Invariants.lean

  The §7 invariants of the shipped scope, plus the helper predicates they use.
  `Inv` is a structure (still a `Prop`) so the preservation proofs can name
  each conjunct.  §7.6 (fence/lease) is deferred with the lease feature.

  Review round 1 (2026-11-24): the compact barrier is defined once
  (`CompactBarrier`) and is Inv1's third conjunct; there is no separate Inv5
  field.  The hypotheses are the product's premises, not decoration: H2 is the
  ms-only stop-old mint premise, H3 is the existence (not value-identity)
  retention of a home record, H4 is the one-bounded-read enumeration
  obligation, and H7 is owner-key uniqueness (see README Limits).

  Review round 2 (2026-11-24): H3 was false in its value-identity form (a
  start/restore rewrites the incarnation; `closeHome`/`forceRemoveHome` replace
  the tombstone), so it is the existence form the plan §8 states: a
  previously recorded value is never silently dropped, though it may be
  replaced.  H4 covers every map row, retained done rows included, which is
  what `EntriesSound` asserts.
-/
namespace GcAdmission

/-- An intent holds Branch `b` when it names `b`, when it is Store-scoped,
or when it is Project-scoped for the project that owns `b`.  The real
`gc_intent_all_branch` query returns all three, so a live Store/Project row
blocks a Branch removal just as a live Branch row does. -/
def HoldsBranch (s : State) (b : BranchId) (i : Intent) : Prop :=
  i.branch = some b ∨
  (i.branch = none ∧ i.project = none) ∨
  (i.branch = none ∧ ∃ p, i.project = some p ∧
      (s.branchProject b = none ∨ s.branchProject b = some p))

def MountedOn (s : State) (b : BranchId) : Prop :=
  ∃ op i, s.store.intents op = some i ∧
          HoldsBranch s b i ∧ i.state ≠ .done

def AnyMounted (s : State) : Prop :=
  ∃ op i, s.store.intents op = some i ∧ i.state ≠ .done

/-- The force-remove / close end: every visible row naming machine `m` and
Sandbox `sb` is either a retained done row or the explicit uncertain marker a
refused release leaves (`uncertain = true`).  No `.live` row remains; an
uncertain row is retained and still blocks `storeCompact`. -/
def SandboxIntentsEnded (s : State) (m : MachineId) (sb : SandboxId) : Prop :=
  ∀ op i, s.store.intents op = some i → i.machine = some m → i.sandbox = some sb →
    i.state = .done ∨ (i.state = .uncertain ∧ i.uncertain = true)

/-- The rows an adoption/compact guard uses: a non-done intent that is not
dead-certain.  A dead-certain row may be removed automatically; a live or
explicitly uncertain one may not. -/
def LiveHeldOn (s : State) (b : BranchId) : Prop :=
  ∃ op i, s.store.intents op = some i ∧ HoldsBranch s b i ∧
          i.state ≠ .done ∧ (¬ Dead s i ∨ i.uncertain = true)

def AnyLiveHeld (s : State) : Prop :=
  ∃ op i, s.store.intents op = some i ∧ i.state ≠ .done ∧
          (¬ Dead s i ∨ i.uncertain = true)

def OperatorDone (i : Intent) : Prop :=
  i.state = .done ∧ i.doneBy = some .operator ∧ i.action ≠ some .preview

/-- Every place witness that says `dead` names an ordered cause: a newer
machine incarnation, a newer Sandbox generation, or a retired generation at or
after the recorded one. -/
def OrderedDeath (w : Witness) : Prop :=
  w.kind = .place → w.verdict = .dead →
    ∃ p, w.place = some p ∧
      ((∃ r c, w.recorded.machine = some r ∧ p.machine = some c ∧
                comparePlace r c = .recordedOlder) ∨
       (∃ r c, w.recorded.sandbox = some r ∧ p.sandbox = some c ∧
                comparePlace r c = .recordedOlder) ∨
       (∃ r t, w.recorded.sandbox = some r ∧ p.retired = some t ∧
                (comparePlace r t = .recordedOlder ∨ comparePlace r t = .equal)))

/-- The exact sequential `compareMachine` → `compareSandbox` decomposition of
`placeVerdict`.  There is no lease step in the shipped scope. -/
def OrderedRule (w : Witness) : Prop :=
  w.kind = .place →
  match w.place with
  | none => True
  | some p =>
      match compareMachine w.recorded.machine p.machine with
      | some .dead      => w.verdict = .dead
      | some .uncertain => w.verdict = .uncertain
      | some .live      => w.verdict = .live
      | none =>
          match compareSandbox w.recorded.sandbox p.sandbox p.retired with
          | some .dead      => w.verdict = .dead
          | some .uncertain => w.verdict = .uncertain
          | some .live      => w.verdict = .live
          | none            => w.verdict = .live

/-- The Store-wide compact barrier: a compacted Store has no non-done row.
This is the third conjunct of Inv1, defined once. -/
def CompactBarrier (s : State) : Prop :=
  s.store.compacted = true → ¬ AnyMounted s

/-- 1. A live mount is never reclaimed: a Branch-scoped non-done row sits on a
live Branch, a removed Branch has no non-done holder, and a compacted Store
has no non-done row (`CompactBarrier`). -/
def Inv1 (s : State) : Prop :=
  (∀ op i, s.store.intents op = some i → i.state ≠ .done →
      ∀ b, i.branch = some b → s.store.branches b = some .live) ∧
  (∀ b, s.store.branches b = some .removed → ¬ MountedOn s b) ∧
  CompactBarrier s

/-- 2. Release only on an explicitly uncertain (settled) row; settle only
marks an existing non-done row uncertain.  Every done release and every
uncertain row carries an admission witness. -/
def Inv2 (s : State) : Prop :=
  (∀ op i, s.store.intents op = some i → OperatorDone i →
    ∃ w, i.witness = some w ∧ WitnessConsistent w ∧
      (w.verdict = .dead ∨ w.verdict = .uncertain ∨ i.settled = true)) ∧
  (∀ op i, s.store.intents op = some i → i.state = .uncertain →
    ∃ w, i.witness = some w ∧ WitnessConsistent w)

/-- 3. Missing / restored / unordered place evidence is never Dead. -/
def Inv3 (s : State) : Prop :=
  ∀ op i w, s.store.intents op = some i → i.witness = some w →
    WitnessConsistent w → OrderedDeath w

/-- 4. The ordered rule: newer ⇒ Dead, older ⇒ Uncertain, equal ⇒ Live
(unless the tombstone says otherwise). -/
def Inv4 (s : State) : Prop :=
  ∀ op i w, s.store.intents op = some i → i.witness = some w →
    WitnessConsistent w → OrderedRule w

/-- A retained done row: the one live map has a done intent at this key. -/
def DoneRow (s : State) (op : OpId) : Prop :=
  ∃ i, s.store.intents op = some i ∧ i.state = .done

/-- 7a. No transition drops a retained done row.  With no pruning operation
in the shipped scope this is the plain non-removal statement; a transition
that deletes a done row falsifies it, so `step_preserves` cannot hold
vacuously. -/
def Inv7_no_done_removed (s : State) : Prop :=
  ∀ (ho : Op) (op' : OpId), DoneRow s op' → DoneRow (step ho s) op'

/-- 7b. The compact transition copies every done row of its source. -/
def Inv7_compact_copies (s : State) : Prop :=
  ∀ op', DoneRow s op' → DoneRow (step .storeCompact s) op'

/-- 7. `Copy::Done` as non-removal over the single intent map (`doneRows` is
derived; there is no second audit map): no transition drops a retained done
row, and compact copies every one. -/
def Inv7 (s : State) : Prop :=
  Inv7_no_done_removed s ∧ Inv7_compact_copies s

/-- The Store support is exact: a key has a live map value exactly when it is
listed in `entries`.  `Store.intents` is total (the map value), so this is a
real property, not a definitional fact. -/
def Store.Sound (s : Store) : Prop :=
  ∀ op, (∃ i, s.intents op = some i) ↔ op ∈ s.entries

/-- The state-level support invariant: `Store.Sound` on the store.  It is what
lets the executable guards scanning `entries` see every row; the DB read that
builds `entries` must return every map row, retained done rows included (H4,
README Limits). -/
def EntriesSound (s : State) : Prop :=
  s.store.Sound

/-- 8. No automatic release: every done row was created by an operator
action (settle/release, or audit-only preview). -/
def Inv8_no_automatic_release (s : State) : Prop :=
  ∀ op i, s.store.intents op = some i → i.state = .done →
    i.doneBy = some .operator

/-- The conjunction of the preservation invariants.  §7's Inv3 and Inv4 are
stated separately as `WitnessConsistent` corollaries (`Inv3_preserves` and
`Inv4_preserves` in `Proofs.lean`): they constrain a witness's own field, so
`WitnessConsistent` already forces them and they are not independent state
invariants.  §7.6 (fence) is deferred with the lease/fence feature.  The
compact barrier is Inv1's third conjunct (`CompactBarrier`), not a separate
field. -/
structure Inv (s : State) : Prop where
  inv1 : Inv1 s
  inv2 : Inv2 s
  inv7 : Inv7 s
  inv8 : Inv8_no_automatic_release s
  entriesSound : EntriesSound s

/-! ## §8 hypotheses (definitions only; obligations, not axioms) -/

/-- H2 is the product's stop-old mint premise: the new incarnation is strictly
greater in milliseconds than the one it replaces (same label), and the old
kernel is stopped before the new one serves.  Only the token-order half is
expressible over the abstract State; the stop is an operational premise.  The
model's restore branch (an older current value is `Uncertain`) means a failed
H2 can only lose reclamation, never cause a false death. -/
def H2_stop_old (s : State) (m : MachineId) (inc : Ordered) : Prop :=
  ∀ cur, homeIncarnation s m = some (.ordered cur) →
    cur.label = inc.label → cur.ms < inc.ms

/-- H3: home record **existence** retention.  A tombstone is written before the
record is deleted, and neither a tombstone nor a MachineRecord incarnation is
ever *removed*.  The value may change: a reused Sandbox name writes a newer
retirement (`SandboxTombstone::write` replaces the earlier one), and a
`machineStart`/`machineRestore` rewrites the incarnation, and a home restore can
bring an older value back.  The ordered direction of a write is H2's job; H3
only forbids losing the evidence.  (The value-identity form is false.) -/
def H3_retention (s s' : State) : Prop :=
  (∀ m sb g, tombstone s m sb = some g → ∃ g', tombstone s' m sb = some g') ∧
  (∀ m inc, homeIncarnation s m = some inc →
      ∃ inc', homeIncarnation s' m = some inc')

/-- H4: one bounded read returns **every** map row, retained done rows
included.  This is the DB-to-`entries` enumeration obligation: the read that
builds the support must not miss a row the map carries, or `EntriesSound` (the
`Inv` field) would not hold of the loaded state. -/
def H4_read (s : State) : Prop :=
  ∀ op i, s.store.intentMap op = some i → op ∈ s.store.entries

def Intent.scopeKey (i : Intent) : Option (ProjectId ⊕ BranchId) :=
  match i.branch, i.project with
  | some b, _     => some (Sum.inr b)
  | none, some p  => some (Sum.inl p)
  | none, none    => none            -- Store scope

/-- H7: owner-key uniqueness (the product's
`UNIQUE(scope_kind, scope_id, owner_boot)` for non-done rows; multiple NULL
owners are permitted).  It says nothing about done-row durability. -/
def H7_owner_key_unique (s : State) : Prop :=
  ∀ op i op' i', s.store.intents op = some i → s.store.intents op' = some i' →
    i.state ≠ .done → i'.state ≠ .done →
    i.ownerBoot ≠ none → i.ownerBoot = i'.ownerBoot →
    i.scopeKey = i'.scopeKey → op = op'

end GcAdmission
