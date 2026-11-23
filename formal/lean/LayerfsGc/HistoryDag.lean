import LayerfsGc.History

/-!
# #1: History stays a DAG at any length

The doctor (`diagnostic/history.rs`) peels history nodes parents-first and
fails `store.history.cycles` if any node is left. This file proves that no
sequence of history operations can leave one, however long the history
grows.

**What the order is.** `history_serial` cannot serve as the topological
order. A Layer that republishes a Commit carries that Commit's serial
(`history/layerstack.rs` at `04e55a7f4c`, "A Layer that republishes a Commit
carries that Commit's serial"). A forced publish (#691) puts the new Layer
on the current stack head, whose source Commit can be newer. So along
`layer.parent_layer_id` the serial can go *up*. The invariant is insertion
order instead, which the model carries as a ghost `stamp`: every insert
names only rows that already exist (the foreign keys and `verify_layer`),
so every edge points to an older row.

The operations:

* `insert r`: a Commit (`commit_insert_commit.sql`, `ON CONFLICT DO
  NOTHING`) or a Layer (`layerstack_insert_layer.sql` after
  `verify_layer`). An identity already present changes nothing.
* `prune K`: compaction keeps the rows whose node a closed mark kept.
* `other`: Branch head moves, fork, `branch.remove`, `project.delete`,
  `remove_file`. They write Branch, LayerStack or marker rows and change no
  Commit or Layer row, so the doctor's peel graph is unchanged.
-/

namespace LayerfsGc.HistoryDag

open LayerfsGc LayerfsGc.History

/-- The doctor's peel edges: the `edges` table of `inventory` (Commit
`parent`, `base`, `second`, `merged_from`, `displaced`; Layer `parent`,
`source`). Branch references are not peel edges. -/
def dagRefs (r : Row) : List Node :=
  match r.kind with
  | .commit => col .commit r.parent ++ col .layer r.base ++ ref r.second ++
      ref r.merged ++ col .commit r.displaced
  | .layer => col .layer r.parent ++ col .commit r.srcCommit
  | _ => []

/-- Every peel edge is an edge the GC mark follows, so a mark that is
`MarkOk` over `sqlEdges` meets `Allowed (.prune K)`. -/
theorem dagRefs_followed {r : Row} {t : Node} (ht : t ∈ dagRefs r) : t ∈ sqlTargets r := by
  have hk : r.kind ≠ .stack := by
    intro h; simp [dagRefs, h] at ht
  apply required_followed hk
  cases hr : r.kind with
  | stack => exact absurd hr hk
  | branch => simp [dagRefs, hr] at ht
  | commit =>
    simp only [dagRefs, hr] at ht
    simp only [doctorRequired, hr, List.mem_append] at ht ⊢
    exact Or.inl ht
  | layer =>
    simp only [dagRefs, hr] at ht
    simp only [doctorRequired, hr, List.mem_append] at ht ⊢
    exact Or.inl ht

/-- A Commit or Layer row with its ghost insertion stamp. -/
structure DagRow where
  row : Row
  stamp : Nat

structure Dag where
  rows : List DagRow
  /-- The next insertion stamp. -/
  next : Nat

def Dag.present (d : Dag) (t : Node) : Prop := ∃ x ∈ d.rows, x.row.node = t

instance (d : Dag) (t : Node) : Decidable (d.present t) :=
  inferInstanceAs (Decidable (∃ x ∈ d.rows, x.row.node = t))

def Dag.empty : Dag := ⟨[], 0⟩

inductive Op where
  | insert (r : Row)
  | prune (K : List Node)
  | other

def apply (d : Dag) : Op → Dag
  | .insert r => if d.present r.node then d else ⟨d.rows ++ [⟨r, d.next⟩], d.next + 1⟩
  | .prune K => ⟨d.rows.filter (fun x => x.row.node ∈ K), d.next⟩
  | .other => d

/-- What the code guarantees before each operation. -/
def Allowed (d : Dag) : Op → Prop
  /- The foreign keys: every peel edge of a new row names an existing row. -/
  | .insert r => ∀ t ∈ dagRefs r, d.present t
  /- The mark is closed under the peel edges (true of `MarkOk` over
     `sqlEdges`, which contains them: `required_followed`). -/
  | .prune K => ∀ x ∈ d.rows, x.row.node ∈ K → ∀ t ∈ dagRefs x.row, d.present t → t ∈ K
  | .other => True

/-- The invariant. -/
structure Ranked (d : Dag) : Prop where
  below : ∀ x ∈ d.rows, x.stamp < d.next
  keys : ∀ x ∈ d.rows, ∀ y ∈ d.rows, x.row.node = y.row.node → x = y
  refs : ∀ x ∈ d.rows, ∀ t ∈ dagRefs x.row, d.present t
  order : ∀ x ∈ d.rows, ∀ y ∈ d.rows, y.row.node ∈ dagRefs x.row → y.stamp < x.stamp

theorem ranked_empty : Ranked Dag.empty := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> simp [Dag.empty]

theorem ranked_apply {d : Dag} {op : Op} (hd : Ranked d) (ha : Allowed d op) :
    Ranked (apply d op) := by
  cases op with
  | other => exact hd
  | insert r =>
    simp only [apply]
    split
    · exact hd
    · rename_i hfresh
      simp only [Allowed] at ha
      have hmem : ∀ x, x ∈ d.rows ++ [⟨r, d.next⟩] ↔ x ∈ d.rows ∨ x = ⟨r, d.next⟩ := by
        intro x; simp
      have hpres : ∀ t, d.present t → Dag.present ⟨d.rows ++ [⟨r, d.next⟩], d.next + 1⟩ t := by
        rintro t ⟨x, hx, hxt⟩
        exact ⟨x, (hmem x).mpr (Or.inl hx), hxt⟩
      refine ⟨?_, ?_, ?_, ?_⟩
      · intro x hx
        rcases (hmem x).mp hx with hx | rfl
        · exact Nat.lt_succ_of_lt (hd.below x hx)
        · exact Nat.lt_succ_self _
      · intro x hx y hy hxy
        rcases (hmem x).mp hx with hx | rfl <;> rcases (hmem y).mp hy with hy | rfl
        · exact hd.keys x hx y hy hxy
        · exact absurd ⟨x, hx, hxy⟩ hfresh
        · exact absurd ⟨y, hy, hxy.symm⟩ hfresh
        · rfl
      · intro x hx t ht
        rcases (hmem x).mp hx with hx | rfl
        · exact hpres t (hd.refs x hx t ht)
        · exact hpres t (ha t ht)
      · intro x hx y hy hyx
        rcases (hmem x).mp hx with hx | rfl <;> rcases (hmem y).mp hy with hy | rfl
        · exact hd.order x hx y hy hyx
        · -- An old row naming the new one: it would name a missing row.
          obtain ⟨z, hz, hzt⟩ := hd.refs x hx _ hyx
          exact absurd ⟨z, hz, hzt⟩ hfresh
        · exact hd.below y hy
        · obtain ⟨z, hz, hzt⟩ := ha _ hyx
          exact absurd ⟨z, hz, hzt⟩ hfresh
  | prune K =>
    simp only [apply]
    simp only [Allowed] at ha
    have hmem : ∀ x, x ∈ d.rows.filter (fun x => x.row.node ∈ K) ↔ x ∈ d.rows ∧ x.row.node ∈ K := by
      intro x; simp [List.mem_filter]
    refine ⟨?_, ?_, ?_, ?_⟩
    · intro x hx; exact hd.below x ((hmem x).mp hx).1
    · intro x hx y hy; exact hd.keys x ((hmem x).mp hx).1 y ((hmem y).mp hy).1
    · intro x hx t ht
      obtain ⟨hx, hxK⟩ := (hmem x).mp hx
      have hp := hd.refs x hx t ht
      obtain ⟨z, hz, hzt⟩ := hp
      have htK := ha x hx hxK t ht ⟨z, hz, hzt⟩
      exact ⟨z, (hmem z).mpr ⟨hz, hzt ▸ htK⟩, hzt⟩
    · intro x hx y hy; exact hd.order x ((hmem x).mp hx).1 y ((hmem y).mp hy).1

/-- A run of operations, each allowed when it happens. -/
def Valid : Dag → List Op → Prop
  | _, [] => True
  | d, op :: ops => Allowed d op ∧ Valid (apply d op) ops

def applyAll : Dag → List Op → Dag
  | d, [] => d
  | d, op :: ops => applyAll (apply d op) ops

theorem ranked_applyAll : ∀ (ops : List Op) (d : Dag), Ranked d → Valid d ops →
    Ranked (applyAll d ops)
  | [], _, hd, _ => hd
  | op :: ops, d, hd, ⟨ha, hv⟩ => ranked_applyAll ops (apply d op) (ranked_apply hd ha) hv

/-! ## No cycle -/

/-- A path along peel edges between two rows. -/
inductive Path (d : Dag) : DagRow → DagRow → Prop where
  | edge {x y} : x ∈ d.rows → y ∈ d.rows → y.row.node ∈ dagRefs x.row → Path d x y
  | trans {x y z} : Path d x y → Path d y z → Path d x z

theorem path_stamp {d : Dag} (hd : Ranked d) {x y : DagRow} (h : Path d x y) :
    y.stamp < x.stamp := by
  induction h with
  | edge hx hy he => exact hd.order _ hx _ hy he
  | trans _ _ ih1 ih2 => exact Nat.lt_trans ih2 ih1

/-- **No cycle, at any history length.** -/
theorem acyclic {d : Dag} (hd : Ranked d) (x : DagRow) : ¬ Path d x x :=
  fun h => Nat.lt_irrefl _ (path_stamp hd h)

/-! ## The doctor's peel finishes -/

/-- A node the doctor's loop can peel: every existing parent is peelable
first ("NOT EXISTS (SELECT 1 FROM edges e JOIN nodes p ON p.id=e.parent
WHERE e.child=n.id AND p.done=0)"). -/
inductive Peelable (d : Dag) : DagRow → Prop where
  | mk {x} : (∀ y ∈ d.rows, y.row.node ∈ dagRefs x.row → Peelable d y) → Peelable d x

theorem all_peelable {d : Dag} (hd : Ranked d) : ∀ x ∈ d.rows, Peelable d x := by
  have key : ∀ n, ∀ x ∈ d.rows, x.stamp < n → Peelable d x := by
    intro n
    induction n with
    | zero => intro x _ h; exact absurd h (Nat.not_lt_zero _)
    | succ n ih =>
      intro x hx hlt
      refine Peelable.mk ?_
      intro y hy hyx
      exact ih y hy (Nat.lt_of_lt_of_le (hd.order x hx y hy hyx) (Nat.le_of_lt_succ hlt))
  exact fun x hx => key _ x hx (Nat.lt_succ_self _)

/-- The loop stops when no undone node has all its parents done. At that
point every peelable node is done. -/
theorem peel_done {d : Dag} {done : DagRow → Prop}
    (hstop : ∀ x ∈ d.rows, ¬ done x →
      ∃ y ∈ d.rows, y.row.node ∈ dagRefs x.row ∧ ¬ done y)
    {x : DagRow} (hx : x ∈ d.rows) (hp : Peelable d x) : done x := by
  induction hp with
  | mk _ ih =>
    rename_i x' _
    apply Classical.byContradiction
    intro hnd
    obtain ⟨y, hy, hyx, hyd⟩ := hstop x' hx hnd
    exact hyd (ih y hy hyx hy)

/-- **The doctor's `store.history.cycles` check passes after any valid run of
history operations from an empty Store.** When the peel loop stops, no node
is left undone. -/
theorem doctor_cycles_pass {ops : List Op} (hv : Valid Dag.empty ops)
    {done : DagRow → Prop}
    (hstop : ∀ x ∈ (applyAll Dag.empty ops).rows, ¬ done x →
      ∃ y ∈ (applyAll Dag.empty ops).rows, y.row.node ∈ dagRefs x.row ∧ ¬ done y) :
    ∀ x ∈ (applyAll Dag.empty ops).rows, done x := by
  have hd := ranked_applyAll ops _ ranked_empty hv
  exact fun x hx => peel_done hstop hx (all_peelable hd x hx)

end LayerfsGc.HistoryDag
