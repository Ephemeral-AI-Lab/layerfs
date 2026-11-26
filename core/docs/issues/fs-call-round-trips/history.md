# fs call round trips: History track

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract. Part of [SPEC.md](SPEC.md). Source pinned at `056e920166`; all after-values are estimates until measured. The corrections at the end override earlier text.

Scope: `fs.blame`, `fs.revision`, `fs.diff_files`, and the blame pages that
`fs.diff_files` with `includeAttribution` runs (`read/attribution.rs:55-94`). Paths are
relative to `engine/crates/layerfs-store-service/src/` unless they start with `core/` or
`engine/`. Source: `056e920166`.

**Why it costs what it does (verified).** A history call repeats a single-path read once
per Commit it walks. Every read goes to the control reader, which keeps nothing between
demands (`store/read_handle.rs:192-199`).

Blame, per walked Commit:

- `revision(node)` (`read/blame.rs:218`) reads the Commit row (`operator/files.rs:23`)
  and then `branch_any` for its Branch name (`operator/files.rs:27`).
- `revision(p)` (`read/blame.rs:224`) reads the parent's two rows. The next iteration
  reads them again at `:218`.
- `entry` opens the parent root (`operator/files.rs:55`) and walks the path uncached
  through `find_with` (`read/snapshot.rs:138-170`). `/hist/a.txt` takes 4 inode `value()`
  calls (root, strict check, `hist`, `a.txt`) and 2 directory lookups.
- `node_serial(p)` (`read/blame.rs:229`) is one more read.
- For a changed pair: two sizes (`:248`), then both texts read whole (`:256`). Each
  version's text is read twice, as `older` and again as `current`, because `:305` keeps
  the entry but drops the `Text`.
- At the end, `saved::reference` runs per line (`:323`), reading a Commit row and
  `branch_any` again (`history/saved.rs:13-27`).

Server counts for `blame.h100` (`arms/pg.log`, 2,325 trips) match this:

| Statement | Calls | Accounting |
| --- | ---: | --- |
| Commit row | 252 | 2 × 100 Commits + 50 owners + 2 for the snapshot |
| Branch row | 250 | 2 × 100 + 50 |
| `history_serial` | 101 | 100 + 1 for the snapshot |
| Value blocks (`first_ordinal…`) | 404 | 4 per walk × 101 walks |
| Span (page) reads | 1,212 | about 12 per Commit |
| Locates | 92 | |

`blame.h10` (267 trips) shows the same split: Commit rows 27 = 20 + 5 + 2, Branch rows
25, serials 11, value blocks 44 = 4 × 11. The slope is (2,325 − 267) / 90 = **22.9 trips
per Commit**.

Revision walks the path twice per node (`read/revision.rs:138-157`):

- `parent()` (`:15-30`; under the default `provenance` traversal this is `revision()`,
  which is Commit + Branch);
- `entry(root(node))` (`:139`), `node_serial(node)` (`:144`), `entry(root(p))` (`:152`)
  and `node_serial(p)` (`:155`);
- per returned item: `changes_bounded` sizes (`read/file_facts.rs:76-77`),
  `saved::record` (`:192`, which reads the Commit plus parent and base refs), two
  references (`:198`, `:202`) and two `facts` metadata reads (`:214-215`).

Server counts for `revision.h100` (3,890 trips): 2,127 span reads, 740 value blocks, 508
Commit rows, 277 Branch rows, 185 serials and 46 Layer rows. That is **≈ 39 trips per
node**. The split between the per-node and per-item parts is inferred.

`diff_files` resolves both selectors and walks each side twice: `snapshot::entry`, then
`snapshot::existing` (`read/file_compare.rs:59,70`). Both are strict `find_with`. Server:
46 spans, 16 value blocks, 10 Commit rows. Total 88 trips.

**The lineage premise holds (verified).** A Commit continues its parent's run unless the
parent's run already continues past the parent; then it starts its own run, named by
itself (`engine/crates/layerfs-persistence/src/history/lineage.rs:161-181`). Its depth is
always the parent's depth + 1, and `(run, depth)` is unique
(`engine/crates/layerfs-persistence/sql/postgresql/schema.sql:268`). So a run is one
first-parent path, and the rows `run = R AND depth BETWEEN D−W+1 AND D` are exactly the
first-parent chain of R's Commit at depth D, down to R's first Commit.

That first Commit's parent sits at depth − 1 in another run; continue the walk there. A
depth-1 Commit's parent is its base Layer, which is read through the existing per-step
`layer()` path (`operator/files.rs:36-44`).

Second parents are not in the run, which matches blame's "persisted first parents" rule.
No fallback is silent: the PostgreSQL window read checks that each row's
`parent_commit_id` equals the next row's id, and refuses `Integrity` if not. The catalog
default (one `commit()` per step) needs no lineage at all.

## Targets

> SPEC §4 recomputes these rows with each case's own measured cost per trip and the cold-pin rules; where the two differ, SPEC §4 governs.

All after-values are estimates. Projected ms = SQLite ms after + trips × 0.69 (local), and
+ trips × 11.65 more at +10 ms. SQLite ms after is scaled by its statement ratio.

| Case | SQLite ms (stmts), now → after | PG trips, now → target | PG local ms, now → after | PG +10 ms, now → after |
| --- | --- | --- | --- | --- |
| blame, 10 versions | 17.7 (714) → ~8.8 (~360) | 267 → **~17** | 203 → ~21 | 3.42 s → ~0.22 s |
| blame, 100 versions | 172.7 (7,575) → ~86 (~3,800) | 2,325 → **~29** | 1,829 → ~106 | 31.2 s → ~0.44 s |
| revision, 100 versions | 286.0 (12,658) → ~129 (~5,700) | 3,890 → **~30** | 3,263 → ~150 | 50.9 s → ~0.50 s |
| diff_files | 5.2 (198) → ~3.1 (~119) | 88 → **~12** | 64 → ~11 | 1.15 s → ~0.15 s |

The SQLite statement drop comes only from H1–H3 (fewer rows, one walk, one text read).
H4–H6 batching leaves SQLite on its current path.

Formula after H6, cold, per call:

T ≈ F + d + Σ_windows (c + h + 1 + m_w + k)

| Symbol | Meaning |
| --- | --- |
| F | Fixed per-call trips after R5 (1–2) |
| d | Path depth; first-use directory lookups, then memoized |
| windows | ⌈V⌉ split as 16, 32, 64, then 128 each, so V = 100 is 3 windows and V = 10 is 1 |
| c | 1 chain-window statement + 1 Branch-name statement for new ids + 1 roots demand |
| h | Inode-table height (pages + pooled leaves) |
| 1 | The value-block statement |
| m_w | Path levels whose directory root changed inside the window (≤ d; 0 for content-only edits) |
| k | Content levels: lengths + bytes ≈ 2 |

With h = 2 this is about 8–10 per window. Blame stops early, so it overfetches by at most
one window (≤ 2× the versions it needed). Revision pages end at `page.limit` or
`ROW_BYTES`; prefetched rows past the stop point are discarded.

## Phases

H1–H3 remove duplicated work on both backends. H4–H6 batch reads and run only where the
provider reports a round-trip cost. Trip savings are inferred, about ±30%, and are pinned
only when measured.

**H1. Carry rows forward; read each reference once.** Depends on R2.

- Change:
  - Blame keeps the `(root, parent, value)` it read for `p` at `read/blame.rs:224` and
    uses it as the next iteration's `revision(node)` at `:218`.
  - Branch names are memoized per call in a `HashMap<BranchId, Option<String>>`, used by
    `operator/files.rs:27` and `history/saved.rs:21`.
  - `saved::reference` and `saved::record` take an optional held `CommitRecord`, and
    blame builds owners from rows it already holds (`:323`).
  - Blame carries `Text` from `current` to `older` (`:305`), so each version's text is
    read once.
- Trips saved:
  - blame h100: Commit rows 252 → ~101, Branch rows 250 → ~1, plus about half of the
    changed-pair text spans; ≈ −500 (2,325 → ~1,800).
  - revision h100: Commit rows 508 → ~110, Branch rows 277 → ~1; ≈ −670 (3,890 →
    ~3,200).
- SQLite: the same rows disappear, a statement cut of about −8% on blame (no time
  claim).
- Memory: the name memo is bounded by the distinct Branch ids met, at most one per
  walked Commit (≤ `MAXIMUM_QUERY_WORK` = 10,000, `read/query.rs:8`), about 0.5 MB worst
  case. The carried `Text` is bounded by `window::BYTES`/`LINES` (48 KiB / 2,000 lines,
  `read/window.rs:11-12`).
- Invariants: identical `owner`, `origins`, `commit`/`message` per line, `coverage` and
  `comparisonCoarse`.
- Exit: the history pins below pass, with server Commit rows ≤ V + 3 and Branch rows ≤
  distinct + 1.

**H2. Revision walks once and carries `now` forward as the next node's `before`.**
Depends on H1.

- Mechanism: the next node is `previous`, so its `now` is this node's `before`
  (`read/revision.rs:139,152`). Keep `before`, the root and the serial, and skip `:139`
  and `:144` on the following iteration. `comparedTo` reuses the previous item's `ref`.
- Exception: on a `hidden_before` stop the carried value is `None`; compute it as today.
- Trips: about −1 walk and −1 serial per node (≈ −13/node): 3,200 → ~1,900.
- Invariants: `candidateIndex`, `Boundary { next, candidate }` (`:184-189`), continuation
  tokens and `revision_pages` are unchanged, because the iteration order is unchanged.
- Test: the existing `sdk_file_history.rs` pages pass unchanged.

**H3. One per-call cache across versions, plus a directory-lookup memo.** Depends on R4.

- Change: all reads of a history call go through one `Cached` (`read/cached.rs`;
  `objects_with` at `store/ports.rs:202`), including `size()` (`read/snapshot.rs:203`)
  and `file_lengths`.
- Add a memo keyed by `(DirectoryRoot, PathName) → serial`. Directory contents are
  immutable, so an equal root gives an equal binding. It is bounded at 4,096 entries.
- `diff_files` uses `raw` instead of walking again when `allow_missing` is false
  (`read/file_compare.rs:59,70`). Both walks are strict, so behaviour and refusal are the
  same.
- Trips: consecutive versions share every page outside the changed path, so a version
  costs only its new root, its new inode branch and leaf, and its pooled values (≈ 6–8)
  instead of ≈ 12 + 4 value blocks.
  - blame h100: ~1,800 → ~900
  - revision h100: ~1,900 → ~1,000
  - diff_files: 88 → ~55
- SQLite: fewer statements, because pages are no longer re-read.
- Memory: the existing `ReadCache` bound, `KeptReads::ORDINARY_BYTES` (3 MiB), dropped
  at call end.
- Exit: page reads per Commit ≤ 8 on `blame.h100`.

**H4. Windowed first-parent chain read.** Depends on H1. New SQL only, no format change.

- Add to `HistoryCatalog` (`core/crates/layerfs-history/src/contract/catalog.rs`, next
  to `commit` at `:234`):

  ```rust
  fn first_parent_window(&self, head: CommitId, count: usize) -> HistoryResult<Vec<ChainRow>>
  ```

  `ChainRow` carries id, root, parent, base layer, branch id, `history_serial`, message
  and time. `CommitRecord` has no serial today (`contract/records.rs:73-88`), hence the
  new row type.
- The default implementation loops `commit()` and `node_history_serial`. SQLite keeps
  it.
- PostgreSQL (`engine/crates/layerfs-persistence/src/history/catalog.rs`, beside
  `:161`) sends one fenced statement:
  - join `commit_lineage` (`run = R AND depth BETWEEN D−W+1 AND D`, using the
    `commit_runs` index) to `"commit"`;
  - continue across a run boundary through the first Commit's `parent_commit_id`;
  - stop at a depth-1 Commit, whose base Layer is read per step as today.
- Windows of 16, 32, 64, then 128 rows, clipped to the remaining `Limits.versions`
  (blame) or `MAXIMUM_QUERY_WORK − examined` (revision). Budgets are charged before the
  fetch.
- Branch names: one `= ANY` statement per window, for new ids only.
- Trips: −2 per Commit (row + serial), down to 2 per window.
  - blame h100: ~900 → ~700
  - revision h100: ~1,000 → ~800
- Invariant: `hidden_at` reads the same `history_serial` column
  (`position_node_serial.sql`).
- Tests: the window equals the default loop on randomly generated histories (forks,
  merges, Layer boundaries, removed Branches), on both backends.

**H5. Cross-version, level-by-level path fetch.** Depends on H3 and H4. Gated on
`batched_chain_reads()` (`core/crates/layerfs-storage/src/port/persistence.rs:276`).

- Mechanism:
  - Read all W roots in one demand.
  - From the first walk, take the serial chain (root, then each component). Fetch those
    d + 1 serials across all W inode tables with one demand per inode level, then one
    value-block statement.
  - Versions whose directory roots all hit the H3 memo are complete. The rest get one
    batched directory-page demand per level, for the misses only.
- Where: a new multi-root lookup in content, `compare/across.rs`, built on
  `filesystem/inode/read.rs` (`lookup_many`). It is called from a new
  `read/versions.rs`, which owns the window and the budgets.
- Trips: per window ≈ h + 1 + m_w, instead of W × (6–8).
  - blame h100: ~700 → ~35
  - revision h100: ~800 → ~40
- Memory:
  - leaf demands within `PREPARED_LEAVES` = 128 and about 1 MiB
    (`core/crates/layerfs-storage/src/encoding/pool/read.rs:99`);
  - the roots demand ≤ 128 objects;
  - all inside the per-call cache bound.
- Invariant: every object is still authenticated by the resolver; a strict-walk symlink
  refusal still happens at the version where it would happen today.

**H6. Lengths and content per window; `Text` carried.** Depends on H5. Gated as H5.

- Mechanism:
  - One `file_lengths` demand for every version whose `content_root` changed.
  - Blame admits pairs to `limits.bytes` in walk order, then fetches the admitted texts
    in one bounded demand: ≤ 4 MiB (`read/cached.rs` `ROOT_BYTES`) and ≤ 64 roots.
  - Revision batches `metadata_root` reads for `facts`.
  - `equal_edges` (the coarse path, files over 48 KiB) stays per pair.
  - Equal lengths are still byte-compared; different lengths decide content changed
    without reading bytes, as today.
- Trips: blame h100 ~35 → **~29**, revision ~40 → **~30**, blame h10 → **~17**,
  diff_files → **~12** (both sides planned together).
- Invariant: `CONTENT_QUERY_BYTES` (64 MiB, `read/file_facts.rs:61`) is charged in the
  same order as today.

**H7 (cond.). Attribution pages share one version table.**
`read/attribution.rs:60-94` calls `blame()` once per 2,000-line page, and each page walks
the history again. Keep the H4–H6 window rows and roots in the call, bounded by the H5
budgets, so later pages walk 0 extra windows. Build it only if
`diff_files includeAttribution` on files over 2,000 lines is in the benchmark.

## Layout

Scope: `056e920166`, production LOC from `tools/production_loc.py --files`. Partial
listings: only files this track edits or reads closely. Tests live in
`engine/crates/layerfs-store-service/tests/`: extend `fs_call_round_trips.rs` (probe) and add
`sdk_history_round_trips.rs` (new pins).

```diff
  engine/crates/layerfs-store-service/src/     [2508]  (partial)
    operator/                                   [174]
      files.rs                                  174   (H1: revision takes held rows; Branch-name memo)
    history/                                    [210]
      saved.rs                                  210   (H1: reference/record from a held CommitRecord)
    read/                                      [2124]  (partial)
      mod.rs                                     24   (H4/H5: declare versions)
      blame.rs                                  367   (H1 carry rows/Text; H5/H6 use versions)
      revision.rs                               289   (H2 one walk; H5 use versions)
      file_compare.rs                           296   (H3: reuse raw walk; H6 plan both sides)
      attribution.rs                            321   (H7 cond.: share the version table)
      file_facts.rs                             129   (H6: batched lengths/metadata)
      snapshot.rs                               212   (H3: size via the call cache)
      cached.rs                                  88   (H3: directory memo, lengths through ReadCache)
      query.rs                                   62   (unchanged; listed for MAXIMUM_QUERY_WORK)
      window.rs                                 336   (unchanged; listed for BYTES/LINES)
+     versions.rs                                 —   new (H4–H6: chain windows, version table, budgets)
```

```diff
  core/crates/layerfs-history/src/              [785]  (partial)
    contract/                                   [785]
      catalog.rs                                407   (H4: first_parent_window with default loop)
      records.rs                                378   (H4: ChainRow)
```

```diff
  core/crates/layerfs-content/src/              [878]  (partial)
    compare/                                    [643]
      mod.rs                                     21   (H5: declare across)
      tree.rs                                   622   (H5: expose table/serial helpers to across)
+     across.rs                                   —   new (H5: multi-root level-by-level lookup)
    filesystem/                                 [235]
      inode/                                    [235]
        read.rs                                 235   (H5: lookup_many over several tables)
```

```diff
  engine/crates/layerfs-persistence/            [768]  (partial)
    src/                                        [767]
      history/                                  [674]
        catalog.rs                              378   (H4: PostgreSQL first_parent_window)
        lineage.rs                              296   (read only; run/depth premise)
      backend/                                   [93]
        postgresql/                              [93]
          history/                               [93]
            statements.rs                        93   (H4: qualified chain-window text)
    sql/                                          [1]
      postgresql/                                 [1]
        queries/                                  [1]
          history/                                [1]
            commit_commit_by_id.sql               1   (unchanged; row shape reference)
+           lineage_chain_window.sql              —   new (H4)
```

## Owner decisions

1. **Window sizes.** Doubling 16 → 128 and the 128-object roots demand. Larger windows
   cut trips further, but blame then overfetches more when it stops early.
2. **SQLite pins.** H1–H3 lower SQLite statement counts, by about −50% on blame and
   revision (estimate). Accept new SQLite pins, or gate H1–H3 on
   `batches_locations()`/`batched_chain_reads()` too. The recommendation is to accept:
   the answers are the same with less work.
3. **H7.** Build it only if large-file attribution is in scope.
4. **Not proposed (format change, flagged only).** A per-path change index (path →
   Commits that changed it) would make blame and revision cost O(changed versions)
   instead of O(walked Commits). It needs a Store format change and history-sized
   maintenance on every Commit.

## Corrections from the code review (2026-11-27)

These override the text above where they differ.

- **`ChainRow` is a full `CommitRecord` plus `history_serial`**, decoded as `commit_commit_by_id` does. Revision items pass the row to `saved::record` → `commit_value` (`layerfs-bridge/src/sdk_values.rs:56-100`). That uses `meta.via`, `merged_from`, the lww count, `statistics` and the second parent, so a thinner row would leave one `commit()` read per item.
- **H4 on SQLite:** the override in persistence branches on `Engine`, and SQLite calls the free-function default (see reads.md corrections). The statement body lives in persistence `history/lineage.rs`, or a new `history/chain.rs` if `lineage.rs` would take a second job.
- **H5 core:** lane C adds a new multi-table lookup and keeps `lookup_many`'s signature, which W5 wraps.
- **Pin file:** `sdk_history_round_trips.rs`. The probe `fs_call_round_trips.rs` is a measurement, not a gate.
