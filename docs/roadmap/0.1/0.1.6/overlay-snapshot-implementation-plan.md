# v0.1.6 Snapshot-Isolated Workspace implementation plan

Status: implementation and execution plan, 2026-09-14. No product implementation
or benchmark execution is claimed by this document. This plan follows the
[rule](overlay-snapshot-rule.md), [specification](overlay-snapshot-spec.md),
[architecture](overlay-snapshot-architecture-design.md), and
[adversarial review](overlay-snapshot-spec-review.md).

- Implementation issue: [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124)
- Benchmark execution/reporting issue: [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125)
- Related pending-capacity objective: [#123](https://github.com/Ephemeral-AI-Lab/layerfs/issues/123).
- Explicitly excluded benchmark campaign: [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122).

The final numbered phase is the full existing benchmark campaign excluding #122.
The benchmark issue owns execution/evidence for that phase; it is not a second
copy of the implementation work. Its dependency is a correct, source-sealed product
candidate, not closure of #124; there is no circular wait for issue closure.
This turn creates the plan/issues only; it does not run the phases.

Latest execution instruction: [handoff prompt](overlay-snapshot-handoff-prompt.md).
The owner requires phase-completion issue updates, fast targeted iteration with
valid pass reuse, and terminal success before closing #124/#125. This supersedes
the earlier execution/reporting-only closure allowance; existing numerical benchmark
contracts remain unchanged and no new performance gate is introduced.

## Scope and fixed decisions

Implement one current mutable Workspace overlay with host private backing, owned
Commit snapshots, and independent Commit attempts. Snapshot input feeds the existing
shared Init/Commit construction machinery, then canonical admission,
`workspace_stages`, and conditional branch publication. Commands and normal
filesystem/SDK operations continue during Commit. Publication does not install the
captured snapshot into live inodes or reset newer changes.

Preserve CAS/authentication, small-file FULL/DELTA selection, large-file CDC/extents,
packing/compression, stable inode/handle semantics, incremental namespace updates,
existing branch leases, resource checks, and public operation surfaces. No per-file
Workspace checkpoints, copied snapshot tree, second snapshot mount, operation-history
log, or duplicate content/admission engine.

Implement correctly first, then evaluate through the existing sophisticated
benchmark suite and report observed numbers. No new capture-latency gate, arbitrary
throughput threshold, mandatory replacement suite, or repeated million-file sampling
campaign is introduced. Existing per-case contracts/criteria still apply and their
outcomes must be reported honestly. Correctness/resource requirements are not
waived by this performance-policy decision.

## Issue ownership and relationship

| Work | Owner | Completion meaning |
| --- | --- | --- |
| Product contracts, implementation, removal of old coupling, correctness and source handoff | Implementation issue | Correct behavior with V1-V4 addressed and affected integration checks; no claim of benchmark success before final campaign. |
| Full non-#122 suite selection, execution, verification, raw data and human-readable report | Benchmark issue | All included required runs accounted for and honestly reported; benchmark execution/reporting is distinct from release qualification. |
| Pending-metadata capacity and million-file correctness objective | #123, coordinated through implementation issue | Preserve required scale/ownership evidence; do not silently replace or close #123. |
| #122's new/extended benchmark cases | #122 only, outside this plan | Do not run, prepare case-specific inputs for, or claim completion of those cases in this campaign. |

Current specification V1-V4 contain real unresolved correctness/design obligations.
Phase 1 must resolve them or record a concrete blocker. A host-root-only snapshot,
removing mmap support, requiring implicit user fsync, or reverting to global
freeze/drain is not an accepted implementation shortcut. V5 is later evaluation
using existing benchmarks, not a newly invented performance-readiness gate.

## Phase 1 — Resolve contracts and freeze the implementation source of truth

Work:

- Reconcile the specification with current code and close V1's actual supported
  FUSE/kernel visibility contract, including dirty mappings and acknowledged data
  previously buffered only in the container. Name the mechanism and error/order
  semantics. If no compliant mechanism exists, document the exact incompatibility
  and keep the implementation issue open; do not silently weaken the rule.
- Complete V2's root/page/catalog ownership transactions, source-root revalidation,
  resource defaults, replay window, retry fairness, range liveness and host physical
  allocation-release behavior. Keep acquisition independent of changed-set size.
- Complete V3's bounded predecessor adapter and lineage/zero normalization contract;
  retain the reviewed two-pass nonzero-anchor/zero-gap algorithm or justify a
  replacement against the same counterexamples and rules.
- Complete V4's exact Created/UpToDate uncertainty witness, stage retry and explicit
  End/Discard coordination. A missing stage is not publication proof.
- Record the current active benchmark catalog and protect any usable existing
  baseline evidence/artifacts under the existing rules. Do not run an invented new
  performance campaign as a prerequisite to implementation.
- Commit/push the authoritative rule/spec/architecture/review/plan and exclusion
  manifest when implementation work begins; attach immutable links to both issues.
  At issue creation these documents are local uncommitted drafts, not published
  source evidence. Preserve unrelated work and historical results.

Exit evidence: explicit resolved contract decisions and source links; remaining
blockers are concrete and cannot be marked complete. No code claiming full public
snapshot support proceeds on a known weaker visibility model.

## Phase 2 — Implement shared host overlay backing and atomic live state

Work:

- Add/adapt `overlay.rs` responsibility for host COW roots, bounded page/index cache,
  shared payload arenas, disk-indexed extent locations, ownership and reclaim.
- Install one consistent bundle of inode/binding/change-index roots. Use leased
  source-root identity and prepared sequence/index keys; unrelated concurrent writes
  cannot be lost by installing a stale root.
- Maintain latest-by-key and latest-sequence/key tracking atomically, with safe
  conditional covered-key cleanup. Keep live-base namespace masks separate from
  Commit change bookkeeping.
- Preserve compact/range semantics with bounded cursors; avoid all-piece vectors,
  per-logical-file descriptors, global namespace/path rewrites, and full-node scans
  where indexed identity/allocation state suffices. Trace non-Commit consumers
  before replacing path tracking.
- Implement accounted host ownership before ordinary successful acknowledgment,
  bounded active-session replay results, interval/block liveness, protected reclaim
  headroom, and failure-safe physical relocation/release.

Exit evidence: existing core tests plus focused root-race, replay, quota/short-I/O,
partial-retention, stale-reference and tombstone tests. Failed operations preserve
prior valid state; all retained memory/disk/FD ownership is explained.

## Phase 3 — Integrate FUSE/SDK operations and owned snapshots

Work:

- Adapt the existing live owner/backing protocol to the chosen single authority;
  container caches and SDK operations do not become competing mutable states.
- Add `snapshot.rs` responsibility for an owned captured root and bounded
  inode/name/range/change readers. Snapshot acquisition performs bounded root/owner
  registration, not fact-map export, cache drainage, payload copying, or deferred
  whole-map work on the next mutation.
- Implement the resolved kernel visibility/coherence contract. Keep existing mount,
  stable live inode identity, descriptors, working directories and supported mapping
  behavior. Retained snapshot/read handles see their own captured state.
- Make both FUSE and SDK operations complete while a test builder is held after
  snapshot acquisition; retain normal per-operation atomicity and resource admission.

Exit evidence: public-path exact snapshot/live separation, equal/unequal edits,
append/truncate/zero/large ranges, cached/mapped visibility, hardlinks and
open-unlinked lifetime. The correctness hold blocks only the test builder, not
operations; it is not performance evidence.

## Phase 4 — Separate Commit attempts and reuse staged construction/publication

Work:

- Replace mutable-live construction inputs with the owned snapshot reader; release
  lifecycle/live/backing-map guards before building.
- Keep one unresolved attempt per Workspace and a bounded queue whose snapshots
  are acquired only when execution starts. Keep other Workspaces independent under
  existing leases and aggregate budgets.
- Preserve shared Init/Commit output driver, canonical builders, bounded admission,
  localized inode/directory updates, result journals and `workspace_stages`.
- Separate live content provenance from the published root/head/base context.
  Implement metadata-only predecessor descriptions, indexed lineage matching and
  bounded transient predecessor-relative plans. Complete correspondence before staging.
- Publish the exact candidate conditionally and resolve exact attempt outcomes.
  Apply only bounded published-context/covered-boundary bookkeeping; never install
  old snapshot contents or globally clear newer changes.
- Retain staged conflict/unknown outcome without making live operations inactive.
  Retry the same candidate, not a new snapshot of current state.

Exit evidence: C1 excludes x written during construction and C2 includes it when
still effective; no-op coverage, create/unlink/recreate, zero insertion/deletion,
old-base correspondence, head conflicts and lost acknowledgments are exact.

## Phase 5 — Remove obsolete Commit coupling and finish safe cleanup

Work:

- Remove ordinary Commit's pause/quiesce/writer-finish/resume dependency, same-generation
  checkpoint installation, build-duration shared locks, global dirty reset, and
  pending-stage/publication-inactive coupling.
- Replace Commit's full dirty-fact export/mirror with snapshot access. Remove dead
  code and obsolete fields/messages only after auditing remaining fsync, SDK cache,
  diagnostics, reconciliation and explicit lifecycle consumers.
- Repurpose useful bounded checkpoint output as correspondence, without its old
  live-install obligation. Preserve the shared Init/canonical pipeline.
- Finish snapshot/read/attempt ownership release, bounded deferred cleanup and
  explicit End/Discard coordination. Stage-row deletion and raw backing reclamation
  remain distinct. Keep cleanup failure charged and separately reported.
- Update runtime/API limits and behavior documentation to match actual implemented
  semantics. Remove claims of a universal action-count ceiling or implemented
  functionality that has not been proved.

Exit evidence: call-site/removal audit against the spec inventory; no ordinary
Commit route still blocks live operations through a second lock or fallback.
Shared consumers continue working; no unbounded legacy map/history remains as a
hidden second implementation.

## Phase 6 — Complete correctness and integration; seal benchmark candidate

Work:

- Run appropriate existing unit/integration and supported live FUSE/SDK correctness
  checks, with focused additions only for genuinely missing snapshot semantics.
- Prove source-root races, replay, snapshot spill/reload, ownership/reclaim, staged
  retry/uncertainty, unchanged namespaces, hardlinks/open-unlinked handles,
  zero-range counterexamples and repeated Commit continuation.
- Complete #123's explicitly selected million-changed-file correctness/capacity
  proof with declared RAM/disk/work/deadline and fresh Store reopen. This is not
  a new numeric performance gate and not a substitute for the final full suite.
- Preserve all initial failures; fix shared causes rather than altering oracles,
  removing mmap, adding intermediate Commits, or weakening workload semantics.
- Seal the correct product source, host binary/helper/image and relevant harness
  identity, then hand the candidate and correctness evidence to the benchmark issue.
  Check no known V1-V4 defect remains while claiming implementation complete.

Exit evidence: product correctness/integration results and exact candidate custody.
This phase does not execute #122's benchmark cases. Reusing an existing fixture
helper is not permission to run an excluded scenario. Earlier focused checks are
not the final full benchmark campaign.

## Phase 7 — Final phase: run the full existing benchmark suite, excluding #122

Owner: [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125). This is the last numbered phase; reporting and
benchmark-discovered repair/revalidation belong inside it, not a later hidden phase.

### Select the complete suite without excluded cases

1. Re-read #122 and compare the exclusion manifest's source hash with `cases.json`.
   Reconcile any renamed/versioned successors before execution; do not silently
   execute an unclassified #122 descendant because its literal ID changed.
2. Enumerate the full active catalog from the frozen benchmark source/binary using
   existing `infra-list`/family entrypoints. Include active supported modes and
   explicitly selected extended/proof-only cases. Supplement the catalog for active
   family-specific entrypoints that are not emitted by the generic runner. Do not
   mistake `HOST_FAMILIES`, a smoke/default selection, or an affected subset for
   the whole suite. Active unavailable cases remain visible as blocked, not filtered out.
3. Let U be all active case/version identities; let E be the reconciled #122-owned
   set. Execute I = U minus E, preserving inherited modes/seeds/repetitions/source
   arms and applicability. The recorded 36 exclusions may include unregistered
   planned cases; report E intersect U separately, not an invented registry count.
4. Freeze an explicit include/exclude execution manifest and check disjointness,
   coverage, uniqueness, nonempty inclusion, modes and cardinality before launching.
   Exclusion applies to every mode and case-specific preparation. Do not exclude
   whole shared families: inherited `dedup_branch_history`, `mixed_load_bearing`,
   and `historical_access` cases remain included unless individually owned by #122.
5. Do not run `shared/v016_matrix.py` as this campaign: it targets the excluded
   `cases.json` matrix. Reuse existing generic/family runners against the explicit
   include manifest, with the smallest orchestration change only if needed.

### Execute and preserve evidence

- Follow `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md` and
  `benchmark/fs-bench-pro/QUICKSTART.md`. macOS owns Store/SDK/coordinator/canonical
  construction/spool; Linux Docker owns daemon/FUSE/workload. Use the existing
  measurement lock and approved resource configuration; no competing builds or
  verifiers contaminate measurements.
- Reuse compatible sealed builds and pristine fixtures, with independent writable
  copies per mutation run. Preserve exact source/binary/image/fixture identities.
- Execute every included performance and independent verification mode under its
  existing contract, including required extended cases, seeds and repetitions.
  Proof-only rows have performance N/A, not zero. Do not invent new latency gates
  or run the withdrawn capture/SW/MW campaign.
- Compare compatible baseline/candidate numbers only under matching existing
  semantics and custody. If a baseline is missing or incomparable, report that
  explicitly and publish candidate numbers without a fabricated speedup.
- Do not silently skip failures, cherry-pick retries, shorten work, raise deadlines
  after a miss, or mark missing observations as zero. Retain valid slow results,
  failures, timeouts, invalid runs and remediation attempts with distinct status.
- If benchmarks expose a correctness defect, return to the relevant implementation
  and integration work, then reseal and rerun required affected evidence on the
  corrected candidate. Preserve original evidence and account for stale rows.
  The final reporting phase is still Phase 7. Do not optimize speculative thresholds
  or rerun unchanged code merely to obtain prettier numbers.

### Report actual numbers in the benchmark issue

Publish append-only raw manifests plus machine-readable and readable tables. For
all included case/version/seed/arm/mode combinations report:

| Category | Required content |
| --- | --- |
| Coverage | Full discovered catalog, included identities, explicit #122 exclusions, expected/observed run counts, missing/blocked rows and reasons. |
| Timing | Actual raw units, sample count, supported aggregates/range, full operation/invocation timing under the existing contract, relevant edit/read/Init/Commit phases. |
| Resources | Host/container CPU and RSS/cgroup domains separately; Store/spool/metadata/index/retained/cleanup disk and I/O where available; measurement caveats. |
| Correctness and outcome | Verification coverage, Created/UpToDate/conflict/error, timing/resource/cleanup/custody statuses separately. |
| Comparisons | Comparable baseline/candidate values and actual deltas; unavailable/unpaired/incomparable clearly labelled. |
| Reproduction | Commands, source/binary/helper/image seals, fixture/oracle identity, unique artifact paths and manifests. |

Every #122-owned row is `EXCLUDED_ISSUE_122` in the scope ledger and is not counted
as PASS, FAIL, NOT_RUN, or included workload. All included rows must be accounted
for; unexecuted required rows mean the campaign is incomplete. Publish valid failures
and measured regressions honestly, but under the latest handoff instruction do not
close #124/#125 on reporting alone: resolve required failed criteria and obtain
terminal success with valid evidence. Reporting-only targets stay reporting-only;
no new numerical gates are introduced. Correctness failures remain open product
defects. Reuse valid unaffected passes and rerun only checks whose evidence is
invalidated, subject to the required final full campaign and existing custody rules.

Final output: full non-#122 benchmark matrix, raw evidence links, observed numbers,
remaining problems, and an explicit statement that #122 was excluded. No release,
tag, website publication or #122 completion is part of this plan.

## Removal/addition accountability

| Action | Main responsibilities |
| --- | --- |
| Add/adapt | Host snapshot-capable overlay and ownership, snapshot reader/cursors, independent attempt context, dual change indexes, predecessor provenance/correspondence, bounded replay/reclamation. |
| Remove from ordinary Commit | Global freeze/quiesce/resume, long live/fact locks, mandatory checkpoint reinstall, global dirty/generation reset, inactive-on-stage coupling and retry-from-current-state. |
| Replace | Dirty-prefix export/mirror, per-segment FD scaling, whole-extent retention, unbounded piece/map/path access where the new bounded model requires it. |
| Preserve | Shared Init/Commit machinery, CAS/FULL-DELTA/CDC, canonical authentication, object admission, stage/conditional publication, supported semantics and resource checks. |
| Audit before deletion | Non-Commit fsync/cache/SDK/reconciliation/end/discard consumers; retain required behavior and remove only obsolete dependencies. |

## Exact #122 exclusion snapshot

Machine-readable source: [benchmark-exclusions-issue122.json](benchmark-exclusions-issue122.json).
Captured from [cases.json](cases.json) on 2026-09-14: **36 cases, 33 regular and
3 extended**, SHA-256 `6a89e9ac1c5df25eb0a4a495cf013d0eb8ab66f55b10281de1f5d72b0a0fcdc8`.
This is a case-identity exclusion, not a blanket exclusion of the six families.
Reconcile and freeze its actual registry membership before Phase 7.

| Family | Lane | Excluded case ID |
| --- | --- | --- |
| `dedup_branch_history` | regular | `v016-history-large-hotset-k10-v1` |
| `dedup_branch_history` | regular | `v016-history-large-hotset-k100-v1` |
| `dedup_branch_history` | regular | `v016-history-namespace-inode-k10-v1` |
| `dedup_branch_history` | regular | `v016-history-namespace-inode-k100-v1` |
| `dedup_branch_history` | regular | `v016-history-boundary-cycle-k10-v1` |
| `dedup_branch_history` | regular | `v016-history-boundary-cycle-k100-v1` |
| `file_size_transition` | regular | `v016-boundary-small-control-v1` |
| `file_size_transition` | regular | `v016-boundary-below-v1` |
| `file_size_transition` | regular | `v016-boundary-exact-v1` |
| `file_size_transition` | regular | `v016-boundary-above-v1` |
| `file_size_transition` | regular | `v016-boundary-large-control-v1` |
| `file_size_transition` | regular | `v016-boundary-roundtrip-v1` |
| `file_size_transition` | regular | `v016-boundary-alias-roundtrip-v1` |
| `mixed_load_bearing` | regular | `v016-mixed-development-100mb-5000-k10-v1` |
| `mixed_load_bearing` | regular | `v016-mixed-development-100mb-5000-k100-v1` |
| `mixed_load_bearing` | regular | `v016-mixed-development-500mb-30000-k10-v1` |
| `mixed_load_bearing` | regular | `v016-mixed-development-500mb-30000-k100-v1` |
| `multi_workspace_development` | regular | `v016-workspace-mixed-100mb-5000-k10-v1` |
| `multi_workspace_development` | regular | `v016-workspace-mixed-100mb-5000-k100-v1` |
| `multi_workspace_development` | regular | `v016-workspace-mixed-500mb-30000-k10-v1` |
| `multi_workspace_development` | regular | `v016-workspace-mixed-500mb-30000-k100-v1` |
| `branch_development` | regular | `v016-branch-mixed-100mb-5000-k10-v1` |
| `branch_development` | regular | `v016-branch-mixed-100mb-5000-k100-v1` |
| `branch_development` | regular | `v016-branch-mixed-500mb-30000-k10-v1` |
| `branch_development` | regular | `v016-branch-mixed-500mb-30000-k100-v1` |
| `branch_development` | regular | `v016-branch-convergent-content-v1` |
| `branch_development` | regular | `v016-branch-fork-descendant-v1` |
| `historical_access` | regular | `v016-access-boundary-before-v1` |
| `historical_access` | regular | `v016-access-boundary-after-v1` |
| `historical_access` | regular | `v016-access-inode-before-v1` |
| `historical_access` | regular | `v016-access-inode-after-v1` |
| `historical_access` | regular | `v016-access-fork-point-v1` |
| `historical_access` | regular | `v016-access-divergent-head-v1` |
| `mixed_load_bearing` | extended | `v016-mixed-exhaustive-100mb-5000-k100-v1` |
| `mixed_load_bearing` | extended | `v016-mixed-exhaustive-500mb-30000-k100-v1` |
| `multi_workspace_development` | extended | `v016-workspace-four-100mb-5000-k100-v1` |
