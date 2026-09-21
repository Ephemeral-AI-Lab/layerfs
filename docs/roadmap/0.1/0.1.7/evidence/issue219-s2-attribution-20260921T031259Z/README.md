# S2 — attribution: anchor vs small files, phases, host vs container (#219)

> **Status:** S2 deliverable. **No product change.** Every cell is either sourced to a
> raw receipt or marked `NOT_MEASURED`. The mechanism named in §4 is a mechanism, not
> a correlation; §5 states what could not be separated and why.

Sources: the two sealed S1 rows (identical source/product/compilation/dependency
seals, image and harness — see
[`../issue219-s1-reproduction-20260921T031259Z/`](../issue219-s1-reproduction-20260921T031259Z/)),
plus the historical `init_namespace` rows inventoried in
[`../issue219-s0-custody-20260921T031259Z/`](../issue219-s0-custody-20260921T031259Z/).

## 1. The case's composition, from the product's own manifest

`namespace-10000`: 10,000 regular files, 100 data directories, 300,000,000 logical
bytes, of which **1 file / 100,000,000 bytes is the anchor** (`anchor_files: 1`,
`anchor_bytes: 100000000`) and 9,999 files carry the other 200,000,000 bytes. The
fixture is skewed by design (`fixture_profile: synthetic-small-heavy-v2`):
7,899 tiny + 1,500 small + 500 medium + 100 empty + 1 anchor.

## 2. Phases

| phase | pseudorandom | `-text-v1` | source |
| --- | ---: | ---: | --- |
| setup (Store create, client, binding) — *outside the timer* | 250.0 ms | 263.7 ms | `records[0].setup_ns` |
| **timed** `layerstack_init_ns` | **944.881 ms** | **708.991 ms** | `records[0].layerstack_init_ns` |
| teardown — *outside the timer* | 249.1 ms | 1.0 ms | `records[0].teardown_ns` |
| complete command wall | 6.152 s | 5.705 s | `sample.wall_ns` |
| of which the product command window | 1.457 s | 1.024 s | `sample.command_wall_ns` |
| `exec` / `sdk` / `commit` | `NOT_MEASURED` | `NOT_MEASURED` | this family's route is `namespace`, not `workspace`; there is no exec/sdk/commit split for `init_namespace` and no receipt field for one |
| container command-window CPU | 12.0 ms | 14.5 ms | `resources.command_window_cpu_ns` |

The timer covers initialization only. Setup and teardown are reported outside it, so
the 944.881 / 708.991 ms figures are not inflated by them.

## 3. Where the time goes: bytes, and specifically *incompressible* bytes

The two S1 rows are the one controlled contrast this case offers: **identical file
count (10,000), identical scanned bytes (300 MB), identical build and image — the
content differs** (pseudorandom bytes vs structured text).

| | pseudorandom | `-text-v1` | ratio |
| --- | ---: | ---: | ---: |
| `layerstack_init_ns` | 944.881 ms | 708.991 ms | 1.333× |
| user CPU | 1036.7 ms | 1150.4 ms | 0.901× |
| system CPU | 752.0 ms | 381.6 ms | 1.971× |
| **user + system** | **1788.8 ms** | **1531.9 ms** | **1.168×** |
| `store_database_bytes` | 304.8 MB | 8.5 MB | **35.9×** |
| `store_growth_bytes` | 304.7 MB | 8.4 MB | **36.2×** |
| `inserted_objects` | 25,158 | 22,242 | 1.131× |
| `initialize_admission_transactions` | **73** | **73** | **1.000×** |
| `initialize_max_transaction_bytes` | 4.15 MB | 4.16 MB | 1.00× |
| wall per byte | 3.150 ns/B | 2.363 ns/B | 1.333× |
| CPU per byte | 5.963 ns/B | 5.106 ns/B | 1.168× |
| wall per file | 94.49 µs | 70.90 µs | 1.333× |
| CPU per file | 178.88 µs | 153.19 µs | 1.168× |

**Mechanism.** The two rows do the same number of *files* of work — the admission
transaction count is **exactly 73 in both**, as is the maximum single-transaction
size — and they differ by 36× in how many bytes the Store must durably hold. The
235.890 ms wall-time difference is carried almost entirely by **system CPU**
(+370.5 ms), while user CPU is 113.6 ms *lower* on the slower row: the compressible
content costs more user-space CPU to compress and far less system CPU to write.
That is a store-write cost, not a traversal or metadata cost.

**Marginal per-byte cost (derived, incompressible vs compressible at 300 MB):**
235.890 ms / 300 MB = **0.786 ms per MB**. This is a *derived* figure from a contrast
in content compressibility, not an isolated measurement of a byte-path treatment; it
is labelled derived, not measured.

## 4. The anchor

**The 100 MB anchor does not flow through the admission-transaction path.**
`initialize_max_transaction_bytes` is **4.15 MB** (pseudorandom) and **4.16 MB**
(`-text-v1`), while the anchor alone is 100 MB — **24× larger than the largest single
admission transaction in either row**. The mean transaction is 4.14 MB across 73
transactions (10,000 files ⇒ ≈137 files per transaction), so the transaction geometry
is set by the small files, not by the anchor.

| cell | value | source |
| --- | ---: | --- |
| anchor bytes / files | 100,000,000 / 1 | `records[0].anchor_bytes`, `anchor_files` |
| small-file bytes (scanned − anchor) | 200,000,000 | derived from `scanned_bytes − anchor_bytes` |
| largest single admission transaction | 4.15 MB | `initialize_max_transaction_bytes` |
| anchor as a multiple of that | 24.1× | derived |
| anchor's share of `layerstack_init_ns` | **`NOT_MEASURED`** | no receipt field isolates it; no anchor-size treatment was run |
| anchor's share of `scanned_bytes` | 33.3 % | derived |
| anchor's share of `store_growth_bytes` | **`NOT_MEASURED`** | `store_growth_bytes` is one total |
| small files' share of `layerstack_init_ns` | **`NOT_MEASURED`** | same |

An upper bound, stated as a bound and not as a measurement: if the whole 944.881 ms
were charged per byte at the marginal rate of §3, the anchor's 100 MB would account
for ≈78.6 ms, i.e. **≈8.3 %**. This is an extrapolation from a compressibility
contrast, and it assumes the anchor's per-byte cost equals the small files' — which
§5 shows has not been established. It is not used as a gate anywhere.

## 5. Per-file vs per-byte: what separates and what does not

**Separates (measured, within one build):** per-file *admission* work is
content-independent. 73 transactions in both rows at identical max-transaction size,
against a 36× difference in stored bytes, is a direct observation, not an inference.

**Does not separate (not measured):** the marginal cost *per file* versus the
marginal cost *per byte* within the 10,000-file row. The case fixes file count and
byte count together (10,000 files **and** 300 MB), so no single case varies one while
holding the other. The only within-build contrast available varies **compressibility**,
which is a byte-path property; it does not move file count at all.

Historical cross-build scaling — every row from a **different** product seal, cache
state and object count, so this table is `NOT_MEASURED`-grade for a cost model and is
shown only to record why it cannot carry one:

| case | files | logical MB | median ms | ns per byte | µs per file | candidate objects |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `namespace-100-compact-v3` | 100 | 5 | 9.352 | 7.34 | 93.52 | 663 |
| `namespace-1000-compact-v3` | 1,000 | 20 | 39.891 | 7.24 | 39.89 | 4,961 |
| `namespace-10000` (S1, sealed) | 10,000 | 300 | 944.881 | 11.27 | 94.49 | 25,158 |
| `namespace-100000` | 100,000 | 500 | 3,769.591 | 7.01 | 37.70 | 112,451 |

Marginal costs between adjacent tiers are not stable (per-file marginals 30.5 µs then
100.6 µs; per-byte marginals 18.7 ns/B then 3.4 ns/B), and the 100-file tier is 6 MB
where timer resolution and the 1 MB anchor dominate (S0's source handoff says the
same). **A clean per-file/per-byte separation needs a controlled treatment — same
file count with a different anchor size, or the same bytes as few files — and none
was run.** It is pre-registered in §7 rather than guessed here.

## 6. Host vs container

| scope | pseudorandom | source |
| --- | ---: | --- |
| host Store process, user + system CPU | **1788.8 ms** | `records[0].initialization_{user,system}_cpu_ns` |
| host process peak RSS | 72.2 MB (from 5.0 MB at t0) | `process_t1_peak_rss_bytes`, `process_t0_rss_bytes` |
| container lifetime peak | 5.7 MB | `resources.sample_container_lifetime_peak_bytes` |
| container command-window CPU | 12.0 ms | `resources.command_window_cpu_ns` |
| host process disk read | 0.73 MB | `initialization_disk_read_bytes` |
| host process disk write | 2.97 MB | `initialization_disk_write_bytes` |
| container swap | 0 B | `resources.swap_current_bytes` |
| OOM kills | 0 | `resources.oom_kill_delta` |

`initialization_{user,system}_cpu_ns` are `getrusage(RUSAGE_SELF, …)` deltas
(`benchmark/fs-bench-pro/src/main.rs:225-232`, `native_peak_rss_and_swaps_for(0)`),
i.e. **process-wide including the initialization worker threads** — consistent with
CPU/wall = 1788.8 / 944.9 = **1.89×**, the parallelism actually achieved. The
container is 12.0 ms of that work (0.7 % of the host process's 1788.8 ms): **the cost is
on the host, not in the container**, because the Store and the namespace scan are host-side on this topology
(`topology: host-store`, and the runner records
`resource_limit_scope: "Linux container only; host CPU is not capped"`).

The disk counters are worth stating plainly: 0.73 MB read against 300 MB scanned.
**Nothing in the timed phase paid a storage read** — consistent with S1 §3's cache
declaration and the reason these rows are not cold rows.

## 7. Pre-registered treatment (not run)

If S3/S4 requires the per-file/per-byte split, the pre-registered one-difference arms
are, in order of preference:

1. **Anchor-size treatment at fixed file count.** Same 10,000 files, same build, same
   cache declaration, anchor 100 MB vs a different anchor, reporting
   `layerstack_init_ns` and `store_growth_bytes` for each. Requires a registered case
   or a fixture at the non-registered anchor size — **not available today**; the
   registered scenarios fix the anchor (`NAMESPACE_ANCHOR_BYTES = 100_000_000` for
   both 10,000- and 100,000-file tiers).
2. **File-count treatment at fixed bytes.** Same bytes as few files. **Not
   available**: no registered case holds bytes constant across file counts.
3. **Anchor-excluded control.** A fixture with the anchor removed and the same 9,999
   small files, which prices the anchor directly. **Not registered.**

Each would be one difference, one sample per arm, fresh `--output`, with the identity
it is compared against declared before it runs.

## 8. Gate

> *Gate:* every cell sourced or marked `NOT_MEASURED`; a written mechanism, not a
> correlation.

Met. §3 names the mechanism with the counters that carry it (36× store growth,
identical 73-transaction geometry, system-CPU-dominated delta). §4 and §5 mark the
anchor's share and the per-file/per-byte separation `NOT_MEASURED` rather than
inferring them, and §7 pre-registers the treatments that would close them.
