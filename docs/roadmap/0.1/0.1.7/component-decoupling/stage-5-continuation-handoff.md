# Continue Stage 5 to verified completion

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Execution prompt for [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170),
under [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
This prompt supplies the recovery order and completion conditions. The
[original handoff](stage-5-handoff.md), [file/LOC plan](stage-5-file-plan.md) and
current owner instructions retain their full acceptance scope.

## Copy/paste assignment

Continue the existing Stage 5 implementation in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Your assignment is to **finish Stage 5**, including required tests, resource and
failure behavior, qualification, final report and issue closure. Implement and
verify the missing work; do not stop after explaining it or offering to begin.
Keep working until every applicable completion gate below is satisfied, or until
a demonstrated external blocker prevents further progress after all independent
work is complete. Never manufacture a PASS to satisfy this instruction.

Do not restart Stages 1–4, rebuild the whole architecture, implement Stage 7, or
produce another design-only handoff. Preserve the existing working algorithms and
complete their missing obligations. Use the smallest fixes at the actual shared
ownership boundaries. New abstractions need a concrete product responsibility.

## 1. Establish the actual starting state

Read repository AGENTS.md, core/AGENTS.md, this prompt, the original Stage 5 handoff,
file plan, filesystem design, current #170 body, Stage 5 report/verification files,
and the [blocker investigation](stage-5-blocker-investigation-20260917.md).
Follow their detailed C1/C2, timing and benchmark contracts.

The investigation tested `0979fbd5bcd46352f36dcc5f4a8ffd23786111328`. Check actual
HEAD and working-tree changes; do not reset to that commit or overwrite concurrent
work. Preserve the local investigation and raw evidence if they remain uncommitted.
Do not indiscriminately stage unrelated files. Record the starting source identity
and bind every new check to the source it actually exercises.

Read complete relevant functions and callers before editing. The source map below
is a starting point, not a restriction against fixing a proven shared root cause:

```text
core/crates/layerfs-content/
  src/filesystem/
    input.rs / update.rs / objects.rs
    references/backing.rs / runs.rs / merge.rs / reduce.rs / release.rs
    sorted/ / validate.rs / inode/read.rs / directory/read.rs
  tests/
    filesystem_ordering.rs              create
    filesystem_failure.rs               create
    filesystem_bounds.rs                create
    support/filesystem.rs               reuse and extend only as needed
  examples/filesystem_timing_c1.rs       preserve real production route

core/crates/layerfs-storage/
  src/cas/                              change only for demonstrated integration defects
  tests/filesystem_failure.rs            create
  tests/filesystem_pipeline.rs           extend real composition coverage
  tests/visibility.rs                    reuse existing failure/visibility patterns
  examples/measure_filesystem.rs          preserve independent and pipeline modes

crates/layerfs-content/
  tests/stage5_reference_fixtures.rs      existing reference oracle, not candidate code
  examples/stage5_component_reference.rs optional concrete comparison driver
```

Tests/helpers/examples remain outside production. Do not expose private product
code or add product fault hooks solely to make a test possible. Reuse existing
public read, consumer, backing and real SQLite capabilities.

## 2. Checkpoint A: regressions, then fixes for the confirmed defects

The retained [diagnostic output](../evidence/stage-5-blocker-audit-20260917T055334Z/attempt-2/probe.stdout)
establishes:

| Defect | Observed before fix | Required after fix |
| --- | --- | --- |
| Backing accounting | An 88-byte run reports held=0, peak=0 | Real held/peak bytes agree with owned resources and release; explicit quota enforced before growth. |
| Checked completion | A three-inode build creates 14 runs, returns success, never calls a deliberately failing release, and leaves 2,200 bytes at return | Required cleanup is attempted and checked before successful operation completion; failure cannot produce successful completion. |
| Final work counters | Returned result reports zero runs/merges/peak run bytes despite actual spills and runs | Actual work, including consolidation/final-stream work, reaches the returned result and agrees with external observations. |

**The diagnostic asserts the defects. Its passing exit is not a product PASS.**
Create permanent external regression tests that assert the required corrected
behavior and fail on the investigated source. Preserve the original probe and logs.
Use a fresh evidence path for any replay; do not overwrite them or repeatedly run
an unchanged passing check without a reason.

Implement these ownership rules:

1. One explicit owner accounts ordering bytes across pending state, runs and
   simultaneous old/new merge output. Reserve before growth; handle checked
   arithmetic and partial I/O honestly. Count resources until they are released.
2. Release obsolete runs when they are no longer needed, retaining failure
   correctness. Do not count only the final run while old files remain owned.
3. After row consumers finish, close/release their handles and perform checked
   backing completion before returning success. The lifetime must not depend on
   reading an already-unlinked file descriptor or one OS's deletion semantics.
4. On failure, preserve the original operation error and any cleanup failure;
   attempt known-owned cleanup according to the single-attempt contract. No
   automatic retry, silent Drop-only failure or removal of successful stored data.
5. Propagate actual counters from their owners at the right time. A zero field
   cannot mean uncollected work. Snapshot counters before destruction as needed.

```text
construct / order / consume final rows
                 |
release readers and run handles
                 |
checked backing cleanup ---- error ---> failed operation / no successful result
                 |
C1 construction completed
                 |
C2 finish / acknowledgement
                 |
all output persisted
```

C1 may have emitted private finalized objects before a late error. Do not try to
guarantee zero emission by collecting the whole operation. Require no successful
result/publication after failure and let the composed C2 owner abort known-private
output without affecting earlier successful versions. Drop may provide best-effort
cancellation cleanup; it cannot certify successful fallible completion.

Checkpoint A is complete only when corrected external assertions pass and the
real filesystem build/update entry points use the fixed path.

## 3. Checkpoint B: finish all four missing test matrices

Implement the following behavior coverage, not merely four files with smoke tests:

| Target | Required cases and assertions |
| --- | --- |
| C1 filesystem_ordering | Golden/truncated/invalid record framing; actual pending threshold crossings; several tier carries; newest pending/run precedence, tombstones and accumulated counts; old/new run overlap; byte/quota boundary and first invalid growth; append/read/flush/release failures; success and error cleanup; counters cross-checked against backing observations. |
| C1 filesystem_failure | Existing external FailingReader at several meaningful read boundaries; refused consumer; malformed demanded canonical page; invalid/late input where the actual API streams; no second operation attempt; no successful root after failed required work; previous roots intact; failure and cancellation resource ownership. Eager slice validation is tested before output, not with an invented stream API. |
| C1 filesystem_bounds | Empty/wide/deep cases; decoded-page/provider-batch/output overlap; ordering buffers/run multiplicity; changed-path versus unrelated-tree reads; subtree removal with moved-out/externally linked descendants; bounded read waves; actual live-owner and release evidence. Include operation input and auxiliary collections, not only sorted scratch. |
| C2 filesystem_failure | Save a real old tree first; force private early writes in a new save; cause a tree-operation/read/consumer/SQL/owner failure through existing public seams or external fixture corruption; assert failure, unchanged publication watermark, cleanup restricted to this attempt, old tree readable, no retry and no false acknowledgement. Exercise actual new tree roles and pooling. |

Reuse existing visibility/owner contention patterns. Corrupt only disposable owned
fixtures; do not modify a dependency or live user Store. Supply actual content and
attribute objects for C2 tests; synthetic unbacked IDs establish only isolated C1
grammar behavior and cannot prove real persistence.

For resource cases, count input slices/vectors, topology and declared-new sets,
touched-serial collection, retained maps, decoded bytes, merge readers, codec/pool/
pack/SQL state and report buffers while they coexist. Eliminate unnecessary copies
and collectors. A required collection needs an enforced bound and explicit owner;
a smaller scratch counter does not hide it. Do not impose an arbitrary tiny file/
tree limit to make the tests pass. Preserve required algorithmic/localized behavior.

These matrices may uncover more defects. Fix them and rerun the affected cases.
Do not label a failed bound as merely missing evidence or defer it to Stage 6.

## 4. Checkpoint C: correct and freeze the qualification contract

The existing verification file calls itself frozen while explicitly leaving
performance gates unfrozen. Correct that status and add a dated prospective
addendum. Preserve previous results and explain exactly which new rows it governs.

Before benchmark implementation/qualification collection, record and commit the
applicable contract per repository rules: exact public entry points on both arms,
normalized inputs, physically realized sizes, provider/output work, cache contract,
worker policy, timing/acknowledgement boundaries, source/fixture/harness identities,
metric units, resource/storage bounds, numerical gates and failure schedules.

Use the existing owner requirement of existing-or-better performance and the
applicable approved numerical rules. Missing implementation detail is yours to
resolve prospectively; it is not permission to invent a tolerance or waive a gate.
Where baseline measurements are required to derive limits, collect that baseline
and freeze the limits before candidate optimization/qualification. Earlier smoke
results are not a campaign baseline. Any relaxed acceptance requires an explicit
owner decision; meanwhile complete independent work.

Read the actual issue and governing handoff before declaring a comparison
mandatory, inapplicable or satisfied. The original handoff already distinguishes
component comparison from complete-operation comparison and allows clearly scoped
non-comparative evidence where no equivalent public surface exists. Apply that
rule with source evidence; it does not excuse an unbuilt reachable comparator or
convert a required NOT_RUN row into PASS.

## 5. Checkpoint D: execute valid comparisons and real pipeline proof

### D1. Matched component work

Public v0.1.6 sorted directory/inode APIs and root encoding exist. The fixture
generator already uses them. Build a small driver using those public APIs; keep
reference and candidate executables/dependencies separate. Never copy a private
Workspace implementation into a newly invented reference algorithm.

```text
same checked base + same prepared sorted directory/inode changes
                    |                          |
            reference primitives       candidate primitives
                    |                          |
               matching work / outputs / measurement boundary
```

Exclude normalization, final-count derivation, topology, ordering preparation and
persistence from BOTH arms if this is the selected component boundary. Do not
compare reference primitives against the full candidate update_filesystem call.
Use corresponding candidate public primitives with equal input/read/output work.
Verify roots/partitions/results independently before accepting a speed comparison.

The existing reference fixture model supplies final counts. Do not time those
supplied counts against the candidate deriving them. An in-memory primitive smoke
is not automatically an eligible storage or complete-operation measurement.

### D2. Actual C1/C2 composition

Exercise the real complete native operation and C2 acknowledgement/reopen/readback.
Collect independent C1, supplied-object C2 and integrated timing; actual final
pack/SQLite footprint; ordering I/O; simultaneous memory and cleanup evidence.
Ensure --case/--mode executes what the receipt says. Include necessary validation,
ordering, provider acquisition, output waits and finish work in the stated scope.

For matched complete operations inspect public reference apply_changes/rename/
hard_link/remove_path and initialization routes first. They are not automatically
equivalent to the private optimized Workspace batch path. Compare only equivalent
successful semantics and included work; account adapters/normalization honestly.
Do not rebuild the entire old Workspace runtime solely to time a directory primitive.
If an applicable complete-operation criterion truly needs that runtime, implement
the narrowly required reference driver or identify the exact unsatisfied criterion;
do not hide it beneath D1's component result.

Where an existing contractual non-comparative alternative applies, document the
source-backed absence of an equivalent surface, give the required structural/
correctness/resource evidence and explicitly bound the claim. No global speedup
claim follows. A genuinely mandatory missing comparator remains an open gate.

### Measurement rules remain unchanged

- One sample per case/arm unless a specific later owner approval applies. No n3
  default, best-of selection or unchanged reruns to obtain a PASS.
- Preserve the shared measurement lock; no concurrent sensitive builds/runs.
  Reuse prepared fixtures/build seals outside timers, never measured work.
- Enforce equal declared cache state and the repository no-warm-credit contract.
  No cold/storage claim from the warm-process fixture smoke.
- One mutation construction producer; preserve initialization's permitted parallel
  exception. Do not add workers or shrink workloads to pass.
- Fresh append-only receipts, separate verification, all failures retained.
  Ordinary full command <=15 s, declared exceptions <=25 s, verifier <=60 s under
  the applicable rules. No timeout inflation or hiding lifecycle/cleanup work.
- Counters need provenance and actual nonzero work where exercised. Distinguish
  source-derived capacity, measured live allocations, RSS and file-cache scope.
  Report unavailable metrics; never substitute zero or a lifetime peak.
- If a valid gate fails, fix its demonstrated cause and rerun the affected matched
  scope under the unchanged gate. Product no-retry does not forbid an honest
  development rerun after a relevant source fix; evidence must show the change.

## 6. Checkpoint E: verification, report and final review

First run the new focused targets, fixing failures before broad checks:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_ordering
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_failure --test filesystem_bounds
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test filesystem_failure --test filesystem_pipeline
```

When stable, run individually:

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

Keep previous canonical, hardlink, topology, attribute, edit, transition, pooling,
visibility and timing tests active. Execute real independent/pipeline examples with
fresh output paths, observing their current CLI. fmt has no --locked flag; report
any governing formatter-pin discrepancy rather than changing unrelated formatting.
No aggregate preflight, CI workflow or substitute gate.

Use the [Stage 5 and complete Stages 1–5 review checklist](stages-1-5-reviewer-handoff.md)
for the final acceptance pass. Fix any new Stage 5 or shared-path regressions it
finds. Respect closed Stages 1–4 dispositions; do not turn earlier owner-waived
measurements into proof or use them to waive Stage 5. Separate an implementation
agent's self-review from a review performed by an independent reviewer.

Create a new dated completion report and append dated corrections to stale Stage 5
report claims. Retain historical evidence unchanged. Report:

- Actual file tree and per-file/directory production LOC before/after/signed delta,
  estimates versus actual, physical caps and exact per-commit accounting.
- Each original #170 criterion and every checkpoint here, with code/tests/raw
  evidence, source identity and separate correctness/resource/performance statuses.
- Counter corrections, full memory/disk ownership, actual statistics and limits;
  no claim that ordered working-state bounds establish whole-operation bounds.
- Portable/generic attributes, environment-neutral capabilities, and required
  future adapter work; no Apple/APFS or Workspace/runtime implementation added.
- All failed attempts, current dispositions, exact commands and remaining concerns.

## 7. Completion gates and stopping rules

| Gate | Completion requirement |
| --- | --- |
| G1 | The three confirmed defects have corrected external regressions and real-path fixes. |
| G2 | Ordering grammar, precedence, tier boundaries, byte/quota accounting and checked cleanup are verified. |
| G3 | Both C1 and C2 failure matrices pass; earlier successful roots remain intact. |
| G4 | Whole-operation resource ownership/bounds and localized work are established, including backing/provider overlap. |
| G5 | Canonical/profile/schema, hardlink/topology/attributes and existing Stages 1–4 integration regressions pass on the final source. |
| G6 | Independent C1/C2/integrated timers use real bodies, honest scopes, bounded reports and unchanged on/off behavior. |
| G7 | Applicable matched performance/resource/storage gates have valid evidence; every permitted non-comparative alternative is justified under the existing contract, not invented. |
| G8 | Required core checks, final review, reports, exact LOC accounting and evidence identities are complete. |

Do not stop because an implementation compiles, parity passes, most tests pass,
three targets are done, a primitive benchmark works, a comparison needs its driver,
or the remaining work has low value per line. These are intermediate states.
Do not ask "say the word and I will continue" for work already assigned here.

A real blocker requires: exact failing command/input or unavailable capability,
source/evidence, attempted legitimate approaches, the precise acceptance item it
prevents and the minimum external change needed. Do not call an unimplemented test,
unwritten comparator, large work item or unknown code path an external blocker.
Continue every independent checkpoint while one dependency is blocked. If a hard
execution interruption occurs, preserve an exact continuation (source, passing/
failing gates, next edit/command); do not call the stage complete.

Close #170 only after every applicable original criterion and G1–G8 has qualifying
evidence and no unwaived blocker remains; update the issue/report with concrete
evidence and limitations. No unilateral waiver, gate relaxation, NOT_RUN-to-PASS
promotion or deferral of required Stage 5 work to Stage 6. Keep parent #165 and
Stage 6/7 issues open. No release/tag or legacy retirement is part of this task.

Commit coherent completed work with the required exact first-parent production LOC
comparison and migration subtotals. Follow existing branch/publication authority;
do not stage unrelated work or infer permission for unrelated publication.
