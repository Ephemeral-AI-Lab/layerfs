# Measuring a retained-history row

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §5,
> §6, §9, §10 and §11, and by
> [`../memory_cpu_space_support.md`](../memory_cpu_space_support.md).

## 1. The four phases, and the six published numbers

| | phase | what runs | published as | budgeted |
| --- | --- | --- | --- | --- |
| a | preparation | acquire the base (copy or load the master) + de-warm | `preparation_wall_ns`, with the copy/de-warm part also as `acquisition_wall_ns` | no — published only |
| b | **work** | `Store::open` on the sample copy → construct state *k*'s files and directories → save → close | `operation_ns` (the product's own telemetry root) | **yes — the performance claim** |
| c | verification | the oracle of [`verification.md`](verification.md) | `verification_wall_ns` | its own 60 s budget |
| d | cleanup | destroy the per-case copy, close | `cleanup_wall_ns` | inside the complete-command wall |
| | | process wall | `complete_command_ns` | the lifecycle and cleanup ceiling |

`handoff_ns` is published **beside** `operation_ns`, not inside it, so harness work that
happens within the timed region is visible rather than absorbed. Measured precedent:
the harness's object handoff was 0.162 s of a 4.997 s measured phase on
`dedup-cdc-overwrite-500`.

**Cleanup does not exist today**, because nothing is copied per case yet. This is the
first family where it becomes real work: every row copies a base of tens to hundreds of
megabytes and destroys it. It must be timed and bounded like the rest.

`verify` re-derives all six from the raw artifacts and **fails closed**: if
`preparation + operation + verification + cleanup` does not reconcile with the
invocation, and the invocation with the process wall, inside the declared tolerance, the
row is `INCOMPLETE` rather than quietly wrong. The tolerance is the harness's existing
one — 250 ms plus 2 % of the wall — which covers process start and teardown and nothing
else.

## 2. The real work, and what is inside the timer

Fixed per case shape, and the driver does not choose it. A history row runs **two
product entry points in one timed region** — the shape `pipeline.*` already has:

```text
inside the timer   Store::open  +  build_filesystem  +  save  +  ack
outside            corpus read · manifest decode · blob accumulation ·
                   building state k-1 · copying the base · de-warm · the oracle
```

**C1 half — `build_filesystem` / `update_filesystem`** over a `FilesystemInput`:

| field | meaning |
| --- | --- |
| `base: Option<FilesystemRootId>` | the previous state's root, **read back from the Store** |
| `scope`, `root_serial` | allocation scope and root serial |
| `directories: &[DirectoryUpdate]` | final directory bindings, ordered by parent serial |
| `inodes: &[InodeUpdate]` | typed final inode values, ordered by serial |
| `new_inodes: &[u64]` | serials the caller's allocator just created |
| `resources: FilesystemResources` | declared ceilings |

The important line: the harness supplies the **declared final state**, and the product
computes the delta against `base`. The harness's job is *translation* — corpus tree to
update lists — and never diffing. A harness that computed the diff would be doing the
product's work and the measurement would say nothing about the product.

**C2 half — `Store::open` on the sample copy plus the save**, through the product's own
`SaveHandoff` adapter ("lets C1 feed a save operation directly"). `CountingConsumer`
counts what crossed on that same path, which is why `g2.handoff` compares *emitted*
against *acknowledged* rather than trusting a number the save reports about itself.

**The C1 half needs no new machinery.** `PreparedTree` (`ops/fs_fixture.rs`) already
carries `directories`, `inodes`, `new_inodes`, `listings`, `files` and
`directory_serials`, and already has `emit`, `load`, `input_parts()` and `check()`. The
history row is `c1.fs.build-scale`'s shape with the tree coming from the corpus and the
base being the previous state.

Counters produced: `FilesystemUpdateCounters`, `SortedWork` (dirs and inodes),
`ObjectWork`, `CdcCounters`, the delta counters (`prefix_selected`, `full_losses`,
`no_candidate`, `work_exceeded`), `SaveCounters` and `StoreReadCounters`.

A fixture is an **input**, never part of the measurement. A driver handed no prepared
artifact fails closed with that reason rather than silently rebuilding — because a driver
that builds its fixture inside the timer is measuring setup.

## 3. Memory, CPU and disk — what is tracked, and the two gaps

| axis | instrument | status |
| --- | --- | --- |
| heap | counting `GlobalAlloc` → `heap.peak_incremental_bytes`, `heap_charged_bytes`, `heap_allocations` | exists — **measured phase only** |
| RSS | 10 ms sampler → `phase_peak_bytes`, `incremental_peak_bytes` | exists — measured phase only |
| lifetime RSS | `ru_maxrss` → `lifetime_peak_rss_bytes` | exists; a **lifetime** value, never substituted for a phase peak |
| **CPU** | `CpuReading { user_ns, system_ns }` from `getrusage(RUSAGE_SELF)`, `cpu_now()` | **instrumented but never published** — no driver calls it |
| swaps | `swaps()` plus `gates::swap_gate` in every C1/C2 driver | exists |
| disk | `st_blocks × 512`, `st_size`, `page_count`, `freelist_count`, `pack_bodies`, `object_rows`, `catalogue`, `schema_shape`, `quick_check`, `sidecars` | exists |
| disk I/O | `disk_read_bytes` against `requested`, for a de-warmed claim | exists |

The sampler's interval cannot cover a phase shorter than ~200 ms, so a missed sample or an
excessive gap makes the peak **unavailable** and the row `INELIGIBLE` — never quietly
fast.

**Two gaps, and they have the same fix.** CPU time is never published, and preparation's
memory is never published: `delta_prepare` (`ops/c2.rs:818`) emits three counters and
nothing else. Both are closed by having the `prepare` and `perf` invocations bracket
`cpu_now()` around each phase and publish `cpu.user_ns` / `cpu.system_ns`, plus
`preparation_peak_heap_bytes` and `preparation_peak_rss_bytes` on the prepare invocation.
See [`preparation.md`](preparation.md) §7.

**CPU is a diagnostic, like `operation_ns`.** `getrusage(RUSAGE_SELF)` is process-wide
and cumulative, so a phase's CPU is a difference of two readings, and it is only
meaningful because `AGENTS.md` §3.8 mandates a single construction worker. It never
gate-decides; counters, heap and disk do.

## 4. The report shape

One row per lane, with the runtime columns dropped because they are Stage 7's claim:

```text
lane · states · rows
canonical content bytes / objects          pinned, gated
verified path-states / logical bytes       pinned, gated
Store allocated / apparent                 O6
attribution: pack bodies by role · sqlite non-pack · allocation difference
sum(operation_ns) · median per state       the golden number
preparation · acquisition · verification · cleanup · complete command
peak incremental heap · phase peak RSS
Git53 · Git157                             cited constants
v0.1.6 Store allocated                     labelled reference point, not a comparison
```

`sum(operation_ns)` is published beside the per-row numbers so that a row which got
faster at another row's expense is visible rather than averaged away. It is also the
campaign's **falsifier**: if the operation total falls when preparation is optimised,
measured work moved into setup and the change is rejected.

The report's time axis is `operation_ns`. It is **never** the process wall under a time
label — the 217-row harness shipped that defect (`shared/analyze.py` printed
`max(row.wall_ns)` under the header `time max ms`), and this campaign does not inherit it.

## 5. Lanes and budgets

| lane | rows | selection |
| --- | --: | --- |
| `history-stride10` | 17 | `range(1,158,10) ∪ {157}` |
| `history-stride3` | 53 | `range(1,158,3)` |
| `history-stride1` | 157 | all checkpoints |

The 217-row `full` lane is **unchanged**: history rows are selected only by their own
lane, so lane compositions stay comparable and round-5 baselines stay valid.

| budget | limit |
| --- | --- |
| complete command, per row | ≤ 15 s; declared exceptions ≤ 25 s |
| verification, per row | ≤ 60 s, its own scope |
| preparation | published, not budgeted per row; per-lane budget declared after stride-10 is measured |

No tier is shrunk, no timeout inflated and no worker added to make a row fit. A row that
cannot fit is recorded as `NOT_RUN` with its measured wall and the reason.

`benchmark_rules.md` §15 also governs what may run by default: a large-history lane is
explicitly selectable and **no default invocation launches stride-1**.

## 6. Sampling policy

One sample per case per arm, fresh `--output` per run, receipts append-only, measurement
lock held for the whole invocation. No n3 and no best-of selection; diagnostics are
labelled as diagnostics and reported beside the gate sample.

For an optimization campaign the #118 regression rule applies prospectively: three fresh
alternating pairs, median paired slowdown greater than `max(15 % of the control median,
3 ms)` and at least two of three pairs slower. Every attempt is retained; no arm-only
retry and no outlier deletion.

## 7. Identity on every receipt

| field | why |
| --- | --- |
| source commit and tree seal | a rebuilt artifact needs a rebuilt matched arm |
| product seal, compilation seal, dependency seal | the product under test |
| harness binary sha256 and harness lock sha256 | the harness is a measured input |
| registry TSV sha256 | which rows ran |
| **corpus manifest sha256 and pinned tip** | this campaign's external input |
| `construction_workers` | must be `1`; no run raises it |
| `clone_method`, `allocation_attribution` | which rung, and whether allocated bytes are attributable |
| `cache_state` | states are never pooled |

A harness change invalidates the pair. A corpus change invalidates the prepared root.
