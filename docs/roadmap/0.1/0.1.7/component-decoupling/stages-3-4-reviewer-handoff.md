# Reviewer handoff: Stages 3–4 — correctness, efficiency and actual limits

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Independent review of [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168)
(physical encoding/pooling/packing) and
[#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169)
(localized edits/transitions), under
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
This is a reviewer prompt, not an implementation review or acceptance result.

## Copy/paste assignment

Review the actual Stages 3–4 implementation in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Produce a report answering all five questions:

1. What is the resulting file/folder structure and production LOC change?
2. Are all applicable Stage 3 and Stage 4 criteria met?
3. What can be removed, merged or simplified without weakening the contract?
4. What actual statistics and source evidence establish speed, efficiency,
   bounded memory and memory safety? What remains unproven?
5. What limits exist: file revisions, file size, files per workspace, directory
   size/name/path/depth, workspace size, and relevant underlying storage limits?

Inspect code, callers, tests and retained raw evidence independently. Do not accept
an implementation report, closed issue, proposed constant or green smoke example as
proof by itself. Keep product source unchanged. Write the report and retain evidence;
do not implement fixes, commit, push, close/reopen issues or post comments.
Preserve other agents' work and never interrupt their measurement runs.

## 1. Freeze scope, source and acceptance criteria

Read [repository rules](../../../../../AGENTS.md),
[core rules](../../../../../core/AGENTS.md), the complete
[Stages 3–4 handoff](stages-3-4-handoff.md), its
[exact file/LOC plan](stages-3-4-file-plan.md), and current #168/#169 issue bodies.
Read `stages-3-4-report.md` and `stages-3-4-verification.md` if present; missing
required deliverables are gaps, not reasons to invent their contents.

Follow the handoff's detailed sources, especially [file content](file-content.md),
[physical encoding/packing](physical-encoding-and-packing.md),
[policy/tables](content-storage-policy-and-tables.md),
[finalized output](finalized-object-handoff.md), [I/O](content-io.md),
[persistence](admission-and-persistence.md) and [memory audit](content-io-memory-audit.md).
Before measurement read the [benchmark rules](../../../../general/benchmark_rules.md),
[benchmark instructions](../../../../../benchmark/AGENTS.md),
[quick start](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[release](../../../../general/release-policy.md) and
[documentation policy](../../../../general/documentation-policy.md).

Record HEAD/tree/branch, staged and unstaged changes, untracked product files,
dependency/build/harness identities and reviewed specification identity. Pin the
actual reviewed snapshot; HEAD alone does not identify uncommitted implementation.
If implementation continues concurrently, use an identified stable snapshot or
mark affected results incomplete. Recheck relevant identities after verification.
Do not overwrite prior reports or present tests of one tree as tests of another.

Use three distinct comparisons:

- Actual pre-Stage-3 implementation snapshot versus reviewed tree for incremental
  LOC and regressions. The file plan's `5e8b8cbc2` is a planning baseline; resolve
  the real implementation base rather than assuming it is still correct.
- Exact first parent versus committed tree for each implementation commit's LOC.
- Pinned v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` for equivalent algorithm,
  performance and storage comparisons. Stage 2 lacks edits/delta and cannot stand
  in for a feature-equivalent performance reference.

Later owner instructions govern stale wording: tools/preflight.sh is permanently
retired. Do not run/restore it or add an aggregate replacement. Preserve the
Stage 2 publication watermark and pending-group fixes; its amended baseline has
four tables/twenty columns, not the original nineteen-column proposal. Audit any
later schema/profile change against its explicit compatibility decision.

## 2. Resulting structure and production LOC

Show the actual ASCII tree for C1/C2, including runtime SQL and relevant supporting
tests/examples/tools. Annotate each production file with production LOC and physical
lines; mark other files non-production. Include changed dependencies and API exposure
when they affect isolation or responsibility.

For every production file and recursive directory, report:

```text
path | before production LOC | actual after | signed delta
     | recommended final range | below/within/above | physical lines
     | added/updated/retained/moved/deleted | explanation
```

Compare with the file plan, including justified merges/splits, absent planned files
and unexpected additions. Its ranges are final sizes including old implementation,
not additions. Parent directory totals include children; do not sum both again.
Report disjoint C1, C2 including SQL, telemetry, remaining core, reference, adapter
and combined product totals as applicable. Moving codec.rs into codec/ is relocation;
retaining old reference source is coexistence, not removal or algorithmic savings.

Use one audited counter/version and classification for both snapshots: nonblank,
non-comment first-party production including imports/declarations/runtime SQL;
exclude tests, examples, fixtures, tooling, docs, generated and third-party code.
Verify core SQL and legacy inline-test/cfg(not(test)) handling. Report unresolved
counter defects instead of certifying totals. A retained external counting correction
is allowed if applied identically to both snapshots; do not edit product source.

Audit exact per-commit first-parent counts and message disclosures. Keep working-tree
observations separate. Check physical limits independently: <=999 for production
files, <=200 for lib.rs/mod.rs with declarations/reexports/direct delegation only.
Read responsibility/ownership, not just line counts. Neither a smaller file nor a
below-estimate total proves complete functionality or efficient code.

## 3. Are all criteria met for each stage?

Map every #168/#169 acceptance item and implementation-handoff obligation to source,
external test, retained evidence, status and precise gap. Use PASS / FAIL /
INCOMPLETE / NOT_RUN; NOT_APPLICABLE requires an explicit scope justification.
Distinguish demonstrated defects from missing evidence. Required missing evidence
blocks full acceptance. Give separate Stage 3 and Stage 4 verdicts.

At minimum cover:

| Area | Required review |
| --- | --- |
| Isolation | C1 logical edits run with supplied I/O and no C2/SQLite dependency. C2 saves supplied objects without C1 file construction. No Workspace/FUSE/daemon/history/checkpoint coupling or reference fallback. |
| Configurable transparency | Defaults 128 KiB/8/4; accepted non-default cutoff/depth values; 256-KiB and 1-MiB cutoff cases required by the handoff; derived capacities, persisted reopen agreement and explicit rejection. No clamping or fixed-depth checks contradicting accepted config. |
| Payload encoding | WHOLE_FILE and CHUNK FULL/PREFIX; exact reuse first; correct explicit/cache/first-candidate ordering; actual framed cost and FULL-based grouping; at most one trial under the specified policy. |
| Dependencies | Iterative chain reconstruction; separate depth, encoded/decoded work and live-memory bounds; role/length/chronology/cycle/intermediate identity checks and ceiling-aware cache/base reads. |
| Physical metadata | Supplied canonical leaf grammar, exact value pooling/ordinals/digests, pooled FULL/COPY-INSERT DELTA, catalogue integrity and index window/reset/reopen/failure semantics. File payload delta alone does not complete #168. |
| Pack/read path | Placement before one selected assembly, stable locators, bounded grouped acquisition and selected-encoding singleton; required format/profile readers with explicit dispatch. No trial decoder or RAW switch after error. |
| Single edits | Overwrite/insert/delete/append, current-result coordinates, frozen CDC/extent partition/root identity, old-root immutability and real localized I/O. |
| No-op/transitions | Empty/equal edits, bounded long-prefix comparison/replay, correct correspondence after shifts; small/large/empty transitions at accepted T with actual physical acquisition charged. |
| Multiple edits/finality | Checked normalized stream, exact segmentation, both sides of joins, 80+100 -> 90+90 partition case, height/root collapse, bounded decoded frontier and child-first final output with no unreachable emitted drafts. |
| Storage regressions | Exact duplicates within/across preparation waves, unfinished-group owner reads, private early commits, retained publication watermark, failed-save cleanup and previously successful objects intact. |
| Independent measurement | Real C1-only edits, C2-only save/read, integrated edit-to-ack and readback; original errors and identical enabled/disabled work; complete bounded coarse timing and honest inclusions. |
| Resource/safety | Simultaneous input/base/frontier/record/pack/SQL/index/report ownership, direct large canonical singleton, no payload spill/scratch, no size-proportional hidden collectors; FFI lifetimes and arithmetic. |
| Delivery/constraints | Required source/tests/report/LOC/verification evidence; no retry/fallback/fsync/WAL/third-party patches, external tests, file caps and locked dependencies. |

Check runtime failure paths and indirect library settings, not only forbidden-word
searches. Absent/ineligible optional candidates and successful losing delta trials
are normal FULL selection; corrupt required data, codec/read/SQL failure must fail.
Group sealing is not a commit; finish must include all required acknowledgement work.

Keep deferred scope explicit: Stage 5 filesystem algorithms and Stage 7 Workspace/
FUSE/cloud are not implemented merely because C2 accepts an inode leaf. Do not demand
those features for #168/#169, or use deferral to excuse required pooling, multi-edit
finality, larger cutoffs or transitions. Stage 6 broadens qualification; Stage 3–4's
own promised correctness/performance/resource criteria still require evidence now.

Run the implementation handoff's focused tests, workspace checks and real examples
on the pinned state, or reuse evidence only when the applicable policy permits exact
identity-matched reuse. Record actual commands, exits, test counts and every gap.
An absent test target or zero-test discovery is not PASS. Use explicit core test/
fmt/clippy/guard commands; no aggregate gate, repeated unchanged qualification or
resource-sensitive run alongside builds. Review public-input tests and their oracles;
candidate encode/decode agreement alone does not prove frozen reference equivalence.

## 4. Can it be simplified further?

Trace construction, save, read and failure paths through all relevant callers.
Look for repeated encode/hash/decode of drafts, graph pruning, duplicated payload
owners, whole-input/edit collectors, point loops behind batch APIs, root/plan
reacquisition, per-call connection/codec creation, repeated group decode, double
pack assembly, dead config/API/compatibility paths and wrappers with no responsibility.
Check whether an optimization was already present in Stage 2 or v0.1.6 before
crediting it again.

For each material finding give location, current behavior and work cost, smallest
deletion/change, callers affected, invariant preserved, estimated LOC/work benefit
and a verification case. Use a short before/after ASCII diagram where it clarifies
the change. Estimates stay labelled estimates. No arbitrary finding quota.

Do not remove trust-boundary validation, exact CAS comparison, candidate quality,
backpressure, atomicity, visibility or cleanup to shorten code. Do not propose a
registry, generic buffer manager or interface for a hypothetical future deployment.
Keep the report actionable; do not implement its recommendations during this review.

## 5. Statistics: speed, efficiency and memory

For each claim distinguish measured observation, source-derived bound and unproven
expectation. Use the committed case specification and retained receipts. If no valid
component campaign exists, state the qualification gap and the smallest missing
case; do not build a speculative benchmark framework or manufacture a speedup.
Allowed smoke/diagnostic observations remain admission-ineligible.

| Dimension | Requested statistics/proof |
| --- | --- |
| Time | C1 edit/construct, C2 save-to-ack, authenticated read, integrated edit-to-ack, readback and complete command wall; units, sizes, sample counts, includes/excludes. |
| Work | Actual SQL statements/transactions, source/replacement/base/pack reads, scanned bytes, canonical copies/hashes/encodes, codec trials, pack assemblies and rewrites; counts need provenance. |
| Selection/storage | Supplied/usable candidate counts, exact reuse, FULL/DELTA wins and reasons, observed chain depths/work, total retained DB/pack/index/value-group bytes and occupancy. Frame size alone is insufficient. |
| Memory/disk | Declared allocation ledger plus observed process/SQL/index memory where available; separate heap/RSS/lifetime/phase scopes, file cache and temporary payload disk from retained Store/fixtures/verifier resources. |
| Timer cost | Matched enabled/disabled work, results/errors, absolute and relative overhead, node count/clipping and output size. |
| Scaling | Real tested file sizes, revision counts, edits and streamed file counts, with memory and work trends; no extrapolated performance at a theoretical maximum. |

Use one sample per case/arm unless an applicable owner-approved campaign says
otherwise. With n=1 report one observation, no p95 or statistical-confidence claim.
Retain failures and all declared cases; obey measurement lock, worker/cache/preparation
rules and lifecycle budgets. No warm setup credit, best-of reruns or raised limits.

Only compare v0.1.6 on equivalent successful public operations, profiles, inputs,
acknowledgement boundaries, worker counts, cache/index states and controlled harness
semantics. A legacy Workspace pipeline versus candidate C1-only edit is not a pair.
New unsupported-by-reference cutoff values get correctness/resource evidence and
separate diagnostics. CPU time cannot be obtained by subtracting overlapping wall
spans. A synchronous construction span may include consumer/storage time; label it.

Show actual C1-only, C2-only and integrated report trees with artifact links. Keep
required base acquisition, comparison/replay, encoding/SQL and waits inside their
declared operation. Distinguish finish's final drain from earlier writes. Audit
bounded telemetry and clipping; missing detail is not zero time.

Give two separate memory conclusions:

- **Bounded resource use:** identify owner, byte/count bound, live multiplicity,
  capacity/transient overlap, lifetime and release event for every significant
  allocation, including COW frontier, bases, FULL/PREFIX alternatives, all pack
  lanes, SQLite MEMORY journal/BLOB copies and ordered-set nodes. Bound multiple
  files/edits/Stores, blocked output and failures. SQLite cache_size is not total RSS.
- **Memory safety:** inspect unsafe/FFI, borrowed prefix lifetimes and reset/error
  paths, allocation/offset arithmetic, decompression lengths, malformed records,
  intermediate authentication and use after failure/cleanup. Cite public-input tests
  and compatible existing external tooling. Safe Rust or low RSS alone does not
  prove dependency safety. Missing instrumentation is a coverage limit.

Every unavailable metric is null with reason, never fabricated zero. Record peak
sampling coverage and distinguish lifetime high-water from a phase-local maximum.
Synthetic sparse/repeated fixtures may support structural tests, not performance
claims at their apparent logical size. Total storage after many revisions includes
retained versions and their physical dependencies, not only the latest root.

## 6. Mandatory limits and supported-envelope audit

Give a user-readable answer for every requested dimension, even when its owning
component is deferred. Derive limits from the actual path: API validation, arithmetic,
canonical/physical formats, traversal/fanout, policy, codec work, database schema and
runtime limits, allocation budgets and tested behavior. A wide integer field is not
an end-to-end support promise; an absent explicit cap does not mean unlimited.

For each dimension fill these columns, using separate rows when limits have
different scopes/units:

```text
dimension | owner/scope | unit | default | accepted configurable range
          | enforced hard/resource limit | format/theoretical ceiling + derivation
          | largest actually verified case + evidence | first limiting mechanism
          | at/over-limit behavior | source locations | confidence/gap
```

Distinguish **enforced**, **configurable**, **derived/theoretical**, **verified**,
**environment-dependent**, **not yet qualified** and **outside Stage 3–4 ownership**.
Where possible test the real boundary immediately below/at/above it through ordinary
APIs, checked over-limit metadata or bounded fixtures. Do not allocate a theoretical
multi-terabyte maximum, exhaust disk or start an unregistered endurance campaign.
Report impossible/unrun boundary coverage honestly. Cite bytes versus characters,
inclusive/exclusive bounds and operation versus Store scope.

### Required dimensions and misleading inferences to reject

| Dimension | What the reviewer must establish |
| --- | --- |
| File revisions | Distinguish retained immutable file roots from a history/branch revision service. Determine whether revision count is explicitly capped, or grows until resource/ID/storage bounds. Retaining a root and its dependencies is different from merely computing it. No successful-version rollback is introduced. |
| Delta depth | Report WHOLE_FILE, CHUNK and pooled-metadata depth/work bounds separately. A chain cap of 8 or 4 is not a cap of 8 or 4 file revisions. Verify a history longer than the configured cap, normal FULL selection at ineligibility, and continued readback of retained earlier results. Raising depth must not silently raise memory/work budgets. |
| File size | Separate empty/WHOLE_FILE routing threshold, maximum canonical object/field, chunk size, extent offsets/counts, tree height, logical length fields and real streaming/read/edit limits. Report the effective supported file-size envelope and largest physical test. T=128 KiB or T=1 MiB is not a maximum large-file size. |
| Files in one operation | Establish streaming across many supplied file roots, per-batch versus whole-save limits, object/reference bookkeeping and transaction bounds. A 512-object batch or 128-ID query page does not mean at most that many files can exist. |
| Files per workspace | Locate an actual filesystem/Workspace owner before claiming a value. Stage 3–4 storage of independent roots does not implement workspace membership/count enforcement. Report underlying verified object/file-root scale and defer the workspace cap to Stages 5/7 if no such owner exists. |
| Directory length/size | Report separately name-component length (bytes and character rules), full path length, nesting depth and entries per directory. These are different limits. A pooling value, inode leaf or mapping-tree height does not establish directory limits; check Stage 5 ownership. Never import host PATH_MAX/NAME_MAX as a core contract without code evidence. |
| Workspace size | Distinguish sum of current logical file lengths, retained-history logical bytes, unique canonical bytes, physical encoded Store bytes and quotas. Deduplication can make logical size differ greatly from disk use. If Workspace quotas/membership are absent, state that the core cannot yet promise a workspace maximum. |
| Store/database | Audit object/pack IDs, ordinals, row cardinalities, group/record counts, BLOB length and SQLite page/database/runtime limits for the actual build/configuration. DB file maximum is not a maximum single file or workspace. Selected provider limits may be lower; unimplemented cloud/remote providers are unqualified. |
| Pool/index | Separate 131,072-entry candidate window from all persisted metadata values, ordinal exhaustion, values/group and total filesystem count. Cache-window eviction does not delete stored values or impose the same cap on a workspace. |
| Edits/concurrency | Maximum accepted edit shape/count, replacement length, replay requirements, finality/frontier limits, simultaneous owners/Stores/readers and error behavior. Fixed-worker operation limits are not workspace-size limits. |

When deriving an effective ceiling, show the binding constraint and compatible units;
do not take a minimum of unrelated byte/count/depth values. Record how a boundary
fails: validation before mutation, ordinary planned FULL/new-pack selection, resource
error, or late failure with cleanup. Trace these against the single-attempt contract.
Do not silently clamp, increase budgets, retry or swap algorithms for a limit test.

The final limits summary must state actual values only when supported by source or
evidence. Examples of acceptable wording: "no revision-count cap found in reviewed
C1/C2 code; history service outside scope; N retained roots verified" or "directory
name/path/entry limits not established by Stages 3–4; Stage 5/7 review required."
Replace N with a measured count or say not measured; these examples are not results.
Do not label a later-stage omission a Stage 3–4 defect unless this batch promised it.

## 7. Required report and final verdict

Save a fresh `stages-3-4-review-<UTC timestamp>.md` beside this prompt. Begin with a
dated-review status banner and exact source identity. Retain commands/output,
per-file/directory LOC, criterion table, limits table and available raw receipts in
`docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-<UTC timestamp>/`.
Earlier evidence stays unchanged. Large tables may be separate CSV/Markdown files
linked from the report; no reporting framework is needed.

Order the report:

1. **Verdict:** separate Stage 3 / Stage 4 PASS, FAIL or INCOMPLETE and issue-close
   readiness, with specific blockers. Do not actually change issue state.
2. **Findings:** severity, exact source location, trigger/impact, evidence and
   smallest recommended fix. Correctness/data-loss failures first.
3. **Structure/LOC:** actual ASCII tree and per-file/directory before/after/delta,
   estimates, hard caps, relocations and per-commit accounting.
4. **Criteria:** all requirements, source/tests/evidence, statuses and gaps.
5. **Simplification:** concrete ranked cuts and useful before/after diagrams.
6. **Statistics/memory:** actual timing trees and metrics; measured/source-derived/
   unavailable labels; distinct boundedness and safety conclusions.
7. **Limits:** complete supported-envelope table and direct answers for revisions,
   file size, workspace file count, directory dimensions and workspace size.
8. **Next actions:** smallest ordered fixes/missing proofs and later-stage ownership.

End with five plain answers matching the user's questions. State what the reviewed
source actually supports and what has been verified, without turning smoke evidence
into a speedup, integer ranges into qualified capacities, or Stage 3–4 acceptance
into a completed Workspace/runtime or full v0.1.7 release.
