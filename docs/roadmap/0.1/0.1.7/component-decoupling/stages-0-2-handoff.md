# Handoff: Stages 0–2 — complete-file construction, real CAS and independent timers

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Implementation parent: [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). Assigned implementation issues:
[#166](https://github.com/Ephemeral-AI-Lab/layerfs/issues/166) (Stages 0–1) and
[#167](https://github.com/Ephemeral-AI-Lab/layerfs/issues/167) (Stage 2).
The seven-child mapping combines Stage 0 with Stage 1; Stages 2–7 each have their
own child. Do not confuse these execution issues with the seven component boundaries.

## Copy/paste assignment

You are the implementation agent for Stages 0–2 in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Complete the real replacement path:

```text
stable complete-file input
          |
C1: canonical objects + empty/small/CDC construction
          |
bounded finalized-object handoff
          |
C2: exact CAS reuse / supported FULL encoding / packing / real SQLite
          |
acknowledged storage -> close/reopen -> authenticated readback
```

Deliver independently runnable C1-only construction, C2-only real save/read, and
integrated timing using the existing layerfs-telemetry. This is the primary proof
of separation. No Workspace, FUSE, daemon, history entity or checkpoint is needed.
A root is an ObjectId; a file result returns root ID and logical byte length.
Internal tree counts/levels stay internal unless a real caller needs them.

This assignment is implementation, external tests, runnable examples and honest
handoff evidence. Do not stop at another proposal, a stub, a synthetic benchmark
or an end-to-end-only timer. Finish both assigned issues' scope; keep later issues
open. Do not implement delta chains, arbitrary edits, filesystem trees, runtime
adapters or cloud deployment merely to make this slice look feature-complete.

### Start from the actual reviewed workspace

Read `AGENTS.md`, `core/AGENTS.md`, then the design files listed below. Inspect Git
status before editing. This task's reviewed design/policy files may still be
uncommitted: a clean checkout of main is not automatically the reviewed specification.
Preserve existing work; do not reset, delete or indiscriminately stage unrelated files.
If using an isolated checkout, explicitly carry the reviewed design/rules into it
before implementation. Record its source identity. No new thread is required.

Root `crates/` remains reference only. Read/reuse first-party algorithms by porting
them into the replacement; no path dependency, source include, linked legacy binary
or runtime fallback into the reference. Keep reference and candidate build identities
separate. The audited product baseline is v0.1.6 source
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; the design review HEAD was
`a8a1ba848429d5f2fbba83c2de22dada8c29def9`. Recheck later changes explicitly.

### Required design inputs

All under `docs/roadmap/0.1/0.1.7/component-decoupling/`:

1. `implementation-plan.md` — architecture, scope and sequence.
2. `canonical-objects.md`, `file-content.md` — canonical and complete-file behavior.
3. `finalized-object-handoff.md`, `content-io.md` — ownership, backpressure and
   mandatory independent measurement modes.
4. `physical-encoding-and-packing.md`, `admission-and-persistence.md` — real C2
   save/read, locators, constraints, transactions, visibility and failure.
5. `content-storage-policy-and-tables.md`, `content-io-memory-audit.md` — capacities,
   minimal schema, algorithm/resource obligations and proposed reductions.
6. `telemetry.md`, `repository-layout.md` and the implemented telemetry README/USAGE.

Before benchmark work also read `docs/general/benchmark_rules.md`,
`benchmark/AGENTS.md`, `benchmark/fs-bench-pro/QUICKSTART.md`, release policy and
documentation policy. These issues do not themselves register a performance campaign.

## 1. Hard rules

- No retry, fallback, fsync, fdatasync, File::sync_all or File::sync_data. No WAL,
  added crash-durability/recovery system or third-party patch/fork/vendor edits.
- Embedded SQLite: MEMORY journal, synchronous OFF, zero busy timeout; preserve
  transaction atomicity and definite-failure abort. COMMIT remains required.
- Failures propagate once. Unknown write acknowledgement is failure with unknown
  persistence outcome; no resend, automatic polling or destructive cleanup on a guess.
- One save mutation owner; bounded batches/transactions across files. Preserve
  useful producer overlap and the later namespace-init concurrency exception.
- Every product implementation file <=999 physical lines; lib.rs/mod.rs <=200
  physical lines and declarations/reexports/direct delegation only. No minification,
  includes, renamed god modules or arbitrary numbered splits to evade these limits.
- Product src/ contains no tests, mocks, fixtures, benchmark drivers, test-only cfg,
  fault injection or test-only public methods. External tests use actual public APIs.
- No new generic plugin/transport/validation/metrics framework. Use concrete functions
  internally and narrow real read/output interfaces; no trait per algorithm.
- Reuse existing dependency pins: BLAKE3 =1.8.5, rusqlite =0.40.2, zstd-sys =2.0.16
  with required existing feature choices after checking the reference manifests.
  Do not enable a bundled/forked SQLite or change codec parameters to ease the port.
  Update the candidate lockfile deliberately, then use --locked.
- Keep the 128-KiB construction cutoff and 8/4 depth defaults visible in typed policy.
  This slice implements exact reuse and FULL operations; delta policy is later scope,
  not a silently claimed feature. Accept only explicitly implemented profiles/ranges.
- No generic payload spool/scratch. Supported large objects must have a proven
  bounded in-memory path; never hide buffering in the caller or shrink advertised
  capacity to make a test pass. Unfinished larger-cutoff coverage is reported explicitly.

## 2. Exact package homes and dependency direction

Use these package names for this handoff:

```text
core/crates/layerfs-content      C1
core/crates/layerfs-storage      C2
core/crates/layerfs-telemetry    existing; reuse unchanged unless a real defect requires work

layerfs-content -> layerfs-telemetry + existing hashing dependency
layerfs-storage -> layerfs-content + layerfs-telemetry + required existing SQL/codec dependencies
C1 never imports C2. Neither imports Workspace/history/FUSE/runtime/Monitor.
```

The existing root layerfs-content package can retain its name because core/ is an
independent Cargo workspace. Keep explicit core members and separate lockfile/target
selection. Inherit the existing core workspace version/MSRV; this is not a release
version-bump task.

Create files only when implementing their real responsibility. The exact baseline
file map and estimates below are the starting plan. A justified merge/split/rename
is allowed when it improves cohesion; report every difference and its actual size.
Do not create empty placeholder files just to match the map.

## 3. Production file map and recommended LOC

These are **estimated production source LOC**, not raw file lengths: nonblank,
non-comment first-party implementation, including imports/declarations and shipped
SQL. Tests/docs/manifests/examples/tooling are excluded. Physical file ceilings still
apply independently. Ranges are recommendations, not correctness gates or promised
savings. Do not weaken checks or compress formatting to fit them.

### `core/crates/layerfs-content/`

| File | Recommended production LOC | Responsibility |
| --- | ---: | --- |
| `src/lib.rs` | 12–30 | Public declarations and reexports |
| `src/error.rs` | 35–65 | Typed content errors |
| `src/policy.rs` | 70–130 | Checked construction/profile capacities |
| `src/object/mod.rs` | 6–14 | Module declarations/reexports |
| `src/object/id.rs` | 45–80 | ObjectId and frozen hash domain |
| `src/object/codec.rs` | 110–200 | Canonical framing/authentication |
| `src/object/access.rs` | 60–110 | Bounded authenticated read contract |
| `src/object/output.rs` | 60–110 | Owned finalized output and bounded consumer |
| `src/file/mod.rs` | 8–16 | File API reexports |
| `src/file/content.rs` | 90–150 | Empty/small/chunked dispatch and final result |
| `src/file/read.rs` | 110–200 | Logical file and range reads |
| `src/file/cdc/mod.rs` | 4–8 | CDC reexports |
| `src/file/cdc/gear.rs` | 120–210 | Frozen streaming CDC |
| `src/file/mapping/mod.rs` | 8–16 | Mapping reexports |
| `src/file/mapping/types.rs` | 90–160 | Typed file/extent fields and bounds |
| `src/file/mapping/codec.rs` | 180–300 | Checked canonical mapping encode/decode |
| `src/file/mapping/build.rs` | 220–380 | Streaming finalized mapping construction |
| `src/file/mapping/read.rs` | 180–300 | Bounded extent traversal and ordered demand |

Package estimate: **1,408–2,479 production LOC**.

### `core/crates/layerfs-storage/`

| File | Recommended production LOC | Responsibility |
| --- | ---: | --- |
| `src/lib.rs` | 15–30 | Public declarations/reexports |
| `src/error.rs` | 45–90 | Storage/unknown-outcome errors |
| `src/policy.rs` | 75–140 | Persisted policy and supported storage capacities |
| `src/cas/mod.rs` | 8–18 | CAS reexports |
| `src/cas/store.rs` | 70–120 | Public Store handle and entry-point delegation |
| `src/cas/owner.rs` | 90–160 | One save owner, cursors and terminal state |
| `src/cas/batch.rs` | 110–190 | Bounded accepted/pending ownership |
| `src/cas/save.rs` | 180–320 | Real batched save coordination |
| `src/cas/membership.rs` | 120–210 | Exact reuse/collision decisions |
| `src/cas/dependencies.rs` | 100–180 | Incremental dependency availability |
| `src/cas/read.rs` | 140–240 | Batched object reads and visibility |
| `src/cas/finish.rs` | 100–180 | Acknowledgement and one failure boundary |
| `src/encoding/mod.rs` | 6–12 | Encoding reexports |
| `src/encoding/codec.rs` | 120–220 | Pinned bounded compression workspace |
| `src/encoding/full.rs` | 140–250 | Supported FULL representation creation |
| `src/encoding/decode.rs` | 160–280 | FULL reconstruction and checked framing |
| `src/pack/mod.rs` | 6–14 | Pack reexports |
| `src/pack/layout.rs` | 170–300 | Format directories, locators and checked sizes |
| `src/pack/placement.rs` | 110–190 | Exact append/new placement |
| `src/pack/assemble.rs` | 140–240 | One selected bounded assembly |
| `src/pack/read.rs` | 170–300 | Grouped/range record extraction |
| `src/sqlite/mod.rs` | 8–16 | Persistence reexports |
| `src/sqlite/connection.rs` | 90–150 | Open/configure owner, no WAL/sync/busy retry |
| `src/sqlite/schema.rs` | 130–220 | Exact schema/profile validation and policy load |
| `src/sqlite/lookup.rs` | 120–210 | Bounded membership/location/presence SQL |
| `src/sqlite/write.rs` | 180–320 | Batch bindings and bounded transactions |
| `src/sqlite/cleanup.rs` | 150–260 | Abort and indexed owned cleanup mechanics |
| `sql/schema.sql` | 80–140 | Four tables, constraints and required indexes |

Package estimate: **2,833–5,000 production LOC**.

### Directory totals

Each row is a **recursive directory total**: a parent already includes its child
folders. Do not add parent and child rows together. Package totals below are disjoint.

| Directory | Recommended production LOC |
| --- | ---: |
| `core/crates/layerfs-content/src/` | 1,408–2,479 |
| `core/crates/layerfs-content/src/file/` | 1,010–1,740 |
| `core/crates/layerfs-content/src/file/cdc/` | 124–218 |
| `core/crates/layerfs-content/src/file/mapping/` | 678–1,156 |
| `core/crates/layerfs-content/src/object/` | 281–514 |
| `core/crates/layerfs-storage/sql/` | 80–140 |
| `core/crates/layerfs-storage/src/` | 2,753–4,860 |
| `core/crates/layerfs-storage/src/cas/` | 918–1,618 |
| `core/crates/layerfs-storage/src/encoding/` | 426–762 |
| `core/crates/layerfs-storage/src/pack/` | 596–1,044 |
| `core/crates/layerfs-storage/src/sqlite/` | 678–1,176 |

Combined new C1+C2 estimate: **4,241–7,479 production LOC across 46 planned source files**.
Existing telemetry and reference source are excluded from this estimate, but included
in their correct subtotals for actual per-commit production counting.

## 4. Supporting files and external tests

Create actual manifests and external tests; these have **no production LOC budget**:

```text
core/crates/layerfs-content/
  Cargo.toml
  README.md
  tests/
    object_identity.rs
    file_complete.rs
    file_read.rs
    streaming.rs
    timing.rs
    support/mod.rs
    fixtures/                        only small actual frozen test data, as needed

core/crates/layerfs-storage/
  Cargo.toml
  README.md
  tests/
    cas_roundtrip.rs
    cas_reuse.rs
    pack_locator.rs
    persistence_failure.rs
    memory_bounds.rs
    timing.rs
    core_pipeline.rs
    support/mod.rs
  examples/
    measure_components.rs            actual C1/C2 APIs, not mocked storage
```

Suggested external-test sizes: 100–300 nonblank code lines per named test target;
40–120 for each support module; 100–220 for the runnable example. These are separate
planning estimates, never added to production LOC. Add/split a target for a real
coverage need, not one test file per implementation function. Register additional
test helpers under actual package tests/; do not recompile private src/ into tests.

Modify existing files only as needed:

- `core/Cargo.toml`, `core/Cargo.lock`: two real members and existing dependency pins.
- `core/README.md`: actual package/API/command inventory.
- `tools/preflight.sh`: register the new real example/checks without dropping current checks.
- `tools/production_loc.py`: include candidate runtime SQL and per-file reporting as
  needed. Its current scope only explicitly includes reference SQL; fix classification
  before committing new candidate SQL. Audit comment/test handling rather than
  assuming the current counter proves every rule.
- Add focused counter tests in `tools/test_production_loc.py` and wire the check into
  preflight. Recommended tooling-only change: roughly 40–120 implementation lines
  plus 60–160 test lines; excluded from production totals.
- `core/tools/check_product_boundary.py` / its tests: extend only for real new source
  paths/formats or a discovered guard gap; the 999/200 Rust/SQL guard already exists.
- Roadmap index/implementation status and a concrete `stages-0-2-report.md` under the
  component-decoupling directory. Preserve unrelated working-tree changes.

Do not create filesystem/, delta/, metadata-pooling, Workspace, FUSE, cloud or
application-adapter stubs in these packages. Their issues remain separate.
The agreed metadata_value_groups SQL table may be empty in this FULL/file slice;
that does not justify a fake metadata-pooling implementation.

## 5. Implement in this order

### Stage 0 — freeze the initial executable contract

1. Inspect all callers and reference algorithms needed by this slice; keep a short
   source-to-target map. Read required formats rather than copying whole monoliths.
2. Record exact accepted canonical/physical profile, schema identifier/role codes,
   supported sizes, initial cutoff range and Store create/open behavior before
   persisting data. Do not silently migrate existing Stores or create a schema
   that looks like schema 10 while violating its contract.
3. Define small production entry points for complete construction, logical reads,
   bounded authenticated object reads, finalized output and standalone CAS save/read.
   Keep input acquisition, output ownership and final acknowledgement explicit.
4. Make timing injection and independent execution part of those signatures now.
   No single API that can only time the complete Workspace pipeline.
5. Fix LOC classification for the new runtime SQL, and retain all structural guards.

### Stage 1 — real complete-file C1

Port/reuse canonical identity/framing and complete-file algorithms. Preserve empty
file form, WHOLE_FILE below T, chunked form at/above T, frozen CDC, extent partition
and root identity under the same profile. No logical checkpoint or history record.

Construct with stable sequential input, explicit policy and a bounded output consumer.
Return root ID and logical byte length. A supplied consuming/discarding harness
must allow C1 to complete with no SQLite dependency, save call or input-size object
collector. The same constructor must feed the integrated C2 path.

Implement complete and logical-range reads using the narrow authenticated object
provider; C2 supplies that provider later. Do not introduce point-loop batch defaults
or clone a payload per duplicate demand. Unknown-length input can use the existing
bounded threshold probe; known length need not rediscover it.

### Stage 2 — real CAS/FULL/SQLite path

Implement the four-table/19-column design and required location/base indexes.
Use exact membership/collision semantics, direct-reference availability, owned
batches and one mutation owner. Start supported SQL transactions lazily, share them
across batches/files, and finish all output before success. Skip an empty final
write transaction only when nothing remains to commit or compose.

Use the implemented FULL physical formats/compression for the explicitly accepted
profile. Decide append/new placement before assembling and assemble only the
selected write once. Preserve stable (pack,group,record) locators and exact SQL
cardinality checks. No per-file transaction or per-object RPC.

FULL may be compressed. Stage 2 must not claim DELTA, pooled metadata or arbitrary
old-Store compatibility. Unsupported requested representations fail explicitly;
no failed encoding/decoder/backend operation falls back to another representation.
Preserve default representation behavior for the covered paths and record any
explicitly scoped format limitations before implementation.

Save/read must work on supplied canonical objects with no C1 file/tree construction,
Workspace, Branch, Commit, mount or daemon. C2 can reuse C1 object primitives for
identity/validation; that is not invoking file construction.

Implement complete failure semantics immediately: one terminal cleanup attempt,
definite abort before owned early-write cleanup, preserved preexisting data and
unknown-outcome quarantine. Early committed private output is owner-bound. An
ordinary read captures a retained-pack ceiling and applies it through dependencies
and caches; do not expose cleanup-owned objects to unrelated readers.

Prove a real supported large canonical singleton directly at C2. A large input
split into small CDC chunks does not exercise this path. No temporary singleton
pack file, hidden spill, larger unreported memory allowance or removed capacity.
If the allocation proof cannot be met, report the specific blocker and keep the
relevant issue open; do not claim all of Stage 2 complete.

## 6. Required time measurement deliverable

Use existing `Timing::record`, `Timing::disabled`, child scopes and completed-report
attachment. Do not implement a replacement timer or add telemetry tables, collectors,
per-object trace retention, fake clocks or live cross-thread scope sharing.

| Mode | Required real operation |
| --- | --- |
| C1 only | Stable input -> real complete-file constructor -> bounded non-persisting consumer -> root/length |
| C2 only | Supplied bounded canonical objects -> real CAS save -> transaction acknowledgement; independent authenticated read |
| Integrated | Real C1 -> bounded C2 handoff -> final storage completion -> authenticated logical readback |

C1 source reads and declared consumer work remain timed. C2 fixtures are explicit
inputs; any preparation excluded from its component scope is stated honestly and
is included in a larger operation that requires it. Keep required save-time base,
index/synchronization and SQL work inside that scope. No warmed setup credit.

Bulk default reports retain complete coarse component/operation durations. Detailed
bounded calls may expose membership, encoding, pack and SQL work. Respect the
recorder's 1024-node/32-level limits; clipped detail is incomplete, not zero or a
complete breakdown. Preserve producer/consumer overlap; durations can overlap and
must not be subtracted/summed into fictional CPU or pure-construction time.

The caller renders text and optionally saves JSON using ordinary writes, never
fsync. Error reports retain the original failure and completed timing; no retries.
Recording disabled must execute identical product work and preserve results/errors.

`examples/measure_components.rs` must support `--mode c1|c2|pipeline`, `--input PATH`,
`--timings FRESH_PATH`, and `--store FRESH_DB` only when storage is used. It must print
actual root/length/result plus timing scope/exclusions, and verify readback in modes
that store. C2-only can use one bounded canonical fixture prepared from the small
input before its storage scope, with that preparation explicitly reported; reject
larger unsupported demo fixtures instead of collecting an unbounded candidate.
All three modes call real production functions, with real SQLite for C2/pipeline.
These smoke examples demonstrate wiring, not release-admissible benchmarks.

## 7. External acceptance tests

| Test target | Must establish |
| --- | --- |
| C1 object_identity | Canonical bytes/IDs match frozen profile fixtures; malformed/trailing/overflow input rejected; no replacement raw digest |
| C1 file_complete | Empty, 1 byte, T-1/T/T+1, exact/unknown lengths, compressible/incompressible input, deterministic CDC partitions/root |
| C1 file_read | Full and cross-extent ranges, EOF/bounds, ordered repeated demands, exact bytes |
| C1 streaming | Bounded source requests/final output, child-before-parent dependencies, many files without whole-workload retention, late EOF/read failure |
| C1 timing | Database-free real construction, on/off equivalence, stable real scope names, error/incomplete behavior |
| C2 cas_roundtrip | Fresh exact schema, supported FULL objects of all initial roles, close/reopen/authenticated readback and malformed Store rejection |
| C2 cas_reuse | Exact reuse, repeated IDs within/across batches, existing corrupted record rejection without test-only hash hooks |
| C2 pack_locator | Append/new boundaries, stable existing locators, same-save visibility, corrupt locator/framing rejection |
| C2 persistence_failure | Unavailable owner, actual SQL/constraint failures where externally inducible, late input failure after early writes, cleanup once, preserved retained objects, terminal unknown/cleanup failure where reachable |
| C2 memory_bounds | Declared size/count/capacity rejection, bounded pending/queue ownership, supported incompressible singleton, no payload temporary file; disclose limits of heap/RSS evidence |
| C2 timing | Supplied-object real save/read independently; original errors and on/off results; valid bounded report structure |
| C2 core_pipeline | Real C1+C2 many-file streaming, backpressure, root/length and byte readback, independent/integrated equivalence |

Exercise failures through real inputs, filesystem/database behavior and existing
production capabilities. Do not add fault-injection switches to src/. Use deterministic
barriers for backpressure tests rather than sleeps/tight clock thresholds. If a
failure condition cannot be induced without a prohibited hook, report that exact
coverage gap and its source/contract proof; do not invent a passing test result.

Use frozen reference output fixtures or a separately declared/sealed reference
execution for equivalence. No candidate dependency on old crates, no copied fake
algorithm as the oracle, and no comparison against a different CDC/profile.
Unsupported old-format paths are explicit scope limits, not tests silently dropped.

## 8. Commands to run

From `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, after adding the real packages:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_nested
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_composition
```

Run package-focused tests while implementing. The workspace test is the final
combined check; do not rerun already-passing resource-sensitive selections without
new changes, failures or a stated need. Do not collect performance during builds or
another owner's measurement.

Real-mode smoke commands (fresh outputs, small explicit fixture):

```sh
stage02_run="$(mktemp -d /tmp/layerfs-stage02.XXXXXX)"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_bytes(bytes(range(256)) * 64)' "$stage02_run/input.bin"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c1 --input "$stage02_run/input.bin" --timings "$stage02_run/c1.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c2 --input "$stage02_run/input.bin" --store "$stage02_run/c2.sqlite" --timings "$stage02_run/c2.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode pipeline --input "$stage02_run/input.bin" --store "$stage02_run/pipeline.sqlite" --timings "$stage02_run/pipeline.json"
git diff --check
```

Before an implementation push, run `tools/preflight.sh` with its new example/counter
checks wired in and record the result. No CI exists; do not claim CI green. A failing
or unrun required check remains visible. Never expand a timeout, warm input or remove
a test merely to get a green result.

For performance evidence, first create the required committed case specification
and source/harness/cache identities under the benchmark policy, linked to these
issues. Record exploratory timing as exploratory. Full C1/C2 release qualification
belongs to the later qualification issue; do not call these smoke values a speedup.

## 9. LOC reporting and completion

Before each commit, materialize the exact first-parent and final staged tree and
run the same final counter/version against both snapshots. Do not compare current
working-tree counts with another counter embedded in the parent. Include candidate
runtime SQL, exclude inline legacy test code and report reference/core/combined
subtotals. Recompute after staged changes, amendment or rebase and confirm the
resulting commit. Every commit/handoff includes:

`Production LOC: <before> -> <after> (delta <signed difference>)`

Report recommended versus actual per file and directory in `stages-0-2-report.md`:

```text
path | recommended production LOC range | actual production LOC | physical lines
     | within/below/above range | explanation / justified split or merge
```

Use the same production counter classification for per-file and total reporting.
Do not report raw file lines as production LOC. The recommendation is not a cap:
above-range correct code is allowed with an honest reason. Below-range code is not
automatically better. All physical hard limits still apply. Every planned omission,
new file, renamed path and unrun test receives an explicit disposition.

Completion report must include:

- Implemented scope and exact supported profile/capacities/format limits.
- Actual folder tree and per-file/directory estimates versus actual counts.
- C1-only, C2-only and integrated run commands/results/real timing artifacts.
- Correctness/readback, bounded ownership and no retry/fallback/fsync evidence.
- Source/build identity, commands and every FAIL/NOT_RUN/INCOMPLETE item.
- Remaining Stage 3+ features and any format, memory, crash or visibility limits.

Close #166 and #167 only when their actual acceptance
criteria are met and evidence is linked. Keep the parent and Stages 3–7 open.
No automatic release/tag/deploy, no root reference retirement and no assertion that
all of v0.1.7 is complete. If blocked, report the precise unmet gate and completed
work instead of substituting a fallback or relaxing the requirement.
