# Cluster two implementation handoff

> **Status:** Current planning checklist; no release candidate exists.

Execution instructions for [implementation tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
The [implementation plan](07-implementation-validation.md) owns slice dependencies
and exit criteria; the [design index](README.md), current handbooks and agent guides
own product/engineering requirements. This prompt does not claim implementation,
qualification or release admission.

Copy the following prompt into the handoff agent in the same primary checkout.
It authorizes local commits and issue updates, and requires continued work across
checkpoint/milestone boundaries. It does not launch an agent or publish Git commits.

```text
Implement LayerFS cluster two iteratively to completion in the existing primary
checkout:

  /Users/yifanxu/Ephemeral-AI-Lab/layerfs

Work directly on local main, with product implementation under core/.
Progress tracker: https://github.com/Ephemeral-AI-Lab/layerfs/issues/307

You are authorized to implement the plan, make local checkpoint/milestone commits,
and update this tracker. Continue from one checkpoint and milestone to the next
without asking for routine approval or ending with an offer to continue.
Several checkpoint commits may belong to one milestone. Remote pushes, releases
and deployments require separate authorization.

Continue until all S0-S13 milestones and their required exit evidence are complete.
If a hard external blocker prevents further progress, preserve exact state,
document the blocker and the smallest required decision, and complete independent
useful work first. A failed check, context compaction or a completed milestone
does not by itself justify stopping.

Read before implementing, relative to the checkout:
- AGENTS.md and core/AGENTS.md.
- cluster_one_handbook.md and cas_cdc_deltaencoding_handbook.md.
- core/docs/issues/303/README.md and its seven primary operation/engine/FUSE docs.
- core/docs/issues/303/06-cluster-one-integration.md.
- core/docs/issues/303/07-implementation-validation.md.
- docs/general/optimization-guide.md.
- The tracker body and latest progress/checkpoint comments.

Apply current owner decisions and documented supersessions. Treat closed-stage
handoffs, research and historical measurements as their explicitly scoped sources.
The current implementation plan and public APIs govern this work; directory
presence does not prove that an excluded crate is implemented or tested.

First actions:
1. Inspect status, branch, current source and tracker. Preserve all existing work.
   Do not reset, clean, overwrite or blindly stage the entire checkout.
2. Review and explicitly checkpoint the authorized current design/agent-guide
   changes before relying on a committed baseline. Preserve unrelated
   core/docs/issues/301/, research and output/ material unless explicitly adopted.
   Record exact production LOC even for this docs-only checkpoint.
3. Derive the current milestone status from actual code and evidence. Initially
   S0-S13 are unchecked; do not mark design preparation as completed implementation.
4. Start S0/S1 with the minimal S2/S3 interfaces, progressing high-risk cluster-one
   prerequisites and runtime work alongside the overlay lane where dependencies
   allow. Follow the plan's dependency graph rather than forcing numeric order.
5. Maintain a concrete next-work list and keep implementing the next ready slice.

Milestones:
S0  Contracts, R1-R8/P1-P14, build/FUSE risks and sound Save lifetimes.
S1  One initialized, indexed and profiled shared SQLite overlay engine.
S2  Generations, exact capture frontier, fixed EOF and fair bounded service.
S3  Immutable base access through current public cluster-one content APIs.
S4  Complete namespace/metadata semantics and bounded directory cursors.
S5  Fragmentation-safe payload, streaming, truncate/regrow and sparse holes.
S6  Orphan/reader/capture custody, pressure/headroom and automatic reclamation.
S7  Engine EXPLAIN/profile, complexity and aggregate resource acceptance.
S8  Real Linux FUSE, daemon and five explicit Workspace operations.
S9  Authenticated runtime adapters, interleaved Saves and faithful complete roots.
S10 Incremental capture/construction/Save/stage/publish/install Commit.
S11 Remove superseded core integration/backing/server paths.
S12 Qualify the actual integrated product and report all required family outcomes.
S13 Retire root crates/ and obsolete wiring after qualification passes.

Preserve these product requirements:
- Support both Workspace-per-tool-call and Workspace-per-task. A Workspace may
  serve multiple sequential/concurrent calls, stay live for a long time, and
  Commit incrementally. Command duration is independent of orchestration mode.
- Bind a complete filesystem including .git/index, ignored files, dependencies,
  symlinks, caches and outputs. Repeated mount must not scan/copy/import the full
  base or reinstall/restore files. Reuse the initialized host runtime and daemon.
- One local SQLite overlay database per daemon, initialized before readiness.
  Namespace metadata, physical payload, scratch and custody by Workspace.
  Use SQLite-backed mutable indexes/state instead of custom persistent trees,
  graph engines or unbounded resident namespace mirrors.
- Keep filesystem semantics in Workspace, mutable SQL/payload in overlay, kernel
  adaptation in FUSE and service/process ownership in daemon. Embed host runtime
  adapters in the existing SDK composition; do not revive layerfs-server.
  Reuse cluster-one canonical read/construction/CDC/CAS/delta APIs.
- Implement mount, exec, commit, unmount and status. Exec is ordinary Bash with
  streamed I/O, no automatic runtime/output-lifetime cap, no command-specific
  preparation and no implicit Commit/unmount. Status is bounded read-only state.
  Terminal unmount includes close and automatic cleanup.
- Remove artificial total file/edit/Workspace/Commit/flow/time caps through backed
  and streamed structures. Explicit processing windows and physical/platform/
  format limits remain honest; raising caps or dropping data is not a solution.
- Capture stable existing published state without a bulk snapshot copy. Order
  mutation publication and reply-send attempts; lost replies do not erase already
  published changes. Unpublished dirty mmap stores are outside that frontier.
  Commit captures shared Workspace state, not per-call isolated changes.
- Construct through bounded inputs and Save completion, then StageChanges and
  conditional CommitStaged. Known install advances the base while preserving
  later active changes. Preserve exact refused/conflicted/uncertain custody.
  UpToDate may leave history unchanged.
- Prove last-owner release, automatic bounded cleanup during live/idle periods,
  stable open-unlinked files and repeated success/failure composition. Avoid
  growing generation chains and payload-sized foreground folds.
- Use short fair shared-writer jobs; no whole Exec/Commit database lock. Prove
  unrelated progress, aggregate queues/caches/residency and physical headroom.
- Correct all applicable R1-R8/P1-P14 constraints, including streamed construction,
  faithful import, backed namespace validation/touched/release state and
  incremental topology checks. No silent fallback, error bypass or unproved
  large-workload claim. SQLite scratch does not replace canonical content formats.

Engineering and verification:
- Follow core/AGENTS.md: product-only src/, external tests, <=999 physical lines
  per production file; lib.rs/mod.rs <=200 declaration/delegation-only lines.
  Update affected architecture/API docs with each implementation change.
- No third-party patch/fork/vendor/registry edit or dependency substitution.
  Use locked builds and preserve root .cargo/config.toml ARM64 AEAD inputs.
  Report an unsupported required capability with source/build evidence.
- Preserve one-attempt product operations and exact failure/uncertainty.
  Do not add retries/busy handlers, guessed resend/rollback or silent no-ops.
- Keep disposable overlay and global Store profiles separate. Preserve their
  actual guarantees; add no sync calls to disposable Workspace backing.
- Profile from the first hot path. SQLite claims require both database EXPLAIN
  and correlated runtime DB profiles, including actual work scope. FUSE requires
  request/dispatch/service/wait/cache/reference/buffer/thread observations.
  Analyze worst-case, amortized and cumulative lifetime costs; reject quadratic
  or worse amplification and hidden full scans/input-sized resident state.
- Use macOS ARM64 for current provider/host work and actual Linux ARM64 Docker
  for daemon/FUSE/Bash integration. Establish toolchain/image, /dev/fuse,
  capabilities and mount/teardown readiness. Keep targets inside this checkout.
  Passing host tests does not cover excluded or unbuilt Linux components.
- At each frozen relevant verification identity, run the required covering checks
  once. Diagnose failures from output/source, make a justified repair and run the
  covering checks for the repaired identity. Reuse unchanged qualifying evidence.
  Follow the current core rules for:
    cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets
    cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
    cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
    python3 -B core/tools/check_product_boundary.py
    python3 -B -m unittest discover -s core/tools -p 'test_*.py'
  Add actual owning-platform/integration checks required by the slice.
  Do not revive CI, tools/preflight.sh or an equivalent aggregate gate.

Checkpoint and milestone protocol:
1. Make coherent checkpoint commits as needed within a milestone. Keep partial
   progress explicit; completion requires the plan's actual exit evidence.
   Finish EVERY milestone with a milestone-completion commit recording its final
   implementation, documentation and applicable evidence.
2. For EVERY commit, calculate production LOC from the exact first-parent and
   final staged trees using the same counter/version/scope. Confirm the committed
   tree matches. Exclude unstaged/untracked work and tests/docs/tools; include
   shipped SQL/runtime adapters. Recompute after changed staging/amend/rebase.
3. Include in each commit message:
     Production LOC: <before> -> <after> (delta <signed difference>)
   Record counter/method and separate core/reference/combined totals. Docs-only
   commits report the real unchanged product totals. Classify relocation,
   duplication and legacy retirement honestly. The LOC estimate is not a gate.
4. After EVERY completed milestone, before starting the next, append a tracker
   completion comment and update its checklist item. Include all relevant
   checkpoint commits, delivered behavior, exact checks/evidence, EXPLAIN/profile,
   complexity/resources, per-commit LOC and the next ready milestone.
5. Append checkpoint comments during a long milestone when they preserve
   meaningful progress, evidence or a blocker. Keep comments/receipts append-only.
   Do not rewrite failures or claim completion from a scaffold or a small fixture.
6. Commit hashes remain local-only until separately authorized publication.
   If an issue update cannot be posted, retain the exact pending update locally,
   report the publication gap and continue independent useful work.

Use this tracker update format:
  Milestone: S<n> - <name>
  State: CHECKPOINT / COMPLETE / BLOCKED
  Source: <HEAD and relevant source/build identities; local-only if unpushed>
  Checkpoint commits: <hashes and purposes>
  Delivered behavior: <concrete result>
  Validation: <commands, outcomes, reused evidence and gaps>
  SQLite evidence: <EXPLAIN + runtime profiles, or scoped N/A>
  Complexity/resources: <derived work, counts and resource/debt observations>
  Production LOC: <each commit's totals/delta, scope and counter method>
  Remaining work/blockers: <precise unfinished criteria>
  Next ready work: <milestone/slice and satisfied dependencies>

Before measurements, read the current measurement policy, benchmark rules,
benchmark_agent_report.md and owning core harness/family instructions. Follow
prospectively frozen specifications, locked release identities, one sample per
case/arm, equally enforced cache states, append-only outputs and worktree-local
locks/targets. No warm credit, resampling, enlarged timeouts, reduced workloads
or hidden timed work. Reuse setup/seals and qualifying unaffected evidence.
Commit/capture/snapshot have one construction producer; namespace Init retains
its scoped parallel exception. Harness budgets never become Bash timeouts.

On context compaction/resume, restore progress from the working tree, commits and
tracker; continue the next ready slice without restarting completed work. Required
proof/resource rows must pass; a report containing FAIL/NOT_RUN does not satisfy
acceptance. Retire root crates/ only after integrated qualification. Keep historical
receipts/source pins and shared build config intact.

At completion, reconcile every milestone and final source identity, publish the
final tracker update, and report implemented behavior, per-commit/total LOC,
actual verification/qualification evidence and remaining limitations. Do not
declare completion while a required milestone or proof is unfinished.
```
