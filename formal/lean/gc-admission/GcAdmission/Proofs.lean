import GcAdmission.Invariants

/-!
  GcAdmission/Proofs.lean

  Pure lemmas, the per-transition preservation lemmas and `step_preserves`.
  No Mathlib: `simp`, `split`, `cases`, `omega`, `decide`.
-/
namespace GcAdmission

/-! ## Store: the live intent map -/

theorem Store.intents_mem {s : Store} (he : s.Sound) {op : OpId} {i : Intent}
    (h : s.intents op = some i) : op ∈ s.entries :=
  (he op).mp ⟨i, h⟩

theorem Store.intents_add_self (s : Store) (op : OpId) (i : Intent) :
    (s.addIntent op i).intents op = some i := by
  simp [Store.addIntent, Store.intents]

theorem Store.intents_add_other (s : Store) (op x : OpId) (i : Intent) (hx : x ≠ op) :
    (s.addIntent op i).intents x = s.intents x := by
  by_cases h : x = op
  · exact absurd h hx
  · simp [Store.addIntent, Store.intents, h]

theorem Store.intents_remove_self (s : Store) (op : OpId) :
    (s.removeIntent op).intents op = none := by
  simp [Store.removeIntent, Store.intents]

theorem Store.intents_remove_other (s : Store) (op x : OpId) (hx : x ≠ op) :
    (s.removeIntent op).intents x = s.intents x := by
  by_cases h : x = op
  · exact absurd h hx
  · simp [Store.removeIntent, Store.intents, h]

theorem Store.intents_removeWhere_sub (s : Store) (p : OpId → Intent → Bool)
    (op : OpId) (i : Intent) (h : (s.removeWhere p).intents op = some i) :
    s.intents op = some i := by
  unfold Store.intents at h ⊢
  cases hm : s.intentMap op with
  | none => simp [Store.removeWhere, Store.intents, hm] at h
  | some i0 =>
      by_cases hp : p op i0 = true
      · simp [Store.removeWhere, Store.intents, hm, hp] at h
      · have hp' : p op i0 = false := by
          cases hv : p op i0 with
          | false => rfl
          | true => exact absurd hv hp
        simp [Store.removeWhere, Store.intents, hm, hp'] at h
        rw [← h]

theorem Store.intents_removeWhere_removed (s : Store) (p : OpId → Intent → Bool)
    (op : OpId) (i : Intent) (hi : s.intents op = some i) (hp : p op i = true) :
    (s.removeWhere p).intents op = none := by
  have hm : s.intentMap op = some i := hi
  simp [Store.removeWhere, Store.intents, hm, hp]

theorem Store.intents_removeWhere_kept (s : Store) (p : OpId → Intent → Bool)
    (op : OpId) (i : Intent) (hi : s.intents op = some i) (hp : p op i = false) :
    (s.removeWhere p).intents op = s.intents op := by
  have hm : s.intentMap op = some i := hi
  simp [Store.removeWhere, Store.intents, hm, hp]

theorem Store.intents_update_self (s : Store) (op : OpId) (i : Intent)
    (f : Intent → Intent) (hi : s.intents op = some i) :
    (s.updateIntent op f).intents op = some (f i) := by
  have hm : s.intentMap op = some i := hi
  simp [Store.updateIntent, Store.intents, hm]

theorem Store.intents_update_other (s : Store) (op x : OpId) (f : Intent → Intent)
    (hx : x ≠ op) : (s.updateIntent op f).intents x = s.intents x := by
  by_cases h : x = op
  · exact absurd h hx
  · simp [Store.updateIntent, Store.intents, h]

/-! ## Witnesses -/

theorem witnessFor_consistent (s : State) (i : Intent) :
    WitnessConsistent (witnessFor s i) := by
  unfold witnessFor
  split
  · rename_i p hp
    exact ⟨p, rfl, rfl⟩
  · rename_i hp
    exact ⟨rfl, rfl⟩

theorem auditWitness_consistent :
    WitnessConsistent auditWitness := by
  unfold auditWitness WitnessConsistent
  exact ⟨rfl, rfl⟩

theorem auditWitness_not_place :
    auditWitness.kind ≠ .place := by
  unfold auditWitness; simp

/-- Every place witness that says `dead` names an ordered cause. -/
theorem compareMachine_dead {r c : Option PlaceValue}
    (h : compareMachine r c = some .dead) :
    ∃ rm cm, r = some rm ∧ c = some cm ∧ comparePlace rm cm = .recordedOlder := by
  cases r with
  | none => simp [compareMachine] at h
  | some rm =>
    cases c with
    | none => simp [compareMachine] at h
    | some cm =>
      cases hcp : comparePlace rm cm with
      | equal => simp [compareMachine, hcp] at h
      | recordedOlder => exact ⟨rm, cm, rfl, rfl, hcp⟩
      | recordedNewer => simp [compareMachine, hcp] at h
      | unordered => simp [compareMachine, hcp] at h

theorem compareSandbox_dead {r c ret : Option PlaceValue}
    (h : compareSandbox r c ret = some .dead) :
    (∃ rs cs, r = some rs ∧ c = some cs ∧ comparePlace rs cs = .recordedOlder) ∨
    (∃ rs t, r = some rs ∧ ret = some t ∧
        (comparePlace rs t = .recordedOlder ∨ comparePlace rs t = .equal)) := by
  cases r with
  | none => simp [compareSandbox] at h
  | some rs =>
    cases c with
    | some cs =>
      cases hcp : comparePlace rs cs with
      | recordedOlder => exact Or.inl ⟨rs, cs, rfl, rfl, hcp⟩
      | equal =>
        cases ret with
        | none => simp [compareSandbox, hcp] at h
        | some t =>
          cases hrt : comparePlace rs t with
          | equal => exact Or.inr ⟨rs, t, rfl, rfl, Or.inr hrt⟩
          | recordedOlder => exact Or.inr ⟨rs, t, rfl, rfl, Or.inl hrt⟩
          | recordedNewer => simp [compareSandbox, hcp, hrt] at h
          | unordered => simp [compareSandbox, hcp, hrt] at h
      | recordedNewer => simp [compareSandbox, hcp] at h
      | unordered => simp [compareSandbox, hcp] at h
    | none =>
      cases ret with
      | none => simp [compareSandbox] at h
      | some t =>
        cases hrt : comparePlace rs t with
        | equal => exact Or.inr ⟨rs, t, rfl, rfl, Or.inr hrt⟩
        | recordedOlder => exact Or.inr ⟨rs, t, rfl, rfl, Or.inl hrt⟩
        | recordedNewer => simp [compareSandbox, hrt] at h
        | unordered => simp [compareSandbox, hrt] at h

theorem placeVerdict_dead_ordered (r : RecordedPlace) (p : PlaceRecord)
    (h : placeVerdict r p = .dead) :
    (∃ rm cm, r.machine = some rm ∧ p.machine = some cm ∧
        comparePlace rm cm = .recordedOlder) ∨
    (∃ rs cs, r.sandbox = some rs ∧ p.sandbox = some cs ∧
        comparePlace rs cs = .recordedOlder) ∨
    (∃ rs t, r.sandbox = some rs ∧ p.retired = some t ∧
        (comparePlace rs t = .recordedOlder ∨ comparePlace rs t = .equal)) := by
  unfold placeVerdict at h
  split at h
  · rename_i hcm
    cases h
    exact Or.inl (compareMachine_dead hcm)
  · rename_i hcm
    split at h
    · rename_i hcs
      cases h
      rcases compareSandbox_dead hcs with h1 | h2
      · exact Or.inr (Or.inl h1)
      · exact Or.inr (Or.inr h2)
    · rename_i hcs
      simp at h

theorem witnessFor_orderedDeath (s : State) (i : Intent) :
    OrderedDeath (witnessFor s i) := by
  unfold OrderedDeath witnessFor
  split
  · rename_i p hp
    intro _ hdead
    exact ⟨p, rfl, placeVerdict_dead_ordered i.recorded p hdead⟩
  · rename_i hp
    intro hkind
    simp at hkind

theorem witnessFor_orderedRule (s : State) (i : Intent) :
    OrderedRule (witnessFor s i) := by
  unfold OrderedRule witnessFor
  split
  · rename_i p hp
    intro _
    cases hcm : compareMachine i.recorded.machine p.machine with
    | some v =>
        cases v <;> simp [placeVerdict, hcm]
    | none =>
        cases hcs : compareSandbox i.recorded.sandbox p.sandbox p.retired with
        | some v => cases v <;> simp [placeVerdict, hcm, hcs]
        | none => simp [placeVerdict, hcm, hcs]
  · rename_i hp
    intro hkind
    simp at hkind

theorem auditWitness_orderedDeath :
    OrderedDeath auditWitness := by
  unfold OrderedDeath
  intro hkind
  exact absurd hkind auditWitness_not_place

theorem auditWitness_orderedRule :
    OrderedRule auditWitness := by
  unfold OrderedRule
  intro hkind
  exact absurd hkind auditWitness_not_place

/-! ## Intent-map case lemmas -/

theorem Store.intents_remove_sub {s : Store} {op0 : OpId} {op : OpId} {j : Intent}
    (h : (s.removeIntent op0).intents op = some j) : s.intents op = some j := by
  by_cases hh : op = op0
  · subst hh
    rw [Store.intents_remove_self] at h
    simp at h
  · rw [Store.intents_remove_other s op0 op hh] at h
    exact h

theorem Store.intents_add_cases {s : Store} {op0 : OpId} {i0 : Intent}
    {op : OpId} {j : Intent}
    (h : (s.addIntent op0 i0).intents op = some j) :
    (op = op0 ∧ j = i0) ∨ (op ≠ op0 ∧ s.intents op = some j) := by
  by_cases hh : op = op0
  · subst hh
    left
    have hs := Store.intents_add_self s op i0
    rw [hs] at h
    exact ⟨rfl, (Option.some.inj h).symm⟩
  · right
    refine ⟨hh, ?_⟩
    rw [Store.intents_add_other s op0 op i0 hh] at h
    exact h

theorem Store.intents_update_cases {s : Store} {op0 : OpId} {f : Intent → Intent}
    {op : OpId} {j : Intent}
    (h : (s.updateIntent op0 f).intents op = some j) :
    (op = op0 ∧ ∃ i, s.intents op = some i ∧ j = f i) ∨
    (op ≠ op0 ∧ s.intents op = some j) := by
  by_cases hh : op = op0
  · left
    refine ⟨hh, ?_⟩
    by_cases hsome : ∃ i, s.intents op = some i
    · rcases hsome with ⟨i, hi⟩
      refine ⟨i, hi, ?_⟩
      have hs : (s.updateIntent op f).intents op = some (f i) :=
        Store.intents_update_self s op i f hi
      rw [← hh, hs] at h
      exact (Option.some.inj h).symm
    · exfalso
      have hm : s.intentMap op = none := by
        cases hv : s.intentMap op with
        | none => rfl
        | some i => exact absurd ⟨i, hv⟩ hsome
      have hnone : (s.updateIntent op0 f).intents op = none := by
        rw [← hh]
        simp [Store.updateIntent, Store.intents, hm]
      rw [hnone] at h
      simp at h
  · right
    refine ⟨hh, ?_⟩
    rw [Store.intents_update_other s op0 op f hh] at h
    exact h

theorem Store.intents_updateWhere (s : Store) (p : OpId → Intent → Bool)
    (f : Intent → Intent) (op : OpId) :
    (s.updateWhere p f).intents op =
      match s.intents op with
      | some i => if p op i then some (f i) else some i
      | none => none := by
  rfl

theorem Store.intents_foldl_remove_sub (rows : List OpId) (s : Store)
    (op : OpId) (i : Intent)
    (h : (rows.foldl (fun st op => st.removeIntent op) s).intents op = some i) :
    s.intents op = some i := by
  induction rows generalizing s with
  | nil => simpa using h
  | cons r rs ih =>
      simp only [List.foldl_cons] at h
      exact Store.intents_remove_sub (ih (s.removeIntent r) h)

theorem Store.intents_foldl_remove_other (rows : List OpId) (s : Store)
    (op : OpId) (h : op ∉ rows) :
    (rows.foldl (fun st op => st.removeIntent op) s).intents op = s.intents op := by
  induction rows generalizing s with
  | nil => rfl
  | cons r rs ih =>
      simp only [List.foldl_cons]
      have hr : op ≠ r := by intro he; exact h (by simp [he])
      have hrs : op ∉ rs := by intro he; exact h (by simp [he])
      rw [ih (s.removeIntent r) hrs]
      exact Store.intents_remove_other s r op hr

theorem State.intents_mapHome (s : State) (m : MachineId) (f : Home → Home) :
    (s.mapHome m f).store.intents = s.store.intents := by
  rfl

/-! ## Force remove: the release / mark-uncertain effect -/

theorem endsOnSandboxB_state {m : MachineId} {sb : SandboxId} {i : Intent}
    (h : endsOnSandboxB m sb i = true) : i.state ≠ .done := by
  intro hd
  unfold endsOnSandboxB at h
  rw [hd] at h
  simp at h

theorem endsOnSandboxB_true {m : MachineId} {sb : SandboxId} {i : Intent}
    (hm : i.machine = some m) (hs : i.sandbox = some sb) (hd : i.state ≠ .done) :
    endsOnSandboxB m sb i = true := by
  simp [endsOnSandboxB, hm, hs, hd]

theorem releaseSandboxMounts_sub {s : State} {m : MachineId} {sb : SandboxId}
    {op : OpId} {i : Intent}
    (hi : (releaseSandboxMounts s m sb).store.intents op = some i) :
    s.store.intents op = some i :=
  Store.intents_removeWhere_sub s.store (fun _ i => endsOnSandboxB m sb i) op i hi

theorem markSandboxMountsUncertain_shape {s : State} {m : MachineId}
    {sb : SandboxId} {op : OpId} {i : Intent}
    (hi : (markSandboxMountsUncertain s m sb).store.intents op = some i) :
    ∃ i0, s.store.intents op = some i0 ∧
      ((endsOnSandboxB m sb i0 = false ∧ i = i0) ∨
       (endsOnSandboxB m sb i0 = true ∧ i0.state ≠ .done ∧
          i = { i0 with state := .uncertain, uncertain := true, settled := true,
                         witness := some (witnessFor s i0) })) := by
  have h := hi
  change (s.store.updateWhere (fun _ i => endsOnSandboxB m sb i) (fun i =>
      { i with state := .uncertain, uncertain := true, settled := true,
               witness := some (witnessFor s i) })).intents op = some i at h
  cases hs : s.store.intents op with
  | none =>
      rw [Store.intents_updateWhere, hs] at h
      simp at h
  | some i0 =>
      rw [Store.intents_updateWhere, hs] at h
      dsimp only at h
      by_cases hp : endsOnSandboxB m sb i0 = true
      · rw [ite_eq_left hp] at h
        exact ⟨i0, rfl, Or.inr ⟨hp, endsOnSandboxB_state hp,
          (Option.some.inj h).symm⟩⟩
      · have hpf : endsOnSandboxB m sb i0 = false := by
          cases hv : endsOnSandboxB m sb i0 with
          | false => rfl
          | true => exact absurd hv hp
        rw [ite_eq_right hp] at h
        exact ⟨i0, rfl, Or.inl ⟨hpf, (Option.some.inj h).symm⟩⟩

theorem endSandboxMounts_done_kept (s : State) (m : MachineId) (sb : SandboxId)
    (refused : Bool) {op : OpId} {i : Intent}
    (hi : s.store.intents op = some i) (hd : i.state = .done) :
    (endSandboxMounts s m sb refused).store.intents op = some i := by
  have hp : endsOnSandboxB m sb i = false := by simp [endsOnSandboxB, hd]
  cases refused with
  | false =>
      change (s.store.removeWhere (fun _ i => endsOnSandboxB m sb i)).intents op = some i
      exact (Store.intents_removeWhere_kept s.store (fun _ i => endsOnSandboxB m sb i) op i hi
        (by simpa using hp)).trans hi
  | true =>
      change (s.store.updateWhere (fun _ i => endsOnSandboxB m sb i) (fun i =>
          { i with state := .uncertain, uncertain := true, settled := true,
                   witness := some (witnessFor s i) })).intents op = some i
      rw [Store.intents_updateWhere, hi]
      simp [endsOnSandboxB, hd]

/-! ## Witness-good predicates -/

def WitnessGood3 (i : Intent) : Prop :=
  ∀ w, i.witness = some w → WitnessConsistent w → OrderedDeath w

def WitnessGood4 (i : Intent) : Prop :=
  ∀ w, i.witness = some w → WitnessConsistent w → OrderedRule w

theorem WitnessGood3_eq {i j : Intent} (h : WitnessGood3 i)
    (hw : j.witness = i.witness) : WitnessGood3 j := by
  intro w hj hc
  exact h w (by rw [← hw]; exact hj) hc

theorem WitnessGood4_eq {i j : Intent} (h : WitnessGood4 i)
    (hw : j.witness = i.witness) : WitnessGood4 j := by
  intro w hj hc
  exact h w (by rw [← hw]; exact hj) hc

theorem WitnessGood3_witnessFor {s : State} {i j : Intent}
    (hw : j.witness = some (witnessFor s i)) : WitnessGood3 j := by
  intro w hj hc
  have hwj : w = witnessFor s i := by
    have : some (witnessFor s i) = some w := by rw [← hw]; exact hj
    exact (Option.some.inj this).symm
  rw [hwj]
  exact witnessFor_orderedDeath s i

theorem WitnessGood4_witnessFor {s : State} {i j : Intent}
    (hw : j.witness = some (witnessFor s i)) : WitnessGood4 j := by
  intro w hj hc
  have hwj : w = witnessFor s i := by
    have : some (witnessFor s i) = some w := by rw [← hw]; exact hj
    exact (Option.some.inj this).symm
  rw [hwj]
  exact witnessFor_orderedRule s i

theorem WitnessGood3_audit {i : Intent}
    (hw : i.witness = some auditWitness) : WitnessGood3 i := by
  intro w hj hc
  have hwj : w = auditWitness := by
    have : some auditWitness = some w := by rw [← hw]; exact hj
    exact (Option.some.inj this).symm
  rw [hwj]
  exact auditWitness_orderedDeath

theorem WitnessGood4_audit {i : Intent}
    (hw : i.witness = some auditWitness) : WitnessGood4 i := by
  intro w hj hc
  have hwj : w = auditWitness := by
    have : some auditWitness = some w := by rw [← hw]; exact hj
    exact (Option.some.inj this).symm
  rw [hwj]
  exact auditWitness_orderedRule

/-- The shared intent-map step lemma for Inv3/Inv4: any witness we add is
either a `witnessFor` snapshot or the audit witness, and every other visible
row is an old row or keeps its witness. -/
theorem WitnessGood_step (G : Intent → Prop)
    (hWF : ∀ (s : State) (i j : Intent), j.witness = some (witnessFor s i) → G j)
    (hAW : ∀ (_s : State) (i : Intent), i.witness = some auditWitness → G i)
    (hEq : ∀ {i j : Intent}, G i → j.witness = i.witness → G j)
    (s : State)
    (h : ∀ op i, s.store.intents op = some i → G i) :
    ∀ op' i, (step op s).store.intents op' = some i → G i := by
  intro op' i hi
  cases op with
  | sandboxCreate m sb gen =>
      by_cases hg : sandboxCreateOk s m sb gen = true
      · simp [step, doSandboxCreate, hg, State.mapHome] at hi
        exact h op' i hi
      · simp [step, doSandboxCreate, hg] at hi
        exact h op' i hi
  | sandboxMount i0 =>
      by_cases hg : mountOk s i0 = true
      · simp [step, doSandboxMount, hg, settlePlace] at hi
        rcases Store.intents_add_cases hi with ⟨rfl, rfl⟩ | ⟨hne, hi0⟩
        · exact hWF s i0 _ rfl
        · exact h op' i (Store.intents_removeWhere_sub s.store _ op' i hi0)
      · simp [step, doSandboxMount, hg] at hi
        exact h op' i hi
  | sandboxUnmount op0 refused =>
      by_cases hg : sandboxUnmountOk s op0 = true
      · cases refused with
        | false =>
            simp [step, doSandboxUnmount, hg] at hi
            exact h op' i (Store.intents_remove_sub hi)
        | true =>
            simp [step, doSandboxUnmount, hg] at hi
            rcases Store.intents_update_cases hi with ⟨rfl, i0, hi0, hj⟩ | ⟨hne, hi0⟩
            · exact hWF s i0 i (by rw [hj])
            · exact h op' i hi0
      · simp [step, doSandboxUnmount, hg] at hi
        exact h op' i hi
  | sandboxClose m sb mounted refused =>
      by_cases hg : sandboxCloseOk s m sb mounted = true
      · simp only [step, doSandboxClose, hg] at hi
        cases refused with
        | false =>
            exact h op' i (releaseSandboxMounts_sub hi)
        | true =>
            rcases markSandboxMountsUncertain_shape hi with
              ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, _, heq⟩⟩
            · rw [heq]; exact h op' i0 hi0
            · rw [heq]
              exact hWF (closeHome s m sb) i0 _ rfl
      · simp [step, doSandboxClose, hg] at hi
        exact h op' i hi
  | sandboxForceRemove m sb gen refused =>
      by_cases hg : sandboxForceRemoveOk s m sb gen = true
      · simp only [step, doSandboxForceRemove, hg] at hi
        cases refused with
        | false =>
            exact h op' i (releaseSandboxMounts_sub hi)
        | true =>
            rcases markSandboxMountsUncertain_shape hi with
              ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, _, heq⟩⟩
            · rw [heq]; exact h op' i0 hi0
            · rw [heq]
              exact hWF (forceRemoveHome s m sb gen) i0 _ rfl
      · simp [step, doSandboxForceRemove, hg] at hi
        exact h op' i hi
  | machineStart m inc =>
      by_cases hg : machineStartOk s m inc = true
      · simp [step, doMachineStart, hg, State.mapHome] at hi
        exact h op' i (Store.intents_removeWhere_sub s.store _ op' i hi)
      · simp [step, doMachineStart, hg] at hi
        exact h op' i hi
  | machineRestore m inc =>
      by_cases hg : machineRestoreOk s m inc = true
      · simp [step, doMachineRestore, hg, State.mapHome] at hi
        exact h op' i hi
      · simp [step, doMachineRestore, hg] at hi
        exact h op' i hi
  | admissionRecheck op0 =>
      cases hrow : s.store.intents op0 with
      | none =>
          simp [step, doAdmissionRecheck, hrow] at hi
          exact h op' i hi
      | some i0 =>
          by_cases hd : (i0.state != .done) = true
          · simp [step, doAdmissionRecheck, hrow, hd] at hi
            rcases Store.intents_update_cases hi with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
            · subst hj
              exact hWF s i1 _ rfl
            · exact h op' i hi1
          · simp [step, doAdmissionRecheck, hrow, hd] at hi
            exact h op' i hi
  | branchRemove b =>
      by_cases hg : branchRemoveOk s b = true
      · simp [step, doBranchRemove, hg] at hi
        change (s.store.removeWhere (fun _ i => holdsBranchB s b i &&
          (i.state != .done))).intents op' = some i at hi
        exact h op' i (Store.intents_removeWhere_sub s.store _ op' i hi)
      · simp [step, doBranchRemove, hg] at hi
        exact h op' i hi
  | storeCompact =>
      by_cases hg : allDeadCertainB s = true
      · simp [step, doStoreCompact, hg] at hi
        change (s.store.removeWhere (fun _ i => i.state != .done)).intents op' = some i at hi
        exact h op' i (Store.intents_removeWhere_sub s.store _ op' i hi)
      · simp [step, doStoreCompact, hg] at hi
        exact h op' i hi
  | operatorPreview branch row label =>
      by_cases hg : previewOk s row = true
      · simp [step, doOperatorPreview, hg] at hi
        rcases Store.intents_add_cases hi with ⟨rfl, rfl⟩ | ⟨hne, hi0⟩
        · exact hAW s _ rfl
        · exact h op' i hi0
      · simp [step, doOperatorPreview, hg] at hi
        exact h op' i hi
  | operatorSettle op0 label =>
      cases hrow : s.store.intents op0 with
      | none =>
          simp [step, doOperatorSettle, hrow] at hi
          exact h op' i hi
      | some i0 =>
          by_cases hd : (i0.state != .done) = true
          · simp [step, doOperatorSettle, hrow, hd] at hi
            rcases Store.intents_update_cases hi with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
            · subst hj
              exact hWF s i1 _ rfl
            · exact h op' i hi1
          · simp [step, doOperatorSettle, hrow, hd] at hi
            exact h op' i hi
  | operatorRelease op0 label =>
      by_cases hg : operatorReleaseOk s op0 = true
      · simp [step, doOperatorRelease, hg] at hi
        rcases Store.intents_update_cases hi with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
        · rw [heq] at hi1
          exact hEq (h op0 i1 hi1) (by rw [hj])
        · exact h op' i hi1
      · simp [step, doOperatorRelease, hg] at hi
        exact h op' i hi

theorem Inv3_preserves (op : Op) (s : State) (h3 : Inv3 s) : Inv3 (step op s) := by
  intro op' i w hi hw hc
  have hgood : WitnessGood3 i :=
    WitnessGood_step WitnessGood3
      (fun s i j hw => WitnessGood3_witnessFor hw)
      (fun s i hw => WitnessGood3_audit hw)
      (fun h hw => WitnessGood3_eq h hw)
      s (fun op i hi => fun w0 hw0 hc0 => h3 op i w0 hi hw0 hc0)
      op' i hi
  exact hgood w hw hc

theorem Inv4_preserves (op : Op) (s : State) (h4 : Inv4 s) : Inv4 (step op s) := by
  intro op' i w hi hw hc
  have hgood : WitnessGood4 i :=
    WitnessGood_step WitnessGood4
      (fun s i j hw => WitnessGood4_witnessFor hw)
      (fun s i hw => WitnessGood4_audit hw)
      (fun h hw => WitnessGood4_eq h hw)
      s (fun op i hi => fun w0 hw0 hc0 => h4 op i w0 hi hw0 hc0)
      op' i hi
  exact hgood w hw hc


/-! ## Inv7: done-row retention over the single intent map -/

theorem DoneRow_of_sub {s s' : State} {op : OpId} (h : DoneRow s op)
    (hsub : ∀ i, s.store.intents op = some i → s'.store.intents op = some i) :
    DoneRow s' op := by
  rcases h with ⟨i, hi, hd⟩
  exact ⟨i, hsub i hi, hd⟩

theorem compact_copies_doneRow (s : State) {op : OpId} (h : DoneRow s op) :
    DoneRow (step .storeCompact s) op := by
  rcases h with ⟨i, hi, hd⟩
  by_cases hg : allDeadCertainB s = true
  · have hp : (fun _ i => i.state != .done) op i = false := by simp [hd]
    refine ⟨i, ?_, hd⟩
    simp only [step, doStoreCompact, hg]
    exact (Store.intents_removeWhere_kept s.store (fun _ i => i.state != .done) op i hi hp).trans hi
  · refine ⟨i, ?_, hd⟩
    simp only [step, doStoreCompact, hg]
    exact hi

theorem Inv7_holds (s : State) : Inv7 s := by
  constructor
  · intro ho op hd
    cases ho with
    | sandboxCreate m sb gen =>
        by_cases hg : sandboxCreateOk s m sb gen = true
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxCreate, hg, State.mapHome]
          exact hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxCreate, hg]
          exact hi
    | sandboxMount i0 =>
        by_cases hg : mountOk s i0 = true
        · by_cases hq : op = i0.op
          · exfalso
            rcases hd with ⟨i, hi, hd⟩
            rw [hq] at hi
            have hnone : s.store.intents i0.op = none := by
              by_cases hh : s.store.intents i0.op = none
              · exact hh
              · have h' := hg; simp [mountOk, hh] at h'
            rw [hnone] at hi; simp at hi
          ·
            let i1 : Intent := { i0 with state := .live, witness := some (witnessFor s i0), doneBy := none, uncertain := false, settled := false, action := none }
            let p : OpId → Intent → Bool := fun _ i => i0.hasPlaceEvidenceB && (i.machine == i0.machine) && (i.state != .done) && (!i.uncertain) && (verdict s i == .dead)
            rcases hd with ⟨i, hi, hd⟩
            refine ⟨i, ?_, hd⟩
            have hp : p op i = false := by simp [p, hd]
            simp only [step, doSandboxMount, settlePlace]
            rw [ite_eq_left hg]
            show ((s.store.removeWhere p).addIntent i0.op i1).intents op = some i
            rw [Store.intents_add_other (s.store.removeWhere p) i0.op op i1 hq]
            exact (Store.intents_removeWhere_kept s.store p op i hi hp).trans hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxMount, hg]
          exact hi
    | sandboxUnmount op0 refused =>
        by_cases hg : sandboxUnmountOk s op0 = true
        · by_cases hq : op = op0
          · exfalso
            rcases hd with ⟨i, hi, hd⟩
            rw [hq] at hi
            have h' := hg
            simp [sandboxUnmountOk, hi, hd] at h'
          ·
            rcases hd with ⟨i, hi, hd⟩
            refine ⟨i, ?_, hd⟩
            cases refused with
            | false =>
                simp [step, doSandboxUnmount, hg]
                rw [Store.intents_remove_other s.store op0 op hq]
                exact hi
            | true =>
                simp [step, doSandboxUnmount, hg]
                rw [Store.intents_update_other s.store op0 op _ hq]
                exact hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxUnmount, hg]
          exact hi
    | sandboxClose m sb mounted refused =>
        by_cases hg : sandboxCloseOk s m sb mounted = true
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp only [step, doSandboxClose, hg]
          exact endSandboxMounts_done_kept (closeHome s m sb) m sb refused hi hd
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxClose, hg]
          exact hi
    | sandboxForceRemove m sb gen refused =>
        by_cases hg : sandboxForceRemoveOk s m sb gen = true
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp only [step, doSandboxForceRemove, hg]
          exact endSandboxMounts_done_kept (forceRemoveHome s m sb gen) m sb refused hi hd
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doSandboxForceRemove, hg]
          exact hi
    | machineStart m inc =>
        by_cases hg : machineStartOk s m inc = true
        ·
          let p : OpId → Intent → Bool := fun _ i => reconcileDeadB s m inc i
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          have hp : p op i = false := by simp [p, reconcileDeadB, reconcileDead, hd]
          simp only [step, doMachineStart, hg, State.mapHome]
          exact (Store.intents_removeWhere_kept s.store p op i hi hp).trans hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doMachineStart, hg]
          exact hi
    | machineRestore m inc =>
        by_cases hg : machineRestoreOk s m inc = true
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doMachineRestore, hg, State.mapHome]
          exact hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doMachineRestore, hg]
          exact hi
    | admissionRecheck op0 =>
        cases hrow : s.store.intents op0 with
        | none =>
            rcases hd with ⟨i, hi, hd⟩
            refine ⟨i, ?_, hd⟩
            simp [step, doAdmissionRecheck, hrow]
            exact hi
        | some i0 =>
            by_cases hd' : (i0.state != .done) = true
            · by_cases hq : op = op0
              · exfalso
                rcases hd with ⟨i, hi, hd⟩
                rw [hq] at hi
                have hne : i0.state ≠ .done := by simpa using hd'
                rw [hrow] at hi
                have hii : i = i0 := (Option.some.inj hi).symm
                subst hii
                exact absurd hd hne
              ·
                rcases hd with ⟨i, hi, hd⟩
                refine ⟨i, ?_, hd⟩
                simp [step, doAdmissionRecheck, hrow, hd']
                rw [Store.intents_update_other s.store op0 op _ hq]
                exact hi
            ·
              rcases hd with ⟨i, hi, hd⟩
              refine ⟨i, ?_, hd⟩
              simp [step, doAdmissionRecheck, hrow, hd']
              exact hi
    | branchRemove b =>
        by_cases hg : branchRemoveOk s b = true
        ·
          let p : OpId → Intent → Bool := fun _ i => holdsBranchB s b i && (i.state != .done)
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          have hp : p op i = false := by simp [p, hd]
          simp only [step, doBranchRemove, hg]
          exact (Store.intents_removeWhere_kept s.store p op i hi hp).trans hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doBranchRemove, hg]
          exact hi
    | storeCompact =>
        exact compact_copies_doneRow s hd
    | operatorPreview branch row label =>
        by_cases hg : previewOk s row = true
        · by_cases hq : op = row
          · exfalso
            rcases hd with ⟨i, hi, hd⟩
            rw [hq] at hi
            have hnone : s.store.intents row = none := by
              by_cases hh : s.store.intents row = none
              · exact hh
              · have h' := hg; simp [previewOk, hh] at h'
            rw [hnone] at hi; simp at hi
          ·
            let i : Intent := previewIntent s branch row label
            rcases hd with ⟨j, hj, hd⟩
            refine ⟨j, ?_, hd⟩
            simp [step, doOperatorPreview, hg]
            rw [Store.intents_add_other s.store row op i hq]
            exact hj
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doOperatorPreview, hg]
          exact hi
    | operatorSettle op0 label =>
        cases hrow : s.store.intents op0 with
        | none =>
            rcases hd with ⟨i, hi, hd⟩
            refine ⟨i, ?_, hd⟩
            simp [step, doOperatorSettle, hrow]
            exact hi
        | some i0 =>
            by_cases hd' : (i0.state != .done) = true
            · by_cases hq : op = op0
              · exfalso
                rcases hd with ⟨i, hi, hd⟩
                rw [hq] at hi
                have hne : i0.state ≠ .done := by simpa using hd'
                rw [hrow] at hi
                have hii : i = i0 := (Option.some.inj hi).symm
                subst hii
                exact absurd hd hne
              ·
                rcases hd with ⟨i, hi, hd⟩
                refine ⟨i, ?_, hd⟩
                simp [step, doOperatorSettle, hrow, hd']
                rw [Store.intents_update_other s.store op0 op _ hq]
                exact hi
            ·
              rcases hd with ⟨i, hi, hd⟩
              refine ⟨i, ?_, hd⟩
              simp [step, doOperatorSettle, hrow, hd']
              exact hi
    | operatorRelease op0 label =>
        by_cases hg : operatorReleaseOk s op0 = true
        · by_cases hq : op = op0
          · exfalso
            rcases hd with ⟨i, hi, hd⟩
            rw [hq] at hi
            have h' := hg
            simp [operatorReleaseOk, hi, hd] at h'
          ·
            rcases hd with ⟨i, hi, hd⟩
            refine ⟨i, ?_, hd⟩
            simp [step, doOperatorRelease, hg]
            rw [Store.intents_update_other s.store op0 op _ hq]
            exact hi
        ·
          rcases hd with ⟨i, hi, hd⟩
          refine ⟨i, ?_, hd⟩
          simp [step, doOperatorRelease, hg]
          exact hi
  · intro op hd
    exact compact_copies_doneRow s hd

theorem Inv7_preserves (op : Op) (s : State) : Inv7 (step op s) :=
  Inv7_holds (step op s)

/-- The compact-specific corollary of Inv7. -/
theorem done_survives_compact (s s' : State)
    (hstep : step .storeCompact s = s') :
    ∀ op, DoneRow s op → DoneRow s' op := by
  intro op hd
  rw [← hstep]
  exact compact_copies_doneRow s hd

/-- The force-remove / close effect: after a successful force remove of
Sandbox `(m, sb)`, every visible row that names that Sandbox is a retained
done row or the explicit uncertain marker of the refused release
(`state = uncertain` and `uncertain = true`).  No `.live` row remains; an
uncertain row is retained (and still blocks compact); a row the Store refused
to delete is `Unresolved`, never silently dead. -/
theorem force_remove_ends_mounts (s s' : State) (m : MachineId) (sb : SandboxId)
    (gen : Ordered) (refused : Bool)
    (hok : sandboxForceRemoveOk s m sb gen = true)
    (hstep : step (.sandboxForceRemove m sb gen refused) s = s') :
    SandboxIntentsEnded s' m sb := by
  intro op i hi hm hs
  rw [← hstep] at hi
  simp only [step, doSandboxForceRemove, hok] at hi
  cases refused with
  | false =>
      change (releaseSandboxMounts (forceRemoveHome s m sb gen) m sb).store.intents op = some i at hi
      have hi0 := releaseSandboxMounts_sub hi
      by_cases hd : i.state = .done
      · exact Or.inl hd
      · exfalso
        have hp : endsOnSandboxB m sb i = true := endsOnSandboxB_true hm hs hd
        have hrem : (releaseSandboxMounts (forceRemoveHome s m sb gen) m sb).store.intents op = none :=
          Store.intents_removeWhere_removed _ (fun _ i => endsOnSandboxB m sb i) op i hi0 hp
        rw [hrem] at hi
        simp at hi
  | true =>
      change (markSandboxMountsUncertain (forceRemoveHome s m sb gen) m sb).store.intents op = some i at hi
      rcases markSandboxMountsUncertain_shape hi with
        ⟨i0, hi0, ⟨hpf, heq⟩ | ⟨_, _, heq⟩⟩
      · rw [heq] at hm hs
        by_cases hd : i0.state = .done
        · rw [heq]; exact Or.inl hd
        · exfalso
          have hp : endsOnSandboxB m sb i0 = true := endsOnSandboxB_true hm hs hd
          rw [hp] at hpf
          simp at hpf
      · rw [heq]
        exact Or.inr ⟨rfl, rfl⟩

/-! ## EntriesSound: the support is exact -/

theorem EntriesSound_of_eq {s s' : State} (h : EntriesSound s)
    (he : s'.store.entries = s.store.entries)
    (hi : ∀ op, s'.store.intents op = s.store.intents op) : EntriesSound s' := by
  intro op
  constructor
  · intro hx
    rcases hx with ⟨i, hxi⟩
    have hx0 : ∃ i, s.store.intents op = some i := ⟨i, by rw [← hi op]; exact hxi⟩
    have hmem := (h op).mp hx0
    rwa [he]
  · intro hmem
    have hmem0 : op ∈ s.store.entries := by rwa [he] at hmem
    rcases (h op).mpr hmem0 with ⟨i, hi0⟩
    exact ⟨i, by rw [hi op]; exact hi0⟩

theorem EntriesSound_addIntent {s : State} (h : EntriesSound s) (op : OpId) (i : Intent) :
    EntriesSound { s with store := s.store.addIntent op i } := by
  intro x
  constructor
  · intro hx
    rcases hx with ⟨j, hj⟩
    by_cases hxo : x = op
    · subst x
      simp [Store.addIntent]
    · have hj' : s.store.intents x = some j := by
        simpa [Store.addIntent, Store.intents, hxo] using hj
      have hmem := (h x).mp ⟨j, hj'⟩
      simpa [Store.addIntent, hxo] using hmem
  · intro hmem
    by_cases hxo : x = op
    · subst x
      exact ⟨i, Store.intents_add_self s.store op i⟩
    · have h0 : x ∈ s.store.entries := by simpa [Store.addIntent, hxo] using hmem
      rcases (h x).mpr h0 with ⟨j, hj⟩
      exact ⟨j, (Store.intents_add_other s.store op x i hxo).trans hj⟩

theorem EntriesSound_removeIntent {s : State} (h : EntriesSound s) (op : OpId) :
    EntriesSound { s with store := s.store.removeIntent op } := by
  intro x
  constructor
  · intro hx
    rcases hx with ⟨i, hi⟩
    have hxne : x ≠ op := by
      intro he
      subst he
      rw [Store.intents_remove_self] at hi
      simp at hi
    have hi' : s.store.intents x = some i := by
      rwa [Store.intents_remove_other s.store op x hxne] at hi
    have hmem := (h x).mp ⟨i, hi'⟩
    simp [Store.removeIntent, hxne, hmem]
  · intro hmem
    have hx := List.mem_filter.mp hmem
    have hxne : x ≠ op := by simpa using hx.2
    rcases (h x).mpr hx.1 with ⟨i, hi⟩
    exact ⟨i, (Store.intents_remove_other s.store op x hxne).trans hi⟩

theorem EntriesSound_removeWhere {s : State} (h : EntriesSound s)
    (p : OpId → Intent → Bool) :
    EntriesSound { s with store := s.store.removeWhere p } := by
  intro x
  constructor
  · intro hx
    rcases hx with ⟨i, hi⟩
    cases hm : s.store.intents x with
    | none =>
        have hm' : s.store.intentMap x = none := hm
        simp [Store.removeWhere, Store.intents, hm'] at hi
    | some i0 =>
        have hm' : s.store.intentMap x = some i0 := hm
        by_cases hp : p x i0 = true
        · simp [Store.removeWhere, Store.intents, hm', hp] at hi
        · have hp' : p x i0 = false := by
            cases hv : p x i0 with
            | false => rfl
            | true => exact absurd hv hp
          have hmem := (h x).mp ⟨i0, hm⟩
          simp [Store.removeWhere, Store.intents, hm', hp', hmem]
  · intro hmem
    have hx := List.mem_filter.mp hmem
    have hpred : (match s.store.intents x with
        | some i => !p x i
        | none => true) = true := hx.2
    rcases (h x).mpr hx.1 with ⟨i, hi⟩
    have hi' : s.store.intentMap x = some i := hi
    have hp : p x i = false := by
      have hnot : (!p x i) = true := by simpa [hi] using hpred
      simpa using hnot
    refine ⟨i, ?_⟩
    simp [Store.removeWhere, Store.intents, hi', hp]

theorem EntriesSound_updateIntent {s : State} (h : EntriesSound s)
    (op : OpId) (f : Intent → Intent) (hier : op ∈ s.store.entries) :
    EntriesSound { s with store := s.store.updateIntent op f } := by
  intro x
  constructor
  · intro hx
    rcases hx with ⟨i, hi⟩
    by_cases hx0 : x = op
    · subst x
      simpa [Store.updateIntent] using hier
    · have hi' : s.store.intents x = some i := by
        rwa [Store.intents_update_other s.store op x f hx0] at hi
      have hmem := (h x).mp ⟨i, hi'⟩
      simpa [Store.updateIntent, hx0] using hmem
  · intro hmem
    by_cases hx0 : x = op
    · subst x
      rcases (h op).mpr hier with ⟨i, hi⟩
      exact ⟨f i, Store.intents_update_self s.store op i f hi⟩
    · rcases (h x).mpr hmem with ⟨i, hi⟩
      exact ⟨i, (Store.intents_update_other s.store op x f hx0).trans hi⟩

theorem EntriesSound_updateWhere {s : State} (h : EntriesSound s)
    (p : OpId → Intent → Bool) (f : Intent → Intent) :
    EntriesSound { s with store := s.store.updateWhere p f } := by
  intro x
  constructor
  · intro hx
    rcases hx with ⟨i, hi⟩
    cases hm : s.store.intents x with
    | none =>
        have hm' : s.store.intentMap x = none := hm
        simp [Store.updateWhere, Store.intents, hm'] at hi
    | some i0 => exact (h x).mp ⟨i0, hm⟩
  · intro hmem
    rcases (h x).mpr hmem with ⟨i0, hi0⟩
    rw [Store.intents_updateWhere, hi0]
    dsimp only
    by_cases hp : p x i0 = true
    · exact ⟨f i0, by rw [ite_eq_left hp]⟩
    · exact ⟨i0, by rw [ite_eq_right hp]⟩

theorem EntriesSound_releaseSandboxMounts {s : State} (h : EntriesSound s)
    (m : MachineId) (sb : SandboxId) :
    EntriesSound (releaseSandboxMounts s m sb) :=
  EntriesSound_removeWhere h _

theorem EntriesSound_markSandboxMountsUncertain {s : State} (h : EntriesSound s)
    (m : MachineId) (sb : SandboxId) :
    EntriesSound (markSandboxMountsUncertain s m sb) :=
  EntriesSound_updateWhere h _ _

theorem EntriesSound_endSandboxMounts {s : State} (h : EntriesSound s)
    (m : MachineId) (sb : SandboxId) (refused : Bool) :
    EntriesSound (endSandboxMounts s m sb refused) := by
  cases refused with
  | false => exact EntriesSound_releaseSandboxMounts h m sb
  | true => exact EntriesSound_markSandboxMountsUncertain h m sb

theorem EntriesSound_foldl_removeIntent (s : State) (rows : List OpId) (h : EntriesSound s) :
    EntriesSound { s with store := rows.foldl (fun st op => st.removeIntent op) s.store } := by
  induction rows generalizing s with
  | nil => simpa using h
  | cons r rs ih =>
      simp only [List.foldl_cons]
      exact ih { s with store := s.store.removeIntent r } (EntriesSound_removeIntent h r)

theorem EntriesSound_preserves (op : Op) (s : State) (h : Inv s) : EntriesSound (step op s) := by
  cases op with
  | sandboxCreate m sb gen =>
      by_cases hg : sandboxCreateOk s m sb gen = true
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxCreate, hg, State.mapHome]
        · intro x; simp [step, doSandboxCreate, hg, State.mapHome]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxCreate, hg]
        · intro x; simp [step, doSandboxCreate, hg]
  | sandboxMount i0 =>
      by_cases hg : mountOk s i0 = true
      · let i1 : Intent := { i0 with state := .live, witness := some (witnessFor s i0), doneBy := none, uncertain := false, settled := false, action := none }
        let p : OpId → Intent → Bool := fun _ i => i0.hasPlaceEvidenceB && (i.machine == i0.machine) && (i.state != .done) && (!i.uncertain) && (verdict s i == .dead)
        have h1 : EntriesSound { s with store := s.store.removeWhere p } := EntriesSound_removeWhere h.entriesSound p
        have h2 : EntriesSound { s with store := (s.store.removeWhere p).addIntent i0.op i1 } := EntriesSound_addIntent h1 i0.op i1
        refine EntriesSound_of_eq h2 ?_ ?_
        · simp [step, doSandboxMount, hg, settlePlace, p, i1]
        · intro x; simp [step, doSandboxMount, hg, settlePlace, Store.intents, p, i1]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxMount, hg]
        · intro x; simp [step, doSandboxMount, hg]
  | sandboxUnmount op0 refused =>
      by_cases hg : sandboxUnmountOk s op0 = true
      · have hsome : ∃ i, s.store.intents op0 = some i := by
          cases hv : s.store.intents op0 with
          | none => simp [sandboxUnmountOk, hv] at hg
          | some i => exact ⟨i, rfl⟩
        rcases hsome with ⟨i0, hi0⟩
        have hmem : op0 ∈ s.store.entries := (h.entriesSound op0).mp ⟨i0, hi0⟩
        cases refused with
        | false =>
            refine EntriesSound_of_eq (EntriesSound_removeIntent h.entriesSound op0) ?_ ?_
            · simp [step, doSandboxUnmount, hg]
            · intro x; simp [step, doSandboxUnmount, hg]
        | true =>
            refine EntriesSound_of_eq (EntriesSound_updateIntent h.entriesSound op0 (fun i => { i with state := .uncertain, uncertain := true, witness := some (witnessFor s i) }) hmem) ?_ ?_
            · simp [step, doSandboxUnmount, hg]
            · intro x; simp [step, doSandboxUnmount, hg]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxUnmount, hg]
        · intro x; simp [step, doSandboxUnmount, hg]
  | sandboxClose m sb mounted refused =>
      by_cases hg : sandboxCloseOk s m sb mounted = true
      · have h1 : EntriesSound (closeHome s m sb) := by
          refine EntriesSound_of_eq h.entriesSound ?_ ?_
          · simp [closeHome, State.mapHome]
          · intro x; simp [closeHome, State.mapHome]
        simp only [step, doSandboxClose, hg]
        cases refused with
        | false =>
            change EntriesSound (releaseSandboxMounts (closeHome s m sb) m sb)
            exact EntriesSound_releaseSandboxMounts h1 m sb
        | true =>
            change EntriesSound (markSandboxMountsUncertain (closeHome s m sb) m sb)
            exact EntriesSound_markSandboxMountsUncertain h1 m sb
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxClose, hg]
        · intro x; simp [step, doSandboxClose, hg]
  | sandboxForceRemove m sb gen refused =>
      by_cases hg : sandboxForceRemoveOk s m sb gen = true
      · have h1 : EntriesSound (forceRemoveHome s m sb gen) := by
          refine EntriesSound_of_eq h.entriesSound ?_ ?_
          · simp [forceRemoveHome, State.mapHome]
          · intro x; simp [forceRemoveHome, State.mapHome]
        simp only [step, doSandboxForceRemove, hg]
        cases refused with
        | false =>
            change EntriesSound (releaseSandboxMounts (forceRemoveHome s m sb gen) m sb)
            exact EntriesSound_releaseSandboxMounts h1 m sb
        | true =>
            change EntriesSound (markSandboxMountsUncertain (forceRemoveHome s m sb gen) m sb)
            exact EntriesSound_markSandboxMountsUncertain h1 m sb
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doSandboxForceRemove, hg]
        · intro x; simp [step, doSandboxForceRemove, hg]
  | machineStart m inc =>
      by_cases hg : machineStartOk s m inc = true
      · let p : OpId → Intent → Bool := fun _ i => reconcileDeadB s m inc i
        refine EntriesSound_of_eq (EntriesSound_removeWhere h.entriesSound p) ?_ ?_
        · simp [step, doMachineStart, hg, State.mapHome, p]
        · intro x; simp [step, doMachineStart, hg, State.mapHome, p]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doMachineStart, hg]
        · intro x; simp [step, doMachineStart, hg]
  | machineRestore m inc =>
      by_cases hg : machineRestoreOk s m inc = true
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doMachineRestore, hg, State.mapHome]
        · intro x; simp [step, doMachineRestore, hg, State.mapHome]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doMachineRestore, hg]
        · intro x; simp [step, doMachineRestore, hg]
  | admissionRecheck op0 =>
      cases hrow : s.store.intents op0 with
      | none =>
          refine EntriesSound_of_eq h.entriesSound ?_ ?_
          · simp [step, doAdmissionRecheck, hrow]
          · intro x; simp [step, doAdmissionRecheck, hrow]
      | some i0 =>
          have hmem : op0 ∈ s.store.entries := (h.entriesSound op0).mp ⟨i0, hrow⟩
          by_cases hd : (i0.state != .done) = true
          · refine EntriesSound_of_eq (EntriesSound_updateIntent h.entriesSound op0 (fun i => { i with witness := some (witnessFor s i) }) hmem) ?_ ?_
            · simp [step, doAdmissionRecheck, hrow, hd]
            · intro x; simp [step, doAdmissionRecheck, hrow, hd]
          · refine EntriesSound_of_eq h.entriesSound ?_ ?_
            · simp [step, doAdmissionRecheck, hrow, hd]
            · intro x; simp [step, doAdmissionRecheck, hrow, hd]
  | branchRemove b =>
      by_cases hg : branchRemoveOk s b = true
      · let p : OpId → Intent → Bool := fun _ i => holdsBranchB s b i && (i.state != .done)
        refine EntriesSound_of_eq (EntriesSound_removeWhere h.entriesSound p) ?_ ?_
        · simp [step, doBranchRemove, hg, p]
        · intro x; simp [step, doBranchRemove, hg, Store.intents, p]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doBranchRemove, hg]
        · intro x; simp [step, doBranchRemove, hg]
  | storeCompact =>
      by_cases hg : allDeadCertainB s = true
      · let p : OpId → Intent → Bool := fun _ i => i.state != .done
        refine EntriesSound_of_eq (EntriesSound_removeWhere h.entriesSound p) ?_ ?_
        · simp [step, doStoreCompact, hg, p]
        · intro x; simp [step, doStoreCompact, hg, Store.intents, p]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doStoreCompact, hg]
        · intro x; simp [step, doStoreCompact, hg]
  | operatorPreview branch row label =>
      by_cases hg : previewOk s row = true
      · let i : Intent := previewIntent s branch row label
        refine EntriesSound_of_eq (EntriesSound_addIntent h.entriesSound row i) ?_ ?_
        · simp [step, doOperatorPreview, hg, i]
        · intro x; simp [step, doOperatorPreview, hg, i]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doOperatorPreview, hg]
        · intro x; simp [step, doOperatorPreview, hg]
  | operatorSettle op0 label =>
      cases hrow : s.store.intents op0 with
      | none =>
          refine EntriesSound_of_eq h.entriesSound ?_ ?_
          · simp [step, doOperatorSettle, hrow]
          · intro x; simp [step, doOperatorSettle, hrow]
      | some i0 =>
          have hmem : op0 ∈ s.store.entries := (h.entriesSound op0).mp ⟨i0, hrow⟩
          by_cases hd : (i0.state != .done) = true
          · refine EntriesSound_of_eq (EntriesSound_updateIntent h.entriesSound op0 (fun i => { i with state := .uncertain, uncertain := true, settled := true, action := some .settle, evidence := evidenceFor "settle" label op0, witness := some (witnessFor s i) }) hmem) ?_ ?_
            · simp [step, doOperatorSettle, hrow, hd]
            · intro x; simp [step, doOperatorSettle, hrow, hd]
          · refine EntriesSound_of_eq h.entriesSound ?_ ?_
            · simp [step, doOperatorSettle, hrow, hd]
            · intro x; simp [step, doOperatorSettle, hrow, hd]
  | operatorRelease op0 label =>
      by_cases hg : operatorReleaseOk s op0 = true
      · cases hrow : s.store.intents op0 with
        | none => simp [operatorReleaseOk, hrow] at hg
        | some i0 =>
            have hmem : op0 ∈ s.store.entries := (h.entriesSound op0).mp ⟨i0, hrow⟩
            refine EntriesSound_of_eq (EntriesSound_updateIntent h.entriesSound op0 (fun i => { i with state := .done, uncertain := false, settled := true, doneBy := some .operator, action := some .release, ownerBoot := none, evidence := evidenceFor "release" label op0 }) hmem) ?_ ?_
            · simp [step, doOperatorRelease, hg]
            · intro x; simp [step, doOperatorRelease, hg]
      · refine EntriesSound_of_eq h.entriesSound ?_ ?_
        · simp [step, doOperatorRelease, hg]
        · intro x; simp [step, doOperatorRelease, hg]

/-! ## Base case -/

def emptyState : State :=
  { store := Store.empty, home := none, branchProject := fun _ => none }

theorem empty_inv : Inv emptyState := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩
  · refine ⟨?_, ?_, ?_⟩
    · intro op i hi; simp [emptyState, Store.empty, Store.intents] at hi
    · intro b hb; simp [emptyState, Store.empty] at hb
    · intro hc; simp [emptyState, Store.empty] at hc
  · constructor
    · intro op i hi; simp [emptyState, Store.empty, Store.intents] at hi
    · intro op i hi; simp [emptyState, Store.empty, Store.intents] at hi
  · exact Inv7_holds emptyState
  · intro op i hi; simp [emptyState, Store.empty, Store.intents] at hi
  · intro op
    constructor
    · intro hx
      rcases hx with ⟨i, hi⟩
      simp [emptyState, Store.empty, Store.intents] at hi
    · intro hmem
      simp [emptyState, Store.empty] at hmem

/-! ## Inv2: operator done rows and uncertain rows -/

theorem Inv2_of_sub {s s' : State} (h : Inv2 s)
    (hsub : ∀ op i, s'.store.intents op = some i → s.store.intents op = some i) :
    Inv2 s' := by
  constructor
  · intro op i hi hdone
    exact h.1 op i (hsub op i hi) hdone
  · intro op i hi hunc
    exact h.2 op i (hsub op i hi) hunc

theorem Inv2_of_eq {s s' : State} (h : Inv2 s)
    (heq : ∀ op, s'.store.intents op = s.store.intents op) : Inv2 s' :=
  Inv2_of_sub h (fun op i hi => by rw [← heq op]; exact hi)

theorem mountOk_live {s : State} {i : Intent} (h : mountOk s i = true) :
    i.state = .live := by
  by_cases hne : i.state = .live
  · exact hne
  · have h' := h
    simp [mountOk, hne] at h' 

theorem Inv2_mount (s : State) (i : Intent) (h : Inv2 s) :
    Inv2 (doSandboxMount s i) := by
  by_cases hg : mountOk s i = true
  · have hlive := mountOk_live hg
    constructor
    · intro op j hj hdone
      simp [doSandboxMount, hg, settlePlace] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · unfold OperatorDone at hdone
        simp at hdone
      · exact h.1 op j (Store.intents_removeWhere_sub s.store _ op j hj0) hdone
    · intro op j hj hunc
      simp [doSandboxMount, hg, settlePlace] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · simp at hunc
      · exact h.2 op j (Store.intents_removeWhere_sub s.store _ op j hj0) hunc
  · simp [doSandboxMount, hg]
    exact h

theorem Inv2_recheck (s : State) (op0 : OpId) (h : Inv2 s) :
    Inv2 (doAdmissionRecheck s op0) := by
  cases hrow : s.store.intents op0 with
  | none => simp [doAdmissionRecheck, hrow]; exact h
  | some i0 =>
      by_cases hd : (i0.state != .done) = true
      · have hi0ne : i0.state ≠ .done := by
          intro hc; rw [hc] at hd; simp at hd
        constructor
        · intro op j hj hdone
          simp [doAdmissionRecheck, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · have hi1' : i1 = i0 := by
              rw [hrow] at hi1
              exact (Option.some.inj hi1).symm
            subst hi1'
            unfold OperatorDone at hdone
            rw [hj] at hdone
            exact absurd hdone.1 hi0ne
          · exact h.1 op j hi1 hdone
        · intro op j hj hunc
          simp [doAdmissionRecheck, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · rw [hj]
            exact ⟨witnessFor s i1, rfl, witnessFor_consistent s i1⟩
          · exact h.2 op j hi1 hunc
      · simp [doAdmissionRecheck, hrow, hd]
        exact h

theorem Inv2_settle (s : State) (op0 : OpId) (label : String) (h : Inv2 s) :
    Inv2 (doOperatorSettle s op0 label) := by
  cases hrow : s.store.intents op0 with
  | none => simp [doOperatorSettle, hrow]; exact h
  | some i0 =>
      by_cases hd : (i0.state != .done) = true
      · constructor
        · intro op j hj hdone
          simp [doOperatorSettle, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · rw [hj] at hdone
            unfold OperatorDone at hdone
            simp at hdone
          · exact h.1 op j hi1 hdone
        · intro op j hj hunc
          simp [doOperatorSettle, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · rw [hj]
            exact ⟨witnessFor s i1, rfl, witnessFor_consistent s i1⟩
          · exact h.2 op j hi1 hunc
      · simp [doOperatorSettle, hrow, hd]
        exact h

theorem Inv2_release (s : State) (op0 : OpId) (label : String)
    (h : Inv2 s) :
    Inv2 (doOperatorRelease s op0 label) := by
  by_cases hg : operatorReleaseOk s op0 = true
  · constructor
    · intro op j hj hdone
      simp [doOperatorRelease, hg] at hj
      rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
      · rw [heq] at hi1
        have hunc : i1.state = .uncertain := by
          have h' := hg
          unfold operatorReleaseOk at h'
          rw [hi1] at h'
          simp at h'
          exact h'.1
        rcases h.2 op0 i1 hi1 hunc with ⟨w, hw, hc⟩
        refine ⟨w, ?_, hc, ?_⟩
        · rw [hj]; exact hw
        · right; right; simp [hj]
      · exact h.1 op j hi1 hdone
    · intro op j hj hunc
      simp [doOperatorRelease, hg] at hj
      rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
      · rw [hj] at hunc; simp at hunc
      · exact h.2 op j hi1 hunc
  · simp [doOperatorRelease, hg]
    exact h

theorem Inv2_preview (s : State) (branch : BranchId) (row : OpId)
    (label : String) (h : Inv2 s) :
    Inv2 (doOperatorPreview s branch row label) := by
  by_cases hg : previewOk s row = true
  · constructor
    · intro op j hj hdone
      simp [doOperatorPreview, hg] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · unfold OperatorDone at hdone
        simp [previewIntent] at hdone
      · exact h.1 op j hj0 hdone
    · intro op j hj hunc
      simp [doOperatorPreview, hg] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · simp [previewIntent] at hunc
      · exact h.2 op j hj0 hunc
  · simp [doOperatorPreview, hg]
    exact h

/-! ## Inv8: done rows are operator-created -/

theorem Inv8_of_sub {s s' : State} (h : Inv8_no_automatic_release s)
    (hsub : ∀ op i, s'.store.intents op = some i → s.store.intents op = some i) :
    Inv8_no_automatic_release s' := by
  intro op i hi hdone
  exact h op i (hsub op i hi) hdone

theorem Inv8_of_eq {s s' : State} (h : Inv8_no_automatic_release s)
    (heq : ∀ op, s'.store.intents op = s.store.intents op) :
    Inv8_no_automatic_release s' :=
  Inv8_of_sub h (fun op i hi => by rw [← heq op]; exact hi)

theorem Inv8_of_fields {s s' : State} (h : Inv8_no_automatic_release s)
    (hfields : ∀ op i, s'.store.intents op = some i →
      ∃ i0, s.store.intents op = some i0 ∧ i.state = i0.state ∧ i.doneBy = i0.doneBy) :
    Inv8_no_automatic_release s' := by
  intro op i hi hdone
  rcases hfields op i hi with ⟨i0, hi0, hstate, hdoneBy⟩
  have hd : i0.state = .done := by rw [← hstate]; exact hdone
  rw [hdoneBy]
  exact h op i0 hi0 hd

theorem Inv8_release (s : State) (op0 : OpId) (label : String)
    (h : Inv8_no_automatic_release s) :
    Inv8_no_automatic_release (doOperatorRelease s op0 label) := by
  by_cases hg : operatorReleaseOk s op0 = true
  · intro op j hj hdone
    simp [doOperatorRelease, hg] at hj
    rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
    · rw [hj]
    · exact h op j hi1 hdone
  · simp [doOperatorRelease, hg]
    exact h

theorem Inv8_preview (s : State) (branch : BranchId) (row : OpId)
    (label : String) (h : Inv8_no_automatic_release s) :
    Inv8_no_automatic_release (doOperatorPreview s branch row label) := by
  by_cases hg : previewOk s row = true
  · intro op j hj hdone
    simp [doOperatorPreview, hg] at hj
    rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
    · simp [previewIntent]
    · exact h op j hj0 hdone
  · simp [doOperatorPreview, hg]
    exact h

theorem Store.intents_compact_done {s : Store} {op : OpId} {i : Intent}
    (h : (s.removeWhere (fun _ i => i.state != .done)).intents op = some i) :
    i.state = .done := by
  by_cases hd : i.state = .done
  · exact hd
  · exfalso
    have hi := Store.intents_removeWhere_sub s _ op i h
    have hp : (fun _ i => i.state != .done) op i = true := by simp [hd]
    have hrem := Store.intents_removeWhere_removed s (fun _ i => i.state != .done) op i hi hp
    rw [hrem] at h
    simp at h

/-! ### The Sandbox-intent end helpers -/

theorem Inv2_endSandboxMounts (s : State) (m : MachineId) (sb : SandboxId)
    (refused : Bool) (h : Inv2 s) :
    Inv2 (endSandboxMounts s m sb refused) := by
  cases refused with
  | false =>
      apply Inv2_of_sub h
      intro op j hj
      exact releaseSandboxMounts_sub hj
  | true =>
      constructor
      · intro op j hj hdone
        rcases markSandboxMountsUncertain_shape hj with
          ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, _, heq⟩⟩
        · rw [heq] at hdone ⊢; exact h.1 op i0 hi0 hdone
        · rw [heq] at hdone ⊢
          unfold OperatorDone at hdone
          simp at hdone
      · intro op j hj hunc
        rcases markSandboxMountsUncertain_shape hj with
          ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, _, heq⟩⟩
        · rw [heq] at hunc ⊢; exact h.2 op i0 hi0 hunc
        · rw [heq]
          exact ⟨witnessFor s i0, rfl, witnessFor_consistent s i0⟩

theorem Inv8_endSandboxMounts (s : State) (m : MachineId) (sb : SandboxId)
    (refused : Bool) (h : Inv8_no_automatic_release s) :
    Inv8_no_automatic_release (endSandboxMounts s m sb refused) := by
  cases refused with
  | false =>
      apply Inv8_of_sub h
      intro op j hj
      exact releaseSandboxMounts_sub hj
  | true =>
      intro op j hj hdone
      rcases markSandboxMountsUncertain_shape hj with
        ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, _, heq⟩⟩
      · rw [heq] at hdone ⊢; exact h op i0 hi0 hdone
      · rw [heq] at hdone ⊢
        exact absurd hdone (by simp)

/-! ## Preservation of Inv2 and Inv8 -/

theorem Inv2_preserves (op : Op) (s : State) (h : Inv s) : Inv2 (step op s) := by
  cases op with
  | sandboxCreate m sb gen =>
      apply Inv2_of_eq h.inv2
      intro op
      by_cases hg : sandboxCreateOk s m sb gen = true
      · simp [step, doSandboxCreate, hg, State.mapHome]
      · simp [step, doSandboxCreate, hg]
  | sandboxMount i =>
      simpa [step] using Inv2_mount s i h.inv2
  | sandboxUnmount op0 refused =>
      by_cases hg : sandboxUnmountOk s op0 = true
      · cases refused with
        | false =>
            apply Inv2_of_sub h.inv2
            intro op j hj
            simp [step, doSandboxUnmount, hg] at hj
            exact Store.intents_remove_sub hj
        | true =>
            constructor
            · intro op j hj hdone
              simp [step, doSandboxUnmount, hg] at hj
              rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
              · rw [hj] at hdone
                unfold OperatorDone at hdone
                simp at hdone
              · exact h.inv2.1 op j hi1 hdone
            · intro op j hj hunc
              simp [step, doSandboxUnmount, hg] at hj
              rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
              · rw [hj]
                exact ⟨witnessFor s i1, rfl, witnessFor_consistent s i1⟩
              · exact h.inv2.2 op j hi1 hunc
      · simpa [step, doSandboxUnmount, hg] using h.inv2
  | sandboxClose m sb mounted refused =>
      by_cases hg : sandboxCloseOk s m sb mounted = true
      · have h1 : Inv2 (closeHome s m sb) := by
          apply Inv2_of_eq h.inv2
          intro x
          simp [closeHome, State.mapHome]
        simp only [step, doSandboxClose, hg]
        exact Inv2_endSandboxMounts (closeHome s m sb) m sb refused h1
      · simpa [step, doSandboxClose, hg] using h.inv2
  | sandboxForceRemove m sb gen refused =>
      by_cases hg : sandboxForceRemoveOk s m sb gen = true
      · have h1 : Inv2 (forceRemoveHome s m sb gen) := by
          apply Inv2_of_eq h.inv2
          intro x
          simp [forceRemoveHome, State.mapHome]
        simp only [step, doSandboxForceRemove, hg]
        exact Inv2_endSandboxMounts (forceRemoveHome s m sb gen) m sb refused h1
      · simpa [step, doSandboxForceRemove, hg] using h.inv2
  | machineStart m inc =>
      by_cases hg : machineStartOk s m inc = true
      · apply Inv2_of_sub h.inv2
        intro op j hj
        simp [step, doMachineStart, hg, State.mapHome] at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doMachineStart, hg] using h.inv2
  | machineRestore m inc =>
      apply Inv2_of_eq h.inv2
      intro op
      by_cases hg : machineRestoreOk s m inc = true
      · simp [step, doMachineRestore, hg, State.mapHome]
      · simp [step, doMachineRestore, hg]
  | admissionRecheck op0 =>
      simpa [step] using Inv2_recheck s op0 h.inv2
  | branchRemove b =>
      by_cases hg : branchRemoveOk s b = true
      · apply Inv2_of_sub h.inv2
        intro op j hj
        simp [step, doBranchRemove, hg] at hj
        change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doBranchRemove, hg] using h.inv2
  | storeCompact =>
      by_cases hg : allDeadCertainB s = true
      · apply Inv2_of_sub h.inv2
        intro op j hj
        simp [step, doStoreCompact, hg] at hj
        change (s.store.removeWhere (fun _ i => i.state != .done)).intents op = some j at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doStoreCompact, hg] using h.inv2
  | operatorPreview branch row label =>
      simpa [step] using Inv2_preview s branch row label h.inv2
  | operatorSettle op0 label =>
      simpa [step] using Inv2_settle s op0 label h.inv2
  | operatorRelease op0 label =>
      simpa [step] using Inv2_release s op0 label h.inv2

theorem Inv8_mount (s : State) (i : Intent) (h : Inv8_no_automatic_release s) :
    Inv8_no_automatic_release (doSandboxMount s i) := by
  by_cases hg : mountOk s i = true
  · intro op j hj hdone
    simp [doSandboxMount, hg, settlePlace] at hj
    rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
    · simp at hdone
    · exact h op j (Store.intents_removeWhere_sub s.store _ op j hj0) hdone
  · simp [doSandboxMount, hg]
    exact h

theorem Inv8_settle (s : State) (op0 : OpId) (label : String)
    (h : Inv8_no_automatic_release s) :
    Inv8_no_automatic_release (doOperatorSettle s op0 label) := by
  cases hrow : s.store.intents op0 with
  | none => simp [doOperatorSettle, hrow]; exact h
  | some i0 =>
      by_cases hd : (i0.state != .done) = true
      · intro op j hj hdone
        simp [doOperatorSettle, hrow, hd] at hj
        rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
        · rw [heq] at hi1
          rw [hj] at hdone
          simp at hdone
        · exact h op j hi1 hdone
      · simp [doOperatorSettle, hrow, hd]
        exact h

theorem Inv8_preserves (op : Op) (s : State) (h : Inv s) :
    Inv8_no_automatic_release (step op s) := by
  cases op with
  | sandboxCreate m sb gen =>
      apply Inv8_of_eq h.inv8
      intro op
      by_cases hg : sandboxCreateOk s m sb gen = true
      · simp [step, doSandboxCreate, hg, State.mapHome]
      · simp [step, doSandboxCreate, hg]
  | sandboxMount i =>
      simpa [step] using Inv8_mount s i h.inv8
  | sandboxUnmount op0 refused =>
      by_cases hg : sandboxUnmountOk s op0 = true
      · cases refused with
        | false =>
            apply Inv8_of_sub h.inv8
            intro op j hj
            simp [step, doSandboxUnmount, hg] at hj
            exact Store.intents_remove_sub hj
        | true =>
            intro op j hj hdone
            simp [step, doSandboxUnmount, hg] at hj
            rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
            · rw [hj] at hdone; simp at hdone
            · exact h.inv8 op j hi1 hdone
      · simpa [step, doSandboxUnmount, hg] using h.inv8
  | sandboxClose m sb mounted refused =>
      by_cases hg : sandboxCloseOk s m sb mounted = true
      · have h1 : Inv8_no_automatic_release (closeHome s m sb) := by
          apply Inv8_of_eq h.inv8
          intro x
          simp [closeHome, State.mapHome]
        simp only [step, doSandboxClose, hg]
        exact Inv8_endSandboxMounts (closeHome s m sb) m sb refused h1
      · simpa [step, doSandboxClose, hg] using h.inv8
  | sandboxForceRemove m sb gen refused =>
      by_cases hg : sandboxForceRemoveOk s m sb gen = true
      · have h1 : Inv8_no_automatic_release (forceRemoveHome s m sb gen) := by
          apply Inv8_of_eq h.inv8
          intro x
          simp [forceRemoveHome, State.mapHome]
        simp only [step, doSandboxForceRemove, hg]
        exact Inv8_endSandboxMounts (forceRemoveHome s m sb gen) m sb refused h1
      · simpa [step, doSandboxForceRemove, hg] using h.inv8
  | machineStart m inc =>
      by_cases hg : machineStartOk s m inc = true
      · apply Inv8_of_sub h.inv8
        intro op j hj
        simp [step, doMachineStart, hg, State.mapHome] at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doMachineStart, hg] using h.inv8
  | machineRestore m inc =>
      apply Inv8_of_eq h.inv8
      intro op
      by_cases hg : machineRestoreOk s m inc = true
      · simp [step, doMachineRestore, hg, State.mapHome]
      · simp [step, doMachineRestore, hg]
  | admissionRecheck op0 =>
      apply Inv8_of_fields h.inv8
      intro op j hj
      cases hrow : s.store.intents op0 with
      | none => simp [step, doAdmissionRecheck, hrow] at hj; exact ⟨j, hj, rfl, rfl⟩
      | some i0 =>
          by_cases hd : (i0.state != .done) = true
          · simp [step, doAdmissionRecheck, hrow, hd] at hj
            rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
            · rw [heq] at hi1
              rw [heq]
              exact ⟨i1, hi1, by rw [hj], by rw [hj]⟩
            · exact ⟨j, hi1, rfl, rfl⟩
          · simp [step, doAdmissionRecheck, hrow, hd] at hj
            exact ⟨j, hj, rfl, rfl⟩
  | branchRemove b =>
      by_cases hg : branchRemoveOk s b = true
      · apply Inv8_of_sub h.inv8
        intro op j hj
        simp [step, doBranchRemove, hg] at hj
        change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doBranchRemove, hg] using h.inv8
  | storeCompact =>
      by_cases hg : allDeadCertainB s = true
      · apply Inv8_of_sub h.inv8
        intro op j hj
        simp [step, doStoreCompact, hg] at hj
        change (s.store.removeWhere (fun _ i => i.state != .done)).intents op = some j at hj
        exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doStoreCompact, hg] using h.inv8
  | operatorPreview branch row label =>
      simpa [step] using Inv8_preview s branch row label h.inv8
  | operatorSettle op0 label =>
      simpa [step] using Inv8_settle s op0 label h.inv8
  | operatorRelease op0 label =>
      simpa [step] using Inv8_release s op0 label h.inv8

/-! ## Inv1: no reclaim of a live mount -/

theorem holdsBranchB_iff {s : State} {b : BranchId} {i : Intent} :
    holdsBranchB s b i = true ↔ HoldsBranch s b i := by
  cases hb : i.branch with
  | some b' => simp [holdsBranchB, HoldsBranch, hb]
  | none =>
      cases hp : i.project with
      | none => simp [holdsBranchB, HoldsBranch, hb, hp]
      | some p =>
          cases hbp : s.branchProject b with
          | none => simp [holdsBranchB, HoldsBranch, hb, hp, hbp]
          | some q =>
              simp [holdsBranchB, HoldsBranch, hb, hp, hbp, beq_iff_eq]
              exact eq_comm

theorem MountedOn_of_sub {s s' : State} {b : BranchId}
    (hbp : s'.branchProject = s.branchProject)
    (hsub : ∀ op i, s'.store.intents op = some i → s.store.intents op = some i)
    (h : MountedOn s' b) : MountedOn s b := by
  rcases h with ⟨op, i, hi, hh, hnd⟩
  refine ⟨op, i, hsub op i hi, ?_, hnd⟩
  unfold HoldsBranch at hh ⊢
  rcases hh with hh | hh | hh
  · exact Or.inl hh
  · exact Or.inr (Or.inl hh)
  · rcases hh with ⟨hb, p, hp, hbp'⟩
    exact Or.inr (Or.inr ⟨hb, p, hp, by rw [← hbp]; exact hbp'⟩)

theorem HoldsBranch_of_branchProject {s s' : State} {b : BranchId} {i : Intent}
    (hbp : s'.branchProject b = s.branchProject b) (h : HoldsBranch s' b i) :
    HoldsBranch s b i := by
  unfold HoldsBranch at h ⊢
  rcases h with hh | hh | hh
  · exact Or.inl hh
  · exact Or.inr (Or.inl hh)
  · rcases hh with ⟨hb, p, hp, hbp'⟩
    exact Or.inr (Or.inr ⟨hb, p, hp, by rw [← hbp]; exact hbp'⟩)

theorem Inv1_of_sub {s s' : State} (h : Inv1 s)
    (hbp : ∀ b, s'.branchProject b = s.branchProject b)
    (hbr : ∀ b, s'.store.branches b = s.store.branches b)
    (hcomp : s'.store.compacted = s.store.compacted)
    (hsub : ∀ op i, s'.store.intents op = some i → i.state ≠ .done →
        s.store.intents op = some i) : Inv1 s' := by
  refine ⟨?_, ?_, ?_⟩
  · intro op i hi hnd b hb
    rw [hbr b]
    exact h.1 op i (hsub op i hi hnd) hnd b hb
  · intro b hb
    have hb0 : s.store.branches b = some .removed := by rw [← hbr b]; exact hb
    rintro ⟨op, i, hi, hh, hd⟩
    exact h.2.1 b hb0 ⟨op, i, hsub op i hi hd,
      HoldsBranch_of_branchProject (hbp b) hh, hd⟩
  · intro hc
    rw [hcomp] at hc
    rintro ⟨op, i, hi, hd⟩
    exact h.2.2 hc ⟨op, i, hsub op i hi hd, hd⟩

theorem Inv1_of_fields {s s' : State} (h : Inv1 s)
    (hbp : ∀ b, s'.branchProject b = s.branchProject b)
    (hbr : ∀ b, s'.store.branches b = s.store.branches b)
    (hcomp : s'.store.compacted = s.store.compacted)
    (hfields : ∀ op i, s'.store.intents op = some i →
      ∃ i0, s.store.intents op = some i0 ∧ i.branch = i0.branch ∧
            i.project = i0.project ∧ (i.state ≠ .done → i0.state ≠ .done)) :
    Inv1 s' := by
  refine ⟨?_, ?_, ?_⟩
  · intro op i hi hnd b hb
    rcases hfields op i hi with ⟨i0, hi0, hbrch, hproj, hnd0⟩
    rw [hbr b]
    exact h.1 op i0 hi0 (hnd0 hnd) b (by rw [← hbrch]; exact hb)
  · intro b hb
    have hb0 : s.store.branches b = some .removed := by rw [← hbr b]; exact hb
    rintro ⟨op, i, hi, hh, hd⟩
    rcases hfields op i hi with ⟨i0, hi0, hbrch, hproj, hnd0⟩
    refine h.2.1 b hb0 ⟨op, i0, hi0, ?_, hnd0 hd⟩
    unfold HoldsBranch at hh ⊢
    rcases hh with hh | hh | hh
    · exact Or.inl (by rw [← hbrch]; exact hh)
    · exact Or.inr (Or.inl ⟨by rw [← hbrch]; exact hh.1,
                              by rw [← hproj]; exact hh.2⟩)
    · rcases hh with ⟨hbranch, p, hp, hbp'⟩
      exact Or.inr (Or.inr ⟨by rw [← hbrch]; exact hbranch, p, by rw [← hproj]; exact hp,
        by rw [← hbp b]; exact hbp'⟩)
  · intro hc
    rw [hcomp] at hc
    rintro ⟨op, i, hi, hd⟩
    rcases hfields op i hi with ⟨i0, hi0, _, _, hnd0⟩
    exact h.2.2 hc ⟨op, i0, hi0, hnd0 hd⟩

theorem Inv1_endSandboxMounts (s : State) (m : MachineId) (sb : SandboxId)
    (refused : Bool) (h : Inv1 s) :
    Inv1 (endSandboxMounts s m sb refused) := by
  cases refused with
  | false =>
      apply Inv1_of_sub h
      · intro b; rfl
      · intro b; rfl
      · rfl
      · intro op j hj hnd
        exact releaseSandboxMounts_sub hj
  | true =>
      apply Inv1_of_fields h
      · intro b; rfl
      · intro b; rfl
      · rfl
      · intro op j hj
        rcases markSandboxMountsUncertain_shape hj with
          ⟨i0, hi0, ⟨_, heq⟩ | ⟨_, hnd, heq⟩⟩
        · refine ⟨i0, hi0, ?_, ?_, ?_⟩
          · rw [heq]
          · rw [heq]
          · intro h; rw [heq] at h; exact h
        · refine ⟨i0, hi0, ?_, ?_, ?_⟩
          · rw [heq]
          · rw [heq]
          · intro _; exact hnd

theorem mountOk_branch {s : State} {i : Intent} (h : mountOk s i = true) :
    ∃ b, i.branch = some b ∧ s.store.branches b = some .live := by
  cases hb : i.branch with
  | none => simp [mountOk, hb] at h
  | some b =>
      refine ⟨b, rfl, ?_⟩
      have h' := h
      simp [mountOk, hb] at h'
      exact h'.2

theorem Inv1_mount (s : State) (i : Intent) (h : Inv1 s) :
    Inv1 (doSandboxMount s i) := by
  by_cases hg : mountOk s i = true
  · have hbranches : ∀ b, (doSandboxMount s i).store.branches b = s.store.branches b := by
      intro b
      simp [doSandboxMount, hg, settlePlace, Store.addIntent, Store.removeWhere]
    have hcomp : (doSandboxMount s i).store.compacted = false := by
      simp [doSandboxMount, hg]
    obtain ⟨b0, hb0, hlive0⟩ := mountOk_branch hg
    refine ⟨?_, ?_, ?_⟩
    · intro op j hj hnd b hb
      simp [doSandboxMount, hg, settlePlace] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · have hb' : i.branch = some b := by simpa using hb
        rw [hb0] at hb'
        have hbb : b0 = b := Option.some.inj hb'
        rw [← hbb, hbranches b0]
        exact hlive0
      · rw [hbranches b]
        exact h.1 op j (Store.intents_removeWhere_sub s.store _ op j hj0) hnd b hb
    · intro b hb
      rintro ⟨op, j, hj, hh, hnd⟩
      simp [doSandboxMount, hg, settlePlace] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · unfold HoldsBranch at hh
        rcases hh with hh | hh | hh
        · have : i.branch = some b := hh
          rw [hb0] at this
          have hbb : b0 = b := Option.some.inj this
          rw [← hbb] at hb
          rw [hbranches b0] at hb
          rw [hb] at hlive0
          simp at hlive0
        · have : i.branch = none := hh.1
          rw [hb0] at this
          simp at this
        · have : i.branch = none := hh.1
          rw [hb0] at this
          simp at this
      · refine h.2.1 b ?_ ⟨op, j, Store.intents_removeWhere_sub s.store _ op j hj0,
          HoldsBranch_of_branchProject (s := s) (s' := doSandboxMount s i) (by simp [doSandboxMount, hg, settlePlace]) hh, hnd⟩
        rw [← hbranches b]
        exact hb
    · intro hc
      rw [hcomp] at hc
      simp at hc
  · simp [doSandboxMount, hg]
    exact h

theorem Inv1_preview (s : State) (branch : BranchId) (row : OpId)
    (label : String) (h : Inv1 s) :
    Inv1 (doOperatorPreview s branch row label) := by
  by_cases hg : previewOk s row = true
  · apply Inv1_of_sub h
    · intro b; simp [doOperatorPreview, hg, Store.addIntent]
    · intro b; simp [doOperatorPreview, hg, Store.addIntent]
    · simp [doOperatorPreview, hg, Store.addIntent]
    · intro op j hj hnd
      simp [doOperatorPreview, hg] at hj
      rcases Store.intents_add_cases hj with ⟨rfl, rfl⟩ | ⟨hne, hj0⟩
      · simp [previewIntent] at hnd
      · exact hj0
  · simp [doOperatorPreview, hg]
    exact h

theorem Inv1_compact (s : State) (h : Inv1 s) : Inv1 (doStoreCompact s) := by
  by_cases hg : allDeadCertainB s = true
  · refine ⟨?_, ?_, ?_⟩
    · intro op i hi hnd b hb
      simp [doStoreCompact, hg] at hi
      change (s.store.removeWhere (fun _ i => i.state != .done)).intents op = some i at hi
      exact absurd (Store.intents_compact_done hi) hnd
    · intro b hb
      rintro ⟨op, i, hi, hh, hnd⟩
      simp [doStoreCompact, hg] at hi
      change (s.store.removeWhere (fun _ i => i.state != .done)).intents op = some i at hi
      exact absurd (Store.intents_compact_done hi) hnd
    · rintro _ ⟨op, i, hi, hnd⟩
      simp [doStoreCompact, hg] at hi
      change (s.store.removeWhere (fun _ i => i.state != .done)).intents op = some i at hi
      exact absurd (Store.intents_compact_done hi) hnd
  · simp [doStoreCompact, hg]
    exact h

theorem Inv1_branchRemove (s : State) (b : BranchId) (h : Inv1 s) :
    Inv1 (doBranchRemove s b) := by
  by_cases hg : branchRemoveOk s b = true
  · have hpostBranch : ∀ b', (doBranchRemove s b).store.branches b' =
        (if b' = b then some .removed else s.store.branches b') := by
      intro b'; simp [doBranchRemove, hg]
    refine ⟨?_, ?_, ?_⟩
    · intro op j hj hnd b' hb'
      simp [doBranchRemove, hg] at hj
      change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
      have hj0 := Store.intents_removeWhere_sub s.store (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj
      by_cases hbb : b' = b
      · have hb : j.branch = some b := by rw [← hbb]; exact hb'
        have hhold : holdsBranchB s b j = true := by
          rw [holdsBranchB_iff]; exact Or.inl hb
        have hrem := Store.intents_removeWhere_removed s.store
          (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj0
          (by simp [hhold, hnd])
        rw [hrem] at hj; simp at hj
      · rw [hpostBranch b', ite_eq_right hbb]
        exact h.1 op j hj0 hnd b' hb'
    · intro b' hb'
      by_cases hbb : b' = b
      · rw [hpostBranch b', ite_eq_left hbb] at hb'
        rintro ⟨op, j, hj, hh, hnd⟩
        simp [doBranchRemove, hg] at hj
        change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
        have hj0 := Store.intents_removeWhere_sub s.store
          (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj
        have hh' : HoldsBranch s b j := by
          rw [hbb] at hh
          exact HoldsBranch_of_branchProject (s := s) (s' := doBranchRemove s b) (by simp [doBranchRemove, hg]) hh
        have hhold : holdsBranchB s b j = true := by
          rw [holdsBranchB_iff]; exact hh'
        have hrem := Store.intents_removeWhere_removed s.store
          (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj0
          (by simp [hhold, hnd])
        rw [hrem] at hj; simp at hj
      · have hb0 : s.store.branches b' = some .removed := by
          rw [hpostBranch b', ite_eq_right hbb] at hb'
          exact hb'
        rintro ⟨op, j, hj, hh, hnd⟩
        simp [doBranchRemove, hg] at hj
        change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
        have hj0 := Store.intents_removeWhere_sub s.store
          (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj
        exact h.2.1 b' hb0 ⟨op, j, hj0,
          HoldsBranch_of_branchProject (s := s) (s' := doBranchRemove s b) (by simp [doBranchRemove, hg]) hh, hnd⟩
    · intro hc
      rintro ⟨op, j, hj, hnd⟩
      have hc' : s.store.compacted = true := by simpa [doBranchRemove, hg, Store.removeWhere] using hc
      simp [doBranchRemove, hg] at hj
      change (s.store.removeWhere (fun _ i => holdsBranchB s b i && (i.state != .done))).intents op = some j at hj
      have hj0 := Store.intents_removeWhere_sub s.store
        (fun _ i => holdsBranchB s b i && (i.state != .done)) op j hj
      exact h.2.2 hc' ⟨op, j, hj0, hnd⟩
  · simp [doBranchRemove, hg]
    exact h


theorem Inv1_recheck (s : State) (op0 : OpId) (h : Inv1 s) :
    Inv1 (doAdmissionRecheck s op0) := by
  apply Inv1_of_fields h
  · intro b
    unfold doAdmissionRecheck
    split <;> (try split) <;> rfl
  · intro b
    unfold doAdmissionRecheck
    split <;> (try split) <;> rfl
  · unfold doAdmissionRecheck
    split <;> (try split) <;> rfl
  · intro op j hj
    cases hrow : s.store.intents op0 with
    | none => simp [doAdmissionRecheck, hrow] at hj; exact ⟨j, hj, rfl, rfl, fun h => h⟩
    | some i0 =>
        by_cases hd : (i0.state != .done) = true
        · simp [doAdmissionRecheck, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · rw [heq] at hi1
            rw [heq]
            exact ⟨i1, hi1, by rw [hj], by rw [hj], by rw [hj]; exact fun h => h⟩
          · exact ⟨j, hi1, rfl, rfl, fun h => h⟩
        · simp [doAdmissionRecheck, hrow, hd] at hj
          exact ⟨j, hj, rfl, rfl, fun h => h⟩

theorem Inv1_settle (s : State) (op0 : OpId) (label : String) (h : Inv1 s) :
    Inv1 (doOperatorSettle s op0 label) := by
  apply Inv1_of_fields h
  · intro b; unfold doOperatorSettle; split <;> (try split) <;> rfl
  · intro b; unfold doOperatorSettle; split <;> (try split) <;> rfl
  · unfold doOperatorSettle; split <;> (try split) <;> rfl
  · intro op j hj
    cases hrow : s.store.intents op0 with
    | none => simp [doOperatorSettle, hrow] at hj; exact ⟨j, hj, rfl, rfl, fun h => h⟩
    | some i0 =>
        by_cases hd : (i0.state != .done) = true
        · simp [doOperatorSettle, hrow, hd] at hj
          rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
          · rw [heq] at hi1
            rw [heq]
            have hne : i1.state ≠ .done := by
              intro hc
              have heq' : i1 = i0 := by
                rw [hrow] at hi1
                exact (Option.some.inj hi1).symm
              subst heq'
              rw [hc] at hd
              simp at hd
            exact ⟨i1, hi1, by rw [hj], by rw [hj], fun _ => hne⟩
          · exact ⟨j, hi1, rfl, rfl, fun h => h⟩
        · simp [doOperatorSettle, hrow, hd] at hj
          exact ⟨j, hj, rfl, rfl, fun h => h⟩

theorem Inv1_release (s : State) (op0 : OpId) (label : String)
    (h : Inv1 s) :
    Inv1 (doOperatorRelease s op0 label) := by
  apply Inv1_of_fields h
  · intro b; unfold doOperatorRelease; split <;> rfl
  · intro b; unfold doOperatorRelease; split <;> rfl
  · unfold doOperatorRelease; split <;> rfl
  · intro op j hj
    by_cases hg : operatorReleaseOk s op0 = true
    · simp [doOperatorRelease, hg] at hj
      rcases Store.intents_update_cases hj with ⟨heq, i1, hi1, hj⟩ | ⟨hne, hi1⟩
      · rw [heq] at hi1
        rw [heq]
        exact ⟨i1, hi1, by rw [hj], by rw [hj], by
          intro hc; rw [hj] at hc; simp at hc⟩
      · exact ⟨j, hi1, rfl, rfl, fun h => h⟩
    · simp [doOperatorRelease, hg] at hj
      exact ⟨j, hj, rfl, rfl, fun h => h⟩


theorem Inv1_preserves (op : Op) (s : State) (h : Inv s) : Inv1 (step op s) := by
  cases op with
  | sandboxCreate m sb gen =>
      by_cases hg : sandboxCreateOk s m sb gen = true
      · apply Inv1_of_sub h.inv1
        · intro b; simp [step, doSandboxCreate, hg, State.mapHome]
        · intro b; simp [step, doSandboxCreate, hg, State.mapHome]
        · simp [step, doSandboxCreate, hg, State.mapHome]
        · intro op j hj hnd
          simpa [step, doSandboxCreate, hg, State.mapHome] using hj
      · simpa [step, doSandboxCreate, hg] using h.inv1
  | sandboxMount i =>
      simpa [step] using Inv1_mount s i h.inv1
  | sandboxUnmount op0 refused =>
      by_cases hg : sandboxUnmountOk s op0 = true
      · cases refused with
        | false =>
            apply Inv1_of_sub h.inv1
            · intro b; simp [step, doSandboxUnmount, hg, Store.removeIntent]
            · intro b; simp [step, doSandboxUnmount, hg, Store.removeIntent]
            · simp [step, doSandboxUnmount, hg, Store.removeIntent]
            · intro op j hj hnd
              simp [step, doSandboxUnmount, hg] at hj
              exact Store.intents_remove_sub hj
        | true =>
            apply Inv1_of_fields h.inv1
            · intro b; simp [step, doSandboxUnmount, hg, Store.updateIntent]
            · intro b; simp [step, doSandboxUnmount, hg, Store.updateIntent]
            · simp [step, doSandboxUnmount, hg, Store.updateIntent]
            · intro op j hj
              simp [step, doSandboxUnmount, hg] at hj
              rcases Store.intents_update_cases hj with ⟨rfl, i1, hi1, hj⟩ | ⟨hne, hi1⟩
              · have hne1 : i1.state ≠ .done := by
                  have h' := hg
                  simp [sandboxUnmountOk, hi1] at h'
                  intro hd; rw [hd] at h'; simp at h'
                refine ⟨i1, hi1, ?_, ?_, ?_⟩
                · rw [hj]
                · rw [hj]
                · intro _; exact hne1
              · exact ⟨j, hi1, rfl, rfl, fun h => h⟩
      · simpa [step, doSandboxUnmount, hg] using h.inv1
  | sandboxClose m sb mounted refused =>
      by_cases hg : sandboxCloseOk s m sb mounted = true
      · have h1 : Inv1 (closeHome s m sb) := by
          apply Inv1_of_sub h.inv1
          · intro b; simp [closeHome, State.mapHome]
          · intro b; simp [closeHome, State.mapHome]
          · simp [closeHome, State.mapHome]
          · intro op j hj hnd; simpa [closeHome, State.mapHome] using hj
        simp only [step, doSandboxClose, hg]
        exact Inv1_endSandboxMounts (closeHome s m sb) m sb refused h1
      · simpa [step, doSandboxClose, hg] using h.inv1
  | sandboxForceRemove m sb gen refused =>
      by_cases hg : sandboxForceRemoveOk s m sb gen = true
      · have h1 : Inv1 (forceRemoveHome s m sb gen) := by
          apply Inv1_of_sub h.inv1
          · intro b; simp [forceRemoveHome, State.mapHome]
          · intro b; simp [forceRemoveHome, State.mapHome]
          · simp [forceRemoveHome, State.mapHome]
          · intro op j hj hnd; simpa [forceRemoveHome, State.mapHome] using hj
        simp only [step, doSandboxForceRemove, hg]
        exact Inv1_endSandboxMounts (forceRemoveHome s m sb gen) m sb refused h1
      · simpa [step, doSandboxForceRemove, hg] using h.inv1
  | machineStart m inc =>
      by_cases hg : machineStartOk s m inc = true
      · apply Inv1_of_sub h.inv1
        · intro b; simp [step, doMachineStart, hg, State.mapHome, Store.removeWhere]
        · intro b; simp [step, doMachineStart, hg, State.mapHome, Store.removeWhere]
        · simp [step, doMachineStart, hg, State.mapHome, Store.removeWhere]
        · intro op j hj hnd
          simp [step, doMachineStart, hg, State.mapHome] at hj
          exact Store.intents_removeWhere_sub s.store _ op j hj
      · simpa [step, doMachineStart, hg] using h.inv1
  | machineRestore m inc =>
      by_cases hg : machineRestoreOk s m inc = true
      · apply Inv1_of_sub h.inv1
        · intro b; simp [step, doMachineRestore, hg, State.mapHome]
        · intro b; simp [step, doMachineRestore, hg, State.mapHome]
        · simp [step, doMachineRestore, hg, State.mapHome]
        · intro op j hj hnd
          simpa [step, doMachineRestore, hg, State.mapHome] using hj
      · simpa [step, doMachineRestore, hg] using h.inv1
  | admissionRecheck op0 =>
      simpa [step] using Inv1_recheck s op0 h.inv1
  | branchRemove b =>
      simpa [step] using Inv1_branchRemove s b h.inv1
  | storeCompact =>
      simpa [step] using Inv1_compact s h.inv1
  | operatorPreview branch row label =>
      simpa [step] using Inv1_preview s branch row label h.inv1
  | operatorSettle op0 label =>
      simpa [step] using Inv1_settle s op0 label h.inv1
  | operatorRelease op0 label =>
      simpa [step] using Inv1_release s op0 label h.inv1

theorem step_preserves (op : Op) (s : State) (h : Inv s) : Inv (step op s) :=
  { inv1 := Inv1_preserves op s h
    inv2 := Inv2_preserves op s h
    inv7 := Inv7_preserves op s
    inv8 := Inv8_preserves op s h
    entriesSound := EntriesSound_preserves op s h }


/-! ## Guard soundness (for the corollaries) -/

theorem allDeadCertainB_sound {s : State} (he : EntriesSound s)
    (h : allDeadCertainB s = true) :
    ∀ op i, s.store.intents op = some i → i.state ≠ .done →
      Dead s i ∧ i.uncertain = false := by
  intro op i hi hnd
  have hmem : op ∈ s.store.entries := Store.intents_mem he hi
  unfold allDeadCertainB at h
  have hp := (List.all_eq_true.mp h) op hmem
  simp [hi] at hp
  rcases hp with hdone | hdead
  · exact absurd hdone hnd
  · exact hdead

theorem allDeadCertainOnB_sound {s : State} {b : BranchId}
    (he : EntriesSound s) (h : allDeadCertainOnB s b = true) :
    ∀ op i, s.store.intents op = some i → HoldsBranch s b i → i.state ≠ .done →
      Dead s i ∧ i.uncertain = false := by
  intro op i hi hh hnd
  have hmem : op ∈ s.store.entries := Store.intents_mem he hi
  unfold allDeadCertainOnB at h
  have hp := (List.all_eq_true.mp h) op hmem
  have hhold : holdsBranchB s b i = true := holdsBranchB_iff.mpr hh
  simp [hi, hhold, hnd] at hp
  exact hp

theorem reconcileDeadB_iff {s : State} {m : MachineId} {inc : Ordered} {i : Intent} :
    reconcileDeadB s m inc i = true ↔ reconcileDead s m inc i := by
  simp [reconcileDeadB]

theorem Store.intents_removeWhere_removed_of_none {s : Store} (p : OpId → Intent → Bool)
    {op : OpId} {i : Intent} (hi : s.intents op = some i)
    (h : (s.removeWhere p).intents op = none) : p op i = true := by
  by_cases hp : p op i = true
  · exact hp
  · have hp' : p op i = false := by
      cases hpv : p op i with
      | false => rfl
      | true => exact absurd hpv hp
    have hkeep := Store.intents_removeWhere_kept s p op i hi hp'
    rw [hkeep, hi] at h
    simp at h

/-! ## The §9.4 corollaries -/

theorem done_is_operator (s : State) (h : Inv s) (op : OpId) (i : Intent)
    (hi : s.store.intents op = some i) (hdone : i.state = .done) :
    i.doneBy = some .operator :=
  h.inv8 op i hi hdone

theorem place_fallback_conservative (s : State) (i : Intent)
    (h : placeOf s i = none) :
    verdict s i = ownerVerdict s.sharedStore i.ownerEvidence := by
  unfold verdict
  rw [h]

theorem conservative_no_death (s : State) (i : Intent)
    (h : placeOf s i = none) : ¬ PlaceDead s i := by
  unfold PlaceDead
  rw [h]
  simp

theorem unordered_never_dead (r : RecordedPlace) (p : PlaceRecord)
    (h : compareMachine r.machine p.machine = some .uncertain) :
    placeVerdict r p ≠ .dead := by
  unfold placeVerdict
  rw [h]
  simp

/-- The plan's wording uses `comparePlace r.machine p.machine = recordedNewer`;
with a possibly-missing machine value that falls through to the Sandbox, so the
correct premise is the machine comparison itself.  Both forms are recorded in
the README. -/
theorem restore_is_uncertain (r : RecordedPlace) (p : PlaceRecord)
    (h : compareMachine r.machine p.machine = some .uncertain) :
    placeVerdict r p = .uncertain := by
  unfold placeVerdict
  rw [h]

/-- The type-corrected machine-level ordered rule: the plan's §9.4 statement
used `comparePlace` on `Option PlaceValue`s, which does not typecheck.  The
machine comparison is exactly `compareMachine`, and `placeVerdict` consults it
first; the Sandbox comparison is consulted only when the machine comparison has
no answer, and the final fallback is Live. -/
theorem ordered_rule (r : RecordedPlace) (p : PlaceRecord) :
    (compareMachine r.machine p.machine = some .dead → placeVerdict r p = .dead) ∧
    (compareMachine r.machine p.machine = some .uncertain → placeVerdict r p = .uncertain) ∧
    (compareMachine r.machine p.machine = none →
       (compareSandbox r.sandbox p.sandbox p.retired = some .dead →
         placeVerdict r p = .dead) ∧
       (compareSandbox r.sandbox p.sandbox p.retired = some .uncertain →
         placeVerdict r p = .uncertain) ∧
       (compareSandbox r.sandbox p.sandbox p.retired = none →
         placeVerdict r p = .live)) := by
  refine ⟨?_, ?_, ?_⟩
  · intro h; unfold placeVerdict; rw [h]
  · intro h; unfold placeVerdict; rw [h]
  · intro h
    refine ⟨?_, ?_, ?_⟩
    · intro hsb; unfold placeVerdict; rw [h, hsb]
    · intro hsb; unfold placeVerdict; rw [h, hsb]
    · intro hsb
      unfold placeVerdict
      rw [h, hsb]

theorem no_reclaim_live (s s' : State) (b : BranchId)
    (h : Inv s) (hstep : step (.branchRemove b) s = s')
    (hrem : s'.store.branches b = some .removed) : ¬ LiveHeldOn s b := by
  rintro ⟨op, i, hi, hhold, hnd, hlive⟩
  by_cases hg : branchRemoveOk s b = true
  · have hdc0 : allDeadCertainOnB s b = true := by
      have h' := hg
      simp [branchRemoveOk] at h'
      exact h'.2
    have hdc := allDeadCertainOnB_sound h.entriesSound hdc0 op i hi hhold hnd
    rcases hlive with hl | hu
    · exact hl hdc.1
    · rw [hdc.2] at hu; simp at hu
  · subst s'
    simp [step, doBranchRemove, hg] at hrem
    exact h.inv1.2.1 b hrem ⟨op, i, hi, hhold, hnd⟩

theorem compact_barrier (s s' : State)
    (h : Inv s) (hstep : step .storeCompact s = s')
    (hcompact : s'.store.compacted = true) : ¬ AnyLiveHeld s := by
  rintro ⟨op, i, hi, hnd, hlive⟩
  by_cases hg : allDeadCertainB s = true
  · have hdc := allDeadCertainB_sound h.entriesSound hg op i hi hnd
    rcases hlive with hl | hu
    · exact hl hdc.1
    · rw [hdc.2] at hu; simp at hu
  · subst s'
    simp [step, doStoreCompact, hg] at hcompact
    exact h.inv1.2.2 hcompact ⟨op, i, hi, hnd⟩

theorem reconcile_only_dead (s s' : State) (m : MachineId) (inc : Ordered)
    (hstep : step (.machineStart m inc) s = s') :
    ∀ op i, s.store.intents op = some i → s'.store.intents op = none →
      reconcileDead s m inc i := by
  intro op i hi hnone
  by_cases hg : machineStartOk s m inc = true
  · subst s'
    simp [step, doMachineStart, hg, State.mapHome] at hnone
    have hp := Store.intents_removeWhere_removed_of_none
      (fun _ i => reconcileDeadB s m inc i) hi hnone
    exact reconcileDeadB_iff.mp hp
  · subst s'
    simp [step, doMachineStart, hg] at hnone
    rw [hi] at hnone
    simp at hnone



theorem preview_is_done (s s' : State) (branch : BranchId)
    (row : OpId) (label : String)
    (hstep : step (.operatorPreview branch row label) s = s')
    (hfresh : s.store.intents row = none) :
    ∃ i, s'.store.intents row = some i ∧ i.state = .done ∧
         i.action = some .preview ∧ i.branch = some branch := by
  have hg : previewOk s row = true := by
    simp [previewOk, hfresh]
  subst s'
  refine ⟨previewIntent s branch row label, ?_, ?_, ?_, ?_⟩
  · simp [step, doOperatorPreview, hg]
    exact Store.intents_add_self s.store row (previewIntent s branch row label)
  · simp [previewIntent]
  · simp [previewIntent]
  · simp [previewIntent]



/-! ## Option-level forms of the ordered lemmas (§9.4) -/

/-- A machine-level uncertain answer is exactly a recordedNewer or an unordered
place comparison: it is never a proof of death. -/
theorem compareMachine_uncertain_cases {r c : Option PlaceValue}
    (h : compareMachine r c = some .uncertain) :
    ∃ rm cm, r = some rm ∧ c = some cm ∧
      (comparePlace rm cm = .recordedNewer ∨ comparePlace rm cm = .unordered) := by
  cases r with
  | none => simp [compareMachine] at h
  | some rm =>
      cases c with
      | none => simp [compareMachine] at h
      | some cm =>
          cases hcp : comparePlace rm cm with
          | recordedOlder => simp [compareMachine, hcp] at h
          | recordedNewer => exact ⟨rm, cm, rfl, rfl, Or.inl hcp⟩
          | unordered => exact ⟨rm, cm, rfl, rfl, Or.inr hcp⟩
          | equal => simp [compareMachine, hcp] at h

/-- Place-level raw form: an unordered comparison is never some dead. -/
theorem unordered_place_never_dead {r c : PlaceValue}
    (h : comparePlace r c = .unordered) :
    compareMachine (some r) (some c) ≠ some .dead := by
  simp [compareMachine, h]

/-- Place-level raw form: recordedNewer is uncertain, never dead. -/
theorem recorded_newer_is_uncertain {r c : PlaceValue}
    (h : comparePlace r c = .recordedNewer) :
    compareMachine (some r) (some c) = some .uncertain := by
  simp [compareMachine, h]

/-- The full sequential rule, including both Sandbox cases: a machine answer
wins; otherwise a Sandbox answer (current generation or tombstone) wins;
otherwise the verdict is Live. -/
theorem ordered_rule_sequential (r : RecordedPlace) (p : PlaceRecord) :
    (compareMachine r.machine p.machine = some .dead → placeVerdict r p = .dead) ∧
    (compareMachine r.machine p.machine = some .uncertain → placeVerdict r p = .uncertain) ∧
    (compareMachine r.machine p.machine = none →
       compareSandbox r.sandbox p.sandbox p.retired = some .dead →
         placeVerdict r p = .dead) ∧
    (compareMachine r.machine p.machine = none →
       compareSandbox r.sandbox p.sandbox p.retired = some .uncertain →
         placeVerdict r p = .uncertain) ∧
    (compareMachine r.machine p.machine = none →
       compareSandbox r.sandbox p.sandbox p.retired = none →
         placeVerdict r p = .live) := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩
  · intro h; unfold placeVerdict; rw [h]
  · intro h; unfold placeVerdict; rw [h]
  · intro h hsb; unfold placeVerdict; rw [h, hsb]
  · intro h hsb; unfold placeVerdict; rw [h, hsb]
  · intro h hsb; unfold placeVerdict; rw [h, hsb]

end GcAdmission
