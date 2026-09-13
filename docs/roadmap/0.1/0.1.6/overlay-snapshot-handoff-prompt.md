# Snapshot-Isolated Workspace implementation and verification handoff

Implement and verify Snapshot-Isolated Workspace in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs` until both of these issues have reached
verified terminal success and are closed:

- [#124: implementation](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124)
- [#125: full existing benchmarks excluding #122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125)

This is an execution instruction, not a request for another plan or specification.
Continue through implementation, debugging, correctness, the full included benchmark
campaign, evidence publication, and verified issue closure. A draft, partial feature,
single passing family, or promises to continue are not completion.

The owner's latest completion requirement is stronger than the earlier statement
that benchmark reporting could finish with failed results: **do not close #124 or
#125 merely because failures were honestly documented. Resolve required failures
and obtain terminal success under the existing applicable contracts.** Reporting-only
targets remain reporting-only; do not invent new numerical gates to interpret this
requirement. Preserve failed attempts even after their causes are fixed.

You are authorized to make the scoped implementation/test/documentation changes,
commit and publish reviewable source through the repository's normal workflow,
post progress/evidence comments to #124 and #125, and close those two issues only
when the terminal conditions below are actually met. Do not publish a release,
tag, deploy a website, close #123, or execute/claim completion of #122.

## Read the correct source of truth

Read current #124/#125 bodies and applicable AGENTS.md instructions, then:

1. `docs/roadmap/0.1/0.1.6/overlay-snapshot-rule.md`
2. `docs/roadmap/0.1/0.1.6/overlay-snapshot-spec.md`
3. `docs/roadmap/0.1/0.1.6/overlay-snapshot-spec-review.md`
4. `docs/roadmap/0.1/0.1.6/overlay-snapshot-architecture-design.md`
5. `docs/roadmap/0.1/0.1.6/overlay-snapshot-implementation-plan.md`
6. `docs/roadmap/0.1/0.1.6/benchmark-exclusions-issue122.json`
7. `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md`, and
   `benchmark/fs-bench-pro/QUICKSTART.md` before benchmark work.

The existing generic `docs/roadmap/0.1/0.1.6/handoff-prompt.md` targets #122.
Do not execute that handoff. Read `cases.json` as the source of #122 exclusions,
not as this campaign's include list. References to #122 infrastructure do not
authorize running its scenarios.

Inspect actual current source and git status. These snapshot documents may still
be uncommitted in the main checkout: preserve them before using a different checkout.
Do not start from an older clean worktree and lose the specification. Preserve
unrelated changes, other agents' work, existing fixtures, and raw evidence. Freeze
the authoritative scoped documents in source as required before implementation/
benchmark work, and attach immutable links to the issues.

## Required product outcome

Implement the specified current mutable overlay with host private backing and
owned temporary Commit snapshots. Commit reads its snapshot independently, feeds
the existing shared Init/Commit construction machinery, admits canonical objects,
retains the exact candidate in `workspace_stages`, and conditionally publishes
branch history.

Keep live commands, mount identity, inode identity, open descriptors and supported
filesystem/SDK semantics intact. Ordinary operations must complete while Commit
builds/stages/publishes. Do not use a global freeze/drain, writer-finish requirement,
remount, or mandatory checkpoint installation/reset as a shortcut.

Preserve CAS/authentication, FULL/DELTA policies, large-file CDC/extents, packing/
compression, localized range edits and namespace updates, bounded journals and
admission, and existing branch leases. No second encoding pipeline, copied snapshot
tree, per-update Workspace checkpoint, or unbounded operation history.

Follow the spec's add/remove/replace inventory. Remove obsolete ordinary-Commit
dependencies when their replacements are correct; do not leave a hidden legacy
freeze behind a second lock or fallback. Audit non-Commit fsync/cache/SDK/
reconciliation/End/Discard consumers before deleting shared helpers.

Resolve the real V1-V4 obligations. Host-readable acknowledged buffers do not
automatically solve dirty mmap visibility. Root installation must compare its leased
source root, preserve unrelated writes, and keep backing owned. C2 must reuse the
last canonical predecessor without rewriting live content. Unknown publication,
including UpToDate, needs an authoritative resolution contract.

If a requirement appears incompatible with the supported platform, investigate
the actual source/kernel contract and test the smallest relevant hypothesis. Do
not silently weaken semantics, remove mmap, require implicit user fsync, relabel
kernel-visible data, or fabricate a passing result. Record a precise blocker if
external input is truly required; keep the issues open and continue independent
authorized work. Exhausted assumptions are not evidence of terminal success.

## Execute the seven phases and update issues on every completion

Follow the implementation plan's phases:

1. Resolve contracts and freeze the source of truth.
2. Implement host overlay backing and atomic live state.
3. Integrate FUSE/SDK operations and owned snapshots.
4. Separate Commit attempts and reuse staged construction/publication.
5. Remove obsolete Commit coupling and finish safe cleanup.
6. Complete correctness/integration and seal the benchmark candidate.
7. **Final phase: run the full existing benchmark suite excluding #122, repair
   measured failures as necessary, publish numbers, and complete terminal validation.**

After each phase actually completes, post a completion comment to #124 before
claiming or proceeding on that completion. Mirror benchmark-facing changes and
the phase-6 handoff to #125. Phase 7's complete evidence goes to #125 and is linked
back to #124. If a completed phase is invalidated by a later defect, post that
correction and update the local phase state; do not leave a false completed status.

Use this concise phase-comment structure:

```text
Phase N complete: <phase name>

Changes: <what is now implemented; what was removed/replaced>
Source: <exact commit/PR and binary/image identity where relevant>
Verification: <test/case IDs, results, commands and immutable evidence>
Reused passes: <unchanged tests retained from ledger, with justification>
Failures resolved: <original evidence, root cause and targeted successful rerun>
Remaining: <actual unresolved obligations and next phase>
```

Do not post "complete" while exit evidence is missing. Update issue checklists
truthfully. Keep user-facing progress concise during work and issue comments
substantive; do not spam unchanged polling results. The phase-completion comments
are mandatory, not replaced by local notes or a final summary alone.

The benchmark handoff requires a correct sealed candidate, not prior closure of
#124. Avoid a circular wait in which #125 cannot start until #124 is closed while
#124 is waiting for #125 evidence.

## Iterate fast: preserve passes and target failures

Maintain a lightweight durable progress note and an append-only verification
ledger using existing result locations/receipts. This is bookkeeping, not a new
test framework. For each test/benchmark record:

```text
identity: test/case + mode + fixture/seed/configuration
source: product + test/harness + relevant dependency/fixture identities
environment: target/toolchain/image and applicable runtime settings
result: pass/fail/timeout/blocked; command and raw evidence path
validity: current or invalidated, with the exact reason
```

Rules:

- Run the smallest relevant failing test or case while diagnosing a defect.
- Once a test passes, retain its result. **Do not rerun an already successful test
  solely because you fixed another test or changed unrelated code.** Repeatedly
  successful, unaffected tests should be reused from the ledger.
- Before rerunning a passing test, identify a concrete invalidation: its exercised
  code/shared dependency changed, its fixture/oracle/environment changed, evidence
  was invalid, or an applicable final-candidate custody rule requires new evidence.
  Record that reason. A new repository HEAD alone is not an explanation that every
  unit test became relevant to the fix.
- Trace the changed function's callers and shared dependencies. A fix in a common
  encoder, ownership primitive, or publication path may affect multiple passing
  tests; a fix confined to another independent test/helper does not justify a
  blanket full-suite rerun. Do not reuse a pass whose relevant dependency changed.
- After a fix, rerun the previously failing check and the smallest genuinely
  affected regression set. Broaden only for new failures or unresolved concerns.
- Reuse compatible Cargo/incremental builds, sealed binaries/images, pristine
  inputs and existing runner facilities. No routine clean builds, cache deletion,
  repeated fixture generation or all-family runs during every debugging iteration.
- Never repeat an unchanged benchmark to obtain a better number. Preserve every
  attempt and follow its existing paired-run/custody rules.
- The required final full benchmark campaign is not optional. Plan it after
  correctness is established and the candidate is sealed; run its complete included
  set, not only previously failing cases. This is distinct from repeatedly running
  full suites after every edit during development.
- If a final benchmark exposes a defect, fix it, invalidate only evidence actually
  affected under the applicable source/custody rules, and rerun the required set.
  Carry forward valid unaffected evidence only when those rules permit. Do not
  falsely label old binary evidence as final-candidate evidence; do not use final
  validation as an excuse for gratuitous repeated runs.

Do not impose fixed repetition counts or invent new performance gates. Existing
benchmark repetitions are part of their contracts, not redundant debugging reruns.
Preserve the separation of performance, verification, setup and cleanup.

## Full benchmark scope and honest numbers

Phase 7 executes the full active existing benchmark suite, including its required
extended/proof-only cases and applicable modes/seeds/repetitions, **minus all
#122-owned cases**. Reconcile the exclusion manifest and current issue/registry
before freezing the execution manifest. The recorded set has 36 cases: 33 regular
and 3 extended. Match exact case identities and explicitly resolved successors;
do not drop whole families containing inherited cases.

Do not use `shared/v016_matrix.py` as the full-suite driver: it targets #122's
excluded matrix. Do not mistake smoke/default selection, `HOST_FAMILIES`, or a
single affected subset for full coverage. Include active family-specific entrypoints
not returned by generic enumeration. Verify include/exclude disjointness, complete
coverage, modes and cardinality before launch.

Respect the existing measurement lock. macOS hosts Store/SDK/coordinator/canonical
construction/spool; Linux Docker hosts daemon/FUSE/workload under the established
limits. No concurrent builds or unrelated resource-sensitive tests contaminate
measurement. Do not steal a lock or damage another campaign's artifacts.

Report actual per-case numbers to #125 with machine-readable full matrix, readable
tables and raw evidence links: sample counts and supported statistics, timing
boundaries, CPU/RSS/cgroup domains, Store/spool/index/retained/cleanup storage and
I/O where available, verification coverage, statuses and exact source/harness/image/
fixture identities. Report unavailable values as unavailable, not zero. Claim
speedups only from compatible matched evidence.

Excluded #122 rows are `EXCLUDED_ISSUE_122` in the scope ledger, not passing rows.
Permitted proof-only performance is N/A, not fabricated timing. Record original
failures/timeouts/regressions and their fixes. Do not hide a failed existing
acceptance criterion behind fast inner Commit time, shorten workloads, weaken
oracles, raise deadlines after a miss, or seek new waivers to force closure.

## Persistence, delegation, and resumability

Use bounded subagents for independent implementation responsibilities or adversarial
reviews when useful. Assign non-overlapping file ownership, warn them about shared
work, and integrate/review their outputs. Prefer local work when delegation would
only duplicate exploration. Keep performance measurement serialized under its
existing lock even when analysis or unrelated edits are delegated.

Maintain current phase, next concrete action, issue-comment links, pass ledger,
active processes, source/fixture seals, blockers and exclusion identity in durable
progress notes. On context compaction or continuation, resume that state; do not
restart exploration or rerun successful checks by default.

Do not stop after planning, scaffolding, one green test, or an intermediate report
while meaningful authorized work remains. Continue fixing and verifying until
the terminal conditions are met. A genuine external approval/platform/resource
blocker is not terminal PASS: report the exact failed action and evidence, keep
the issues open, request only the indispensable intervention, and continue other
useful work. Do not spin on identical failures or manufacture closure.

## Terminal PASS and verified closure

All of the following must be true before closing either issue:

- The product meets the governing rules and specification, including non-pausing
  supported FUSE/SDK semantics, bounded ownership, exact C1/C2 results and safe
  staging/retry/cleanup. No unresolved V1-V4 correctness/design obligation is
  concealed as a future feature.
- The required correctness/integration/capacity evidence is complete and valid for
  the final implementation. Required phase-completion comments have been posted.
- Every included required benchmark/performance/verification row is accounted for
  with valid evidence and meets its existing applicable mandatory criteria. No
  required unresolved FAIL/TIMEOUT/BLOCKED/NOT_READY/NOT_RUN, invalid verification,
  resource, cleanup or custody outcome remains. Existing explicitly permitted N/A
  or out-of-scope exclusions retain those labels; reporting-only targets do not
  become new mandatory gates.
- #125 contains actual final numbers, raw manifests, exact inclusion/exclusion
  accounting, reproducible commands, source/binary/image identities, all retained
  failures/fixes, and a truthful terminal status. No #122 benchmark was executed
  as part of this campaign.
- Repository changes are scoped, reviewable and published through the required
  repository workflow. The evidence describes the actual delivered source. No
  pending required review/approval is bypassed by closing an issue.

Then post final completion summaries and evidence links, close #125 and #124 with
the completed reason, and read both issues back to verify their CLOSED state and
final comments. If an API call is uncertain, inspect its result/state before
retrying; avoid duplicate comments or closure claims. Do not report terminal PASS
or end with "done" until both verified states and the evidence support it.

Final response: link the delivered source, phase/evidence comments, final benchmark
matrix and both closed issues; summarize measured outcomes and the retained #122
exclusion. No release/tag is part of this handoff.
