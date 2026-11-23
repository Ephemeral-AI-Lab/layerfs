/-!
# #5: Delta chains stay decodable at any history length

On `layerfs_v2_0_0` at `04e55a7f4c` (`core/crates/layerfs-storage`):

* A stored record is FULL or PREFIX with exactly one base identity
  (`encoding/delta/record.rs`). Depth is never stored.
* A Save selects a base only among **registered winners** of the same role
  (`save/select.rs`: pending bases are published first). Winners are
  insert-only (`ON CONFLICT(object_id) DO NOTHING`); a second copy of an
  identity loses and changes nothing.
* `trial_base` (`encoding/delta/select.rs:731-757`) takes a base only below
  the role's cap; otherwise it re-bases onto the chain's FULL root, or
  stores FULL. So `depth(new) = depth(base) + 1 ≤ cap(role)`. Pooled leaves
  and program pages follow the same bound (`save/pooled.rs`,
  `delta/branch.rs`).
* A read walks the chain and refuses a repeated identity ("dependency
  cycle"), more edges than the cap ("dependency chain depth"), a missing
  base, or a role mismatch (`delta/read.rs:243-270`).
* Compaction keeps every base of every kept record, transitively, and copies
  records byte for byte (`pack/repack.rs`). No record is re-based or
  re-encoded, and the policy caps are copied unchanged.

The model carries two ghost fields per record: an insertion `stamp` and the
chain `depth`. `decode_ok` proves the read's guarded walk always succeeds
and never trips a guard, after any valid run of Saves and compactions.
The caps (`cap : role → Nat`) are fixed, as the Store policy is frozen.
-/

namespace LayerfsGc.Delta

/-- A stored winner record: identity, role, and base (`none` = FULL). -/
structure Rec where
  id : Nat
  role : Nat
  base : Option Nat
  deriving DecidableEq, Repr

structure DRow where
  record : Rec
  /-- Ghost: registration order. -/
  stamp : Nat
  /-- Ghost: chain depth (edges to the FULL root). -/
  depth : Nat

structure DStore where
  rows : List DRow
  next : Nat

def DStore.empty : DStore := ⟨[], 0⟩

def find (s : DStore) (i : Nat) : Option DRow := s.rows.find? (fun x => x.record.id = i)

/-- The invariant, for role caps `cap`. -/
structure Good (cap : Nat → Nat) (s : DStore) : Prop where
  keys : ∀ x ∈ s.rows, ∀ y ∈ s.rows, x.record.id = y.record.id → x = y
  below : ∀ x ∈ s.rows, x.stamp < s.next
  full : ∀ x ∈ s.rows, x.record.base = none → x.depth = 0
  base : ∀ x ∈ s.rows, ∀ b, x.record.base = some b →
    ∃ y ∈ s.rows, y.record.id = b ∧ y.record.role = x.record.role ∧ y.stamp < x.stamp ∧
      x.depth = y.depth + 1
  capped : ∀ x ∈ s.rows, x.depth ≤ cap x.record.role

inductive Op where
  /-- A Save registers one record (first wins). -/
  | publish (r : Rec)
  /-- Compaction keeps the records whose identity the mark kept. -/
  | compact (K : List Nat)

def depthOf (s : DStore) (r : Rec) : Nat :=
  match r.base with
  | none => 0
  | some b => ((find s b).map (·.depth)).getD 0 + 1

def apply (s : DStore) : Op → DStore
  | .publish r =>
    if (find s r.id).isSome then s else ⟨s.rows ++ [⟨r, s.next, depthOf s r⟩], s.next + 1⟩
  | .compact K => ⟨s.rows.filter (fun x => x.record.id ∈ K), s.next⟩

def Allowed (cap : Nat → Nat) (s : DStore) : Op → Prop
  /- Base selection: a registered winner of the same role, below the cap. -/
  | .publish r => ∀ b, r.base = some b →
      ∃ y ∈ s.rows, y.record.id = b ∧ y.record.role = r.role ∧ y.depth + 1 ≤ cap r.role
  /- The mark keeps every base of every kept record. -/
  | .compact K => ∀ x ∈ s.rows, x.record.id ∈ K → ∀ b, x.record.base = some b → b ∈ K

theorem find_of_mem {s : DStore} {cap : Nat → Nat} (hs : Good cap s) {y : DRow}
    (hy : y ∈ s.rows) : find s y.record.id = some y := by
  unfold find
  cases h : s.rows.find? (fun x => x.record.id = y.record.id) with
  | none =>
    have := List.find?_eq_none.mp h y hy
    simp at this
  | some z =>
    have hz := List.mem_of_find?_eq_some h
    have hzid := List.find?_some h
    simp at hzid
    rw [hs.keys z hz y hy hzid]

theorem find_isSome {s : DStore} {i : Nat} :
    (find s i).isSome ↔ ∃ x ∈ s.rows, x.record.id = i := by
  unfold find
  constructor
  · intro h
    obtain ⟨z, hz⟩ := Option.isSome_iff_exists.mp h
    exact ⟨z, List.mem_of_find?_eq_some hz, by simpa using List.find?_some hz⟩
  · rintro ⟨x, hx, hxi⟩
    cases h : s.rows.find? (fun x => x.record.id = i) with
    | none => have := List.find?_eq_none.mp h x hx; simp [hxi] at this
    | some _ => rfl

theorem good_empty (cap : Nat → Nat) : Good cap DStore.empty := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> simp [DStore.empty]

theorem good_apply {cap : Nat → Nat} {s : DStore} {op : Op} (hs : Good cap s)
    (ha : Allowed cap s op) : Good cap (apply s op) := by
  cases op with
  | publish r =>
    simp only [apply]
    split
    · exact hs
    · rename_i hfresh
      have hfresh' : ∀ x ∈ s.rows, x.record.id ≠ r.id := fun x hx he =>
        hfresh (find_isSome.mpr ⟨x, hx, he⟩)
      have hmem : ∀ x, x ∈ s.rows ++ [⟨r, s.next, depthOf s r⟩] ↔
          x ∈ s.rows ∨ x = ⟨r, s.next, depthOf s r⟩ := by intro x; simp
      refine ⟨?_, ?_, ?_, ?_, ?_⟩
      · intro x hx y hy hxy
        rcases (hmem x).mp hx with hx | rfl <;> rcases (hmem y).mp hy with hy | rfl
        · exact hs.keys x hx y hy hxy
        · exact absurd hxy (hfresh' x hx)
        · exact absurd hxy.symm (hfresh' y hy)
        · rfl
      · intro x hx
        rcases (hmem x).mp hx with hx | rfl
        · exact Nat.lt_succ_of_lt (hs.below x hx)
        · exact Nat.lt_succ_self _
      · intro x hx hb
        rcases (hmem x).mp hx with hx | rfl
        · exact hs.full x hx hb
        · have hb' : r.base = none := hb
          simp [depthOf, hb']
      · intro x hx b hb
        rcases (hmem x).mp hx with hx | rfl
        · obtain ⟨y, hy, h1, h2, h3, h4⟩ := hs.base x hx b hb
          exact ⟨y, (hmem y).mpr (Or.inl hy), h1, h2, h3, h4⟩
        · obtain ⟨y, hy, h1, h2, _⟩ := ha b hb
          refine ⟨y, (hmem y).mpr (Or.inl hy), h1, h2, hs.below y hy, ?_⟩
          have hb' : r.base = some b := hb
          have hf : find s b = some y := by rw [← h1]; exact find_of_mem hs hy
          simp [depthOf, hb', hf]
      · intro x hx
        rcases (hmem x).mp hx with hx | rfl
        · exact hs.capped x hx
        · simp only [depthOf]
          cases hb : r.base with
          | none => exact Nat.zero_le _
          | some b =>
            obtain ⟨y, hy, h1, _, h3⟩ := ha b hb
            have hf : find s b = some y := by rw [← h1]; exact find_of_mem hs hy
            simpa [hf] using h3
  | compact K =>
    simp only [apply]
    have hmem : ∀ x, x ∈ s.rows.filter (fun x => x.record.id ∈ K) ↔ x ∈ s.rows ∧ x.record.id ∈ K := by
      intro x; simp [List.mem_filter]
    refine ⟨?_, ?_, ?_, ?_, ?_⟩
    · intro x hx y hy; exact hs.keys x ((hmem x).mp hx).1 y ((hmem y).mp hy).1
    · intro x hx; exact hs.below x ((hmem x).mp hx).1
    · intro x hx; exact hs.full x ((hmem x).mp hx).1
    · intro x hx b hb
      obtain ⟨hx, hxK⟩ := (hmem x).mp hx
      obtain ⟨y, hy, h1, h2, h3, h4⟩ := hs.base x hx b hb
      exact ⟨y, (hmem y).mpr ⟨hy, h1 ▸ ha x hx hxK b hb⟩, h1, h2, h3, h4⟩
    · intro x hx; exact hs.capped x ((hmem x).mp hx).1

/-- A run of Saves and compactions, each allowed when it happens. -/
def Valid (cap : Nat → Nat) : DStore → List Op → Prop
  | _, [] => True
  | s, op :: ops => Allowed cap s op ∧ Valid cap (apply s op) ops

def applyAll : DStore → List Op → DStore
  | s, [] => s
  | s, op :: ops => applyAll (apply s op) ops

theorem good_applyAll {cap : Nat → Nat} : ∀ (ops : List Op) (s : DStore), Good cap s →
    Valid cap s ops → Good cap (applyAll s ops)
  | [], _, hs, _ => hs
  | op :: ops, s, hs, ⟨ha, hv⟩ => good_applyAll ops (apply s op) (good_apply hs ha) hv

/-! ## The read's guarded walk (`delta/read.rs:243-270`) -/

/-- Follow bases from `i`, refusing a missing record (`ObjectMissing`), a
repeated identity ("dependency cycle"), more than `c` edges ("dependency
chain depth") or a role change ("dependency role"). Returns the chain. -/
def walk (s : DStore) (c role : Nat) : Nat → Nat → List Nat → Option (List Nat)
  | 0, _, _ => none
  | f + 1, i, chain =>
    match find s i with
    | none => none
    | some x =>
      if i ∈ chain then none
      else if x.record.role ≠ role then none
      else match x.record.base with
        | none => some (chain ++ [i])
        | some b => if c < (chain ++ [i]).length then none else walk s c role f b (chain ++ [i])

/-- The read of `x` starts with an empty chain, its role and its role's cap. -/
def read (s : DStore) (cap : Nat → Nat) (x : DRow) : Option (List Nat) :=
  walk s (cap x.record.role) x.record.role (cap x.record.role + 1) x.record.id []

theorem walk_ok {cap : Nat → Nat} {s : DStore} (hs : Good cap s) (c role : Nat) :
    ∀ (d : Nat) (x : DRow), x ∈ s.rows → x.depth = d → x.record.role = role →
      ∀ (chain : List Nat) (f : Nat),
        (∀ j ∈ chain, ∃ y ∈ s.rows, y.record.id = j ∧ x.stamp < y.stamp) →
        chain.length + d ≤ c → d < f →
        ∃ path, walk s c role f x.record.id chain = some (chain ++ path) ∧ path.length = d + 1 := by
  intro d
  induction d with
  | zero =>
    intro x hx hd hr chain f hchain hlen hf
    obtain ⟨f', rfl⟩ : ∃ f', f = f' + 1 := ⟨f - 1, by omega⟩
    have hnot : x.record.id ∉ chain := by
      intro hj
      obtain ⟨y, hy, hyid, hlt⟩ := hchain _ hj
      rw [hs.keys y hy x hx hyid] at hlt
      exact Nat.lt_irrefl _ hlt
    have hbase : x.record.base = none := by
      cases hb : x.record.base with
      | none => rfl
      | some b =>
        obtain ⟨y, _, _, _, _, h4⟩ := hs.base x hx b hb
        omega
    refine ⟨[x.record.id], ?_, rfl⟩
    simp [walk, find_of_mem hs hx, hnot, hr, hbase]
  | succ d ih =>
    intro x hx hd hr chain f hchain hlen hf
    obtain ⟨f', rfl⟩ : ∃ f', f = f' + 1 := ⟨f - 1, by omega⟩
    have hnot : x.record.id ∉ chain := by
      intro hj
      obtain ⟨y, hy, hyid, hlt⟩ := hchain _ hj
      rw [hs.keys y hy x hx hyid] at hlt
      exact Nat.lt_irrefl _ hlt
    obtain ⟨b, hb⟩ : ∃ b, x.record.base = some b := by
      cases hb : x.record.base with
      | none => have := hs.full x hx hb; omega
      | some b => exact ⟨b, rfl⟩
    obtain ⟨y, hy, hyid, hyrole, hylt, hyd⟩ := hs.base x hx b hb
    have hchain' : ∀ j ∈ chain ++ [x.record.id], ∃ z ∈ s.rows, z.record.id = j ∧ y.stamp < z.stamp := by
      intro j hj
      rcases List.mem_append.mp hj with hj | hj
      · obtain ⟨z, hz, hzid, hzlt⟩ := hchain j hj
        exact ⟨z, hz, hzid, Nat.lt_trans hylt hzlt⟩
      · simp at hj
        exact ⟨x, hx, hj.symm, hylt⟩
    obtain ⟨path, hwalk, hplen⟩ := ih y hy (by omega) (hyrole.trans hr) (chain ++ [x.record.id]) f'
      hchain' (by simp; omega) (by omega)
    refine ⟨x.record.id :: path, ?_, by simp [hplen]⟩
    have hle : ¬ c < (chain ++ [x.record.id]).length := by simp; omega
    simp only [walk, find_of_mem hs hx, hnot, hr, hb, ite_false, ne_eq, not_true_eq_false]
    rw [ite_eq_right_iff.mpr (fun h => absurd h hle), ← hyid, hwalk]
    simp

/-- **Every stored record decodes, after any valid run of Saves and
compactions, at any history length.** The read's walk reaches a FULL root
without tripping the cycle, depth, role or missing-base guard, and visits
`depth + 1 ≤ cap + 1` records. -/
theorem decode_ok {cap : Nat → Nat} {ops : List Op} (hv : Valid cap DStore.empty ops)
    {x : DRow} (hx : x ∈ (applyAll DStore.empty ops).rows) :
    ∃ chain, read (applyAll DStore.empty ops) cap x = some chain ∧
      chain.length = x.depth + 1 ∧ x.depth ≤ cap x.record.role := by
  have hs := good_applyAll ops _ (good_empty cap) hv
  obtain ⟨path, hw, hlen⟩ := walk_ok hs (cap x.record.role) x.record.role x.depth x hx rfl rfl []
    (cap x.record.role + 1) (by simp) (by simpa using hs.capped x hx)
    (Nat.lt_succ_of_le (hs.capped x hx))
  exact ⟨path, by simpa [read] using hw, hlen, hs.capped x hx⟩

/-- **Compaction never changes a record, its base or its depth.** -/
theorem compact_keeps_record {s : DStore} {K : List Nat} {x : DRow}
    (hx : x ∈ (apply s (.compact K)).rows) : x ∈ s.rows := by
  simp only [apply, List.mem_filter] at hx
  exact hx.1

end LayerfsGc.Delta
