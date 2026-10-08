# R7 optimization ledger

> **Status:** Current planning checklist; no release candidate exists.

Dispatch: owner direction 2026-10-09, local `main`, one checkout. Start commit
`2f8e1ec422a40f4220c40315867c72dd3463ff28`; initial product identity `2b4dc28a6`.
All timing selections are exploratory and admission-ineligible. Global Store is
explicit Disposable/WAL/OFF; Durable is **NOT_RUN — disabled by owner until
explicit reauthorization**. Overlay remains MEMORY/OFF/EXCLUSIVE. No push,
publication, PR, worktree, predecessor retirement or reference retirement.

## Stage state

Stage 0 IN_PROGRESS. Three disjoint subagent tracks build prospective registry,
SDK real-binary lifecycle/fixture harness, and passthrough/per-file residency
tools. Lead owns every command receipt, timed sample, decision, record and commit.
Only one Cargo/Docker/test/measurement command holds the existing checkout lock.

| Iteration/checkpoint | Identity | Cells and ratios | Candidates/gates | Next step |
| --- | --- | --- | --- | --- |
| 0, preparation | initial identity above | No sample; ratios unavailable until Stage 0 | No kept product change. Stage 0 diagnostics access gap identified by source, not a measured optimization finding | Build release daemon, finish authentic harness, prepare full fixture once, prove lifecycle, expose mandatory counters before baseline |

## Prospective structural floors

These are direction bounds derived before optimization, not achieved results.
Exact per-opcode/capture-window accounting will refine them before each candidate.

| Ratio | Historical start (not a current timed sample) | Floor/model |
| --- | --- | --- |
| Owner jobs/FUSE request | about 5 | One bounded atomic owner job when overlay state is required; zero when already-held authenticated state suffices. Necessary last-owner release still counted, never hidden |
| Store readers/wholly-local READ | 1 | Zero; local composition must establish no immutable-base demand before reader admission |
| Object demands/cold request | about 1 separately per neighbour | One grouped acquisition per existing bounded admitted plan; canonical dependency/authentication reads accounted separately |
| Owner jobs/captured row | about 31 at 672 rows | Fixed jobs per existing bounded 64-row window plus necessary affected-file construction and publication; no point job per membership question |
| Statements/captured row | about 130 | Indexed useful-row work per bounded window and necessary guarded writes; framing per window/job, not per point |
| Mount-to-Ready/unmount jobs | startup_cost counted | Constant in unrelated root/history size; exact attach/drain/custody jobs remain |
| FUSE requests/syscall | unavailable | At most same-profile P; necessary permission/coherence work retained |
| Wait/service per class | unavailable | No wait behind unrelated work without explicit resource/dependency justification; no numeric latency target invented |

## Decisions

1. Taken under the owner's direction of 2026-10-09: use three concurrent
   disjoint harness implementation tracks and serialize executions through the
   existing checkout lock. Subagents run no timings or Git mutations.
2. Taken under the owner's direction of 2026-10-09: Stage 0 must retain missing
   SDK diagnostics as UNAVAILABLE until an additive legitimate diagnostics path
   is implemented and proven. In-process functional tests cannot replace the
   real-daemon arm. This is required instrumentation, not an accepted speed win.
3. Taken under the owner's direction of 2026-10-09: pinned-image inventory is
   read-only and network-disabled. Missing `node` is a workload prerequisite,
   never permission to silently replace E07 or invoke commands in the fixture.

## Receipts

- [000 environment](000-environment/result.json): pinned image runs, ARM64 Linux
  6.12.76-linuxkit, `/dev/fuse` character device, bash/git available, node absent.
  This is an inventory, not a mounted proof or performance sample.
- [001 fuser provenance](001-fuser-provenance/result.json): authorized patch
  verification passed; no third-party modification.
- `002-linux-release-daemon-build`: initial release build in its own target,
  offline, pinned image and repository ARM flags; pending completion.

## Preservation

The handoff's protected containers, source checkout, temporary installed Store,
untracked handoffs and multi-workspace receipt remain untouched. Removal is
limited to resources created by this stage. Existing raw evidence is immutable.

## Commit accounting

Each commit is compared using pinned `tools/production_loc.py` through
`core/target/rx-count.py`: exact first parent versus staged tree, then committed
tree confirmation. Initial combined 184297; core 118880; active core 76006;
excluded predecessors 38878; excluded integration 3996; root reference 65417.
Harness/docs/test code is excluded. No staged source change may use estimates.
