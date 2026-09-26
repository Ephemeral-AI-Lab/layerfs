# #245 iterative implementation handoff prompt

> **Status:** Current planning checklist; no release candidate exists.
>
> Copy the assignment below into the implementation agent's new task. The
> [implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md) and
> [mini benchmark contract](SHELL_BRAINSTORM_MINI_V1.md) are committed in the
> draft research [PR #257](https://github.com/Ephemeral-AI-Lab/layerfs/pull/257).
> Its `f74dbe77da12fa533587be8a578375bce3f19373` product source pin is
> historical: inspect the current source before editing.

## Assignment to paste

Implement the open post-Phase-1 #245 work iteratively. The owning issues are
[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) (file runs and
Commit), [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256)
(namespace count and streaming), [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258)
(base-resident directory move), [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249)
(multiple Workspaces and count-free concurrent Exec), and
[#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
(`max_workspaces_per_sandbox`). [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245)
owns the integrated result. Treat Phase 1 A–F and #252 as completed
prerequisites; preserve their receipts and exact accepted semantics.

Read `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
`docs/general/documentation-policy.md`, and
`core/benchmark/fs-bench-pro/AGENTS.md`. Then read
`core/docs/issues/245/POST_PHASE1_IMPLEMENTATION_SPEC.md`,
`JOINT_248_256_TREE_RESEARCH.md`, `RESOURCE_CONSTRAINT_LIFT_PLAN.md`,
`FINAL_DELTA_COMMIT_COMPLEXITY.md`, `LOAD_BEARING_CASE_REVIEW.md`, and
`SHELL_BRAINSTORM_MINI_V1.md`. Read the live issue bodies and source, since
the documents are source-pinned plans, not proof that later code is unchanged.
Use an isolated implementation worktree/branch based on the latest compatible
source, keeping the docs research PR reviewable. Preserve other owners' work.

### Product contract

The only measured mutation surface is public `WorkspaceApi::mount` → one
opaque `WorkspaceApi::exec(command)` → `/bin/sh -c` → ordinary POSIX/FUSE
callbacks → explicit `WorkspaceApi::commit` for mutating cases. The same API
must accept arbitrary shell commands. Do not classify command text, detect
benchmark IDs in product code, call private Workspace/Bridge/C1 mutation
methods in a benchmark driver, introduce a special editor or ioctl route, or
make a benchmark-specific product shortcut. Read-only `find`, `grep`, and
printed-output cases have no Commit.

Implement the smallest coherent change that passes each phase's gate. Reuse
the existing Arena, keyed pages, C1 sorted builder and charged `FileBacking`
where they suffice; do not create a generic B+ engine, packed-page format or
new C1 file builder merely because the research names a possible module.
Preserve atomic private-root publication, G1/G2 custody, exact canonical
identity for previously accepted inputs, one expected-head publication,
unknown-outcome retention and incremental Commit against the immediately
preceding successful head. One Commit/Stage submission at a time **per
Workspace** is the only logical serialization rule; different Workspaces may
progress independently subject to actual Store/resource admission.

### Iteration loop — repeat after every completed phase

1. Name the issue, source commit, exact behavior and public proof for this
   phase. Trace all callers and the shared root cause before editing. Freeze
   any new benchmark scenario and resource/cache contract before its first
   sample; old receipts remain append-only.
2. Implement one reviewable slice. Update the affected `core/docs/architecture/`
   document **in the same commit** if a boundary, format, algorithm or named
   bound changes. Keep production files under 1,000 physical lines and
   `lib.rs`/`mod.rs` under 200; no third-party patch or new dependency for an
   existing capability.
3. Build **locked release binaries only** (`cargo +1.85.1 ... --release
   --manifest-path core/Cargo.toml --locked`) in this worktree's target. All
   new SDK/FUSE benchmark drivers, independent verifiers, daemon images and
   Rust test executables used as evidence must be release builds. Never run,
   compare, or promote a debug binary. Record binary/source/profile seals;
   reuse a matching sealed release build instead of rebuilding it.
4. Run the smallest covering checks. A test command must finish in **at most
   30 seconds**; a timeout or slower test is not an accepted PASS. Prebuild
   release test binaries separately if compilation obscures test wall. The
   benchmark's stricter complete-command 15 s / prospectively declared 25 s
   exception and independent verifier under 10 s still apply. Do not raise a
   timeout, worker count, quota or cache allowance to turn a miss into PASS.
   Run the relevant locked Core test, release Clippy, format and product-boundary
   checks required by repository policy, splitting focused test selections
   where needed and reporting any broader check that did not fit.
5. **Do not rerun a successful test after a minor unrelated fix.** Keep a
   phase-local list of passed checks and the source/behavior they cover. After
   a fix, rerun the failing check and only passed checks whose covered code or
   assumptions changed; run an exact-identity final check only where a frozen
   admission contract requires it. Do not resample an unchanged benchmark arm
   or rerun a passing suite for reassurance. Record the identity and limits
   of any retained proof rather than calling an older result a new PASS.
6. When the phase gate passes, compute exact first-parent **production LOC
   before → after (signed delta)**, with Core and legacy subtotals, using the
   same `core/tools/production_loc.py` method on parent and staged tree.
   Commit that completed slice, push it, and update its owning GitHub issue
   plus #245 with commit link, public route, exact checks/receipts, limits,
   nonpassing rows and next phase. Do not mark an issue complete from a
   cache-ineligible speed row or from an internal-only test.
7. Continue directly to the next phase. **Do not ask the owner for
   authorization to continue, commit, update issues or stop.** If an actual
   external blocker prevents a gate, record its evidence in the issue and
   work on any independent slice. If no meaningful work remains possible,
   provide a precise blocked handoff; never claim an unfinished phase done.

### Product implementation phase order

| Phase | Implementation focus | Required completion evidence |
| ---: | --- | --- |
| 1 | Shared #248/#256 page and payload ownership: charged allocation, indexed payload lookup, work-driven reclamation and exact pinned-root custody. | Count-driven public-route diagnostic shows no per-write scan of all earlier acquisitions; quota/failure paths preserve owners. |
| 2 | #248 file extent and frozen Commit path: height transitions, monotone cursors, short writer-gate holds, bounded C1 replay. | Public 4,097 separated writes in one Exec, Commit, exact old/new bytes/roots, writer progress through declared Commit phases and registered resource/time receipts. |
| 3 | #256 keyed namespace, live pins, prepared stream and C1 ordering. | Public 129/257/1,025 changed-file/name cases, exact whole-tree verifier and one-head publication; no fixed count before resource admission. |
| 4 | #258 stable canonical origin for inherited directory moves. | Mounted base-resident subtree move, descendant reads, temp replacement, old-path absence, old-head readability and atomic invalid-path refusal. |
| 5 | #249 multi-Workspace daemon registry and lightweight concurrent Exec; #219 operator Workspace-count policy. | Multiple mounts, overlapping Exec within/across Workspaces, no whole-Exec timer or fixed Exec count, one Commit slot per Workspace, exact cleanup, configured Workspace counts 1/2/3. |

### Verification and benchmark tests — separate from product implementation

Run focused release tests and required public proof as each product phase
becomes ready. Build the 13-cell mini runner specified by
`SHELL_BRAINSTORM_MINI_V1.md` as **benchmark/test code**, after the relevant
product path is implemented. The registry is already frozen. The runner may
reuse the public SDK driver and independent verifier, but it must send each
command unchanged through `WorkspaceApi::exec`; use one independent master
clone per cell and retain one append-only attempt per case. Its uncontrolled
cache wall numbers remain `INELIGIBLE`. A mini sample is not a #248 or #256
full-size gate. Keep this runner, its fixtures and receipts out of production
source and production LOC; commit and report that verification artifact
separately with production LOC delta 0. Push its commit and update the owning
issue plus #245 with every sampled, failing and unrun row.

Complete #245's **integration verification** after the product phases: combined
file-plus-namespace generations, two sequential Commits against the immediate
predecessor, G2 writes retained during G1 construction, exact old/new heads,
and every registered load-bearing PASS/FAIL/INELIGIBLE/NOT_RUN row. A failing
verification row sends the agent back to the owning product phase for a
focused fix; it does not become a new implementation phase or a reason to
weaken the benchmark.

At each phase, prefer counts of actual FUSE callbacks, page/ledger I/O,
payload records, C1 work, spool/private/Store bytes, CPU and resident/file
cache domains over repeated wall samples. The source analysis estimates for
#256 are *not* a LOC budget: the proposed line items sum to roughly 2,990,
but the actual implementation should reuse existing structures and report
measured LOC per commit.

Send concise progress updates during long work. End each completed turn with
the current product or verification phase, commit, issue update, checks with
exact status, and the next actionable step. Continue autonomously until every
required product phase and verification gate is complete or a real blocker is
documented with no independent work left.
