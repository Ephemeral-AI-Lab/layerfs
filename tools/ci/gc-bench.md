# GC capacity benchmark home (item-44)

> **Status:** measurement harness for the GC spec section 13 arms. It adds no
> contract; [`gc-capacity-model.md`](../../core/docs/issues/project-41/gc-capacity-model.md)
> remains the capacity model and [`LayerFS-GC-spec.md`](../../core/docs/issues/project-41/LayerFS-GC-spec.md)
> wins.

## Where it lives

- `engine/crates/layerfs-persistence/examples/gc_bench.rs` — the Cargo example
  target, measurement loop and JSON report.
- `engine/crates/layerfs-persistence/examples/gc_bench/gc_fixture.rs` — the
  generated Store fixture, pack probe, dead-share setup and temp sampler. It is
  a module of the example, not a second target.
- `tools/ci/gc_capacity_final.sh` — the runbook entry point: pins the
  environment, checks free space, builds `--release`, refuses the owner-only
  arm, runs the example and validates the JSON.

The fixture is built through the public persistence/storage APIs. Every live
file is one largest whole-file object reached through one `InodeLeaf` and one
`FilesystemRoot`; every pack holds one object, so the pack table gives the exact
stored/dead byte population. Only the Disposable (WAL/synchronous=OFF) profile
is built and measured: owner direction 2026-10-07 disabled Durable execution
indefinitely, so this harness has no Durable arm.

## Scales

| Scale | Fixture | Runs per arm | Invocation |
| --- | --- | --- | --- |
| `small` | ~30 MB live | 3 | default; part of the ordinary small run |
| `medium` | ~10 GB live (the T-G61 shape) | 3 | opt-in, `--medium` (**requires `GC_BENCH_MEDIUM=1`**) |
| `final` | ~50 GB live (the T-G66 shape) | 1 | **owner-only, NOT run by CI or agents** |

The medium and final arms are never built by `cargo test` or by a default run.
The final arm additionally needs `GC_BENCH_OWNER=1`, `GC_BENCH_OWNER_APPROVED=1`
and `GC_BENCH_FINAL_ACK=1`; without them the example exits 2 and the script
exits 3 and prints the exact owner command.

## Environment

```bash
export PATH=/usr/local/cargo/bin:$PATH
export RUSTUP_HOME=/usr/local/rustup
export CARGO_HOME=/tmp/cargo
export CARGO_TARGET_DIR=/tmp/target-gc-bench
export TMPDIR=/path/to/large/scratch     # >=3.4x the live bytes
export CARGO_NET_GIT_FETCH_WITH_CLI=true # flaky HTTPS git on the container host
```

`TMPDIR` is the run's scratch filesystem. The fresh-copy rebuild needs the old
Store plus about one new Store: >=110 GB for the 50 GB arm (with margin) and
>=22 GB for the 10 GB arm. TMPDIR must not be inside the repository.

## Commands

```bash
# Small (default): builds the release example and runs both arms, 3 runs each.
tools/ci/gc_capacity_final.sh small

# Opt-in 10 GB arm.
GC_BENCH_MEDIUM=1 tools/ci/gc_capacity_final.sh medium \
  --out /var/tmp/sqlite_compact_capacity_measure_10gb.json

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

`--cold-cache` runs `sync` and writes `3` to `/proc/sys/vm/drop_caches` before
each measured run. Dropping needs privilege: when it is refused the run is
recorded as `warm (drop_caches refused)` and keeps going, so a non-root run
never silently claims a cold cache.

## Reported fields

Per arm (`none`, `rebuild`), per run and as median/min/max:

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
- `perGb`: seconds per stored GB, per live GB and per new GB, mark and build
  MB/s, verify seconds per new GB, install seconds, and new bytes per freed
  byte.

The `fixture` object records groups, object size, live objects, pack bytes,
physical bytes, dead bytes and permille, build seconds/MB/s and the dead-object
setup. `host` and `cache` are described above.

## Not a registered benchmark family

This is a measurement harness for the GC section 13 arms, not a new registered
`fs-bench-pro` family: it produces the per-host capacity receipt the capacity
model asks for, including the E3 median/range for the medium arm. The 50 GB arm
is a single run per arm, as section 13 specifies, and its result stays
owner-only until the owner records it.
