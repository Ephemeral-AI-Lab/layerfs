# Reviewer handoff: Stages 1–2 — construction, CAS and independent timing

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Review [#166](https://github.com/Ephemeral-AI-Lab/layerfs/issues/166) (Stages 0–1)
and [#167](https://github.com/Ephemeral-AI-Lab/layerfs/issues/167) (Stage 2) under
parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
Use this assignment after implementation, or produce an explicitly incomplete
review if implementation is still changing. This document is a prompt, not a
review result or proof that either issue is complete.

## Copy/paste assignment

You are the independent reviewer of LayerFS Stages 1–2 in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Review the real implementation and report four answers:

1. What is the resulting file/folder structure, and how did production LOC change?
2. Are all applicable Stage 0–2 acceptance criteria met, with evidence for each?
3. What can be deleted, merged or simplified without weakening the contract?
4. What measured statistics and source evidence establish speed, efficiency,
   bounded memory and memory safety? What remains unproven?

Review source, callers, tests and raw evidence independently. An implementation
report, a closed issue, a passing example or a design diagram is not acceptance
evidence by itself. Keep production source unchanged. Write the review and retain
its evidence; do not implement fixes, commit, push, close issues or post comments.
Preserve unrelated work and do not interrupt another owner's measurement.

### Read first and freeze the reviewed state

- Read [repository rules](../../../../../AGENTS.md),
  [core rules](../../../../../core/AGENTS.md), and the complete
  [Stages 0–2 implementation handoff](stages-0-2-handoff.md).
- Read the current #166/#167 bodies, implementation report if present, and the
  [implementation plan](implementation-plan.md). Reconcile any conflicting scope
  explicitly; do not silently choose the easiest requirement or rewrite it.
- Follow the detailed contracts linked by the handoff: [canonical objects](canonical-objects.md),
  [file content](file-content.md), [finalized output](finalized-object-handoff.md),
  [content I/O](content-io.md), [encoding/packing](physical-encoding-and-packing.md),
  [persistence](admission-and-persistence.md), [policy/tables](content-storage-policy-and-tables.md),
  [memory audit](content-io-memory-audit.md), and [timing](telemetry.md).
- Before measurement read [benchmark rules](../../../../general/benchmark_rules.md),
  [benchmark agent rules](../../../../../benchmark/AGENTS.md),
  [quick start](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
  [release policy](../../../../general/release-policy.md), and
  [documentation policy](../../../../general/documentation-policy.md).
- Record HEAD, branch, relevant tracked diff, staged changes, untracked product
  files, source/build/lockfile identities and the reviewed specification identity.
  Pin an immutable snapshot or a complete manifest of the inputs actually reviewed.
  A HEAD SHA alone does not identify an implementation with uncommitted files.
  Recheck relevant identities after checks; concurrent changes invalidate affected
  results. Keep changing work explicitly partial rather than issuing a moving PASS.
- Identify the actual pre-implementation commit for LOC comparison. The handoff's
  v0.1.6 reference SHA is an algorithm/comparison reference, not automatically the
  correct LOC parent. Record any reviewer snapshot separately from committed trees.

## 1. Report actual structure and production LOC

Produce an ASCII tree of the resulting C1/C2 implementation, including runtime SQL,
external tests/examples, and changes to telemetry, manifests and supporting tools.
Annotate product files with production LOC and physical lines. Mark other files
as non-production; do not mix their sizes into the production total.

For every product file and every recursive directory, report:

```text
path | before production LOC | after production LOC | signed delta
     | recommended range | below/within/above | physical lines | explanation
```

Use the per-file and directory recommendations in the implementation handoff.
Include added/deleted/renamed/merged files and explain deviations from its starting
map. Directories include descendants; do not sum parent and child totals twice.
Show disjoint subtotals for C1, C2 including SQL, telemetry, remaining replacement
code, legacy reference, application adapters if present, and the combined product.
Do not present retained reference code as deleted or duplication as simplification.

Use one reproducible counter/version against both exact snapshots: nonblank,
non-comment first-party production source, including declarations and shipped SQL,
excluding tests, examples, benchmarks, tooling, docs and generated/third-party code.
Inspect `tools/production_loc.py` classification and tests before trusting output:
the original tool omitted core runtime SQL, and legacy inline-test/comment handling
needs validation. If unresolved, mark its totals incomplete; a transparent external
counting correction is allowed if retained and applied identically to both trees.
Do not modify product source to improve the count. Never substitute Git diff stats.

Audit each implementation commit's first-parent comparison and reported LOC. For
uncommitted work, label the separate before-to-reviewed-snapshot comparison honestly;
do not invent commit totals or stage another agent's files. Report the counting
command, counter identity, scope/exclusions and all discrepancies.

Hard structural checks use physical lines: production files <=999; lib.rs/mod.rs
<=200 and declarations/reexports/direct delegation only. Review responsibility as
well as size: moving a god object into another filename does not pass. Recommended
LOC ranges are guidance, not acceptance caps; below-range code is not proof of quality.

## 2. Answer every acceptance criterion

Build a traceability table from every acceptance item in #166/#167 and the handoff,
including Stage 0 prerequisites. The following groups are the minimum, not a
replacement for reading those criteria:

| Group | Review obligations |
| --- | --- |
| Isolation | C1 imports no C2/SQLite; C2 saves supplied objects without file construction. No Workspace, FUSE, daemon, history, checkpoint or legacy runtime dependency. Trace real public callers and dependency/feature graphs. |
| Canonical construction | Frozen identity/framing/CDC and roots; empty and small files; chunked files; T-1/T/T+1; known/unknown lengths; malformed/overflow/trailing data; exact reads and cross-extent ranges. |
| Configuration | Typed visible policy, correct defaults and explicitly supported ranges. Reject unsupported settings. Do not silently clamp a configurable cutoff or treat declared but unused fields as implemented features. |
| Finalized output | Owned immutable output, child-before-parent references, bounded source requests, release on consumption/error, backpressure across many files, and separate construction/storage completion. |
| Real CAS | Exact existing and in-batch reuse; required authentication/collision checks; missing/corrupt object errors; supplied-object save/read; close/reopen readback. |
| Physical FULL path | Actual supported FULL compression/decoding, checked framing, stable pack/group/record locators, append/new placement before one selected assembly, grouped/range reads. |
| SQLite | Correct four-table/19-column profile and indexes; schema/profile rejection; bound query cardinality; lazy bounded transactions shared across files; one save owner; final acknowledgement after required writes. |
| Visibility/failure | Private early output stays owner-bound; retained-pack ceiling applies through reads/dependencies/caches; definite abort and one owned-cleanup attempt; retained data survives failure; unknown outcome fails without replay or guessed deletion. |
| Memory | Full allocation/lifetime accounting, many-file bounds, actual supported incompressible canonical singleton at C2, bounded reads, failure-path release, no hidden payload spool/scratch. |
| Timing | Real C1-only, supplied-object C2-only save/read, and integrated execution through identical production functions; enabled/disabled results and failures match; caller-owned report output. |
| Product discipline | External tests, no test-only product hooks or benchmark branches, module/LOC rules, locked dependencies, no patched/forked/vendored third-party code. |
| Single attempt/no sync | No automatic retry/busy handler/error-driven fallback; MEMORY journal, synchronous OFF, zero busy timeout; no WAL, fsync/fdatasync/sync_all/sync_data or added crash-durability path, including timer output. |
| Delivery | Required actual tests/examples/checks, precise supported profile/capacities, LOC comparison, evidence and explicit limitations. No empty or zero-test command presented as passing coverage. |

Each row must include the exact criterion/source, code location, test or retained
evidence, status, and concrete gap. Use PASS / FAIL / INCOMPLETE / NOT_RUN;
NOT_APPLICABLE requires a cited scope exclusion. Distinguish a demonstrated defect
from missing evidence. A required incomplete or unrun criterion blocks a full PASS.

Stage 3 delta chains/full pooling policy, Stage 4 arbitrary localized edits and size
transitions, Stage 5 filesystem trees, Stage 6 full qualification and Stage 7 runtime
integration are deferred. Do not fail this slice merely for their absence, or claim
their benefits. Required Stage 2 FULL packing/compression remains in scope. A large
CDC file is not evidence for the distinct C2 large-canonical-singleton obligation.

Run the commands and three real smoke modes in handoff section 8 against the pinned
state, or reuse only identity-matched evidence where policy permits. Record command,
exit status, test count and raw output. Verify test targets actually exist and run;
Python discovery can return success with zero tests. The explicit core checks are
mandatory. The later owner rule permanently retires tools/preflight.sh: do not run
or restore it or add an aggregate replacement. Report the individual applicable
workspace checks without creating a push. Do not repeatedly rerun unchanged passes.

## 3. Find concrete simplifications

Trace both directions and the failure path, not just the public API:

```text
input -> C1 construct -> finalized bytes -> C2 batch -> FULL/pack -> SQLite
reader <- logical ranges <- authenticated objects <- locators/groups <- SQLite
failure -> abort / known-owned cleanup -> one terminal result
```

Look for redundant validation/re-hashing after a trusted immutable boundary,
duplicate payload copies, whole-input collectors, point-query loops behind a batch
API, per-file flushes/transactions, repeated pack assembly or whole-pack rewriting,
reopening/repreparing in hot loops, unused compatibility paths, redundant owners,
one-use wrappers and exported internal types. Check whether caches avoid real work
and have scoped bounds/invalidation. Inspect every caller before proposing removal.

For each finding give location, present behavior/cost, smallest deletion or change,
invariant preserved, affected callers, expected LOC/work reduction, and verification
needed. Label estimated savings as estimates. Do not remove trust-boundary checks,
exact CAS authentication, backpressure, atomicity or cleanup to make code shorter.
Do not introduce registries/factories/interfaces for speculative future deployments.
Existing narrow I/O seams with real independent users are not automatically wasteful.
Do not implement suggested changes during this review. If nothing material remains,
say so; no quota of simplification findings is required.

## 4. Evaluate speed, efficiency, bounded memory and memory safety

### Evidence levels and measurement discipline

Separate measured results, source-derived bounds and unproven expectations. Stage 2
smoke timings establish wiring only; they do not qualify the engine or demonstrate
a v0.1.6 speedup. Reuse qualified evidence with exact matching identities. If no
registered component campaign exists, report the performance question as incomplete
and propose the smallest missing measurement; do not invent a benchmark framework.
Any allowed diagnostic remains diagnostic with admission_eligible=false.

Use the repository's one sample per case/arm unless an applicable owner-approved
campaign says otherwise. With n=1 report one observation, not statistical confidence,
p95 or a claimed distribution. No best-of reruns, changed workloads or relaxed gates.
Preserve failures and fresh evidence directories. Respect the measurement lock,
single construction worker, cache policy, preparation reuse and lifecycle budgets.
Do not launch builds or other resource-sensitive work alongside a measurement.

Compare v0.1.6 only when operation, workload, public surface, acknowledgement,
workers, cache state and harness semantics match. A legacy full Workspace pipeline
cannot be compared with candidate C1-only construction. No matching surface means
separate non-comparative rows, not a speedup ratio. Keep source checks of removed
round trips distinct from observed round-trip counts. No Git/FUSE/cloud claim here.

### Statistics to request from applicable existing evidence

| Question | Metrics and interpretation |
| --- | --- |
| Construction cost | Input bytes, constructed object/chunk counts, construction elapsed ns, and CPU time only when independently observed. Throughput names its byte basis. |
| Storage/read cost | CAS save-to-ack elapsed ns, authenticated read elapsed ns, integrated elapsed ns, complete command wall, inserted/reused counts and bytes. Include required membership/authentication/SQL work. |
| Round trips/batching | Public calls, SQL statements/steps and transactions, actual remote round trips only if a remote backend was exercised, objects/bytes per batch and transaction. Prepared statement reuse is not proof of one SQL execution. |
| Amplification | Source bytes read/scanned, canonical bytes copied, encoded bytes, pack bytes rewritten/read, SQL/BLOB bytes where observable, final Store bytes. State each ratio's numerator/denominator; DB file growth is not a physical-write counter. |
| Storage efficiency | Unique/duplicate inputs, inserted/reused objects, FULL compression size, occupied pack bytes, DB allocation/freelist where available. Empty-DB overhead is separate. No claim of later delta savings. |
| Memory | Process baseline/phase peak/final RSS, heap/live allocation if externally observable, applicable SQLite/codec costs, separate cgroup domains, temporary payload disk peak and Store disk. State measurement coverage and unavailable values. |
| Timing overhead | Matched enabled/disabled operations with identical work, results and cache contract; absolute and relative elapsed overhead, retained node count, truncation status and JSON size. No negligible-overhead claim from an unpaired tiny run. |

Every statistic needs unit, scope, source identity, provenance, sample count and raw
evidence. Unavailable is null plus reason, never zero. Normal reopen does not make a
read cold; same-process readback can verify correctness without proving cold latency.
Do not label process-lifetime high-water as phase peak. Sampled peaks need coverage
and interval/gap disclosure; cgroup fields are not applicable on an uncontained run.

Choose applicable cases from the existing frozen contract: empty/tiny/boundary files,
compressible and incompressible inputs, multiple real file sizes, many small files
in one save, all-new and duplicate-heavy saves, append/new pack boundaries, a direct
large canonical singleton, reopen/range/repeated reads, and late failure/cleanup.
Record exact sizes, counts, seeds, capacities and exclusions before collection.
No giant sparse/repeated logical fixture passed off as physically measured input.

### Inspect the actual timer attribution

Show one actual report tree for each real mode, with artifact links and exclusions:

```text
C1-only:     stable input -> constructor -> bounded nonpersisting consumer
C2-only:     supplied canonical objects -> save/ack ; authenticated read
Integrated:  input -> construct -> handoff/backpressure -> save/ack -> readback
```

Verify the timer encloses the work its label names. A synchronous construction timer
may include sink/storage calls; name that inclusion rather than calling it pure C1.
Do not derive CPU or exclusive time by subtracting overlapping elapsed spans. Confirm
consumer waiting is attributable where claimed, construction completion differs from
all-output persistence, and disabled timing preserves the same operation/errors.
Reports must keep coarse operation timing complete without retaining a node per
object indefinitely. Audit 1,024-node/32-level/128-byte-label limits and clipping;
incomplete detail is not a zero-duration step or a complete breakdown.

### Give two separate memory conclusions

**Bounded resource use:** make a compact allocation ledger with owner, purpose,
configured byte/count bound, maximum live multiplicity, lifetime, release event and
proof. Cover source/chunk buffers, mapping frontier, finalized/pending objects,
canonical+encoded+pack overlap, decoded reads, caches/indexes, SQLite bindings/BLOB
copies/page cache/MEMORY journal, codec workspace, concurrent Stores and reports.
Count capacities and transient overlap, not only logical buffer lengths. State the
actual bound's parameters, including mapping depth and concurrency; do not call an
O(log file size) frontier constant. Show why slow storage, many files or an error
cannot accumulate one buffer per object/file. SQLite cache_size is not a total cap.

**Memory safety:** inspect unsafe/FFI, allocation arithmetic, codec input/output
lengths, pointer lifetimes/aliasing, malformed pack offsets, decompression bounds,
ownership across handoff and use after failure/cleanup. Cite targeted public-input
tests and existing compatible external tooling when available. Safe Rust alone does
not prove dependency/FFI safety or a memory budget; low RSS alone proves neither.
An unavailable sanitizer/Miri result is a coverage limit, not a passing run. Do not
patch dependencies or add test-only product hooks to make instrumentation work.

## 5. Required report and final verdict

Save a new `stages-1-2-review-<UTC timestamp>.md` beside this prompt, with a proposal
status banner and an explicit dated review identity. Retain command output, count
tables and available raw receipts in a fresh directory under
`docs/roadmap/0.1/0.1.7/evidence/stages-1-2-review-<UTC timestamp>/`.
Keep earlier evidence untouched. Link the raw evidence; a short report may put large
per-file and criterion tables in adjacent CSV/Markdown artifacts.

Use these sections in this order:

1. **Verdict:** Stage 1 PASS/FAIL/INCOMPLETE; Stage 2 PASS/FAIL/INCOMPLETE; blocking
   criteria and whether either issue is ready to close. Do not actually close it.
2. **Findings:** severity, source location, concrete trigger/impact, supporting
   evidence and smallest recommended fix. Put correctness/data-loss defects first.
3. **Structure and LOC:** actual ASCII tree, per-file/directory changes, estimates
   versus actuals, hard-limit violations and per-commit accounting.
4. **Criteria:** complete traceability table; test/check outcomes and coverage gaps.
5. **Simplification:** ranked cuts and a small before/after ASCII diagram for each
   material change; estimated benefit and invariant that must remain true.
6. **Statistics:** C1/C2/integrated timing trees and the applicable metrics above;
   measured/source-derived/unavailable labels, reproducible commands and limits.
7. **Memory:** allocation ledger, boundedness verdict and separate safety verdict.
8. **Next actions:** smallest ordered fixes or missing proofs, and explicit Stage 3+
   deferrals. Distinguish unmet first-slice requirements from later qualification.

End by answering plainly: Are all Stage 1 criteria met? All Stage 2 criteria? What
can be simplified now? What do we actually know about speed and memory, at which
sizes/configurations? What cannot yet be claimed? Never turn missing required
evidence into a PASS, or turn a Stage 1–2 pass into full v0.1.7 qualification.
