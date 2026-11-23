/-!
# #4: Commit capture and install keep the live view

The Workspace overlay (`layerfs-overlay` on `layerfs_v2_0_0` at
`04e55a7f4c`) keys every row by a key and a **generation**: `fs_inode`
`(inode, generation)`, `fs_entry` `(parent, name, generation)`, and the
data cells. A read takes the newest row whose generation lies in
`(installed, active]` and otherwise falls through to the base
(`INODE_LOOKUP`, `DIRECTORY_ENTRY_LOOKUP`: `generation <= ?active AND
generation > ?installed ORDER BY generation DESC LIMIT 1`).

* Capture (`lifetime/generation.rs:79-117`): `captured := active = g`,
  `active := g+1`. No row is copied, and writes go on in `g+1`.
* Construction builds the published root from the base and the rows in
  `(installed, g]`.
* Install (`generation.rs:12-28`): `base := R'`, `installed := g`. Rows at or
  below `g` stop being visible and are deleted later (`RETIRE`).
* On a definite failure before publication, `FOLD` merges `g` into `g+1`:
  the `g+1` row wins, otherwise the `g` row moves up; then `g` is retired.

Here a key stands for any of those keys (an inode, a directory entry, a
data cell), and a value for what that row says, whiteouts included. The
orphan area (generation `-1`) and byte cutoffs inside a cell are not
modelled: a cell is the unit.

`install_view` proves the view is unchanged by install, on one condition:
for every key **without** a `g+1` row, the published root agrees with the
captured view. That condition is the obligation the code must meet. Two
code facts meet it:

* Construction writes the captured view (`construct`), so the root agrees
  everywhere it was built from the capture.
* Where construction deliberately keeps the base's value (an mtime-only
  change), `keep_live_row` (`namespace/inode.rs:85-119`) first copies the
  captured row into `g+1`, so that key has a `g+1` row.

If the Branch head moved and the Commit merged upstream changes, the root is
not the captured view and the view changes by design. The theorem does not
apply then.
-/

namespace LayerfsGc.Capture

variable {K V : Type} [DecidableEq K]

/-- One overlay row. -/
structure ORow (K V : Type) where
  key : K
  gen : Nat
  val : V

/-- The row of `k` at generation `g` (the primary key makes it unique). -/
def at? (rows : List (ORow K V)) (k : K) (g : Nat) : Option V :=
  (rows.find? (fun r => r.key = k ∧ r.gen = g)).map (·.val)

/-- The newest row of `k` with generation in `(lo, hi]`. -/
def win (rows : List (ORow K V)) (k : K) (lo : Nat) : Nat → Option V
  | 0 => none
  | hi + 1 => if hi + 1 ≤ lo then none else (at? rows k (hi + 1)).or (win rows k lo hi)

/-- What a read of `k` returns. -/
def view (rows : List (ORow K V)) (base : K → V) (lo hi : Nat) (k : K) : V :=
  (win rows k lo hi).getD (base k)

/-- The published root a Commit constructs from capture `g`. -/
def construct (rows : List (ORow K V)) (base : K → V) (lo g : Nat) : K → V :=
  view rows base lo g

theorem win_le {rows : List (ORow K V)} {k : K} {lo : Nat} :
    ∀ {hi : Nat}, hi ≤ lo → win rows k lo hi = none
  | 0, _ => rfl
  | hi + 1, h => by simp [win, h]

theorem win_succ {rows : List (ORow K V)} {k : K} {lo hi : Nat} (h : lo ≤ hi) :
    win rows k lo (hi + 1) = (at? rows k (hi + 1)).or (win rows k lo hi) := by
  simp only [win]
  rw [ite_eq_right_iff.mpr (fun h => absurd h (by omega))]

/-- **Install keeps the live view.** Before install the Workspace reads the
window `(lo, g+1]` over base `R`. After install it reads `(g, g+1]` over
`R'`. Every key reads the same, provided `R'` agrees with the captured view
on every key that has no `g+1` row. -/
theorem install_view (rows : List (ORow K V)) (R R' : K → V) {lo g : Nat} (hlo : lo ≤ g)
    (hroot : ∀ k, at? rows k (g + 1) = none → R' k = construct rows R lo g k) (k : K) :
    view rows R lo (g + 1) k = view rows R' g (g + 1) k := by
  simp only [view]
  rw [win_succ hlo, win_succ (Nat.le_refl g), win_le (Nat.le_refl g)]
  cases h : at? rows k (g + 1) with
  | some v => rfl
  | none => simp [hroot k h, construct, view]

/-- The common case: the published root is exactly what was constructed. -/
theorem install_view_constructed (rows : List (ORow K V)) (R : K → V) {lo g : Nat}
    (hlo : lo ≤ g) (k : K) :
    view rows R lo (g + 1) k = view rows (construct rows R lo g) g (g + 1) k :=
  install_view rows R _ hlo (fun _ _ => rfl) k

/-- Capture itself changes nothing: no row is at `g+1` yet. -/
theorem capture_view (rows : List (ORow K V)) (R : K → V) {lo g : Nat} (hlo : lo ≤ g)
    (hnone : ∀ k, at? rows k (g + 1) = none) (k : K) :
    view rows R lo g k = view rows R lo (g + 1) k := by
  simp only [view]
  rw [win_succ hlo, hnone k]
  rfl

/-! ## Retire: deleting hidden rows changes nothing -/

theorem at?_filter_gen (rows : List (ORow K V)) (k : K) {g h : Nat} (hh : g < h) :
    at? (rows.filter (fun r => g < r.gen)) k h = at? rows k h := by
  induction rows with
  | nil => rfl
  | cons r rs ih =>
    by_cases hr : g < r.gen
    · rw [List.filter_cons_of_pos (by simpa using hr)]
      unfold at? at ih ⊢
      rw [List.find?_cons, List.find?_cons]
      split
      · rfl
      · exact ih
    · rw [List.filter_cons_of_neg (by simpa using hr), ih]
      have : ¬ (r.key = k ∧ r.gen = h) := fun ⟨_, h2⟩ => hr (h2 ▸ hh)
      unfold at?
      rw [List.find?_cons]
      simp [this]

theorem win_filter (rows : List (ORow K V)) (k : K) (g : Nat) :
    ∀ hi, win (rows.filter (fun r => g < r.gen)) k g hi = win rows k g hi
  | 0 => rfl
  | hi + 1 => by
    simp only [win]
    split
    · rfl
    · rename_i h
      rw [at?_filter_gen rows k (by omega), win_filter rows k g hi]

/-- **`RETIRE(g)` after install changes no read.** -/
theorem retire_view (rows : List (ORow K V)) (R : K → V) (g hi : Nat) (k : K) :
    view (rows.filter (fun r => g < r.gen)) R g hi k = view rows R g hi k := by
  simp only [view, win_filter]

/-! ## Fold: a failed Commit merges `g` into `g+1` without changing a read -/

/-- What `FOLD` leaves, stated per key: the `g+1` row wins, otherwise the `g`
row moves up; nothing is left at `g`; other generations are untouched
(`lifetime/composition.rs:39-215`). -/
structure FoldOk (rows rows' : List (ORow K V)) (g : Nat) : Prop where
  up : ∀ k, at? rows' k (g + 1) = (at? rows k (g + 1)).or (at? rows k g)
  gone : ∀ k, at? rows' k g = none
  rest : ∀ k h, h ≠ g → h ≠ g + 1 → at? rows' k h = at? rows k h

theorem win_rest {rows rows' : List (ORow K V)} {g lo : Nat} {k : K}
    (hrest : ∀ h, h ≠ g → h ≠ g + 1 → at? rows' k h = at? rows k h) :
    ∀ hi, hi < g → win rows' k lo hi = win rows k lo hi
  | 0, _ => rfl
  | hi + 1, h => by
    simp only [win]
    split
    · rfl
    · rw [hrest (hi + 1) (by omega) (by omega), win_rest hrest hi (by omega)]

/-- **A failed Commit's fold changes no read.** The window is `(lo, g+1]` with
`lo < g` (the floor is below the capture). -/
theorem fold_view {rows rows' : List (ORow K V)} {g lo : Nat} (hf : FoldOk rows rows' g)
    (hlo : lo < g) (R : K → V) (k : K) :
    view rows' R lo (g + 1) k = view rows R lo (g + 1) k := by
  obtain ⟨g', rfl⟩ : ∃ g', g = g' + 1 := ⟨g - 1, by omega⟩
  simp only [view]
  rw [win_succ (by omega), win_succ (by omega), win_succ (by omega), win_succ (by omega),
    hf.up k, hf.gone k, win_rest (hf.rest k) g' (by omega)]
  cases at? rows k (g' + 1 + 1) <;> cases at? rows k (g' + 1) <;> rfl

end LayerfsGc.Capture
