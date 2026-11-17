# GC capacity benchmark home (item-44)

> **Status:** measurement harness for the GC spec section 13 arms. It adds no
> contract; `LayerFS-GC-spec.md` wins. The measured numbers in
> [`gc-42-decisions.md`](../../core/docs/issues/project-41/gc-42-decisions.md)
> and [`gc-runbook.md`](../../core/docs/issues/project-41/gc-runbook.md) were
> taken on the earlier whole-file-only fixture; see
> [Representativeness](#representativeness-caveat).

## Where it lives

- `engine/crates/layerfs-persistence/examples/gc_bench.rs` — the Cargo example
  target, measurement loop and JSON report.
- `engine/crates/layerfs-persistence/examples/gc_bench/gc_fixture.rs` — the
  generated Store fixture, pack probe, dead-share setup and temp sampler. It is
  a module of the example, not a second target.
- `tools/ci/gc_capacity_final.sh` — the runbook entry point: pins the
  environment, checks free space, builds `--release`, refuses the owner-only
  arm, runs the example and validates the JSON.

## Fixture shape

The fixture is built through the public persistence/storage APIs. It is no
longer "one whole-file object per pack and one genesis Layer per project":

- **History depth.** `stacks` LayerStacks (projects) each publish `layers`
  Layers (16 projects x 52 Layers for the 10 GB arm). A live Stack retains all
  its Layers, so the mark closure walks real history rather than a single
  genesis root per project.
- **Shared content.** A pool of whole-file objects is saved once and referenced
  by the inode leaves of every project and Layer (12 slots per 99-entry leaf,
  2500 shared objects at 10 GB). One object has many references instead of one
  reference per object.
- **Delta chains.** Selected inode leaves reference a chain target whose bases
  are reached only through their stored PREFIX records. Chain depths vary
  (1..4 edges), so the report carries a depth histogram. The fixture publishes
  the base as FULL and each successor as PREFIX through the public `publish`
  API (the same construction the compaction fixtures use) and asserts that the
  representation is PREFIX, so a chain is deterministic and never silently
  falls back to FULL records.
- **Dead payload.** Whole-file objects that no retained root reaches, saved in
  Saves that hold nothing live, so every pack holding a dead record is wholly
  dead. `SELECT count(*), sum(length) FROM pack` therefore stays the
  exact pack-byte population and the section 11 dead-byte bound/refined fields
  can be checked against it.
- **Content objects.** Every content object is still one whole-file payload of
  `object_bytes` (<= 131,071 B). Chunked files, extent trees, merges and retired
  Branches are not built; see the caveat below.

Only the Disposable (WAL/synchronous=OFF) profile is built and measured: owner
direction 2026-10-07 disabled Durable execution indefinitely.

## Scales

| Scale | Fixture | Runs per arm | Invocation |
| --- | --- | --- | --- |
| `small` | ~28 MB live, 4 projects x 4 Layers | 3 | default; part of the ordinary small run |
| `medium` | ~9.8 GB live, 16 projects x 52 Layers (the T-G61 shape) | 3 | opt-in, positional `medium` (requires `GC_BENCH_MEDIUM=1`) |
| `final` | ~45.6 GB live, 32 projects x 124 Layers (the T-G66 shape) | 1 | **owner-only, NOT run by CI or agents** |

The medium and final arms are never built by `cargo test` or by a default run.
The final arm additionally needs `GC_BENCH_OWNER=1`, `GC_BENCH_OWNER_APPROVED=1`
and `GC_BENCH_FINAL_ACK=1`; without them the example exits 2 and the script
exits 3 and prints the exact owner command. **No 50 GB run was done for this
change; the arm stays gated.**

## Environment

```bash
export PATH=/usr/local/cargo/bin:$PATH
export RUSTUP_HOME=/usr/local/rustup
export CARGO_HOME=/tmp/cargo
export CARGO_TARGET_DIR=/tmp/target-gc-bench
export TMPDIR=/path/to/large/scratch     # >=3.4x the live bytes
export CARGO_NET_GIT_FETCH_WITH_CLI=true # flaky HTTPS git on the container host
```

`TMPDIR` is the run's scratch filesystem and must not be inside the
repository. The fresh-copy rebuild needs the old Store plus about one new
Store: >=110 GB for the 50 GB arm (with margin) and >=22 GB for the 10 GB arm;
the fixture and its artifacts raise the practical minimums to >=155 GB free for
the 50 GB arm and >=34 GiB for the 10 GB arm. The report records the scratch
filesystem type (`host.tmpFilesystem`), so a run on tmpfs is visible in the
receipt.

## Commands

```bash
# Small (default): builds the release example and runs both arms, 3 runs each.
tools/ci/gc_capacity_final.sh small

# Opt-in 10 GB arm, cold cache, 3 runs per arm (the agent budget).
GC_BENCH_MEDIUM=1 tools/ci/gc_capacity_final.sh medium --cold-cache \
  --out /var/tmp/sqlite_compact_capacity_measure_10gb.json

# Opt-in 10 GB arm under a caller-owned cgroup memory limit instead.
GC_BENCH_MEDIUM=1 tools/ci/gc_capacity_final.sh medium --cold-cache \
  --cold-method cgroup-memory --out /var/tmp/sqlite_compact_capacity_measure_10gb_cgroup.json

# Owner-only 50 GB arm: NOT run by CI/agents. The owner runs:
GC_BENCH_OWNER=1 GC_BENCH_OWNER_APPROVED=1 GC_BENCH_FINAL_ACK=1 \
  GC_BENCH_HOST_CLASS='Dell PowerEdge R730 / PERC / rotational (not NVMe)' \
  GC_BENCH_NOMINAL_READ_MBS=500 GC_BENCH_NOMINAL_WRITE_MBS=600 \
  tools/ci/gc_capacity_final.sh final --out /var/tmp/sqlite_compact_capacity_measure_50gb.json

# Build-only check (no measurement).
tools/ci/gc_capacity_final.sh --check
```

The example can be run directly too:

```bash
cargo build -p layerfs-persistence --release --example gc_bench
target/release/examples/gc_bench --scale small [--cold-cache] [--out FILE]
```

## Cold cache and run-to-run variance

The section 13 rule is at least three runs per arm with median **and range**. The
report keeps every run, the median, min/max, the population standard deviation
and `(max-min)/median`, so the 3-run spread is visible without re-deriving it:

```json
"variance": {"runs": 3, "wallSecondsStdDev": 1.234, "wallRelativeRange": 0.012}
```

Cold-cache methods (`--cold-cache` enables the attempt; `--cold-method M`
selects it; `cache.method` and `cache.state` record what happened):

- `drop_caches` (default). `sync` then write `3` to
  `/proc/sys/vm/drop_caches` before every measured run. It needs privilege; a
  refusal is recorded as `warm (drop_caches refused)` and the run continues, so
  a non-root run never claims a cold cache.
- `cgroup-memory`. The harness must be launched inside a cgroup whose
  `memory.max` is a finite limit below `MemTotal` (for example
  `systemd-run --scope -p MemoryMax=4G ...`). Page cache the run faults in is
  charged to that cgroup and the kernel reclaims it at the limit. The harness
  only verifies and records `memory.max`/`memory.current`; it does not change
  its own cgroup (that could throttle the process supervising it). If
  `memory.max` is `max` or absent, the run is recorded warm with the reason.

Report `cache.state` is `cold (drop_caches)`, `cold (cgroup-memory)` or one of
the `warm (...)` forms; `cache.applied`/`cache.refused` count the runs.

A medium receipt is only comparable on the same host class, the same cold
method, the same `TMPDIR` filesystem and the same `stacks`/`shared`/`delta`
fixture knobs. Record all of them beside the JSON.

## Host class and cache

The container does not expose DMI or `/sys/block`, so the owner declares the
host class with `GC_BENCH_HOST_CLASS` and the nominal sequential rates with
`GC_BENCH_NOMINAL_READ_MBS` / `GC_BENCH_NOMINAL_WRITE_MBS`. The report records
both the declared class and the detected one, and sets `storageMedia` and
`notNvme` from the declaration (the string "not NVMe" is recognised as
non-NVMe). The owner target class is:

> Dell PowerEdge R730 / PERC / rotational, ~500/600 MB/s sequential, **not
> NVMe**.

Do not apply the NVMe 50 GB `[M]` rates from the capacity model to this class
without re-measuring; the build phase is device-write-bound. Record the host
class beside every receipt.

## Reported fields

Per arm (`none`, `rebuild`), per run and as median/min/max/variance:

- `downtimeSeconds` / `wallMicros` — the compaction call's wall time;
- `phasesNanos` (`mark`, `build`, `verify`, `install`) and `phaseTotalNanos`;
- `peakTempBytes` — the section 11 receipt (the new Store + the visited
  scratch on the fresh-copy build) — plus `sampledPeakTempBytes` and the
  per-artifact `sampledTemp` (the staged file, journal, visited, rewrite tmp;
  the fresh path samples a zero journal and rewrite);
- `processIo.writeBytes` / `wchar` / `readBytes` / `rchar` — the process's
  `/proc/self/io` delta across the run;
- `newFileBytes`, `oldFileBytes`, `physicalShrink`, `copyBytes` and
  `bytesCopiedPerFreedByte`;
- `cacheApplied`, `cacheMethod` per run;
- `perGb`: seconds per stored GB, per live GB and per freed GB, mark and build
  MB/s, verify seconds per new GB, install seconds, and new bytes per freed
  byte.

The `fixture` object records the realized shape, not just the prose: `groups`,
`stacks`, `layersPerStackMin`/`Max`, `historyLayers`, `historyCommits`,
`groupSize`, `objectBytes`, `freshObjects`, `sharedObjects`,
`sharedReferences`, `deltaChains`, `deltaChainObjects`, `deltaChainMaxDepth`,
`deltaChainsByDepth`, `liveObjects`, `livePayloadBytes`, pack bytes/physical
bytes, dead bytes and permille, build seconds/MB/s and the dead-object setup.
`host` and `cache` are described above.

## Representativeness caveat

The fixture is materially closer to a live Store than the first harness (real
Layer depth, cross-project shared content, delta chains kept as bases), but it
is still synthetic. Read a receipt with these limits:

- **Whole-file content only.** Every content object is a whole-file payload;
  chunked files, `FileState`/extent trees and chunk deltas are not built, so
  `markBytes`, `keptWaste` composition and the build copy rate do not include
  the chunk grammar.
- **Dead bytes live in wholly dead packs.** The fixture adds dead records in
  Saves that hold nothing live, so the rebuild drops those packs without
  copying. A real Store mixes dead and live records in one pack and can rewrite
  live bytes for a dead record; the measured `bytesCopiedPerFreedByte` is
  therefore a lower bound and the build phase is optimistic.
- **Synthetic sharing and chains.** The shared pool and the chain depths are
  generated, not sampled from a production Store; they exercise the mark's
  reference and delta-base paths but do not reproduce a real sharing ratio.
- **No merge, retired-Branch or `remove_file` history.** Those history edges
  and markers are covered by the contract tests, not by this timing fixture.
- **One profile.** Disposable (WAL/`synchronous=OFF`) only, per owner direction
  2026-10-07.
- **Small host sample.** The declared R730/PERC/rotational class is a single
  host; per-GB rates carry that class and the cold method.

The 50 GB owner-final numbers in `gc-42-decisions.md` / `gc-runbook.md` were
measured on the earlier fixture (whole-file-only, one genesis Layer per
project). They are evidence for that shape, not a measurement of the current
one; until the owner re-runs the gated final arm on this fixture, do not treat
them as the current T-G66 receipt.

## Validation run (not a capacity claim)

The arms were exercised once during the #711 fixture work to prove the harness
and script run end to end. The host was the generic container (detected class
`unknown / ssd-or-unknown`, **not** the R730/PERC class above), `TMPDIR` was
the container's overlay filesystem, and the process ran unprivileged (uid 501),
so `drop_caches` was refused and the report says `warm (drop_caches refused)`.
The 50 GB arm was not run.

| Arm | Live / pack bytes | Runs | Median | Range | Relative range |
| --- | --- | --- | --- | --- | --- |
| small `none` | 28 MB | 3 | 0.268 s | 0.268-0.319 s | 19.1% |
| small `rebuild` | 28 MB | 3 | 0.552 s | 0.551-0.616 s | 11.8% |
| medium `none` | 9.79 / 10.73 GB | 3 | 84.10 s | 81.55-84.51 s | 3.5% |
| medium `rebuild` | 9.79 / 10.73 GB | 3 | 206.87 s | 204.76-288.53 s | 40.5% |

The medium fixture realized 16 projects x 52 Layers (832 Layers, 816 Commits),
72,128 fresh objects, 2,500 shared objects with 9,984 references, 256 delta
chains (896 objects, depths 1-4), 75,524 live objects; 8.8% dead took the
`none` path and 10.0% took `rebuild`. With a warm cache the medium arm reports
8.59 s/GB live (`none`) and 21.14 s/GB live, 12.10 s/GB new and 189.2 s/GB
freed (`rebuild`). The rebuild arm's 40.5% relative range is why section 13
asks for three runs and a range: one of the three runs was 41% above the
minimum. These numbers are a harness receipt only; qualify a capacity number on
the declared R730 class with an applied cold method.

## Committed receipts and the two 10 GB runs

Two different generated fixtures carry the 10 GB evidence, and they are not
interchangeable:

- **Fixture A (9.987 GB pack), the `capacity_measure` test.**
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure.json`
  is written by the opt-in `sqlite_compact_capacity.rs#capacity_measure` test.
  On the superseded whole-Store `VACUUM INTO` build, `rebuild` was 246.371 s
  median (`none` 149.318 s, one `Durable` rebuild 284.078 s); that run's
  medians are kept in
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_whole_store.json`
  (a transcribed summary: the raw per-run samples were not retained). The
  fresh-copy build on the same fixture A is 118.439584 s median (`none`
  88.439 s). The same-fixture comparison is **2.08x**. The owner-final 50 GB
  arm on that build is
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_50gb.json`
  (`none` 1,633.585 s, `rebuild` 5,131.835 s, one run per path).
- **Fixture B (9.789 GB live, 10.731 GB pack, 16 projects x 52 Layers), the
  `gc_bench` example.** `tools/ci/gc_capacity_final.sh medium` ran three times
  per arm on 2026-11-16 and the receipt is committed as
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_gc_bench.json`:
  `rebuild` median **133.076 s** (runs 132.274-134.601 s, 1.75% relative
  range; mark 82.050 s, build 23.469 s, verify 6.060 s, install 21.011 s;
  peak temp 9.836 GB, shrink 1.093 GB) and `none` median 82.683 s. It is a
  different synthetic Store from fixture A, so 133.076 s is **not** the other
  side of the 246.371 s -> 118.439584 s comparison. An earlier PR body divided
  246.371 s by 133.076 s and called it 1.85x; that mixed the two fixtures and
  is withdrawn.

The [validation run](#validation-run-not-a-capacity-claim) table above was an
earlier harness exercise on the same fixture-B shape but a different host and
cache state; it is not this committed receipt.

### Section 13 measurement fields

The committed fixture-B receipt declares or records:

- `profile`: `SqlitePersistenceProfile::Disposable` - `journal_mode=wal`,
  `synchronous=OFF` (the `synchronous_commit` equivalent), `page_size=4096` as
  the creation profile sets it;
- bytes written: per-run `processIo.writeBytes` (median 9,839,857,664 B for
  `rebuild`, 7,086,080 B for `none`) and `wchar`;
- peak disk: `peakTempBytes` (9,836,421,120 B median) and `sampledTemp`;
- cache: `warm (drop_caches refused)` - 6 of 6 drops refused;
- host: declared R730/PERC/rotational class, detected `unknown / ssd-or-unknown`.

Recorded deviations from section 13: the `gc-bench/1` receipt does **not**
record the SQLite version or WAL bytes, and `page_size`/`synchronous_commit`
are declared from the creation profile rather than sampled with `PRAGMA` at
run time. A receipt for a capacity claim must add those four fields; the
fixture-A `capacity_measure` JSON has the same gap.

## Not a registered benchmark family

This is a measurement harness for the GC section 13 arms, not a new registered
`fs-bench-pro` family: it produces the per-host capacity receipt the capacity
model asks for, including the E3 median/range for the medium arm. The 50 GB arm
is a single run per arm, as #42 item 44 specifies, and its result stays
owner-only until the owner records it.
