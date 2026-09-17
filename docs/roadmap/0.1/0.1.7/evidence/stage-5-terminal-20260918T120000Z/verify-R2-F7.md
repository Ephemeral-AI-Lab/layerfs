# R2-F7 verification: the whole-tree walk ceiling (verify-R2-F7)

| | |
| --- | --- |
| Row | `R2-F7` / `N-5` / `VF-4` — the 4,096-entry whole-tree walk ceiling is declared with both consequences, with a reproducer |
| Tree | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (`docs(stage5): retain the round-4 check logs on the round tree`), working tree clean for all code and evidence paths this row touches |
| Verdict | **PASS** — with one material boundary-figure erratum recorded below (§6) |
| Method | Read-only inspection plus reproduction through the public entry points (`build_filesystem`, `update_filesystem`, the two constants) from an independent `/tmp` probe; both named test suites re-run `--locked` |

The task's stated hash `99743b2cf3a869b7d8897a1f16b82d742aeedc40` does not resolve in
this repository (`git cat-file -t` exit 128); HEAD matches its 9-character prefix
`99743b2cf`. All code paths this row depends on (`validate.rs`, `limits.rs`, both test
files, the §6 row) are unmodified in the working tree; the §6 row was additionally
confirmed in the committed tree via `git show HEAD:...stage-5-report.md`.

## 1. Commands run and exit codes

| Command | Exit | Result |
| --- | --- | --- |
| `git log -1 --format='%H %s'` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e docs(stage5): retain the round-4 check logs on the round tree` |
| `git status --porcelain` | 0 | 4 pre-existing tracked modifications (`codec.rs`, `physical-encoding-and-packing.md`, `stage-5-completion-report-20260917.md`, `stage-5-report.md`) + sibling `verify-*.md` files from other agents; none touches this row's code, tests or §6 walk row |
| grep `MAXIMUM_WALK_ENTRIES\|MAXIMUM_CYCLE_CHECK_ENTRIES` under `core/crates/layerfs-content/src` | 0 | 8 matches: `limits.rs:72`; `validate.rs:54,55,302,319,467,603,663` |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_bounds --locked` | 0 | **10 passed / 0 failed** (includes both reproducers) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_topology --locked` | 0 | **17 passed / 0 failed** |
| `/tmp/walk-boundary` probe: `cargo +1.85.1 run --quiet --offline` (`CARGO_TARGET_DIR=/tmp/walk-boundary-target`) | 0 | boundary matrix below (§5); three earlier compile iterations exited 101 before the API was matched — only the final exit-0 run's output is evidence |
| `git show --stat 2fe2a4642` and `git show 2fe2a4642 -- .../validate.rs` | 0 | round-3 fix commit; `validate.rs` hunk touches only the doc comment and the alias |
| `git diff f288d2af7..HEAD -- .../validate.rs` | 0 | 5 insertions / 3 deletions — doc + alias only; **enforcement code identical to the round-2 reviewed tree** |
| `git diff --stat 2fe2a4642..HEAD -- .../validate.rs .../limits.rs` | 0 | empty — no drift since the fix |
| `git show HEAD:.../stage-5-report.md \| grep -n "Whole-tree walk entries"` | 0 | row present at line 410 in the committed tree |
| `cat .../stage-5-terminal-20260918T020000Z/walk-ceiling.log` | 0 | filtered run of the rebind reproducer: 1 passed, `EXIT=0` |

## 2. The constant: one figure, one meaning

`core/crates/layerfs-content/src/filesystem/limits.rs:72`:

```rust
pub const MAXIMUM_WALK_ENTRIES: usize = 4_096;
```

declared once, with the per-walk scope and both consequences in its doc
(`limits.rs:56-71`):

> "The ceiling is charged **once per walk, not once per operation**: the
> effective-cycle check walks the subtree of every directory the operation
> rebinds, and each of those walks gets its own allowance, so an operation that
> rebinds N directories may spend up to N times this many entries." (L56-59)
>
> "one `build_filesystem` call is refused above 4,095 bindings, because its
> single reachability walk charges every entry the tree states" (L63-64)
>
> "an existing directory whose effective subtree exceeds the ceiling can never
> be renamed or relocated, however small the change is, because the walk of the
> directory being rebound is bounded by this figure and exceeding a work bound
> is an explicit refusal - the same error a genuine cycle gets" (L67-71)

`core/crates/layerfs-content/src/filesystem/validate.rs:55` aliases it — one
figure, one meaning, no second number:

```rust
pub const MAXIMUM_CYCLE_CHECK_ENTRIES: usize = crate::filesystem::limits::MAXIMUM_WALK_ENTRIES;
```

## 3. stage-5-report.md §6 row (committed tree, line 410)

> "| Whole-tree walk entries | enforced | ≤ 4,096 bindings **per walk**, charged
> once per walk and not once per operation: one build is refused above 4,095
> bindings, and a directory whose effective subtree exceeds the ceiling can
> never be rebound (`MAXIMUM_WALK_ENTRIES`) |"

Both consequences ✓, the per-walk charging ✓, the constant named ✓.

## 4. The reproducers (re-run, both pass)

`core/crates/layerfs-content/tests/filesystem_bounds.rs:743`
`the_cycle_check_work_limit_is_reachable_and_reported` — the build side:

- L787: `build_wide(limit - 8, scope).expect("a directory just under the limit")` (4,088 accepted)
- L788-792: asserts `entries_examined > 0` — the walk's entries are charged to the operation
- L793-805: `build_wide(limit + 512, ...)` (4,608) →
  `Err(ContentError::InvalidRecord("cycle check work limit"))`

`core/crates/layerfs-content/tests/filesystem_bounds.rs:819`
`a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound` — the
rebind side, doc comment at L809-817 headed "R2-F7":

- L843: first operation binds `limit - 8` (4,088) files — accepted, `entries_examined <= limit` asserted (L862-866)
- L887-889: second operation adds 32 more — accepted (`.expect("growing a large directory does not walk it")`), so the 4,120-entry directory exists
- L894-908: the rename (`"d"` → `"moved"`) →
  `Err(ContentError::InvalidRecord("cycle check work limit"))` — "refused by that ceiling, not reported as a cycle"

## 5. Independent boundary reproduction (public API, `/tmp/walk-boundary`)

| Scenario | Result |
| --- | --- |
| flat build, N files directly under root (N bindings) | 4,094/4,095/**4,096 → Ok** (`entries_examined` = N); **4,097/4,098 → Err `cycle check work limit`** |
| nested build, root binds "d", "d" binds N files (N+1 bindings) | **4,096 total bindings → Ok(4096)**; 4,097 total → Err `cycle check work limit` |
| rename "d"→"e" on a built base (root binds only "d") | subtree 4,090…4,095 → Ok, `entries_examined` 8,181…8,191 — **exactly the round-2 review's P2 numbers** (8,189/8,191, review L275-277) |
| grow then rename | subtree 4,095 → Ok; **subtree 4,096 → REFUSED** `cycle check work limit`; subtree 4,120 → REFUSED (matches the product test's step three) |

## 6. Erratum: both declared boundary figures are off by one

Reproduced, not inferred — this is the one material discrepancy:

1. **Build consequence.** The enforced rule (validate.rs:565-615) is: the build's
   single reachability walk charges **exactly one unit per stated binding**
   (`visited` incremented at L601, refused at L603-604 only when
   `visited > 4_096`). So a build stating **4,096 bindings is ACCEPTED**
   (§5, both shapes) and **4,097 is the first refused**. The declared "refused
   above 4,095 bindings" (`limits.rs:63`, §6 row) is one binding conservative;
   the claim's "(4,096 triggers)" is true only under the round-2 review's
   counting — files *inside* the built directory of a two-directory tree,
   excluding the directory's own binding edge (review
   `stages-1-5-review-20260917T230700Z.md` L275-277: "4,095 OK / 4,096 BUILD
   REFUSED" = 4,096/4,097 total bindings). Under the natural reading of
   "bindings" (stated parent→child pairs), 4,097 triggers.
2. **Rebind consequence.** In the minimal shape (root binds only the renamed
   directory) the rename is first refused when the subtree reaches **4,096 —
   equal to, not exceeding, the ceiling** (§5), because the whole-base-tree
   walk (`check_parent_aliases`, validate.rs:279-348) charges 1 + 4,096 =
   4,097 > 4,096 *before* the rebind walk (which would charge 4,096 and allow
   it). "Exceeds the ceiling" is a true sufficient condition, not the tight
   boundary, and at the boundary the refusing walk is the base-tree walk, not
   "the walk of the directory being rebound" (`limits.rs:68`). Both walks are
   per-walk-charged with the same ceiling, so the declared *per-walk* scope
   still holds.

Neither point un-declares the ceiling or removes the reproducer; both warrant a
one-word follow-up correction (4,095 → 4,096 in `limits.rs:63` and the §6 row,
or rephrasing to "a build's single reachability walk charges every stated
binding; builds above 4,096 bindings are refused").

## 7. Per-walk charging confirmed in code

- **Build**: one `visited` per build walk — validate.rs:591 (`let mut visited = 0_usize;`) initialized once before the single whole-tree walk seeded from the root (L592), checked at L603. `check_effective_cycles` delegates builds to it (L477-479).
- **Update/rebind**: `visited` is **reset to 0 inside the per-binding loop** — validate.rs:481 (`for update in checked.input.directories` / L482 `for (_, binding) in &update.changes`) → L506-508 `let mut pending = vec![(seed, *child)]; let mut visited = 0_usize;` — each rebound directory's subtree walk gets its own 4,096 allowance (checks at L603, L663 via `effective_entries` L638-664).
- **Base-tree walk**: `check_parent_aliases` has its own counter (L279) and checks (L302, L319); its doc (L274-276) states "the walk covers the base tree once, bounded by the same entry ceiling as every other whole-tree check".

This is per **walk**, not per operation — exactly what the §6 row and
`limits.rs:56-59` state. The report's charging statement is honest.

## 8. Declared, not re-charged — which option was chosen

The handoff allowed either (`stage-5-terminal-handoff-20260917.md:157`:
"either the ceiling is declared with both consequences (<= 4,095 bindings per
build; an oversize directory cannot be rebound) or the charge becomes
once-per-operation and the new figure is declared"; WP-E at L252-256 spells out
the re-charging alternative as hoisting the walk counter to the operation).

**Declared was chosen; per-walk charging was retained.** Evidence:

- Round-3 commit `2fe2a4642` message: "The whole-tree walk ceiling is declared once as `limits::MAXIMUM_WALK_ENTRIES` with both consequences - one build is refused above 4,095 bindings, and a directory whose subtree exceeds the ceiling can never be rebound - and a new case grows a directory past the ceiling and shows the rename refused (R2-F7)."
- `stage-5-report.md` §14 (L587): "`limits::MAXIMUM_WALK_ENTRIES` declared with both consequences; a case grows a directory past it and shows the rename refused | `walk-ceiling.log`".
- The code: the per-binding `visited` reset (validate.rs:507) survives; `git diff f288d2af7..HEAD -- .../validate.rs` shows the enforcement is byte-identical to the round-2 reviewed tree apart from the doc comment and the alias — the review's S-4 "hoist `visited` to the operation" alternative (review L1113) was **not** taken.
- The **round-4 README** (`stage-5-terminal-20260918T120000Z/README.md`) makes **no R2-F7 claim** — its rows are R2-F4, R2-F8/N-6, R2-F10/N-7, R2-F11/N-8, R2-F15, R2-F23/N-17, R2-F24, N-13, N-14, TR-5, F27. R2-F7's receipts are round-3: README at `stage-5-terminal-20260918T020000Z/README.md:24` and `walk-ceiling.log` (filtered reproducer run, 1 passed, EXIT=0).

## 9. UNVERIFIED

- The full `--workspace` suite, clippy, fmt and `check_product_boundary.py` were not run (out of this row's scope; the two named suites cover both reproducers).
- The round-3 `s5check` and round-4 `s5term` diagnostics clients were not re-run; my `/tmp` probe reaches the same public entry points and reproduces the review's P2 totals exactly, which is a stronger independent check.
- Rename boundary in shapes where the base tree holds directories *besides* the renamed one: the rule (base-tree walk charges the whole base once; refusal when total base edges + batch restatements exceed 4,096) is established by code reading (validate.rs:279-348) plus the minimal-shape probe, not by an exhaustive shape sweep.
- Whether the parent's stated full hash resolves: it does not (exit 128); verification ran on HEAD `99743b2cff...`, matching the stated 9-character prefix.
- No fix for the §6 erratum was applied (read-only constraint; this file is the only repository write I made).

## 10. RE-VERIFICATION (post-erratum), 2026-09-18, tree `3ecb952c8`

The §6 erratum was remediated in commit `3ecb952c8ed530706700e09647f42ee51bf09f98`
("fix(stage5): remedy every round-4 verification finding, with receipts"), which
names this verification. Re-verified read-only at HEAD; every correction checked
against the enforced behavior re-reproduced below.

| Command | Exit | Result |
| --- | --- | --- |
| `git log -1 --format='%H %s'` | 0 | `3ecb952c8ed530706700e09647f42ee51bf09f98 fix(stage5): remedy every round-4 verification finding, with receipts` |
| `git status --porcelain` | 0 | one entry: ` M core/crates/layerfs-storage/src/encoding/codec.rs` — another row's pre-existing modification, not this row's files; this evidence file is committed and unmodified |
| `git diff --stat 99743b2cf..3ecb952c8 -- validate.rs limits.rs filesystem_bounds.rs filesystem_topology.rs` | 0 | **only `limits.rs` changed** (26 insertions / 12 deletions) — enforcement (`validate.rs`) and both reproducer tests are byte-identical to the tree first verified |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_bounds --locked` | 0 | **10 passed / 0 failed** (unchanged, both reproducers included) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_topology --locked` | 0 | **17 passed / 0 failed** (unchanged) |
| `/tmp/walk-boundary` probe re-run against HEAD (`cargo +1.85.1 run --quiet --offline`) | 0 | identical tight figures: flat 4,096 → Ok(4096), 4,097 → Err `cycle check work limit`; nested 4,096 total bindings → Ok, 4,097 → Err; rename subtree 4,095 → Ok (8,191), **4,096 → REFUSED**, 4,120 → REFUSED |

Corrections checked, each against the reproduced behavior:

1. **`limits.rs:59-86`** ✓ — the build consequence now reads "one `build_filesystem`
   call is refused above 4,096 bindings: a build that states exactly 4,096 is
   accepted, 4,097 is the first refusal" (L68-69) — matches the probe exactly.
   The rebind consequence now reads "an existing directory whose effective
   subtree **reaches** the ceiling can never be renamed or relocated … because
   the walk of the base tree charges the rest of the tree beside the rebound
   directory first" (L73-78) — matches: subtree 4,096 (= the ceiling) is the
   first refused rename, and the refusing walk is the base-tree walk
   (`check_parent_aliases`), as reproduced. The counting-difference paragraph
   (L80-85) explains the review's file-count vs stated-bindings figures and
   credits the round-4 verification probe.
2. **`stage-5-report.md` §6 row (L425)** ✓ — "a build stating 4,096 bindings is
   accepted and 4,097 is the first refused, and a directory whose effective
   subtree reaches the ceiling can never be rebound, because the base-tree walk
   charges the rest of the tree beside it first", with the correction pointer
   "(figures corrected 2026-09-18 from the review's file-count phrasing - see
   §13.2)" and the per-walk charging statement retained.
3. **`stage-5-report.md` §13, corrections item 2 (the §13.2 the row points to)** ✓ —
   dated "*Erratum corrected 2026-09-18:*" note: identifies the original figures
   as "the round-2 review's own probe outputs, counted in files inside the built
   directory, which excludes the directory's own binding edge", states the tight
   figures in the bindings the walk charges, attributes the rebind boundary to
   the base-tree walk charging the rest of the tree first, cites
   `verify-R2-F7.md`, and notes the ceiling and both consequences are unchanged.
4. **`filesystem-tree.md` §9 declaration (L463-466)** ✓ — "one build is still
   refused above 4,096 bindings by the whole-tree walk ceiling".
5. **Reproducers unchanged and passing** ✓ — `git diff` shows both test files
   untouched; suites re-run 10/10 and 17/17 (exit 0).

**FINAL VERDICT for R2-F7: PASS** — the ceiling is declared once
(`limits::MAXIMUM_WALK_ENTRIES = 4_096`, aliased by `validate.rs`) with both
consequences now stating the enforced tight figures, the per-walk charging is
enforced in `validate.rs` and stated honestly in the §6 row, both reproducers
exist and pass, and the declare-not-recharge option was chosen. The boundary
erratum recorded in §6 of this file is fully remediated; every declared figure
now matches behavior reproduced through the public API.

Remaining unverified (unchanged in scope): the full `--workspace` suite, clippy,
fmt and the boundary guard were not run; the `codec.rs` working-tree modification
belongs to another row and was not examined.
