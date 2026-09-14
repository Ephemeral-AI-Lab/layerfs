# Execute #130: compact Workspace implementation and tiny-churn qualification

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. This is an execution handoff,
not a request for another plan. Implement, debug, verify, publish and complete the
full current #130 rollout. Do not stop after a review, revision, scaffold, single
passing test, one benchmark case, subagent completion or intermediate phase.

## 1. Current objective and issue ownership

Implement [#130](https://github.com/Ephemeral-AI-Lab/layerfs/issues/130): reduce
Workspace operation, temporary-storage, memory, I/O and cleanup overhead, with
performance close to the existing tiny-churn benchmarks. Quadratic cumulative
scaling is unacceptable. Necessary linear processing is allowed; prefer
logarithmic point/index work and constant-sized snapshot ownership registration.
Keep residual indexed/sorting factors explicit rather than claiming everything
is linear or constant-time.

- #130 owns local payload reclamation, bounded metadata preparation and the
  measured compact storage/correspondence/lifecycle/transport improvements, plus
  their correctness checks and tiny-churn evidence.
- #124 owns production host/Docker FUSE/SDK integration, V1 supported capture,
  remaining Commit/consumer migration and obsolete coupling removal. Advance
  required integration dependencies in scoped changes and update #124; do not
  rename them as #130 improvements or make all #124 work a prerequisite for
  independently testable #130 code.
- #125 owns the later full included benchmark campaign and final matrix. This
  handoff's five-case screen or full tiny-churn family is not that full campaign.

The **25,000-file/two-second milestone and million-file qualification are
deferred**. Do not register, generate inputs for, or run them in this execution.
Keep any broader terminal requirement for the million-file proof OPEN; deferral
is neither PASS nor waiver. No #122-owned scenario or case-specific preparation
is authorized. No release/tag, deployment or #123 closure is authorized.

## 2. Read current authority and preserved state before editing

Read current #130/#124/#125 bodies and applicable AGENTS.md files, then these
files under `docs/roadmap/0.1/0.1.6/`:

1. `overlay-minimal-overhead-implementation-plan.md` — current P130.1–P130.5
   deliverables, ownership, selected mechanisms and acceptance.
2. `overlay-snapshot-rule.md`, `overlay-snapshot-spec.md`,
   `overlay-snapshot-spec-review.md`, `overlay-snapshot-architecture-design.md`.
3. `overlay-snapshot-contract-resolution.md`, `overlay-snapshot-progress.md`,
   `overlay-snapshot-verification-ledger.md`.
4. `benchmark-exclusions-issue122.json`; read `cases.json` only as the source of
   exclusions, not as this campaign's include list.
5. `evidence/minimal-overhead-review/` and preserved capacity/V1/Step 5 evidence,
   including the cleanup/handoff inventory. Older proposals and stopping notes
   are historical where superseded by this handoff and the current plan.

Before benchmark work, read `benchmark/AGENTS.md`,
`docs/general/benchmark_rules.md` and `benchmark/fs-bench-pro/QUICKSTART.md`.
Do not execute the generic v0.1.6 `handoff-prompt.md`: it targets excluded #122.

Inspect actual git status, main, remote state and current source. The cleanup
checkpoint leaves main as the only local branch. The cancelled four-file Step 5
work is preserved as an explicitly UNVERIFIED patch, not applied or passed code.
Review and selectively repair it only when needed for #124; never restore it
wholesale merely because it was archived. Preserve raw failures, fixtures and
other work. Do not rerun historical V1 probes without a changed hypothesis.

Post a concise takeover comment to #130 naming source, retained evidence, open
obligations and the first concrete implementation task. Then execute that task
in the same run. An issue comment or refreshed plan is not completion.

## 3. Strict first-party-only implementation boundary

**Do not patch, implement, fork, vendor, replace or copy/reimplement third-party
libraries or imported dependency implementations. Do not add or modify
third-party dependency/import declarations to bypass an API limitation.**

- Do not edit Cargo registry/cache sources, installed packages, vendor trees,
  downloaded dependency code or external generated implementations.
- Do not add `[patch]`, `[replace]`, source overrides, dependency forks, monkey
  patches, local substitute libraries, new third-party packages or dependency
  version changes for this work. Do not change Cargo.lock to conceal a substitute.
- Use documented existing dependency APIs. First-party implementation and its
  own module imports may change; standard-library facilities remain available.
  `crates/layerfs-fuse` is first-party; the `fuser` dependency is third-party.
- Keep third-party dependency/import manifests unchanged. Verify relevant
  Cargo.toml/Cargo.lock/config diffs and dependency source identity before final
  publication. A first-party filename does not make copied third-party code
  permissible.
- If the only remaining solution genuinely requires a third-party change,
  report the exact API/platform limitation and required external intervention.
  Do not bypass this restriction; continue independent first-party work.

## 4. Implementation order — perform the actual code changes

**P130.1: comparator and cost instrumentation, alongside code work.** Pin the
five existing cases below, compatible control/candidate identities and existing
criteria. Consume #124's verified macOS-host/Docker route for public timing;
legacy dispatch cannot qualify the new implementation. If that route is pending,
only dependent public measurements wait. Continue owned-input component work.

**P130.2: first code slice — local reclamation, then metadata preparation.**
Reproduce the release-triggered whole-arena evacuation counterexample in a small
focused check. Replace it with indexed reusable space and bounded local/tail
relocation/truncation. Prove an amortized bound across completed maintenance:
repeated small deletions/replacements must not recopy unrelated live payload.
Preserve exact owners, relocation pins, quotas and failure charges.

Then prepare an operation's final Index/range records together, starting with
the small range leaf. Avoid repeatedly constructing/retiring intermediate roots.
Keep immutable published pages, bounded preparation, source-root comparison and
rollback. Check stale-source races, disjoint writes, retained snapshots and
partial-I/O/reference failures. Actual page/catalog writes must explain the gain.

**P130.3: select the next measured storage cost.** Implement only justified
changes: byte-aware dense pages in the existing Index; Empty/Single/Indexed
ranges; packed ordinary payload allocation in existing arenas; metadata-only
singleton canonical correspondence. Version the chosen private encoding and
ownership transitions first. Do not blindly raise MAX_KEYS, append a descriptor
that creates an overflow page, or require the complete bundle before useful work.

Prove maximum key/value bounds, split/merge, spill/reload, promotion and append
lineage, file-scoped token/base ownership, unrelated-neighbor release, fsync,
quota/partial-I/O rollback and physical allocation/cleanup. Denser pages can
increase ownership callbacks: count them instead of assuming density means speed.

**P130.4: remove the remaining measured latency.** Select lazy unused resources,
a bounded immutable-page cache, correspondence/clean-construction savings or
transport improvement only for an identified cost. Preserve replay, ordering,
host ownership before acknowledgment, bounded queues and exact publication.
No mandatory new pager, parallel backend, canonical encoder or generic framework.

P130.3/P130.4 may complete without another mechanism only when valid measured
evidence demonstrates that no additional change is needed for their exits. Record
that decision and evidence in the required package comment; do not implement
every optional mechanism merely to tick boxes or skip a remaining measured cost.

**P130.5: complete rollout and qualification.** Meet the five-case comparison,
then qualify all 20 unchanged tiny-churn cases when stable. Complete the genuinely
affected correctness/resource/cleanup checks. Publish reviewed implementation and
evidence through the repository workflow; verify delivered main and issue state.
A passing five-case screen is not terminal success for this full #130 plan.

Throughout, preserve owned capture, C1/C2 locality, live mount/inode/descriptor
identity, ordinary progress during Commit, CAS/FULL-DELTA/CDC/extents/packing,
exact stage/retry and Created/UpToDate receipts. Ordinary Commit must not regain
a pause/quiesce/drain/checkpoint-reset fallback. V1 dirty-mmap visibility remains
open until proved; host-root atomicity alone cannot qualify the complete surface.

## 5. Exact quick-iteration and later family scope

Quick iteration selects exactly:

```text
tiny-create-500-mixed-v4
tiny-stat-500-mixed-v4
tiny-unlink-500-mixed-v4
tiny-bulk-create-500-mixed-v3
tiny-bulk-delete-500-mixed-v3
```

The first three affect 500 files; bulk tier 500 affects 5,000 files/500 MiB plus
its registered witness. Preserve actual paths, distributions, backgrounds,
normalization, root sync, worker counts, modes, seeds and independent oracles.
No smaller tier, one-byte replacement, SDK bulk shortcut or shortened workload.

When stable, include all 20 existing `tiny_file_churn` identities: the five
operations at tiers 1/10/100/500, with registered compact-v2/mixed-v3/mixed-v4
suffixes. Reconcile registry and exclusions; all 20 are outside the recorded
36 #122 cases (33 regular + 3 extended). Do not run `shared/v016_matrix.py`.

For “close performance,” use the current plan's prospective existing #118 paired
screen: n3 fresh alternating pairs, material wall regression only when median
paired slowdown exceeds `max(15% of control median, 3 ms)` and at least two of
three pairs are slower; CPU uses `max(15%, 1 ms)`. Preserve stronger applicable
criteria and existing seeds/arm/custody rules; do not invent extra repetitions.
The workflow's receipt has `operations=1`: do not divide deltas by file count to
evade a floor. Keep reporting-only targets reporting-only and existing failures
or permitted minor differences accurately classified.

Use a compatible v0.1.5 control with matched workload, oracle, harness, topology,
cache and timer. Historical numbers alone are not paired evidence. If a compatible
control is unavailable, report that precise comparison block; do not select an
unlike/slower control silently or claim closeness from an unrelated Init result.

## 6. Fast, evidence-preserving verification loop

Use the existing runner, compatible incremental builds, sealed executables/images
and pristine prepared inputs. No routine clean builds, fixture regeneration or
cache deletion. Keep macOS Store/SDK/coordinator/construction/spool and Linux
Docker daemon/FUSE/workload under established limits. Performance, builds and
other resource-sensitive work obey the existing measurement lock; never steal
it, run competing measurements or damage another campaign's artifacts.

For each change:

1. Run the smallest relevant failing/new correctness check and repair it.
2. Trace shared callers/dependencies and identify affected prior passes.
3. Rerun the failing benchmark and genuinely affected regression set only.
4. Retain successful unaffected results. A new HEAD alone is not invalidation.
5. Record exact source/harness/fixture/environment, command, raw receipt, result
   and validity in the existing append-only ledger; preserve every failed attempt.
6. Immediately execute the next ready task. When stable, complete the required
   full-family/final-source qualification under its actual custody rules.

Separate performance, diagnostics, setup, independent verification and cleanup.
Preserve the registered public-call timer; report phase times and orchestration
wall separately. No inner-Commit headline for a slow complete workflow. Keep
normalization/sync and required engine work in their real boundaries. Full census
and verifier work must not contaminate performance samples.

Report actual time, CPU, host RSS, container/cgroup domains, transport/index work,
physical metadata/payload/retained/scratch/slack bytes and cleanup through
settlement. Unavailable is not zero. No quadratic work hidden in maintenance,
End or the first post-capture write; no quota increases to hide amplification.

## 7. Subagents, phase updates and persistence

Use bounded subagents for independent implementation responsibilities and
adversarial review. Give explicit non-overlapping file ownership, tell them they
share the repository, and prohibit reverting another agent's edits. Every agent
must iterate on its assigned concrete failures until its scoped exit evidence is
valid, or identify an exact external dependency. The coordinating agent reviews,
integrates and continues; a subagent's completion is not overall completion.

After **each actual P130 package completion**, post to #130 immediately:

```text
P130.N complete: <name>
Changes: <implemented and removed/replaced>
Source: <exact commit/PR and relevant binary/image identities>
Verification: <checks/cases, results, commands, immutable raw evidence>
Reused passes: <identities and why still valid>
Failures resolved: <retained failure, root cause and targeted successful rerun>
Remaining: <open obligations and next executable task>
```

Mirror #124 integration/correctness changes to #124 and benchmark-facing results
to #125. Preserve the original seven-phase updates when their full exits actually
complete; do not call a component or P130 package a completed #124 phase.
If later evidence invalidates a completion, correct the issue and local state.

Maintain current package, next action, issue links, source/fixture seals, active
processes, result validity and blockers in the existing progress note. On context
compaction, resume that state; do not restart research or rerun passing suites.
Publish small reviewable PRs, satisfy required checks/reviews, merge only scoped
verified work, then remove completed task branches and return to clean main.
Do not delete unrelated open-PR branches or preserved archives/evidence.

Do not finish with “ready for the next phase,” another plan, a partial result,
or promises to continue while authorized dependency-ready work remains. Wait for
running work, integrate results and continue. Do not repeatedly test an unchanged
failed hypothesis. Only when every remaining path requires unavailable external
input/resources may you report a concrete external block; name the failed action,
evidence, exhausted independent work and exact intervention. Keep issues open.
An external block is not terminal PASS, and persistence never authorizes fabricated
evidence, semantic relaxation or a prohibited dependency patch.

## 8. Terminal success and final response

Terminal #130 success requires all current P130 deliverables actually satisfied:
selected overhead implementation complete; five tier-500 and later full 20-case
evidence meets the existing applicable comparison/mandatory criteria; affected
correctness, supported-surface claims, ownership, resource, cleanup and source
custody valid; no required unresolved FAIL/TIMEOUT/BLOCKED/NOT_RUN; all phase
comments posted; source reviewed, published and delivered to main.

Verify delivery concretely: clean working tree on `main`, local main equals the
freshly fetched remote main, and GitHub merged source/evidence identities match
the delivered artifacts. Preserve unrelated work instead of deleting it to make
the tree appear clean.

Do not claim #130 success solely because old code or the legacy route passed.
Do not conceal an unresolved correctness dependency as an optional future feature.
Deferred scale work stays visibly deferred; no false closure of #124/#125/#123.

Post final #130 source/results/raw-evidence summary, close #130 with completed
reason only when its terminal conditions are met, and read it back to verify
CLOSED and the final evidence comment. Read state before retrying an uncertain
API action. Final response links delivered source, phase comments, full tiny-churn
matrix and verified issue state; report real measured outcomes and remaining
separately owned/deferred migration obligations. No release/tag is included.
