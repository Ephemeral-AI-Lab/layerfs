import LayerfsGc.Closure

/-!
# The history closure (GC spec §6.1)

The model mirrors the history rows on `layerfs_v2_0_0` at `04e55a7f4c`:

* `sqlTargets`/`sqlEdges` follow `kept_node_expand.sql` one `UNION` arm at a
  time. A candidate id becomes an edge only when a row of the joined table
  has it, as with `CROSS JOIN ... ON t.x_id IN (...)`.
* `seed` follows `kept_node_seed.sql`.
* `doctorRequired` and `Doctor` follow the reference checks of
  `engine/crates/layerfs-persistence/src/diagnostic/history.rs` (`inspect`
  and `inventory`).
* `prune` keeps the rows whose node the mark kept, as the rebuild does.

Identities are tagged in LayerFS (`0x12` Commit, `0x32` Layer, `0x11`
Branch, `0x31` LayerStack), so a value can only ever match a row of its own
kind. Here a node is a `(Kind, Nat)` pair, and a column typed `NodeId`
(either a Commit or a Layer) is a `NodeRef`.

Doctor checks that are not about references are not modelled: field widths,
identity derivation, cycles, the `root` equality of a Layer and its source
Commit, NULL-ness, and `lineage::run`. Pruning changes no field, so it
cannot break the field-local checks among them; cycles and lineage are open.
-/

namespace LayerfsGc.History

open LayerfsGc

inductive Kind where
  | stack
  | branch
  | commit
  | layer
  deriving DecidableEq, Repr

/-- A history node: its kind and its identity. -/
abbrev Node := Kind × Nat

/-- A Rust `NodeId`: a Commit or a Layer. -/
inductive NodeRef where
  | commit (id : Nat)
  | layer (id : Nat)
  deriving DecidableEq, Repr

def NodeRef.node : NodeRef → Node
  | .commit n => (.commit, n)
  | .layer n => (.layer, n)

/-- One row of `layer_stack`, `branch`, `layer` or `"commit"`. Columns a
table does not have stay `none`. -/
structure Row where
  kind : Kind
  id : Nat
  /-- `layer_stack_id`; a LayerStack row carries its own id. -/
  stack : Nat
  /-- `root_id` of a Commit or a Layer: where the object closure starts. -/
  root : Option Nat := none
  /-- `layer_stack.head_layer_id` or `branch.head_commit_id`. -/
  head : Option Nat := none
  /-- `branch.base_layer_id` or `commit.base_layer_id`. -/
  base : Option Nat := none
  /-- `layer.parent_layer_id` or `commit.parent_commit_id`. -/
  parent : Option Nat := none
  /-- `commit.second_parent_id`. -/
  second : Option NodeRef := none
  /-- `commit.merged_from_id`. -/
  merged : Option NodeRef := none
  /-- `commit.displaced_id`, a Commit. -/
  displaced : Option Nat := none
  /-- `commit.branch_id`. -/
  branch : Option Nat := none
  /-- `branch.forked_from_id`. -/
  fork : Option NodeRef := none
  /-- `layer.source_commit_id`. -/
  srcCommit : Option Nat := none
  /-- `layer.source_branch_id`. -/
  srcBranch : Option Nat := none
  deriving DecidableEq, Repr

def Row.node (r : Row) : Node := (r.kind, r.id)

/-- The history tables and the two removal-marker tables. -/
structure Store where
  rows : List Row
  /-- `branch_retired.branch_id`. -/
  retiredBranches : List Nat := []
  /-- `layer_stack_retired.layer_stack_id`. -/
  retiredStacks : List Nat := []
  deriving Repr

/-- A row of kind `k` and id `n` exists. -/
def present (s : Store) (t : Node) : Bool := s.rows.any (fun r => r.node == t)

/-- Candidate ids in a column of kind `k`. -/
def col (k : Kind) (o : Option Nat) : List Node := o.toList.map (fun n => (k, n))

/-- Candidate ids in a `NodeId` column. -/
def ref (o : Option NodeRef) : List Node := o.toList.map NodeRef.node

/-- `CROSS JOIN <table k> t ON t.id IN (cands)`: the candidates that are ids
of table `k`. -/
def inTable (k : Kind) (cands : List Node) : List Node := cands.filter (fun t => t.1 == k)

/-- The targets of one row in `kept_node_expand.sql`, before the join checks
that the target row exists. One line per `UNION` arm. -/
def sqlTargets (r : Row) : List Node :=
  match r.kind with
  | .branch =>
      -- "commit" t ON t.commit_id IN (b.head_commit_id, b.forked_from_id)
      inTable .commit (col .commit r.head ++ ref r.fork) ++
      -- layer t ON t.layer_id IN (b.base_layer_id, b.forked_from_id)
      inTable .layer (col .layer r.base ++ ref r.fork)
  | .layer =>
      -- layer t ON t.layer_id = l.parent_layer_id
      inTable .layer (col .layer r.parent) ++
      -- "commit" t ON t.commit_id = l.source_commit_id
      inTable .commit (col .commit r.srcCommit) ++
      -- branch t ON t.branch_id = l.source_branch_id
      inTable .branch (col .branch r.srcBranch)
  | .commit =>
      -- "commit" t ON t.commit_id IN
      --   (c.parent_commit_id, c.second_parent_id, c.merged_from_id, c.displaced_id)
      inTable .commit (col .commit r.parent ++ ref r.second ++ ref r.merged ++
        col .commit r.displaced) ++
      -- layer t ON t.layer_id IN
      --   (c.second_parent_id, c.base_layer_id, c.merged_from_id, c.displaced_id)
      inTable .layer (ref r.second ++ col .layer r.base ++ ref r.merged ++
        col .commit r.displaced) ++
      -- branch t ON t.branch_id = c.branch_id
      inTable .branch (col .branch r.branch)
  | .stack => []

/-- The history edges of a node: every row with that identity, every target
whose row exists. -/
def sqlEdges (s : Store) (n : Node) : List Node :=
  (s.rows.filter (fun r => r.node == n)).flatMap
    (fun r => (sqlTargets r).filter (present s))

def stackLive (s : Store) (k : Nat) : Bool := !(s.retiredStacks.contains k)

/-- `kept_node_seed.sql`: every live LayerStack, every Layer of a live
LayerStack, every live Branch of a live LayerStack. -/
def seed (s : Store) : List Node :=
  (s.rows.filter fun r =>
    match r.kind with
    | .stack => stackLive s r.id
    | .layer => stackLive s r.stack
    | .branch => !(s.retiredBranches.contains r.id) && stackLive s r.stack
    | .commit => false).map Row.node

/-- The references `diagnostic/history.rs` requires to resolve to a row of
the same LayerStack.

* Commit: `parent`, `base`, `second`, `merged_from`, `displaced` (the
  `edges` table, "missing or cross-stack history dependency") and
  `branch_id` ("Commit names absent or cross-stack Branch").
* Layer: `parent`, `source` (`edges`) and, for a Layer with a parent,
  `source_branch` ("Layer source Commit/Branch/root mismatch").
* Branch: `base` ("branch base/stack invalid"), `head` ("branch head
  invalid"), `fork` ("branch fork provenance invalid").
* LayerStack: `head` ("stack head is missing or belongs to another stack"). -/
def doctorRequired (r : Row) : List Node :=
  match r.kind with
  | .commit =>
      col .commit r.parent ++ col .layer r.base ++ ref r.second ++ ref r.merged ++
        col .commit r.displaced ++ col .branch r.branch
  | .layer =>
      col .layer r.parent ++ col .commit r.srcCommit ++
        (if r.parent.isSome then col .branch r.srcBranch else [])
  | .branch => col .layer r.base ++ col .commit r.head ++ ref r.fork
  | .stack => col .layer r.head

/-- A row of node `t` exists in LayerStack `k`. -/
def presentIn (s : Store) (t : Node) (k : Nat) : Prop :=
  ∃ r' ∈ s.rows, r'.node = t ∧ r'.stack = k

/-- The reference part of the full doctor's history check passes. -/
structure Doctor (s : Store) : Prop where
  /-- Every required reference resolves in the row's own LayerStack. -/
  refs : ∀ r ∈ s.rows, ∀ t ∈ doctorRequired r, presentIn s t r.stack
  /-- "history node references absent stack" and "branch base/stack
  invalid": the LayerStack row of every other row exists. -/
  stacks : ∀ r ∈ s.rows, r.kind ≠ .stack → presentIn s (.stack, r.stack) r.stack
  /-- A LayerStack row's `layer_stack_id` is its own id. -/
  stackSelf : ∀ r ∈ s.rows, r.kind = .stack → r.stack = r.id

/-- Every edge the closure follows stays in its LayerStack. The doctor's
reference checks give this for every edge except `source_branch_id` of a
Layer with no parent, which it does not check (see the README). -/
def StackLocal (s : Store) : Prop :=
  ∀ r ∈ s.rows, ∀ r' ∈ s.rows, r'.node ∈ sqlTargets r → r'.stack = r.stack

/-- Primary keys: one row per identity (`PRIMARY KEY` on every history
table; identities are tagged, so they are distinct across tables too). -/
def Keys (s : Store) : Prop :=
  ∀ r ∈ s.rows, ∀ r' ∈ s.rows, r.node = r'.node → r = r'

/-- The rebuild: keep the rows whose node the mark kept. -/
def prune (s : Store) (K : List Node) : Store :=
  { s with rows := s.rows.filter (fun r => r.node ∈ K) }

/-! ## The SQL edges cover every reference the doctor requires -/

theorem mem_inTable {k : Kind} {cands : List Node} {t : Node} :
    t ∈ inTable k cands ↔ t ∈ cands ∧ t.1 = k := by
  simp [inTable]

theorem col_kind {k : Kind} {o : Option Nat} {t : Node} (h : t ∈ col k o) : t.1 = k := by
  simp only [col, List.mem_map, Option.mem_toList] at h
  obtain ⟨n, _, rfl⟩ := h
  rfl

theorem ref_kind {o : Option NodeRef} {t : Node} (h : t ∈ ref o) :
    t.1 = .commit ∨ t.1 = .layer := by
  simp only [ref, List.mem_map, Option.mem_toList] at h
  obtain ⟨n, _, rfl⟩ := h
  cases n <;> simp [NodeRef.node]

/-- **Every reference the doctor requires is an edge the closure follows.**
This is the check that a spec edge list missing `commit.branch_id`,
`layer.source_branch_id`, `merged_from_id` or `displaced_id` would fail
(code map findings 2 and 3). The LayerStack head is the exception: no edge
leads from a LayerStack, and `seed` keeps every Layer of a live one. -/
theorem required_followed {r : Row} {t : Node} (hk : r.kind ≠ .stack)
    (ht : t ∈ doctorRequired r) : t ∈ sqlTargets r := by
  cases hr : r.kind with
  | stack => exact absurd hr hk
  | commit =>
    simp only [doctorRequired, hr, List.mem_append] at ht
    simp only [sqlTargets, hr, List.mem_append, mem_inTable]
    rcases ht with ((((ht | ht) | ht) | ht) | ht) | ht
    · exact Or.inl (Or.inl ⟨Or.inl (Or.inl (Or.inl ht)), col_kind ht⟩)
    · exact Or.inl (Or.inr ⟨Or.inl (Or.inl (Or.inr ht)), col_kind ht⟩)
    · rcases ref_kind ht with h | h
      · exact Or.inl (Or.inl ⟨Or.inl (Or.inl (Or.inr ht)), h⟩)
      · exact Or.inl (Or.inr ⟨Or.inl (Or.inl (Or.inl ht)), h⟩)
    · rcases ref_kind ht with h | h
      · exact Or.inl (Or.inl ⟨Or.inl (Or.inr ht), h⟩)
      · exact Or.inl (Or.inr ⟨Or.inl (Or.inr ht), h⟩)
    · exact Or.inl (Or.inl ⟨Or.inr ht, col_kind ht⟩)
    · exact Or.inr ⟨ht, col_kind ht⟩
  | layer =>
    simp only [doctorRequired, hr, List.mem_append] at ht
    simp only [sqlTargets, hr, List.mem_append, mem_inTable]
    rcases ht with (ht | ht) | ht
    · exact Or.inl (Or.inl ⟨ht, col_kind ht⟩)
    · exact Or.inl (Or.inr ⟨ht, col_kind ht⟩)
    · split at ht
      · exact Or.inr ⟨ht, col_kind ht⟩
      · simp at ht
  | branch =>
    simp only [doctorRequired, hr, List.mem_append] at ht
    simp only [sqlTargets, hr, List.mem_append, mem_inTable]
    rcases ht with (ht | ht) | ht
    · exact Or.inr ⟨Or.inl ht, col_kind ht⟩
    · exact Or.inl ⟨Or.inl ht, col_kind ht⟩
    · rcases ref_kind ht with h | h
      · exact Or.inl ⟨Or.inr ht, h⟩
      · exact Or.inr ⟨Or.inr ht, h⟩

/-- No edge leads to a LayerStack. -/
theorem sqlTargets_not_stack {r : Row} {t : Node} (ht : t ∈ sqlTargets r) :
    t.1 ≠ .stack := by
  intro hs
  unfold sqlTargets at ht
  split at ht <;> simp [List.mem_append, mem_inTable, hs] at ht

theorem mem_sqlEdges {s : Store} {n t : Node} :
    t ∈ sqlEdges s n ↔ ∃ r ∈ s.rows, r.node = n ∧ t ∈ sqlTargets r ∧ present s t := by
  simp [sqlEdges, List.mem_flatMap, List.mem_filter, and_assoc]

theorem present_iff {s : Store} {t : Node} : present s t ↔ ∃ r ∈ s.rows, r.node = t := by
  simp [present, List.any_eq_true]

theorem mem_seed {s : Store} {n : Node} :
    n ∈ seed s ↔ ∃ r ∈ s.rows, r.node = n ∧
      (match r.kind with
       | .stack => stackLive s r.id
       | .layer => stackLive s r.stack
       | .branch => !(s.retiredBranches.contains r.id) && stackLive s r.stack
       | .commit => false) = true := by
  simp [seed, List.mem_map, List.mem_filter]
  constructor
  · rintro ⟨r, ⟨h1, h2⟩, h3⟩; exact ⟨r, h1, h3, h2⟩
  · rintro ⟨r, h1, h3, h2⟩; exact ⟨r, ⟨h1, h2⟩, h3⟩

/-! ## Everything kept lies in a live LayerStack -/

/-- Every row of a reached node lies in a live LayerStack. -/
theorem reach_live {s : Store} (hkeys : Keys s) (hloc : StackLocal s) (hdoc : Doctor s)
    {n : Node} (h : Reach (seed s) (sqlEdges s) n) :
    ∀ r ∈ s.rows, r.node = n → stackLive s r.stack = true := by
  induction h with
  | root hx =>
    intro r hr hn
    obtain ⟨r0, hr0, hn0, hlive⟩ := mem_seed.mp hx
    have := hkeys r hr r0 hr0 (hn.trans hn0.symm)
    subst this
    cases hk : r.kind with
    | stack =>
      simp only [hk] at hlive
      rw [hdoc.stackSelf r hr hk]
      exact hlive
    | layer => simpa [hk] using hlive
    | branch =>
      simp only [hk, Bool.and_eq_true] at hlive
      exact hlive.2
    | commit => simp [hk] at hlive
  | step _ hy ih =>
    intro r hr hn
    obtain ⟨rx, hrx, hnx, hty, _⟩ := mem_sqlEdges.mp hy
    have h1 := hloc rx hrx r hr (hn ▸ hty)
    rw [h1]
    exact ih rx hrx hnx

/-- A kept LayerStack node is a live LayerStack: no edge leads to one, so it
was seeded. -/
theorem reach_stack {s : Store} {k : Nat}
    (h : Reach (seed s) (sqlEdges s) (.stack, k)) : (.stack, k) ∈ seed s := by
  cases h with
  | root hx => exact hx
  | step _ hy =>
    obtain ⟨_, _, _, hty, _⟩ := mem_sqlEdges.mp hy
    exact absurd rfl (sqlTargets_not_stack hty)

/-! ## Pruning to a correct mark passes the doctor -/

theorem mem_prune {s : Store} {K : List Node} {r : Row} :
    r ∈ (prune s K).rows ↔ r ∈ s.rows ∧ r.node ∈ K := by
  simp [prune, List.mem_filter]

/-- **History safety.** Pruning a doctor-clean Store to a mark that is rooted
at `seed`, closed under the SQL edges and holds only reachable nodes leaves
a Store whose history references the doctor accepts: no retained row names
a collected one. -/
theorem prune_doctor {s : Store} {K : List Node}
    (hkeys : Keys s) (hloc : StackLocal s) (hdoc : Doctor s)
    (hm : MarkOk (seed s) (sqlEdges s) K) : Doctor (prune s K) := by
  refine ⟨?_, ?_, ?_⟩
  · intro r hr t ht
    obtain ⟨hr, hrK⟩ := mem_prune.mp hr
    obtain ⟨r', hr', hn', hs'⟩ := hdoc.refs r hr t ht
    refine ⟨r', mem_prune.mpr ⟨hr', ?_⟩, hn', hs'⟩
    by_cases hk : r.kind = .stack
    · -- The LayerStack head: a Layer of a live LayerStack, so seeded.
      have hlive := reach_live hkeys hloc hdoc (hm.sound _ hrK) r hr rfl
      apply hm.rooted
      apply mem_seed.mpr
      refine ⟨r', hr', rfl, ?_⟩
      have hl : r'.kind = .layer := by
        simp only [doctorRequired, hk] at ht
        rw [← hn'] at ht
        exact col_kind ht
      simp only [hl, hs']
      exact hlive
    · apply hm.closed _ hrK
      have hf := required_followed hk ht
      rw [← hn'] at hf
      exact mem_sqlEdges.mpr ⟨r, hr, rfl, hf, present_iff.mpr ⟨r', hr', rfl⟩⟩
  · intro r hr hk
    obtain ⟨hr, hrK⟩ := mem_prune.mp hr
    obtain ⟨r', hr', hn', hs'⟩ := hdoc.stacks r hr hk
    refine ⟨r', mem_prune.mpr ⟨hr', ?_⟩, hn', hs'⟩
    have hlive := reach_live hkeys hloc hdoc (hm.sound _ hrK) r hr rfl
    apply hm.rooted
    apply mem_seed.mpr
    refine ⟨r', hr', rfl, ?_⟩
    have hk' : r'.kind = .stack := congrArg Prod.fst hn'
    have hid : r'.id = r.stack := congrArg Prod.snd hn'
    simp only [hk', hid]
    exact hlive
  · intro r hr hk
    exact hdoc.stackSelf r (mem_prune.mp hr).1 hk

/-- Spec #42 item 29: a retired Branch that a kept Commit names stays, and
so does its head Commit. -/
theorem retired_branch_kept {s : Store} {K : List Node}
    (hm : MarkOk (seed s) (sqlEdges s) K) {c b : Row}
    (hc : c ∈ s.rows) (hck : c.kind = .commit) (hcK : c.node ∈ K)
    (hb : b ∈ s.rows) (hbk : b.kind = .branch) (hcb : c.branch = some b.id) :
    b.node ∈ K ∧ ∀ h ∈ s.rows, h.node = (.commit, b.head.getD 0) → b.head.isSome → h.node ∈ K := by
  have hbK : b.node ∈ K := by
    apply hm.closed _ hcK
    refine mem_sqlEdges.mpr ⟨c, hc, rfl, ?_, present_iff.mpr ⟨b, hb, rfl⟩⟩
    simp [sqlTargets, hck, mem_inTable, col, hcb, Row.node, hbk]
  refine ⟨hbK, ?_⟩
  intro h hh hn hsome
  apply hm.closed _ hbK
  refine mem_sqlEdges.mpr ⟨b, hb, rfl, ?_, present_iff.mpr ⟨h, hh, rfl⟩⟩
  obtain ⟨v, hv⟩ := Option.isSome_iff_exists.mp hsome
  simp [hv] at hn
  simp [sqlTargets, hbk, mem_inTable, col, hv, hn]

end LayerfsGc.History
