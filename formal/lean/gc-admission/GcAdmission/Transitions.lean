import GcAdmission.State

/-!
  GcAdmission/Transitions.lean

  The 13 transitions of the shipped scope.  Guards are executable `Bool`
  predicates over the finite support `Store.entries`; a refused guard returns
  the state unchanged.  Keeping the guards computable is what makes the
  `#guard` regression net in `Examples.lean` evaluate real transitions.

  Review round 1 (2026-11-24): `sandboxClose` refuses `MountsActive` (a
  `mounted` parameter) and ends the Sandbox's mount intents after the
  tombstone (`release`, or `uncertain` when the Store refuses the delete);
  its tombstone generation comes from the home record, not the caller, and a
  record without a generation closes with no tombstone.  `sandboxUnmount`
  models `release_or_mark`.  `branchRemove` requires the Branch to exist.
  The preview evidence carries no `op=`, and the settle scope is documented
  as an abstraction (README Limits).

  Review round 2 (2026-11-24): the reconcile's owner fallback reads the Store
  kind (`State.sharedStore`) through `ownerVerdict`, so a place-null other-boot
  row is only settled dead on a local SQLite Store.
-/
namespace GcAdmission

/-! ### Store update helpers -/

/-- Insert (or replace) one intent, keeping the finite support in sync. -/
def Store.addIntent (s : Store) (op : OpId) (i : Intent) : Store :=
  { s with
    entries := op :: s.entries.filter (fun x => x ≠ op)
    intentMap := fun x => if x = op then some i else s.intentMap x }

/-- Remove one intent from the live map and its support. -/
def Store.removeIntent (s : Store) (op : OpId) : Store :=
  { s with
    entries := s.entries.filter (fun x => x ≠ op)
    intentMap := fun x => if x = op then none else s.intentMap x }

/-- Remove every intent satisfying a predicate from the live map and its
support.  The support predicate is evaluated on the old map. -/
def Store.removeWhere (s : Store) (p : OpId → Intent → Bool) : Store :=
  { s with
    entries := s.entries.filter (fun op =>
      match s.intents op with
      | some i => !p op i
      | none => true)
    intentMap := fun op =>
      match s.intentMap op with
      | some i => if p op i then none else some i
      | none => none }

/-- Update one intent in place. -/
def Store.updateIntent (s : Store) (op : OpId) (f : Intent → Intent) : Store :=
  { s with
    intentMap := fun x => if x = op then (s.intentMap x).map f else s.intentMap x }

/-- Update every visible intent satisfying `p`; the finite support `entries`
is unchanged.  This is `release_or_mark`'s `gc_mark_uncertain` fallback. -/
def Store.updateWhere (s : Store) (p : OpId → Intent → Bool)
    (f : Intent → Intent) : Store :=
  { s with
    intentMap := fun op =>
      match s.intents op with
      | some i => if p op i then some (f i) else some i
      | none => none }

/-! ### Bool guards -/

/-- An intent holds Branch `b` when it names `b`, when it is Store-scoped, or
when it is Project-scoped for the project that owns `b` (the boolean form of
§7's `HoldsBranch`). -/
def holdsBranchB (s : State) (b : BranchId) (i : Intent) : Bool :=
  (i.branch == some b) ||
  (i.branch == none && i.project == none) ||
  (i.branch == none &&
    (match i.project with
     | some p => (match s.branchProject b with
                  | none => true
                  | some q => p == q)
     | none => false))

/-- Every visible non-done intent is dead-certain (Dead and not uncertain). -/
def allDeadCertainB (s : State) : Bool :=
  s.store.entries.all (fun op =>
    match s.store.intents op with
    | none => true
    | some i => i.state == .done || (verdict s i == .dead && !i.uncertain))

/-- Every visible non-done holder of `b` is dead-certain. -/
def allDeadCertainOnB (s : State) (b : BranchId) : Bool :=
  s.store.entries.all (fun op =>
    match s.store.intents op with
    | none => true
    | some i =>
        !holdsBranchB s b i || i.state == .done ||
          (verdict s i == .dead && !i.uncertain))

def mountOk (s : State) (i : Intent) : Bool :=
  i.state == .live &&
  (s.store.intents i.op).isNone &&
  (match i.branch with
   | some b => s.store.branches b == some .live
   | none => false)

def sandboxCreateOk (s : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) : Bool :=
  match s.homeFor m with
  | none => false
  | some h =>
      (h.sandboxes.find? (fun p : SandboxId × Sandbox => p.1 = sb)).isNone &&
      (h.retired.filter (fun p => p.1 = sb)).all (fun p =>
        match p.2 with
        | .ordered g => decide (g < gen)
        | _ => true)

/-- The current record of Sandbox `sb` on machine `m`, if any. -/
def sandboxExistsOn (s : State) (m : MachineId) (sb : SandboxId) : Bool :=
  match s.homeFor m with
  | none => false
  | some h =>
      (h.sandboxes.find? (fun p : SandboxId × Sandbox => p.1 = sb)).isSome

/-- The generation the home record carries for Sandbox `sb`.  The close
tombstone is written from this value, never from a caller argument. -/
def sandboxGenerationOn (s : State) (m : MachineId)
    (sb : SandboxId) : Option PlaceValue :=
  match s.homeFor m with
  | none => none
  | some h =>
      (h.sandboxes.find? (fun p : SandboxId × Sandbox => p.1 = sb)).bind
        (fun p => p.2.generation)

/-- A non-forced close is refused while a Workspace is mounted
(`MountsActive`), and requires the Sandbox record to exist.  `force` is the
separate `sandboxForceRemove` transition. -/
def sandboxCloseOk (s : State) (m : MachineId) (sb : SandboxId)
    (mounted : Bool) : Bool :=
  mounted == false && sandboxExistsOn s m sb

/-- The force-remove guard: the record must carry the ordered generation the
caller names, so the tombstone the effect writes is the record's generation.
A record without an ordered generation is refused (documented in README). -/
def sandboxForceRemoveOk (s : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) : Bool :=
  match sandboxGenerationOn s m sb with
  | some (.ordered g) => g == gen
  | _ => false

def machineStartOk (s : State) (m : MachineId) (inc : Ordered) : Bool :=
  match s.homeFor m with
  | none => false
  | some h =>
      match h.incarnation with
      | none => true
      | some (.ordered cur) => cur.label == inc.label && decide (cur < inc)
      | some _ => true

def machineRestoreOk (s : State) (m : MachineId) (inc : Ordered) : Bool :=
  match s.homeFor m with
  | none => false
  | some h =>
      match h.incarnation with
      | some (.ordered cur) => cur.label == inc.label && decide (inc < cur)
      | _ => false

def sandboxUnmountOk (s : State) (op : OpId) : Bool :=
  match s.store.intents op with
  | none => false
  | some i => i.state != .done && i.doneBy == none

def operatorSettleOk (s : State) (op : OpId) : Bool :=
  match s.store.intents op with
  | none => false
  | some i => i.state != .done

/-- The explicit release guard: only an explicitly uncertain row may be
released.  There is no fence token in the shipped scope. -/
def operatorReleaseOk (s : State) (op : OpId) : Bool :=
  match s.store.intents op with
  | none => false
  | some i => i.state == .uncertain && i.uncertain == true

/-- A preview needs a fresh row: the live map has no intent at `row`.  The
audit lookup is derived from the live map, so it adds no second condition. -/
def previewOk (s : State) (row : OpId) : Bool :=
  (s.store.intents row).isNone

/-- A Branch removal requires the Branch to exist (absent Branch is not a
successful removal) and every dead-certain holder to be settled. -/
def branchRemoveOk (s : State) (b : BranchId) : Bool :=
  (s.store.branches b == some .live) && allDeadCertainOnB s b

/-! ### Reconcile at worker start -/

/-- The place record the worker-start reconcile compares against: the new
incarnation, plus the Sandbox/retired values admission already has. -/
def reconcileRecord (s : State) (i : Intent) (inc : Ordered) : PlaceRecord :=
  { machine := some (.ordered inc)
  , sandbox := (placeOf s i).bind (·.sandbox)
  , retired := (placeOf s i).bind (·.retired) }

def reconcileDead (s : State) (m : MachineId) (inc : Ordered)
    (i : Intent) : Prop :=
  i.machine = some m ∧ i.state ≠ .done ∧ i.uncertain = false ∧
    (placeVerdict i.recorded (reconcileRecord s i inc) = .dead ∨
     (i.recorded.machine = none ∧ i.recorded.sandbox = none ∧
      ownerVerdict s.sharedStore i.ownerEvidence = .dead))

instance (s : State) (m : MachineId) (inc : Ordered) (i : Intent) :
    Decidable (reconcileDead s m inc i) := by
  unfold reconcileDead
  infer_instance

def reconcileDeadB (s : State) (m : MachineId) (inc : Ordered)
    (i : Intent) : Bool :=
  decide (reconcileDead s m inc i)

/-! ### Transition effects -/

/-- Update the single home record of machine `m`, if it exists.  The store is
left definitionally unchanged. -/
def State.mapHome (s : State) (m : MachineId) (f : Home → Home) : State :=
  { s with home := s.home.map (fun h => if h.machine = m then f h else h) }

def doSandboxCreate (s : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) : State :=
  if sandboxCreateOk s m sb gen then
    s.mapHome m (fun h =>
      { h with
        sandboxes :=
          ((sb, { machine := m, generation := some (.ordered gen) }) ::
            h.sandboxes.filter (fun p => p.1 ≠ sb)) })
  else s

/-- The explicit `settlePlace` sub-step: release every existing non-done intent
at the admitting machine that carries place authority and is dead-certain
(`Dead`, not `PlaceDead`; the real `settle_place` also accepts the
owner-evidence death of a place-null row, which is the owner path).
It never touches a live or uncertain row. -/
def settlePlace (s : State) (intent : Intent) : State :=
  { s with store := s.store.removeWhere (fun _ i =>
      intent.hasPlaceEvidenceB && (i.machine == intent.machine) &&
      (i.state != .done) && (!i.uncertain) && (verdict s i == .dead)) }

def doSandboxMount (s : State) (i : Intent) : State :=
  if mountOk s i then
    let s1 := settlePlace s i
    let i1 := { i with state := .live, witness := some (witnessFor s i),
                       doneBy := none, uncertain := false, settled := false,
                       action := none }
    { s1 with store :=
        { (s1.store.addIntent i.op i1) with compacted := false } }
  else s

/-- The `release_or_mark` outcome of the daemon's own unmount: release
(`refused = false`) deletes the row; a refused delete marks it uncertain and
records the admission witness, so it stays `Unresolved`. -/
def doSandboxUnmount (s : State) (op : OpId) (refused : Bool) : State :=
  if sandboxUnmountOk s op then
    if refused then
      { s with store :=
          s.store.updateIntent op (fun i =>
            { i with state := .uncertain, uncertain := true,
                     witness := some (witnessFor s i) }) }
    else { s with store := s.store.removeIntent op }
  else s

/-- The home part of a close: write the record's generation as the durable
tombstone (if it has one), then delete the record.  A record without a
generation closes without a tombstone; an older tombstone is left alone. -/
def closeHome (s : State) (m : MachineId) (sb : SandboxId) : State :=
  s.mapHome m (fun h =>
    match h.sandboxes.find? (fun p : SandboxId × Sandbox => p.1 = sb) with
    | some rec =>
        { h with
          retired :=
            match rec.2.generation with
            | some g => (sb, g) :: h.retired.filter (fun p => p.1 ≠ sb)
            | none   => h.retired
          sandboxes := h.sandboxes.filter (fun p => p.1 ≠ sb) }
    | none => h)

/-- A non-done intent a force remove ends: it names machine `m` and Sandbox
`sb` (the daemon that owned it is stopped, so its mount is over).  This
includes a row already marked uncertain by an earlier refused release. -/
def endsOnSandboxB (m : MachineId) (sb : SandboxId) (i : Intent) : Bool :=
  (i.machine == some m) && (i.sandbox == some sb) && (i.state != .done)

/-- The force-remove home effect: write the generation's durable tombstone,
then delete the current Sandbox record (the same home rewrite as
`doSandboxClose`). -/
def forceRemoveHome (s : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) : State :=
  s.mapHome m (fun h =>
    { h with
      retired := ((sb, .ordered gen) :: h.retired.filter (fun p => p.1 ≠ sb))
      sandboxes := h.sandboxes.filter (fun p => p.1 ≠ sb) })

/-- The normal force-remove end: `gc_release` deletes every non-done mount
intent of the stopped Sandbox, including a row an earlier refused release had
marked uncertain (the audit keeps only the retained done rows).
`endsOnSandboxB` excludes done rows, so retained audit rows survive. -/
def releaseSandboxMounts (s : State) (m : MachineId) (sb : SandboxId) : State :=
  { s with store := s.store.removeWhere (fun _ i => endsOnSandboxB m sb i) }

/-- `release_or_mark`'s fallback: when the Store refuses the delete, mark each
mount intent of the stopped Sandbox uncertain (`gc_mark_uncertain`) and record
the admission witness of the post-tombstone state (`witnessFor s i`), so a
later call reports `Unresolved`.  The row's earlier witness is not preserved:
a row recorded at the closed generation gets `witness = dead` while its state
is `uncertain`.  Done rows never match. -/
def markSandboxMountsUncertain (s : State) (m : MachineId)
    (sb : SandboxId) : State :=
  { s with store := s.store.updateWhere (fun _ i => endsOnSandboxB m sb i) (fun i =>
      { i with state := .uncertain, uncertain := true, settled := true,
               witness := some (witnessFor s i) }) }

/-- End the mount intents of the stopped Sandbox: release (`refused = false`,
the row is deleted) or mark them uncertain (`refused = true`, the Store
refused the delete).  Both product outcomes of `release_or_mark`. -/
def endSandboxMounts (s : State) (m : MachineId) (sb : SandboxId)
    (refused : Bool) : State :=
  if refused then markSandboxMountsUncertain s m sb
  else releaseSandboxMounts s m sb

/-- A non-forced close.  It refuses `MountsActive` (`mounted = true`), writes
the record's tombstone, deletes the record, then ends the Sandbox's leftover
mount intents (`mount_intents::end`: release or mark uncertain). -/
def doSandboxClose (s : State) (m : MachineId) (sb : SandboxId)
    (mounted : Bool) (refused : Bool) : State :=
  if sandboxCloseOk s m sb mounted then
    endSandboxMounts (closeHome s m sb) m sb refused
  else s

/-- The product's force remove is not a close: it ends the Sandbox's mount
intents -- release, or mark them uncertain when the release is refused --
around the tombstone write and the Sandbox record's deletion.  Retained done
rows are never touched, so the done-row invariant survives it. -/
def doSandboxForceRemove (s : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) (refused : Bool) : State :=
  if sandboxForceRemoveOk s m sb gen then
    endSandboxMounts (forceRemoveHome s m sb gen) m sb refused
  else s

def doMachineStart (s : State) (m : MachineId) (inc : Ordered) : State :=
  if machineStartOk s m inc then
    let s1 := s.mapHome m (fun h => { h with incarnation := some (.ordered inc) })
    { s1 with store :=
        s1.store.removeWhere (fun _ i => reconcileDeadB s m inc i) }
  else s

def doMachineRestore (s : State) (m : MachineId) (inc : Ordered) : State :=
  if machineRestoreOk s m inc then
    s.mapHome m (fun h => { h with incarnation := some (.ordered inc) })
  else s

def doAdmissionRecheck (s : State) (op : OpId) : State :=
  match s.store.intents op with
  | none => s
  | some i =>
      if i.state != .done then
        { s with store := s.store.updateIntent op (fun i =>
            { i with witness := some (witnessFor s i) }) }
      else s

/-- A Branch removal ends the non-done holders only: the product keeps the
retained done rows, so the audit path is untouched.  The guard requires a
live Branch, so an absent/already-removed Branch is refused. -/
def doBranchRemove (s : State) (b : BranchId) : State :=
  if branchRemoveOk s b then
    { s with store :=
        { (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))) with
          branches := fun x => if x = b then some .removed else s.store.branches x } }
  else s

def doStoreCompact (s : State) : State :=
  if allDeadCertainB s then
    { s with store :=
        { (s.store.removeWhere (fun _ i => i.state != .done)) with
          compacted := true } }
  else s

def previewIntent (s : State) (branch : BranchId) (row : OpId)
    (label : String) : Intent :=
  { op := row
  , state := .done
  , branch := some branch
  , project := s.branchProject branch
  , machine := none
  , sandbox := none
  , ownerBoot := none
  , ownerEvidence := .none
  , recorded := { machine := none, sandbox := none }
  , evidence := previewEvidence label
  , doneBy := some .operator
  , uncertain := false
  , settled := false
  , action := some .preview
  , witness := some auditWitness }

def doOperatorPreview (s : State) (branch : BranchId) (row : OpId)
    (label : String) : State :=
  if previewOk s row then
    { s with store := s.store.addIntent row (previewIntent s branch row label) }
  else s

def doOperatorSettle (s : State) (op : OpId) (label : String) : State :=
  match s.store.intents op with
  | none => s
  | some i =>
      if i.state != .done then
        { s with store := s.store.updateIntent op (fun i =>
            { i with state := .uncertain, uncertain := true, settled := true,
                     action := some .settle,
                     evidence := evidenceFor "settle" label op,
                     witness := some (witnessFor s i) }) }
      else s

def doOperatorRelease (s : State) (op : OpId) (label : String) : State :=
  if operatorReleaseOk s op then
    { s with store := s.store.updateIntent op (fun i =>
        { i with state := .done, uncertain := false, settled := true,
                 doneBy := some .operator, action := some .release,
                 ownerBoot := none,
                 evidence := evidenceFor "release" label op }) }
  else s

/-! ### `Op` and `step` -/

inductive Op where
  | sandboxCreate      (machine : MachineId) (sandbox : SandboxId) (gen : Ordered)
  | sandboxMount       (intent : Intent)
  | sandboxUnmount     (op : OpId) (refused : Bool)
  | sandboxClose       (machine : MachineId) (sandbox : SandboxId)
      (mounted : Bool) (refused : Bool)
  | sandboxForceRemove (machine : MachineId) (sandbox : SandboxId) (gen : Ordered)
      (refused : Bool)
  | machineStart       (machine : MachineId) (inc : Ordered)
  | machineRestore     (machine : MachineId) (inc : Ordered)
  | admissionRecheck   (op : OpId)
  | branchRemove       (branch : BranchId)
  | storeCompact
  | operatorPreview    (branch : BranchId) (row : OpId) (operator : String)
  | operatorSettle     (op : OpId) (operator : String)
  | operatorRelease    (op : OpId) (operator : String)
  deriving Repr

/-- One atomic model transition.  A refused guard is the identity. -/
def step : Op → State → State
  | .sandboxCreate m sb gen, s      => doSandboxCreate s m sb gen
  | .sandboxMount i, s              => doSandboxMount s i
  | .sandboxUnmount op refused, s   => doSandboxUnmount s op refused
  | .sandboxClose m sb mounted refused, s => doSandboxClose s m sb mounted refused
  | .sandboxForceRemove m sb gen refused, s => doSandboxForceRemove s m sb gen refused
  | .machineStart m inc, s          => doMachineStart s m inc
  | .machineRestore m inc, s        => doMachineRestore s m inc
  | .admissionRecheck op, s         => doAdmissionRecheck s op
  | .branchRemove b, s              => doBranchRemove s b
  | .storeCompact, s                => doStoreCompact s
  | .operatorPreview b row label, s => doOperatorPreview s b row label
  | .operatorSettle op label, s     => doOperatorSettle s op label
  | .operatorRelease op label, s    => doOperatorRelease s op label

end GcAdmission
