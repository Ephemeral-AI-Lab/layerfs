# Costly Store access patterns

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** store-access · **Applies when:** designing or reviewing code on a Store read or write path (Commit, Init, or a per-call file or history read), especially for a remote Store · **From:** [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810), [#815](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815), [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816), [#818](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818), [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822), [#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824)

## Rule

Before shipping a Store path, check it against these patterns; each one has cost
hundreds to tens of thousands of round trips or megabytes per connection.

## The catalog at a glance

Patterns 1–9 come from Commit, ranked by the largest cost seen. Counts are
PostgreSQL round trips per Commit on local loopback. **Estimated** is #810's
per-source estimate at 0.9 ms a round trip, taken with statement logging on; it
overstates the largest cells.
**Measured** is one sample on cold local PostgreSQL. Loopback hides these costs:
at +10.6 ms, #816's cuts took B1 from 8,426 to 3,924 ms. The fixes follow
[Round trips follow depth](round-trips-follow-depth.md) and keep the
[Rules to preserve](rules-to-preserve.md); none changed the Store format.

Patterns 10–15 come from SDK file and history calls and Init, ranked likewise.
Counts are client round trips per call (writer and readers) at v2 `056e920166`,
one sample, local PostgreSQL at 0.34 ms a trip. At +10 ms each trip cost ~11.4 ms,
and these calls ran 57–419× slower than on SQLite (5–33× locally) (measured,
[#818 timing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203237505)).

| # | Pattern | Largest cost seen | Status |
| --- | --- | --- | --- |
| 1 | One walk per changed directory | ~17,100 reads, ~15 s (estimated) | Fixed |
| 2 | One base read per touched or removed directory | 7,359 trips, ~6.6 s (estimated) | Fixed |
| 3 | One write round trip per Save batch or publication wave | ~4.5 s on B1 (estimated); 260 trips (measured) | Fixed; B1 cap waived |
| 4 | Re-reading the same pages in each phase | 3,446 reads, ~3.1 s on C4 (estimated) | Fixed |
| 5 | One base-file read per changed file | 725 trips, ~1.2 s on C4 (estimated) | Fixed |
| 6 | Reading a whole deleted subtree | 1,417 pages read to write 4 objects (measured) | Open |
| 7 | Very large statements on a long-lived connection | ~15 MiB more retained on B4 (measured) | Fixed |
| 8 | Locate, then read, as two trips | joining them saved nothing (measured) | Lesson only |
| 9 | Cold per-item reader reads | 25–34 trips (measured) | Open |
| 10 | A per-item loop hidden by a cache or size threshold | 31,041 trips, Init of 10k files in one directory (measured) | Open, #818 |
| 11 | Fetching what the call already computed | 10,142 locates, Init of 10k files (measured) | Open, #818 |
| 12 | History depth as a second chain of waits | 3,890 trips, 50.9 s at +10 ms, `fs.revision` of 100 versions (measured) | Open, #818 |
| 13 | The same path walked twice in one call | 73 trips, 918 ms at +10 ms, `fs.stat` at depth 7 (measured) | Open, #818 |
| 14 | A fixed cost on every call | 18 trips for `fs.stat /`: 214 ms at +10 ms, 0.8 ms on SQLite (measured) | Open, #818 |
| 15 | A batched helper that serves one route only | No count of its own; behind 8, 10, 11 and 14 | Open, #818 |

## The patterns

### 1. One walk per changed directory

- **What it looks like.** After the Save, a walk over the old and new roots reads
  each changed directory version with its own locate and value read:
  `collect_new_bindings` (`engine/crates/layerfs-store-service/src/workspace/commit/hints.rs`)
  over `new_bindings` (`core/crates/layerfs-content/src/compare/tree.rs`).
- **Cost seen.** C4s4 (10,000 files added in 3,942 directories): 15,570 locates and
  1,558 value reads at `ca4be0bf3c`, about 15 s (estimated). After F1 the
  `bindings` phase fell from 11,403 to 2,056 trips (measured,
  [PostgreSQL Store](../../architecture/engine/store/postgresql.md#commit-reads-before-the-lane-810));
  98 were left before #816's binding transition (measured,
  [#815 record](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815#issuecomment-7185513233)).
- **How to see it.** The writer's `SqlWork` trips in the `bindings` phase; pinned by
  `engine/crates/layerfs-store-service/tests/commit/batched_reads.rs` (`adds`).
- **Fix.** Diff 128 changed directories a window (`directory_bindings`), or reuse
  the bindings the directory merge already saw (`BindingChanges` in
  `core/crates/layerfs-content/src/filesystem/construction/captured_bindings.rs`).
- **Status.** Fixed: F1 in [#820](https://github.com/Ephemeral-AI-Lab/layerfs/pull/820),
  the certified binding transition in #816. C4s4 is now 203 trips in all.

### 2. One base read per touched or removed directory

- **What it looks like.** Construction rewrites each changed base directory from its
  root page, read on its own, about twice per touched and once per removed directory:
  `apply_directory_changes` in `core/crates/layerfs-content/src/filesystem/tree/pages/finish.rs`.
- **Cost seen.** 7,359 trips on the 10k add and 1,443 on B2 (delete `node_modules`)
  at `ca4be0bf3c`; about 6.6 s and 1.3 s (estimated).
- **How to see it.** Most of `construct` on a wide add; pinned per phase in
  `commit_batched_directories.rs`.
- **Fix.** Group a window's base roots into demands of `PAGES_AHEAD` = 256 ids
  (`core/crates/layerfs-content/src/filesystem/tree/directory/read_ahead.rs`); see
  [construction](../../architecture/engine/commit/construction.md#base-directory-reads-810-f3).
- **Status.** Fixed: F3 in #820. A large directory's deeper pages are read on demand.

### 3. One write round trip per Save batch or publication wave

- **What it looks like.** A similar tiny file whose ring proposal is still pending
  forces a publication first (`offer` in `core/crates/layerfs-storage/src/save/select/representation.rs`,
  then `seal_pending` and `register_ready`), about 6 trips each. And each Save batch
  publishes as its own fenced `BEGIN`, lane acquisition, registration and `COMMIT`.
- **Cost seen.** B1 (npm install, 10,002 files) made 614 waves, about 4.5 s
  (estimated); F2 took it from 5,309 to 1,405 trips (measured,
  [PostgreSQL Store](../../architecture/engine/store/postgresql.md#similar-files-publish-once-per-wave)).
  Its 54 separate Save publications then cost 216 trips; one flight per batch took
  B1 from 713 to 453, 260 saved (measured,
  [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7187186832)).
- **How to see it.** `Diagnostics::publish` (`core/crates/layerfs-storage/src/read/source/counters.rs`),
  pinned in `commit_similar_tiny_files.rs`; transactions in `SqlWork`.
- **Fix.** Decide before publishing (`declared_acquired`, `prefix_cannot_win`); send
  a batch as one fenced flight (`run` in
  `engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish_flight.rs`).
- **Status.** Fixed: F2 in #820 (storage and chain depth matched in all 12 pairs),
  the flight in #816. B1 is 270 trips against a waived cap of 150.

### 4. Re-reading the same pages in each phase

- **What it looks like.** `construct`, `finish`, `logical.count` and `bindings` each
  read the same inode leaves and values (`leaf_parts_with_caches` in
  `core/crates/layerfs-storage/src/encoding/pool/read.rs`, `logical_after`): the
  512 KiB `POOLED_VALUE_CACHE_BYTES` overflows at about 1,000 changed files.
- **Cost seen.** C4 (1,000 edits in a deep 50k-file tree): 1,164 + 114 + 1,084 +
  1,084 value reads over the four phases, about 3.1 s (estimated). With F1 and F4,
  C4 went from 4,848 to 950 trips (measured, PostgreSQL Store page).
- **How to see it.** The same page IDs in several phases; per-phase counts in the
  `edits` scenario of `commit_batched_reads.rs`.
- **Fix.** Walk once (`logical_size_and_directories`); read leaves and values a window
  at a time (`prepare_leaves`, `inode_values_ahead`), never a cache for the whole Commit.
- **Status.** Fixed in #820 and #816; C4 is 91 trips at `056e920166`. No test pins
  the two logical-count routes (up to 64 rows, and larger) equal.

### 5. One base-file read per changed file

- **What it looks like.** Per changed file, even for `chmod` or `touch`, the Workspace
  reader asks its length (`provided_length` in
  `engine/crates/layerfs-vfs/src/base/fetch.rs`), then locates and reads it.
- **Cost seen.** 725 trips on C4 and 375 on C3 at `ca4be0bf3c`; about 1.2 s and
  0.35 s (estimated).
- **How to see it.** Workspace-reader trips in the writer/reader split.
  `commit_batched_base_files.rs` pins `content_reads` at 0 for its C3-like shape.
- **Fix.** Decide a whole inode page together: keep a fully inherited file
  (`inherited` in `engine/crates/layerfs-vfs/src/construction/file/kept.rs`),
  batch the rest (`base_files` in `engine/crates/layerfs-vfs/src/base/batch.rs`).
- **Status.** Fixed: F5 in #820. C3 is 36 trips and reads no file content
  (measured, [#810 closing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7203083110)).

### 6. Reading a whole deleted subtree

- **What it looks like.** Removing a directory's last binding releases every entry
  under it: `release_zero_count` in
  `core/crates/layerfs-content/src/filesystem/construction/reference_count/release.rs` reads the
  removed tree's pages and base records while writing almost nothing.
- **Cost seen.** B2 at `056e920166`, 11,439 entries deleted: 1,417 directory pages
  and 1,649 objects read in 33 waves, 4 objects written; 114 trips (92 in
  `construct`), 1,075 ms; SQLite 904 ms (measured, #810 closing).
- **How to see it.** `ReleaseWork` (`pages`, `entries`, `base_records`) against
  objects written. SQLite nearly as slow as PostgreSQL means local work.
- **Fix.** None built. Batching cut the trips, not the reading; the fix is to account
  for a removed subtree without reading all of it.
- **Status.** Open. The B2 ceilings were waived; #824 holds it, closed not planned.

### 7. Very large statements on a long-lived connection

- **What it looks like.** One statement binds a whole batch (a 4 MiB captured flight,
  a fused tail of up to 16 MiB). tokio-postgres keeps its request and write buffers
  at their peak size for the connection's life: two batch-sized buffers per writer.
- **Cost seen.** RSS after a Commit, KiB: B4 58,708 on v2, 72,992 at `db54e9d75f`,
  57,556 after the split; A1 64,000, 63,428, 56,672 (measured,
  [#816 results](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418)).
- **How to see it.** RSS retained after the Commit; the largest single statement.
- **Fix.** Split pack inserts into runs of at most `STATEMENT_PACK_BYTES` (=
  `PACK_LIMIT`, 256 KiB) in the same flight and transaction (`pack_statements` in
  `engine/crates/layerfs-persistence/src/backend/postgresql/history/fused.rs`).
- **Status.** Fixed in #822. A singleton pack cannot split; row and hint statements
  reach about 1 MiB. On receive, copy a kept row out of the driver's buffer.

### 8. Locate, then read, as two trips

- **What it looks like.** `locate_chains` returns locators, then a separate pack or
  range read fetches them: two sequential waits per window.
- **Cost seen.** Joining both in one statement (`locate_chains_with_packs`) moved
  C4s4 from 568 to 569 trips and C4 from 146 to 147 (measured, #815 Stage 1).
- **How to see it.** A locate followed at once by a read of what it returned.
- **Fix.** Cut sequential waits, not statements: one pipelined message per level
  (`pipeline` in `engine/crates/layerfs-persistence/src/backend/postgresql/session/pipeline.rs`).
  Joining helps only where the locate is still its own sequential trip, such as an
  uncached read that locates and then fetches each object; count that path first.
  #818 found one: the SDK's control reader (session 0) caches nothing
  (`ReadSession::cached`, `engine/crates/layerfs-store-service/src/workspace/bind/reader.rs`),
  so each object read is a locate, then a pack fetch (pattern 15).
- **Status.** In Commit construction, landed in #820 as a measured miss. For the
  uncached SDK reader, joining is proposed in #818 (open).

### 9. Cold per-item reader reads

- **What it looks like.** On a cold Commit the Workspace readers fetch base pages one
  demand per round trip (`CanonicalClient` in
  `engine/crates/layerfs-vfs/src/base/fetch.rs` misses its cache).
- **Cost seen.** Reader trips, v2 to tip: B5e 26 to 25, C1 32 to 31, C4s4 38 to 34
  (measured, [#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824)). Enough
  to put B5e and C1 over caps set from warm counts.
- **How to see it.** Count cold and split writer from readers; a warm count hides
  them ([State the cache behind a cap](../measurement/state-the-cache-behind-a-cap.md)).
- **Fix.** Proposed: plan-then-fetch per window of sealed records (#815).
- **Status.** Open; set aside as a small gain in #824.

### 10. A per-item loop hidden by a cache or size threshold

- **What it looks like.** Init's `logical_size`
  (`core/crates/layerfs-content/src/filesystem/construction/statistics.rs`, called from
  `engine/crates/layerfs-project/src/import/run.rs`) reads each file's root alone,
  hidden until the 4,096-entry locator cache overflows. A directory of 512+ entries
  (`WINDOW_ROWS`) is placed by `place_children` (`postgresql/acquisition/writes.rs`
  in `layerfs-persistence`): per child, `INSERT init_host_file`, then `UPDATE init_entry`.
- **Cost seen.** 1k files in one directory: 2,246 trips (229–286 in ten); 10k in
  one: 31,041, ~3 trips a file. `logical_size`: no locates at 1k files, 10,142 at
  10k (measured, [#818 counts](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203068532)).
  At +10 ms, 1k flat took 27,041 ms (SQLite 65); 10k flat ~379 s (estimate).
- **How to see it.** Count above every cache and threshold, flat and nested
  ([Count round trips above every threshold](../measurement/count-round-trips-above-every-threshold.md));
  in review, find Store calls in per-row loops (`tx.typed` in `place_children`).
- **Fix.** Proposed in #818: one multi-row `unnest` statement per window.
- **Status.** Open, [#818](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818).

### 11. Fetching what the call already computed

- **What it looks like.** Init's scan knows each file's size, yet `logical_size`
  reads every file's root back from the Store for its length.
- **Cost seen.** The 10,142 locates at 10k files in pattern 10 (measured).
- **How to see it.** Ask where each read's answer was first known.
- **Fix.** Proposed in #818: lengths from the scan, or batched (`file_lengths`).
- **Status.** Open, #818.

### 12. History depth as a second chain of waits

- **What it looks like.** Each `fs.blame` version reads its Commit row and Branch
  name, the parent's row (again next step), the parent's root, an uncached path
  walk and `node_serial`, one version after another. `fs.revision` (`history` in
  `engine/crates/layerfs-store-service/src/families/operator/files.rs`) walks twice a version.
- **Cost seen.** ~22 trips a version for `blame`, ~40 for `revision`; at 100
  versions 2,325 and 3,890 trips, 31,202 and 50,860 ms at +10 ms (SQLite 173, 286).
  `blame` server reads: 1,212 pages, 252 Commit rows, 250 Branch rows, 101
  `history_serial` (measured, #818 counts and timing).
- **How to see it.** Run at 10 and 100 versions; find a path walk in a version loop.
- **Fix.** Proposed in #818: the per-call `Cached` reader (`read/cached.rs`) and
  rows; the first-parent chain in batches, then all versions' paths level by level.
- **Status.** Open, #818.

### 13. The same path walked twice inside one call

- **What it looks like.** `fs.stat` and `fs.read` (`methods/file/stat.rs`, `read.rs`
  in `engine/crates/layerfs-store-service/src/`) first walk with an uncached
  `snapshot::entry` for the hidden-path check, even with no `path_removals`
  markers, then again in `reads::stat` or `reads::read` through `Cached`; strict
  `find_with` (`read/snapshot.rs`) reads `value(serial)` twice a component.
- **Cost seen.** About 6 trips a component: `fs.stat` 18 at `/`, 50 at depth 3, 73
  at depth 7; 214, 628 and 918 ms at +10 ms (SQLite 0.8–5.3 ms) (measured).
- **How to see it.** Two walks of one path in a call; `snapshot::entry` callers.
- **Fix.** Proposed in #818: skip the check when the project has no markers, else
  reuse the read's walk; read `size()` through the cache.
- **Status.** Open, #818.

### 14. A fixed cost on every call

- **What it looks like.** Before the tree, each call reads the project, the Branch,
  the Commit or Layer row (twice), `node_history_serial`, `path_removals` and
  `saved::reference`, each its own statement, in two read transactions.
- **Cost seen.** 18 trips for `fs.stat /` and `fs.list /`: 214 and 211 ms at
  +10 ms, against 0.8 ms on SQLite (measured, #818 timing).
- **How to see it.** Count `stat /`. No test pins it; #818's probe is untracked.
- **Fix.** Proposed in #818: one statement for these, as `branch_snapshot_named` does.
- **Status.** Open, #818.

### 15. A batched helper that exists but serves one route only

- **What it looks like.** `branch_snapshot_named` (`HistoryCatalog`,
  `core/crates/layerfs-history/src/catalog/contract.rs`) is called only from mount
  (`engine/crates/layerfs-store-service/src/workspace/bind/run.rs`); the joined locate and
  pack fetch only through `locate_for_save` (`layerfs-storage/src/read/fetch.rs`).
  Init skips `file_lengths`; only its narrow windows use a multi-row `unnest`.
- **Cost seen.** None of its own; it is the unused half of patterns 8, 10, 11, 14.
- **How to see it.** `grep -rn` the batched sibling and list its callers.
- **Fix.** Proposed in #818: route the other paths through the existing helper.
- **Status.** Open, #818.

## Using this list in review

1. Does any loop await one Store call per file, directory, page, pack or value?
2. Does a later phase read what an earlier phase already read?
3. How many transactions or flights does one Save batch send?
4. Does a deletion or metadata-only change read content or a subtree it does not need?
5. What is the largest single statement the connection can send, and what bounds it?
6. Is each window sized by rows or bytes, never by the whole Commit?
7. Is the shape's PostgreSQL count pinned, split by role, with its cache state?
8. What does the cheapest call cost, and is it paid again per version or item?
9. Was the count taken above every cache and threshold, flat and nested?
10. Does a batched form of this read already exist on another route?

## Evidence

- #810: [per-source counts and estimates](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810),
  [results at the v2 tip and the B2 waiver](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7203083110).
- #815: [Stage 1 record](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815#issuecomment-7185513233), [closing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815#issuecomment-7202838625).
- #816: [step 0](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7186732914),
  [260 trips](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7187186832),
  [results and memory](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418),
  [closing note](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202676223).
- #818 (open), at `056e920166`: [round trips per call and causes](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203068532), [time on SQLite, local PostgreSQL and +10 ms](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203237505).
- [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822) (statement splits),
  [#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824) (what was left),
  [PostgreSQL Store](../../architecture/engine/store/postgresql.md),
  [Commit construction](../../architecture/engine/commit/construction.md).
- Receipts on the owner's machine: `2026-11-26-commit-batching/bench-810-qualify/`.
