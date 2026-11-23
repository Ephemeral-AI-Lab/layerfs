/-!
# Reachability and the batched mark driver

The generic part of the GC mark (GC spec §6). `Reach roots edges x` says `x`
is reachable from `roots`. A mark is safe when what it keeps contains every
reachable node; it is precise when it keeps nothing else.

`run` models the batched history closure of
`engine/crates/layerfs-persistence/src/history/kept.rs` (`close`, on
`layerfs_v2_0_0` at `04e55a7f4c`):

* `kept_node_seed` writes the roots; every root is then pending.
* Each statement (`kept_node_expand`) starts from at most `BATCH_NODES`
  pending nodes and may produce at most `BATCH_ROWS` rows, counting the
  batch itself. It inserts the nodes it found that were not kept and
  returns them.
* If `rows + batch < BATCH_ROWS`, the statement ran to its end, and nothing
  it found needs following again. Otherwise it stopped at its limit: the
  returned rows and the batch are pushed to `found`, which becomes the next
  round's pending set.

The SQL statement is not modelled. `ExpandOk` states what the driver needs
from it. `run_correct` proves the driver keeps exactly the reachable set for
any statement that meets that contract.
-/

namespace LayerfsGc

section Generic

variable {α : Type}

/-- `x` is reachable from `roots` along `edges`. -/
inductive Reach (roots : List α) (edges : α → List α) : α → Prop where
  | root {x : α} : x ∈ roots → Reach roots edges x
  | step {x y : α} : Reach roots edges x → y ∈ edges x → Reach roots edges y

/-- Every edge out of a node of `K` lands in `K`. -/
def Closed (edges : α → List α) (K : List α) : Prop :=
  ∀ x ∈ K, ∀ y ∈ edges x, y ∈ K

/-- Safety core: a closed set holding the roots holds everything reachable. -/
theorem reach_mem_of_closed {roots : List α} {edges : α → List α} {K : List α}
    (hr : ∀ r ∈ roots, r ∈ K) (hc : Closed edges K) {x : α}
    (h : Reach roots edges x) : x ∈ K := by
  induction h with
  | root hx => exact hr _ hx
  | step _ hy ih => exact hc _ ih _ hy

/-- Reachability is transitive through an intermediate root set. -/
theorem reach_trans {roots mid : List α} {edges : α → List α}
    (hmid : ∀ m ∈ mid, Reach roots edges m) {y : α}
    (h : Reach mid edges y) : Reach roots edges y := by
  induction h with
  | root hx => exact hmid _ hx
  | step _ hy ih => exact Reach.step ih hy

/-- Reachability is monotone in the edge relation. -/
theorem reach_mono {roots : List α} {e₁ e₂ : α → List α}
    (he : ∀ x y, y ∈ e₁ x → y ∈ e₂ x) {y : α}
    (h : Reach roots e₁ y) : Reach roots e₂ y := by
  induction h with
  | root hx => exact Reach.root hx
  | step _ hy ih => exact Reach.step ih (he _ _ hy)

/-- What the driver needs from one `kept_node_expand` statement, run while
`kept` is the content of `kept_node`, from `batch`, with row limit `limit`,
returning `new` (its `RETURNING` rows).

* `fresh`: a returned node was not kept (`NOT EXISTS` and
  `ON CONFLICT DO NOTHING`).
* `reached`: a returned node is reachable from the batch.
* `complete`: when the statement stopped short of its limit, every edge out
  of the batch and out of each returned node lands in `kept` or in `new`.

`complete` carries the driver's whole completion test,
`rows.len() + batch.len() < BATCH_ROWS`. It holds for the SQL only while
every row the recursive CTE produced is either a batch row or a returned
row. See the README for that argument. -/
structure ExpandOk (edges : α → List α) (kept batch : List α) (limit : Nat)
    (new : List α) : Prop where
  fresh : ∀ y ∈ new, y ∉ kept
  reached : ∀ y ∈ new, Reach batch edges y
  complete : new.length + batch.length < limit →
    ∀ x, (x ∈ batch ∨ x ∈ new) → ∀ y ∈ edges x, y ∈ kept ∨ y ∈ new

/-- The driver state: `kept_node`, the pending nodes of this round, and the
nodes found to follow next round. -/
structure State (α : Type) where
  kept : List α
  cur : List α
  found : List α

/-- After `kept_node_seed`: the roots are kept and pending. -/
def State.start (roots : List α) : State α := ⟨roots, roots, []⟩

/-- One driver step: one statement, or the move to the next round. `none`
means the closure has ended. -/
def step (expand : List α → List α → List α) (batchNodes batchRows : Nat)
    (s : State α) : Option (State α) :=
  match s.cur with
  | [] =>
    match s.found with
    | [] => none
    | f => some ⟨s.kept, f, []⟩
  | c =>
    -- `batch` is `c.take batchNodes`; `new` is `expand s.kept batch`.
    if (expand s.kept (c.take batchNodes)).length + (c.take batchNodes).length
        < batchRows then
      some ⟨s.kept ++ expand s.kept (c.take batchNodes), c.drop batchNodes, s.found⟩
    else
      some ⟨s.kept ++ expand s.kept (c.take batchNodes), c.drop batchNodes,
        s.found ++ expand s.kept (c.take batchNodes) ++ c.take batchNodes⟩

/-- The driver with fuel: `none` when the fuel ran out first. -/
def run (expand : List α → List α → List α) (batchNodes batchRows : Nat) :
    Nat → State α → Option (List α)
  | 0, _ => none
  | fuel + 1, s =>
    match step expand batchNodes batchRows s with
    | none => some s.kept
    | some s' => run expand batchNodes batchRows fuel s'

/-- The loop invariant of `close`. -/
structure Inv (roots : List α) (edges : α → List α) (s : State α) : Prop where
  rooted : ∀ r ∈ roots, r ∈ s.kept
  sound : ∀ x ∈ s.kept, Reach roots edges x
  pending_kept : ∀ x, (x ∈ s.cur ∨ x ∈ s.found) → x ∈ s.kept
  frontier : ∀ x ∈ s.kept, x ∈ s.cur ∨ x ∈ s.found ∨ ∀ y ∈ edges x, y ∈ s.kept

theorem inv_start (roots : List α) (edges : α → List α) :
    Inv roots edges (State.start roots) where
  rooted := fun _ h => h
  sound := fun _ h => Reach.root h
  pending_kept := by
    intro x h
    simpa [State.start] using h
  frontier := by
    intro x h
    exact Or.inl h

theorem step_none {expand : List α → List α → List α} {bn br : Nat} {s : State α}
    (h : step expand bn br s = none) : s.cur = [] ∧ s.found = [] := by
  obtain ⟨kept, cur, found⟩ := s
  cases cur with
  | nil => cases found with
    | nil => exact ⟨rfl, rfl⟩
    | cons f fs => simp [step] at h
  | cons c cs =>
    simp only [step] at h
    split at h <;> simp at h

private theorem mem_take_or_drop {x : α} {c : List α} (n : Nat) (h : x ∈ c) :
    x ∈ c.take n ∨ x ∈ c.drop n := by
  rw [← List.take_append_drop n c] at h
  exact List.mem_append.mp h

theorem inv_step {roots : List α} {edges : α → List α}
    {expand : List α → List α → List α} {bn br : Nat}
    (hexp : ∀ K B, ExpandOk edges K B br (expand K B))
    {s s' : State α} (hs : Inv roots edges s)
    (h : step expand bn br s = some s') : Inv roots edges s' := by
  obtain ⟨kept, cur, found⟩ := s
  cases cur with
  | nil =>
    -- No node left this round: start the next round from `found`.
    cases found with
    | nil => simp [step] at h
    | cons f fs =>
      simp only [step, Option.some.injEq] at h
      subst h
      refine ⟨hs.rooted, hs.sound, ?_, ?_⟩
      · intro x hx
        apply hs.pending_kept
        simp only [List.not_mem_nil, false_or, or_false] at hx ⊢
        exact hx
      · intro x hx
        rcases hs.frontier x hx with h1 | h1 | h1
        · simp at h1
        · exact Or.inl h1
        · exact Or.inr (Or.inr h1)
  | cons c cs =>
    -- One statement from the next batch.
    have hok := hexp kept ((c :: cs).take bn)
    have hbatch : ∀ b ∈ (c :: cs).take bn, b ∈ kept := fun b hb =>
      hs.pending_kept b (Or.inl (List.mem_of_mem_take hb))
    have hnew_reach : ∀ y ∈ expand kept ((c :: cs).take bn), Reach roots edges y :=
      fun y hy => reach_trans (fun m hm => hs.sound m (hbatch m hm)) (hok.reached y hy)
    have hrest : ∀ x ∈ (c :: cs).drop bn, x ∈ kept := fun x hx =>
      hs.pending_kept x (Or.inl (List.mem_of_mem_drop hx))
    simp only [step] at h
    split at h
    · -- The statement ran to its end.
      rename_i hshort
      simp only [Option.some.injEq] at h
      subst h
      have hcomplete := hok.complete hshort
      refine ⟨?_, ?_, ?_, ?_⟩
      · intro r hr; exact List.mem_append_left _ (hs.rooted r hr)
      · intro x hx
        rcases List.mem_append.mp hx with hx | hx
        · exact hs.sound x hx
        · exact hnew_reach x hx
      · intro x hx
        rcases hx with hx | hx
        · exact List.mem_append_left _ (hrest x hx)
        · exact List.mem_append_left _ (hs.pending_kept x (Or.inr hx))
      · intro x hx
        rcases List.mem_append.mp hx with hx | hx
        · rcases hs.frontier x hx with h1 | h1 | h1
          · rcases mem_take_or_drop bn h1 with h2 | h2
            · refine Or.inr (Or.inr ?_)
              intro y hy
              rcases hcomplete x (Or.inl h2) y hy with h3 | h3
              · exact List.mem_append_left _ h3
              · exact List.mem_append_right _ h3
            · exact Or.inl h2
          · exact Or.inr (Or.inl h1)
          · exact Or.inr (Or.inr fun y hy => List.mem_append_left _ (h1 y hy))
        · refine Or.inr (Or.inr ?_)
          intro y hy
          rcases hcomplete x (Or.inr hx) y hy with h3 | h3
          · exact List.mem_append_left _ h3
          · exact List.mem_append_right _ h3
    · -- The statement stopped at its limit: follow its rows and batch again.
      simp only [Option.some.injEq] at h
      subst h
      refine ⟨?_, ?_, ?_, ?_⟩
      · intro r hr; exact List.mem_append_left _ (hs.rooted r hr)
      · intro x hx
        rcases List.mem_append.mp hx with hx | hx
        · exact hs.sound x hx
        · exact hnew_reach x hx
      · intro x hx
        rcases hx with hx | hx
        · exact List.mem_append_left _ (hrest x hx)
        · simp only [List.mem_append] at hx
          rcases hx with (hx | hx) | hx
          · exact List.mem_append_left _ (hs.pending_kept x (Or.inr hx))
          · exact List.mem_append_right _ hx
          · exact List.mem_append_left _ (hbatch x hx)
      · intro x hx
        rcases List.mem_append.mp hx with hx | hx
        · rcases hs.frontier x hx with h1 | h1 | h1
          · rcases mem_take_or_drop bn h1 with h2 | h2
            · exact Or.inr (Or.inl (by simp [h2]))
            · exact Or.inl h2
          · exact Or.inr (Or.inl (by simp [h1]))
          · exact Or.inr (Or.inr fun y hy => List.mem_append_left _ (h1 y hy))
        · exact Or.inr (Or.inl (by simp [hx]))

/-- What a finished mark guarantees. -/
structure MarkOk (roots : List α) (edges : α → List α) (K : List α) : Prop where
  rooted : ∀ r ∈ roots, r ∈ K
  closed : Closed edges K
  sound : ∀ x ∈ K, Reach roots edges x

theorem MarkOk.complete {roots : List α} {edges : α → List α} {K : List α}
    (h : MarkOk roots edges K) {x : α} (hx : Reach roots edges x) : x ∈ K :=
  reach_mem_of_closed h.rooted h.closed hx

theorem run_inv {roots : List α} {edges : α → List α}
    {expand : List α → List α → List α} {bn br : Nat}
    (hexp : ∀ K B, ExpandOk edges K B br (expand K B)) :
    ∀ (fuel : Nat) (s : State α) (K : List α), Inv roots edges s →
      run expand bn br fuel s = some K → MarkOk roots edges K
  | 0, _, _, _, h => by simp [run] at h
  | fuel + 1, s, K, hs, h => by
    unfold run at h
    split at h
    · rename_i hnone
      cases h
      obtain ⟨hcur, hfound⟩ := step_none hnone
      refine ⟨hs.rooted, ?_, hs.sound⟩
      intro x hx y hy
      rcases hs.frontier x hx with h1 | h1 | h1
      · rw [hcur] at h1; simp at h1
      · rw [hfound] at h1; simp at h1
      · exact h1 y hy
    · rename_i s' hsome
      exact run_inv hexp fuel s' K (inv_step hexp hs hsome) h

/-- **Mark safety and precision of the batched driver.** If every statement
meets `ExpandOk` and the driver ends, it has kept every node reachable from
the roots and nothing else. -/
theorem run_correct {roots : List α} {edges : α → List α}
    {expand : List α → List α → List α} {bn br fuel : Nat} {K : List α}
    (hexp : ∀ K B, ExpandOk edges K B br (expand K B))
    (h : run expand bn br fuel (State.start roots) = some K) :
    MarkOk roots edges K ∧ ∀ x, x ∈ K ↔ Reach roots edges x := by
  have hm := run_inv hexp fuel _ K (inv_start roots edges) h
  exact ⟨hm, fun x => ⟨hm.sound x, hm.complete⟩⟩

end Generic

end LayerfsGc
