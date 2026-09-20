# S4 — v0.1.6 Commit composition

> **Status: Research; informative and not a product contract.** Read-only historical reconstruction for #190. No benchmark was rerun. Existing receipts retain their original status and cache declaration; this document grants no admission.

## Findings

The raw historical Commit sum reproduces as **11,370,679,212 ns**, median **564,295,292 ns**, across 17 states. Against the handoff's retained core operation, the exact descriptive difference is **32,566,067,669 − 11,370,679,212 = 21,195,388,457 ns**, ratio **2.8640389075994275**. This is an unmatched historical comparison, not a measured causal effect.

The hypothesis that v0.1.6 excluded canonical content construction, tree construction, or Store admission from Commit is **falsified by source and retained receipts**. Its `content_ns` sums to 9,394,444,501 ns and `namespace_ns` to 342,355,542 ns, both inside Commit. However, its content interval includes concurrent admission and payload transfer, while core's named `content` interval stops before filesystem construction and admission. They cannot be subtracted as matching component times.

There are real composition differences: legacy builds the mutable workspace and reads/writes changed payload during a separately timed exec; core reads and authenticates corpus bytes outside each child and constructs filesystem input inside it. Legacy additionally captures a remote generation, transfers backing data during construction, publishes branch state, and completes/rebases the live workspace. Source establishes inclusion but does not quantify how much of the 21,195,388,457 ns difference each semantic difference causes.

## Custody and identity corrections

The original receipts were available under `benchmark-results/repository-history/stride-10/`. [custody.json](custody.json) lists SHA-256, byte length, original relative path and lossless gzip copy for five raw files. [derived-v016.json](derived-v016.json) contains per-state extracted values and original source identities. These are copies/derivations, not new samples. Container capability files were not copied.

The report says source `8308cd8e…` at `7fab1027a`, but raw `identity.json` records compiled commit **ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb**, tree **688a9af298f84b87ca17d5eb9c1d3ae4efe02d2f**, clean source seal **8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a**. `git diff ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb 7fab1027a -- crates benchmark/fs-bench-pro` is empty. Every one of the six historical Python harness hashes in the receipt equals `sha256(git show 7fab1027a:<path>)`. Therefore the requested revision's product/harness source is applicable, while the raw compiled identity is retained rather than rewritten.

Other identities:

- Product: `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`.
- Compilation: `bff3ff080d64671f9bf9ef7e73450afd4dbaaecbce86c91a5c6ae285d5b63743`.
- Dependency: `a1cf72ac4b77536d2d3b44c09872457a246673ca2eacf0997b11b293900709eb`.
- Workload: `821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8`.
- Host executable: `336c4cbb691374f6ab848d1f195b232f101440beae5db92e7f307066e4e820f2`.
- Image: `sha256:d2f871e4b794f0e280bccb0699d648943231170b9fb01cfe6f054eee8065941a`.

**Source notation below:** `L:path:line` is pinned to `7fab1027a0061e8b932345d4fcd6ac22a089b155`; `C:path:line` is pinned to pre-investigation core HEAD `9f35c49ad62956f131dc2676787f99d69659686e`. Resolve with `git show <revision>:<path> | nl -ba`. Current working-tree line numbers may move as S1 instruments the harness.

## Exact timer reconstruction

1. Python creates container and launches the **host** SDK coordinator before `phase_started`: `L:benchmark/fs-bench-pro/shared/storage_smoke.py:297–324`. The retained command points to the host `target/release/fs-benchmark-pro`; SQLite/runtime directory is a host path. The historical run is a hybrid topology, not container-owned canonical construction/SQLite.
2. For each state, Python removes `/input/checkpoint` and installs the fixture tree; `transfer_ns` encloses both operations, outside Commit but inside work wall: `L:benchmark/fs-bench-pro/shared/storage_smoke.py:333–346`.
3. The host coordinator runs one `storage-smoke-import` workload and then one Commit: `L:benchmark/fs-bench-pro/src/storage_smoke.rs:975–987,1050–1060`. `workload` wraps `execute` in `timed(store, "exec", ...)`: same file `699–710`.
4. Import parses old/new manifests, forms directory sets, removes deleted bindings, creates directories, copies changed payload through FUSE, and normalizes files/directories: `L:benchmark/fs-bench-pro/workload/storage_smoke.rs:90–156`. Unchanged files skip payload copy at `115–118`; all final directories are normalized at `152–155`.
5. Commit's timer brackets **only** `client.commit_workspace_session_with_status(id)`: `L:benchmark/fs-bench-pro/src/storage_smoke.rs:737–739`. `timed` reads `Instant::now()` immediately before the call and measures immediately after it; resource snapshots, physical counters and JSON emission are outside `elapsed_ns`: same file `572–625`.
6. SDK observation wraps the workspace call: `L:crates/layerfs-sdk/src/client.rs:244–262`. The remote workspace route includes lifecycle locking and metrics, then calls `commit_remote`: `L:crates/layerfs-workspace/src/lifecycle.rs:549–599`.
7. Remote Commit captures (`remote_commit.rs:76–85`), pulls frozen changed records (`118–125`), constructs (`167–203`), publishes (`205–231`), completes the remote generation (`236–257`), then rebases the host workspace (`260–269`). All references are `L:crates/layerfs-workspace/src/remote_commit.rs`.
8. `build_remote_candidate` routes to `CandidateInputs::build`: `L:crates/layerfs-workspace/src/changes.rs:368–407`. Content construction/admission goes through `construct_workspace_files` at `680–722`; reference application and inode finish are timed as namespace at `851–877`. Thus construction, metadata/tree update and admission are unequivocally inside Commit.
9. The independent visible-head query, monitor snapshot, allocation query and formatted receipt occur after Commit: `L:benchmark/fs-bench-pro/src/storage_smoke.rs:740–779,628–679`. Full historical oracle verification is a separate mode: `L:benchmark/fs-bench-pro/shared/storage_smoke.py:350–365`.

## Item-by-item boundaries

| Work | Historical Commit | Core state child | Evidence |
|---|---|---|---|
| Fixture selection, changed-byte acquisition | Excluded; preparation/install/exec | Excluded; corpus transition before child | L `shared/storage_smoke.py:340–345`; C `src/ops/history.rs:1513–1552` |
| Parse/diff old/new manifests, mutate mutable namespace | Excluded; exec importer | Transition diff outside; filesystem input assembly inside | L `workload/storage_smoke.rs:90–156`; C `src/ops/history.rs:1840–1850` |
| Initial Store creation | Excluded; setup before ready/work | Included once in state 1 | L `shared/storage_smoke.py:297–324`; C `src/ops/history.rs:1828–1838` |
| Remote snapshot capture and changed-record pull | Included | No equivalent remote workspace operation | L `remote_commit.rs:76–125` |
| Canonical file construction/hash/chunking | Included | Included | L `changes.rs:680–722`; C `src/ops/history.rs:1567–1569,1631–1650` |
| Payload transfer from sandbox backing during construction | Included | Changed bytes already held in memory | L `remote_commit.rs:118–121`, `changes.rs:365–367`; C `src/ops/history.rs:1515,1567` |
| Canonical namespace/tree/reference update | Included | Included | L `changes.rs:851–877`; C `src/ops/history.rs:1884–1896` |
| Store encoding, dedup/delta selection and admission | Included, pipelined with construction | Included in accept loop and finish | L `changes.rs:702–722`; C `src/ops/history.rs:1897–1933` |
| Caller advisory/base bookkeeping, filesystem input shaping | Different workspace implementation; no matching isolated timer | Included | C `src/ops/history.rs:1743–1826,1840–1850,1917–1928` |
| Commit/branch publication and live workspace completion | Included | Store finish; no branch/live workspace counterpart | L `remote_commit.rs:205–269`; C `src/ops/history.rs:1930–1948` |
| Post-call visibility query, monitoring and allocation enumeration | Excluded from Commit | Outcome/counter bookkeeping inside child; final reporting outside | L `src/storage_smoke.rs:764–779`; C `src/ops/history.rs:1933–1955` |

Abbreviations in this table expand to the full source paths in the reconstruction above; core paths begin `core/benchmark/fs-bench-pro-storage-content/`.

## Reproduced historical numbers

All durations below are integer nanoseconds. Original receipt fields retain their historical names.

| full157 ordinal | Commit elapsed_ns | Exec elapsed_ns | transfer_ns |
|---:|---:|---:|---:|
| 1 | 53,324,750 | 153,024,084 | 388,461,584 |
| 11 | 142,175,334 | 541,421,875 | 505,415,500 |
| 21 | 220,096,625 | 776,288,583 | 751,873,875 |
| 31 | 220,605,917 | 793,795,834 | 724,121,375 |
| 41 | 379,635,375 | 1,746,586,542 | 731,775,792 |
| 51 | 400,768,959 | 1,745,855,625 | 709,613,875 |
| 61 | 518,177,667 | 2,790,759,833 | 875,125,000 |
| 71 | 564,295,292 | 2,353,232,708 | 700,084,292 |
| 81 | 559,649,459 | 2,921,205,000 | 721,093,334 |
| 91 | 569,144,625 | 3,149,527,792 | 788,494,166 |
| 101 | 883,937,583 | 4,662,216,834 | 1,079,213,334 |
| 111 | 891,035,958 | 4,951,549,875 | 1,102,259,709 |
| 121 | 956,240,292 | 5,099,404,583 | 998,743,125 |
| 131 | 1,254,233,792 | 6,860,813,167 | 2,173,423,542 |
| 141 | 1,227,454,125 | 6,749,939,666 | 2,276,563,292 |
| 151 | 1,199,054,459 | 6,606,173,291 | 1,961,592,792 |
| 157 | 1,330,849,000 | 7,816,125,958 | 2,100,619,167 |
| **Sum** | **11,370,679,212** | **59,717,921,250** | **18,588,473,754** |

`work_wall_ns = 96,444,055,041`. Its residual beyond these three disjoint timer sums is **6,766,980,825 ns**, covering protocol interaction, monitoring, report collection, and other work-wall overhead; no attribution inside that residual is measured.

`wall_ns = 98,009,764,333 = setup 1,053,082,083 + work 96,444,055,041 + cleanup 467,753,209 + other lifecycle 44,874,000`. The historical report's text “setup/cleanup outside those walls” conflicts with the actual `wall_ns` field it rounded to 98.0 s. The code records `started` before setup and `wall_ns` after cleanup (`L:benchmark/fs-bench-pro/shared/storage_smoke.py:289,297,409–410`). The top-level wrapper `performance-summary.json.wall_ns` is **120,303,399,625**, including preparation **22,167,736,250 ns**; neither is a Commit timer.

| Retained Commit subfield / counter | Sum across 17 states | Interpretation |
|---|---:|---|
| host_cpu_ns | 12,373,244,915 | Host process CPU sampled around calls; no codec-only CPU |
| WorkspaceCommitReceipt.total_ns | 11,370,535,832 | Internal wall; outer elapsed exceeds it by 143,380 ns |
| capture_ns | 14,707,418 | Remote capture |
| candidate_plan_ns | 346,842,668 | Both outer pull and inner plan accounting |
| content_ns | 9,394,444,501 | Construction plus pipelined admission and content result processing |
| namespace_ns | 342,355,542 | Namespace/reference/inode update region |
| candidate_finish_ns | 9,810,613,877 | Includes outer full build and inner finalization; overlaps content/namespace |
| output_pipeline_ns | 7,485,573,500 | Nested concurrent pipeline accounting |
| output_admission_ns | 7,033,160,056 | Nested admission work |
| publication_ns | 927,849,296 | Publication region, includes admission finalization |
| checkpoint_ns | 270,491,294 | Completion generation delivery |
| object_admission_transactions | 101 | Count, not core SaveOutcome.commits-equivalent by assertion |
| physical encoding_ns | 2,423,813,393 | Sum of elapsed encoder intervals, not CPU |
| native_full_encode_ns | 70,707,309 | Subset of encoding_ns |
| native_prefix_encode_ns | 44,631,749 | Subset of encoding_ns |
| native_decode_ns | 46,781,452 | Native decode elapsed intervals |
| metadata_index_sync_ns | 70,906,624 | Persisted index maintenance timer |

**Do not add these rows into a decomposition.** `note_workspace_commit_phase` accumulates by field (`L:crates/layerfs-layerstack-store/src/telemetry.rs:710–734`), and `CandidateFinish` is called both inside `changes.rs:887` and around the entire build at `remote_commit.rs:198–201`. `candidate_plan_ns` also accumulates outer pull and inner planning (`remote_commit.rs:122–125`, `changes.rs:679`). The recorded `unattributed_ns = 0` is not proof that these overlapping fields partition elapsed time. Derived JSON mechanically sums original counters; sums of `max_*`, peak, or last-ID fields are **not** campaign maxima or meaningful count totals and are not used here.

The encoder instrumentation uses `Instant::now()` and elapsed wall (`L:crates/layerfs-layerstack-store/src/objects/admission.rs:614–625`); it provides no codec CPU counter. Native encoder values are added to aggregate encoding_ns there, so adding both would double count.

## Worker and cache confounds

The handoff says the one-worker rule did not apply to the historical campaign. The historical report instead attributes its stride3 cost to “the single construction worker directive.” These statements disagree. Neither supersedes a retained environment record.

Actual code is conditional: `L:crates/layerfs-workspace/src/changes.rs:572–582` accepts `LAYERFS_CONSTRUCTION_WORKERS` in 1..8, otherwise defaults to host available parallelism capped at eight. `changes.rs:683–688` independently forces **one producer if the prepared plan has a predecessor**, with task/memory bounds also applied. `changes.rs:1513–1526` marks that plan from prior regular-file roots or eligible removed-file matches. `objects.rs:4473–4489` caps small-content construction at four (`objects.rs:48`). A global construction mutex (`remote_commit.rs:170–173`) serializes candidate builds but does not itself prove one producer thread.

The Python coordinator inherits `os.environ` (`L:benchmark/fs-bench-pro/shared/storage_smoke.py:305–315`); its saved identity/result does not record `LAYERFS_CONSTRUCTION_WORKERS` or effective per-state producer count. `LAYERFS_HOST_BUILD_JOBS=8` in the receipt is a **build** setting, not construction concurrency. The container has NanoCpus=2,000,000,000, but canonical construction runs on the host. Hence neither “reference used four/eight workers” nor “all reference states used one” is measured. A worker contribution to the 21.2 s remains **NOT_MEASURED**; source specifically weakens the simple all-states parallel-reference hypothesis.

The historical identity explicitly says **fresh-store-existing-os-cache-uncontrolled**. The inspected history performance route has no invalidation/residency proof between import and Commit (`L:benchmark/fs-bench-pro/shared/storage_smoke.py:333–346`, `src/storage_smoke.rs:975–1060`). It cannot support a cold claim. The current core route acquires changed blobs before each state and rereads the growing Store during update; no matched cold proof exists in this comparison. Preserve the original historical exploratory PASS, but classify a new cold or like-for-like performance claim as **INELIGIBLE**. No cache-cost quantity can be derived from these receipts.

## Selection and corpus confirmation

Historical selection is explicitly `range(1,158,10) ∪ {157}` (`L:benchmark/fs-bench-pro/shared/repository_history.py:5–13`, `deepseek_ten.py:11–16`); its retained fixture indices match all 17 table rows. Both historical pins (`L:benchmark/fs-bench-pro/shared/storage_smoke.py:22–23`) and core pins (`C:core/benchmark/fs-bench-pro-storage-content/shared/history_corpus.py:41–57`) name manifest **03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271** and tip **b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed**. Read-only hashing of the current manifest reproduced that SHA, tip, and 157-checkpoint cardinality. No corpus payload was primed.

Same corpus does not mean same canonical object stream: raw legacy final allocation gives **51,722 objects / 380,563,155 canonical bytes**, whereas the handoff records core **52,032 / 380,921,328**, differences **310 objects / 358,173 bytes**. Raw legacy database-only allocation is **49,336,320 / 49,315,840 allocated/apparent B**, whereas report's retained host-directory total is **49,344,512 / 49,315,940 B**. The extra **8,192 allocated / 100 apparent B** is directory-scope overhead, not additional database payload. The scope distinction must remain visible in any size comparison.

## Reproduction and disposition

Read the retained gzip files with Python stdlib `gzip.decompress`; compare SHA-256 against custody.json. Parse performance-result.records; in each record select `kind == "storage-smoke-phase"` and `phase == "commit"`/`"exec"`, sum `elapsed_ns`, and sum record-level `transfer_ns`. `statistics.median` over the 17 Commit values reproduces 564,295,292. Parse the debug `WorkspaceCommitReceipt { ... }` segment separately; do not confuse it with outer elapsed or additive subphases. All original per-state receipts also remain embedded in the retained JSON result.

Recommend **accept-and-record this boundary reconstruction, leave causal timing disposition open**. It falsifies wholesale exclusion of construction/admission but does not allocate the remaining gap. Missing evidence: a matched cache/environment/reference run; effective historical construction-worker count; codec-only CPU; semantic equivalence of current filesystem work and legacy frontier operations; exclusive legacy subphase intervals. No product change or tripwire re-ruling follows from S4 alone.
