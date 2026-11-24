import GcAdmission.Proofs

/-!
  GcAdmission/Examples.lean

  Executable regression net.  `#guard` elaborates a `Bool` at build time, so a
  false example breaks `lake build`.  The closed computations cover the pure
  ordered authority and verdict cases, a successful path through mount, settle,
  release, compact and branchRemove, the retained-done-row case, one case per
  invariant (Inv1/Inv2/Inv7/Inv8 -- the compact barrier is Inv1's third
  conjunct -- and the Inv3/Inv4 witness corollaries), the single readable home
  with the own-place fallback, the store-kind owner fallback, the preview's
  stored audit witness, and the force-remove release / mark-uncertain end
  (including the old-uncertain-row deletion).
-/
namespace GcAdmission

def w10 : Ordered := ⟨"worker", 10, "aa", "p1:worker:0000000000000010:aa"⟩
def w20 : Ordered := ⟨"worker", 20, "aa", "p1:worker:0000000000000020:aa"⟩
def s10 : Ordered := ⟨"sandbox", 10, "aa", "p1:sandbox:0000000000000010:aa"⟩
def s20 : Ordered := ⟨"sandbox", 20, "aa", "p1:sandbox:0000000000000020:aa"⟩

-- Two spellings of the same (label, ms, nonce): a shorter zero padding is a
-- different raw token, so the product's raw-first compare does not call them
-- equal.
def w10pad : Ordered := ⟨"worker", 10, "aa", "p1:worker:0010:aa"⟩
def w10short : Ordered := ⟨"worker", 10, "aa", "p1:worker:10:aa"⟩

def rp (m sb : Option PlaceValue) : RecordedPlace :=
  { machine := m, sandbox := sb }

def pp (m sb r : Option PlaceValue) : PlaceRecord :=
  { machine := m, sandbox := sb, retired := r }

/-! ### Ordered compare and parse (22) -/

#guard comparePlace (.ordered w10) (.ordered w20) = .recordedOlder
#guard comparePlace (.ordered w20) (.ordered w10) = .recordedNewer
#guard comparePlace (.ordered w10) (.ordered w10) = .equal
#guard comparePlace (.ordered w10) (.ordered s10) = .unordered
#guard comparePlace (.legacy "worker:7") (.legacy "worker:8") = .unordered
#guard parseToken "p1:worker:0000000000000010:aa" = some w10
#guard parseToken "worker:10" = none
#guard parseToken "p1:worker:0000000000000010:" = none

-- Round 2: the raw token is the identity the product compares first.
#guard comparePlace (.ordered w10) (.ordered w10pad) = .unordered
#guard comparePlace (.ordered w10pad) (.ordered w10) = .unordered
#guard comparePlace (.ordered w10) (.ordered w10short) = .unordered
#guard comparePlace (.ordered w10pad) (.ordered w10pad) = .equal

-- The ms field is decimal digits below 2^64: no sign, whitespace or `_`.
#guard parseToken "p1:worker:0010:aa" = some w10pad
#guard parseToken "p1:worker:10:aa" = some w10short
#guard parseToken "p1:worker:1_0:aa" = none
#guard parseToken "p1:worker:+10:aa" = none
#guard parseToken "p1:worker:-10:aa" = none
#guard parseToken "p1:worker: 10:aa" = none
#guard parseToken "p1:worker::aa" = none
#guard parseToken "p1:worker:18446744073709551615:aa" =
  some ⟨"worker", 18446744073709551615, "aa", "p1:worker:18446744073709551615:aa"⟩
#guard parseToken "p1:worker:18446744073709551616:aa" = none
#guard parseToken "p1:worker:999999999999999999999999:aa" = none

/-! ### Millisecond-only ordering: same-ms nonces are unordered (7) -/

def w10b : Ordered := ⟨"worker", 10, "bb", "p1:worker:0000000000000010:bb"⟩
def s10b : Ordered := ⟨"sandbox", 10, "bb", "p1:sandbox:0000000000000010:bb"⟩

#guard comparePlace (.ordered w10) (.ordered w10b) = .unordered
#guard comparePlace (.ordered w10b) (.ordered w10) = .unordered
#guard comparePlace (.ordered w10b) (.ordered w10b) = .equal
-- A same-millisecond worker token is never a death, in either nonce order.
#guard placeVerdict (rp (some (.ordered w10)) none)
  (pp (some (.ordered w10b)) none none) = .uncertain
#guard placeVerdict (rp (some (.ordered w10b)) none)
  (pp (some (.ordered w10)) none none) = .uncertain
-- The tombstone case: a same-ms retired generation is uncertain, not dead.
#guard placeVerdict (rp none (some (.ordered s10)))
  (pp none none (some (.ordered s10b))) = .uncertain
#guard placeVerdict (rp none (some (.ordered s10)))
  (pp none (some (.ordered s10)) (some (.ordered s10b))) = .uncertain

/-! ### Place verdict (7) -/

#guard placeVerdict (rp (some (.ordered w10)) none)
  (pp (some (.ordered w20)) none none) = .dead
#guard placeVerdict (rp (some (.ordered w20)) none)
  (pp (some (.ordered w10)) none none) = .uncertain
#guard placeVerdict (rp (some (.ordered w10)) none)
  (pp (some (.ordered w10)) none none) = .live
#guard placeVerdict (rp none none) (pp none none none) = .live
#guard placeVerdict (rp none (some (.ordered s10)))
  (pp none (some (.ordered s10)) (some (.ordered s10))) = .dead
#guard placeVerdict (rp none (some (.ordered s20)))
  (pp none (some (.ordered s20)) (some (.ordered s10))) = .live
#guard placeVerdict (rp none (some (.ordered s20)))
  (pp none none (some (.ordered s10))) = .uncertain

/-! ### Witness construction and the store-kind owner fallback (4) -/

def st0 : State :=
  { store := Store.empty, home := none, branchProject := fun _ => none }

def stShared : State := { st0 with sharedStore := true }

def i0 : Intent :=
  { op := 1, state := .live, branch := none, project := none,
    machine := none, sandbox := none, ownerBoot := none,
    ownerEvidence := .none,
    recorded := { machine := none, sandbox := none },
    evidence := "", doneBy := none, uncertain := false, settled := false,
    action := none, witness := none }

-- A place-null other-boot row is Dead only on a local SQLite Store.
#guard (witnessFor st0 { i0 with ownerEvidence := .oldBoot }).verdict = .dead
#guard (witnessFor stShared { i0 with ownerEvidence := .oldBoot }).verdict = .live
#guard (witnessFor stShared { i0 with ownerEvidence := .oldBoot }).sharedStore = true
#guard (witnessFor st0 { i0 with ownerEvidence := .otherNamespace }).verdict = .live

/-! ### Single readable home and the own-place fallback (9) -/

def homeInc (v : Ordered) : Home :=
  { machine := "m1", incarnation := some (.ordered v), sandboxes := [], retired := [] }

def stHome : State :=
  { store := Store.empty, home := some (homeInc w20), branchProject := fun _ => none }

def iPlace : Intent :=
  { i0 with op := 9, machine := some "m1", recorded := { machine := some (.ordered w10), sandbox := none } }

-- The one readable home carries a newer incarnation: a place death.
#guard verdict stHome iPlace = .dead

-- No readable home: admission falls back to the intent's own recorded place,
-- which here names the same incarnation, so the row is Live (the documented
-- conservatism), never owner-killed.
#guard verdict st0 iPlace = .live
#guard (placeOf st0 iPlace).isSome = true

-- A truly place-null intent has no fallback and gets the owner evidence.
#guard placeOf st0 { i0 with machine := none } = none

-- Authority gate: a place-null row naming a readable machine does NOT read
-- the place path; it goes to the owner evidence.
#guard placeOf stHome { i0 with machine := some "m1" } = none
#guard verdict stHome { i0 with machine := some "m1", ownerEvidence := .oldBoot } = .dead

-- The per-field fallback: the machine record is gone, the Sandbox record is
-- gone, and only the tombstone remains.  `placeOf` fills sandbox from the
-- recorded generation (per field), the tombstone comes from the home, and
-- the equal generation is dead through the tombstone.
def homeRetired : Home :=
  { machine := "m1", incarnation := some (.ordered w20), sandboxes := [],
    retired := [("sb1", .ordered s10)] }

def stRetired : State :=
  { store := Store.empty, home := some homeRetired, branchProject := fun _ => none }

def iSandboxOnly : Intent :=
  { i0 with op := 12, machine := some "m1", sandbox := some "sb1", recorded := ({ machine := none, sandbox := some (.ordered s10) } : RecordedPlace) }

def expectedRetired : Option PlaceRecord :=
  some ⟨some (.ordered w20), some (.ordered s10), some (.ordered s10)⟩
#guard placeOf stRetired iSandboxOnly = expectedRetired
#guard verdict stRetired iSandboxOnly = .dead

-- The record-first case: a current Sandbox record older than the tombstone
-- is still Live (the record decides first); equal-to-tombstone is Dead.
def homeRecordFirst : Home :=
  { machine := "m1", incarnation := some (.ordered w20),
    sandboxes := [("sb1", ({ machine := "m1", generation := some (.ordered s20) } : Sandbox))],
    retired := [("sb1", .ordered s10)] }

def stRecordFirst : State :=
  { store := Store.empty, home := some homeRecordFirst, branchProject := fun _ => none }

def iSandboxCurrent : Intent :=
  { i0 with op := 13, machine := some "m1", sandbox := some "sb1", recorded := ({ machine := none, sandbox := some (.ordered s20) } : RecordedPlace) }

#guard verdict stRecordFirst iSandboxCurrent = .live

/-! ### Transitions, done rows, and the operator escape (17) -/

def iMount : Intent :=
  { i0 with op := 5, state := .live, branch := some 7, machine := some "m1" }

def stRemoved : State :=
  { st0 with store := { Store.empty with branches := fun b => if b = 7 then some .removed else none } }

-- Inv1: a mount on a removed Branch is refused.
#guard (step (.sandboxMount iMount) stRemoved).store.intents 5 = none

def liveIntent : Intent := { i0 with op := 1, state := .live, branch := some 7 }

def stLive : State :=
  { st0 with store :=
      { (Store.empty.addIntent 1 liveIntent) with branches := fun _ => some .live } }

-- Compact barrier (Inv1 third conjunct): a live intent blocks compact.
#guard (step .storeCompact stLive).store.compacted = false

def iUnc : Intent :=
  { i0 with op := 1, state := .uncertain, uncertain := true, settled := true }

def stUnc : State := { st0 with store := Store.empty.addIntent 1 iUnc }

-- Inv2: release succeeds on an explicitly uncertain row; it refuses a live row.
#guard ((step (.operatorRelease 1 "op") stUnc).store.intents 1).map (fun i => i.state) = some .done
#guard (step (.operatorRelease 1 "op") stLive).store.intents 1 =
  stLive.store.intents 1

def doneIntent : Intent :=
  { i0 with op := 1, state := .done, doneBy := some .operator, action := some .release, settled := true }

def stDone : State := { st0 with store := Store.empty.addIntent 1 doneIntent }

-- Inv7: compact keeps the retained done row (Copy::Done).
#guard (step .storeCompact stDone).store.compacted = true
#guard ((step .storeCompact stDone).store.intents 1).isSome = true

-- G4: preview retains a done row.
#guard ((step (.operatorPreview 7 9 "op") st0).store.intents 9).map (fun i => i.state) =
  some .done

-- Inv1/Inv2: a successful mount on a live Branch adds a live row with a
-- witness snapshot.
def iMountOk : Intent :=
  { i0 with op := 11, state := .live, branch := some 7, machine := some "m1" }

#guard ((step (.sandboxMount iMountOk) stLive).store.intents 11).map (fun i => i.state) = some .live
#guard ((step (.sandboxMount iMountOk) stLive).store.intents 11).bind (fun i => i.witness) = some (witnessFor stLive iMountOk)

-- Inv2/Inv8: settle then release succeed on an uncertain row, and the
-- released (done) row keeps the admission witness.
def stSettled : State := step (.operatorSettle 1 "op") stUnc

#guard (stSettled.store.intents 1).map (fun i => i.state) = some .uncertain
#guard (stSettled.store.intents 1).map (fun i => i.uncertain) = some true
#guard (stSettled.store.intents 1).bind (fun i => i.witness) = some (witnessFor stUnc iUnc)
#guard ((step (.operatorRelease 1 "op") stSettled).store.intents 1).map (fun i => i.state) = some .done
#guard ((step (.operatorRelease 1 "op") stSettled).store.intents 1).bind (fun i => i.witness) = some (witnessFor stUnc iUnc)

-- Inv1/Inv7: a successful branchRemove ends the dead live holder but retains
-- the done row.
def stDead : State :=
  { store := { Store.empty with branches := fun b => if b = 7 then some .live else none }
  , home := some (homeInc w20)
  , branchProject := fun _ => none }

def iDead : Intent :=
  { i0 with op := 1, state := .live, branch := some 7, machine := some "m1", recorded := { machine := some (.ordered w10), sandbox := none } }

def iDoneRetained : Intent :=
  { i0 with op := 2, state := .done, branch := some 7, doneBy := some .operator, action := some .release, settled := true }

def stMix : State :=
  { stDead with store := { (Store.empty.addIntent 1 iDead).addIntent 2 iDoneRetained with branches := fun b => if b = 7 then some .live else none } }

#guard (step (.branchRemove 7) stMix).store.branches 7 = some .removed
#guard ((step (.branchRemove 7) stMix).store.intents 1).isNone = true
#guard ((step (.branchRemove 7) stMix).store.intents 2).isSome = true

/-! ### Preview stores the audit witness (3) -/

def stPreview : State := step (.operatorPreview 7 9 "op") st0

#guard (stPreview.store.intents 9).bind (fun i => i.witness) = some auditWitness
#guard (stPreview.store.intents 9).map (fun i => i.doneBy) = some (some .operator)
#guard ((stPreview.store.intents 9).bind (fun i => i.witness.map (fun w => w.verdict))) =
  some .live

/-! ### Force remove: release, the refused-release fallback, and N1 (9) -/

def sbRec : Sandbox :=
  { machine := "m1", generation := some (.ordered s10) }

def sbHome : Home :=
  { machine := "m1", incarnation := some (.ordered w20),
    sandboxes := [("sb1", sbRec)], retired := [] }

def iMountedOn : Intent :=
  { i0 with op := 3, state := .live, branch := some 7, machine := some "m1", sandbox := some "sb1", recorded := { machine := none, sandbox := some (.ordered s10) } }

def iDoneOn : Intent :=
  { i0 with op := 4, state := .done, branch := some 7, machine := some "m1", sandbox := some "sb1", doneBy := some .operator, action := some .release, settled := true }

-- N1: a row an earlier refused release marked uncertain; a later successful
-- release deletes it too (the audit keeps only the retained done rows).
def iOldUncertainOn : Intent :=
  { i0 with op := 6, state := .uncertain, uncertain := true, settled := true, branch := some 7, machine := some "m1", sandbox := some "sb1" }

def stForce : State :=
  { store := { ((Store.empty.addIntent 3 iMountedOn).addIntent 4 iDoneOn).addIntent 6 iOldUncertainOn with branches := fun _ => some .live },
    home := some sbHome,
    branchProject := fun _ => none }

-- release accepted: the live mount intent is deleted (gc_release), the old
-- uncertain row is deleted too, and the retained done row on the same Sandbox
-- survives (Copy::Done).
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 false) stForce).store.intents 3).isNone = true
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 false) stForce).store.intents 6).isNone = true
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 false) stForce).store.intents 4).map (fun i => i.state) = some .done

-- release refused: the live row is marked uncertain (gc_mark_uncertain), not
-- silently dead, and the done row is still retained.
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 true) stForce).store.intents 3).map (fun i => i.state) = some .uncertain
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 true) stForce).store.intents 3).map (fun i => i.uncertain) = some true
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 true) stForce).store.intents 4).map (fun i => i.state) = some .done

-- The refused-release fallback records the post-tombstone witness: a row
-- recorded at the closed generation gets witness = dead while state =
-- uncertain (N2/N3), so a later call reports Unresolved, not a silent Live.
#guard ((step (.sandboxForceRemove "m1" "sb1" s10 true) stForce).store.intents 3).bind (fun i => i.witness) =
  some (witnessFor (forceRemoveHome stForce "m1" "sb1" s10) iMountedOn)
#guard (((step (.sandboxForceRemove "m1" "sb1" s10 true) stForce).store.intents 3).bind
  (fun i => i.witness.map (fun w => w.verdict))) = some .dead

-- A stale generation is refused by the guard: the force remove is the
-- identity (no tombstone, no intent end).
#guard ((step (.sandboxForceRemove "m1" "sb1" s20 true) stForce).store.intents 3).map (fun i => i.state) = some .live

/-! ### Both authority columns: a placed mount records machine and Sandbox (4) -/

def iBind : Intent :=
  { i0 with op := 14, state := .live, branch := some 7, machine := some "m1", sandbox := some "sb1", recorded := { machine := some (.ordered w10), sandbox := some (.ordered s10) } }

def stBind : State :=
  { store := { Store.empty with branches := fun _ => some .live },
    home := some sbHome, branchProject := fun _ => none }

def expectedBindPlace : PlaceRecord :=
  { machine := some (.ordered w20), sandbox := some (.ordered s10), retired := none }

def expectedBindWitness : Option Witness :=
  some { kind := .place, recorded := iBind.recorded, place := some expectedBindPlace, ownerEvidence := .none, verdict := .dead }

-- The stored witness carries both recorded authority columns and the current
-- place the readable home supplies; the newer machine incarnation is the death.
#guard ((step (.sandboxMount iBind) stBind).store.intents 14).bind (fun i => i.witness) =
  expectedBindWitness
#guard (placeOf stBind iBind).bind (fun p => p.machine) = some (.ordered w20)
#guard (placeOf stBind iBind).bind (fun p => p.sandbox) = some (.ordered s10)
#guard verdict stBind iBind = .dead

end GcAdmission
