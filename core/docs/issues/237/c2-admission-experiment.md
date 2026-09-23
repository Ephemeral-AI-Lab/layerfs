# #237: bounded C2 multi-group admission experiment

> **Status:** Research; informative and not a product contract.

## Prospective design and measurement contract

This is a separate worktree based on `acb17d287`. The control is this source without the treatment. The candidate changes only Core C2 placement on a Save preparation wave; SQLite `page_size` remains **4,096 B** and source data stays cold for each measured public Init. Fixture preparation may be reused, but the timed operation must scan and read its own source and write a fresh Store. One release-profile public `namespace-10000` sample per arm, same harness and fixture identity, fresh output paths. Both arms use the root task's exact fixed-identity research driver from `3c2c8d793`, SHA-256 `124e9323d2580cb2a7cbc14ba54923f7e240ea7307e928a02b55f6279ccebbbe`, with `--fixed-operation-identity --verify`; retain its `operation-identity.json` in each raw result. The existing full reopened verifier runs after `performance_command_wall_ns` and has its unchanged 5 s watchdog; its cost never enters public throughput, and any failure or timeout is retained. Coordinate the host window with the parent before either timed call. Record any failed arm; do not select a replacement sample. Changed wave/worker/transaction policy, if later explored, is a **different** preregistered product treatment, not part of this comparison.

The candidate will retain the existing `512`-object/`4 MiB` wave and its one transaction and Store arbitration. Only ordinary/native/whole-file groups sealed **during** that wave may wait in one FIFO queue. The queue holds at most one lane at a time, at most **256 KiB of encoded group bodies** and **512 locator rows**; a lane switch or projected limit flushes it. Pooled and singleton groups stay on the immediate path. Each flush gives all queued groups to the existing `LanePlacement::select_many`, writes each resulting pack increment once, and passes all locator rows to the existing engine-limited `insert_objects`. Every group stays in original seal order, including across lane switches. The queue cannot cross a wave, COMMIT, final publication or a same-save read.

**Flush barriers:** `seal_pending` must flush when a requested ID is in an open or queued group; same-wave duplicate/reuse then looks up and compares its written row. Whole-file predecessor/advisory or winner-cache selection must call this demand path before acquiring a base. `Availability` must regard queued members as accepted while direct references are validated. At the wave's end, place and insert every queued row before collision validation, candidate-index flush and COMMIT. All writes and row INSERTs remain in the wave's existing transaction; failure terminates or rolls it back under the existing cleanup path. On finish or abandonment there can be no uncommitted queue. Cache invalidation remains attached to each pack increment. Count new queue bodies, member rows, selected increments and row statements; report Store physical bytes, pack occupancy and readback, including #229 sparse-pack risk.

The D11 diagnostic found **7,696** group placements and **7,750** object-row INSERTs for the 10k file Save, but this design's achievable reduction is unknown. Same-save dependency flushes may erase most grouping; no speedup is assumed. The D12 owner charged **123.805 ms** to pack writes and **71.157 ms** to row INSERTs, so merely removing those calls cannot explain the roughly **0.739 s** still required after the C1 prototype for 518.8 MB/s. A perf result must be interpreted with owner/receiver wait and CPU, not added nested counters.

Success requires a returned C5 root and positive, separately reported caller-speed change with **no worse** exact fixed-scope root, full reopened manifest readback, pack capacity/used bytes, apparent/allocated Store bytes and sparse-history readback. The fixed-operation-identity driver gives both arms the same stack/scope IDs; root comparison is now a preregistered check. `source-cache-uncontrolled-v1` remains a diagnostic cache label until the harness itself qualifies all required cache state; zero-resident payload sidecars alone do not grant an admission PASS. Focused external tests cover bounded multi-group publication, queued-predecessor resolution and reopened bytes; the existing storage tests cover the adjacent same-save read, collision and multi-writer boundaries.

## Attempt ledger

The prospective design was implemented on branch `codex/issue237-c2-admission`.
The measured candidate source is `60ced47d1`; its product source is identical
to architecture-document amendment `3be628221`. The measured control is
`d98cca6fe`. Both product trees were clean when timed. The harness seal was
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
and the same prepared manifest SHA-256 was
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
The control created the fixture; the candidate reused only that prepared source
for setup. Each arm independently rehashed and invalidated all source files,
then performed a nonfaulting whole-input residency check just before its own
public timer. Both checks in both arms found **0 resident pages out of 27,503**
over the 10,000 files and 300,000,000 bytes; the recheck-to-timer gaps were
**6.566 ms** and **1.020 ms** ([control](evidence/c2-admission/raw/control/cold-recheck.json),
[candidate](evidence/c2-admission/raw/candidate/cold-recheck.json)). These sidecars
prove source-payload residency, not directory/inode metadata cache state. The
runner's `source-cache-uncontrolled-v1` label remains in both receipts.

The exact commands, each issued once in this worktree, were:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --out benchmark-results/fs-bench-pro/issue237-c2-control-d98cca6fe-a --fixed-operation-identity --verify
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --out benchmark-results/fs-bench-pro/issue237-c2-candidate-60ced47d1-a --fixed-operation-identity --verify
```

The fixed operation identity was the same in both arms: stack
`1b88dfdc2ffd3c6b0b0ac0ec4c064284`, scope seed
`3afa1d39bd5ede7b5a0a40d71e50160dde8e897fa9865929300cb01b95bbdd18`
([sidecars](evidence/c2-admission/raw/control/operation-identity.json)).
The complete full reopened verifier ran **after** the performance command wall
was frozen. It checked all **10,101 paths, 10,000 files and 300 MB** through
the real C1/C2 read path, plus C5 history root, and passed in **2.194 / 2.172 s**
under its unchanged 5 s watchdog. Both public calls returned the **same exact
root** `e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.

| Observation | Control | Candidate | Candidate − control |
| --- | ---: | ---: | ---: |
| Public Init | 1.597006084 s | 1.468407708 s | **−0.128598376 s (−8.05%)** |
| Caller throughput, 300 MB decimal | 187.85 MB/s | **204.30 MB/s** | +16.45 MB/s |
| Complete performance command | 2.533916291 s | 2.431370375 s | −0.102545916 s |
| Service + daemon lifecycle CPU, user + system | 2.181343 s | 2.093421 s | −0.087922 s (−4.03%) |
| Full reopened verifier, outside performance command | PASS, 2.193813 s | PASS, 2.172006 s | Both complete |
| Telemetry | **INCOMPLETE**: one daemon event lost | PASS | Control failure retained |
| Cleanup | Daemon/Service PASS; stderr retained | Daemon/Service PASS | Control stderr retained with failure |
| Overall receipt | **INCOMPLETE** | **INELIGIBLE** | Neither is admission PASS |

The raw [control](evidence/c2-admission/raw/control/receipt.json) and
[candidate](evidence/c2-admission/raw/candidate/receipt.json) receipts,
performance samples, build identities, verifier outputs, telemetry and cold
sidecars are retained under `evidence/c2-admission/raw/`. The control's lost
daemon telemetry event left only 3 of 4 expected daemon events, so it cannot
be promoted after the fact. The candidate is `INELIGIBLE` because the runner
has no fully declared cold-cache contract, despite its successful payload-page
sidecars. The **8.05% is one raw matched-route observation**, not a gate PASS,
median, or demonstrated stable saving. No arm was repeated to replace a status
or choose a better time.

### Physical Store and actual counts

The two closed 4 KiB-page Stores were scanned read-only with the existing
[`geometry.py`](evidence/c1-direct/geometry.py). The complete Store files stay
under the named private `benchmark-results` paths in the commands above;
their SHA-256 hashes and the reproducible per-save SQL counts are in the
[control geometry](evidence/c2-admission/control-geometry.json) and
[candidate geometry](evidence/c2-admission/candidate-geometry.json). The
SQL group count is `SELECT count(*) FROM (SELECT save_id, pack_id, group_number
FROM objects WHERE save_id=1 GROUP BY save_id, pack_id, group_number)`.

| Closed-Store measure | Control | Candidate | Candidate − control |
| --- | ---: | ---: | ---: |
| Apparent file bytes | 333,910,016 | 333,660,160 | −249,856 |
| Allocated bytes (`st_blocks × 512`) | 335,609,856 | 335,609,856 | 0 |
| Pack BLOB capacity | 331,087,872 | 330,825,728 | −262,144 |
| Declared pack used bytes | 305,973,682 | 305,969,635 | −4,047 |
| Capacity minus declared used | 25,114,190 | 24,856,093 | −258,097 |
| Pack rows, all saves | 1,263 | 1,262 | −1 |
| Physical groups, file Save 1 | 7,701 | 7,710 | +9 |
| Canonical objects, all saves | 24,683 | 24,683 | 0 |
| Ordered object-ID SHA-256 | `4a9f14a45482…` | `4a9f14a45482…` | identical full digest in geometry files |
| SQLite `page_size` | 4,096 | 4,096 | unchanged |

File Save 1 held 24,364 object rows in both arms; prerequisite/tree Saves held
11/308. The extra nine groups and one fewer pack demonstrate that physical
grouping is not identical across concurrent producer schedules; they cannot
alone attribute the small Store-size difference to the C2 queue. The public
`SaveOutcome` was not emitted by either run. A closed Store reveals final pack
and group counts, **not the number of `write_pack` invocations or object-INSERT
SQL statements**; those call counts are **NOT_MEASURED** for this pair. Do not
substitute D11's 7,696 placements / 7,750 statements from another instrumented
source identity or infer one call per physical group in the candidate. The
candidate's existing `select_many` can write several queued groups to one pack
increment, but its actual 10k count still needs a distinct count-driven
diagnostic if a follow-up design decision depends on it.

The observed dense 10k Store was no larger on the candidate, and the canonical
root and full manifest readback matched. This is **not** a #229 sparse-history
compactness proof. No matched sparse lane with fixed advisory/depth policy and
full reopened readback was run for this prototype; its status remains NOT_RUN.

### Focused implementation checks and decision

The external [`c2_bulk_admission.rs`](../../../crates/layerfs-storage/tests/c2_bulk_admission.rs)
passed 4/4. It exercises bounded multi-group publication and reopened bytes,
a queued whole-file predecessor before delta selection, an exact duplicate and
lane switch while groups wait, a >512-object wave/commit followed by a
same-save public read, and abort after private queued-write output. The
existing `delta_payload`, `pack_locator`, `multi_writer` and `visibility`
targets also passed **15/15, 10/10, 5/5, 9/9** at the final product source.
`SaveOperation::read_batch` cannot be called externally *during* a queued wave:
the `accept` call owns that synchronous wave and drains its queue before
returning. The predecessor test reaches the in-wave read-dependent barrier,
and the wave test checks the externally reachable read immediately after
commit. These tests do not substitute for the full product workspace checks or
the sparse-history proof.

After the product/document amendment, `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked -p layerfs-storage --tests` passed all storage test
targets; `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p
layerfs-storage --all-targets -- -D warnings`, `cargo +1.85.1 fmt
--manifest-path core/Cargo.toml --all --check`,
`python3 core/tools/check_product_boundary.py`, and
`python3 -m unittest discover -s core/tools -p 'test_*.py'` also passed (six
boundary-tool tests). Full Core workspace test/Clippy and the #229 sparse
history lane were **not run** in this isolated C2 experiment; the root task
owns the integrated final-source checks. The architecture change is in
[`13-physical-writing.md`](../../architecture/13-physical-writing.md) in the
same commit as the C2 product source.

This candidate is a real, bounded **research speed signal** with passing
canonical readback and unchanged 4 KiB pages. It has not achieved the historical
518.8 MB/s: the candidate's 1.468408 s is **0.890163 s** above that 0.578245 s
time. Preserve both old `INCOMPLETE`/`INELIGIBLE` labels and keep the next 10k
experiment aimed at the remaining file-save critical path. A combined C1+C2
source needs its **own** declared performance and readback row; subtracting
this 128.6 ms from a different C1 sample would invent a result.
