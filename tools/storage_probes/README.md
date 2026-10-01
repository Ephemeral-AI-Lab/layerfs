# Standalone MinIO and SQLite probes

Status: Research; informative and not a product contract.

Experiment [#291](https://github.com/Ephemeral-AI-Lab/layerfs/issues/291).
The [frozen specification](../../docs/roadmap/0.1/0.1.7/issue290/EXPERIMENT-V1.md)
declares cases, cache interpretation, expected results and scope.

Run from the owned repository root using Python 3.11 or newer. This tooling uses
the standard library; it needs no Cargo build, Docker image or daemon. The pinned
native MinIO provider is for macOS ARM64. Record the actual Python-linked SQLite
version, which can differ from Apple's SQLite and the LayerFS provider.

## Prepare once

```sh
python3 tools/storage_probes/run.py --list
python3 tools/storage_probes/acquire.py
python3 tools/storage_probes/minio_probe.py start --root benchmark-results/storage-probes/provider
python3 tools/storage_probes/minio_probe.py setup --case M-directory-10000-v1 --root benchmark-results/storage-probes/provider
```

Acquisition verifies the exact community bottle hash, extracts only the native
binary and records its version/hash. It never installs globally. An incomplete
download is retained and fails hash validation; acquisition has no retry loop.
Retain an incomplete acquisition as evidence. A later corrected acquisition can
use a fresh provider root via `acquire.py --root <new-root>`; pass that same root
to setup/start/stop and `run.py --provider <new-root>`. Do not overwrite or
automatically retry the failed artifact.
Do not run acquisition concurrently with an existing download. Server startup
creates private credentials, loopback ports and an owned bucket, then performs
an untimed transport check on a distinct object. Listing setup creates and
qualifies its 10,000-key master once. Other cases need no object preparation.
Do not regenerate or replace an existing master/provider owner.

The master preparation is outside throughput timers; its wall may exceed a
performance selection's 15-second limit. Reuse is visible in the source and
provider seals and does not imply cold cache. Keep private-config.json and
startup-private.log under the ignored output root; never publish them.

## Select fast, bounded work

After committing the tool source, collect one selected case:

```sh
python3 tools/storage_probes/run.py --engine sqlite --case S-tiny-1024-b128-v1 --output benchmark-results/storage-probes/sqlite-first
python3 tools/storage_probes/run.py --engine minio --case M-grouped-1024-v1 --output benchmark-results/storage-probes/minio-first
```

SQLite selections need no MinIO preparation. Omit `--case` to run all four cases
of the selected engine once. Use a fresh output path for a new collection; the
runner rejects an already attempted case and a changed identity. Source must
have no tracked edits. An unchanged arm must not be collected again just to get
a better number. Per-worktree locking prevents overlapping this runner's cases.
Record any unrelated host interference rather than interrupting other work.

Each performance child has a 15-second wall limit. Its independent verification
and case cleanup have a separate 10-second limit. A timeout stays visible and is
not retried with larger limits or smaller workloads. The parent retains exact
commands, exits, walls and logs even when the child fails. A zero runner exit
alone is not a verdict: inspect each command receipt.

The fixed first pass answers request and transaction granularity questions.
The 1 KiB tiny-object and four-object aggregate cases have equal logical bytes.
They do not measure a LayerFS pack format. SQLite's 1-versus-128 batching cases
have equal metadata populations. SQLite stores file attributes and range
locators, with indexed parent/name, inode and file/offset access, not file bodies.
Deletes are separately timed cleanup, not credited to the operation phases.

## Read receipts and finish

`source.json` records source commit, Python, SQLite, platform and tool hashes.
Each case has `performance-command.json`, `performance.json` when complete,
`verification-command.json`, and `verification.json` when proof completes.
Performance JSON gives stage times, ops/s, MiB/s and counts. LIST ops/s means
entries/s; page-query counts are separate. HTTP byte counters cover bodies,
not headers, physical disk writes or erasure amplification. RSS observations
are current snapshots, not phase peaks or a memory bound.

Expected keys, SQL rows and body hashes come from deterministic reference
generation. Proof checks exact results; cleanup removes case objects/rows and
retains the empty database file for inspection. The directory master remains
until final owned provider cleanup:

```sh
python3 tools/storage_probes/minio_probe.py stop --root benchmark-results/storage-probes/provider
```

Stop records the cleanup outcome and the owned server's termination outcome.
Retain receipts, the provider seal and any failed case artifacts. Do not delete
other worktree outputs or discover/adopt an unrelated MinIO service.

Numerical rows are explicitly cache `INELIGIBLE`, `performance_claim=false`:
GET follows PUT, SQL reads follow insertion, and server/OS residency is unknown.
These are exploratory cost observations. They cannot establish cold speed,
durability, physical memory bounds, concurrent writer behavior or LayerFS
release admission. SQLite's MEMORY journal / synchronous OFF profile is for
runtime metadata; a durable global catalog needs its own declared experiment.

No throughput samples exist at harness preparation. pjdfstest, smallfile,
mdbench, fio, fsstress and fsx are deferred to the later mounted-filesystem stage.

First collection: [2026-10-01 report](../../docs/roadmap/0.1/0.1.7/issue290/RESULTS-20261001-V1.md).
The report links exact receipts and limitations; harness preparation status
above remains a historical checkpoint.

## Four-group follow-up (v2)

The [prospective v2 specification](../../docs/roadmap/0.1/0.1.7/issue290/EXPERIMENT-V2.md)
registers24 selections. Build the existing public C1/C2 example once, outside all
measurement windows, with the owned target and repository ARMv8 flags:

```sh
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-storage --example minio_pack_probe
python3 tools/storage_probes/pack_probe.py setup --fixtures benchmark-results/storage-probes/v2-pack-fixtures --binary core/target/release/examples/minio_pack_probe
python3 tools/storage_probes/catalog_probe.py setup --fixtures benchmark-results/storage-probes/v2-catalog-fixtures
python3 tools/storage_probes/run_v2.py --list
```

Setup roots are acquired/qualified once and reject replacement. Reuse the sealed
native MinIO binary in a fresh owned provider root; acquire/start is unchanged,
but v2 needs no10,000-key master. Source must be committed before collection.
For this collection the owned provider root is
`benchmark-results/storage-probes/provider-v2`.

```sh
python3 tools/storage_probes/run_v2.py --group pack --provider benchmark-results/storage-probes/provider-v2 --output benchmark-results/storage-probes/collection-v2
python3 tools/storage_probes/run_v2.py --group publication --provider benchmark-results/storage-probes/provider-v2 --output benchmark-results/storage-probes/collection-v2
python3 tools/storage_probes/run_v2.py --group concurrency --provider benchmark-results/storage-probes/provider-v2 --output benchmark-results/storage-probes/collection-v2
python3 tools/storage_probes/run_v2.py --group scaling --provider benchmark-results/storage-probes/provider-v2 --output benchmark-results/storage-probes/collection-v2
python3 tools/storage_probes/minio_probe.py stop --root benchmark-results/storage-probes/provider-v2
```

`--case` restricts a group to one registered selection. Each attempted case gets
performance15s and independent proof/cleanup10s limits, logs and explicit status.
The FULL/fullfsync profile is confined to the standalone catalog tool; LayerFS's
MEMORY/OFF profile stays unchanged. Queued metadata and native BUSY are different
registered capabilities. Body gates/events establish actual incomplete HTTP
requests, blocked admission or held SQL work; no sleeps establish overlap.
Actual source/build/provider seals, original fixture references and expected
outcomes accompany the results. No cold, peak-memory or power-loss claim follows.
