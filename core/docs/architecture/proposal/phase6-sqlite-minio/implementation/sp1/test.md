# SP1 minimal tests and retained-history qualification

Status: Dated planning checkpoint; not release evidence or a product contract.
Revision written against published source `8c926b9392f3636ae156236dc26d0510ee069d8d`,
2026-10-02; branch `codex/phase6-metadata-experiments`. At that recorded revision,
the owner strict-split additions were unimplemented/unverified and `NOT_RUN`.
It supersedes the unreleased all-object-MinIO proposal; its prior evidence retains
its recorded source/scope/status.
Owning issue: [#295](https://github.com/Ephemeral-AI-Lab/layerfs/issues/295).
Read [implementation](IMPLEMENTATION.md), [SP1 scope](../STORAGE-PARITY-SPEC.md),
[historical baseline](../HISTORY-STORAGE-BASELINE.md), and
[benchmark policy](../../../../../../../docs/general/benchmark_rules.md).

Owner extension, 2026-10-02: the [S2 specification](../S2-SPEC.md) and
[S2 test contract](../S2-TEST.md) require these existing witnesses and
retained-history obligations to pass before final seven-family qualification.
Published strict SP1 implementation `c838d8d6db85158c6ed9576dd7e8fddf65489704`
and evidence `16407e03ee36e7bde8af66ae3fc12f3b1c197c80` are external dependencies;
their source-specific component proofs do not imply the public SDK/FUSE witness
or every obligation below has passed. New S2 qualification remains `NOT_RUN`.

## 1. Minimal means four complete witnesses

Use existing external C2/adapter tests and the existing `fs-bench-pro` runner.
Add the four witnesses below to those owners; do not build a second framework,
new workload family, or per-edit suite. These compact witnesses precede the
owner-requested final seven-family S2 campaign in section 5; they do not launch
that campaign during development. A component PASS proves
its component contract only. The SDK/FUSE witness is independently required.
Strict SP1 starts from clean published source independently of S2a WIP; no
coherent-S2a prerequisite or WIP import. Reader/writer witnesses must pass before
stride collection. Keep C1
unfinished-draft removal, S2 population/shared-engine/concurrency, successor
cutover and complete-checkout import outside this suite. Global SQLite stores
committed directory/inode/attribute/file-mapping canonical metadata, pooled
values/leaf records, locators and C5 history. MinIO stores file-content whole/CDC
FULL/PREFIX/STORED packs only; no Ordinary/PooledMetadata objects are uploaded.

Freeze fixture/oracle manifests **before candidate implementation or sampling**:
input bytes/SHA256, old producer commit/binary SHA256, C1 roots/ObjectIds,
canonical bytes, roles, predecessors, physical FULL/PREFIX/pool selections,
framed costs, group shape, locators, counters, policies and oracle SHA256.
Generate expected bytes with an independent byte splice; use the sealed existing
C1/C2 producer for canonical and codec/selection expectations. Metadata
physical placement now changes to SQL; retain its canonical IDs and existing
framing/codec rather than a second metadata codec. Freeze new location inventory
separately from these unchanged semantic expectations. Never derive expected
roots/selections by running the candidate. If the proposed fixture fails to
produce a required PREFIX/grouped/pool witness on the old producer, preregistration
is `INCOMPLETE`; freeze a corrected fixture before candidate work, preserving
that attempt. No post-result seed search or threshold relaxation.

All small fixtures use seed `0x9e3779b97f4a7c15`, the existing external-test
`noise(n)` generator: for each byte, apply u64 wrapping XOR shifts left7,
right9, left8 in that order, then emit the low byte. Call this `N(n)`; reset
state per file. `E(x)` XORs bytes `[64,96)` with `0xff`. `T=131072` is the
existing default small-file threshold: length `<T` is whole-file, `>=T` chunked.
Use default codec/framing/depth/work policies unless a listed refusal explicitly
supplies a reduced supported policy. Fixture filenames, ordered operations,
request cardinalities and expected counters are immutable manifest data.

| Proposed witness ID | Minimal fixture and operations | Required result |
| --- | --- | --- |
| `sp1-reader-strict-v2` | Old-producer sealed payload packs: whole-file `N(96000)` and `E(N(96000))`; chunks `N(20000)`, `E(N(20000))`, and a second mutation `[128,160)` XOR `0xff`; unrelated chunk bytes XOR `0xa5`. Store base and dependent in different immutable packs. Read deepest, base, then mixed FULL/PREFIX IDs in reverse locator order, with one repeated requested ID. | Exact requested cardinality/order and every canonical/logical byte; actual PREFIX, intermediate/final ObjectId authentication; same-lane mixed FULL/PREFIX and native multi-record group; base GETs, decoded work and peak bounded by existing policy. Payload reader succeeds without SQL file-payload shadow packs; SQL-only metadata/pool subfixture below reconstructs the same canonical IDs with zero MinIO GETs during its isolated metadata read. |
| `sp1-writer-strict-v2` | The same payload set through real C1 finalized/predecessor handoff. One save contains two similar eligible whole-files (same-save candidate), two grouped native chunks, an exact duplicate, and unrelated payload. Separate saves make the cross-pack chain. Add SQL pooled leaves, attribute/dual-use provenance below and file lengths `T-1 -> T -> T-1`. | Dedup before trial; exactly one eligible trial per selected candidate; FULL and PREFIX counts/costs equal sealed expectations, both formats readable in one lane; grouped Native by framed size rather than one-record-per-group. SQL pool ordinals/groups and threshold roots/bytes retain parity; MinIO upload inventory has file-payload origin only, SQL holds metadata including mappings/attribute values, and exact ID dual use retains both domain locations. |
| `sp1-refusals-custody-strict-v2` | Disposable independent copies of reader/writer fixture; one fault per copy using external provider/SQL controls. Missing payload base pack or SQL metadata/pool dependency, altered base/intermediate frame, wrong role, forward/cyclic chronology, invalid locator, supported chunk-depth1 on the sealed two-edge chunk fixture and fixed chain-work boundary; definite PUT/registration failure and lost ACK after successful PUT/registration. | Typed failure at owning boundary for malformed stored records; policy-ineligible/depth/work candidates select FULL with explicit counters; no codec/I/O/integrity error converted into FULL, no publish/retry/alternate route. Unknown quarantines preserved representations/pins and sends no guessed delete/resend. First valid locator remains selected; discarded Commit ownership cannot reclaim a live base. |
| `sp1-exec-history-strict-v2` | One real SDK Workspace from a prepared empty namespace; three source files `whole=N(96000)`, `chunked=N(200000)`, `boundary=N(T-1)`. Three known successful retained commits: initial; edit `[64,96)` of whole/chunked and append `0x7a` to boundary; truncate boundary to `T-1`, create `duplicate` containing the initial whole bytes. | Generic `WorkspaceApi::exec` runs the ordinary workload through real Linux FUSE, daemon C1/C2, real payload-only MinIO packs, host global SQLite committed metadata/pools and C5. Separate proof reads complete old/new bytes for every path at all three retained roots, content roots/partitions match sealed expectations; metadata and independent full filesystem root follow the timestamp rule below; actual PREFIX and exact dedup recorded, checked cleanup/custody. |

The SQL pool subfixture in `sp1-writer-strict-v2` uses four 40-row inode leaves, first
serial1; values use the existing `metadata_pool` external-test generator:
RegularFile, refs1, seeds0..39, big-endian seed in a 32-byte zero buffer for
content ObjectId and `[seed as u8;8]` for metadata ObjectId. At step0..3,
increment byte1 of the first four encoded values by the step; supply previous
leaf as predecessor. Require one pooled FULL leaf and three pooled delta leaves,
reused value ordinals across leaves/saves, exact canonical bytes after reopen. Read each retained pool leaf independently
and verify its sealed canonical ID. Current pooled decoding authenticates value-
group digests/edge IDs and the requested final leaf, without reconstructing/hash-
checking every intermediate leaf inside a single dependent read. Payload chain
intermediate/final hashing is a separate retained requirement.
The policy group ceiling is 165 values, but public construction emits one leaf
of at most100 values per group. If reservation/group-window ownership changes,
reuse `a_group_holds_one_leaf_and_never_reaches_the_group_capacity`: three
100-row leaves have ordinal starts1/101/201. Do not invent an existing165-value
public fixture or duplicate unchanged capacity proofs.

For the SQL pool seam, include the existing minimal two-leaf same-save
fixture: eight shared values plus two new values from `metadata_pool.rs`. Require
`leaves=2`, `reused_values=8`, `new_values=10` (eight initial plus two new),
private ordinal reuse inside that save, while an unrelated/older captured scope
refuses private groups even after a successful own-save read warmed the cache.
Check publication/save identity and captured SQL visibility ceiling before
value/group cache answers. Observe via a fresh read-only SQL connection that
private rows are absent from public selection until ready; count bounded SQL
group reads/seals, with zero MinIO GET/PUT for that metadata phase. Include
these within the writer/refusal witnesses, not a separate campaign.

The reader reuses the old producer's sealed initial three-file directory/inode/
attribute/mapping graph, with declared mode0640/directory0750/mtime
1700000000000000000ns, scope seed `[0x53;32]`, and the
four-leaf pool chain, now stored in global SQL with existing C2 framing. Read
through the production SQL domain route from fresh connections; require canonical
roots/bytes, pooled dependencies and full retained graph authentication unchanged.
Its isolated metadata/pool-read window has **MinIO GET delta0**; a later file
content read may legitimately GET payload packs. SQL BLOB/group metadata is
allowed; a full old SQL Store containing file-payload packs cannot qualify.

Audit every producer/caller route to the typed domain boundary; absent provenance
fails explicitly. Coalesce provenance vectors into writer/reader witnesses: seal
the first actual CDC chunk from ordinary regular-file `construct_stream(N(200000))`
at frozen default policy before candidate work. Record its raw offset/range,
bytes/hash and canonical ID in the independent manifest; raw length is at most
32768. Use those **exact chunk raw bytes** as `AttributeKey` domain `sp1`, key
`opaque` through ordinary C1 attribute construction. Current `attributes/value.rs`
`emit_value` emits Chunk + ExtentLeaf + FileState. Both real producers must emit
the identical Chunk ID/canonical bytes; a manually labelled `N(20000)` Chunk
cannot prove the regular-file producer, whose default representation is WholeFile.
The raw20k reader/codec component fixtures above remain unchanged.

Both uses reopen with locators keyed `(PlacementDomain, ObjectId)` and agreeing
canonical role/length descriptors; both placements are mandatory for dual use.
Read SQL Metadata first then FilePayload, and reverse the order, sharing the
owner caches. On separate private fixture copies omit each required domain
placement in turn, warm the other domain's ID, then require the missing-domain
read to refuse despite its canonical-ID cache hit. Check domain, physical-body
identity, captured scope and generation before cache answers; separate these
cache keys. Attribute-only use stays SQL, file-only use stays MinIO; dual use
cannot silently substitute SQL for a file read or MinIO for a metadata read.
A shared SQL FileState/Extent mapping ID also retains reference-use facts
`RegularFileGraph` versus `MetadataGraph`; its child routing follows the current
logical use, never whichever use first populated the ID/cache.

Inventory provenance for every uploaded record; reject directory, inode,
attribute, mapping or pooled-value origin, regardless of filename/command or
misleading WholeFile/Chunk role. A dual-use ID in MinIO is allowed only for its
independently recorded file-payload use; metadata lookup still uses SQL. No
recognized command path or special test hook may establish classification.

Persist signature/pool catalog derivations in bounded SQL batches, query through
indexed O(log n) point lookups/keyset pages, and preserve private/public scope.
The small witnesses record SQL row/page/VM work and observed query plans for
changed ownership; no custom rolling B-tree, population cache or new pager is
required. C1 immutable content/metadata trees and bounded codec buffers remain.

The same-save witness freezes current C2 eligibility after its **private group
seal**, before external ACK. Raw `Pending` finalized bytes alone cannot be read
as published PREFIX bases. SP1 must preserve that eligible FULL candidate through
an explicit compatible representation/seal rule, keep first-locator custody, and
avoid one MinIO round trip or immutable-pack PUT per candidate. Read the existing
bounded private placed-pack snapshot only inside its captured save-owner scope. A gate that silently makes every
same-save predecessor ineligible is not parity. Record local retained decode
bytes, GETs and group/pack seals separately from upload/registration ACKs.

Refusal mutations occur outside timers, on private copies, without product fault
hooks, fake clocks/allocators, inline tests or test-only product branches.
Corrupt immutable bytes externally, retaining key/locator so integrity fails;
for payload-chain intermediate-authentication proof, prepare a digest-valid altered pack and
its private selected locator before the read. Distinguish pack SHA, frame
checksum, intermediate canonical ID and final canonical ID refusals; a pack-digest
failure alone cannot prove deeper authentication. Missing candidate is policy
FULL; missing required base of an existing PREFIX is failure. A sealed two-edge chain must fail read at supported chunk-depth1; at writer
chunk-depth2, the first two edges can select PREFIX and the next candidate at
the cap selects FULL without a trial. Use the sealed
C2 depth/work boundary expectations rather than synthetic giant logical files.
Chain work is fixed, not a test-adjustable quota: reuse the twelve-step
`N(120000)` fixture from `a_chain_that_would_exceed_its_work_budget_is_never_stored`
with whole-depth16/chunk-depth4 and step mutations `[step*64,step*64+32)` set to
`0xa5`. Freeze its exact FULL/PREFIX/work-exceeded outcomes from the old producer.
Charge dependencies only for a normal requested-object read; the requested object
itself is charged when resolving it as a dependency, matching existing C2 semantics.

For first-locator custody, submit two valid complete packs for one CAS object
serially with reversed alternate placement, read selected normalized locator,
and prove duplicate registration does not replace it or invalidate dependencies.
Reuse the existing alternate-race/publication owners if that code changes;
SP1 is not a concurrency campaign. Base retention proof removes only an allowed
history owner/reference through its public API, then reopens the surviving
dependent history and reads every byte. If removal/GC is unsupported, preserve
pins and report unsupported reclamation; never invent a GC service to pass.

## 2. Public route, counters, timers and provider identity

`sp1-exec-history-strict-v2` uses arbitrary shell/POSIX workload bytes through generic
Exec. Freeze three Exec processes and three Commit calls: the first creates the three
files in the mounted empty Workspace, the next two perform the listed changes.
No command recognition, SDK range-edit replacement, direct Store mutation
or host canonical construction shortcut. The workload binary/script SHA and
full commands belong to its manifest. The independent verifier uses ordinary
public historical reads; direct SQL/pack inspection is supplemental, never the
semantic oracle. Audit `Engine::now()` and ordinary FUSE timestamp/attribute
operations before pinning metadata: set deterministic mode/mtime only through
supported ordinary operations and freeze those commands. Independent C1 content
roots/partitions and exact bytes are presealed regardless. An independent full
filesystem-root PASS requires deterministic metadata, including directories,
under those supported operations. Otherwise label full filesystem-root oracle
`INCOMPLETE` with the unsupported timestamp field; report semantic content and
actual metadata observations separately. Never fake a clock, use candidate wall
times as oracle inputs or copy its filesystem root into an independent pin.
Freeze exact expected calls after source/API review, not
illustrative counts for Mount/Status/cleanup. Unprovable/nonzero forbidden-route
counts fail authenticity. Missing telemetry is `UNAVAILABLE`, never zero.

Record public calls, Exec processes, FUSE read/write requests/bytes, construction
objects, exact reuses, payload FULL/PREFIX/trials/losses, pooled FULL/delta leaves,
ordinal reuse, framed/group/pack bytes, GET/PUT calls and bytes, chain edges,
canonical/encoded/physical decode work, selected locators and dependency pins.
Separate payload prefixes from pooled deltas and distinguish selected from
retained physical record counts. Use existing production counters or external
provider evidence; add genuine bounded telemetry only when the product needs it.

Component tests are correctness/count diagnostics, with no numeric speed gate.
Integrated witness declares a 15s complete command bound and separate 9.5s proof
bound; existing Workspace16MiB, backing1GiB and container512MiB ceilings stay
unchanged. Combined live SQL/codec/cache peaks need real attribution; bounded
individual buffers cannot establish a whole-owner memory PASS. The external timer is launch-to-exit including Mount/Exec/Commit/Status,
unmount/deletion and receipt/cleanup work; report inner Exec/Commit spans as
nested attribution. Oracle generation, fixture acquisition/builds and verifier
are separate scopes. No per-operation output poll, digest or reconnect in the
performance path. A missing cold contract makes numeric timing `INELIGIBLE` even
when semantic/storage/cleanup PASS. Never credit setup/source/recent-write cache.

Before these tests register provider identity: exact immutable MinIO image digest,
server version, endpoint/bucket isolation, erasure/single-disk mode and disk root,
mounts/resource limits, HTTP/TLS profile, SQLite profile/schema/wire allocation,
SDK/daemon/host binaries and ARMv8 flags. These are prerequisites, not guessed
pins: unresolved identity means `NOT_RUN`. Local prototype MinIO is HTTP without
fallback; that is an experimental topology, not cloud durability. Host owns
SDK/Server/global SQLite/C5; Linux owns daemon/FUSE/workload; real MinIO provider
owns file-content packs at the declared provider location; global SQLite owns
all committed canonical metadata/pool representations and indices. Freeze locality and account for
all owners. Do not put global SQLite/coordinator in Docker or silently switch
provider. Schema/wire changes require reviewed source allocation before tests.

## 3. Existing owning checks and final verification

These are valid **existing** commands, run from the owning worktree root once
at frozen final identity. No checks are run by this document. During diagnosis,
use one affected named test from the relevant target; diagnose red output/source,
apply one fix, then run its covering command once. Passing unchanged proofs are
reused by relevant product/binary/fixture identity, not rerun after every edit.

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage \
  --test delta_payload --test delta_chains --test pack_locator \
  --test metadata_pool --test codec_frames
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked \
  --test packing --test publication
```

`delta_payload` owns selection/dedup/cost/same-save eligibility; `delta_chains`
owns depth/work/chronology/authentication; `pack_locator` owns grouped/closed
placement/locators; `metadata_pool` owns ordinals/pool chains; `codec_frames`
owns frame refusal. Existing adapter `packing` tests are FULL-only and
`publication` uses an unavailable MinIO endpoint: neither proves real MinIO
PREFIX reads. Proposed witness IDs are specification names, **not existing
Cargo targets or runner selectors**. Add them to appropriate external tests
and minimal runner profile after the reviewed interface; publish valid commands
then. Only extend `metadata_pool_index`, `provider_errors`, `persistence_failure`,
`cas_reuse` or candidate-window tests when their actual owning code changes.

Final Core source changes still require the owning workspace checks once:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

At final checkpoint, include locked adapter tests/Clippy/fmt for the independent
`core/benchmark/phase6-live/Cargo.toml` workspace if changed; include only the
focused `core/benchmark/fs-bench-pro/tests/test_history_retention.py` harness
checks if its adapter/registry/accounting changes. Scope these from actual edits.
No CI/preflight/aggregate gate. Builds use `--locked`; measured drivers use
`--release`, own worktree target and sealed binary/image reuse. Record root
`.cargo/config.toml` and ARMv8 `aes_armv8`, `polyval_armv8`,
`chacha20_force_neon`, `+aes,+sha2`; no foreign/shared writable target. Hold the
worktree-local nonblocking lock, one construction worker and
`LAYERFS_CONSTRUCTION_WORKERS=1`; no build overlaps a local timed phase.

## 4. Stride10, stride3, stride1 after the small gates

Reuse read-only corpus `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`,
tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`, manifest SHA256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`.
This frozen 157-state corpus is distinct from the entire current checkout goal.
Reuse sealed independent per-state tree/metadata/content oracles and corrected
Core root ledgers under `core/docs/issues/286/oracles/history-reference-v2/`.
Do not regenerate them with SP1. Each schedule directly transitions between its
selected states; never replay skipped states or infer one stride from another.

| Stride | Independent schedule | States | Logical B | Complete/proof bound | Historical accepted v4 allocation B |
| --- | --- | ---: | ---: | --- | ---: |
| 10 | `sorted(range(1,158,10) union {157})` | 17 | 561010345 | 60s / 10s | 52473856 |
| 3 | `range(1,158,3)` | 53 | 1676767835 | 170s / 20s | 65142784 |
| 1 | `range(1,158)` | 157 | 4936693030 | 170s / 30s | 86179840 |

Freeze source/profile/accounting, then collect once in order10 ->3 ->1.
Stride1 remains explicit run-only and cannot supply optimization input. These
are registered Family2 exceptions, not new ordinary integration timeouts. Do not
raise limits, shrink states, change workers or rerun the same arm after a miss.
Diagnose existing receipts or separately labelled count diagnostics; a relevant
source change gets a new identity and only necessary affected checkpoint.

Historical storage evidence: [stride10](evidence/history-baseline/stride10-receipt.json),
[stride3](evidence/history-baseline/stride3-receipt.json),
[stride1](evidence/history-baseline/stride1-receipt.json),
[derived operands](evidence/history-baseline/derived.json) and
[hash manifest](evidence/history-baseline/manifest.json). r046 source is
`a12ab932e969fabcd1193c62611bde5caf5f2170` (10/3), r047 source is
`b2ea44f0f0f3e0112f8b862ce71f2087e018c7a7` (1). Ratios are10.6912/25.7399/
57.2836x logical/allocated, with pack bodies45594892/57146716/74887668 B and
canonical bytes380559460/589480854/871337620. Payload PREFIX selections38154/
54324/70626 and pooled deltas747/3016/7719 are separate: total prefix_records
38901/57340/78345 includes pooled deltas. Original strict allocation targets
49344512/64024576/83947520 B and their FAILs remain separate from the owner
accepted up-to10% v4 profile. Historical timing stays `INELIGIBLE` and source
scope remains in-process C1/C2/C5, without SDK/FUSE.

Existing grammar, **old SQLite-backed profile only**, shown to prevent invented
flags; these commands are not authorization to recollect unaffected history:

```sh
python3 core/benchmark/fs-bench-pro/runner.py run \
  --case history-retention-stride-10-total-storage-v4 --out NEW_OWNED_OUTPUT
python3 core/benchmark/fs-bench-pro/runner.py verify --run EXISTING_OWNED_RUN
python3 core/benchmark/fs-bench-pro/runner.py report --run EXISTING_OWNED_RUN
```

Stride3/1 use `history-retention-stride-3-total-storage-v4` and
`history-retention-stride-1-total-storage-v4`. Existing `run` invokes performance
and the separate bounded native verifier; `verify --run` rederives retained
evidence only. There is currently **no strict-split MinIO/SQL history selector/profile**; no
`--backend minio`, `--profile sp1` or `--storage-verify-run` in this Core parser.
Add a versioned adapter/profile to this same runner/registry prospectively,
retaining schedule/oracle semantics and documenting topology before collection.

Freeze meaningful same-layer strict-split numeric parity gates before candidate
sampling. Inventory all MinIO payload records/pack bodies plus SQL encoded
metadata/pool records/groups: this sum is the comparable encoded-body denominator
against the historical all-object pack-body totals, not MinIO alone. Also inventory
all MinIO object bytes, provider allocated disk/control overhead, all required
global SQL canonical metadata/group bodies and locator/candidate/pooling/
dependency/history indices, persistent daemon storage, temporary/spool high-water and duplicate/alternate packs. Track retained base
closure and abandoned/Unknown objects. MinIO payload bytes alone cannot be
compared to historical C2+C5 `st_blocks*512`. Separate payload efficiency from
provider disk footprint and total retained allocation; do not apply the old
10% tolerance blindly to a new topology. Without comparable accounting and
predeclared numeric gates, report observations as `INCOMPLETE`, not storage parity.

History construction remains measured work in fresh-growing Stores; reuse only
closed corpus/oracles/build seals, never cached history/results. Ordinary
post-init integration uses independent validated byte-copy setup (`--setup clone`
where its runner supports it), never a mutated sample; clone is not cold.
Preserve append-only fresh outputs and every PASS/FAIL/INCOMPLETE/INELIGIBLE/
NOT_RUN cell. Separate semantics, command/proof budgets, storage, cleanup and
numeric cache eligibility. Before every invocation read
[report template](../../../../../../../benchmark_agent_report.md); publish raw
operands, exact identities/commands, included scopes and omissions. A missing
oracle/provider/cold proof or exceeded bound cannot become admission PASS.


## 5. Owner-requested S2 final qualification extension — 2026-10-02

The owner now requires the simplified S2/SP1 public path to satisfy every
existing requirement in sections 1–4, add deterministic concurrent workload
qualification, and then pass the selected contracts of all seven Core
`fs-bench-pro` families with speed/storage comparison against Core/Phase B #286.
The [S2 test contract](../S2-TEST.md) owns the added requirement-to-receipt matrix,
concurrent capture/successor cases, complete family selection, comparator
bindings and accounting. This extends final qualification rather than weakening
or replacing the compact witnesses, independent root/byte/partition oracles,
retained-history schedules, custody rules or limits above.

Reader/writer/refusal and the real generic SDK Exec/FUSE/C5 witness must retain
full coverage. A component Engine proof is not the fourth public witness.
A serialized public proof is not a non-pausing proof. The added concurrent
workload must demonstrate acknowledged successor reads/writes while captured
Commit remains pending, exact captured publication and successor-preserving
installation, with one construction producer. Multi-Workspace or additional
Exec admission requires its own lifecycle/resource evidence.

Use the accepted #286 [seven-family checkpoint](../../../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
and [exact receipt map](../../../../../issues/286/experiments/20260930-seven-family-reuse.json)
as the selected primary baseline. Historical mixed-source, cache-ineligible
observations stay under their original identities. Qualified speed ratios require
matched baseline/candidate operation, fixture, harness, timing/acknowledgement
and cache evidence. Count strict MinIO payload plus SQL metadata bodies for
encoded parity and each original physical provider/SQL/history owner once for
allocation; never compare MinIO payload alone to historical C2+C5 totals.

The final campaign uses the existing seven family owners and preserves explicit
rows, expected refusals/custody outcomes, failures, deferrals and unsupported
cases. Required unsupported or unrun members block complete-family PASS.
A strict-split history/public route adapter and an exact ordered case/arm
manifest must be published before new samples; no invented backend flags or
component replacement for public workloads. Do not run a broad sweep on every
edit or add another runner. Reuse exact-identity qualifying candidate history
receipts in the final Family 2 table when the same final mechanism and contract
already passed section 4; never sample that unchanged arm twice.

This extension records prospective requirements only. It runs no checks,
benchmarks or concurrency campaign and grants no new PASS or numeric admission.
