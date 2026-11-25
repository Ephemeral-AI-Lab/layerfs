# Commit read batching: per-file and per-directory round trips

> **Status:** Integrated candidate with open qualification gates. 2026-11-26. Issue [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810). Local copy of the doc https://claude.ai/artifact/HV5FkH17y4w1SmuGk5WyYB; if the two differ, this file wins.

## Summary

A PostgreSQL Commit pays one round trip for each changed file, directory or inode page it touches. An npm install of 10,002 files costs 5,309 round trips and 5.9 s, against 2.1 s on SQLite. Six fixes in the Rust read and Save paths should cut those shapes to tens of round trips and bring PostgreSQL close to SQLite's client CPU time. None of them changes the Store format.

- **Base:** `layerfs_v2_0_0` at `ca4be0bf3c`, with #768 merged.
- **Already cheap:** one-file Commits and typical agent edits cost 3–20 round trips.
- **Scope:** the Commit phases before the publication lane (construct, finish, logical.count, bindings). The lane itself is one pipelined message and stays as it is.
- **Storage:** F1 and F3–F5 only change reads, so stored bytes must stay identical. F2 and F6 change encoding choices within the existing format, so they need a storage comparison before they are kept.
- **SQLite:** gains less, roughly 10–40% depending on the shape, from not decoding the same data twice.

**Scope decision (owner, 2026-11-26): conservative only.** This work is F1–F5, with F6 optional. There is no aggressive phase:

- no Store format change;
- no server-side SQL tree walk;
- no parallel construction or second connection;
- no cache sized to a whole Commit;
- no stretch targets.

The terminal pass condition below is the whole contract.

## Measurements

Round trips grow with the change, not with the tree: a 4 KB edit inside a 500 MB file costs 18 round trips, but `chmod` on 1,000 files costs 823. All cells were run at `ca4be0bf3c` with the store-service harness, one sample each, on 2026-11-25. PostgreSQL ran in a private postgres:17 container with `log_statement=all`, which inflates its times somewhat; the round-trip counts are exact. Host load was 3.9–12.9.

| Cell | Change in the measured Commit | PG round trips | PG ms | SQLite statements | SQLite ms | Stored |
| --- | --- | --- | --- | --- | --- | --- |
| Add 10k into 3,942 directories | 10,000 new files spread over a deep tree | 26,740 | 17,780 | 31,839 | 4,139 | 13 MB |
| B1 | npm install, 10,002 files | 5,309 | 5,948 | 7,077 | 2,125 | 18.7 MB |
| C4 | 1,000 edits across a 50k-file, 12-level tree | 4,851 | 4,993 | 6,134 | 876 | 147 KB |
| B6e | Overwrite 1,000 tiny (~60 B) near-identical files | 4,509 | 4,572 | 4,722 | 828 | 107 KB |
| B2 | Delete `node_modules` | 1,723 | 1,655 | 1,733 | 813 | 0.5 KB |
| B5e | Overwrite 1,000 files with random content | 898 | 2,992 | 1,062 | 320 | 2.1 MB |
| C3 | `chmod` and `touch` on 1,000 files | 823 | 1,149 | 806 | 392 | 22 KB |
| A6 | Rewrite a 500 MB file | 908 | 13,727 | 13,809 | 8,363 | 526 MB |
| B4 | Add 1,843 build-output files | 479 | 3,428 | 2,203 | 2,128 | 85 MB |
| C1 | 500 edits, 200 new files, 4 KB into a 100 MB file | 454 | 845 | 651 | 238 | 327 KB |
| A1 | Add one 100 MB file | 87 | 1,402 | 738 | 408 | 105 MB |
| B3 | 50 small source edits | 20 | 53 | 68 | 22 | 6.3 KB |
| A3 | 4 KB edit inside a 500 MB file | 18 | 54 | 34 | 37 | 9.8 KB |
| A5 | 4 KB edit inside a 50 MB file | 15 | 42 | 29 | 14 | 7.4 KB |
| C2 | Rename a directory of 2,000 files | 8 | 29 | 22 | 3 | 0.6 KB |
| D2 | B3 as the 50th Commit in a row | 3 | 52 | 57 | 26 | 6.3 KB |

**Already cheap, no work needed:**

- localized edits in large files: A3 against A5 shows ten times the file size adds 3 round trips, so cost grows with manifest depth, not with the number of chunks;
- small source edits, renames, and steady-state Commits;
- large new files are throughput-bound: round trips are at most 6% of A1, A2 and A6.

Other cells are in the full report: A2, A4, B6, B6n, D1.

## Where the round trips come from

Six sources issue one read or one small transaction per changed file, directory or leaf page. Each one runs before the lane. The file references were checked at `ca4be0bf3c`; the call paths come from lldb backtraces and the statement logs.

| Source | Issued once per | Session | Entry point | Biggest cell |
| --- | --- | --- | --- | --- |
| N1 Bindings walk | Changed directory version, and growing with directory size | Writer / Save reader | `engine/crates/layerfs-store-service/src/store/commit.rs:359` → `store/hints.rs:66` `collect_new_bindings` → `core/crates/layerfs-content/src/compare/tree.rs:187` `new_bindings` → `:225` `binding_changes` / `list_directory` | 10k add: 15,570 locates + 1,558 values reads |
| N2 Directory-rewrite base reads | Touched directory (about 2), removed directory (1) | Writer / Save reader | `engine/crates/layerfs-workspace/src/construction/*/normalize.rs:143` `names` → `core/crates/layerfs-content/src/filesystem/sorted/finish.rs:147` `apply_directory_changes` (path inferred) | 10k add: 7,359; B2: 1,443 |
| S2 Inode value-block reads | Inode leaf page, repeated in each later phase | Writer | `core/crates/layerfs-storage/src/encoding/pool/read.rs:443` `leaf_parts_with_caches` and `:666` `leaf_canonical_with_caches`; callers `save/wave.rs:309`, `save/pooled.rs:181` `pooled_bases`, `store/commit.rs:820` `logical_after`, `store/hints.rs:66` | C4: 1,164 construct + 114 finish + 1,084 logical.count + 1,084 bindings |
| S3 Publication waves | Similar tiny (~60 B) file whose proposed base is still pending in this Save | Writer | `core/crates/layerfs-storage/src/save/select.rs:19` `offer` → `seal_pending` → `save/publication.rs:29` `register_ready`, about 6 round trips each | B1: 614 waves |
| S1 Base-file reads | Changed file | Workspace reader | `engine/crates/layerfs-workspace/src/base/file.rs:21` → `base/client.rs:206` `provided_length` (keeps nothing) → `locate_chains`; then `base/view.rs:33` → `ranges` | C4: 725 |
| N3 Metadata-only changes read content | `chmod` / `touch` file | Workspace reader | Same path as S1, reached with no content change | C3: 375 |

**Already in the code and reusable:**

- `inode_values_ahead` (`engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs:292`) reads value blocks in batches and keeps them.
- `locate_chains` and `ranges` already take arrays.
- `sorted::open` reads pages ahead, and `prepare_leaves` batches leaf bodies.
- #758's `hints.page` groups reads by page.
- `Session::pipeline` (`session/pipeline.rs:182`) folds several statements into one message.
- The kept-value cache is `POOLED_VALUE_CACHE_BYTES` = 512 KiB (`core/crates/layerfs-storage/src/store/policy.rs:140`). It overflows at about 1,000 changed files, which is when later phases start reading again.
- `select.rs:52-70` already drops a pending proposal when a PREFIX record cannot beat FULL (layerfs-project #67), yet tiny files still trigger waves. Why is not established.

## The fixes

Every fix follows one pattern: collect the ids first, then read them in one batched call per window. None adds a table, a column or a format version. Savings are estimates at 0.9 ms per round trip on this Mac's Docker loopback.

| Fix | Source | Change | Round trips after (estimate) | Estimated PG saving | Storage risk |
| --- | --- | --- | --- | --- | --- |
| F1 | N1 | Batch the bindings walk: prefetch every changed directory version's objects in one windowed read, or reuse the objects construct already decoded | 10k add: 17k → about 10 | about 15 s on the 10k add, 1.2 s on C4, 0.6 s on B1 | None |
| F2 | S3 | No wave per similar file: defer pending-base proposals to the next seal, or let `publish_rows.sql`'s in-batch recursion resolve the chain, or delta against pending bytes | B1: 5.0k → about 50 | about 4.5 s on B1, 3.8 s on B6e and B6n | **Yes**: delta-base choice |
| F3 | N2 | Batch the directory-rewrite base reads in `apply_directory_changes` | 10k add: 7.4k → about 20; B2: 1.4k → about 10 | about 6.6 s on the 10k add, 1.3 s on B2 | None |
| F4 | S2 | One values read per leaf set via `inode_values_ahead`, kept across construct, finish, logical.count and bindings; merge the two difference walks (`logical_after`, `collect_new_bindings`) over the same two roots | C4: 3.4k → about 10 | about 3.1 s on C4, 0.4 s each on C1, C3, B5e | None (bounded memory, see constraints) |
| F5 | S1, N3 | Batch base-file `locate_chains` and `ranges` per window; skip base reads entirely when only metadata changed | C4: 725 → about 10; C3: 375 → 0 | about 1.2 s on C4, 0.35 s each on C3, B5e | None |
| F6 | Chunk publication | Larger chunk waves for big new files | 500 MB: about 370 → 60 | about 0.3 s per 500 MB | **Yes**: packing |

**Notes for the implementer:**

- **F1:** the cost also grows with directory size, from 8.9k to 15.6k reads over four setup Commits on the same tree. Check that the batched form grows with the changed entries, not the whole directory.
- **F2:** first establish why the existing #67 guard does not stop waves for ~60 B files. The proposals form a chain (each names the previous similar file), so a proposal cannot just be dropped without measuring the storage effect.
- **F4:** the cache must stay bounded. Read ahead in windows, and do not raise `POOLED_VALUE_CACHE_BYTES` to cover a whole Commit.
- **F5:** a metadata-only change must still detect real content changes. Skip only when the file's content identity is unchanged.

## Expected speed

On the shapes dominated by round trips, PostgreSQL should get about 3–7 times faster and end within about 1.5 times SQLite's time. The per-shape ranges and ceilings are in the terminal pass condition below.

- **Biggest gains:** C4 (wide edits) and B6e (tiny similar files), about 7 times; the 10k add, npm install and `rm`, about 4–5 times; `chmod`/`touch`, about 3 times.
- **Smaller gains:** C1 (mixed), about 2 times. B5e gains about 1.4 times, because most of its PostgreSQL time is not round trips (an open question).
- **No change expected:** cheap shapes and large new files.
- **With network distance the gain multiplies:** each removed round trip also saves the full network delay. At 1 ms, an npm install would go from roughly 11 s to under 2 s.
- **SQLite:** about 10–40% faster depending on the shape, from not decoding the same directories and values twice and from fewer publication waves. Its remaining time is construct CPU, which this work does not touch.

## Terminal pass condition

The work passes only when every round-trip ceiling, time ceiling, storage check and test below holds on the final commit. Round-trip ceilings are exact counts and must be met. Time ceilings are judged on one sample per arm. A miss is reported as a miss with its diagnosis and goes to the owner; it is never rerun, relabeled or passed.

### Round trips and time, per shape

The "Before" columns are the 2026-11-25 receipts at `ca4be0bf3c`, recorded with statement logging on. Re-run the base arm with logging off before judging time. "Expected" is the estimate behind the fix plan; "Pass" is the ceiling.

| Cell | PG round trips: before → pass | PG time: before | PG expected | PG pass | SQLite time: before → pass |
| --- | --- | --- | --- | --- | --- |
| Add 10k into 3,942 directories | 26,740 → ≤ 1,000 | 17.8 s | 3–5 s | ≤ 5.0 s | 4.1 s → ≤ 4.1 s |
| B1 npm install, 10,002 files | 5,309 → ≤ 1,000 | 5.9 s | 1.0–1.5 s | ≤ 2.0 s | 2.1 s → ≤ 2.1 s |
| C4 1,000 edits, deep 50k tree | 4,851 → ≤ 125 | 5.0 s | 0.6–0.9 s | ≤ 1.0 s | 0.88 s → ≤ 0.88 s |
| B6e overwrite 1,000 tiny similar files | 4,509 → ≤ 125 | 4.6 s | 0.5–0.8 s | ≤ 1.0 s | 0.83 s → ≤ 0.83 s |
| B2 delete `node_modules` | 1,723 → ≤ 100 | 1.7 s | 0.3–0.4 s | ≤ 0.5 s | 0.81 s → ≤ 0.81 s |
| C3 `chmod`/`touch` 1,000 files | 823 → ≤ 125, with 0 Workspace-reader content reads | 1.1 s | 0.3–0.4 s | ≤ 0.5 s | 0.39 s → ≤ 0.39 s |
| B5e overwrite 1,000 random files | 898 → ≤ 125 | 3.0 s | 2.0–2.3 s | ≤ 2.3 s, and the non-round-trip gap explained | 0.32 s → ≤ 0.32 s |
| C1 mixed | 454 → ≤ 100 | 0.85 s | 0.4–0.5 s | ≤ 0.6 s | 0.24 s → ≤ 0.24 s |
| A6 rewrite 500 MB | 908 → ≤ 908 (≤ 300 if F6 lands) | 13.7 s | unchanged | ≤ base + 5% | 8.4 s → ≤ base + 5% |
| A1, A2, B4 large new files | no increase (lower if F6 lands) | 1.4 / 6.8 / 3.4 s | unchanged | ≤ base + 5% | ≤ base + 5% |
| B3, A3, A5, C2, D2 cheap shapes | no increase: ≤ 20 / 18 / 15 / 8 / 3 | 29–54 ms | unchanged | ≤ base + 5 ms | ≤ base + 5 ms |

Any other cell must meet PG round trips ≤ 25 + 0.1 × changed files + 0.1 × changed directories.

### Storage, memory and correctness

- **Storage, read-path fixes (F1, F3–F5):** every cell's resulting root and the Save's `stored_bytes`, `inserted`, `reused` and `packs` must be identical to the base arm.
- **Storage, F2 and F6:** report raw deltas against B1 18,651,894; B4 85,413,707; B6 239,287; B6e 106,516; B6n 82,398; A2 525,798,462 bytes, and chain depths. Pass is at most +5% stored bytes in each cell, with no chain deeper than the current limit. More than that needs an owner decision; no fix buys speed with storage.
- **Memory:** peak RSS in any cell grows by at most 10% over the base arm. Read-ahead stays windowed; no cache sized to a whole Commit.
- **Correctness:** the same logical tree and Branch head as the base arm in every cell, on both backends.
- **Tests:** counted round-trip pins in `layerfs-store-service` for C4, B2, C3 and B6e (or equivalent small shapes). Drop and kill sweeps at every server message of any new or changed batched message. A seeded SQLite–PostgreSQL equivalence run. The existing persistence, store-service, daemon and API suites pass with NOT_RUN 0 against a real PostgreSQL server. Clippy `--workspace -D warnings` is clean.
- **Independent check:** one independent review and one independent outside-in test round find no open product bug.

### Rules

- One sample per arm, base and head in ABBA order. Statement logging off for timing; counts may come from logged runs.
- Normal SQLite profile, `LAYERFS_CONSTRUCTION_WORKERS=1`, host load recorded, no waiting for load to drop.
- Every command and test under 120 s. No retry, replay or busy handler in product code.
- A production LOC line in every commit, and docs updated: `docs/architecture/engine/store/postgresql.md`, `sqlite.md`, `engine/commit/`.

## Work order, risks and open questions

Build the four read-path fixes first, each as its own counted commit with storage identity checked, then the two storage-risk fixes.

1. **F1** (bindings walk): the largest saving, and it changes no stored bytes.
2. **F3** (directory-rewrite reads).
3. **F4** (values kept across phases, one difference walk).
4. **F5** (batched base reads, metadata-only skip).
5. **F2** (publication waves): start by establishing why the #67 guard misses ~60 B files, then pick the design by measured storage.
6. **F6** (larger chunk waves): optional; land it only if storage stays within the pass condition.
7. Run the whole matrix once at the final commit, then an independent review and an independent outside-in test round.

**Risks:**

- **The estimates come from one sample per cell** with statement logging on. The 0.9 ms per round trip overstates cost in the biggest cells: the report's savings for the 10k add sum to more than its measured time.
- **F2 and F6 can change storage.** Chained proposals mean a dropped base can make later files encode as FULL.
- **Batching can raise memory** if a window covers a whole Commit. Read-ahead must stay windowed.
- **Branch overlap:** local branch `claude/writer-reopen` (not merged) changes PostgreSQL session, pipeline and lane code. Expect merge conflicts in `session/pipeline.rs` and `lane/lane.rs` if both land.

**Open questions for the implementer to settle:**

- Why do ~60 B near-identical files trigger publication waves when 0.1–4 KB similar files trigger none?
- B5e spends about 2.2 s on PostgreSQL outside round trips, against 0.32 s on SQLite. Is that statement payload size, statement logging, or client CPU?
- N2's call path was inferred, not traced. Confirm it with a backtrace before batching it.

## Handoff

Start from the current tip of `layerfs_v2_0_0`. All receipts were taken at `ca4be0bf3c`, so re-run the base arm on the tip you start from before comparing.

**Local copy:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/commit-read-batching/SPEC.md` (this file), untracked in the main checkout. The next agent adds it to its branch and works from that file. The handoff prompt is also in [HANDOFF-PROMPT.md](HANDOFF-PROMPT.md).

**Receipts** (outside the repository, kept on this machine):

| What | Where |
| --- | --- |
| Shape matrix: report, table, per-cell receipts, harness `zzshape_commit.rs` | `~/.cache/layerfs-measure/2026-11-25-commit-shapes/` (`REPORT.md`, `table.md`, `cells.json`, `runs/`, `harness/`) |
| Per-statement diagnosis: statement log with phase markers, lldb backtraces, harness `zzrt_commit.rs`, parsing scripts | `~/.cache/layerfs-measure/2026-11-25-commit-rt/` (`pg-change.log`, `lldb100.log`, `lldb1000.log`, `parse.py`, `phases.py`) |
| Tree-size and history scaling (one-file Commit flat from 10k to 100k files and 1 to 1,001 Commits) | `~/.cache/layerfs-measure/2026-11-25-commit-scaling/` (`README.md`, `tables.txt`, `runs/`) |

**Environment:**

- Use the parallel worktree launcher (`tools/worktree.py`) for a private Cargo home and target. Host cargo is `+1.85.1`.
- **PostgreSQL:** your own `postgres:17` container with trust auth, published on 127.0.0.1 only. Turn on `log_statement='all'` inside it for count runs, and off for timing. Never change host network settings.
- The harnesses are untracked test files: copy one into `engine/crates/layerfs-store-service/tests/`. They use `telemetry::mark_phases` and the writer/reader `SqlWork` split. Add counted pins as real tests; remove the scratch harness before committing.
- This Mac has no `timeout`: wrap each command in a 120 s wall-timeout script.

**Related:** #768 (merged round-trip work), layerfs-project #67 (the existing one-wave-per-similar-file guard), and the earlier [round-trip spec](https://claude.ai/artifact/5MQxCpVy21vK8oMv5Cdb54).

## Handoff prompt

The prompt for the next agent is in [HANDOFF-PROMPT.md](HANDOFF-PROMPT.md), the same text as the doc's "Handoff prompt" section. It runs four implementation lanes in parallel, then verifies, then loops on fixes until the terminal pass condition holds.


## Pickup results (2026-11-26)

The pickup branch integrates F1–F5. The measured product is `0e88f73f4b`
with the shape binary built at `66f325cd10`. PostgreSQL and SQLite Normal
used matched Phase 0 fixtures and statement logging off.

| Cell | PostgreSQL round trips | PostgreSQL Commit | SQLite Commit | Disposition |
| --- | ---: | ---: | ---: | --- |
| B2 | 1,722 → 93 | 1.601 → 0.904 s | 0.838 → 0.768 s | PostgreSQL latency miss |
| C4 | 4,848 → 146 | 4.295 → 1.137 s | 0.859 → 0.602 s | PostgreSQL count and latency miss |
| C4s4 | 568 candidate | 3.541 s candidate | 3.515 → 2.656 s | PostgreSQL base exceeded the 25 s command cap. Pair incomplete |
| B1 | 5,309 → 713 | 4.935 → 2.771 s | 2.040 → 1.907 s | PostgreSQL latency miss |

C4 spends 79 trips in construction, 5 in finish, 61 in logical counting
and 1 in fused publication. The source-pinned logged diagnostic attributes
logical counting to 37 locates, 17 value reads and 7 span reads. Its
observed count remains 21 trips above the spec ceiling.

The count pins, seeded backend equivalence, concurrent shared-Branch case,
binding-overflow boundary, 4,200-directory correctness case and 13
PostgreSQL fault binaries passed. The matrix, per-phase traces, additional
regression rows, build hashes and outstanding qualification checks are
recorded in the local campaign's `PICKUP-REPORT.md` under
`~/.cache/layerfs-measure/2026-11-26-commit-batching/`.
