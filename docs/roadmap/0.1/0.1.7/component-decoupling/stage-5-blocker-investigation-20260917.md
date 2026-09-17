# Stage 5 blocker investigation and recovery order

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Investigated source: `0979fbd5bcd46352f36dcc5f4a8ffd23786111328`.
Product source was clean and unchanged during the diagnostic. This is a focused
investigation of the handoff agent's stated blockers, not the complete Stages 1–5
review. [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) was OPEN when
read. No issue state, product implementation or historical receipt was changed.

## 1. Conclusion

The four missing external test matrices have no demonstrated infrastructure or
permission blocker. Session allocation explains a stop, not technical inability.
However, the claim that none of the missing work relates to incorrect product code
is false: public-API diagnostics reproduce ordering accounting and cleanup defects.
The reported resource-completion claim must be corrected before measurement.

The comparison problem is narrower than stated. The exact optimized Workspace
batch route has private, runtime-coupled orchestration, but public reference
filesystem operations and sorted primitives already exist. Component comparisons
are feasible without the old whole runtime. They cannot substitute for complete
reference-accounting, persistence, cleanup or end-to-end performance evidence.

## 2. Reproduced defects

Raw output: [probe.stdout](../evidence/stage-5-blocker-audit-20260917T055334Z/attempt-2/probe.stdout).
The [probe](../evidence/stage-5-blocker-audit-20260917T055334Z/attempt-2/probe.rs)
uses public APIs and an external backing implementation. There are no product hooks.

| Diagnostic | Actual observation | Consequence |
| --- | --- | --- |
| Append one 88-byte ordering run | File and run length are 88; `held_bytes()` and `peak_bytes()` both return 0 | Physical backing accounting is incorrect. |
| Build a three-inode filesystem with pending capacity 1 and a backing whose `release()` returns an error | Build returns success; 14 runs were created; release called zero times; 2,200 bytes still exist when the operation returns | Required checked cleanup is bypassed; a cleanup error cannot affect completion because it is not called. |
| Inspect the successful build's returned work counters | `rows_spilled=7`, but reported `runs_created=0`, `merges=0` and `peak_run_bytes=0` | Returned ordering statistics omit real work and cannot qualify resource/performance claims. |

The probe explicitly calls the failing cleanup afterwards as a control and confirms
that it returns an error; then it removes its own files through the inner backing.
Success of this diagnostic means the defects were reproduced, not product acceptance.
Synthetic content IDs isolate C1 tree construction; no real C2 save/read or storage
identity completeness is claimed by this probe.

Source paths explain the observations:

- [backing.rs](../../../../../core/crates/layerfs-content/src/filesystem/references/backing.rs#L53):
  `held`/`peak` start at zero; append updates only the individual run's `written`.
  No shared byte-budget owner or file-backing quota is enforced on that path.
- [reduce.rs finish](../../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs#L245):
  moves the final handle into `FinalRows` and passes `self.work`, losing the live
  `RunStore` counters that `work()` otherwise combines. It does not carry the
  backing's checked completion into the returned stream.
- [update.rs completion](../../../../../core/crates/layerfs-content/src/filesystem/update.rs#L280):
  consumes rows, emits a filesystem root and returns success without invoking
  the backing's checked release. `Drop` is not a substitute for a fallible finish.

There are additional source-visible bounds concerns for the missing tests to
investigate: touched-serial collection, declared-new sets, operation-sized maps/
vectors and old/new run coexistence. This investigation does not claim complete
coverage of those paths or equate internal scratch limits with total live memory.

## 3. The comparison claim

The following APIs are public in the pinned v0.1.6 source
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`:

- [directory_apply_sorted_observed](../../../../../crates/layerfs-content/src/tree/batch.rs#L1364)
  and [compact_inode_table_apply_sorted](../../../../../crates/layerfs-content/src/tree/batch.rs#L1153).
- [apply_changes](../../../../../crates/layerfs-content/src/filesystem/change.rs#L67),
  [rename](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L157),
  [hard_link](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L602)
  and [remove_path](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L653).

The current relevant reference source matches that pin. The claim that filesystem
updates exist only in private Workspace code is too broad. Public command-based
operations are not automatically equivalent to the optimized Workspace batch route:
normalize semantics and include the same work before comparing them.

The [fixture generator](../../../../../crates/layerfs-content/tests/stage5_reference_fixtures.rs#L327)
already composes public sorted primitives with a test MemoryStore. It supplies
expected final inode records/counts from its model. That is a valid canonical result
oracle, but does not time deriving reference counts or topology. Reusing it as a
benchmark must not credit preparation or compare it with the whole candidate update.

Use this comparison boundary:

```text
same checked base + same prepared sorted directory/inode changes
               |                                      |
       reference primitives                    candidate primitives
               |                                      |
       directory/inode/root work               directory/inode/root work
               +-------------- compare ---------------+

Excluded on BOTH arms, explicitly:
normalization, deriving final counts, full topology validation,
reference ordering preparation, physical persistence and publication
```

Do not just add a reference timer and compare it with `build_filesystem` or
`update_filesystem`: those candidate entry points perform additional obligations.
Use the corresponding candidate primitive boundary, equal provider/output work
and same input/output identities. Report any implementation-specific representation
differences rather than silently changing the comparator.

The real C1+C2 pipeline still needs its own successful acknowledgement/readback,
failure/cleanup, physical storage and simultaneous-memory evidence. These can be
measured directly without claiming a nonexistent matched end-to-end speed ratio.
An unmeasured mandatory comparison remains open unless explicitly rescoped; the
reviewer or implementation agent cannot turn NOT_RUN into acceptance.

## 4. The campaign is not fully frozen

Despite its status banner, [stage-5-verification.md](stage-5-verification.md#4-numerical-gates-correctness-first-then-resources)
explicitly says that performance gates are not frozen. Its listed checks cover
canonical correctness, selected scratch counts and composition; it does not freeze
matched numerical performance, total memory or storage gates.

Its warm in-memory fixtures are suitable only for a clearly scoped diagnostic or
an expressly allowed comparison contract. They cannot support a cold-storage or
whole-operation performance claim and cannot override the repository's no-cache-
credit rules. The fixture matrix also does not prescribe all the missing ordering
and failure schedules. Therefore "nothing is ambiguous now" is not supported.

Before qualification, append a prospective contract correction with exact paired
entry points, included/excluded work, provider/fixture/cache state, limits, failure
schedules and numerical gates. Use the existing benchmark rules and approved
thresholds; do not invent tolerance or silently relax required work. Keep original
smokes and reports intact as historical diagnostics. Correct the overbroad frozen
status explicitly instead of treating missing performance gates as already approved.

## 5. Recovery order

```text
ordering/failure regression cases
             |
fix checked completion + real byte ownership/quota + returned counters
             |
all four external matrices, including real C2 late failure
             |
freeze the precise component comparison contract
             |
matched component runs + real C1/C2 pipeline resource/storage proof
             |
independent Stage 5 and cumulative Stages 1–5 review
```

### A. Finish the correctness/resource work first

1. Turn the reproduced defects into permanent external regression tests whose
   passing assertions require correct counters and failure after failed cleanup.
2. Keep one explicit resource owner for ordering bytes/quota across live and
   merging runs; reserve before growth, release/account old runs and record actual
   simultaneous high water. Avoid a new generic resource framework.
3. Carry fallible cleanup through operation completion after run consumers have
   released their handles. Success requires successful checked cleanup. Preserve
   the original operation failure and cleanup information without retry, hidden
   Drop-only errors or deletion of successful stored versions.
4. Snapshot/propagate actual ordering work into final results; test counters against
   external backing observations instead of asserting values derived from themselves.
5. Complete the four matrices using existing public provider/consumer/backing seams:
   ordering boundaries/precedence/quotas; C1 read/output/corruption failures; full
   live-state and read-wave bounds; C2 private visibility/late error/old-root survival.

The existing external `FailingReader` and ordering traits already permit controlled
failures. C2's [visibility tests](../../../../../core/crates/layerfs-storage/tests/visibility.rs)
already demonstrate real owner contention and failed-save cleanup. Extend that
pattern to tree objects; no third-party patch or test-only product fault machinery
is required. For C1, distinguish emitted private objects from a successful returned
root and C2 publication: a late failure need not mean that zero objects were emitted.

### B. Then perform honest, scoped comparisons

Use separately compiled reference/candidate drivers over existing public primitives.
Keep normalization/reference derivation exclusions symmetric. Do not transplant
private Workspace algorithms into a newly invented reference implementation.
Freeze valid gates before collecting qualified candidate results; retain missing
full-operation comparisons as explicit gaps without blocking independent tests.

### C. Close only on actual criteria

Run the focused tests, affected core checks and independent review. #170 currently
requires qualified ordering failures/resources and matched successful performance/
resource/storage evidence. No existing Stage 3–4 waiver covers these Stage 5 gaps.
There is no approval needed to continue already-authorized tests and fixes. Any
proposal to waive/defer an original required comparison is a separate scope change,
not a reason to stop the reachable correctness work.

## 6. Diagnostic custody and limits

Evidence directory:
[stage-5-blocker-audit-20260917T055334Z](../evidence/stage-5-blocker-audit-20260917T055334Z/).
The first external client compile failed because its runner supplied the library
directory instead of the dependency directory to rustc. Those logs/source remain
unchanged. A separate `attempt-2/` corrects only that harness path and reproduces the
defects. This was a diagnostic harness correction, not a product retry or alternate
product route.

The locked core library build, external compile and probe ran under the shared
measurement lock; each succeeded in attempt 2. Retained identity and manifest files
bind the source commit, library/binary hashes, commands, logs and probe source.
The assertions intentionally confirm the current defects. No latency/storage
comparison, whole-core test rerun, product fix, commit or issue mutation was done.
