# Issue290 standalone storage throughput experiment v1

Status: prospective exploratory backend experiment. No LayerFS product,
release, strict memory, cold storage or durability admission is established.
Owner request: 2026-10-01; issue https://github.com/Ephemeral-AI-Lab/layerfs/issues/290.
Parent source: ffdfa022f21930f3e7325b95e6ae5c2b11b7d2a0.

## Scope and identities

Measure standalone MinIO S3 operations and standalone SQLite metadata workloads
on the macOS host. No daemon, sandbox, FUSE, CDC, codec experiment, new product
format or #288 campaign. All tooling/provider artifacts stay in this owned
worktree. The experiment does not enable any larger LayerFS profile.

MinIO provider: published Homebrew arm64_tahoe bottle for community
RELEASE.2025-10-15T17-29-55Z, SHA256
f399aa93691336acf3ef6f79c08f97f2d43784ecf14d1f535c77bd3813082ee1.
This is the pinned community provider, not a claim about current AIStor.
Bind an isolated instance to loopback with owned data, ports and credentials.
Record binary SHA, startup/version, filesystem/CPU/RAM and data-directory scope.
SQLite provider: Python stdlib sqlite3, actual loaded SQLite version/build
recorded at execution. This is distinct from the product's Apple provider.
Record Python/tool SHA and SQL/schema/query text in the source seal.

## Frozen selections

| ID | Declared workload | Timed operations |
| --- | --- | --- |
| M-directory-10000-v1 | 10000 precreated zero-byte keys under one prefix | ListObjectsV2 with max1000, all pages |
| M-big-64m-v1 | One64MiB body, streamed from one seeded64KiB memory block | PUT then GET with64KiB receive window |
| M-tiny-1024-v1 | 1024 independent keys, each1024 bytes | PUT all, GET all, LIST all |
| M-grouped-1024-v1 | Same1024 logical1024-byte bodies concatenated into four256KiB objects | PUT all, GET all, LIST all |
| S-directory-10000-v1 | 10000 entries in one parent with inode attributes | Batched insert,128 point lookups,128-row keyset listing, rename/delete |
| S-tiny-1024-b1-v1 | 1024 file metadata pairs | Insert transaction batch1, point/list/delete |
| S-tiny-1024-b128-v1 | Same metadata workload | Insert transaction batch128, point/list/delete |
| S-big-index-4096-v1 | Metadata locators for4096x16KiB ranges, one64MiB logical file | Insert,256 indexed point lookups,128-row keyset scan,delete |

Grouped objects are synthetic backend aggregates, not valid LayerFS packs or
CAS fixtures. SQLite stores metadata only; it does not store the64MiB file body.
Batch comparisons describe bulk engine work, not individual POSIX-call atomicity.
One producer/connection, no retries and no additional worker/concurrency arm.

## Measurement boundaries and cache contract

Each selected case executes once. No repetition, warmup sample, median/best-of
or unchanged resampling. An actual correction receives a new source inventory
and narrowly covering command; failures stay in the append-only results.

Stage-specific monotonic timers cover the actual HTTP or SQL operations,
client bindings/signing/loop overhead and body transmission/consumption. Fixtures,
schema/profile setup, provider startup, output writing and independent proof are
outside operation timers and separately recorded. Complete performance command
is <=15s. Verification+owned case cleanup is separate and <=10s. Any timeout,
missing cell or unsupported capability remains explicit, never shrunk/rerun.

Input source is declared seeded synthetic memory data, not a filesystem read
claim. MinIO is loopback HTTP, single host/data directory and reused authenticated
HTTP connection. No network/cluster/TLS or durable-file-write speed claim.

Server/client OS cache and SQLite native/page-cache residency are not proven
cold. M-directory intentionally reuses a qualified setup population;
GET follows the PUT of its own data. Their numerical cache verdict is
INELIGIBLE, performance_claim=false. SQL setup/insert/read phases also receive
UNKNOWN/INELIGIBLE cache labels. Retained provider/schema reuse is reported.
Do not relabel raw observations as cold, qualified speed PASS or product proof.

SQLite profile: file-backed, journal_mode=MEMORY, synchronous=OFF,
cache_size=-512KiB, mmap_size=0, busy_timeout=0, temp_store=FILE, foreign_keys=ON.
Record effective readback. No WAL/fsync/durability extension or provider fallback.
Database files are fresh per case. This profile is runtime-only and differs from
durable global-catalog deployment decisions.

Preparation is acquired once. M-directory creates/qualifies its10000-key master
once; no other timed selection uses those keys. All other case keys are fresh.
No read pre-touch or cache manipulation. Provider/build reuse is sealed.

## Expected results, limits and receipts

Expected body SHA256 is computed from the frozen seeded fixture independently
of GET output. Exact byte lengths and all keys/counts are checked separately.
LIST oracle uses deterministic expected key names/counts. SQL counts, sorted
name/range transcripts, point values and final emptiness are checked against
Python reference generation independently of SQL output. No candidate-derived pin.

Record all command exits/walls, phase durations, operations, requests, logical
and physical bytes, MiB/s and ops/s. Record independent proof and cleanup status,
source commit/tree/tool/provider hashes, topology, cache contract and omissions.
No numerical speed target is frozen in this capability/observation experiment.

Current process RSS before/after is observational; it is not a phase peak or
physical memory bound. macOS file-cache, per-phase physical I/O and native heap
observation remain unavailable unless actually measured. Never substitute a
lifetime maximum. This experiment establishes no bounded-memory proof.

Use fresh append-only directories under benchmark-results/storage-probes/.
Credentials and startup logs remain private ignored files, never committed or
printed. Delete only exact case keys/owned temporary databases. Stop only the
owned MinIO process after collection; keep receipts and provider seals.

## Gate to the next stage

Deliver every selected observation and proof, including misses/unavailable
fields, and explain the chunk-request/grouping and transaction costs. Daemon,
mount, publication, capture/G1/G2, delta dependency and full architecture proofs
remain the next stage. Do not infer them from these primitives.
