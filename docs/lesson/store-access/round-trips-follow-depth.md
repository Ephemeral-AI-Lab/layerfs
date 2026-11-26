# Round trips follow depth

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** store-access · **Applies when:** code awaits a Store read or write once per changed item · **From:** [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810), [#815](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815), [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816), [#820](https://github.com/Ephemeral-AI-Lab/layerfs/pull/820), [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822), [#823](https://github.com/Ephemeral-AI-Lab/layerfs/pull/823)

## Rule

A remote Store call's round trips must grow with tree depth, in bounded windows,
not with how many files, directories or pages the change touches.

## In short

```text
 per item:   (dirs + files) x reads per item x phases     C4s4 before #810:   26,740 trips
 per level:  levels x windows (+ writes as fenced flights) C4s4 at 056e920166:    203 trips
```

Collect what the change already names, read it a level at a time, keep it for later
phases, and send dependent writes together. The ranked list of other costly shapes is
in [costly patterns](costly-patterns.md); the invariants a batching change must keep
are in [rules to preserve](rules-to-preserve.md).

## What happened

The Commit read path was written when the Store was SQLite in the same process, where a
lookup is a function call. Construction located and read each changed directory's page,
its chain bases, its inode leaf values and each changed file's base, each when the code
reached it. PostgreSQL turned each call into a round trip that waits for the one before.
Illustrative, for one changed directory `d` with two changed files:

```text
 before: each line is one round trip, and each waits for the previous reply
   construct      locate(d page) -> read(d page) -> locate(chain base) -> read(base)
                  values(d's inode leaf)
                  length(f1 base) -> read(f1 spans); length(f2 base) -> read(f2 spans)
   finish         values(d's inode leaf) again
   logical.count  read(d's inode leaf) -> values(leaf)
   bindings       locate(old d page) -> read -> chain level -> new d page -> ...
 after: one demand per window, for d and its window siblings together
   construct      [pages of 128 dirs] -> [their chain bases, a level at a time]
                  [inode leaves + values of the window] ; [lengths + roots of one inode page]
   Save           pack upload + row registration in one fenced flight
   later phases   reuse kept pages and values; bindings come from the merge itself
```

The phases multiplied it. `construct`, `finish`, `logical.count` and `bindings` each
walked the same two roots. The kept-value cache (`POOLED_VALUE_CACHE_BYTES`, 512 KiB)
overflows at about 1,000 changed files, so a large change read the same pages again in
every phase. C4 (1,000 edits in a 12-level tree) paid 1,164 inode value reads in
construct, 114 in finish, 1,084 in logical.count and 1,084 in bindings.

The reads depended on each other only by level: the captured change list already names
every changed directory and file. Adding 10,000 files in 3,942 directories (C4s4) took
26,740 round trips and 17.8 s on loopback; #815 estimated about 270 s at 10 ms.

## How the trips were cut

#810's per-source savings were **estimates** at 0.9 ms per round trip. The counts below
are **measured**, one sample, in the named commit's own base and head arms.

### #810: batch each source per window (merged in #820; optional F6 not built)

- **F1, bindings walk.** Before: `bindings` diffed each changed directory alone, one
  point read per side and per chain level (C4s4: 11,403 trips). Now the inode
  difference notes the changed directories (`ChangedDirectories`) and
  `directory_bindings` diffs them 128 at a time, both root pages of a window in one
  demand of at most 2 MiB. Measured C4s4 `bindings` 11,403 → 2,056 (`0e8c0a79ae`).
  Lives in `core/crates/layerfs-content/src/compare/tree.rs` (`directory_bindings`,
  `new_bindings`), called from `engine/crates/layerfs-store-service/src/store/hints.rs`
  (`collect_new_bindings`).
- **F2, publication waves** (write side). Before: a ~60 B similar file's ring proposal
  was often still unpublished in the same Save, so `register_ready` published it first,
  about 6 trips per file. Now the Save trials against kept bytes
  (`SAVE_PENDING_BASE_BYTES`) and holds winners to the end of the wave
  (`SAVE_DEFERRED_BYTES`). Measured B1 5,309 → 1,405, B6e 4,508 → 891 (base
  `e11d8c8d0f`); stored bytes changed slightly and were reported. Lives in
  `core/crates/layerfs-storage/src/save/select.rs` and `save/deferred.rs`.
- **F3, directory-rewrite base reads.** Before: one point read per base directory
  rewritten or removed (an lldb trace corrected the spec's entry point). Now a window's
  base roots are read in grouped demands of at most 256 pages (`RootsAhead`), each row
  decodes its fetched page (`fetched_root`), and chains are located a level at a time
  (`chains` in `core/crates/layerfs-storage/src/read/prefetch.rs`). Measured C4s4
  construct 8,514 → 2,021, B2 1,722 → 322 (`945393a43f`). Lives in
  `core/crates/layerfs-content/src/filesystem/directory/ahead.rs` and
  `sorted/finish.rs` (`apply_directory_changes`).
- **F4, inode values shared across phases.** Before: logical counting and bindings
  each walked both inode tables and read-ahead gave up early, so each changed leaf was
  a point read and a value lookup, twice. Now `logical.count` walks once
  (`logical_size_and_directories` in `core/crates/layerfs-content/src/filesystem/statistics.rs`)
  and hands the changed directories to `bindings`. The Save prepares a demand's leaves
  ahead with one value lookup per window (`prepare_leaves` in
  `core/crates/layerfs-storage/src/encoding/pool/read.rs`: 128 leaves, 1 MiB, the
  unchanged value cache). Measured with F1: C4 4,848 → 950 (`3f1326002c`).
- **F5, base-file reads.** Before: the Workspace reader asked each changed file's
  length (`provided_length`), then read it, also for `chmod` and `touch`. Now each
  captured inode page (1,024 rows) asks all lengths in one demand (`provided_lengths`)
  and fetches roots in batches of at most 4 MiB; a file whose overlay inherits every
  byte keeps its base root with no read (`files_kept`). Measured reader trips C3
  356 → 0, C4 726 → 1, B5e 384 → 6 (`961cfe242d`). Lives in
  `engine/crates/layerfs-workspace/src/construction/namespace/files.rs` and `base/batch.rs`.

### #815 Stage 1: joined locate and read (a measured miss)

The joined fetch (`b829bda882`) returned eligible metadata packs inside the existing
locate reply (`admit_joined_packs` in `core/crates/layerfs-storage/src/read/joined.rs`).
C4s4 went 568 → 569 trips, 472 statements in both arms; C4 146 → 147, the extra trip a
first-use prepare. Construction still issued each read when it reached it, so every
dependent wait stayed. The cost was the chain of waits, not the number of statements.

### #816 Stage 2: fewer waits per window (merged in #823)

- **Pipelined captured Save registrations.** Before: each Save publication was fenced
  BEGIN, lane, row registration and COMMIT; B1's 54 cost 216 trips, above its 150 cap.
  Now an eligible batch (≤ 4 MiB of pack bodies, no held lane) is one fenced flight,
  same lane and acknowledgement order. Measured B1 713 → 453 (`388bfe6c30`). Lives in
  `engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish_flight.rs`
  (`eligible`, `run`, `run_tail`), selected by `publish_captured` in `storage/provider.rs`.
- **Grouped inode chain and pooled-value reads.** The sorted inode merge looks at the
  next 1,024 serials and fetches up to 64 touched leaves in one demand (`Leaves::read`
  in `core/crates/layerfs-content/src/filesystem/sorted/read_ahead.rs`). A pooled-value
  demand above 4,096 ordinals sends two statements in one flight (`inode_values_pair`
  in `metadata/reads.rs`). No separate count; see the combined result below.
- **Reuse of captured base pages.** Pages the Workspace's captured base already holds
  are served from it, up to 2 MiB per demand; only missing pages go to the Save read
  port (`BasePageReader` in `core/crates/layerfs-content/src/filesystem/objects.rs`).
- **Canonical logical accounting.** The merge records changed file roots and weights as
  it runs, so `logical.count` resolves lengths for that root pair instead of walking
  again (`LogicalChanges` in `filesystem/statistics.rs`).
- **Binding transition.** C4s4 `bindings` still had 98 trips. Now the canonical
  directory merge records each binding as it consumes it, and the final inode merge
  proves each parent's rebuilt root before Commit uses the rows (`BindingChanges` in
  `core/crates/layerfs-content/src/filesystem/binding_changes.rs`). An early build of it
  measured 171 C4s4 trips against the prototype's 288 (`216df82fc2`, other sources).
- **Retained locators, fenced overflow writes.** Checked locators stay with the capture
  within 196 KiB (`CapturedLocators` in `core/crates/layerfs-storage/src/read/captured.rs`);
  overflow binding batches go as fenced flights (`publish_captured_tail`). C4s4 242 → 203
  cold (`db54e9d75f`).

Combined, v2 `7e6caa86b2` → `056e920166`: B1 729 → 270, C4s4 611 → 203, cold. #822 then
cut pack inserts into statements of about one pack (`PACK_LIMIT`, 256 KiB), same flight
(`pack_statements` in `engine/crates/layerfs-persistence/src/backend/postgresql/history/fused.rs`).

## How to spot it

- A loop that awaits one Store read per item: directory, file, inode page, pack, length.
- A call that returns a location, followed by a second call to read it.
- The same page or value read again in a later phase, often after a large change
  overflowed a cache budget.
- One write transaction per batch where the batches could travel in one flight.
- Per-phase trips that grow with the changed-file count at a fixed depth.
  `telemetry::phase_sql` (`engine/crates/layerfs-store-service/src/telemetry.rs`)
  returns each phase's `SqlWork` with `round_trips`; `store/commit.rs` names the phases.
- Writer and Workspace-reader trips counted apart, since they have different owners:
  B5e's reader trips barely moved in Stage 2 (26 → 25) while its writer trips halved.
- Loopback hides it: at 0.5–0.9 ms a trip, 5,000 trips look like a few seconds of CPU
  until the Store is remote. Count trips, not only time.

## What to do

- Collect the IDs the operation already knows, then ask for all of them in one statement
  (`= ANY($1)`) or one pipelined message (`pipeline` in
  `engine/crates/layerfs-persistence/src/backend/postgresql/session/pipeline.rs`) per level.
- Cut each batch into windows bounded by rows or bytes, never sized to the whole change.
- Read each page once per operation and hand it to later phases in a bounded cache, or
  record the fact where it is computed (bindings, logical weights).
- Send dependent writes as one fenced flight that keeps the lane, acknowledgement order
  and failure custody of the ordinary path.
- Take the batched path only when the provider reports that a lookup costs a round trip
  (`batches_locations`, `batched_chain_reads`). Leave the SQLite path as it is.

## How to verify a change

- Count trips per phase, writer and readers apart, on a cold and a warm Workspace. Name
  the cache state with each count; see
  [state the cache behind a cap](../measurement/state-the-cache-behind-a-cap.md).
- Run a latency arm with netem inside the PostgreSQL container only, never on the host
  network. The added time should stay near or below trips × delay: B1 on `262ef260be`
  added 2,199 ms at +10.6 ms for 270 trips (at most 2,862 ms).
- Require byte identity: the same root and the same Save `stored_bytes`, `inserted`,
  `reused` and `packs` as the base arm, on both backends.
- Run the pinned count tests in `engine/crates/layerfs-store-service/tests/`:
  `commit_batched_reads.rs`, `commit_batched_base_files.rs`,
  `commit_batched_directories.rs`, `commit_similar_tiny_files.rs` and
  `mount_round_trips.rs`. Their PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL`.
  Every new flight needs drop and kill sweeps, as in `commit_fault_wide_*`,
  `commit_fault_directories.rs` and `layerfs-persistence`'s
  `postgresql_captured_save_fault.rs`.
- Report peak RSS beside time. A batch-sized statement grows the connection's buffers.

## Limits

- Depth is the floor. Without a format change, level d+1's IDs are known only after
  level d is read.
- Joining locate and read into one statement saved nothing in Commit construction
  (#815 Stage 1): there the locates were already batched or cached, so no wait went
  away. It helps only on a path where the locate is still its own sequential trip,
  for example an uncached read that locates and then fetches each object. Count that
  path's trips first. The gain comes from fewer sequential waits, not fewer statements.
- A large statement leaves its buffer on the connection: tokio-postgres keeps its request
  and write buffers at their largest size. Split large inserts into bounded statements
  inside one message ([#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822)).
- Reads by a cold Workspace still cost one trip each. Batching them per window
  (plan-then-fetch) was proposed in #815 and set aside as a small gain
  ([#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824)).
- A deleted subtree is still read whole (B2). Deleting 11,439 entries reads 1,417
  directory pages and 1,649 objects in 33 waves to write 4 objects; construct is 92 of
  114 trips. SQLite takes 0.90 s for the same work, so this is local work, not latency.
  See [costly patterns](costly-patterns.md).

## Evidence

PostgreSQL round trips per Commit, cold local PostgreSQL, one sample per arm:

| Cell | Before #810 | After #810 (`7e6caa86b2`) | After #816 (`056e920166`) |
| --- | ---: | ---: | ---: |
| B1 npm install, 10,002 files | 5,309 | 729 (writer 722) | 270 (writer 263), 1.72 s |
| C4s4 add 10k files in 3,942 dirs | 26,740 | 611 (writer 573) | 203 (writer 169), 2.1–3.6 s |
| B5e overwrite 1,000 random | 898 | 90 (writer 64) | 53 (writer 28), 0.26 s |
| C1 mixed | 454 | 82 (writer 50) | 58 (writer 27), 0.45–0.59 s |
| B2 delete `node_modules` | 1,723 | 137 | 114 (writer 101), 1.08 s |
| C4 1,000 deep edits | 4,851 | — | 91 (writer 49), 1.0 s |
| C3 `chmod`/`touch` 1,000 | 823 | — | 36 (writer 16), 0.15 s |
| B6e overwrite 1,000 tiny similar | 4,509 | — | 52 (writer 27), 0.34 s |

"Before" is the [#810 issue](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810) at
`ca4be0bf3c`, an earlier harness with statement logging on: compare by order of magnitude.
The rest: #816 [merge results](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418)
and [closing note](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202676223);
[#810 closing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7203083110)
(C4, C3, B6e, B2 and its waiver); [#815 closing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815#issuecomment-7202838625)
(Stage 1). Per-fix counts are in the commit messages named above. Receipts are on the
owner's machine under `2026-11-26-commit-batching/` (`bench-262ef260be/`, `bench-810-qualify/`).
