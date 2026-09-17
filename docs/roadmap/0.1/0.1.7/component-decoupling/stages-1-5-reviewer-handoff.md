# Reviewer handoff: Stage 5 and the complete Stages 1–5 C1/C2 core

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Independent acceptance review of [Stage 5 / #170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170)
and cumulative core readiness under [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
This is an executable reviewer assignment, not a review result. It covers Stage 0
foundations and existing telemetry where Stages 1–5 depend on them. Stage 6 remains
whole-core qualification; Stage 7 owns subsequent Workspace/runtime integration.

## Copy/paste assignment

Review the actual implementation in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Produce **two explicit verdicts**: Stage 5 acceptance and cumulative Stages 1–5
C1/C2 readiness. This is the final planned core feature-implementation boundary;
inspect complete production paths and their composition, not only the last diff.
Answer all six owner questions:

1. What is the resulting file/folder structure, with incremental Stage 5 and
   cumulative Stages 1–5 production LOC changes?
2. Are every stage's applicable criteria and the composed-core criteria met?
3. What can be deleted, merged or simplified while preserving required behavior?
4. What evidence establishes speed, work/storage efficiency, bounded memory and
   memory safety? Which claims remain unmeasured or unproven?
5. What are the enforced, configurable, theoretical and actually verified limits
   for revisions, files, directories, filesystem size and physical storage?
6. Can later FUSE and host-only, host/remote, host/container or other Workspace
   arrangements use this core? Show exactly how they connect and what is missing.

Read code, callers, external tests and raw evidence independently. An implementation
report, closed issue, passing smoke run or attractive interface is not proof.
Keep product, existing tests, fixtures and harness source unchanged. You may write
the new review and isolated external public-API diagnostic clients/reproducers in
its evidence directory; do not add test-only product hooks or include private source.
Do not implement fixes, commit, push, change issue state or post comments. Preserve
concurrent work and never interrupt or overlap another owner's measurement.

## 1. Freeze the reviewed snapshot and governing contract

Read [repository rules](../../../../../AGENTS.md),
[core rules](../../../../../core/AGENTS.md), the current issue bodies #166–#170,
and these sources:

| Scope | Required documents |
| --- | --- |
| Stage 5 | [Handoff](stage-5-handoff.md), [file/LOC plan](stage-5-file-plan.md), [filesystem design](filesystem-tree.md) |
| Stages 0–2 | [Handoff](stages-0-2-handoff.md), [report and amendments](stages-0-2-report.md), [review obligations](stages-1-2-reviewer-handoff.md) |
| Stages 3–4 | [Handoff](stages-3-4-handoff.md), [file/LOC plan](stages-3-4-file-plan.md), [review obligations](stages-3-4-reviewer-handoff.md), [latest closure record](stages-3-4-completion-round-20260917.md) and the reviews/receipts it cites |
| C1 contracts | [Canonical objects](canonical-objects.md), [file content](file-content.md), [finalized output](finalized-object-handoff.md) |
| C2 contracts | [Physical encoding/pooling/packs](physical-encoding-and-packing.md), [admission/persistence](admission-and-persistence.md), [policy/tables](content-storage-policy-and-tables.md) |
| Composition | [Content I/O](content-io.md), [memory audit](content-io-memory-audit.md), [telemetry](telemetry.md), [implementation plan](implementation-plan.md), [issue mapping](implementation-issues.md) |

Read the actual Stage 5 report and verification specification if present. Their
absence, incompleteness or disagreement with the code is reported explicitly. Follow
the latest owner decisions where older proposals conflict:

- Portable mode/mtime and generic opaque attributes; no Apple-specific semantics
  or APFS materialization. Generic byte preservation is not permission enforcement.
- No retries, error-driven fallbacks, fsync/fdatasync/sync_all/sync_data, WAL,
  added crash-durability services or third-party patches/forks/vendoring.
- SQLite MEMORY journal, synchronous OFF and zero busy timeout retain runtime
  transaction atomicity. Failed-save cleanup must not delete successful versions.
- No payload spool/general scratch service. Narrow reference-ordering records and
  bounded algorithm working memory remain real, explicitly accounted resources.
- No aggregate preflight or CI. Product-only source, external tests, <=999 physical
  lines per production file and <=200 declaration/delegation-only lib.rs/mod.rs.

Record HEAD, branch, source tree, staged/unstaged diff, untracked source, manifests,
lockfile and specification identities. A dirty tree needs a complete content seal;
HEAD alone is insufficient. If implementation is changing, use an already stable
identified snapshot or report affected checks INCOMPLETE. Do not commit another
agent's work or quietly review an older commit as the current implementation.
Recheck identity after checks; bind every result to what it actually exercised.

Use separate comparisons:

1. Actual pre-Stage-5 tree -> reviewed tree, for incremental LOC/regressions.
   `b6b83162a8f4ca3ddb1a59adf1b1ea0cfb6d3369` is the planning baseline, not proof
   of the actual implementation boundary. Derive the commit range from history.
2. Actual pre-Stage-1 C1/C2 tree -> reviewed tree, for cumulative implementation.
   Telemetry already existed; do not count its existing code as new Stage 1 work.
3. Each implementation commit's exact first parent -> committed tree, for the
   required per-commit production LOC audit; keep uncommitted changes separate.
4. Pinned v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, for matched semantic,
   algorithm and performance comparisons. A smaller earlier core lacks features
   and cannot substitute for a feature-equivalent performance reference.

Read current closure decisions and carried residuals such as
[#174](https://github.com/Ephemeral-AI-Lab/layerfs/issues/174). Respect closed scope;
do not silently reopen it, erase it, or promote its unmeasured rows into evidence.
The Stages 3–4 performance waivers remain owner-waived and unmeasured. They do not
waive Stage 5 or establish cumulative performance superiority.

## 2. Actual structure and production LOC

Show an ASCII tree of the complete C1/C2 implementation and relevant telemetry,
runtime SQL, tests/examples and tools. Annotate production files with both production
LOC and physical lines; label supporting files non-production. Report actual paths,
including added, retained, moved and deleted files, not just the proposed tree.

Supply per-file and recursive per-directory tables for both comparisons:

```text
path | pre-Stage-1 LOC | pre-Stage-5 LOC | reviewed LOC
     | cumulative delta | Stage-5 delta | physical lines
     | recommended final range | below/within/above | responsibility/deviation
```

Directory totals include children: never sum parents and children twice. Compare
planned ranges only where a matching responsibility exists; report merged/split
files and missing work explicitly. Recount file totals and deviations from the
actual tree instead of copying report prose. File count and LOC reduction do not
prove completeness or better performance.

Report disjoint C1, C2 including SQL, telemetry, other core, reference and adapters
if present, plus combined product totals. Relocation, coexistence and reference
retirement are distinct from algorithmic simplification. No reference deletion is
authorized by this review.

Use the same audited counter/version, classification and exclusions on both exact
snapshots. Count nonblank/non-comment first-party product code, imports/declarations
and shipped SQL; exclude tests including legacy inline test branches, docs, examples,
fixtures, harnesses, tools, generated and third-party source. Audit the counter's
classification when files moved or formats changed. Report counter defects rather
than certifying misleading totals. Retain the reproducible method and commands.

Audit each implementation commit's actual before/after/signed delta and required
message disclosure; do not rewrite commits. Check physical line caps separately and
inspect responsibility, public exposure and dependency direction. Passing a text
guard alone does not establish SRP or a product-only implementation.

## 3. Criterion-by-criterion acceptance

Build a complete matrix from issue acceptance items, handoff checkpoints and the
current owner contract. Assign stable review row IDs and retain the denominator:

```text
criterion | stage | governing source/decision | implementation location
          | test and actual assertion | raw evidence/source seal
          | observed status | owner disposition | precise gap
```

Use PASS, FAIL, INCOMPLETE, NOT_RUN and justified NOT_APPLICABLE. Keep an explicit
owner-WAIVED disposition separate from observed evidence. A waiver is not a
measurement; a missing proof is not automatically a demonstrated code defect.
Required unwaived gaps block that acceptance criterion. Give separate correctness,
resource, performance, cleanup and evidence-integrity outcomes.

### Stage 5: inspect these obligations in full

| Area | Required checks |
| --- | --- |
| Canonical identity | Shared inode value/leaf codec; resolved 73-versus-81 encoded-row summary with independent golden bytes; exact supported root/branch/leaf/symlink framing, reference ordering and role identity; no duplicate codec or self-referential round-trip oracle. |
| Profile/schema compatibility | Accepted profile and actual schema/role constraints checked against on-disk state, including old stores; same schema-version number alone is not compatibility proof. Verify when refusal occurs, absence of silent mutation/migration and old canonical/pool expansion behavior. |
| Scoped identity | Scope plus serial; caller allocation obligations and exhaustion; root/non-root kind/count invariants; no fake ObjectId from a serial; no unchecked caller count or retention flag accepted as proof. |
| Sorted COW | Optional-base build, exact page occupancy/partition/tail rebalance/height collapse, changed-path work, untouched subtree IDs, grouped reads, final-only child-first emission and no provisional empty seed. |
| Whole-tree validity | Effective final topology, cycles created by several moves, duplicate names/identities, single parent for directories/symlinks, root operations, disconnected dirty inputs and membership before final emission. Page finality alone is insufficient. |
| Reference accounting | Additions before removals across directories/waves; derive new counts once; include aliases outside changes; newest pending value overrides base; moved-out child and externally linked file survive subtree removal; zero-count release bounded; old roots remain readable. |
| Ordering | Compact typed records, actual threshold crossings, tier carry/merge/tombstone precedence, overflow/truncation, simultaneous old/new runs, backing quota and cleanup on success/error/drop; no error-triggered alternate route. |
| Reads | Resolve/stat/list/readlink, duplicate-demand cardinality/order, shared ancestor reads, bounded count and bytes, progressing pagination, wrong summary/kind/scope/truncation rejected; no hidden full-tree collection. |
| Attributes | Portable mode/mtime grammar, generic key bounds/order, opaque untouched-key/value-root preservation, bounded extent-only values, exact page sizing/partitions, no Apple-specific dispatch. Distinguish page equality with supplied value roots from equality of independently constructed complete attribute trees. |
| Real C2 composition | New roles through real save/finish/reopen; correct direct object references; pooled inode interoperability, locators, dependency visibility and batch reuse; no flush per inode/directory. A reported root must have its required objects persisted. |
| Timing and resources | Real independent C1/C2/integrated bodies, bounded reports and on/off equality; validation/ordering/provider/output waits attributed; simultaneous memory/backing costs covered. |
| Verification | Every Stage 5 test obligation, source/fixture seals, meaningful assertions, limits evidence and required comparison. A missing named target needs equivalent mapped coverage elsewhere, not a filename-only verdict. |

The initial Stage 5 draft reports were prepared during active work. Re-evaluate
their missing ordering/failure/bounds cases, comparative measurements, scoped
attribute parity, schema compatibility, LOC census and memory ownership claims.
These are review targets, not predetermined findings or permission to freeze an
earlier incomplete state as the final result.

### Whole-core review: follow every stage through the final tree

| Stage | Required cumulative checks |
| --- | --- |
| 0–1: contracts and construction | Canonical framing/identity and authentication boundaries; exact-length stable inputs, empty/small/large construction, frozen CDC and mapping partitions, complete input validation, direct finalized output and backpressure; C1-only operation without SQLite/Workspace. |
| 2: CAS/FULL/save/read | Exact reuse, admission/dependencies, ordinary FULL and pack locator reads, bounded batch ownership, SQLite transactions, private early output/publication watermark, pending-owner reads, one acknowledged finish, reopen and one definite-failure cleanup; C2-only operation without construction. |
| 3: encoding/pooling/packs | Configurable cutoff and independent payload/metadata chain limits; persisted policy agreement, exact reuse before one allowed delta trial, explicit candidate ordering, framed cost, compression/lane selection, iterative authenticated reconstruction, real value pooling/ordinals/digests/window boundaries and retained cache/index bounds. |
| 4: file edits | Small and large edits, current-result coordinates, stored-subtree split/concat/coalesce, exact reference partition including 80+100 -> 90+90, local CDC convergence/work, bounded decoded frontier, no-op and both cutoff transitions, finality and old-root immutability. |
| 5: filesystem | All rows above plus file content and attributes through real inode/root construction, ordering, pooled save and authenticated reopening. |
| Shared telemetry | One real operation hierarchy, disabled-path behavior, bounded retention/clipping, honest errors and construction-versus-persistence completion; existing timer reused rather than a second monitor. |

Trace complete integration routes: create small/large content -> attributes/inode ->
filesystem root -> C2 finish -> reopen/list/read; localized edit -> new file root ->
inode-only update; small/large transitions under non-default cutoffs; cross-directory
move/hardlinks/subtree deletion -> reference reduction -> pooled persistence; late
failure after private C2 writes -> definite cleanup with previous roots intact.
Use physically valid referenced objects for storage proofs, not synthetic IDs whose
payloads are never supplied. Include duplicate objects and repeated versions.

Check indirect library settings and all reachable routes. Ordinary FULL selection
after a successful losing delta comparison, configured backpressure and planned
ordering passes are valid policy. Codec/read/SQL errors triggering a different
algorithm/backend, busy retry or resend violate the contract. Audit successful
selection and failed-operation behavior separately.

## 4. Simplification and algorithmic work

Read the complete paths and every relevant caller before recommending removal.
Look for repeated encode/hash/decode, temporary inode objects, trial serialization,
whole-file/tree scans hidden behind a batch API, per-object SQL/RPC, repeated group
decompression, clone/collect chains, unchecked or useless caches, redundant state,
provisional graph pruning, unused public APIs, dead versions and placeholder traits.

For each material opportunity report location, current work, smallest change,
affected callers, invariant preserved, verification case and expected LOC/work
effect. Label expected effects estimates; do not invent speedup percentages.
Credit an inherited optimization to its original implementation. Distinguish a
required composition boundary from an interface with no actual responsibility.

Provide before/after ASCII diagrams for construction/edit, attributes/inodes,
ordering, save and read. For example, verify whether this cut actually happened:

```text
reference: typed inode -> encode/store temporary object -> order ID -> read/decode
                                                                -> inline leaf
reviewed:  typed inode -> bounded ordering of final values        -> inline leaf
```

Derive time, extra-space and I/O work in terms of input bytes, changed bytes,
affected paths, tree height, delta depth, distinct groups and released entries.
Initial construction consumes its input; subtree deletion visits released entries;
CDC resynchronization has a worst case. Do not promise universal O(changes), zero
disk or constant memory by ignoring caller input, reference ordering or caches.
Flag asymptotic regressions even when the small smoke fixture is fast.

## 5. Speed, efficiency, bounded memory and safety

### Measurement contract

Before running measurements read [benchmark rules](../../../../general/benchmark_rules.md),
[benchmark instructions](../../../../../benchmark/AGENTS.md),
[quick start](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[release policy](../../../../general/release-policy.md) and
[documentation policy](../../../../general/documentation-policy.md).

Reuse eligible identity-matched receipts under their actual reuse contract. Do not
launch an unregistered full campaign to fill a table. If the comparator, numerical
gates, resource limits or harness are missing/unfrozen, report INCOMPLETE and the
exact missing work; diagnostic runs cannot silently become qualification.

One sample per case/arm under the current owner rule, unless a specific later
approval applies; no best-of/n3 default. Use fresh append-only evidence, equal
declared/enforced cache state, sealed builds, clone setup where applicable and the
measurement lock. No setup/previous-phase cache credit, threshold/worker/timeout
changes to obtain PASS, or omission of failed selections. Preserve the permitted
init_namespace parallel exception; mutation cases remain one producer.
Ordinary complete selection <=15 s, declared exceptions <=25 s and verification
<=60 s under the applicable contract; retain every NOT_RUN reason and measured wall.

### Required statistics

For every claimed operation, give actual input size/change shape, cache policy,
source/operation identity, repetitions, units, timing boundaries and evidence links.
Separate isolated C1, supplied-object C2, integrated acknowledgement and readback.

| Dimension | Evidence to report where applicable |
| --- | --- |
| Time | Elapsed ns, CPU if available, complete command wall; construction, validation, ordering, acquisition, encoding, compression, packing, SQL and finish scopes. |
| Work/round trips | Public calls, page/group/SQL/transaction counts, bytes read/hashed/chunked/encoded/decoded/copied, untouched data visited, ordered-run passes, reused/inserted objects; actual mechanism counters or labelled source-derived bounds. |
| Storage | Canonical bytes versus encoded records versus final pack/SQLite footprint; pack framing/slack/indexes and temporary ordering high water; exact/delta/pool reuse across revisions and incompressible inputs. |
| Memory | Simultaneous live ownership, allocations if measured, anonymous RSS, file cache and other relevant cgroup domains separately; exact scope of each peak and unobserved components. |
| Scaling | Increasing real file/tree/history size with a fixed localized edit; cutoff boundaries and supported non-default settings, actual chain caps, many aliases, wide/deep trees and ordering thresholds. |
| Safety/failure | Public malformed-input/error coverage, allocation and arithmetic checks, FFI audit, late-error cleanup, retained old data and no retry/fallback. |

A source-derived budget is not measured RSS. A lifetime high-water mark is not a
phase-local peak. A sample's configured ceiling is not observed use. Missing metrics
stay unavailable with scope/reason, never zero. Do not fabricate synthetic logical
sizes as measured physical workloads. Complete and reproduce the arithmetic for
throughput, read/write amplification and storage ratios, with the byte basis stated.

Compare only equivalent successful operations with identical normalized inputs,
profiles, worker policy, cache conditions and included work. Preordered C1 versus a
complete reference Workspace pipeline is not a valid speed comparison. Raw smoke
numbers and waived comparisons cannot justify "as fast as v0.1.6". State separately
what the source reduces and what measurements establish.

### Independent timers must diagnose the actual boundary

Inspect C1-only, C2-only and integrated routes for both content and filesystem work.
Verify the same production bodies run with timing on/off and retain errors. Check
whether provider work is timed inside an inclusive C1 span and labelled correctly;
do not call it pure C1 CPU or obtain CPU time by subtracting overlapping wall spans.
SQL timing must include its real SQL work; final persistence includes finish/ack.

Trace growth must be bounded independently of file/inode/object count. Clipping
cannot disappear from the report; check coarse totals remain interpretable and that
qualification rejects incomplete traces where required. JSON output belongs to the
caller with explicit destination/error handling and no sync/durability requirement.

### Bound every simultaneous allocation and backing resource

Build this ledger from allocation sites and release paths, not just budget structs:

```text
owner | buffer/state | capacity source | maximum multiplicity
      | concurrent overlap | growth checked before allocation?
      | release on success/error/cancellation | measured scope | evidence/gap
```

Cover caller input arrays/normalization, pending maps/topology sets, input windows,
CDC/base reconstruction, decoded COW pages, provider-returned object batches,
finalized bytes/references, C2 preparation/pool/index/codec workspace, pack assembly,
SQLite caches/transactions, ordering runs and descriptors/readers, output buffers
and timing reports. Vec capacity and allocator/container overhead count where relevant.
An owned Vec returned by a provider is not borrowed/free memory. Bounded internal
scratch does not establish a bounded whole operation if supplied input or another
live owner grows without a declared bound.

Audit ordering bytes while old and merged runs coexist, peak memory/disk counters,
quota enforcement, resource release and checked finish errors. Calling temporary
records "backing" does not exempt disk or OS cache. Algorithm scratch memory and
compact ordering records are not permission to restore whole-payload staging.

**Memory safety is a separate verdict from bounded memory.** Inspect unsafe/FFI
sites, buffer lengths/capacities, initialization, ownership/lifetimes, checked casts,
decompression expansion limits, corrupted locators/ordinals and overflow. Identify
what Rust checks, what a dependency guarantees and what tests actually exercise.
Use applicable existing sanitizer/Miri evidence if available; report unsupported or
unrun tooling honestly. Do not claim mathematical memory-safety proof from Clippy,
a passing test suite or low RSS, and do not patch a dependency to run a tool.

## 6. Limits: answer the owner's concrete capacity questions

Every limit needs source, unit, scope, configurability, boundary failure, largest
verified input and evidence. Separate format width, enforced policy, derived bound,
backend/build restriction, measured coverage and future-owner responsibility.

| Question | Required distinction |
| --- | --- |
| How many file revisions? | Total retained roots/versions versus maximum whole-file, chunk and pooled-metadata delta depth. A chain cap is not a history-count cap; inspect how later successful FULL representations bound chains. History naming/retention/GC is outside C1/C2. |
| Maximum file size? | Logical length/offset widths, chunk/extent counts, tree depth/fanout, canonical-object and decoder limits, accepted cutoff/capacities, backend row/blob/pack address limits and verified real bytes. The small-file cutoff is not a maximum file size. |
| Maximum file count? | Files versus directory entries/hardlink aliases, unique inodes, scope/serial/count widths, inode-tree capacity, allocation lifetime and storage limits. Distinguish one-operation input/resource limits from total stored filesystem size. |
| Maximum directory length? | Answer name bytes versus characters, full path bytes, component count, directory depth, entries per directory and listing page size separately. Native inode-based operations and path-based APIs may have different bounds. |
| Maximum filesystem/workspace size? | Logical unique payload versus bytes summed through aliases, all retained revisions versus one root, actual physical packs/DB plus temporary records; C1/C2 provide filesystem roots, not a completed Workspace quota/lifecycle system. |
| Attributes/symlinks? | Domain/key/value sizes, entries, extent-only representation, portable ranges, symlink target bytes and which bounds are fixed/configurable/verified. |
| Storage capacity? | Actual SQLite build/runtime limits, signed IDs/ordinals, pack locator widths, row/transaction/group ceilings, disk availability and retained resources; do not borrow an unrelated advertised SQLite maximum. |
| Operation/resources/concurrency? | Input batch, pending records, number of runs/readers, backing quota, provider batch bytes/count, decoder and report bounds, reader/writer ownership and serialization. |

Check boundary and first-invalid cases where feasible through real public APIs.
Distinguish cheaply testable arithmetic bounds from impractical huge fixtures; no
test-only hash injection or weakened production limits. Do not invent a universal
workspace maximum, interpret u64 as unlimited capacity, or extrapolate from a small
fixture to millions of files. For absent Workspace behavior say which core limits
would constrain it and what a later adapter must specify.

## 7. Environment independence and exactly how to connect adapters

Assess implemented capability, architectural feasibility and tested deployment
separately. A Rust trait is not a wire protocol, and a configurable local DB path is
not a remote SQLite implementation. No new plugin registry, backend framework,
network stack or FUSE implementation is required for this review.

Draw the actual dependency direction and both data paths:

```text
WRITE: caller -> native C1 input -> finalized owned objects -> bounded C2 save
                                                               -> finish/ack
READ:  caller <- logical C1 reader <- authenticated object batches <- C2 locators
```

Map every seam to its exact public symbol, source and consumer. Starting audit
points include AuthenticatedObjects, FinalizedObject/FinalizedConsumer,
FilesystemInput/FilesystemResources, OrderingBacking/OrderingRun,
Store/SaveOperation/SaveHandoff and TimingScope/TimingReport. Verify actual current
signatures/export paths; names here do not establish that the contract is adequate.

For each seam report:

```text
caller/owner | actual API | borrowed/owned bytes and release event
             | ordering/authentication/identity preconditions
             | batch count+byte limits | backpressure | errors/cancellation
             | process-local assumptions | future adapter work
```

Check hardcoded filesystem paths, native handles, concrete FileBacking use,
connection ownership, process-local lifetimes, thread assumptions, global state,
host byte order and caller-supplied resource escape hatches. Distinguish an optional
local provider implementation from an algorithm that requires that provider.
Validate C1 without C2 and C2 without construction through real public entry points.

### Review these placement choices

```text
A. One process / host only
   application or FUSE adapter -> C1 -> C2 -> local SQLite

B. Workspace elsewhere, core together
   [host/container/remote: Workspace or FUSE]
                  | bounded operation data and results
   [selected service: C1 + C2 + its storage]

C. C1 and storage separated
   [C1 caller] -- bounded output/read batches --> [C2 service + storage]
                  <-- results / acknowledgements --
```

Host/container/remote are placement labels, not semantic types. Swapping which
side is local must not change canonical objects, edit semantics or ownership.
For each choice give READY TODAY / ADAPTER WORK REQUIRED / CORE BLOCKER /
UNVERIFIED, with evidence and minimum work. Distinguish moving the entire embedded
C2 service from replacing C2's SQL backend with cloud/remote SQLite.

For remote SQL, inspect actual query/transaction, locator/range-access, batching,
writer-ownership, visibility, byte-limit and no-sync/no-WAL requirements. State what
requires a C2 implementation change. Do not claim that a URL or one trait makes an
arbitrary provider compatible. Unsupported required capabilities fail explicitly.

### Show the smallest integration recipe

1. The future adapter captures live operations/dirty ranges and produces stable
   native changes, authorized scoped identities and bounded resources. Define its
   normalization/order cost; do not assume the old Workspace shape or treat this
   work as free preparation in an end-to-end claim.
2. It supplies input/read/output/ordering capabilities and starts the operation's
   timer. C1 algorithms operate without FUSE paths, daemon identities or history.
3. It connects finalized output to the same C2 save and backpressure contract.
   The adapter distinguishes construction completion from acknowledged persistence;
   publishing a Workspace/history root is a later owner after successful finish.
4. It serves FUSE lookup/readdir/getattr/read using bounded logical reads, while
   live write state, locking, permissions, errno mapping and open-handle lifecycle
   remain adapter/Workspace responsibilities.
5. Across a process boundary, a later transport preserves batch order/cardinality,
   byte/count bounds, identity checks, cancellation and single-attempt errors.
   Full output queues stop production; reads stay grouped. Quantify potential
   network exchanges per logical operation and identify accidental per-node RPC.
6. A lost acknowledgement returns failure/unknown outcome, never automatic retry,
   resend, guessed success or unsafe cleanup of possibly successful output.
   Trace composition carries completed bounded reports; cross-host monotonic
   timestamps are not directly subtractable and remote elapsed times may overlap.

Include one source-accurate minimal local composition example or a retained external
client using current public APIs. For unimplemented remote/FUSE connections provide
clearly labelled pseudocode and an adapter mapping, not fabricated callable APIs.
Demonstrate two different providers at an existing public seam when feasible without
product changes; otherwise identify the exact limitation. Do not demand generic
traits around every algorithm or a cloud deployment to accept an environment-neutral
core. A required core change is a readiness gap; ordinary future adapter work is
not evidence that Stage 5 must implement Stage 7.

## 8. Verification to perform and record

Run the Stage 5 handoff's focused external targets and the affected whole-core
checks on the identified tree. Reuse previous PASS only where policy permits an
identity-matched receipt. Preserve source and serialize resource-sensitive work.

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

fmt has no --locked flag. Resolve any formatter-pin discrepancy from governing
instructions and report it; do not reformat source to make the review pass. Record
actual test discovery and assertions, exits, logs and all failures/skips. Missing
targets, ignored tests and zero-test binaries cannot supply criterion coverage.
No retired preflight/aggregate replacement and no claim that CI is green.

Inspect the real independent construction/edit/filesystem/C2/pipeline examples
and their CLI parsers before running their documented smoke cases into fresh paths.
Prove supplied-object C2 tests do not perform timed C1 construction, and that an
example's --case/--mode actually selects the claimed operation. Smoke tests prove
wiring/correctness only. Reproduce retained sealed canonical fixtures through the
independent reference route where needed; never overwrite historical fixtures or
seal the candidate's own result as its oracle.

For a finding, retain the smallest public-API reproducer or concrete source path
showing the violation. Do not stop the cumulative review after finding the first
Stage 5 blocker; complete independent read-only checks and report any blocked tests.

## 9. Required report and handoff

Write a new dated `stages-1-5-review-YYYYMMDDTHHMMSSZ.md` beside this prompt and retain
fresh evidence under `../evidence/stages-1-5-review-YYYYMMDDTHHMMSSZ/`. Do not overwrite
prior implementation/review reports, receipts or failed attempts. Use a clear status
banner and a manifest of retained evidence identities.

Lead with findings ordered by severity, each with exact source location, trigger,
observed/expected behavior, consequence, evidence and smallest recommended remedy.
Keep recommendation estimates separate from tested facts. If no finding is proved,
say so without erasing missing evidence.

The report must contain:

1. Executive answers to all six owner questions; separate Stage 5 and cumulative
   verdicts, then feature completeness versus qualification versus runtime readiness.
2. Exact reviewed source/contract identities, comparison bases and reuse/disposition.
3. Actual ASCII source tree, complete LOC/physical-line tables, plan deviations,
   migration subtotals and per-commit audit.
4. Every stage's acceptance matrix with explicit denominators, open/waived items
   and cumulative integration results.
5. Simplification recommendations and before/after diagrams with ownership/work.
6. Actual timing/work/storage statistics and reproducible arithmetic; memory-owner
   ledger, separate memory-safety assessment and all unmeasured claims.
7. Capacity/limitations table answering every row in section 6, including the
   largest verified cases and unsupported future Workspace claims.
8. Placement diagrams, exact API/adapter map, minimal integration example and
   concrete distinction between adapter work and core blockers.
9. Exact checks, receipts, failed/ineligible/not-run cases and remaining proofs.
10. Ordered next actions with scope/owner and a verification condition for each.

Finish by answering: Can Stage 5 be accepted under its current scope? Is the whole
C1/C2 core feature-complete? Is existing-or-better performance demonstrated, waived
or still unknown? Can Stage 6 qualify this artifact, and what must a later Stage 7
adapter supply? No issue closure, release decision or runtime implementation is
performed by the reviewer.
