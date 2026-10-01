# Standalone storage experiments v2: packs, publication, concurrency and scaling

Status: Research; informative and not a product contract.
Owner authorizes all four follow-up groups on 2026-10-01; experiment #291.
Parent source: 341e0622b09bb722cce4b8ff94614f2bb35235da.
No daemon, sandbox, FUSE, product migration or #288 campaign. V1 receipts remain
immutable. All new operations are explicitly external backend experiments.

## Frozen membership: 24 selections

| Group | IDs | Fixed workload / timed boundaries |
| --- | --- | --- |
| Real chunks | P-8-object-v2, P-8-pack-v2, P-16-object-v2, P-16-pack-v2, P-32-object-v2, P-32-pack-v2 | 8 MiB logical bytes, unique seeded incompressible chunks of 8/16/32 KiB; one object per encoded record versus native LayerFS packs <=256 KiB |
| Compressible chunks | P-16-text-object-v2, P-16-text-pack-v2 | Same 8 MiB / 16 KiB count, deterministic repetitive bodies with unique ordinal prefix; same pack encoder and FULL representation selection |
| Publication | D-128-runtime-v2, D-128-full-v2, D-1024-runtime-v2, D-1024-full-v2 | Eight successive publications, 128 or 1024 locators each; pack registration, Commit insertion and conditional Branch update in one SQL transaction |
| Upload concurrency | C-upload-1-v2, C-upload-2-v2, C-upload-4-v2 | Each client uploads eight unique 256 KiB synthetic objects, one connection and producer per client; coordinated incomplete first PUT proves request overlap |
| Queued publication | C-metadata-1-v2, C-metadata-2-v2, C-metadata-4-v2 | Eight publications per client, 128 locators each, independent Branches; application writer gate with native busy_timeout=0, actual queue wait and per-client completion |
| Shared Branch | C-branch-conflict-2-v2 | Two clients read generation0 then submit guarded publication; serialized writes must yield exactly one success and one explicit conflict, losing rows rolled back |
| Native writer contention | C-sqlite-busy-2-v2 | One connection holds BEGIN IMMEDIATE after an actual insert; second connection attempts BEGIN with busy_timeout=0; exact BUSY, no retry, then first owner commits |
| Flat metadata | S-flat-10000-v2, S-flat-100000-v2 | Reused closed metadata fixture, independent writable byte copy; 128 indexed points, one128-row keyset page, one rename and one delete; bounded operations, not full listing |
| Deep metadata | S-deep-10-v2, S-deep-270-v2 | Same100000-flat-entry fixture plus depth10/270 chain; eight full component-by-component resolutions, one leaf rename/delete; statements linear in components |

## Construction and reference method

A release example in the existing layerfs-storage crate calls public C1 chunk
encoding and C2 FULL encoding, native group assembly and pack assembly. It does
not change product source, formats, placement policy or dependencies. All native
groups remain <=64 KiB and packs <=256 KiB, with exact framing charged. Original
fixture bytes and SHA256 hashes are generated independently in Python before
encoding; no expected bytes or digest comes from a downloaded candidate object.
Physical directories are independently parsed in Python. FULL payload frames
are independently decoded using Python3.14 compression.zstd where applicable;
canonical payload equality and exact transcript provide proof. CAS IDs emitted
by C1 are diagnostic, not an independent identity oracle claim.

A single prepared fixture per shape is reused by object/pack arms. Encode/pack
construction has its own Rust monotonic duration, includes canonical identity,
codec, framing and buffered output, and excludes fixture generation. Upload is
measured separately; disk fixture reads/signing/transmission are included.
MinIO sequential read covers every record; scattered read selects128 unique
indices (37*k mod chunk_count). Object arms issue one request per record. Pack
arms record both whole-pack-per-selected-record and exact-record range reads;
whole-pack scattered reads deliberately have no shared cache and disclose their
read amplification. Sequential pack read fetches each pack once. No per-record
helper processes. Decode and verification are separate from HTTP transfer timers.

## Publication profiles and concurrency grammar

Runtime profile remains MEMORY journal/synchronous OFF; new full profile is
DELETE journal/synchronous FULL/fullfsync ON on macOS, cache_size=-512,
mmap_size=0, busy_timeout=0, temp_store=FILE, foreign_keys=ON. Read back settings.
This external experiment changes no LayerFS profile. Record insertion and SQL
COMMIT time separately plus complete acknowledgement. FULL synchronization cost
is measured; power-loss durability is not proven by timing or process reopen.
Publication references simulated already-uploaded pack keys, not an end-to-end
MinIO/SQLite atomic commit. No WAL, automatic busy handler, retries or fallback.

Each concurrency selection prospectively selects1/2/4 clients; this does not
raise V1 workers. Upload clients pause after transmitting the first64 KiB of
their first signed256 KiB PUT; all incomplete request lifetimes must overlap
before release, and all later provider acknowledgements must succeed. Queued
metadata clients rendezvous before requesting the writer gate; a coordinator
holds the gate until all initial contenders have attempted admission. Record
request/start/commit/end intervals and wait; this proves application queue
contention, not simultaneous SQLite writers. Native BUSY uses held actual SQL
transaction/events, not sleeps. Shared-Branch competitors select the same actual
head before release; guarded update/rollback supplies the independent outcome.
One producer per client, no additional producer inside an operation.

## Scaling fixture and query laws

Prepare each flat population and each deep shape once, close and qualify it,
then copy bytes to a fresh writable database. Clone is a byte copy, not APFS
reflink or cold claim. Queries use parent/name and inode primary/index keys;
record EXPLAIN QUERY PLAN and statement counts. Expected results are known
fixture rows and exact final rename/delete state. Depth270 is a standalone SQL
schema experiment and does not enable a larger product path profile or fix#289.
Use no OFFSET, full-frontier collection or recursively concatenated path keys.

## Timing, evidence and interpretation

One attempt per case/arm; fresh append-only output. Freeze this specification
before implementation and pin coherent tool/example/build/provider identities
before collection. Reuse the sealed native provider binary; create a fresh owned
server root with private credentials and loopback ports. No foreign target.
Release build uses locked Cargo and repository-root ARMv8 flags, outside all
measurement windows. Record binary/source/config hashes and build command/wall.

Performance child <=15s, separate exact proof/cleanup <=10s. Setup acquired once
outside timers with separate receipts. Retain every failure, timeout or omitted
cell. No speed threshold frozen. Native BUSY and shared-Branch conflict are
expected correctness outcomes, not error-driven throughput reruns. No unchanged
V1 reruns. A demonstrated repair gets a new identity and covering case only.

All raw numerical rows retain INELIGIBLE/performance_claim=false because OS,
provider and native cache residency are unknown and GET follows PUT. Keep read
bytes/request counts, operation counts, transactions, native waits, statement
counts, per-client intervals and external wall. RSS snapshots are not a memory
bound; native peak/cache/physical I/O remains unavailable. No cold, power-loss,
production Commit/Exec, distributed or release speed admission.

Proof: exact fixture chunks/lengths after FULL decode; independent physical pack
framing; exact locator/Commit/Branch rows and rollback; held-lock BUSY custody;
all expected uploads; query counts/plans and final metadata. Cleanup exact owned
keys, databases/rows and owned server; keep receipts and failure custody.
Commit sanitized receipts, command logs and hashes; exclude credentials/private
startup logs. Append report and #291 checkpoint; retain historical V1 verdicts.

## Pre-collection implementation clarifications

Construction is one shared preparation diagnostic per shape: the release example
writes both standalone records and pack forms in the same invocation. Its duration
is not credited as either arm's construction cost. HTTP sequential pack timing
includes physical directory validation/extraction; payload Zstandard decode and
reference comparison remain separate proof. Upload includes SHA256 required for
SigV4 plus file reads. Range reads require HTTP206 and exact decoded fixture bytes.
Scattered whole-pack reads have no retained cache and report their actual body
bytes. Fixture reuse is sealed by builder binary hash and artifact hashes.

Queued metadata admission must first observe a failed nonblocking acquire while
the coordinator holds the writer gate, then await release. Native contention
connections both finish profile setup before the held-write proof begins.
Scale rename has one separate untimed visibility SELECT before deletion;
mutation statement count is3 (UPDATE, visibility SELECT, DELETE), with only
UPDATE/DELETE inside their phase timers. Publication packs are simulated keys
with nominal bytes, not physical LayerFS packs or canonical metadata construction.
Python bindings/SQL fixtures are an explicitly experimental metadata algorithm.

The runner is tools/storage_probes/run_v2.py; groups pack/publication/concurrency/
scaling select8/4/8/4 cases. V1 tooling/receipts are not re-collected. New external
code and all prospective clarifications are sealed before collection.
