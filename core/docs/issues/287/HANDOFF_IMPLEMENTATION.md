# Issue287 implementation agent handoff

> **Status: Current planning checklist; no release candidate exists.**
> Owner-requested handoff, 2026-09-30. Research baseline:
> `7edddbdb8e8512627aed0ed42533ef099d802384`.
> This is a future implementation assignment. Preparing this prompt does not
> start implementation, publish commits or claim a completed milestone.

## Copyable agent assignment

You own the **implementation phase of
[#287](https://github.com/Ephemeral-AI-Lab/layerfs/issues/287)**: implement bounded
streaming live Workspace/Commit authority and concurrent commands/Workspaces
through the R0-R7 rollout. Work iteratively in coherent milestones. At the end of
each milestone, commit the owned change, publish the checkpoint on your branch,
and update #287 with exact source, checks, production LOC, remaining gates and
the next milestone. Continue autonomously after that checkpoint; it is not a
request for permission to continue.

**Your scope is #287 implementation only.** Read
[#288](https://github.com/Ephemeral-AI-Lab/layerfs/issues/288) for qualification
dependencies, but do not execute its benchmark campaign, implement new benchmark
runners/families, update its issue, or claim it complete. Owning product tests,
real-provider correctness/resource proofs, independent reference vectors and
narrow count-driven implementation diagnostics are required under #287. Record
any remaining benchmark qualification as delegated/unrun under #288.

The older research-only banners describe the original research task. This
assignment now directs implementation after R0 publication and contract freeze.
Do not resume the completed Phase B/#286 campaign, older continuation prompts,
or the previous two-lane execution assignment as a competing objective.

## 1. Establish ownership and read the handoff

Start by inspecting `pwd`, Git status/branch, actual HEAD and ancestry, remote
configuration, available task artifacts/worktrees, and the current #287 body and
comments. Record the actual source before editing. The baseline above is the
research pin; inspect later main changes rather than resetting a checkout to it.

The complete unpublished research package is currently in:

```text
/Users/yifanxu/.codex/worktrees/issue286-phase-b/layerfs/
  scenarios.md
  benchmark_agent_report.md
  core/docs/architecture/proposal/bounded-memory-commit-20260930.md
  core/docs/architecture/proposal/bounded-workspace-implementation-20260930/
  core/docs/issues/287/HANDOFF_IMPLEMENTATION.md
```

Read the root and Core AGENTS, applicable component contracts/roadmaps,
documentation/release rules, `scenarios.md`, and the full packet in this order:

1. [README](../../architecture/proposal/bounded-workspace-implementation-20260930/README.md)
   and [ROLLOUT](../../architecture/proposal/bounded-workspace-implementation-20260930/ROLLOUT.md).
2. [LAYOUT](../../architecture/proposal/bounded-workspace-implementation-20260930/LAYOUT.md),
   [WORKSPACE](../../architecture/proposal/bounded-workspace-implementation-20260930/WORKSPACE.md),
   [SERVER](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md)
   and [CONCURRENCY](../../architecture/proposal/bounded-workspace-implementation-20260930/CONCURRENCY.md).
3. [ACCEPTANCE](../../architecture/proposal/bounded-workspace-implementation-20260930/ACCEPTANCE.md),
   [ISSUE276-AND-SCALING](../../architecture/proposal/bounded-workspace-implementation-20260930/ISSUE276-AND-SCALING.md),
   [ISSUE-CLOSURE](../../architecture/proposal/bounded-workspace-implementation-20260930/ISSUE-CLOSURE.md)
   and [EVALUATION](../../architecture/proposal/bounded-workspace-implementation-20260930/EVALUATION.md).
4. Current #248/#256/#249/#219/#259 acceptance and the latest #276 comments.
   Treat historical source observations as historical. Do not reimplement work
   already delivered by Phase B or promote older receipts to a new source.

Reuse a suitable owned managed worktree, or create a clean owned worktree/branch
from the intentionally selected published source. Preserve the primary checkout,
other owners' worktrees, unmerged candidates and unrelated `output/` posters.
Do not stage/stash/reset/delete their work or use a foreign Cargo target.

Bring only the owned research package, scenarios, this handoff, report extension
and scenario-routing additions in root/Core AGENTS into the implementation
worktree for R0 publication. Inspect and merge those additions if the destination
files advanced; do not overwrite unrelated current rules. Keep all required
supporting source captures. If a required draft is absent, locate it from the
named source before proceeding; do not invent a replacement from memory.

You may use current-thread subagents for well-scoped source research,
implementation or independent review. Assign non-overlapping file/responsibility
ownership, one coordinated Bridge contract owner, and shared interfaces first.
Tell workers they are not alone and must preserve others' edits. You retain
integration, acceptance, commit and issue-update ownership. Do not create new
Codex sessions or message unrelated tasks.

## 2. R0-R7 milestones

R0 must publish the complete packet/scenarios at a repository commit and link
that exact commit in #287 before product implementation. Resolve routine design
choices from source/evidence yourself: exact records, handle/cancel grammar,
real opcode/capability allocation, platform authority and resource arithmetic.
Update affected contracts transparently; never guess tags, reference IDs or proof.

| Milestone | Owned delivery and exit gate |
| --- | --- |
| R0 | Publish/freeze the packet, current source audit, concrete interfaces/formats/profile versions, independent v1/v2 oracle method/vectors, simultaneous window accounting, migration/runtime authority and a file ownership plan. Distinguish proposed admission arithmetic from measured physical capability. |
| R1 | Paged cursor/draft/run/index state in existing crates, admitted caches, typed C2 Save/C5 catalog admission, protected control/completion and SQLite engine accounting. Prove exact refusal before effects, bounded simultaneous allocations and catalog progress under occupied Save slots. |
| R2 | Private Workspace v3: per-file split/join, paged catalogs/owners/locations, immutable sources, conflict leases, short current-catalog publication and bounded retirement. Prove ordinary syscall state, unrelated-inode progress and no resident population/recursive WRITE history. Include live handle/cookie resource admission. |
| R3 | Unified FileSet/binding/result streams, fresh direct input and one inherited replay source, paged exact C1 state, immediate-parent file policy v2, certified namespace parent index, schema11 migration/import implementation. Independent canonical/semantic proof, exact EOF/result seals and Unknown custody must pass. |
| R4 | Complete single-Workspace capture/G1/G2/READY/Branch/install/cleanup composition. First integrate one complete supported mutation through cleanup; then prove successive heads, pins, writes during each real phase and candidate crossing capture/install. No proportional post-publication allocation or stale G2 replacement. Include #248/#256 boundary obligations and the actual supported resource profile. |
| R5 | Owned native I/O/commands/domains and first-party cancellable FUSE session; exact actor/issued-handle access, truthful quota, mount-death recovery and explicit discard. Prove UntilOwnedExit beyond30s, exact cancel/disconnect/descendant/reaping/pipe outcomes and authorized external mounted-path mutations. Preserve finite operation bounds and accepted writes. |
| R6 | Keyed registry/incarnations, channel pooling, operator count1/2/3, independent Exec/mount/Commit ownership and admitted overlap. Enable only after #248 prerequisite gates. Prove same-W/cross-W correctness/progress, one pending submission perW, explicit shared-Branch conflict, capacity/refund and no lifetime command state/scan. |
| R7 | Explicit supported cutover/import, old-owner drain, safe retirement of displaced mutable authority, updated architecture/API/profile docs and final owning checks. Preserve required v1 compatibility and exact Unknown/migration custody. Deliver reviewable final source plus the remaining #288 qualification handoff. |

Milestones are coherent delivery boundaries, not mandatory single giant commits.
If one is too large, define named submilestones (for example R2a/R2b) with real
responsibility, dependency and exit proof before starting them. Apply the same
commit/comment checkpoint to each. Do not use arbitrary iteration numbers or
empty scaffolding as progress. Runtime work can be developed earlier against
frozen interfaces; concurrency enablement waits for the prerequisite composition.

## 3. Iteration loop

For each milestone/submilestone:

1. Record the parent source, affected SC IDs, owned files, representation/algorithm
   being replaced, smallest complete delivery, independent expected result and
   exact exit gates. Keep a short current milestone checklist and append-only
   `core/docs/issues/287/IMPLEMENTATION-LOG.md`.
2. Implement the selected cohesive path. Reuse sound existing primitives and
   retire displaced authority when safe. Do not wrap the old failure mechanism
   in another manager or move its quadratic work to disk unchanged.
3. Review actual allocations, sources, leases, selected roots, failure/Unknown
   boundaries and accumulated-work laws. Freeze the coherent source and run
   meaningful covering checks. A passing byte oracle is separate from resource,
   liveness or performance proof.
4. Diagnose a red result from its output/source and narrow cause evidence; apply
   the demonstrated correction, then run its covering commands. Repeating an
   unchanged passing suite or benchmark to select a nicer result is prohibited.
5. Update architecture/contracts, current checklist and append-only checkpoint
   with exact outcomes, checks/gaps, source/owner evidence and remaining work.
6. Stage only owned files, compute exact production LOC, and commit the milestone.
   Verify the committed snapshot matches the counted staged source. Push the
   owned checkpoint branch and confirm publication; do not force-push over
   another owner's changes. If publication fails, retain the commit and report
   its actual local/published status without inventing a remote link.
7. Post one substantive checkpoint comment on #287, update its own milestone
   checkboxes accurately, and continue the next eligible milestone autonomously.

**A failed/incomplete milestone is not complete.** Commit a reviewable partial
checkpoint when useful, label it PARTIAL with the failed gate, and keep dependent
enablement disabled. Continue resolving that gate or independent authorized work.
A genuine capability/provider blocker gets concrete source/output evidence and
its precise unsupported scope; it cannot become an error-driven fallback or PASS.

Every milestone needs a commit and #287 update. Several smaller commits are
allowed, but every commit gets its own exact LOC comparison. Maintain one clear
current next action across interruption/context compaction; resume from the latest
committed checkpoint/comment rather than restarting passed work.

## 4. Verification scope for implementation

Use owning external contract/integration tests and real providers to prove bytes,
roots/partitions, name/alias/orphan/permission facts, G1/G2/pins, parser/framing,
failure/Unknown custody, allocation overlap, cleanup and actual concurrency.
No inline tests, fake clocks/allocators, fault branches or test-only hooks in
product source. Deterministic process/kernel/provider coordination establishes
overlap; a sleep, submitted RPC or launched PID alone does not.

R0 selects an independent reference for new v2 expected roots. Candidate output
cannot provide its own pins. V1 compatibility and new v2 identities are separate
proofs. Validate schema migration's cross-process quiescence, metadata/pack
identity and Unknown phases; authentication alone is not namespace certification.

Run the policy-required owning locked Core tests/examples/fmt/Clippy and product
boundary/self-tests at coherent handoffs. Scope milestone checks honestly and
reuse unaffected checks; complete required full Core checks at final handoff.
Use the repository-root ARMv8 AEAD flags on aarch64. Report exact commands,
exit/results and Linux/Darwin/real-provider gaps. A host-skipped body is not a
Linux proof. Do not run CI or the permanently retired aggregate preflight.

Do not run the existing seven-family benchmark campaign or new Family8/9
performance rows in this assignment. Preserve their existing receipts and
annotate which mechanisms changed for #288. In particular, no routine Family2
reruns. Necessary count/resource diagnostics stay explicitly labeled, bounded,
source-pinned and separate from speed admission. If a diagnostic enters benchmark
collection, its prospective case/oracle/topology/limits and all measurement rules
apply; defer the campaign execution to #288 rather than silently expanding scope.

The quiet >30s command is a separately bounded implementation correctness proof,
not permission to enlarge15/25s performance gates. Physical Server containment
proof is a separate supported-provider capability; do not silently relocate
benchmark Server/SQLite into Docker. Missing memory/cache observation remains
unavailable/incomplete and cannot be replaced with a lifetime peak or heap-only
claim. Hard cgroup containment alone does not prove healthy progress.

## 5. Commit and LOC procedure

Follow the exact repository counting scope. Before each commit:

1. Identify HEAD as the first parent; inspect the final staged file list and diff
   to exclude unrelated/unstaged/untracked work.
2. Count the first-parent and final staged tree snapshots with the **same**
   counter/version, production classification and exclusions. Use Git tree
   archives or the existing reproducible snapshot method. Include shipped SQL
   and exclude tests/examples/fixtures/tools/docs/generated/third-party code.
3. Report reference, Core and combined before/after/signed delta. Imports,
   declarations and delegation count. Physical file-line ceilings are separate.
4. Include this in every commit message and the milestone handoff:

```text
Production LOC: <combined before> -> <combined after> (delta <signed delta>)
Reference: <before> -> <after> (delta <signed delta>)
Core: <before> -> <after> (delta <signed delta>)
Method: <counter version/hash, source scope, exact snapshot method>
```

After committing, confirm the committed tree reproduces the staged result.
Recompute after an amendment/rebase or staged-source change. Docs/tool/test-only
commits report unchanged product totals and delta0, not a zero-sized product.
The researched baseline was reference65,417/Core70,279/combined135,696; recount
the actual parent, not that historical number by assumption.

The current counter's SQL comment/src-SQL defects did not affect the audited
baseline. If new affected SQL formats encounter them, repair classification with
external tool tests and apply the same revised method to **both** snapshots before
using the comparison. Never omit new production inputs to shrink LOC.

Commit architecture and implementation notes with the owning source. The local
ledger can record parent and staged production-source seal; the issue comment
records the resulting actual commit URL. Do not guess a self-referential future
commit SHA inside its own document.

## 6. Required #287 milestone update

Read the latest issue state before editing. Preserve the original requirements,
other comments and historical verdicts. Append the checkpoint rather than
rewriting old evidence. After R0, add source-pinned packet/scenario/contract links
to the issue body. Check a milestone only when its actual exit gates pass.

Use this structure; omit no failure or unsupported observation:

```text
Milestone: R<n> / <named submilestone>
Disposition: COMPLETE | PARTIAL | CAPABILITY-LIMITED
Source: parent <sha> -> commit(s) <actual URLs>
Owned branch/worktree: <exact identities>; publication <confirmed/local-only>

Delivered behavior and architecture:
- <representation/algorithm/authority replaced and resulting behavior>
- <affected SC IDs and exact supported profile>

Verification:
- <exact commands, outcomes, independent oracle/provider and evidence links>
- <checks not run/reused/platform gaps, with reasons>
- <count/resource diagnostics; do not invent a speed PASS>

Production LOC:
- <each commit's exact comparison; reference/Core/combined and method>
- <relocation/temporary duplication/retirement when applicable>

Remaining gates and #288 dependency:
- <unresolved correctness/resource/provider gates and retained failure custody>
- <affected mechanisms/proofs for later benchmark qualification; no campaign run>

Next: <one concrete next milestone/submilestone and exit gate>
```

This assignment authorizes checkpoint commits/pushes on the owned branch and
milestone updates to #287. Do not message other issues/tasks, merge remote main,
close #287/related issues, delete unrelated artifacts or change another owner's
run without a separate instruction. If a draft PR is created for review, attach
it to the current task through the app tool as required; do not merge it.

## 7. Architecture constraints that every milestone preserves

- Generic opaque commands and supported ordinary POSIX/FUSE mutations; mount
  authority is independent of Exec launch. No command/workload recognizer,
  benchmark mutation route or implicit suffix-free splice.
- Final-state interval overwrite and shared immutable subtrees, not recursive
  WRITE checkpoints. First-touch opaque predecessor spans; exact current-G2
  install; immutable selected resolver/location contexts.
- Paged authority plus bounded admitted windows, with actual graph/order/source/
  cleanup work. No whole-frontier vector, unbounded owner registry or quadratic
  historical scan moved to a scratch DB.
- READY before the single composite final publication request; protected
  installation/control capacity, exact known/Unknown custody, no resend,
  query-based guessed adoption, refund or rollback of accepted writes.
- Per-W submission ownership and actual aggregate resources; typed C2/C5
  permits, unchanged Store capacity and one producer per construction operation.
  Idle Workspaces do not reserve full Commit buffers.
- UntilOwnedExit end to end with finite liveness/callback/Commit/cleanup bounds;
  exact command domains, actors/incarnations, owned partial authenticated I/O,
  bounded output and no per-Exec helper stack/lifetime registry growth.
- No third-party patch/fork/vendor, new dependency when existing capability
  suffices, automatic retry/fallback, WAL/sync/crash-recovery expansion or hidden
  quota/worker/timeout/profile increase to make a gate pass.
- Existing Core crate boundaries, SRP-focused files, product-only src, <=999
  physical lines per production file and <=200 declaration/delegation lines in
  lib.rs/mod.rs. Do not build a universal manager or treat LOC forecasts as budget.
- Never exchange approximately5% storage saving for approximately50% speed loss.
  Preserve the accepted up-to10% history allocation profile; temporary disk has
  its own actual admission. No speculative codec/page-size campaign.
- Larger file/body/count profiles, writable mmap/complete DB locking/durability,
  S3, conflict resolution and stronger confinement remain separate capabilities.
  The deferred10,240 case is not automatically activated.

## 8. End of implementation assignment

At R7, deliver the coherent committed implementation, pinned architecture and
contract docs, exact supported profiles, independent implementation proofs,
required owning checks, per-commit LOC and safe retirement/cutover record. Update
#287 with final implemented versus capability-limited/deferred status and a
precise qualification handoff for #288.

Do not call benchmark or release admission complete. Do not claim every deferred
ticket or every SC tier solved. Leave tickets open unless separately instructed
to close them after their full acceptance. Finish the authorized implementation
work; do not stop after publishing a plan or the first small slice.
