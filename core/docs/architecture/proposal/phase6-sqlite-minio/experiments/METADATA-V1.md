# Daemon metadata engine experiment v1

> **Status: Research; informative and not a product contract.**
> Prospective specification, 2026-10-01; owner: [#294](https://github.com/Ephemeral-AI-Lab/layerfs/issues/294), native sub-issue of [#293](https://github.com/Ephemeral-AI-Lab/layerfs/issues/293).
> Product baseline `7edddbdb8e8512627aed0ed42533ef099d802384`; design parent `9dd2fc8f1ad717b94dee5cd8788d8fa8539db2e6`.

## Question and permitted claim

Evaluate a straightforward indexed, generation-versioned SQLite representation
before choosing the daemon engine. This is a standalone prototype/component
diagnostic, not LayerFS Commit/Exec/FUSE, canonical-root, payload-throughput,
cloud durability or release qualification. No #288 campaign is executed.

Prototype code lives in `core/benchmark/phase6-metadata/`, an independent Cargo
workspace using the existing pinned rusqlite stack. It does not change product
source. Actual prototype source, lock/binary/fixture/oracle hashes, host and
build flags are recorded in each collection. No new product dependency or format
is assigned. SQLite is embedded in-process, not a subprocess.

## Representation and fixed profile

- One real file-backed database per selection; Workspace IDs partition records.
- Tables: Workspaces (active generation, pending capture, lifecycle counter),
  versioned inode attributes and parent/component bindings, versioned file
  extents and capture ownership. Composite primary keys and live partial indexes;
  names are BLOB components, never complete descendant paths.
- A row is visible when `born <= selected_generation < dead`; `dead` has a fixed
  sentinel. Within one writable generation rows are updated/replaced, rather
  than retaining a version per syscall. Captures advance the active generation.
- Extent overwrite selects the predecessor and affected start-key range in
  batches, retires/replaces intersecting rows, retains exact source offsets and
  inserts the final replacement interval. This v1 deliberately tests a single
  atomic affected-range transaction, including its scaling risk. Shrink removes
  affected tails; extension adds explicit zero ranges. No historical WRITE log.
- Retirement removes eligible obsolete versions in batches after capture release.
- Engine: current locked rusqlite `0.40.2`, actual SQLite version read at runtime;
  MEMORY journal, synchronous OFF, temp_store MEMORY, foreign_keys ON, busy
  timeout zero. Prototype cache `-512` KiB, mmap zero, statement cache 64;
  these are experiment allowances, not new product profiles. No WAL/fsync.
- Cursor windows: extents/retirement 128 rows, each at most 7 signed 64-bit
  fields (7,168 field bytes); namespace 256 rows, names exactly 8 bytes plus
  inode (4,096 field bytes), admitted field-byte ceiling 16,384. Rust object
  overhead and engine allocation are separate. No whole-population query vector.

## Complete selection (one sample per case/arm)

Every row below is a distinct case, in this order. All use one construction
producer. Timing includes first-use statement preparation and metadata work.
No universal latency threshold is inferred from these diagnostic observations.

| ID | Pristine fixture and timed work | Independent expected result |
| --- | --- | --- |
| namespace-128 | 128 children; 256 indexed lookups `(i*997)%N`; full keyset list; 64 create/link/rename/unlink transactions | Exact ordered bindings, shared identities, inode facts |
| namespace-10000 | Same operations, 10,000 children | Same semantic oracle at this cardinality |
| namespace-100000 | Same operations, 100,000 children | Same semantic oracle at this cardinality |
| deep-270 | 270 parent/component edges, 100 complete traversals, one subtree binding rename | Exact edges/terminal inode; no descendant rewrite |
| lifecycle-reopen-128 | 128 separate counter-update transactions; reopen/configure/check schema each time | Counter 128; opens 128 |
| lifecycle-persistent-128 | Identical 128 updates; one open/configuration/schema check | Counter 128; opens 1 |
| extent-repeated-4097 | 16,384 inherited bytes; 4,097 overwrites of `[4096,4128)` | Exact bytes; final live map three spans |
| extent-append-512 | 16,384 inherited bytes; 512 appends of 32 bytes | Exact original prefix and appended bytes |
| extent-dispersed-512 | 32,768 inherited bytes; 512 writes of 32 bytes at `((i*61)%1024)*32` | Exact independently replayed bytes |
| extent-fragmented-8192 | 65,536 bytes as 8,192 alternating 8-byte source extents; replace whole range | Exact bytes; one live final span; affected-row work reported |
| extent-truncate-regrow | Same fragmented fixture; shrink to 4,096, extend to 65,536 | Preserved prefix then zeros; no resurrection |
| generation-10000 | 10,000 inode facts; capture G1; 128 successor updates; paged G1 traversal; repeat capture/update/release 8 rounds; bounded retirement | Exact selected and live rows, no pending captures/obsolete rows at completion |
| overlap-1 | 10,000 inode facts per Workspace; capture; reader fetches first page and signals; one mutator performs 128 updates before reader resumes; paged reads; release | G1 unchanged, G2 updated; recorded update completion before resumed traversal |
| overlap-2 | Same coordinated schedule for 2 Workspaces; one shared engine, fixed mutator per Workspace | Exact cross-Workspace isolation and progress |
| overlap-3 | Same for 3 Workspaces; explicit duplicate-capture refusal and cancel/release | Same, zero owned capture slots after completion |

Selected generation rows stream to a bounded result consumer (`selected.tsv`);
its file writes are included in the operation timer, with no oracle comparison
or digest inside that timer. Final verification checks that complete stream.

Source bytes for extent oracles are synthetic: byte at source-relative offset
`o` in source `s` is `(s*17 + o)%251`. Each write uses source `i+1`, offset zero;
fragmented fixtures use alternating sources 1/2, offset zero per fragment.
A Python bytearray oracle replays ordinary slice overwrite/truncate/zero extension
independently of candidate SQL splits. Namespace and generation oracles build
expected rows directly from fixed fixture/workload rules. Full final state is
verified, not a selected checksum supplied by the candidate.

## Timing, setup, custody and verification

- Prepare pristine masters once outside sample timing using the prototype's real
  engine. Setup wall and hashes are retained. Clone by independent byte copy,
  not APFS clone/hard link; never reuse a mutated sample.
- Cache profile: uncontrolled host OS cache, no cold claim. Numerical rows are
  `INELIGIBLE`, `admission_eligible=false`, `performance_claim=false`, including
  lifecycle comparisons. Do not pool with cold or LayerFS product rows.
- For normal cases open/configuration is reported as setup; timed work starts
  before the first workload SQL and ends after its final acknowledgement. The
  lifecycle pair instead includes every required open/configuration/close inside
  its operation timer. All first-use statement preparation remains inside work.
- Each complete performance command (launch/work/engine close/output) has a 15s
  watchdog. A timeout remains FAIL; no shrinking/retry. Verifier is separate,
  <10s, reads the retained database readonly, checks complete expected state and
  integrity, and has its own evidence. Failed attempts remain append-only.
- One sample per registered case/arm, no stability reruns. Exact source changes
  require a new diagnostic/version, not replacement of inconvenient evidence.
- Record raw operation/open/close/SQL ns, external command ns, statement/transaction
  counts, SQLite VM/fullscan/sort counters, affected rows, cursor maximum, engine
  current/high-water memory, DB/sidecar sizes and exact profile/version readback.
  Engine high-water resets before the measured operation. It is not total daemon
  memory. OS/cgroup/file-cache containment is UNAVAILABLE in this host prototype;
  process lifetime RSS, if recorded, is labeled lifetime and cannot fill the gap.
- Persist receipts and failures under a fresh ignored `benchmark-results/` path;
  commit a report and small exact receipts/manifests under this document's evidence
  folder. No payload or large database fixture enters Git.
- Worktree-local nonblocking measurement lock; no local build overlaps collection.
  Release `--locked` build from repository root, carrying `.cargo/config.toml`
  ARMv8 flags. Reuse the sealed executable. Third-party files remain untouched.

## Predeclared correctness and representation checks

Correctness requires exact final/selected metadata and bytes, valid extent
partition/source offsets, explicit zero extension, no stale generation, duplicate
submission refusal without state change, coordinated progress and released
captures. Fullscan/sort counts diagnose work; they are not self-declared speed PASS.

Candidate v1 bounded-transaction target: at most 512 changed metadata rows per
mutation transaction and at most the fixed cursor field-byte window. Exceeding
512 does not trigger fallback/refusal in this diagnostic: retain the result as
representation FAIL, even if bytes pass. This deliberately exposes whether bulk
range overwrite/truncate needs a staged representation before product adoption.
Capture must touch only Workspace/capture records, independent of inode population.
Repeated overwrites must not retain historical versions within the same generation.
Retirement must use bounded indexed batches; any population scan is reported.

Healthy progress in the overlap schedule is a narrow shared-engine proof, not
FUSE/Exec liveness or physical containment. MEMORY journal rollback allocations
may grow with affected rows; engine data must be reported rather than excused.
Global namespace certification, CAS/delta packs, inode-handle semantics, restart/
Unknown publication, cloud persistence and integrated speed remain NOT_RUN.
