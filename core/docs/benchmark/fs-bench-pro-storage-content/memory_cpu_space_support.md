# Memory, CPU and space support for the Stage 6 harness

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Consumed by Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171),
> specifically the C1 families ([#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182),
> [`c1-families.md`](c1-families.md)) and the C2 families
> ([#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183),
> [`c2-families.md`](c2-families.md)). Preparation and cache-state
> discipline live in [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md);
> the gates and oracles are in [`gates_and_oracles.md`](gates_and_oracles.md).
>
> Time is the **fourth observation axis** and it already exists: `layerfs-telemetry`,
> used unmodified. It is covered in §10.
>
> **No figure in this document is a measurement.** Every number is a declared
> constant, a syscall's semantics, or a case configuration. Measured values are
> quoted only where cited to an existing receipt and labelled diagnostic.

## 1. The governing decision

**All resource observation lives in the benchmark harness. Nothing is injected
into product `src/` — not a counter, not a hook, not an accessor.**

`layerfs-telemetry` stays exactly as it is: a time-only axis. It is not extended,
and nothing is grafted onto `TimingNode`. Its own README already anticipates this
split — *"Future memory, CPU and storage observation domains are independent
siblings when implemented; unsupported observations are unavailable, not zero."*

The timing tree is emitted **unmodified**. The resource record rides as a sibling
key in the same case-result JSON, correlated to phases by monotonic clock rather
than by tree nesting. The time axis itself — three clocks, the tree shape, and the
usage rules that keep the recorder honest — is §10.

### Why not product code

| Reason | Source |
| --- | --- |
| Product `src/` may not contain test counters, benchmark drivers or tuned-for-benchmark behaviour | `core/AGENTS.md` |
| A memory counter in the product cannot fail an assertion, so it is not evidence | `core/docs/architecture/10-counters.md` §15.2 |
| It would need a dependency or `unsafe` in an `#![forbid(unsafe_code)]` crate | `core/crates/layerfs-content/src/lib.rs` |
| It would move the product seal for a non-product reason, invalidating sealed arms | `benchmark/AGENTS.md` |
| It would make the measured operation observe itself | `docs/general/benchmark_rules.md` §2 |

The repo already settled this once: `memory_ledger.rs:1-4` — *"This is measurement
code in an external target, never a product hook."*

### What is consumed from the product

Declared resource charges that **already exist** because enforcement requires
them. These are read, never extended:

| Layer | Already public |
| --- | --- |
| C1 | `SortedWork.peak_scratch_bytes`, `EditCounters.peak_deferred_bytes`, `MappingBuild.peak_pending`, `MergeWork.peak_run_bytes` / `peak_live_runs`, `FileBacking.peak_bytes`, `DiscardingConsumer.peak_object_bytes` |
| C2 | `Store::pool_index_bytes`, `candidate_index_bytes`, `retained_tail_bytes`, `pending()`, `PoolCounters`, `StoreReadCounters.canonical_bytes` |

`10-counters.md` §15.7 draws the line itself: *"`peak_deferred_bytes`,
`peak_scratch_bytes` and `peak_pending` are the operation's own declared charges.
They are **not** process RSS, and they do not include allocator overhead, page
cache or SQLite's page allocation."*

`Store::path()` is already public (`cas/store.rs:173`) — which is exactly enough
for the runner to `stat` the file. A `Store::size()` accessor is therefore **not**
added; that would be a benchmark-driven API addition.

## 2. Layout

The harness directory is the crate root; there is no `bench/` subdirectory.

```text
core/benchmark/fs-bench-pro-storage-content/
  CONTRACT.md          frozen case specification, written before any receipt
  README.md            how to run; what each verb guarantees
  runner.py            list | prepare | perf | verify | self-check
  Cargo.toml           the harness crate (publish = false), with an empty [workspace] table
  Cargo.lock           NOT core/Cargo.lock — guarded by shared/test_lock_parity.py
  target/              its own; share with core only via CARGO_TARGET_DIR
  src/                 Rust — all resource observation lives here
    main.rs            dispatch by --case
    registry.rs        Case rows, cardinality self-check, TSV-hash pin
    support/
      alloc_ledger.rs  counting GlobalAlloc
      rss.rs           10 ms sampler thread
      cpu.rs           getrusage bracketing
      residency.rs     mmap + msync(MS_INVALIDATE) + mincore
      result.rs        case-result JSON writer
    families/          c1_*.rs, c2_*.rs, pipeline.rs
  shared/              Python — space, external cross-checks, receipts
    test_lock_parity.py  every package in both locks must match version and checksum
    space.py           st_blocks, pragmas, pack SQL, sidecars
    sampler.py         getrusage(RUSAGE_CHILDREN), deadline, measurement lock
    receipt.py         identity record, append-only write, budget status
    fixtures.py        prepared-input cache + compatibility digest
  results/             gitignored dev runs
```

This mirrors `benchmark/fs-bench-pro/` (Rust in `src/`, Python in `shared/`),
minus Docker.

**Boundary-guard fact:** `core/tools/check_product_boundary.py:90-95` scans only
`core/crates/*/src` and `core/crates/*/sql`. Everything under
`core/benchmark/**` is outside that scan, so raw `extern "C"` FFI and
dev-dependencies are legal here without touching any product rule.

## 3. Memory — three independent instruments

Three instruments, three different meanings. They are **never** collapsed into one
field called `memory`.

### 3.1 Heap — counting global allocator

- **Mechanism:** `unsafe impl GlobalAlloc` over `System` with relaxed atomics
  (`CURRENT`, `PEAK`, `BASE`, `ALLOCATIONS`, `CHARGED`). `alloc` adds
  `layout.size()`; `dealloc` subtracts; `realloc` subtracts the old size and
  records the new. `begin()` snapshots `BASE` and zeroes `PEAK`; `end()` reads.
- **Prior art in this tree:** `memory_ledger.rs:58-99`, `edit_memory_probe.rs:34-64`,
  `filesystem_ordering_scan.rs:26-46`.
- **Precision:** *exact requested bytes.* Excludes allocator metadata, alignment
  padding, and any `mmap` the allocator serves outside `alloc`. Lower bound on
  true heap.
- **Why it can be a gate:** deterministic for a fixed input.
- **Constraint:** the allocator is process-global, so **one phase per process**
  gives the cleanest figure.

### 3.2 RSS — 10 ms sampler thread

- **macOS:** `proc_pid_rusage(pid, RUSAGE_INFO_V2, &mut info).ri_resident_size`.
- **Linux:** `/proc/self/status` → `VmRSS:`.
- **Prior art:** `benchmark/fs-bench-pro/src/main.rs:238-254` (macOS),
  `:256-269` (Linux), struct at `:276-299`; 10 ms thread and receipt fields at
  `src/workspace_bench.rs:344-415` (`nominal_interval_ns = 10_000_000`).
- **Do not** use `ps -o rss= -p <pid>` (as `memory_ledger.rs:144-155` does today)
  for anything but boundary samples: it forks a process per sample, capping the
  rate near 100 Hz and perturbing what is being measured.
- **Mandatory receipt bundle** (`docs/general/benchmark_rules.md` §10):
  `sampling_interval_ns`, `sample_count`, `first_sample_ns`, `last_sample_ns`,
  `maximum_sample_gap_ns`, `rss_baseline_bytes`, `rss_phase_peak_bytes`,
  `rss_incremental_peak_bytes`, `rss_final_bytes`.
- **Fail-closed rule:** a missed boundary or an excessive gap makes the phase peak
  **unavailable** and the row `INELIGIBLE` — never quietly fast.
- **Coverability limit (review S3).** At a 10 ms nominal interval a phase needs
  roughly **≥ 200 ms** to contain an interior observation. The whole `{1, 10, 100}` MiB
  ladder is under that, so **RSS cannot gate the O(1)-memory claim there**. Gate that
  claim on the **counting allocator** (exact, deterministic, per-phase, zero
  perturbation) and use RSS as a G4 bound plus an anomaly detector.
- **Never** report a lifetime high-water (`ru_maxrss`, cgroup `memory.peak`) as an
  incremental figure. Label it.

### 3.3 File-page residency — the anti-warm-cache instrument

- **Mechanism:** `mmap(PROT_READ, MAP_SHARED)` → touch one byte per page →
  `msync(addr, len, MS_INVALIDATE | MS_SYNC)` → `mincore(addr, len, vec)` → count
  set bytes.
- **Prior art:** `benchmark/fs-bench-pro/shared/cold.py:28-80`, including a
  `self_check()` (`:65-80`) that proves the primitive **both detects and evicts**
  known-warm pages. Receipt fields at `:113-183`.
- **Why it exists:** this is the instrument that catches what #151/L18 got wrong —
  page-cache warmth crediting a measured phase.
- **Cost:** `mincore` over a 500 MB fixture is measurable work. Run it only where
  a case reads a file it just wrote (read and transition families), never on pure
  construction.
- **Reuse:** `cold.py` is hard-coded to `init_namespace/namespace-100000`
  (`:13-19`: `FIXTURE_DIGEST`, 100,000 files, 1,001 directories, 500,000,000 bytes,
  `TARGET_NS = 2.7 s`). Generalizing = parameterize those constants and stamp a new
  `contract`/`method` pair.

## 4. CPU — one syscall, two vantage points

### 4.1 In-process, per phase

```rust
// The struct MUST be the full 17-field C layout: getrusage writes all of it,
// so a truncated #[repr(C)] struct is a buffer overrun. See
// benchmark/fs-bench-pro/src/main.rs:185-218 for the correct declaration.
#[repr(C)] struct NativeTimeval { tv_sec: i64, tv_usec: i64 }
#[repr(C)] struct NativeRusage { /* all 17 fields, in order, per <sys/resource.h> */ }
unsafe extern "C" { fn getrusage(who: i32, usage: *mut NativeRusage) -> i32; }
```

Two reads bracketing a phase give **exact** per-phase CPU (`RUSAGE_SELF = 0`).
Unlike wall time it is not perturbed by the host's run-to-run spread. Full working
declaration: `benchmark/fs-bench-pro/src/main.rs:185-236`.

### 4.2 From outside, per process

`resource.getrusage(resource.RUSAGE_CHILDREN)` in the Python runner yields
`ru_utime`, `ru_stime`, `ru_maxrss`, `ru_nswap` with **zero code in the measured
process** — an independent cross-check on the in-process numbers.

### 4.3 The derived figure that matters

**`cpu_ns / elapsed_ns`** — utilization. The product is single-worker, so ≈ 1.0
means CPU-bound (encode, hash, frame, assemble) and ≪ 1.0 means waiting (pack
reads, SQLite). That single ratio locates the run among C2's three cost centres.

### 4.4 Traps

- **Units:** macOS `ru_maxrss` is **bytes**; Linux is **KiB**. The existing code
  multiplies by 1024 on Linux only (`main.rs:233-234`).
- **Swap:** `ru_nswap` != 0 is a hard failure.
- **Sampler self-cost:** a 10 ms thread's CPU is charged to the process by
  `getrusage(RUSAGE_SELF)`. Declare `sampler_threads` in the receipt and run a
  **sampler-on/off control arm** — the same discipline the repo already applies to
  timing (`--timing on|off`; `tests/timing.rs:193-229` proves recorded and
  disabled runs produce identical bytes and counters).

## 5. Space — `stat` and pragmas

All cheap enough to run at every phase boundary.

| Field | How |
| --- | --- |
| `store_allocated_bytes` | `std::os::unix::fs::MetadataExt::blocks() * 512` — **real disk occupancy** |
| `store_apparent_bytes` | `MetadataExt::len()` — logical only; **never** headlined as occupancy |
| `store_db_pages` | `PRAGMA page_count` |
| `freelist_pages` | `PRAGMA freelist_count` — fragmentation |
| `pack_bodies_bytes` / `largest_pack_bytes` / `pack_count` | `SUM(length(data))`, `MAX(length(data))`, `count(*)` over `object_packs` — **already written verbatim** at `tests/memory_bounds.rs:281-296` |
| `pack_tail_waste_bytes` | pack walk: capacity minus used |
| `sidecars` | `.sqlite-wal` / `-shm` / `-journal` must be **absent** under `journal_mode = MEMORY` |

**Do not repeat an existing conflation:** `tests/filesystem_pipeline.rs:509` reads
`st_size` into a variable named `bytes_on_disk`. `st_size` is a **logical** figure.

### 5.1 Why space matters more than a footprint number

There is **no `VACUUM` anywhere in `core/`**, and the only DELETEs are the two
paged statements in `src/sqlite/cleanup.rs:65,120`. Freed pages therefore go to
SQLite's freelist and **the file does not shrink**. `tests/core_pipeline.rs:215-248`
already bounds the *delta* after one failed save (`after <= before + 4_096`); nothing
measures absolute occupancy, the freelist, or what accumulates across repeated
failed saves.

Space is consequently the only instrument that can show whether an abandoned save
leaves durable garbage — a #171 acceptance item currently asserted only in its
weakest form.

## 6. What is unavailable on a bare host

v0.1.6's memory apparatus exists because it containerised. On a local host there is
no cgroup, so the domains `benchmark_rules.md` §10 asks to separate are **not
separable**: page cache, dirty/writeback, shmem, kernel/slab, sockets. They are
reported as `null` with an explicit reason — *"not separable on a bare host; no
cgroup"* — never as a fabricated zero.

What replaces them:

| v0.1.6 cgroup domain | Local replacement |
| --- | --- |
| file cache attributable to our file | `mincore` page residency (§3.3) |
| anonymous memory | process RSS (§3.2) |
| process lifetime peak | `getrusage.ru_maxrss`, labelled lifetime |
| container CPU | `getrusage` self and children |
| container disk | `st_blocks` on the Store and fixture |

**Lost with the container envelope:** v0.1.6 ran under 2 CPU / 2 GiB / no swap /
256 PIDs. There is no ceiling on a laptop, so a runaway allocation can take the
machine down. Copy the guard pattern — `if rss > <declared ceiling> -> exit(125)`
(`src/workspace_bench.rs:367-370`) — record the ceiling, and keep the measurement
lock: this host has demonstrably run a concurrent agent workstream, and the #151
ledger records a stray `grep` at 54 % CPU for 34 minutes during samples.

## 7. Phase correlation

The telemetry tree carries `elapsed()` per node, not absolute timestamps. Three
routes, in order of preference:

1. **Caller-owned boundaries.** The harness creates the coarse scopes itself
   (`scope.child("storage.begin")`, `"storage.save"`, `"storage.read"`), so it
   records `Instant` at those points. This covers every phase worth attributing
   resources to.
2. **Pre-order cumulative reconstruction** from inclusive durations. Valid for
   sequentially nested scopes. **Not valid for `attach`ed subtrees**, whose work
   happened before attachment — declare that limitation rather than imply coverage.
3. **One phase per process.** No correlation needed; heap, RSS and CPU are all
   exact by construction. Use for the cases where only a provably gap-free phase
   peak will do: `c1.construct.chunked` (O(1) memory), `c2.read.waves`
   (O(1 MiB + 4 MiB)), `c1.transition.boundary` (shrink).

Samples falling in no named window are reported as **unattributed**, never dropped.

## 8. Case-result shape

The timing tree is unchanged, but it **cannot carry a sibling key**: the frozen
writer emits exactly `name`, `elapsed_ns`, optional `outcome`, optional
`incomplete`, `children` (`layerfs-telemetry/src/timer/json.rs:29-89`). Any "sibling
key" shape requires the harness to string-surgery the crate's output, which would
stop being the crate's output. The harness therefore **wraps**: it writes its own
`trace.jsonl` and leaves `timing.json` byte-verbatim as the product receipt.

Wrapped shape (see `test_setup_and_cache_discipline.md` and the trace design):

```json
"resources": {
  "heap":    {"current_bytes":…, "peak_bytes":…, "allocs":…, "charged_bytes":…,
              "provenance":"counting-global-allocator-requested-bytes"},
  "rss":     {"baseline_bytes":…, "phase_peak_bytes":…, "incremental_peak_bytes":…,
              "final_bytes":…, "sample_count":…, "sampling_interval_ns":…,
              "maximum_gap_ns":…, "coverage_status":"…"},
  "cpu":     {"user_ns":…, "system_ns":…, "source":"getrusage-self",
              "utilization_milli":…},
  "swap":    {"nswap":0},
  "space":   {"store_allocated_bytes":…, "store_apparent_bytes":…,
              "store_db_pages":…, "freelist_pages":…, "pack_bodies_bytes":…,
              "largest_pack_bytes":…, "pack_count":…, "pack_tail_waste_bytes":…,
              "sidecars":[], "fixture_disk_bytes":…},
  "residency": {"pages_checked":…, "resident_pages":…,
                "method":"darwin-mmap-mincore-v1"},
  "sampler": {"threads":1, "control_arm":"…"},
  "unavailable": {"cgroup_file_cache":null, "reason":"not separable on a bare host; no cgroup"}
}
```

## 9. The unguarded boundary

**No automated check would catch a memory counter added to `core/crates/*/src`.**
The boundary guard checks line caps and `unsafe`; it cannot see semantics. This
boundary therefore has to be written down and reviewed, not merely scanned.

`CONTRACT.md` must carry it as an explicit checklist item — *"no measurement
instrumentation in `core/crates/*/src`; the harness adds none"* — verified by diff
inspection at each landing commit.

## 10. Time — the axis that already exists

Memory, CPU and space are additions. Time is already implemented by
`layerfs-telemetry` and is used **unmodified**.

### 10.1 The scope is in the API

The harness never writes its own product timers. It builds a tree and reads the
report:

| Layer | Form |
| --- | --- |
| C1 file | **every** entry point takes `TimingScope` — `construct_bytes`, `construct_stream`, `apply_edits`, `read_all` / `read_range`, `FileView::open` |
| C1 filesystem | **two forms**: `build_filesystem` / `update_filesystem` take no scope; the `_timed` variants take `&FilesystemPhases` and record six coarse phases (`validate`, `directories`, `references`, `inodes`, `cleanup`, `root.encode`) — deliberately *"without one trace node per inode"* |
| C2 | `Store::create` / `open`, `begin_save`, `finish`, `abort`, `read_batch` all take `TimingScope` |

### 10.2 Two clocks, not three — collapse clock 2 and clock 3

**Correction:** clocks 2 and 3 need not be incomparable. `CLOCK_MONOTONIC_RAW`
(id **4**) is reachable from both sides — Rust via `clock_gettime(4)` (the pattern
already used at `benchmark/fs-bench-pro/src/main.rs:3965-3989`) and Python via
`time.clock_gettime_ns(time.CLOCK_MONOTONIC_RAW)` (verified on this host:
`time.CLOCK_MONOTONIC_RAW == 4`). **Never use `Instant` for a trace window** — it is
opaque and cannot cross a process. `Instant` stays fine inside telemetry (frozen).
This removes the cross-domain problem entirely; the only remaining quantity is the
parent's own monitor thread, which is not an axis.

Also note: `Σ self_ns == root.elapsed_ns` is an **algebraic tautology** and must not
be sold as an `attach()` detector. Attached subtrees cannot be detected numerically;
enforce the prohibition structurally instead (a grep guard over product `src/` plus a
declared-tree comparison).

### 10.2b The clock table

| # | Clock | Owner | Answers |
| --- | --- | --- | --- |
| 1 | product phase tree | `layerfs-telemetry`, inside the child | where time went inside the operation |
| 2 | harness phase boundaries | `Instant` in the bench binary, at the scopes **it** creates | resource correlation |
| 3 | command wall | `runner.py`, `time.monotonic()` around the whole invocation | did the complete command fit 15 s |

Clock 2 is required because the tree carries **durations, not absolute timestamps** —
a resource sample cannot be sliced by it. Clock 3 must be **labelled external** and
never substituted for an inner operation metric, and the two monotonic epochs are
different clock domains that must not be compared as one.

### 10.3 Tree shape

The harness names the coarse phases; product-internal children appear beneath them
automatically:

```text
c1.edit.length-changing.insert-middle-4k      <- harness root, one per case
  content.edit                                 <- harness-owned
    edit.base_read / edit.compare / edit.split / edit.finish
  storage.save                                 <- harness-owned
    storage.begin / storage.accept / storage.finish
  storage.read                                 <- harness-owned
    mapping.navigate / mapping.payload
```

### 10.4 Three usage rules

**(a) Do not use `attach()`.** It grafts a *completed* report, so the attached
subtree's work happened at a different wall time and its window is not
reconstructible; it also consumes the assembled tree's remaining node budget. It
was built for a future adapter/transport integration that does not exist here.

**(b) Nodes scale with phases, not with work units.** `MAX_NODES = 1024` is global
per report. A 500 MB read is roughly 1,000 payload waves — one node per wave blows
the budget. The product already behaves correctly (`SaveOperation::accept` creates
no node; `tests/timing.rs:57-94` pins a 600-object save under 16 nodes) and the
harness must follow it.

**(c) `MAX_LABEL_BYTES = 128`.** A clipped label marks the row incomplete, so keep
labels short and static and put the case ID in the root only.

### 10.5 Clipping is silent — convert it to a hard failure

Clipping marks the affected node **and all ancestors** incomplete but **never fails
the operation**. The harness must therefore check `report.is_incomplete()` and fail
the row itself. All four existing vehicles already do
(`measure_components.rs:145-158`, `measure_edits.rs:299-312`,
`measure_pooled.rs:193-203`, `measure_filesystem.rs:316-326`).

### 10.6 Control arm

`--timing on|off` — `Timing::record` versus `Timing::disabled`. Disabled reads no
clock and creates no nodes, and `tests/timing.rs:193-229` already proves the two
produce **identical roots, counters and bytes**, so the arm is free. Report both.

### 10.7 Saving is the caller's job

`Timing::record` returns `(result, TimingReport)`. The harness writes
`timing.json` with `create_new` (no clobber), flushes explicitly, and keeps the
save result separate from the product result.

### 10.8 Verdict: do not extend the crate

The crate is sound and its limits are contract, not defects: `!Send`/`!Sync` scopes
with compile-fail fixtures make the parent/child lifetime a *type* guarantee;
`Inner::collect` emits `Unknown` + `Duration::ZERO` for a slot still running, so a
panic cannot serialize as success; `disabled` is truly inert; clipping marks
ancestors rather than truncating silently; a duration beyond `u64` nanoseconds is
a writer error, not a saturating number.

One future coupling is worth naming rather than fixing: `!Send` is load-bearing for
a *future* architectural decision. #178 records O4 (producer pool) as *"Blocked by:
`TimingScope` being `!Send`, locator determinism, and the `AGENTS.md` rule — all
three, not one."* That is a Stage 7 question, not this campaign's.

## 11. Non-goals

- No change to `layerfs-telemetry`; no second axis type; no new node fields.
- No instrumentation, hook, counter or accessor in product `src/`.
- No new product dependency; no `unsafe` in product crates.
- No Docker, container or cgroup sampling anywhere in this harness.
- No absolute memory target or cap is claimed. The rules are explicit: *"claim only
  the application-owned bounds that are established, not a total RSS cap."*
- No lifetime figure reported as a phase figure; no fabricated zero for an
  unavailable domain.
