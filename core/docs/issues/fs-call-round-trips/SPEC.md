# fs call round trips (#818): spec, rollout and verification

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.
> Source pinned at `layerfs_v2_0_0` `056e920166` (code identical at `c9dfd0eba5`).
> Every after-value below is an **estimate** until a phase measures it.
> Detailed track specs: [reads](reads.md), [history](history.md), [writes](writes.md),
> [init](init.md), [verification and benchmark](verification.md).
> Baseline receipts: [receipts/056e920166/](receipts/056e920166/); probe source:
> [receipts/fs_call_round_trips.rs](receipts/fs_call_round_trips.rs).
> Issue: [Ephemeral-AI-Lab/layerfs#818](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818)
> (measurements posted in its comments 7203068532 and 7203237505).

## 1. Summary

#815/#816 cut round trips on Commit, Save, mount and unmount. The SDK file calls
(`fs.*`) and `project.init` were not covered. On PostgreSQL they pay:

- **a fixed ~12 history trips per call**: project, Branch and Commit rows read
  several times, serial, markers, recorded Branch name;
- **~7 trips per path component** for a duplicate hidden-path walk that runs
  even when the project has no hidden paths;
- **no reuse inside a call**: one `Cached` per step, not per call;
- **history calls (`blame`, `revision`, `diff_files`) repeat all of that per
  version**: ~23 trips per version for blame, ~40 for revision;
- **two per-file loops in Init**: a `logical_size` walk once past 4,096 files,
  and per-child placement in directories of 512 or more entries.

At +10 ms per trip, `fs.stat` at depth 3 takes 628 ms, `fs.blame` over 100
versions 31 s, `fs.revision` 51 s, and Init of 10k files in one directory an
estimated 386 s. SQLite runs the same calls in 1 ms to 0.7 s.

The fix has four tracks:

- **R** (reads): one walk, one context statement and one cache per call.
- **H** (history): carry rows forward, read the first-parent chain in windows,
  and walk all versions' paths level by level.
- **W** (writes): drop the eager hidden-path check, bind by name, plan parent
  reads.
- **I** (Init): take sizes from the scan, and place children in windows, on
  both backends.

None of it changes the Store format. Round trips then depend on path depth and
window count, not on history length, file count or hidden-path walks. Section 6
gives the rollout: four lanes run in parallel.

## 2. Baseline (measured)

Setup:
- One sample per case, test profile.
- Calls go through the SDK's own route (`open_store_observed` → `StoreService` →
  `methods::call`, `CALLS=64`, 64 KiB object cache).
- PostgreSQL 17.11 runs in a local container behind Docker's port forwarder:
  `SELECT 1` 0.34 ms.
- The +10 ms arm adds `tc netem` inside the container's network namespace:
  `SELECT 1` 11.0–11.9 ms.
- Tree: 10 × 100 files, a depth-7 file, a 2,000-line file, and 100 Commits on
  `work`.
- Trip counts were identical in the two PostgreSQL arms.
- For SQLite the count is client statements.

| Case | SQLite ms (stmts) | PG trips | PG local ms | PG +10 ms |
|---|---:|---:|---:|---:|
| `fs.stat /` | 0.8 (39) | 18 | 12.7 | 214 ms |
| `fs.stat` depth 3 / depth 7 | 2.7 (95) / 5.3 (152) | 50 / 73 | 37 / 56 | 628 / 918 ms |
| `fs.read` small, depth 3 | 2.7 (83) | 38 | 34 | 471 ms |
| `fs.list` 100 entries | 2.5 (69) | 33 | 29 | 393 ms |
| `fs.grep` 1k-file tree | 13.0 (131) | 78 | 79 | 999 ms |
| `fs.diff_files` | 5.2 (198) | 88 | 64 | 1.15 s |
| `fs.blame` 10 / 100 versions | 17.7 (714) / 173 (7,575) | 267 / 2,325 | 203 / 1,829 | 3.4 / 31.2 s |
| `fs.revision` 100 versions | 286 (12,658) | 3,890 | 3,263 | 50.9 s |
| `fs.write` overwrite / new file | 3.9 (54) / 5.0 (77) | 24 / 66 | 22 / 49 | 298 / 809 ms |
| `fs.rm` 100-file dir | 18.2 (179) | 151 | 106 | 1.87 s |
| Init 1k, 10 dirs / one dir | 55 (308) / 65 (2,284) | 229 / 2,246 | 429 / 1,367 | 3.1 / 27.0 s |
| Init 10k, 100 dirs / one dir | 613 (12,164) / 717 (31,881) | 10,955 / 31,039 | 13.4 / 24.0 s | ≈141 / ≈386 s ¹ |

¹ Not run: over the 2-minute command limit. Projected as local + trips × 11.65 ms.

Each added trip cost 11.65 ms at +10 ms (median across cases; range
10.95–12.63). Server work barely registers, so the count is the cost. Every
row of all 15 methods is in the receipts.

## 3. Goal

**Primary (PostgreSQL round trips, cold, one call, measured at one final source identity):**

| Class | Target | Baseline |
|---|---|---|
| Fixed per-call history cost | ≤ 2 trips (1 context statement + root object when cold) | 12 |
| Single-path reads (`stat`, `read`, `list`, `glob`, one-dir `grep`) | ≤ 4 + 5·d (d = components below the root) | 18–73 |
| `grep` over 1k files | ≤ 64; ≤ 30 only if R6 is approved | 78 |
| `blame`/`revision` over V versions | ≤ F + d + 4 + 10·windows(V), windows = 16, 32, 64, then 128 each: ≤ 18 at V = 10 and ≤ 38 at V = 100 for a depth-2 file | 267 / 2,325 / 3,890 |
| `diff_files` | ≤ 15 | 88 |
| Single-path writes | ≤ the section 4 value per case (overwrite ≤ 12, new file ≤ 40, mkdir/cp/mv/rm ≤ 26), every remaining trip attributed by W0. The 5 + d floor is a reference, not a target | 23–66 |
| Recursive `rm` of 100 files | ≤ 50 | 151 |
| Init | ≤ 750 per 10k files nested or flat after M1 (I1, I2); ≤ 250 after M3 (I3), confirmed or revised from M1's per-phase counts because I3's savings are inferred | 10,955 / 31,039 |
| Warm repeat call (only if R7 is approved) | ≤ 2 | — |

**Latency:** at +10 ms, added time ≤ trips × calibrated delay (±10%) for every
row. Every row then fits the 15 s command budget, so all rows are measured, not
projected.

**SQLite:** no call gets slower. Statement counts fall only through dedupe
fixes (same answers, less work) and I2b, each listed with its new pin. Batched
reads stay PostgreSQL-only, gated on `batches_locations` / `batched_chain_reads`.

**Guardrails (all phases):**
- No Store format or schema change (section 5).
- Byte-identical roots and Save outcomes.
- Same answers and the same refusal order.
- One attempt, no retry or replay. A lost reply quarantines, as today.
- Memory bounded by named per-call or per-window bounds, never by Store,
  tree or history size.
- No tuned constants to pass a gate.

## 4. Expected improvement (three arms)

**Method:**
- Trips "after" come from each track's mechanism arithmetic (see the track
  specs).
- SQLite "after" scales SQLite time by its dedupe statement drop.
- PG local after = SQLite after + trips after × **that case's** measured local
  cost per trip, (PG local − SQLite) / trips: 0.46–0.85 ms for `fs.*` and
  0.58–1.64 ms for Init.
- PG +10 ms after = local after + trips after × that case's measured +10 ms
  cost per trip (10.95–12.63 ms; 11.65 where not run).
- Rows are cold (a new Store open per case, no cross-call cache) and assume
  all required phases (M1–M3). The `grep` +R6 row adds optional R6.
- Init's SQLite times are init.md's estimates, not statement-scaled: I1 and
  I2b remove reads and placement statements whose cost is not proportional.
  I1's cut is taken as the same ~120 ms in both 10k shapes, and I2b's as ≤
  ~100 ms.
- The track specs' own tables used the 0.69 ms median per trip. Where they
  differ from this table, this table governs.
- Write rows include W3's −1 only for a second write to the same Branch in
  one process. Cold rows are 1 higher, as shown here.
- "PG gain" is now ÷ after. "PG ÷ SQLite after" compares PostgreSQL's projected time with SQLite's projected time for the same call.

**Reads**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| `fs.stat /` | 0.8 → 0.6 | 39 → ~26 | 18 → 4 | 12.7 → ~3.2 ms | 214 ms → ~48 ms | 4.0× / 4.5× | 5.3× / 80× |
| `fs.stat` depth 3 | 2.7 → 1.4 | 95 → ~49 | 50 → 18 | 37 → ~14 ms | 628 ms → ~227 ms | 2.6× / 2.8× | 10× / 162× |
| `fs.stat` depth 7 ² | 5.3 → 2.5 | 152 → ~70 | 73 → ~30 | 56 → ~23 ms | 918 ms → ~377 ms | 2.4× / 2.4× | 9.2× / 151× |
| `fs.read` small, depth 3 | 2.7 → 1 | 83 → ~31 | 38 → 6 | 34 → ~6 ms | 471 ms → ~75 ms | 5.7× / 6.3× | 6.0× / 75× |
| `fs.read` 2,000 lines | 2.1 → 1 | 68 → ~32 | 32 → 7 | 23 → ~6 ms | 396 ms → ~87 ms | 3.8× / 4.6× | 6.0× / 87× |
| `fs.read` lines 100–200 | 1.8 → 0.7 | 66 → ~28 | 30 → 5 | 20 → ~4 ms | 358 ms → ~60 ms | 5.0× / 6.0× | 5.7× / 86× |
| `fs.list /` | 0.8 → 0.5 | 39 → ~26 | 18 → 4 | 13.8 → ~3.4 ms | 211 ms → ~47 ms | 4.1× / 4.5× | 6.8× / 94× |
| `fs.list` 100 entries | 2.5 → 1.2 | 69 → ~33 | 33 → 8 | 29 → ~7.5 ms | 393 ms → ~96 ms | 3.9× / 4.1× | 6.2× / 80× |
| `fs.glob` 100-entry dir | 2 → 0.9 | 66 → ~29 | 30 → 5 | 23 → ~4.4 ms | 380 ms → ~64 ms | 5.2× / 5.9× | 4.9× / 71× |
| `fs.glob` whole tree | 1.1 → 0.8 | 44 → ~33 | 23 → 9 | 17 → ~7 ms | 294 ms → ~115 ms | 2.4× / 2.6× | 8.8× / 144× |
| `fs.grep` one dir | 1.4 → 0.8 | 49 → ~28 | 23 → 5 | 17 → ~4.3 ms | 289 ms → ~63 ms | 4.0× / 4.6× | 5.4× / 79× |
| `fs.grep` 1k files | 13 → 12 | 131 → ~121 | 78 → 64 | 79 → ~66 ms | 999 ms → ~821 ms | 1.2× / 1.2× | 5.5× / 68× |
| `fs.grep` 1k files, with R6 ³ | 13 → 12 | 131 → ~121 | 78 → ~29 | 79 → ~37 ms | 999 ms → ~379 ms | 2.1× / 2.6× | 3.1× / 32× |

**History**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| `fs.diff_files` | 5.2 → 3.1 | 198 → ~119 | 88 → ~12 | 64 → ~11 ms | 1.15 s → ~159 ms | 5.8× / 7.2× | 3.5× / 51× |
| `fs.blame` 10 versions | 17.7 → 8.8 | 714 → ~360 | 267 → ~17 | 203 → ~21 ms | 3.4 s → ~225 ms | 9.7× / 15× | 2.4× / 26× |
| `fs.blame` 100 versions | 173 → 86 | 7,575 → ~3,800 | 2,325 → ~29 | 1.8 s → ~107 ms | 31.2 s → ~473 ms | 17× / 66× | 1.2× / 5.5× |
| `fs.revision` 100 versions | 286 → 129 | 12,658 → ~5,700 | 3,890 → ~30 | 3.3 s → ~152 ms | 50.9 s → ~519 ms | 21× / 98× | 1.2× / 4.0× |

**Writes**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| `fs.write` overwrite | 3.9 → 3.4 | 54 → ~47 | 24 → ~12 | 22 → ~13 ms | 298 ms → ~150 ms | 1.7× / 2.0× | 3.8× / 44× |
| `fs.edit` | 4.3 → 3.8 | 52 → ~45 | 23 → ~12 | 23 → ~13 ms | 285 ms → ~150 ms | 1.8× / 1.9× | 3.4× / 39× |
| `fs.write` new file (first per open) | 5 → 4.5 | 77 → ~70 | 66 → ~40 | 49 → ~31 ms | 809 ms → ~492 ms | 1.6× / 1.6× | 6.9× / 109× |
| `fs.mkdir` | 4.5 → 4.1 | 69 → ~62 | 45 → ~26 | 30 → ~19 ms | 549 ms → ~319 ms | 1.6× / 1.7× | 4.6× / 78× |
| `fs.cp` | 5 → 4.4 | 64 → ~57 | 44 → ~25 | 29 → ~18 ms | 548 ms → ~313 ms | 1.6× / 1.8× | 4.1× / 71× |
| `fs.mv` | 4.4 → 3.9 | 60 → ~53 | 40 → ~23 | 23 → ~15 ms | 487 ms → ~282 ms | 1.5× / 1.7× | 3.8× / 72× |
| `fs.rm` one file | 4.6 → 4.1 | 58 → ~51 | 37 → ~21 | 23 → ~14 ms | 453 ms → ~259 ms | 1.6× / 1.7× | 3.4× / 63× |
| `fs.apply` 10 writes | 7.9 → 7.2 | 74 → ~67 | 46 → ~27 | 38 → ~25 ms | 562 ms → ~333 ms | 1.5× / 1.7× | 3.5× / 46× |
| `fs.rm` 100-file dir | 18.2 → 8.1 | 179 → ~80 | 151 → ~36 | 106 → ~29 ms | 1.87 s → ~448 ms | 3.7× / 4.2× | 3.6× / 55× |

**Init**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| 1k files, 10 dirs | 55 → 55 | 308 → ~290 | 229 → ~140 | 429 → ~284 ms | 3.1 s → ~1.9 s | 1.5× / 1.6× | 5.2× / 35× |
| 1k files, one dir | 65 → 55 | 2,284 → ~330 | 2,246 → ~150 | 1.4 s → ~142 ms | 27.0 s → ~1.9 s | 9.6× / 14× | 2.6× / 35× |
| 10k files, 100 dirs | 613 → 490 | 12,164 → ~2,000 | 10,955 → ~210 | 13.4 s → ~740 ms | ≈141 s ⁴ → ~3.2 s | 18× / 44× | 1.5× / 6.5× |
| 10k files, one dir | 717 → 500 | 31,881 → ~2,330 | 31,039 → ~240 | 24.0 s → ~680 ms | ≈386 s ⁴ → ~3.5 s | 35× / 110× | 1.4× / 7.0× |

| Init 100k files, 1,000 dirs ⁵ | PG trips ~110,000 → ~2,100; PG +10 ms ~22 min → ~30 s |
|---|---|

² Depth 7 uses a midpoint. Applying R3's −7 per component at every depth gives 13,
below depth 3's 18, which cannot hold. Both walks together add only 5.75 trips per
component from depth 3 to depth 7 ((73 − 50) / 4). So the one walk left costs 0–5.75
per deeper component, and depth 7 lands between 18 and 41. The midpoint, about 30, is
used here; it is within the ≤ 4 + 5·d target (39).

³ R6 (byte-bounded `grep`/`glob` windows) is optional: decision D3.

⁴ Not run: over the 2-minute command limit. Projected as local + trips × 11.65 ms.

⁵ Extrapolated linearly from the 10k rows; not measured.

**What it means for SQLite.**
- SQLite statements fall only where duplicate work is removed (R2, R3, H1–H3,
  W1–W3, W5, I1) and through I2b. Batched statements stay PostgreSQL only.
- The biggest SQLite drops are the history calls (about half the statements
  and time), recursive `rm` (about −55%) and Init.
- Single-path writes barely move on SQLite.
- After the change, PostgreSQL remains slower than SQLite. That remaining gap
  is the round-trip floor, not repeated per-call work:
  - **local:** 1.2–10× (most calls 3–7×), down from 5–33×;
  - **at +10 ms:** 4–160× (history and Init are lowest, about 4–35×), down
    from 57–529×.

**Warm, with optional R7:** a repeated read call drops to 1–2 trips: about
13–26 ms at +10 ms, and 1–2 ms locally.

**What does not change much:**
- **Single-path writes stay about 2× better.** They already use the fused
  one-message Commit (#803). The floor is 5 + d trips: snapshot, admission,
  one planned demand per parent level, membership, the fused message, close.
  Going lower changes custody or messages, and is not proposed (owner
  decision D9).
- **`grep` over 1k files is bound by its tree windows**, not by per-call cost.
  R6 is the only lever.

## 5. Store format and core impact

**Store format: none.**
- No table, column, index or stored byte changes. No migration or upgrade step.
- New SQL is query text only:
  - `branch_branch_by_name.sql`, `paths_markers.sql` (R1);
  - `read_context_{named,node}.sql` (R5);
  - `lineage_chain_window.sql` (H4), which uses the existing `commit_runs`
    (run, depth) index;
  - `allocation_reserve.sql` (W4), the serial reservation as one message, with
    scope authority and `Capacity` still checked on the server;
  - the windowed placement statements (I2a inline; I2b
    `sql/sqlite/acquisition/queries/place_children.sql`).
- A per-path change index would make blame/revision cost O(changed versions).
  It needs a format change and is **not proposed**.

**Core crates: small and additive.** New methods with default
implementations, so existing callers and other providers behave the same.
Layering stays `core < engine < api`.

| Crate | Change | Phase |
|---|---|---|
| `layerfs-history` | `HistoryCatalog::read_context` and `first_parent_window`. Each default body is also a public free function: today's calls in today's order, keeping today's `Unsupported` → serial 0 / no markers mapping. New `ChainRow` = full `CommitRecord` + `history_serial` (`CommitRecord` lacks the serial; revision items still need `meta.via`, `merged_from`, statistics and second parent) | R5, H4 |
| `layerfs-content` | new `compare/across.rs`: one path resolved across many versions' roots, level by level; a **new** multi-table lookup beside `lookup_many`, whose signature stays (W5 wraps it) | H5 |
| `layerfs-storage` | `lengths_with(&mut ReadCache, ids)` beside `read_with` (only if the existing reader cannot cover R4); a separate publish-flight flag for Init Saves, not `captured_output` (`save/state.rs`, `save/operation.rs`, `save/publication.rs`) | R4 (cond.), I3c |

**SQLite keeps today's path through the overrides, not through the trait default.**
Persistence has one `impl HistoryCatalog for HistoryProvider` for both engines
(`history/catalog.rs:6`). An override there replaces SQLite's path too, and Rust cannot
call a trait's default body from an override. So R5 and H4 override in persistence and
branch on `Engine`: PostgreSQL sends the new statement, and SQLite calls the free function.

Declined core change: carrying `logical` in `BranchSnapshot` (W3 option b).
W3 uses a Store-level slot instead.

## 6. Rollout: Phase 0, four parallel lanes, four milestones

### 6.1 Phase 0: shared seams and baseline pins (one session, serial, first)

Phase 0 lands before the lanes fork, so no two lanes edit a shared file.

| Item | Content |
|---|---|
| Probe | `fs_call_round_trips.rs` into `engine/crates/layerfs-store-service/tests/` as a measurement probe (not a gate) |
| Pin files (baseline counts) | `sdk_read_round_trips.rs`, `sdk_history_round_trips.rs`, `sdk_write_round_trips.rs`, `init_round_trips.rs`. Each lane later owns exactly one |
| Server counter | Top-level-only `pg_stat_statements` snapshot helper in the test support module |
| Seeded equivalence | `sdk_round_trip_equivalence_seeded.rs`: all 15 `fs.*` methods + Init, refusals included, SQLite vs PostgreSQL |
| W0 attribution | Per-phase `SqlWork` for write calls (`telemetry::phase_sql`), cold and warm; re-estimates W4/W6/W7 |
| Seam | `Visibility::from_markers` (read/visibility.rs), so W1 needs no edit in the R lane's file |
| Release baseline | Re-take the Init rows in a release build (verification rule); keep the test-profile rows as they are |

Exit:
- The pins equal the baseline counts on both backends.
- The equivalence driver is green.
- No production behaviour changes (the seam has no caller yet).

### 6.2 Lanes

Each lane gets:
- its own worktree (`tools/worktree.py`), with a private Cargo target;
- its own PostgreSQL database (one test binary per database at a time);
- its own pin file and fault-sweep files.

Lanes rebase on `layerfs_v2_0_0` and merge independently. **File ownership is
exclusive.** A lane that needs another lane's file waits for that lane's merge.

| Lane | Items, in order | Owns (exclusive) | Waits for |
|---|---|---|---|
| **RH**: reads then history (critical path) | R1 → R2 → R3 → R4 → R5 → H1 → H2 → H3 → H4 → H5 wiring → H6 → (R6, R7, H7 optional) | store-service `methods/`, `operator/`, `history/`, `read/` (incl. `visibility.rs` after P0; `snapshot::entry` keeps its signature, which `write/files.rs` and `operator/files.rs` call), `store/read_handle.rs`, `store/ports.rs`; persistence `history/catalog.rs`, `history/branch.rs`, `history/lineage.rs` (+ cond. `chain.rs`, `mod.rs`), `backend/postgresql/history/statements.rs`, `sql/postgresql/queries/history/` (all until its H4 merge); `layerfs-history` contract; `layerfs-storage` `read/provider.rs` | Lane C's merge before the H5 wiring; lane W's merge before R7 (`store_service.rs`) |
| **C**: content primitive | H5 core: `compare/across.rs`, a new multi-table lookup (existing `lookup_many` unchanged), unit tests on synthetic roots (no wiring) | `layerfs-content` `compare/`, `filesystem/inode/read.rs` | nothing |
| **W**: writes | W1 → W2 → W3 → W5 → W6 → W4 → W7 (cond., per W0) | store-service `write/`, `removal.rs`, `store_service.rs`, `store/bind.rs`, `store/open.rs`, `store/statistics.rs`, `store/commit.rs` (919 physical lines: no growth; new code goes to `write/plan.rs`), `store/pipeline.rs`, `bootstrap.rs`; persistence `history/allocation.rs`, `backend/postgresql/history/mount.rs`, `backend/postgresql/storage/provider.rs`, `backend/postgresql/session/`; after RH's H4 merge also persistence `history/catalog.rs` (W4 dispatch) and `sql/postgresql/queries/history/allocation_reserve.sql`; `layerfs-workspace` `base/view.rs`, `workspace/view.rs` | P0; RH's H4 merge before W4. R1 and W1/W2 remove overlapping trips (−9 together on an overwrite). Each lane predicts against the tip it rebased on: W2 saves 4 before R1 merges and 2 after; W1 saves 5 before and 3 after |
| **I**: Init | I1 → I2a → I2b → I3 (a, b, c, d per decision) | `layerfs-project` `import/`; persistence `backend/postgresql/acquisition/`, `backend/sqlite/acquisition/`, `sql/sqlite/acquisition/`, `backend/postgresql/objects/publish_flight.rs`; `layerfs-storage` `save/publication.rs`, `save/state.rs`, `save/operation.rs` | nothing; I3c reuses the existing `publish_captured` entry if it keeps no payloads, else waits for lane W's merge of PG `storage/provider.rs` |

Ownership passes once, at a merge: persistence `history/catalog.rs` and
`sql/postgresql/queries/history/` go from RH to W after RH's H4 merge, so W4 runs
in M3. The RH lane is the long pole (11 items). Lane C takes H5's core work off it,
and W and I are fully independent. If RH needs splitting, hand off at the R5
merge: a new H session starts from that merge and owns the same files.

### 6.3 Milestones (cross-lane merge points)

A milestone is reached when every lane's items for it are merged. After each
milestone, the full three-arm benchmark runs once on the merged tip, with
receipts under `receipts/M<n>/`.

| Milestone | Items | PG trips after (cold) |
|---|---|---|
| **M1: dedupe, both backends** | R1, R2, R3; W1, W2, W3, W5; I1, I2a, I2b | stat `/` 8, d3 22, d7 ~34; read small 10; list100 12; grep1k 68; overwrite 15, new file 57, rm dir ~42 (cold); Init 1k flat ~226, 10k nested ~606, 10k flat ~706 |
| **M2: one statement / one cache per call** | R4, R5; H1, H2, H3; W6; Lane C merged | stat `/` 4, d3 18, d7 ~30; read small 6; list100 8; grep1k 64; blame h100 ~900, revision ~1,000, diff ~55; overwrite ~12, new file ~55 |
| **M3: history, Init and create batching** | H4, H5, H6; I3; W4, then W7 (cond.) | blame h10 ~17, h100 ~29; revision ~30; diff ~12; new file ~40; Init 1k nested ~140, 10k nested ~210, 10k flat ~240 |
| **M4: optional** | R6, R7, H7 (each by owner decision) | grep1k ~29; warm reads 1–2 |

M1 gives most of the read and Init gain and carries the least risk: it removes
duplicate work and adds no new messages, except I2a/I2b and R1's two qualified
statements. M3 carries the history gain and the newest messages, so it carries
the most fault-sweep work.

### 6.4 Per-item gate

Every item passes the [per-phase gate](verification.md#per-phase-gate) before
its lane moves on:
- prediction written first;
- build with `--no-run`;
- one bounded (≤120 s) run per test binary;
- pins by role and cache state;
- byte identity, same answers and refusal order;
- drop and kill sweeps for each new or changed message;
- the +10 ms latency arm;
- memory before and after;
- held-out shapes;
- independent review;
- docs;
- `Production LOC: <before> -> <after> (delta …)` in the commit.

A miss of more than 10% or 3 trips against the prediction is explained from
per-role counts before the next item starts.

## 7. Verification and benchmark (summary)

Full rules: [verification.md](verification.md).

- **Counts first, by role, with the cache state.** Writer, control reader
  (session 0) and Workspace readers are counted apart. Cold means a new Store
  open per case; warm means a repeat in one open Store. A warm count never
  stands in for a cold cap.
- **Server cross-check:** `pg_stat_statements` top-level calls only.
  `track=all` counts nested FK trigger checks, about 30k at Init 10k.
- **Latency arm:** netem inside the PostgreSQL container only; the host network
  is never touched. Calibrate with 200 × `SELECT 1`. Pass when added time ≤
  trips × delay.
- **Byte identity and answers:**
  - same roots and Save outcomes;
  - seeded SQLite–PostgreSQL equivalence;
  - `method_refusal_order.rs` unchanged;
  - Init `logical` equals `logical_size(reader, root)`.
- **Faults:** drop and kill sweeps at every message of every new statement or
  flight. One attempt; a lost reply quarantines; the old state or the new
  state, never a partial one.
- **Memory:**
  - peak and after-call RSS per changed case;
  - a 1,000-call soak flat after warm-up;
  - large shapes (Init 100k, blame over 1,000 versions, grep over 10k files)
    show no peak growth beyond window bounds;
  - any cache reports charged bytes within its approved budget.
- **Anti-hack:**
  - a prediction before the code;
  - held-out shapes the design did not use (depth 12, 10k-entry directory,
    64 MiB files, hidden paths present, symlinks, hardlinks repeated inside a
    window, 1,000 versions, a delta-chained directory);
  - no raised caps or timeouts;
  - no weakened assertions;
  - cheap tiers proven with caches off.

**Benchmark matrix (run at P0 and after each milestone):**

| Axis | Values |
|---|---|
| Harness | The in-tree probe (`FS818_ARM` = sqlite / pg / pg10, plus a pg2 arm; `FS818_GROUP` = reads / history / writes). fs-bench-pro stays frozen. At close, the cases move to layerfs-benchmark as an `fs-file-calls` family (follow-up, not a gate) |
| Calls | All 15 `fs.*` methods; Init 1k/10k nested and flat; 100k nested on SQLite and PG local |
| Arms | SQLite Normal; PG local; PG +2 ms; PG +10 ms |
| Cache | Cold for all; warm only for R7 |
| Metrics | Trips and statements by role; server top-level calls; ms; peak and after-call RSS; 1-minute load |
| Build | Release for Init timing; test profile allowed for count-only rows, labelled |
| Samples | One per case and arm; identities pinned; fresh output paths; failures and `NOT_RUN` rows kept |
| Budget | ≤15 s per command (25 s declared exceptions); tests ≤120 s. Rows over budget at +10 ms (blame/revision h100 and Init 1k flat, measured once at baseline in a recorded over-budget diagnostic run; Init 10k, never run) run at +2 ms or on smaller shapes until M3 (D14) |

**Exit for the whole effort:**
- the section 3 targets measured at one final identity;
- every +10 ms row measured directly and within trips × delay ±10%;
- memory bounds and soak held;
- equivalence, refusal-order and sweep tests green;
- SQLite pin changes listed, each a dedupe or I2b.

## 8. Memory and caching rules

- **Per call:** one `Cached` and `ReadCache` per call (R4, H3), bounded by
  `KeptReads::ORDINARY_BYTES` (3 MiB) and freed at the call's end. The
  directory-lookup memo `(DirectoryRoot, PathName) → serial` lives only within
  the call.
- **Per window:**
  - history windows double 16 → 128 versions, with a roots demand of at most
    128 objects;
  - Init placement windows: ≤4,096 rows or 1 MiB on PostgreSQL, 32 rows on
    SQLite;
  - W6 plan: ≤128 directories / 1 MiB, `PLAN_PATHS` = 1,024;
  - R6 byte-bounded windows up to the reader's existing 4 MiB.

  None of these grows with Store size.
- **Across calls (R7, optional):**
  - only immutable, authenticated objects, keyed by project scope;
  - a fixed per-process budget, for example 8 MiB against today's 64 KiB, with
    least-recently-used eviction;
  - locators and kept pack bodies are dropped when `catalog_incarnation`
    changes;
  - **never cached across calls:** Branch heads, markers, absence, or history
    rows.
- The owner prefers not to trade RAM for speed, and machines may have 1 GB.
  Every phase therefore reports peak memory beside time, and R7 is off unless
  approved.

## 9. Decisions

**Decided (owner, 2026-11-27):**
- Take I2 on both backends: I2a PostgreSQL, I2b SQLite.
- Run the work as parallel lanes (section 6).
- No Store format change.

**Open, with recommendation, and what each blocks:**

| # | Decision | Recommendation | Blocks |
|---|---|---|---|
| D1 | Accept new SQLite statement pins for dedupe fixes (R2, R3, H1–H3, W1–W3, W5, I1) | Accept: same answers, less work | M1 merges in every lane |
| D2 | R7 cross-call cache and its budget | Defer to M4; if taken, 8 MiB fixed, LRU, PG only | R7 |
| D3 | R6 byte-bounded grep/glob windows (count cap ≤256) | Take after M3, measured on a large-file tree | R6 |
| D4 | R5 also returns the root object's locator and pack range | No for now; revisit with R7 | none |
| D5 | History window sizes 16 → 128 and the 128-object roots demand | Accept | H4 |
| D6 | H7 (attribution pages share one version table) | Only if large-file attribution is in scope | H7 |
| D7 | W3: Store slot vs `logical` in `BranchSnapshot` | Store slot (no contract change) | W3 |
| D8 | W4: prepare create statements at Store open (+1 trip per open) | Accept for persistent SDK processes; pair with #819 | W4 |
| D9 | Write floor 3 + d (pipelined snapshot+admission, or intent release inside the fused transaction) | Not proposed: changes messages or custody | none |
| D10 | New bound `PLAN_PATHS` = 1,024 for W6 | Accept | W6 |
| D11 | `READ_WINDOW_ROWS` (512) as the next Init lever past 100k | Later, after M3 | none |
| D12 | I3c: Init Saves use `publish_flight` via their own flag | Accept with full sweeps | I3c |
| D13 | I3d: prepare Init statements at open | Only if one-shot SDK processes run Init | I3d |
| D14 | Command budget: rows over 15 s at +10 ms (blame/revision h100 and Init 1k flat, measured once at baseline in a diagnostic run over budget; Init 10k, not run) run at +2 ms or on smaller shapes until M3 brings them under budget | Accept | benchmark |
| D15 | Release builds for every Init timing row; re-take the Init baseline in release at P0 | Accept | P0 |
| D16 | W1 refusal change: today a failed marker read refuses every write; after W1 it refuses only on the `NoSuchPath` path | Approve as a named order change, or keep the eager marker read when the kept markers are absent | W1 |
| D17 | I2a/I2b conflict with rows stored by earlier windows: add a `first_dependent`-style check, or approve that a later row's constraint error may replace an earlier `Changed` | Add the check (no order change) | I2a, I2b |

## 10. Risks

| Risk | Mitigation |
|---|---|
| Refusal order shifts when reads merge into one statement (R5) or bind by name (W2) | Decode in today's order. For W2, keep the project lookup whenever the `branch` argument is an error (not only for `main`), and map `BindError::NoSuchProject`/`NoSuchBranch` back to today's text (`history/lookup.rs:287-300`). `method_refusal_order.rs` unchanged; equivalence driver |
| New messages lose replies or are killed mid-flight | One fence per statement; drop/kill sweeps at every message; quarantine, no replay |
| Blame overfetches when it stops early | Doubling windows bound it to ≤2× the versions needed; measured on 10 and 100 versions |
| A set-based placement statement differs from the per-row loop (I2a/I2b) | Mirror `independent` (PG `acquisition/writes.rs:215-246`) in full: cut the window before a repeated name, a repeated position, a repeated (device, inode), or a position that does not convert to signed; check dependencies on stored rows (D17). SQLite's `UPDATE … FROM` cannot return FROM-table columns, so match returned rows by name or position. An all-hardlink directory degrades to 1-row windows: correct, and the stated bound |
| A stale cross-call cache after compaction or GC (R7) | Key locators by `catalog_incarnation`; a compaction-during-cache test; confirm Branch removal and GC bump it before kept pack bodies |
| Lane merge conflicts | Exclusive file ownership; P0 seams; RH waits for C before H5 |
| Baseline noise (test profile, Docker forwarder, host load 5–22) | Judge on counts; time as diagnostic; one sample; record load; release Init rows |
| Projection model error (server work differs per statement) | Per-case cost per trip; each milestone re-measures and records predicted vs measured |

## 11. Layout

Scope:
- Commit `056e920166`.
- Counter: `python3 tools/production_loc.py --files` (production LOC, `prod`
  column).
- Only the files these phases touch are listed, so each bracket is a partial
  subtotal.

Totals: 65 existing files with 15,148 production LOC listed; 13 new files,
4 of them conditional (added only if an existing file cannot hold the change). Tests live under each crate's `tests/` and are not
counted. New pin files are named in section 6.1 and
[verification.md](verification.md#pinned-tests).

```diff
  engine/crates/layerfs-store-service/src/            [7867]  (partial: touched files only)
    store_service.rs                                    203   (lane W: W2 private named bind; R7 budget (M4))
    bootstrap.rs                                        291   (W4 SDK prepare flight)
    removal.rs                                          400   (W1 kept-markers accessor)
    methods/                                          [341]
      args.rs                                           174   (R4 call cache; R5 context)
      file/                                           [167]
        stat.rs                                          17   (R3)
        read.rs                                          25   (R3)
        list.rs                                          31   (R3)
        glob.rs                                          47   (R3, R6)
        grep.rs                                          47   (R3, R6)
    operator/                                         [174]
      files.rs                                          174   (H1 held rows, Branch-name memo)
    history/                                          [508]
      saved.rs                                          210   (R2, H1 reference from held record)
      lookup.rs                                         298   (R2 root from record)
    read/                                             [2956]
      mod.rs                                             24   (H4 declare versions)
      snapshot.rs                                       212   (R2, R4, R5, H3)
      visibility.rs                                     219   (P0 from_markers seam; R2, R3, R5)
      paged_reads.rs                                    406   (R2 pin takes the record)
      reads.rs                                          356   (R3 entry passed in)
      cached.rs                                          88   (R4 lengths, R6, H3 directory memo)
      walk.rs                                           249   (R6)
      blame.rs                                          367   (H1 carry rows/Text; H5, H6)
      revision.rs                                       289   (H2 one walk; H5)
      file_compare.rs                                   296   (H3 one walk; H6)
      file_facts.rs                                     129   (H6 batched lengths/metadata)
      attribution.rs                                    321   (H7 cond.)
+     versions.rs                                         —   (new: H4-H6 chain windows, version table)
    store/                                            [2113]
      read_handle.rs                                    248   (R7)
      ports.rs                                          279   (R4 lengths_with; R7)
      bind.rs                                           326   (W2 pooled named variant)
      open.rs                                           323   (W3 last-published logical slot)
      statistics.rs                                      34   (W3)
      commit.rs                                         757   (W6/W7 hand reads to the Save; 919 physical lines, no growth)
      pipeline.rs                                       146   (W6/W7)
    write/                                            [881]
      files.rs                                          274   (W1 lazy check; W2; W6)
      mutation.rs                                       311   (W6 planned resolve)
      recursive.rs                                      296   (W5 page kinds)
+     plan.rs                                             —   (cond.: W6, if files.rs would pass 999 physical lines or take a second job)
```

```diff
  engine/crates/layerfs-persistence/                  [3604]  (partial: touched files only)
    sql/                                              [2]
      postgresql/                                     [2]
        queries/                                      [2]
          history/                                    [2]
            branch_snapshot_named.sql                     1   (pattern for R5)
            paths_node_markers.sql                        1   (R2 uses as is)
+           branch_branch_by_name.sql                     —   (new: R1)
+           paths_markers.sql                             —   (new: R1)
+           read_context_named.sql                        —   (new: R5)
+           read_context_node.sql                         —   (new: R5)
+           lineage_chain_window.sql                      —   (new: H4)
+           allocation_reserve.sql                        —   (new: W4 (after RH's H4 merge))
      sqlite/                                         [0]
        acquisition/                                  [0]
          queries/                                    [0]
+           place_children.sql                            —   (new: I2b 32-row bind and place)
    src/                                              [3602]
      history/                                        [1130]
        catalog.rs                                      378   (R1; R5/H4 overrides branch on Engine; W4 dispatch after H4)
        allocation.rs                                    63   (W4 reservation as one message)
        branch.rs                                       349   (R5 body beside branch_snapshot_named)
        lineage.rs                                      296   (H4 body)
        mod.rs                                           44   (cond.: declares chain.rs)
+       chain.rs                                          —   (cond.: H4, if lineage.rs takes a second job)
      backend/                                        [2472]
        postgresql/                                   [2163]
          session/                                    [340]
            prepare.rs                                   35   (W4 re-prepare after reopen)
            reopen.rs                                   305   (W4 flag, as mount_writes)
          history/                                    [371]
            statements.rs                                93   (R1, R5, H4 qualified keys)
            mount.rs                                    278   (W4 prepare list)
          storage/                                    [332]
            provider.rs                                 332   (W4)
          acquisition/                                [979]
            provider.rs                                 212   (I3 statement reads, one-flight units)
            reads.rs                                    268   (I3 sizing folded into reads)
            writes.rs                                   499   (I2a one placement statement per window)
+           placement.rs                                  —   (cond.: I2a, if writes.rs would pass 999 physical lines or take a second job)
          objects/                                    [141]
            publish_flight.rs                           141   (I3c Init flag)
        sqlite/                                       [309]
          acquisition/                                [309]
            writes.rs                                   223   (I2b windowed place_children)
            statements.rs                                86   (I2b statement key)
+           placement_windows.rs                          —   (cond.: I2b, if writes.rs would take a second job)
```

```diff
  engine/crates/layerfs-project/src/                  [554]  (partial: touched files only)
    import/                                           [554]
      files.rs                                          203   (I1 sum canonical and alias lengths)
      init.rs                                           351   (I1 drop the logical_size walk)
```

```diff
  engine/crates/layerfs-workspace/src/                [529]  (partial: touched files only)
    base/                                             [335]
      view.rs                                           335   (W5 values for a page)
    workspace/                                        [194]
      view.rs                                           194   (W5 SourceView kinds; W6 kind-only lookup)
```

```diff
  core/crates/layerfs-history/src/                    [785]  (partial: touched files only)
    contract/                                         [785]
      catalog.rs                                        407   (R5/H4 defaults as public free functions)
      records.rs                                        378   (H4 ChainRow = CommitRecord + history_serial)
```

```diff
  core/crates/layerfs-content/src/                    [878]  (partial: touched files only)
    compare/                                          [643]
      mod.rs                                             21   (H5 declare across)
      tree.rs                                           622   (H5 helpers)
+     across.rs                                           —   (new: H5 multi-root level lookup)
    filesystem/                                       [235]
      inode/                                          [235]
        read.rs                                         235   (H5 new multi-table lookup; lookup_many kept (W5))
```

```diff
  core/crates/layerfs-storage/src/                    [931]  (partial: touched files only)
    read/                                             [98]
      provider.rs                                        98   (R4 lengths_with (cond.))
    save/                                             [833]
      publication.rs                                    366   (I3c flight flag apart from captured_output)
      state.rs                                          180   (I3c flag field)
      operation.rs                                      287   (I3c setter)
```

## 12. Lane handoff prompt (template)

Each lane session starts from this text with its lane row filled in:

```text
You own lane <RH | C | W | I> of the fs call round-trip work (#818).
Read core/docs/issues/fs-call-round-trips/SPEC.md, then <track>.md and
verification.md, and the lessons under docs/lesson/store-access/ on
layerfs_v2_0_0. Work in your own worktree (tools/worktree.py), from
layerfs_v2_0_0 after the Phase 0 merge. Use your own Cargo target and your own
PostgreSQL database.

Your items, in order: <items from SPEC §6.2>.
You own these files exclusively: <owns column>. Do not edit any other
production file. If you need one, stop and report.
Before each item: write the predicted trips per case and role. After it: pass
every box of the per-phase gate (verification.md), commit with the production
LOC line, and record predicted vs measured.
Open owner decisions that block your items: <D-numbers from SPEC §9>. Do not
start a blocked item. Report and continue with the next unblocked one.
Rules: cargo +1.85.1 --locked -j 6; every test command ≤120 s with an explicit
timeout; netem only inside the PostgreSQL container; no Store format change;
one attempt, no retry; no push or merge without the owner.
Done: your items merged-ready on your branch, pins green on both backends,
receipts under receipts/<item>/.
```
