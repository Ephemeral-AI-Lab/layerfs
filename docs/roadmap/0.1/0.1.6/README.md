# v0.1.6: fast mixed development under retained-history load

Status: implementation roadmap, 2026-09-13. Reviewed by three independent
agents at the owner's request. No new benchmark is implemented, measured, or
qualified by these documents. v0.1.5 release work is separate.

Tracking issue for all six families: [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122).
The owner requests implementation, successful runs of every declared case
(including the three explicit extensions), and honest measurements posted there.
Use the [handoff prompt](handoff-prompt.md) to continue this work.

## Owner requirements

- Read and reuse existing benchmark families before adding cases.
- Exercise 10 and 100 new commits per development branch, not an eight-commit
  substitute. Initial imported state and inherited ancestry are separate counts.
- Load-bearing repository views have maximum 100,000,000 bytes / 5,000
  non-directory paths or 500,000,000 bytes / 30,000 non-directory paths.
- Include small files, the exact 128 KiB boundary, and large files; include
  bulk add/remove, edits, directories, attributes, POSIX links and inode lifetime.
- Exercise actual concurrent development on distinct branches/workspaces.
- Complete a selected regular perf invocation within 15 seconds and its
  separate regular verify invocation within 15 seconds, including per-run
  preparation, receipt publication and teardown.
- Use fixed work and run it promptly. No sleeps, duration-filling loops,
  10-minute soak, or automatic large-history replay.

## Deliverables

| Document | Purpose |
| --- | --- |
| [Overlay and snapshot rules](overlay-snapshot-rule.md) | Product requirements for non-pausing Commit, shared backing, locality, and resource bounds; not a qualification result |
| [Snapshot-Isolated Workspace architecture](overlay-snapshot-architecture-design.md) | Reviewed pre-specification diagrams, component boundaries, complexity analysis, gaps, and open decisions |
| [Snapshot-Isolated Workspace specification](overlay-snapshot-spec.md) | Detailed resulting contracts, selected storage/correspondence mechanisms, interfaces, removal/addition inventory, and existing-benchmark evaluation; explicit correctness/design blockers |
| [Specification adversarial review](overlay-snapshot-spec-review.md) | Capture/Commit speed and correctness counterexamples, corrections, and remaining unqualified decisions |
| [Snapshot implementation plan](overlay-snapshot-implementation-plan.md) | Seven phases tracked by [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124), ending with the full non-#122 benchmark campaign in [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125) |
| [Snapshot implementation handoff prompt](overlay-snapshot-handoff-prompt.md) | Execution prompt: phase-completion issue updates, targeted reruns with a pass ledger, and verified terminal success before closing #124/#125 |
| [Exact #122 benchmark exclusions](benchmark-exclusions-issue122.json) | 36 case-level exclusions for the snapshot campaign; inherited cases in shared families remain included |
| [Family and case plan](benchmark-families.md) | Six families, exact case membership, topology and commit counts |
| [Fixtures](fixtures.md) | Byte equations, path/inode counts, content variance and namespace layout |
| [Mixed workloads](workloads.md) | Five-stage operations, deterministic repetition, concurrent/fork schedules |
| [Execution and verification](execution-and-verification.md) | Environment, public operations, deadlines, oracles, receipts and qualification |
| [Review decisions](review-decisions.md) | Existing coverage, accepted reviewer additions and corrections |
| [Machine-readable cases](cases.json) | Fully expanded planned cases and fixture configuration |
| [Plan check](check_plan.py) | Product-free arithmetic/cardinality/selection check |

The new work is **33 regular cases in six families**: three new families and
three extensions. Twelve of these are the mixed load-bearing cases (three
families × two sizes × two depths). Three additional, explicitly selected
extended cases cover exhaustive payload verification or four concurrent
workspaces. Existing scenarios remain intact and are not counted as new work.

One selected invocation runs one case, one seed, one source arm and one mode.
The complete new regular matrix is 33 perf + 33 verify invocations per seed/arm;
15 seconds is not a promise for the entire matrix. Depth 100 remains in the
regular target matrix; it cannot be moved to extended after a valid timing miss.

## Implementation order

1. Freeze the fixture/schedule generator and independent oracle from this plan;
   expand exact per-path manifests and syscall receipts. Reuse existing helpers.
2. Add shared 15-second supervision and one-Store/multiple-session orchestration.
   Keep the actual Linux helper process finished before each Commit.
3. Implement threshold/history diagnostics, then the 10-commit small-tier mixed
   case, then concurrency and fork schedules. Diagnose selected failures only.
4. Implement the unchanged 100-commit schedules and both repository sizes;
   qualify runtime, completeness, isolation, resource and cleanup evidence.
5. Run the declared candidate/control campaign and explicit extended proofs
   only when selected. Publish observed outcomes, including valid misses.

Before benchmark implementation or sample collection, commit the specification,
as required by [benchmark rules](../../../general/benchmark_rules.md). Issue
#122 binds all six new/changed families. Exact baseline/candidate source seals
and reference machine fingerprint remain implementation prerequisites. The
roadmap and handoff are local, uncommitted artifacts until the implementing
agent freezes them; no benchmark outcome or release is claimed here.

```sh
python3 docs/roadmap/0.1/0.1.6/check_plan.py
```

The check validates the plan, not product correctness or 15-second feasibility.
Ordinary Init/Commit deduplication/compression are in scope. Explicit compaction
remains removed under the [v0.1.5 decision](../0.1.5/compaction-removal.md).
