import LayerfsGc.History

/-!
# The object closure (GC spec §6.2) and the whole mark

An object has the logical references `FinalizedObject::admit(..).references()`
returns and at most one delta base, `ChainBases::base_of`. Under spec #42
item 27 there is one visit level: an object reached by either kind of edge
is kept with all of its references. So the object mark is a closure over
`refs ++ base`, rooted at the `root_id` of every kept Commit and Layer.

`retain` is the rebuild's copy of the kept objects. The theorems state the
three object guarantees the spec asks for:

* `retain_closed`: every reference and every delta base of a retained
  object is retained. The full doctor's object scan
  (`diagnostic/scan.rs`) and delta decoding both rely on this.
* `lookup_retain`: an identity never resolves to different bytes; it
  resolves to the same object, or to nothing.
* `reachable_readable`: an object reachable from a kept root reads back
  exactly as before.

`remove_file` (a path-aware walk that skips a removed path below its
frontier) is not modelled here.
-/

namespace LayerfsGc.Objects

open LayerfsGc

structure Obj where
  id : Nat
  /-- Stands in for the canonical bytes. -/
  bytes : Nat
  refs : List Nat
  base : Option Nat := none
  deriving DecidableEq, Repr

/-- The edges of an object: references and delta base, of every stored
object with that identity. -/
def objEdges (objs : List Obj) (i : Nat) : List Nat :=
  (objs.filter (fun o => o.id == i)).flatMap (fun o => o.refs ++ o.base.toList)

def retain (objs : List Obj) (K : List Nat) : List Obj := objs.filter (fun o => o.id ∈ K)

def lookup (objs : List Obj) (i : Nat) : Option Obj := objs.find? (fun o => o.id == i)

theorem mem_objEdges {objs : List Obj} {i j : Nat} :
    j ∈ objEdges objs i ↔ ∃ o ∈ objs, o.id = i ∧ (j ∈ o.refs ∨ o.base = some j) := by
  simp [objEdges, List.mem_flatMap, List.mem_filter, and_assoc]

/-- **No dangling reference or delta base after the rebuild.** -/
theorem retain_closed {objs : List Obj} {K : List Nat} (hc : Closed (objEdges objs) K)
    {o : Obj} (ho : o ∈ retain objs K) {j : Nat} (hj : j ∈ o.refs ∨ o.base = some j)
    {o' : Obj} (ho' : o' ∈ objs) (hid : o'.id = j) : o' ∈ retain objs K := by
  simp only [retain, List.mem_filter, decide_eq_true_eq] at ho ⊢
  refine ⟨ho', ?_⟩
  rw [hid]
  exact hc _ ho.2 _ (mem_objEdges.mpr ⟨o, ho.1, rfl, hj⟩)

/-- What the rebuilt Store returns for an identity: the old answer when the
identity was kept, nothing otherwise. -/
theorem lookup_retain (objs : List Obj) (K : List Nat) (i : Nat) :
    lookup (retain objs K) i = if i ∈ K then lookup objs i else none := by
  induction objs with
  | nil => simp [lookup, retain]
  | cons o os ih =>
    simp only [lookup, retain] at ih ⊢
    by_cases hoK : o.id ∈ K
    · rw [List.filter_cons_of_pos (by simpa using hoK)]
      by_cases hoi : o.id = i
      · subst hoi; simp [hoK]
      · simp [hoi, ih]
    · rw [List.filter_cons_of_neg (by simpa using hoK)]
      by_cases hoi : o.id = i
      · subst hoi
        simp only [hoK, ite_false] at ih ⊢
        exact ih
      · simp [hoi, ih]

/-- **An old identity never resolves to changed contents.** -/
theorem never_different_bytes {objs : List Obj} {K : List Nat} {i : Nat} {o : Obj}
    (h : lookup (retain objs K) i = some o) : lookup objs i = some o := by
  rw [lookup_retain] at h
  split at h
  · exact h
  · simp at h

/-- **Live data stays readable, byte for byte.** -/
theorem reachable_readable {roots : List Nat} {objs : List Obj} {K : List Nat}
    (hm : MarkOk roots (objEdges objs) K) {i : Nat}
    (hi : Reach roots (objEdges objs) i) : lookup (retain objs K) i = lookup objs i := by
  rw [lookup_retain]
  simp [hm.complete hi]

/-! ## The whole mark: history, then objects -/

open LayerfsGc.History

/-- The object roots: `root_id` of every kept Commit and Layer row. -/
def objectRoots (s : Store) (K : List Node) : List Nat :=
  ((prune s K).rows.filter (fun r => r.kind == .commit || r.kind == .layer)).filterMap Row.root

/-- **The mark as a whole.** Run the history driver, then any correct object
mark from the kept roots, and rebuild. The history references pass the
doctor, every kept root's object tree reads back unchanged, and no retained
object names a collected one. -/
theorem compaction_safe {s : Store} {objs : List Obj}
    {expandH : List Node → List Node → List Node} {bn br fuel : Nat} {KH : List Node}
    {KO : List Nat}
    (hkeys : Keys s) (hloc : StackLocal s) (hdoc : Doctor s)
    (hexp : ∀ K B, ExpandOk (sqlEdges s) K B br (expandH K B))
    (hrun : run expandH bn br fuel (State.start (seed s)) = some KH)
    (hobj : MarkOk (objectRoots s KH) (objEdges objs) KO) :
    Doctor (prune s KH) ∧
    (∀ i, Reach (objectRoots s KH) (objEdges objs) i →
      lookup (retain objs KO) i = lookup objs i) ∧
    (∀ o ∈ retain objs KO, ∀ j, (j ∈ o.refs ∨ o.base = some j) →
      ∀ o' ∈ objs, o'.id = j → o' ∈ retain objs KO) := by
  obtain ⟨hmh, _⟩ := run_correct hexp hrun
  exact ⟨prune_doctor hkeys hloc hdoc hmh,
    fun _ hi => reachable_readable hobj hi,
    fun _ ho _ hj _ ho' hid => retain_closed hobj.closed ho hj ho' hid⟩

end LayerfsGc.Objects
