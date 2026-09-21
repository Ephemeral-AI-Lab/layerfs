# Handoff — the v0.1.6 ↔ v0.1.7 storage/content gap: evidence, instruments and what was already done

> **Status:** handoff and commission. It states its own limits, carries the evidence with every number
> sourced to a receipt or a `file:line`, and **gives no direction** on what to conclude or build. The
> sections that say what was done are labelled as record, not as recommendation. Filed from
> [#226](https://github.com/Ephemeral-AI-Lab/layerfs/issues/226) under
> [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219), after round 22d.

**What this document is for.** v0.1.6's namespace initialization peaked at **83,148,800 B** for a
100,000-file / 500 MB workload, on a clean verified-cold receipt. The same declared workload in the
replacement product peaked at **885,325,824 B** before round 22, and at **159,236,096 B** after it. The
time figures for the two are not comparable, and this document says exactly why, with the numbers, so the
next reader does not have to re-derive the boundaries before they can read anything.

## 1. The first thing to know: there is no pairing, and here is the evidence

A comparison of "v0.1.6 versus v0.1.7" is **a comparison of the reference product against the replacement
product**, because **v0.1.6 shipped only the reference product**. Measured, not assumed:

```text
git ls-tree -d --name-only v0.1.6        -> .agents .hallmark .layerfs artifacts benchmark
                                            containers crates docs release-notes tools web
git ls-tree -d --name-only v0.1.6:core   -> (no such tree; core/ does not exist at v0.1.6)

git diff --stat v0.1.6..5be4b7ae0 -- 'crates/*/src/**'     -> empty
git diff --stat v0.1.6..5be4b7ae0 -- 'core/crates/*/src/**' -> 192 files changed, 40873 insertions(+)
```

**Both consequences matter and they point in opposite directions:**

1. **The reference source is byte-identical between v0.1.6 and today.** So a v0.1.6 arm and a
   current-tree arm *at the `crates/` boundary* would compile the same code and measure the same product.
   `evidence/issue219-s0-custody-20260921T031259Z/README.md` §3 reached this first and used it to rule the
   pairing infeasible.
2. **The replacement is disjoint source that did not exist at v0.1.6.** So "v0.1.7" here always means
   `core/crates/**`, and a v0.1.6-vs-v0.1.7 difference is a **reference-vs-replacement** difference.

## 2. The second thing: the two timers do not measure the same interval

`test_setup_and_cache_discipline.md` §2.2 fixes what is inside a timer per case shape. The two rows below
are the *same declared workload* and their timers are not the same measurement:

| | v0.1.6 `namespace-100000` | v0.1.7 `pipeline-namespace-100000` |
| --- | --- | --- |
| timer | `layerstack_init_ns` | `pipeline.operation_work_ns` |
| includes fixture ingest | **yes** — `initialization_disk_read_bytes` **887,242,752** in the same record | **no** (before round 22d); yes under the streamed switch (§6) |
| includes content construction | **yes** | no before 22d; yes under the streamed switch |
| includes the C1 tree build | yes | yes (`pipeline.span_build_ns`) |
| container | **linux/arm64, `container_memory_mib: 2048`, `container_cpus: 2`** | **macOS/arm64** |
| fixture | a prepared, digest-verified directory on disk | generated in process from a recipe |

**Every time comparison between these two rows is therefore a category error, and the campaign has
recorded that three times** (`issue219-ns20-scaling-decision.md` §2, round 21 report §11.5, round 22c gap
page). The memory columns *are* comparable as process footprints. The time columns are not, and no figure
in this document offers them as such.

## 3. Memory — the evidence, both products, all sizes

### 3a. v0.1.6, the full ladder, from its own receipts

Fields: `process_t1_rss_bytes` (RSS after the work) and `process_initialization_incremental_peak_rss_bytes`
(the phase's own high-water above `process_t0`). Both are the reference harness's own instruments, and the
peak carries its own status `exact-new-lifetime-high-water`. Source: `benchmark-results/**/perf.jsonl`,
`kind=sample`, `records[0]`.

| case | source | dirty | cold | `process_t1_rss_bytes` | **incremental peak** | `layerstack_init_ns` |
| --- | --- | --- | --- | ---: | ---: | ---: |
| `namespace-100-compact-v3` | `8b5e0955e` | false | — | 29,392,896 | 24,444,928 | 31,667,167 |
| `namespace-1000-compact-v3` | `8b5e0955e` | false | — | 53,411,840 | 48,431,104 | 130,366,708 |
| `namespace-10000` | `8b5e0955e` | false | — | 74,940,416 | 70,008,832 | 1,100,711,333 |
| **`namespace-100000`** | **`8b5e0955e`** | **false** | **VERIFIED_COLD** | **83,148,800** | **78,233,600** | **4,986,155,625** |
| `namespace-100000` | `1ff1f2ddd` | false | VERIFIED_COLD | 85,475,328 | 80,560,128 | 4,397,542,208 |
| `namespace-100000` | `f8bb6dcb6` | **true** | VERIFIED_COLD | 78,970,880 | 74,006,528 | 3,783,918,959 |
| `namespace-100000` | `441be212e` | **true** | VERIFIED_COLD | 81,362,944 | 76,562,432 | 3,965,555,792 |
| `namespace-100000` | `e9951fc1e` | **true** | VERIFIED_COLD | 81,674,240 | 76,906,496 | 3,926,466,875 |
| `namespace-100000` | `95508a3d7` | **true** | — | 106,627,072 | 100,794,368 | 2,850,586,333 |
| `namespace-100000` | `56fc884e3` | **true** | — | 104,988,672 | 99,155,968 | 2,787,319,750 |
| `namespace-100000` | `448a74bfe` | **true** | — | 102,137,856 | 96,370,688 | 2,734,598,417 |

**Read the ladder, not a point.** The incremental peak **saturates**: 24 MB → 48 MB → 70 MB → 78 MB as
files go 100 → 1,000 → 10,000 → 100,000, with bytes going 5 MB → 20 MB → 300 MB → 500 MB. **Ten times the
files and 1.67× the bytes cost 1.12× the memory.** That shape is the finding; a single row is not.

**The mechanism is in the same records**, not inferred:

```text
namespace-100000, 8b5e0955e, VERIFIED_COLD
  initialization_disk_read_bytes          887,242,752     <- fixture + content, read during the timer
  initialization_disk_write_bytes          13,508,608
  store_database_bytes                    515,366,912
  sqlite_t1_memory_peak_bytes                       0
  sqlite_t1_page_cache_overflow_peak_bytes    568,000
  process_threads_before / after                1 / 1
  process_t0_swaps / process_t1_swaps           0 / 0
  process_t0_physical_footprint_bytes       2,769,280
  process_t1_physical_footprint_bytes      43,172,344
```

**Four rows of six are `dirty` or `UNVERIFIED`.** The two clean numbers are 78,233,600 and 80,560,128, and
they are 3 % apart; the dirty ones span 74–101 MB. Which to cite is a judgement this document does not make.

### 3b. v0.1.7 replacement, the same workload, three states

| | round 21 `ns21-E1` | round-22 `ns22-D2` | **round-22d `ns22-F2` (streamed)** |
| --- | ---: | ---: | ---: |
| **lifetime peak RSS** | 1,050,738,688 | 885,325,824 | **159,236,096** |
| measured-region baseline | 1,013,219,328 | 848,396,288 | 103,088,128 |
| measured-region increment | 37,519,360 | 36,929,536 | 56,147,968 |
| `heap.peak_incremental_bytes` | 27,557,611 | 36,307,857 | 29,833,774 |
| `operation_work_ns` | 3,549,393,833 | 3,640,415,749 | 4,707,842,708 |
| complete command | 6,446,615,375 | 6,476,722,917 | 6,459,083,500 |
| platform | macOS/arm64 | macOS/arm64 | macOS/arm64 |

### 3c. The same workload, three states, one line

| | peak | vs v0.1.6's 83,148,800 |
| --- | ---: | ---: |
| v0.1.6 `namespace-100000` | 83,148,800 | 1.00× |
| v0.1.7 before round 22 | 1,050,738,688 | 12.64× |
| v0.1.7 after round 22 step 1 + B | 885,325,824 | 10.65× |
| **v0.1.7 after round 22d (streamed fixture)** | **159,236,096** | **1.92×** |

**The 1.92× is not established as a product difference**, for two reasons this document records rather
than resolves: the platforms differ (Linux/arm64 in a 2 GiB container against macOS/arm64), and the
replacement's remaining footprint includes the prepared tree and plan — round 21's probe measured the
tree at 28,888,212 B of live heap after `PreparedTree::prepare` and 32,088,212 B after `plan()`, i.e.
**~55 MB of the 159 MB is harness fixture metadata that no fixture change touches**.

### 3d. v0.1.7's own C1 row, for the same declaration, with no Store at all

`c1.fs.build-scale` builds the identical tree and saves nothing. Four `PASS` rungs, one binary
(`97b8a5a7e71f…`), source `88accb7d0`, `evidence/issue219-ns20-ladder-20260921T113500Z/`:

| rung | `phases.operation_ns` | complete command | peak RSS | ns per binding |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100-compact-v3` | 127,292 | 4,680,958 | 3,391,488 | 1,260 |
| `namespace-1000-compact-v3` | 947,250 | 8,224,709 | 4,554,752 | 938 |
| `namespace-10000` | 68,514,625 | 94,736,583 | 18,071,552 | 6,784 |
| `namespace-100000` | 665,695,208 | 918,281,125 | 118,865,920 | **6,591 (−2.8 %)** |

The per-binding cost is **flat** and the peak at 100,000 is **118,865,920 B with no content at all** —
which is the floor any content-carrying row starts from.

## 4. Speed — what the record supports, and the one transformation that makes it comparable

**No figure below is offered as a v0.1.6-vs-v0.1.7 speed comparison.** The boundary-matched totals are
offered as *the same work, billed in one place or another*, which is a statement about the harness.

`ns22-*` runs are round 22, one lock window each, macOS/arm64, one sample, `--verify full`:

| run | `span_build_ns` (C1 tree) | `span_content_ns` | `teardown_ns` | `operation_work_ns` | `phases.operation_ns` | complete command |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ns22-D2` store-held | 1,137,483,625 | 2,490,886,583 | 528,019,917 | 3,640,415,749 | 4,171,219,416 | 6.477 s |
| `ns22-E2` + option B | 1,154,771,417 | 2,494,001,208 | 407,533,875 | 3,662,759,417 | 4,083,231,083 | 6.377 s |
| `ns22-F2` + streamed fixture | 1,170,128,584 | 3,525,885,416 | 54,311,667 | 4,707,842,708 | 4,764,861,417 | 6.459 s |

**The transformation that makes them comparable.** Each arm publishes its untimed construction as
`pipeline.construct_ns` + `pipeline.construct_noise_ns`; under the streamed switch that work is inside the
timer and both read 0. Adding the untimed part to `phases.operation_ns` gives one total for the same work
in every arm:

| arm | untimed construction | `phases.operation_ns` | **total** |
| --- | ---: | ---: | ---: |
| `ns22-D2` | 594,445,575 | 4,171,219,416 | **4,765,664,991** |
| `ns22-E2` | 593,809,480 | 4,083,231,083 | **4,677,040,563** |
| `ns22-F2` | 0 | 4,764,861,417 | **4,764,861,417** |

**D2 and F2 agree to 0.02 %** — 4,765,664,991 against 4,764,861,417. That is an identity rather than a
coincidence: F2 adds a number D2 already published and stops charging it twice. **E2 is 1.9 % below both**,
and the 1.9 % is the measured cost of the copy option B removed (79,174,500 ns standalone).

**One region is unstable and should be known before any timing work:** `teardown_ns` — the save's
connection close, which the row's declared formula **excludes** — reads 60,190,625 / 60,392,959 /
407,533,875 / 528,019,917 / 54,311,667 across runs of the row, a **9.7× span inside single sessions**.
`operation_work_ns` is `accept_span_ns − teardown_ns`, so this term propagates directly into the declared
figure. Round 22's B step is the worked example: the accept span fell 98,142,374 ns while the declared
figure **rose** 22,343,668 ns because the teardown moved further.

## 5. What the instruments are, in both products

Anyone comparing memory across these two trees needs to know the two schemas do not share field names.

| | v0.1.6 (reference harness) | v0.1.7 (core harness) |
| --- | --- | --- |
| lifetime high-water | `process_t1_peak_rss_bytes`, `process_t0_peak_rss_bytes` | `resources.rss.process_peak_bytes` |
| phase high-water | `process_initialization_incremental_peak_rss_bytes` + `process_initialization_peak_status` | `pipeline.rss_phase_peak_bytes` / `_baseline_bytes` / `_incremental_bytes` / `_final_bytes`, plus `rss_samples`, `rss_maximum_gap_ns`, `rss_unavailable_samples`, `rss_phase_peak_usable` |
| physical footprint | `process_t0/t1_physical_footprint_bytes` | not published |
| counting allocator | not published per phase | `heap.peak_incremental_bytes` (`support/instruments.rs`, `#[global_allocator]`) |
| device reads | `initialization_disk_read_bytes` | `instruments::process_usage().disk_read_bytes`; gated by `gates::device_attestation` at ≥ 0.9 × requested |
| SQLite | `sqlite_t0/t1_memory_used_bytes`, `_memory_peak_bytes`, `_page_cache_overflow_peak_bytes`, `_allocation_peak_count` | not published per phase |
| source | `benchmark/fs-bench-pro/` (`src/main.rs`, `src/repository_init.rs`) | `core/benchmark/fs-bench-pro-storage-content/` |

**Both harnesses' `sample_container_lifetime_peak_bytes` fields are container/cgroup readings and are not
process figures** — v0.1.6's reads 5,689,344 against its process's 83,148,800. `AGENTS.md` §1 forbids
quoting a lifetime cgroup counter as a phase number.

## 6. The record: what was done for `pipeline-namespace-100000`

**Record, not recommendation.** Four changes and their measured effect, each with its report.

### 6a. Round 21 — the row was registered, pinned and measured

`ns21-C2`/`ns21-E1`. Declared figures `operation_work_ns` 3,673,602,750 / 3,549,393,833,
`accept_span_ns` 4,162,787,625 / 4,033,601,291, complete command 6.57 s, `PASS` 13/13, fifteen pins.
The memory question was asked of it afterwards and the answer is §11 of that report. From the sampled run
`ns21-E1`: `resources.rss.process_peak_bytes` **1,050,738,688**,
`pipeline.rss_phase_baseline_bytes` **1,013,219,328**, increment **37,519,360** — so **96 % of the peak was
held before the timer**. The report's own decomposition of that baseline is:

| component | bytes | share of baseline |
| --- | ---: | ---: |
| the content store — 503 MB of canonical objects held in RAM | ~728,465,408 | 72 % |
| the 25 per-batch prefix snapshots | **164,347,158** | 16 % |
| the prepared tree and the plan | ~55,394,304 | 5 % |
| everything else, including the measured region | ~65 MB | 6 % |

The four rows are the report's own and the last is its named residual, so they are not a byte-exact sum.
Report: `evidence/issue219-ns21-100k-20260921T155500Z/report.md`.

### 6b. The prefix snapshots — 164,347,158 B removed

`prefixes` held one `TreeStore` snapshot of the reader chain per batch and `TreeStore::absorb` copies, so
the driver retained the sum over batches of the chain so far: 25 snapshots, 66,824 objects,
164,347,158 B, to serve a chain whose final state is 4,221 objects and 12,452,785 B. `PrefixKeys` +
`PrefixProvider` (`workload/providers.rs`) keep the identities instead, and the object set is identical.
**Measured: peak 1,050,738,688 → 885,325,824 (−15.7 %), measured increment 37,519,360 → 36,929,536,
fifteen pins intact, digest `2412681d…fd954`.** Report: `evidence/issue226-ns22-boundedfixture-…/report.md`.

### 6c. The timed deep copy — 98,142,374 ns off the accept span

The content stream offered each object as `content.cloned_object(id)`, a deep copy inside the timer, and
`Store::accept` takes the object **by value** (`cas/store.rs:504`), so the copy bought nothing.
`TreeStore::drain` moves them instead. **Measured: `accept_span_ns` −98,142,374 (−2.35 %)** against an
independent microbenchmark of 79,174,500 ns — two instruments, 0.12 points apart. The declared figure
**rose** 0.61 % because the excluded teardown moved 120,486,042 ns. **Memory: nothing, registered in
advance** — the copy is one object at a time.

### 6d. The streamed fixture — 723 MB off the peak

The row built every file with `fixture::noise(file.size, …)` and handed `construct_bytes` a slice.
`NoiseReader` (`workload/stream.rs`) now feeds **`construct_stream<R: Read>`**
(`core/crates/layerfs-content/src/file/content.rs:279`), which buffers at most
`small_file_threshold_bytes` = **131,072 B** for its probe. **Both products stream; only this harness fed
the whole-slice entry point** — the reference's ingest opens each file and drives
`build_checked_file(objects, &mut source, len)` through a `CountedSourceReader`
(`crates/layerfs-layerstack-store/src/layerstack.rs:2392-2405`), and v0.1.7's own `construct_bytes` is
`construct_stream(Cursor::new(bytes))` internally.

**Measured: peak 882,180,096 → 159,236,096 (−81.95 %, 5.54×), baseline 847,052,800 → 103,088,128,
`operation_work_ns` +28.53 % because construction is now inside the boundary, complete command 6.459 s,
fifteen pins intact on both `pipeline.*` rows.** Report:
`evidence/issue226-ns22d-streamed-20260921T190000Z/report.md`; ledger L86.

**The switch is `LAYERFS_PIPELINE_STREAM_FIXTURE=1`, an environment variable, not a registry row.** It is
not part of any case identity, and `FROZEN_CARDINALITY`/`ADMISSION_CASES` are unchanged.

### 6e. Four shapes closed by measurement

| shape | measured | where |
| --- | ---: | --- |
| one file per object for a spill (`TreeStore::write_to_dir`) | **432 %** of the row's declared figure to read back | `tests/spill_read_price.rs` |
| `mmap` as the reader | **28.4 %** against 12.6 % for positioned reads and 9.8 % for streaming | same |
| hashing the fixture in its own object sizes | **508,737,250 ns = 14.3 %** | same |
| spill the fixture **after** construction, then read it back | **+11.2 % of the figure for −14.7 % of the peak** | `issue226-bounded-fixture-design.md` §5b |

### 6f. The gap page

`evidence/issue226-ns22c-v016gap-20260921T180000Z/README.md` carries the same comparison with diagrams and
is corrected in place by `a491131f4`.

## 7. What is not established

* **No v0.1.6-vs-v0.1.7 speed comparison exists**, and none can be formed from these receipts: different
  intervals (§2), different products (§1), different platforms (§2).
* **No product-level memory comparison exists.** The one comparable axis is fixture placement, and it is a
  harness property. The 1.92× residual is not attributed between platform, harness metadata and product.
* **The replacement's product-level memory is bounded** — measured-region increment 36,929,536 B at
  100,000 entries, 7.34 % of canonical bytes, falling per byte as content grows (9.05 % at 10,000) — so a
  search for a product leak is not what the numbers point at.
* **The `dirty` v0.1.6 rows are not pooled with the clean ones**, and no receipt has been retuned.
* **Nothing here is a v0.1.6 regression claim.** The reference source is unchanged since v0.1.6, so no
  v0.1.6 figure can have moved.

## 8. Reproduction

```text
# the replacement's rows, both boundaries, one lock window each
cd core/benchmark/fs-bench-pro-storage-content
python3 runner.py perf --case namespace-10000           --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-100000 --out <fresh> --verify full
LAYERFS_PIPELINE_STREAM_FIXTURE=1 \
python3 runner.py perf --case pipeline-namespace-100000 --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-10000  --out <fresh> --verify full
python3 runner.py verify --run <each run directory>
python3 runner.py self-check

# the diagnostics
cargo test --release --locked --test namespace_memory_probe -- --nocapture
cargo test --release --locked --test spill_read_price -- --nocapture
cargo test --release --locked                                   # 141 tests

# the reference's receipts, read-only
python3 - <<'EOF'
import json, glob
for p in glob.glob("benchmark-results/**/namespace-*/perf.jsonl", recursive=True):
    ...
EOF

# every commit of the round
python3 tools/production_loc.py --root .        # 97100 -> 97100 (delta 0) for all of them
```

**Housekeeping that applies.** `benchmark-results` is in `.git/info/exclude`, so any receipt a report
depends on must be copied into the evidence directory. The measurement lock is **per worktree**. One
sample per case per arm, fresh `--out` per run, `--verify full` for anything called evidence. `AGENTS.md`
§3.8: `LAYERFS_CONSTRUCTION_WORKERS=1` for every case except `init_namespace`, whose multi-worker
initialization path is preserved deliberately. No change to `core/crates/**` or `core/*/sql/**` was made
in any of the steps above.
