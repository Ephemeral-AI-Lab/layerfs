# The v0.1.6 memory gap, drawn

> **Status:** demonstration. No product line changed, nothing was run for this page, and no performance
> claim of its own is made. Every number is read from a named receipt or a `file:line` in a named tree;
> the two figures that are arithmetic are labelled as such.

Commissioned by the owner's question: *"research and demonstrate the gap in diagrams on why v0.1.6 is more
memory-saving, and v0.1.7 as a refactor successor is supposed to keep its best part of optimization."*
This page answers it in that order: the gap (§1), the mechanism (§2), the measurement that says it is not
the product (§3), and **what the successor dropped and can put back — with the option that needs no new
cache contract and no product change** (§4).

## 1. The gap, measured

Two rows, both "initialize 100,000 files / 500 MB", both verified, both on a clean tree:

| | **v0.1.6** `namespace-100000` | **v0.1.7** `pipeline-namespace-100000` |
| --- | ---: | ---: |
| receipt | `benchmark-results/issue152/g1/namespace-100000-r4/perf.jsonl` | `ns22-D2`, this round |
| source | `8b5e0955e`, `dirty=false` | `47cf17049`, `dirty=false` |
| files / bytes | 100,000 / 500,000,000 | 100,000 / 500,000,000 |
| objects / canonical bytes | 112,451 / 513,026,835 | 109,414 / 502,914,928 |
| **process RSS after the work** | **83,148,800** | **885,325,824** (lifetime peak) |
| **incremental peak across the work** | **78,233,600** | 36,929,536 (measured region only) |
| **ratio** | — | **10.65×** |
| timer | 4,986,155,625 ns | 3,662,759,417 ns (declared) |
| disk read during the work | **887,242,752** | 0 (fixture is generated in process) |

**Read the ratio honestly.** The two timers do not measure the same interval
(`test_setup_and_cache_discipline.md` §2.2): v0.1.6's `layerstack_init_ns` *includes* ingesting its
fixture; v0.1.7's declared figure excludes fixture construction and includes the C1 tree build. So the
time column is not a comparison and is not offered as one. **The memory columns are comparable, because
they are process footprints for the same declared workload** — and the honest statement is
*~10×, in v0.1.6's favour, for the same namespace*.

```
PROCESS RSS OVER THE WORKLOAD  (bytes, log scale)
                                                        v0.1.6      v0.1.7
  1 GB ┤                                                           ████████ 885.3 MB  ← lifetime peak
       │                                                           ████████
  500 MB┤                                                          ████████
       │                                                          ████████
  100 MB┤                                                         ████████
   83 MB┤  ██ 83.1 MB   ← v0.1.6's whole process, after 100k files+500MB
       │  ██                                                ▲
   10 MB┤  ██                                                │ 10.65×
       │  ██                                                │
    1 MB┤  ██                                                │
       └──┴──────────────────────────────────────────────────┴──────────────
```

## 2. Why. The three mechanisms, drawn

### 2a. What the harness hands the product

```text
v0.1.6 — the product is handed a DIRECTORY and streams it
┌──────────────────────────────────────────────────────────────────────────────────┐
│ PREPARED FIXTURE ON DISK        (built once, before the timer, 500 MB of files)   │
│   d0000/…  d0999/…   100,000 files, 500 MB, digest-verified, 0 resident pages     │
└───────────────────────────────┬──────────────────────────────────────────────────┘
                                │  initialize_layerstack(Directory(path))
                                ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│ PRODUCT:  initialization_frontier(path, worker_limit, 1,000, INIT_FRONTIER 8 MiB)│
│                                                                                   │
│   read_dir ──▶ ┌─────────────┐   ≤8 MiB in flight   ┌──────────────┐              │
│                │  FRONTIER   │ ───────────────────▶ │  workers (8) │──▶ Store     │
│                │  bounded    │                      │  ≤8 MiB each │              │
│                └─────────────┘                      └──────────────┘              │
│   file bytes never all exist at once; the bound IS the peak                       │
└──────────────────────────────────────────────────────────────────────────────────┘
   peak = frontier + workers + one batch   ≈  83 MB measured
```

```text
v0.1.7 — the product is handed OBJECTS, and the harness built them all first
┌──────────────────────────────────────────────────────────────────────────────────┐
│ NO FIXTURE ON DISK.  registry row `pipeline-namespace-100000` has prepared = "-"  │
│ (tests/golden/registry.tsv:220), i.e. Preparation::InProcess                      │
└───────────────────────────────┬──────────────────────────────────────────────────┘
                                │  Recipe{entries, directories, seed}.prepare()
                                ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│ HARNESS, before the timer:                                                        │
│   1. generate all 500 MB in process      fixture::noise(size, seed)               │
│   2. build EVERY object into a TreeStore  ← 503 MB of objects resident            │
│   3. accept them into the Store inside the timer from that TreeStore              │
│                                                                                   │
│   ██████████████████████████████████████████████████████  728 MB  retained         │
└──────────────────────────────────────────────────────────────────────────────────┘
   peak = whole fixture + objects + plan + store   ≈  885 MB measured
```

### 2b. Where the peak sits in time

```text
v0.1.6   0 MB ────────────────[ init timer ]───────────────▶ 83 MB
                              ▲ reads fixture, builds objects, writes Store
                              └── peak is INSIDE the region, and bounded by design

v0.1.7   0 MB ──[ construct 503 MB ]──┤ TIMER ├── 885 MB
                 ▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲   ▲
                 peak happens HERE       └── the timer starts AFTER the peak:
                 (outside the timer)        96 % of the peak is already held
```

**The second diagram is the whole finding.** v0.1.7's row is not memory-hungry *while it works* — its
measured region costs 36,929,536 B, 7.3 % of canonical bytes, and that share *falls* as the workload
grows (report §4.3). It is memory-hungry *before it starts*, because the fixture is a construction rather
than an input.

### 2c. The pipeline shape change

```text
v0.1.6   seed ──▶ 500 MB fixture ──▶ DISK ──▶ [bounded frontier] ──▶ objects ──▶ Store
                  (1 MiB scratch)      ▲        8 MiB in flight
                                       └── the bytes live on disk, not in the heap

v0.1.7   seed ──▶ 500 MB fixture ──▶ HEAP ──▶ TreeStore(109,414 objects) ──▶ Store
                  (fixture::noise)     ▲        ▲
                                       │        └── a second resident copy of the same content
                                       └── 500 MB of generated bytes
```

## 3. It is not the product. Three measurements say so

This matters because a "successor that got worse" narrative would be wrong, and the receipts refute it.

| what is measured | v0.1.6 | v0.1.7 | reading |
| --- | ---: | ---: | --- |
| the **product's own** per-region figure | `process_initialization_incremental_peak_rss_bytes` 78,233,600 — includes fixture ingest | `pipeline.rss_phase_incremental_bytes` 36,929,536 — excludes fixture construction | **different intervals; the pair cannot be formed** |
| measured region per canonical byte | — | **7.3 %** at 100k, **9.1 %** at 10k — falling as content grows 1.67× | the replacement's measured path is bounded |
| the same content, held two ways | 500 MB in the **Store** | 500 MB in the **heap** | a **harness** decision, not an engine one |
| the fixture's residency | streamed, 1 MiB scratch, 0 resident pages | constructed in process | a **harness** decision |

**So the honest attribution is:** the 10× is **harness placement**, not product regression. What the
successor lost is not an optimisation inside the engine — it is a **shape the harness used to have**:
*the fixture arrives as an input the product streams, instead of as a construction the harness holds.*

## 4. What the successor dropped, and how it gets it back

### 4a. What was dropped, precisely

| v0.1.6 had | v0.1.7 has | `file:line` |
| --- | --- | --- |
| an **ingest-from-directory** path the product drives | **none**: `read_dir` appears nowhere in `core/crates/layerfs-storage/src/`, and the only hit in `core/crates/layerfs-content/src/` is the ordering backing's own scratch scan (`references/backing.rs:146`) | `crates/layerfs-layerstack-store/src/layerstack.rs:57`, `:1230`, `:1917` |
| a bounded frontier: `INITIALIZATION_TASK_BLOCK_LIMIT` 1,000, `INITIALIZATION_FRONTIER_BYTES` **8 MiB**, `INITIALIZATION_TASK_INPUT_BYTES` 8 MiB, 8 workers | the harness's own batching under `MAXIMUM_WALK_ENTRIES` 4,096 (`ops/fs.rs:44`) — a **tree** bound, not an **ingest** bound | `layerstack.rs:952`, `:962-963` |
| the fixture as a **prepared, digest-verified input on disk** (0 resident pages) | the fixture as a **recipe generated in process**; registry `prepared = "-"` | `tests/golden/registry.tsv:220` |

**This is AGENTS.md §3.8's own exception, read in the other direction.** That rule keeps
`init_namespace`'s multi-worker initialization on purpose — *"its initialization path
(`LayerStackStore::initialize_layerstack` → `direct_initialize_root_directories_inner` /
`prepare_parallel_root_directories`) legitimately uses multiple workers/threads and keeps its 2.7 s cold
Init target"*. The successor reproduced the **job** and kept the **single-worker** discipline for its own
construction, but did not carry the **bounded-stream** property that made the job cheap.

### 4b. The way back that needs **no owner ruling, no new contract, and no product change**

**Materialise the namespace recipe to disk as a prepared artifact, then stream it back through the
existing construction path with a bounded window — exactly the shape v0.1.6 had, built from parts the
replacement already ships.**

```text
NOW      registry: prepared = "-"  →  in-process recipe  →  TreeStore  →  Store     885 MB
PROPOSED registry: prepared = <artifact>  →  files on disk  →  bounded reader
                                            (de-warmed, existing g4.residency)
                                                    │
                                                    ▼
                                          construct_bytes per file   →  Store
                                          peak = window + largest object, not the fixture
```

**Why this one and not the options this campaign already priced:**

| | cost | contract needed | peak |
| --- | --- | --- | --- |
| option A (spill after construction, built and refuted) | **+11.2 % of the declared figure** | new declaration + device attestation | **−14.7 % only** — the fixture was already resident when the spill happened |
| option B (drain instead of copy) | **−2.35 %** — landed | none | none |
| **4b: stream from a prepared artifact** | fixture preparation moves to an untimed prepared artifact (`AGENTS.md` §2: "fixtures: use `--setup clone` … prepared inputs are acquired once and reused"); the timer then reads files the way v0.1.6 did | **none** — the artifact is an input, not a cache claim, and the read is inside the timer the same way the Store read already is | **bounded by the window — plus one file's slice, until part (b) above exists** |

**What is not claimed about 4b, and the second part it needs.** It has not been built or measured. Two
things bound it, and honest pricing needs both:

1. **The harness half.** Materialising the recipe and reading it back bounds what the *harness* holds.
   The row already holds the prepared tree and plan (**~55,394,304 B measured**, report §4.3) and the
   measured region costs **36,929,536 B**, and neither moves.
2. **The construction half, which the fixture's own accounting exposes.** The two 100 MB anchors are
   **not** one object each: the row's store accounting reads **200,216,930 bytes of `chunk` role across
   10,330 objects — a mean of 19,382 bytes** — so the anchors are CDC-chunked and *their objects* are
   small. But `construct_bytes` is handed the **whole file slice** to chunk
   (`fixture::noise(file.size, …)` at `ops/pipeline.rs`), so a 100 MB anchor is materialised in full
   before it is cut, and that single file's slice is a floor on the whole approach unless the
   construction path also streams its input.

**So the floor is not 92 MB.** It is *one file's slice plus one object*, i.e. ~205 MB if that file is an
anchor and the construction path stays whole-file; the object *sizes* are not the problem, the **input
slice** is. **The path has two parts: (a) the fixture becomes a prepared input the harness streams, and
(b) the construction path stops requiring the whole file at once.** Part (a) is the harness change this
page can evidence; part (b) is product work that step 5 of the #226 handoff forbids in that round and
that would need its own commission.

What *is* claimed is narrower and firmer: **the 728 MB is a shape the harness chose; v0.1.6 is the
measured proof the shape is avoidable for the same workload; and the replacement is missing one
capability — an ingest that reads its input in bounded pieces — rather than an optimisation inside its
engine.**

### 4c. What must not be redone

| closed | why |
| --- | --- |
| option A as "spill after construction" | built and measured: **+11.2 % of the figure for −14.7 % of the peak** — the fixture is resident before the spill (`issue226-bounded-fixture-design.md` §5b) |
| "the replacement's engine is memory-hungry" | 36,929,536 B measured region, 7.3 % of canonical bytes, falling per byte |
| the v0.1.6-vs-v0.1.7 timer comparison | different intervals by §2.2; the time columns are not comparable |
| "~500× fixture residency" (round 21 §11.5) | that was fixture *file* against scratch buffer; process against process it is **10.65×**, per §1 |
| the per-file spill shape and `mmap` | 432 % and 28.4 % of the declared figure respectively (`spill_read_price.rs`) |
