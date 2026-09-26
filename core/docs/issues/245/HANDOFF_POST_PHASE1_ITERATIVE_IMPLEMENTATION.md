# #245 iterative implementation handoff prompt

> **Status:** Current planning checklist; no release candidate exists.
>
> Copy the assignment below into the implementation agent's new task. The
> [implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md) and
> [mini benchmark contract](SHELL_BRAINSTORM_MINI_V1.md) are committed in the
> draft research [PR #257](https://github.com/Ephemeral-AI-Lab/layerfs/pull/257).
> Its `f74dbe77da12fa533587be8a578375bce3f19373` product source pin is
> historical: inspect the current source before editing.

## Owner revision, 2026-09-26: implementation and focused tests only

This revision supersedes every *measured-gate* requirement below for the
remaining phases. The work is an **implementation task**: the deliverable is
product code that is correct by construction and covered by focused tests.
Full-size public benchmark rows are **not** required to close a phase.

- No `#248` 4,097-separated-write row, no `#256` 129/257/1,025 changed-file
  rows, no `#258` mounted move case, no `#249` overlapping-Exec case and no
  `#219` configured-count case has to be run, sampled, registered or frozen
  before its phase is complete. The
  [mini runner](SHELL_BRAINSTORM_MINI_V1.md) stays unimplemented and unused.
- Evidence for a phase is the focused test selection that covers the changed
  code — external `tests/` cases in `core/crates/<package>/tests/` that drive the
  real public API, plus the release build, Clippy, `fmt` and the product-boundary
  guard. Route fixtures that need a prepared Store, a daemon image or a
  privileged mount are optional, not prerequisites.
- Speed, cache, quota and amplification numbers are not admission criteria for
  these phases. Do not sample wall time to decide a phase, and do not add a
  benchmark harness to satisfy this document.
- The product contract itself is unchanged: one public
  `WorkspaceApi::mount` → one opaque `WorkspaceApi::exec` → `/bin/sh -c` →
  ordinary POSIX/FUSE callbacks → optional explicit `WorkspaceApi::commit`, with
  no command classification and no benchmark-specific product shortcut. It is a
  contract on the code, not a workload that must be measured.
- Keep the per-commit production LOC comparison, the release-only test builds,
  the file-size limits and the architecture-document-in-the-same-commit rule.

This revision also supersedes the "Required completion evidence" column of
[the implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md), whose per-phase
rows are public benchmark gates, and its closing list of proof gates. Where the
body below or that spec says a phase "requires" a measured row, read it as "the
implementation must make that row possible"; the row itself is optional and, if
ever taken, still needs its own prospectively frozen contract.

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

1. Name the issue, source commit, exact behavior and the focused test that
   will cover it. Trace all callers and the shared root cause before editing.
   If a benchmark scenario is ever registered later, freeze it before its first
   sample; old receipts remain append-only.
2. Implement one reviewable slice. Update the affected `core/docs/architecture/`
   document **in the same commit** if a boundary, format, algorithm or named
   bound changes. Keep production files under 1,000 physical lines and
   `lib.rs`/`mod.rs` under 200; no third-party patch or new dependency for an
   existing capability.
3. Build **locked release binaries only** (`cargo +1.85.1 ... --release
   --manifest-path core/Cargo.toml --locked`) in this worktree's target,
   including every Rust test executable used as evidence. Never run, compare or
   promote a debug binary. Record the source commit, the test binary and the
   profile; reuse a matching sealed release build instead of rebuilding it.
4. Run the smallest covering checks: the focused locked Core tests for the
   changed code, plus release Clippy, `fmt` and the product-boundary guard and
   its self-tests. A test command must finish in **at most 30 seconds**;
   prebuild the release test binaries separately if compilation obscures test
   wall. Do not raise a timeout, worker count, quota or cache allowance to turn
   a miss into a PASS. Report any broader check that did not fit rather than
   substituting a narrower one silently.
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
   plus #245 with the commit link, the route or API the change is on, the exact
   checks and their status, the limits and the next phase. A phase closes on
   its focused tests, not on a speed row.
7. Continue directly to the next phase. **Do not ask the owner for
   authorization to continue, commit, update issues or stop.** If an actual
   external blocker prevents a phase, record its evidence in the issue and work
   on any independent slice. If no meaningful work remains possible, provide a
   precise blocked handoff; never claim an unfinished phase done.

### Product implementation phase order

Each phase closes on its implementation plus the focused tests that cover it.
The right-hand column names **what the code must do**, and the tests that prove
it; it is not a benchmark row to register.

| Phase | Implementation focus | Closure: code property and its focused tests |
| ---: | --- | --- |
| 1 | Shared #248/#256 page and payload ownership: charged allocation, indexed payload lookup, work-driven reclamation and exact pinned-root custody. | Routine maintenance work does not grow with earlier acquisitions; quota and failure paths keep every owner. **Done** (`b298d175d`, `7e6cac877`): indexed registry, release-list reclamation, `routine_scans`/`lookup_scans` counts, four external tests in `tests/backing_ownership.rs`. |
| 2 | #248 file extent and frozen Commit path: height transitions, monotone cursors, short writer-gate holds, bounded C1 replay. | The extent tree stays height-uniform at every count and pack shape; a splice's page visits are `O(H + touched)` rather than `O(H per leaf)`; no Commit phase holds the metadata writer gate across a frozen walk or a transfer pull; C1 consumes a replayable stream. **Partially done** (`5a9a9fc5f`): level-preserving rebuild, balanced packing and the exact 4 GiB bound, covered by `tests/pieces_sequence.rs`. **Remaining:** sibling rebalancing so a collapsed child never refuses, monotone cursor advance, gate-hold narrowing, C1 replay confirmation. |
| 3 | #256 keyed namespace, live pins, prepared stream and C1 ordering. | Path-local binding/tombstone mutation, no 128-name or 32 KiB admission, ordered cursor traversal that visits each reached leaf once, and one published head. Focused tests: create/rename/delete/recreate across a wide directory, a many-identity generation, and an ordered whole-tree walk. |
| 4 | #258 stable canonical origin for inherited directory moves. | A base-resident directory moves without a subtree copy-up, descendants resolve through a stable origin, invalid destinations are refused before publication, and old paths disappear. Focused tests: inherited move, move-back, held handles, deep/invalid paths. |
| 5 | #249 multi-Workspace daemon registry and lightweight concurrent Exec; #219 operator Workspace-count policy. | Per-Workspace Commit slot only, no whole-Exec timer, no fixed Exec count, lease cleanup, and a selected Workspace count of 1/2/3 enforced by the daemon registry. Focused tests: registry admission and refusal, two Workspaces, overlapping Exec, count policy. |
| 6 | #245 integration: combined file-plus-namespace generation, two sequential Commits against the immediate predecessor, G2 writes retained during G1 construction, exact old/new heads. | Focused tests over the public API; no benchmark artifact required. |

### Tests, not benchmarks

Verification for these phases is **focused tests in the package's `tests/`
directory** that drive the real public API over real Linux backing, run as
locked release builds. A counter is welcome when it makes a claim checkable —
page visits, records examined, rows emitted, bytes charged — because a count is
reproducible and cheap; it is recorded in the receipt as a count, never as a
speed claim.

- Do **not** build the 13-cell mini runner, register new scenarios, prepare Store
  masters, build daemon images, mount FUSE, or take full-size rows to close a
  phase. Those artifacts remain optional and, if ever taken, need their own
  prospectively frozen contract with the repository's cache and budget rules.
- Do **not** close a phase from a cache-ineligible or unqualified timing row, and
  do not add a benchmark-only path to product code to make one possible.
- Keep the [joint tree study](JOINT_248_256_TREE_RESEARCH.md) and
  [resource plan](RESOURCE_CONSTRAINT_LIFT_PLAN.md) as design input, not as a
  proof obligation; their estimates are not a LOC budget. Reuse the existing
  Arena, keyed pages, extent splice, C1 builder and charged backing.
- Keep the per-commit production LOC comparison, the release-only test builds,
  the 999-line/200-line file limits, and the architecture document update in the
  same commit. Tests, docs and receipts stay outside production LOC.

Send concise progress updates during long work. End each completed turn with
the current product or verification phase, commit, issue update, checks with
exact status, and the next actionable step. Continue autonomously until every
required product phase and verification gate is complete or a real blocker is
documented with no independent work left.
