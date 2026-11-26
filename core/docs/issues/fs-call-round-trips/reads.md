# fs call round trips: Reads track

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract. Part of [SPEC.md](SPEC.md). Source pinned at `056e920166`; all after-values are estimates until measured. The corrections at the end override earlier text.

Scope: `fs.stat`, `fs.read`, `fs.list`, `fs.glob` and `fs.grep` on a Branch or a
pinned Commit, through the SDK route (`methods::call` on a `StoreService`).
Source references are at `056e920166`; paths are relative to `engine/crates/`
unless they start with `core/`.

On PostgreSQL every one of these calls pays five costs.

1. **Twelve history round trips before it touches the tree.** These are:
   - the project lookup (`layerfs-store-service/src/operator/args.rs:20`);
   - the Branch by name (`history/lookup.rs:312`);
   - the Commit row twice, once for the ownership check (`read/snapshot.rs:86`) and once for the root (`snapshot.rs:92` → `lookup.rs:365`);
   - `node_history_serial` (`snapshot.rs:94`);
   - the removal markers (`read/visibility.rs:82` → `path_removals`);
   - `saved::reference` in `paged_reads::pin`, which reads the Commit row a third time plus `branch_any` (`history/saved.rs:13-26`).
2. **Two of those reads are 3-trip read transactions.**
   - `branch_named` and `path_removals` use `self.read` (`layerfs-persistence/src/history/catalog.rs:97-103`, `:344-346`).
   - PostgreSQL runs a `read` as BEGIN/LOCK/fence, then the statement, then `SELECT 1; COMMIT` (`backend/postgresql/session/transaction.rs:493-510`, `:595-607`, `:761`).
   - Their keys `branch_branch_by_name` and `paths_markers` (`history/branch.rs:33`, `history/paths.rs:18`) are missing from the PostgreSQL `qualified` table (`backend/postgresql/history/statements.rs:47-60`), so they cannot run as one statement.
   - Both bodies are single statements, and SQLite has both query files already.
3. **A duplicate, uncached path walk.**
   - Every method calls `snapshot::entry(&reader, …)` for the hidden-path check before the real read: `methods/file/stat.rs:14`, `read.rs:15`, `list.rs:14`, `glob.rs:15`, `grep.rs:18`.
   - `refuse` can only fire when a marker governs the path (`visibility.rs:112-121`, `:277-281`), yet the walk runs even when the project has no markers.
   - The walk costs about 7 trips per path component on the uncached reader, and 3 at the root. That matches the server statement counts: +20 group reads and +8 pooled lookups from depth 3 to depth 7.
4. **No cache across calls.** The SDK reader is the control reader (`store/ports.rs:99-108`, `workspace: None`). `ReadSession::cached` keeps nothing for session 0 (`store/read_handle.rs:197-199`), so each demand starts with empty pack, group and pooled-value caches. Only the locator cache survives between calls (`core/crates/layerfs-storage/src/read/fetch.rs:102`, `begin_demand` at `:314-320`).
5. **Reads that bypass the call's own `Cached`.** These are `TreeTable::open` (`snapshot.rs:93`), `size()` (`snapshot.rs:203`) and the `file_lengths` inside `Cached::roots` (`read/cached.rs:39`), which goes through `StorePorts::file_lengths`, `store/ports.rs:192`.

## Targets

> SPEC §4 recomputes these rows with each case's own measured cost per trip and the cold-pin rules; where the two differ, SPEC §4 governs.

Measured columns: one sample per case, `056e920166`, test profile, PostgreSQL
17 through Docker's port forwarder (`SELECT 1` 0.34 ms), and `netem` +10 ms
(`SELECT 1` 11.0–11.9 ms). "Cold" means no cross-call cache (R1–R5). "Warm"
adds R7 and repeats the call in the same process.

Projected times use:
- SQLite: the estimated SQLite time after R2–R3 (SQLite statements cut by the dedupe share);
- PostgreSQL local: that SQLite time plus trips × 0.69 ms;
- PostgreSQL +10 ms: the local time plus trips × 11.65 ms.

Per-trip costs are the medians across all measured cases. All after-values are
estimates.

| Case | SQLite ms (stmts) | PG local ms | PG +10 ms | Trips now | Cold (R1–R5) | Warm (R7) | SQLite after: ms (stmts) | PG local after | PG +10 after: cold / warm |
|---|---:|---:|---:|---:|---:|---:|---|---:|---|
| `stat /` | 0.8 (39) | 13 | 214 | 18 | 4 | 1 | 0.6 (~26) | 3.3 | 50 / 13 |
| `stat` depth 3 | 2.7 (95) | 37 | 628 | 50 | 18 | 2 | 1.4 (~49) | 14 | 224 / 26 |
| `stat` depth 7 ³ | 5.3 (152) | 56 | 918 | 73 | 13 (conservative ~30) | 2 | 1.5 (~44) | 11 | 162 / 26 |
| `read` small, depth 3 | 2.7 (83) | 34 | 471 | 38 | 6 | 2 | 1.0 (~31) | 5.1 | 75 / 26 |
| `read` 2,000 lines, depth 2 | 2.1 (68) | 23 | 396 | 32 | 7 | 2 | 1.0 (~32) | 5.8 | 87 / 26 |
| `read` lines 100–200 | 1.8 (66) | 20 | 358 | 30 | 5 | 2 | 0.7 (~28) | 4.2 | 62 / 25 |
| `list /` | 0.8 (39) | 14 | 211 | 18 | 4 | 1 | 0.5 (~26) | 3.3 | 50 / 13 |
| `list` 100 entries | 2.5 (69) | 29 | 393 | 33 | 8 | 2 | 1.2 (~33) | 6.7 | 100 / 26 |
| `glob` 100-entry dir | 2.0 (66) | 23 | 380 | 30 | 5 | 2 | 0.9 (~29) | 4.3 | 63 / 26 |
| `glob` whole tree | 1.1 (44) | 17 | 294 | 23 | 9 | 2 | 0.8 (~33) | 7.0 | 112 / 26 |
| `grep` one dir | 1.4 (49) | 17 | 289 | 23 | 5 | 2 | 0.8 (~28) | 4.3 | 63 / 25 |
| `grep` 1k files | 13 (131) | 79 | 999 | 78 | 64 (≈29 with R6) | 2 | 12 (~121) | 56 (≈32) | 802 (≈370) / 37 |

³ Applying −7 per component at every depth gives 13, below depth 3's 18, which cannot hold. Both walks together add 5.75 trips per component from depth 3 to depth 7, so depth 7 after R1–R5 lies between 18 and 41. SPEC §4 uses the midpoint, about 30.

How the cold column is derived: trips now, minus 11 history trips (R1, R2, R5),
minus the hidden-path walk (R3: 7 per component, 3 at the root). Every row
checks against that arithmetic. R4 may save up to 1 more trip per call; that
is not counted. The R6 effect on grep is inferred, not derived.

Track goal:
- Cold: at most 1 history trip, plus the tree's own depth.
- Warm: at most 2 trips per call.
- SQLite: no read call becomes slower or sends more statements.

## Phases

Phases go in rollout order. Each one lands as its own counted commit with its
predicted trip delta, and the probe runs at that commit (see Verification).

**R1. Single-statement Branch-by-name and marker reads (PostgreSQL only).**
- Change:
  - Add `sql/postgresql/queries/history/branch_branch_by_name.sql` and `paths_markers.sql`, schema-qualified like `branch_snapshot_named.sql`.
  - Register both keys in `qualified` (`backend/postgresql/history/statements.rs:51`).
  - Switch `branch_named` and `path_removals` (`layerfs-persistence/src/history/catalog.rs:97-103`, `:344-346`) from `self.read` to `self.read_statement`.
- Mechanism: one fenced autocommit statement, with the fence column joined in (`backend/postgresql/session/prepare.rs:13-19`). It replaces BEGIN…COMMIT.
- Depends on: nothing.
- Saves: 2 + 2 = 4 trips on every Branch-named read. Every row above drops by 4, and so does every other call that resolves a Branch.
- SQLite: none. Its `read_statement` is the same `run(false)` as `read` (`history/transaction.rs:36-56`).
- Memory: none.
- Invariants:
  - The snapshot becomes per statement, which is no weaker: each read already sees a snapshot of its own.
  - Markers are still read after the head is resolved, so a later read can only hide more (fails closed).
- Tests:
  - Extend the statement-schema guard named in `statements.rs:47-50` to cover the two keys.
  - Pin the probe counts at −4.
- Exit: `stat /` = 14 cold; SQLite statement counts unchanged.

**R2. Read the Commit row once; take the serial and markers together.**
- Change:
  - `snapshot::resolve` keeps the Commit or Layer record it already reads for the ownership check (`read/snapshot.rs:85-88`) and takes `root` from it instead of `args::root` (`:92`).
  - `Snap` carries that record. `paged_reads::pin` → `saved::reference` (`history/saved.rs:13-26`) uses it instead of reading the row again.
  - `snapshot.rs:94` and `Visibility::load` (`visibility.rs:40-46`) are replaced with one `node_path_removals` read. That statement already exists (`catalog.rs:352-358`, `paths_node_markers.sql`) and is used by mount (`removal.rs:207`).
- Saves 3 trips per call: the Commit row for the root (1), the Commit row in `reference` (1), and the serial and markers merged (1).
- SQLite: −3 statements per call. This is a dedupe that applies to both backends: same answers, less work.
- Memory: one record per call.
- Invariants:
  - `Unsupported` from a provider without marker reads still means no markers (`visibility.rs:87-91`).
  - Any other marker read failure still refuses the call.
  - Refusal order is unchanged, because both reads happen where the old ones did.
- Tests:
  - `method_refusal_order.rs`.
  - `sdk_path_removal.rs` (hidden-path refusal).
  - Probe pins at −3.
  - SQLite statement pins updated (owner decision O1).
- Exit: `stat /` = 11 cold.

**R3. One path walk per call.**
- Change:
  - In the five methods (`stat.rs:14`, `read.rs:15`, `list.rs:14`, `glob.rs:15`, `grep.rs:18`), skip the hidden-path walk when `seen.markers` is empty.
  - When markers exist, resolve the entry once through the call's `Memo` over `Cached`, check visibility on it, and pass the entry to `reads::{stat,read,list,glob,grep}` (`read/reads.rs:29-40`, `:100-102`, `:129-139`, `:219-243`, `:350-363`), which today walk again.
  - `hidden_children` and `hidden_matches` (`visibility.rs:166`, `:206`) already loop only over markers; route their `snapshot::find` through the same memo.
- Mechanism: `existing` is strict like `entry` (`snapshot.rs:138-170`), so the symlink-component refusal, `NoSuchPath` and `Removed` keep their order.
- Saves 7 trips per component (3 at the root): `stat` depth 3 −21, depth 7 −49, `read` small −21, `list` 100 −14, `grep` 1k −3.
- SQLite: fewer statements (the estimated after-counts in Targets). A dedupe for both backends.
- Memory: none beyond the existing per-call `ReadCache`.
- Invariants: authentication unchanged; visibility semantics unchanged when markers exist.
- Tests:
  - A probe case with markers present (one removed path), so the merged walk is measured, not only the skipped one.
  - Refusal order.
  - SQLite pins.
- Exit: `stat` depth 3 ≤ 22 cold before R5 (50 − 4 − 3 − 21 = 22).

**R4. One `Cached` for the whole call.**
- Change:
  - Build the call's `Cached` in `snapshot::open`/`snapshot_full` (`methods/args.rs:29-40`) and use it for `TreeTable::open` (`snapshot.rs:93`), `size()` (`snapshot.rs:203`) and `Cached::roots`' lengths (`cached.rs:39`).
  - The lengths part needs a `lengths_with(&mut ReadCache, ids)` beside `read_with` and `lengths_kept` (`core/crates/layerfs-storage/src/read/provider.rs:92`, `:122`) and a `StorePorts` counterpart next to `objects_with` (`store/ports.rs:192-215`).
- Saves 0–1 trip per call: only when the root object or file state shares a pack with walked pages. It is counted as 0 in Targets.
- SQLite: possibly fewer statements.
- Memory: the existing per-call bound, `KeptReads::ORDINARY_BYTES` = 3 MiB, freed at the end of the call.
- Exit: no count increases; the measured delta is recorded.

**R5. One context statement per call (PostgreSQL batching; SQLite default).**
- Change:
  - Add `HistoryCatalog::read_context` with a default implementation that calls today's methods in today's order (`core/crates/layerfs-history/src/contract/catalog.rs`, next to `branch_snapshot_named` at `:163`).
  - The PostgreSQL provider overrides it with new qualified queries `read_context_named.sql` (by Branch name) and `read_context_node.sql` (by full Commit or Layer id). Each returns in one row:
    - the stack row;
    - the Branch row;
    - the head node's id, stack, root, `history_serial` and `branch_id`;
    - the recorded Branch name (as `branch_any` reads it);
    - the markers as an array.
  - The commit-prefix path (`methods/args.rs:46`, `pinned`) stays a separate read.
- Mechanism: one fenced statement, extending the pattern of `branch_snapshot_named.sql`, which mount already uses (`store/bind.rs:303-341`).
- Saves the 5 history trips that remain after R1–R2 (project, Branch, Commit row, serial plus markers, recorded Branch name), leaving 1: −4 per call.
- SQLite: unchanged (default implementation).
- Memory: the marker set, which `path_removals` already loads. Give it a named row bound.
- Invariants:
  - Decode the row in today's refusal order: `NoSuchProject`; the commit-prefix lookup; the identity-shaped rule (`snapshot.rs:65-81`); `NoSuchBranch`, then `NoSuchCommit`/`NotFound`; "belongs to another project"; then the path argument and path refusals.
  - One fence. A lost reply quarantines the session, and `one_call` reopens at most once (no retry).
- Tests:
  - Drop and kill sweep at the one message.
  - Refusal order.
  - A SQLite–PostgreSQL equivalence check on the decoded context.
- Exit: `stat /` = 4 cold, `stat` depth 3 = 18 cold.

**R6. grep and glob windows bounded by bytes (optional; owner decision O3).**
- Today `ROOTS` = 64 (`read/cached.rs:76`) and `DIRECTORY_WINDOW` = 64 with `DIRECTORY_BYTES` = 8192 (`read/walk.rs:19-20`) cap each demand by count, well below the reader's per-demand limits: `ROOT_BYTES` = 4 MiB (`cached.rs:77`) and `READ_OBJECT_LIMIT` = 4,096 (`core/crates/layerfs-storage/src/store/policy.rs:77`).
- Change: let the byte bound decide, with the count cap raised to at most 256.
- Saves: `grep` 1k ≈ 64 → ≈ 29 cold (inferred: about 3 trips per window today).
- PostgreSQL only, gated on `batched_chain_reads` (`core/crates/layerfs-storage/src/port/persistence.rs:276`).
- Memory: up to 4 MiB per demand, as the reader already allows.
- Risk: this changes a window constant, so it needs the justification above and measurement on a large-file tree, not only the probe's.
- Exit: the grep and glob pins drop with no byte or RSS regression on the large-file shape.

**R7. Cross-call immutable cache (optional; owner decision O2).**
- Change:
  - Give the SDK reader a bounded cache that survives between calls. Either route SDK object demands through the Store's `CanonicalCache` (`StorePorts::client`, `store/ports.rs:139`; the bounded authenticated object cache in `layerfs-workspace/src/base/cache.rs:20-25`), or keep a `KeptReads` for session 0 (remove the bypass at `read_handle.rs:197`).
  - The SDK route sizes `CanonicalCache` at 64 KiB today (`store_service.rs:34`); the owner sets the budget, for example 8 MiB.
- Keying and validity:
  - Object bytes are content-addressed and authenticated, so a hit is always correct.
  - Each call still resolves its root from fresh history, so it reaches only objects reachable now.
  - Key by project scope (`StorePorts.scope`), as `cache.rs:22-24` requires one authorization context per cache.
  - Locators and kept pack bodies are tied to the Store generation: every fenced statement returns `catalog_incarnation` (`prepare.rs:13-19`), and compaction bumps it (`backend/postgresql/compact/swap.rs:1-12`). Drop them on any change.
  - Before kept pack bodies are enabled, confirm that Branch removal and GC also bump it.
  - Use least-recently-used eviction, not the drop-all-above-bound rule at `read_handle.rs:202-204`.
- Saves: on a warm repeat call, every object trip → 0, so 1–2 trips per call.
- SQLite: unchanged. Enable only where `batched_chain_reads()` is true.
- Memory: the fixed budget per process, which does not grow with Store size. Report charged bytes (`CanonicalCache::diagnostics`, `cache.rs:53-61`) and peak RSS.
- Never cache Branch heads, markers, absence, or history rows across calls.
- Tests:
  - Warm and cold arms reported apart.
  - A compaction-during-cache test that must refuse or re-read, never serve a stale locator as success.
  - RSS stays flat over 1,000 calls.
- Exit: warm reads ≤ 2 trips; RSS growth within budget.

## Layout

Scope: production LOC from `tools/production_loc.py --files` at `056e920166`.
Only files the R phases touch are shown, so directory subtotals are partial.
Tests live in `engine/crates/layerfs-store-service/tests/` and are not counted.

```diff
  layerfs-persistence/                                    [473]  (partial)
    sql/postgresql/queries/history/                         [2]  (partial)
      branch_snapshot_named.sql                               1   (pattern for R5)
      paths_node_markers.sql                                  1   (R2 uses it as is)
+     branch_branch_by_name.sql                               —   new (R1)
+     paths_markers.sql                                       —   new (R1)
+     read_context_named.sql                                  —   new (R5)
+     read_context_node.sql                                   —   new (R5)
    src/                                                  [471]  (partial)
      history/
        catalog.rs                                          378   (R1 read_statement; R5 override)
      backend/postgresql/history/
        statements.rs                                        93   (R1, R5 keys)
```

```diff
  layerfs-store-service/src/                             [3109]  (partial)
    store_service.rs                                        203   (R7 budget)
    methods/                                              [341]  (partial)
      args.rs                                               174   (R4 call cache, R5 context)
      file/                                               [167]
        stat.rs                                              17   (R3)
        read.rs                                              25   (R3)
        list.rs                                              31   (R3)
        glob.rs                                              47   (R3, R6)
        grep.rs                                              47   (R3, R6)
    read/                                                [1530]  (partial)
      snapshot.rs                                           212   (R2, R4, R5)
      visibility.rs                                         219   (R2, R3, R5)
      paged_reads.rs                                        406   (R2 pin takes the record)
      reads.rs                                              356   (R3 entry passed in)
      cached.rs                                              88   (R4 lengths, R6)
      walk.rs                                               249   (R6)
    history/                                              [508]  (partial)
      saved.rs                                              210   (R2 reference from record)
      lookup.rs                                             298   (R2 root from record)
    store/                                                [527]  (partial)
      read_handle.rs                                        248   (R7)
      ports.rs                                              279   (R4 lengths_with, R7)
```

```diff
  layerfs-history/src/                                    [407]  (partial)
    contract/
      catalog.rs                                            407   (R5 read_context, default impl)
```

```diff
  layerfs-storage/src/                                     [98]  (partial)
    read/
      provider.rs                                            98   (R4 lengths_with, cond.)
```

## Owner decisions

- **O1, SQLite statement pins.** R2 and R3 remove duplicate work on both backends: same answers, fewer SQLite statements. The alternative is to gate them on `batches_locations` and keep SQLite's counts. Recommendation: apply to both backends and update the SQLite pins.
- **O2, cross-call cache (R7).** Approve the per-process budget (for example 8 MiB, against today's 64 KiB on the SDK route) and least-recently-used eviction. Note the earlier preference not to trade RAM for speed: the budget is fixed and does not grow with Store size.
- **O3, R6 windows.** A count cap raised inside existing byte bounds. Approve, or keep 64.
- **O4, root object in R5.** Whether the R5 statement also returns the root object's locator and pack range (the joined pattern in `core/crates/layerfs-storage/src/read/joined.rs`). That saves 1 more cold trip per call, at the cost of a larger statement. Recommendation: not in R5; revisit after R7 is decided.

## Corrections from the code review (2026-11-27)

These override the text above where they differ.

- **R5 on SQLite.** Persistence has one `impl HistoryCatalog for HistoryProvider` for both engines (`engine/crates/layerfs-persistence/src/history/catalog.rs:6`). The override branches on `Engine`: PostgreSQL sends `read_context_*`, SQLite calls a public free function in `layerfs-history` that holds the default body. The body keeps today's mapping: `Unsupported` from `node_history_serial`/`path_removals` means serial 0 and no markers (`read/visibility.rs:87-103`, `read/snapshot.rs:94-97`). The statement body lives in persistence `history/branch.rs`, beside `branch_snapshot_named` (`:217`); `catalog.rs` only delegates.
- **R3** keeps `snapshot::entry`'s signature: `write/files.rs:252` (lane W) and `operator/files.rs:56` call it.
- **Ownership:** `store_service.rs` belongs to lane W; R7's budget change waits for lane W's merge.
