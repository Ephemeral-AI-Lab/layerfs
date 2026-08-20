# Retained 100 MiB Full-Create Lifecycle

## Evidence identity

This is the currently retained **M3 experimental full-create baseline** used as the optimization anchor for Phase 4.5. It describes the measured lifecycle shape and timings; it is not a measurement of later, unmeasured M4.5 dirty-tree changes.

| Evidence | Exact identity |
|---|---|
| Measured Git checkpoint | `c96b5396e98db523b9a983df4ec80fdedfa971c1` (`checkpoint: record WP4 optimization phase breakdown`) |
| Measured implementation diff SHA-256 | `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb` |
| Frozen release executable SHA-256 | `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c` |
| Documentation checkpoint that freezes this custody | `f3df30a80172131b74b5949a6a55234c962dac67` (`docs: specify authority-correct WP4 M4.5 optimization`) |
| Build mode | `--release`; `debug_assertions=false` |
| Qualification status | `qualification=false`; no profile selection or promotion |

The measured source was a dirty worktree. Therefore, the timing identity is the tuple
`(c96b539…, e7d0940…, ff4f7206…)`, **not a clean commit number alone**. Commit `f3df30a…` records the evidence but did not produce the measured executable.

## Frozen workload

| Property | Retained value |
|---|---:|
| Candidate profile | `K64/F64` |
| Source size | `104,857,600` bytes = exactly `100 MiB` |
| Raw source fingerprint | `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| Ordered CDC references | `5,284` |
| CDC-sequence fingerprint | `5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994` |
| File mapping | `83` leaves/pages and `2` branches |
| Approximate durable objects | `5,372` |
| Workspace root | `2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0` |
| Transition/delta | `ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412` |
| Closure digest | `d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a` |
| Campaign shape | One warm-up plus five isolated, balanced measured child processes |
| Cache condition | `warm_or_unknown`; no cold-APFS claim |

## Lifecycle diagram

The numbers inside the diagram are independently selected five-run medians. Phase medians are useful for attribution, but must not be arithmetically added to reproduce the median total; the exact equations held within every raw row.

```text
OUTSIDE ALL TIMERS
  retained-fixture custody gate
  raw fingerprint + ordered CDC fingerprint preflight
  isolated starting database preparation
                 |
                 v
+============================================================================+
| A. DURABLE CAPTURE                                                         |
|                                                                            |
|  A1. CANONICAL CAS MAPPING + OBJECT PERSISTENCE                            |
|      median 410,775,833 ns = 410.776 ms                                    |
|                                                                            |
|      source read + FastCDC  [nested here; separate CDC time unavailable]   |
|            |                                                               |
|            v                                                               |
|      5,284 raw ChunkIds                                                     |
|            |                                                               |
|            v                                                               |
|      canonical encode -> ObjectId -> immutable SQLite CAS put/reuse         |
|            |                                                               |
|            +-> 83 file leaves/pages -> 2 branches -> file root             |
|            +-> workspace root                                              |
|            +-> Phase-3 transition/delta                                    |
|            `-> complete visible-head state staged in one transaction       |
|                 |                                                          |
|                 v                                                          |
|  A2. PRE-COMMIT CLOSURE VALIDATION                                          |
|      median 388,155,208 ns = 388.155 ms                                    |
|                                                                            |
|      authenticate root + transition/delta                                  |
|            |                                                               |
|            v                                                               |
|      walk and authenticate the complete strong-edge closure                |
|            |                                                               |
|            v                                                               |
|      streamed full reconstruction + exact retained-source fingerprint      |
|                 |                                                          |
|                 v                                                          |
|  A3. SQLITE COMMIT DURABILITY                                               |
|      median 152,995,834 ns = 152.996 ms                                    |
|                                                                            |
|      exactly one publication COMMIT for the complete visible head          |
|                                                                            |
|  DURABLE_CAPTURE_TOTAL                                                     |
|      median 953,829,334 ns = 953.829 ms                                    |
|      100 MiB / total = 104.841 MiB/s                                       |
+============================================================================+
                 |
                 v
      close/drop Store, SQLite handles, and process-local receipts
                 |
                 v
+============================================================================+
| B. FRESH-STORE VERIFICATION                                                 |
|                                                                            |
|  B1. FRESH REOPEN + VISIBLE-HEAD/RECEIPT AUTHENTICATION                     |
|      median   1,155,208 ns =   1.155 ms                                    |
|                 |                                                          |
|                 v                                                          |
|  B2. FRESH FULL CLOSURE SCRUB                                               |
|      median 272,814,625 ns = 272.815 ms                                    |
|                 |                                                          |
|                 v                                                          |
|  B3. STREAMED FILE RECONSTRUCTION + EXACT FINGERPRINT                       |
|      median 429,984,583 ns = 429.985 ms                                    |
|                 |                                                          |
|                 v                                                          |
|  B4. EXACT ZERO/CROSS-CHUNK/LEAF/BRANCH RANGE PROBES                        |
|      median     655,958 ns =   0.656 ms                                    |
+============================================================================+
                 |
                 v
COMPLETE_LIFECYCLE_TOTAL
  median 1,663,448,833 ns = 1.663449 s
  100 MiB / total = 60.116 MiB/s
```

## Timer equations

For every measured raw row:

```text
durable_capture_total
  = canonical_cas_mapping_stage
  + precommit_closure_validation
  + sqlite_commit_durability

complete_lifecycle_total
  = durable_capture_total
  + fresh_reopen_head
  + fresh_full_scrub
  + reconstruction
  + range_verification
```

`source_cdc` is inseparable from and nested inside `canonical_cas_mapping_stage` in this retained implementation. It is not added again, and its standalone wall time is reported as `Unavailable`.

## Retained median table

| Phase | Median ns | Median time | Share of independently selected lifecycle median |
|---|---:|---:|---:|
| Canonical CAS mapping and object persistence | `410,775,833` | `410.776 ms` | `24.70%` |
| Pre-COMMIT closure validation | `388,155,208` | `388.155 ms` | `23.33%` |
| SQLite COMMIT durability | `152,995,834` | `152.996 ms` | `9.20%` |
| Fresh reopen/head authentication | `1,155,208` | `1.155 ms` | `0.07%` |
| Fresh full closure scrub | `272,814,625` | `272.815 ms` | `16.40%` |
| Streamed reconstruction | `429,984,583` | `429.985 ms` | `25.85%` |
| Range verification | `655,958` | `0.656 ms` | `0.04%` |
| **Durable capture total** | **`953,829,334`** | **`953.829 ms`** | — |
| **Complete lifecycle total** | **`1,663,448,833`** | **`1.663449 s`** | **100%** |

Because each cell is a separately selected median, the phase medians sum to `1,658.440 ms`, not `1,663.449 ms`. This is expected and is not missing or hidden time; use the per-row equations for exact accounting.

## What was achieved at this checkpoint

The accepted M3 change reduced canonical CAS mapping from `519.309 ms` to `410.776 ms` (`-20.899%`, faster in `5/5` paired measurements). Durable capture improved by `-7.057%` (`5/5`). Complete lifecycle improved by `-4.457%`, which is below the frozen 5% lifecycle threshold and is not a lifecycle-win claim.

SQLite COMMIT moved from `116.511 ms` to `152.996 ms` (`+31.315%`, `0/5` favorable). Physical-I/O/fsync attribution remained unavailable, so the campaign retained the M3 implementation for its protected mapping win but did not qualify or promote the candidate.

## Current optimization reading

The largest retained costs are reconstruction (`429.985 ms`), canonical mapping/CAS (`410.776 ms`), pre-COMMIT validation (`388.155 ms`), scrub (`272.815 ms`), and COMMIT (`152.996 ms`). This makes repeated authenticated traversal, canonical-byte handling, SQLite statement/BLOB activity, and durability attribution the next measurement targets.

Even deleting all pre-COMMIT planning work would leave the retained complete lifecycle near `565.674 ms`, or about `176.8 MiB/s`. Reaching the historical `500 ms` / `200 MiB/s` goal therefore requires both eliminating redundant pre-COMMIT work and removing at least another `65.674 ms` elsewhere. This is an Amdahl-style planning bound, not a measured forecast.

## Scope and non-claims

- This is the private WP4 benchmark candidate Store, not production `layerfs-engine` integration.
- SQLite remains the authoritative Phase 4A disk engine.
- The diagram does not qualify the candidate, select a profile, or prove the empirical 200/300 MiB/s targets.
- Big-O correctness for path-local same-count edits is separate from full-create throughput.
- Debug timing rows are correctness diagnostics only and are excluded from throughput evidence.
- Later M4.5 source changes require a new frozen release executable and a fresh one-warm-up/five-measurement campaign before replacing these numbers.

## Evidence documents

- [M3 milestone report](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/milestones/m3.md)
- [Optimization progress ledger](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/progress.md)
- [Phase 4 M4.5 optimization specification](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/milestones/m4-5/spec.md)
- [Post-M4.5 note](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/read-after-m4-5.md)
