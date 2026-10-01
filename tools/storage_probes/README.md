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

Four-group results: [2026-10-01 v2 report](../../docs/roadmap/0.1/0.1.7/issue290/RESULTS-20261001-V2.md).

## Full closed repository import

The [prospective contract](../../docs/roadmap/0.1/0.1.7/issue290/EXPERIMENT-DEEPSEEK-FULL-V1.md)
includes every captured entry under deepseek-harness, including .git, ignored
files and symlinks. The existing closed independent master is reused; never
regenerate it for a sample. This tool writes no source files. Private catalog
contains all opaque metadata plus real C1 file content roots and C2 locators;
MinIO holds actual packed FULL representations. It does not produce a canonical
filesystem namespace root, Branch or Commit, and performs no mount/daemon work.

```sh
cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release -p layerfs-storage --example minio_repository_probe
DYLD_LIBRARY_PATH="$PWD/benchmark-results/storage-probes/sqlite-3.51.3-provider" python3 tools/storage_probes/repository_selftest.py --root benchmark-results/storage-probes/repository-selftest-v1
python3 tools/storage_probes/repository_probe.py run --output benchmark-results/storage-probes/deepseek-full-import-v1
```

The self-test is correctness only, on four synthetic files, and never a throughput
sample. Its fresh root rejects reuse. The full run requires committed clean source,
a fresh output, sealed MinIO and fixed SQLite3.51.3 providers, the closed manifest
identity and at least16GiB available disk. Native C1/C2 preparation has180s; upload
and WAL/FULL/fullfsync publication have25s including final checkpoint; download,
exact pack comparison and public C1/C2 reconstruction share one10s proof. Each
stage is attempted once; timeouts and partial ACK/proof progress remain evidence.
Only provider/worker cleanup runs after a miss; no automatic resend or completion
adoption. Packs, private catalog and owned MinIO data remain in the output for the
requested import. Credentials and captured content must stay private.

Construction has one producer and indexed CAS lookups, one unfinished record
group and one pack tail per active lane. Group targets48KiB, native/ordinary
ceilings64KiB, whole-file groups bounded by256KiB packs, records<=1024 and groups
<=256. Record/ID vectors are bounded by those limits, not corpus counts. Private
SQL transactions commit at most256 new objects or256 file-root updates per batch;
cache512KiB/mmap0, MEMORY/OFF staging. SQLite engine/OS memory remains unobserved.
Upload has four connections, eight queued/outstanding tasks, at most four256KiB
body allocations. Proof has four original+download body pairs, eight256KiB decoded
pack-cache entries, the existing512KiB group cache and bounded32-object C1 waves.
These are representation/window arithmetic, not measured physical-memory bounds.

Cache state is INELIGIBLE and performance_claim=false. Preparation, sealed catalog
hashing, upload and proof wall are reported separately. No full SDK Init, cold
bootstrap, power-loss durability, cross-process cloud fencing or speed admission
follows from this standalone experiment. Product source is unchanged.

The original full-import proof timed out; its receipt remains failed. The
[prospective remainder diagnostic](../../docs/roadmap/0.1/0.1.7/issue290/DEEPSEEK-RECONSTRUCTION-DIAGNOSTIC-V1.md)
finishes the original coverage in22 keyset batches,4096 files each except the last,
10s per command, without repeating uploads or claiming aggregate speed:

```sh
python3 tools/storage_probes/repository_reconstruction.py --output benchmark-results/storage-probes/deepseek-reconstruction-remainder-v1
```

Raw cursor files stay private; safe batch receipts contain counts/cursor hashes.
The original pack/metadata/first13,312-file proof is reused with identity seals.
