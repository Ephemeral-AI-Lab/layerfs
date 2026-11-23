import LayerfsGc.History

/-!
# #2: A Commit that got a success reply is never lost

On `layerfs_v2_0_0` at `04e55a7f4c`, a Branch head moves inside one exclusive
write transaction (SQLite `BEGIN IMMEDIATE`; PostgreSQL `READ COMMITTED`
plus the lane row `FOR UPDATE`). It moves in two places:

* `commit_branch` (`history/commit.rs:87-181`). A new Commit is created and
  the head set to it. Without `force`, the head is re-read in the lane and
  the new Commit's parent is that head. With `force` and a moved head, the
  parent is the bound head and `displaced_id` is set to the head it
  overwrote.
* `merge_into_branch` (`history/merge.rs:57-195`). Either a fast-forward
  gated by `is_ancestor(old, new)`, or a merge Commit whose parent is the
  old head.

A forced Commit moves the head to a Commit that does **not** descend from
the old head. So "never lost" cannot mean "stays an ancestor of the head".
What holds, and what this file proves, is the GC's notion: the Commit stays
**reachable from its Branch along the GC keep edges** (`sqlEdges`, which
include `displaced_id` and `merged_from_id`). That is exactly what keeps it
through compaction.

The operations:

* `insert r`: a new Commit, Layer or Branch row (commit, publish, fork).
  An identity already present changes nothing (`ON CONFLICT DO NOTHING`).
* `move r`: one head/base move of an existing Branch row, in one lane step.
  Its guard (`Allowed`): everything the old row reached is still reachable
  from the new row. `commit_guard` proves that guard for `commit_branch`,
  forced or not.
* `retire`: `branch.remove`, `project.delete`, `remove_file` markers. They
  change no history row.
* `prune K`: compaction to a correct mark, run while the Branch is live.

A lost reply changes nothing here: the transaction either committed (and
the state moved) or did not. Nothing is retried, so no step is applied
twice.
-/

namespace LayerfsGc.Lane

open LayerfsGc LayerfsGc.History

def bnode (b : Nat) : Node := (.branch, b)

/-- `x` is held by Branch `b`: reachable from it along the GC keep edges. -/
def Held (s : Store) (b : Nat) (x : Node) : Prop := Reach [bnode b] (sqlEdges s) x

def insertRow (s : Store) (r : Row) : Store :=
  if present s r.node then s else { s with rows := s.rows ++ [r] }

def moveBranch (s : Store) (r : Row) : Store :=
  { s with rows := s.rows.map (fun x => if x.node = r.node then r else x) }

def retire (s : Store) (bs ks : List Nat) : Store :=
  { s with retiredBranches := s.retiredBranches ++ bs,
           retiredStacks := s.retiredStacks ++ ks }

inductive Op where
  | insert (r : Row)
  | move (r : Row)
  | retire (bs ks : List Nat)
  | prune (K : List Node)

def apply (s : Store) : Op → Store
  | .insert r => insertRow s r
  | .move r => moveBranch s r
  | .retire bs ks => retire s bs ks
  | .prune K => prune s K

def Allowed (s : Store) : Op → Prop
  | .insert _ => True
  | .move r => r.kind = .branch ∧ present s r.node ∧
      ∀ y ∈ sqlEdges s r.node, Reach [r.node] (sqlEdges (moveBranch s r)) y
  | .retire _ _ => True
  | .prune K => MarkOk (seed s) (sqlEdges s) K

/-! ## Each operation keeps what a Branch holds -/

theorem insert_edges {s : Store} {r : Row} {x y : Node} (h : y ∈ sqlEdges s x) :
    y ∈ sqlEdges (insertRow s r) x := by
  unfold insertRow
  split
  · exact h
  · obtain ⟨rx, hrx, hnx, hty, hpy⟩ := mem_sqlEdges.mp h
    obtain ⟨ry, hry, hny⟩ := present_iff.mp hpy
    exact mem_sqlEdges.mpr ⟨rx, List.mem_append_left _ hrx, hnx, hty,
      present_iff.mpr ⟨ry, List.mem_append_left _ hry, hny⟩⟩

theorem held_insert {s : Store} {r : Row} {b : Nat} {x : Node} (h : Held s b x) :
    Held (insertRow s r) b x :=
  reach_mono (fun _ _ hy => insert_edges hy) h

theorem move_present {s : Store} {r : Row} {y : Node} (h : present s y = true) :
    present (moveBranch s r) y = true := by
  obtain ⟨ry, hry, hny⟩ := present_iff.mp h
  apply present_iff.mpr
  by_cases hq : ry.node = r.node
  · refine ⟨r, ?_, hq ▸ hny⟩
    exact List.mem_map.mpr ⟨ry, hry, by simp [hq]⟩
  · exact ⟨ry, List.mem_map.mpr ⟨ry, hry, by simp [hq]⟩, hny⟩

theorem move_edges {s : Store} {r : Row} {x y : Node} (hx : x ≠ r.node)
    (h : y ∈ sqlEdges s x) : y ∈ sqlEdges (moveBranch s r) x := by
  obtain ⟨rx, hrx, hnx, hty, hpy⟩ := mem_sqlEdges.mp h
  have hq : ¬ rx.node = r.node := fun hq => hx (hnx ▸ hq)
  exact mem_sqlEdges.mpr ⟨rx, List.mem_map.mpr ⟨rx, hrx, by simp [hq]⟩, hnx, hty,
    move_present hpy⟩

theorem held_move {s : Store} {r : Row} {b : Nat} {x : Node}
    (ha : Allowed s (.move r)) (h : Held s b x) : Held (moveBranch s r) b x := by
  obtain ⟨_, _, hguard⟩ := ha
  unfold Held at *
  induction h with
  | root hx => exact Reach.root hx
  | @step x' y' _ hy ih =>
    by_cases hx : x' = r.node
    · subst hx
      exact reach_trans (fun m hm => by simp at hm; exact hm ▸ ih) (hguard y' hy)
    · exact Reach.step ih (move_edges hx hy)

theorem held_retire {s : Store} {bs ks : List Nat} {b : Nat} {x : Node} (h : Held s b x) :
    Held (retire s bs ks) b x := h

/-- Compaction keeps what a live Branch holds, and keeps it reachable. -/
theorem held_prune {s : Store} {K : List Node} {b : Nat} {x : Node}
    (hm : MarkOk (seed s) (sqlEdges s) K) (hlive : bnode b ∈ seed s)
    (h : Held s b x) : Held (prune s K) b x := by
  have hK : ∀ z, Held s b z → z ∈ K := fun z hz =>
    hm.complete (reach_trans (fun m hm' => by simp at hm'; exact hm' ▸ Reach.root hlive) hz)
  unfold Held at *
  induction h with
  | root hx => exact Reach.root hx
  | @step x' y' hxr hy ih =>
    obtain ⟨rx, hrx, hnx, hty, hpy⟩ := mem_sqlEdges.mp hy
    obtain ⟨ry, hry, hny⟩ := present_iff.mp hpy
    have hxK := hK x' hxr
    have hyK := hK y' (Reach.step hxr hy)
    refine Reach.step ih (mem_sqlEdges.mpr ⟨rx, mem_prune.mpr ⟨hrx, hnx ▸ hxK⟩, hnx, hty,
      present_iff.mpr ⟨ry, mem_prune.mpr ⟨hry, hny ▸ hyK⟩, hny⟩⟩)

/-! ## Any run of operations -/

/-- Every operation is allowed when it happens, and every compaction runs
while Branch `b` is live. -/
def Valid (b : Nat) : Store → List Op → Prop
  | _, [] => True
  | s, op :: ops => Allowed s op ∧ (∀ K, op = .prune K → bnode b ∈ seed s) ∧
      Valid b (apply s op) ops

def applyAll : Store → List Op → Store
  | s, [] => s
  | s, op :: ops => applyAll (apply s op) ops

theorem held_apply {s : Store} {op : Op} {b : Nat} {x : Node} (ha : Allowed s op)
    (hlive : ∀ K, op = .prune K → bnode b ∈ seed s) (h : Held s b x) :
    Held (apply s op) b x := by
  cases op with
  | insert r => exact held_insert h
  | move r => exact held_move ha h
  | retire bs ks => exact held_retire h
  | prune K => exact held_prune ha (hlive K rfl) h

/-- **Whatever a Branch holds, it holds after any valid run of later
Commits, merges, forks, publishes, removals and compactions.** -/
theorem held_forever {b : Nat} :
    ∀ (ops : List Op) (s : Store) (x : Node), Valid b s ops → Held s b x →
      Held (applyAll s ops) b x
  | [], _, _, _, h => h
  | _ :: ops, _, x, ⟨ha, hl, hv⟩, h => held_forever ops _ x hv (held_apply ha hl h)

/-! ## `commit_branch` meets the guard, forced or not -/

theorem mem_branch_targets {r : Row} {y : Node} (hk : r.kind = .branch) :
    y ∈ sqlTargets r ↔
      (y ∈ col .commit r.head ∨ (y ∈ ref r.fork ∧ y.1 = .commit)) ∨
      ((y ∈ col .layer r.base ∨ y ∈ ref r.fork) ∧ y.1 = .layer) := by
  simp only [sqlTargets, hk, List.mem_append, mem_inTable]
  constructor
  · rintro (⟨h1 | h1, h2⟩ | h)
    · exact Or.inl (Or.inl h1)
    · exact Or.inl (Or.inr ⟨h1, h2⟩)
    · exact Or.inr h
  · rintro ((h1 | ⟨h1, h2⟩) | h)
    · exact Or.inl ⟨Or.inl h1, col_kind h1⟩
    · exact Or.inl ⟨Or.inr h1, h2⟩
    · exact Or.inr h

/-- A forced or unforced `commit_branch`: insert the new Commit `c`, then set
the head of Branch row `ob` to it. The code's rule: if the head was `h`,
then `c.parent = h` (no `force`, or the head did not move) or
`c.displaced = h` (forced over a moved head). The new row keeps the base
(`WHERE base_layer_id = $3`) and the fork. -/
theorem commit_guard {s : Store} {ob c r : Row} (hkeys : Keys s)
    (hob : ob ∈ s.rows) (hobk : ob.kind = .branch) (hck : c.kind = .commit)
    (hfresh : present s c.node = false)
    (hhead : ∀ h, ob.head = some h → c.parent = some h ∨ c.displaced = some h)
    (hr : r = { ob with head := some c.id }) :
    Allowed (insertRow s c) (.move r) := by
  have hs1rows : (insertRow s c).rows = s.rows ++ [c] := by simp [insertRow, hfresh]
  have hrnode : r.node = ob.node := by subst hr; rfl
  have hrk : r.kind = .branch := by subst hr; exact hobk
  have hrbase : r.base = ob.base := by subst hr; rfl
  have hrfork : r.fork = ob.fork := by subst hr; rfl
  have hrhead : r.head = some c.id := by subst hr; rfl
  refine ⟨hrk, ?_, ?_⟩
  · exact present_iff.mpr ⟨ob, by rw [hs1rows]; exact List.mem_append_left _ hob, hrnode.symm⟩
  · intro y hy
    obtain ⟨rx, hrx, hnx, hty, hpy⟩ := mem_sqlEdges.mp hy
    -- `rx` is `ob`: `c` is a Commit, and the Branch identity is unique.
    have hrx_ob : rx = ob := by
      rw [hs1rows] at hrx
      rcases List.mem_append.mp hrx with hrx | hrx
      · exact hkeys rx hrx ob hob (hnx.trans hrnode)
      · simp at hrx; subst hrx
        have := congrArg Prod.fst hnx
        simp [Row.node, hck, hrk] at this
    rw [hrx_ob] at hty
    -- In the new Store the Branch row is `r`, and `c` and old rows remain.
    have hr_in : r ∈ (moveBranch (insertRow s c) r).rows := by
      apply List.mem_map.mpr
      exact ⟨ob, by rw [hs1rows]; exact List.mem_append_left _ hob, by simp [hrnode]⟩
    have hc_in : c ∈ (moveBranch (insertRow s c) r).rows := by
      apply List.mem_map.mpr
      refine ⟨c, by rw [hs1rows]; simp, ?_⟩
      have : ¬ c.node = r.node := by
        intro h; have := congrArg Prod.fst h; simp [Row.node, hck, hrk] at this
      simp [this]
    have hpy2 := move_present (r := r) hpy
    have hedge : ∀ z, z ∈ sqlTargets r → present (moveBranch (insertRow s c) r) z = true →
        z ∈ sqlEdges (moveBranch (insertRow s c) r) r.node :=
      fun z hz hp => mem_sqlEdges.mpr ⟨r, hr_in, rfl, hz, hp⟩
    rcases (mem_branch_targets hobk).mp hty with (hy1 | ⟨hy1, hy2⟩) | ⟨hy1, hy2⟩
    · -- The old head `h`: reached through `c`, by `parent` or `displaced_id`.
      simp only [col, List.mem_map, Option.mem_toList] at hy1
      obtain ⟨h, hh, rfl⟩ := hy1
      have hcy : (Kind.commit, c.id) ∈ sqlEdges (moveBranch (insertRow s c) r) r.node := by
        apply hedge
        · exact (mem_branch_targets hrk).mpr (Or.inl (Or.inl (by simp [col, hrhead])))
        · exact present_iff.mpr ⟨c, hc_in, by simp [Row.node, hck]⟩
      have hhy : (Kind.commit, h) ∈ sqlEdges (moveBranch (insertRow s c) r) (Kind.commit, c.id) := by
        refine mem_sqlEdges.mpr ⟨c, hc_in, by simp [Row.node, hck], ?_, hpy2⟩
        simp only [sqlTargets, hck, List.mem_append, mem_inTable]
        rcases hhead h hh with hp | hp
        · exact Or.inl (Or.inl ⟨Or.inl (Or.inl (Or.inl (by simp [col, hp]))), trivial⟩)
        · exact Or.inl (Or.inl ⟨Or.inr (by simp [col, hp]), trivial⟩)
      exact Reach.step (Reach.step (Reach.root (by simp)) hcy) hhy
    · -- The fork, a Commit: still an edge of the new row.
      exact Reach.step (Reach.root (by simp))
        (hedge _ ((mem_branch_targets hrk).mpr (Or.inl (Or.inr ⟨hrfork ▸ hy1, hy2⟩))) hpy2)
    · -- The base or the fork, a Layer: still an edge of the new row.
      have hy1' : y ∈ col .layer r.base ∨ y ∈ ref r.fork := by
        rw [hrbase, hrfork]; exact hy1
      exact Reach.step (Reach.root (by simp))
        (hedge _ ((mem_branch_targets hrk).mpr (Or.inr ⟨hy1', hy2⟩)) hpy2)

/-- **The reply's promise.** Right after `commit_branch`, Branch `b` holds the
new Commit. With `held_forever`, it keeps holding it. -/
theorem acknowledged_held {s : Store} {ob c r : Row} {b : Nat}
    (hob : ob ∈ s.rows) (hobk : ob.kind = .branch) (hb : ob.id = b)
    (hck : c.kind = .commit) (hfresh : present s c.node = false)
    (hr : r = { ob with head := some c.id }) :
    Held (moveBranch (insertRow s c) r) b (.commit, c.id) := by
  have hs1rows : (insertRow s c).rows = s.rows ++ [c] := by simp [insertRow, hfresh]
  have hrnode : r.node = ob.node := by subst hr; rfl
  have hrk : r.kind = .branch := by subst hr; exact hobk
  have hrhead : r.head = some c.id := by subst hr; rfl
  have hr_in : r ∈ (moveBranch (insertRow s c) r).rows :=
    List.mem_map.mpr ⟨ob, by rw [hs1rows]; exact List.mem_append_left _ hob, by simp [hrnode]⟩
  have hc_in : c ∈ (moveBranch (insertRow s c) r).rows := by
    apply List.mem_map.mpr
    refine ⟨c, by rw [hs1rows]; simp, ?_⟩
    have : ¬ c.node = r.node := by
      intro h; have := congrArg Prod.fst h; simp [Row.node, hck, hrk] at this
    simp [this]
  have hb' : r.node = bnode b := by rw [hrnode]; simp [Row.node, bnode, hobk, hb]
  unfold Held
  rw [← hb']
  refine Reach.step (Reach.root (List.mem_singleton_self _)) ?_
  refine mem_sqlEdges.mpr ⟨r, hr_in, rfl, ?_, present_iff.mpr ⟨c, hc_in, by simp [Row.node, hck]⟩⟩
  exact (mem_branch_targets hrk).mpr (Or.inl (Or.inl (by simp [col, hrhead])))

end LayerfsGc.Lane
