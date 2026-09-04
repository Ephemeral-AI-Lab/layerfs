# LayerFS 0.1.3

> **Benchmark infrastructure workstream (2026-09-05), [#45](https://github.com/Ephemeral-AI-Lab/layerfs/issues/45):** See the
> [benchmark infrastructure optimization specification](benchmark-infrastructure-optimization-spec.md)
> for Docker-only product/preparation, no data mounts, three execution modes,
> fresh/clone setup, compact logs, and the sequential family migration checklist.
> This new workstream does not restart withdrawn Phase 1 verification or claim
> product performance or release acceptance.

> **Phase 1 scope updated 2026-09-04:** Apply the
> [15-second runtime suppression policy](phase-1-runtime-suppressions.md).
> Fourteen specified case/subset combinations are suppressed; any further
> performance sample exceeding 15 seconds permanently suppresses its combination
> for Phase 1. Suppression is reported separately from passing coverage.

> **Completion policy updated 2026-09-04:** The user requires functional failures
> to be fixed in Phase 1. The [failure-repair amendment](failure-repair-amendment.md)
> supersedes earlier completion/deferral wording here and in linked contracts;
> optional performance and storage optimization remains Phase 2.

> **Status:** Current planning checklist; no release candidate exists.
> Twelve canonical family specifications: eleven performance families and
> one proof-only Workspace reliability family. Workload adapters are not yet
> implemented. Freeze source, fixture, oracle, runner and evidence identities
> before admission; no numbers below are measured performance results.

## Goal

Prove bounded, reliable whole-Workspace behavior and useful CAS/CDC storage
reuse through the existing public SDK, managed daemon, real FUSE and one Store.
Cover small and dense changes, bulk churn, complete reads, real agent tools,
repeated execution, failure handling and storage growth over time.

Use one Branch in each measured trajectory. Bounded retained-Commit history
is included for deduplication
and essential session correctness; multi-Branch sharing, fan-out, Add/promotion,
conflicts and broader history-query scaling remain [v0.1.4](../0.1.4/README.md).
The released crash/power-loss durability limitation remains explicit.

## Delivery stages and issue structure

### Phase 2 implementation handoff

Product scaling work is tracked by [#38](https://github.com/Ephemeral-AI-Lab/layerfs/issues/38)
and children #41–#44, after the completed
[#45 infrastructure handoff](https://github.com/Ephemeral-AI-Lab/layerfs/issues/45).
Use the [shared-code layout and refactoring plan](phase-2-shared-code-layout.md),
[mechanism-adoption audit](phase-2.1-mechanism-adoption-audit.md), and
[Workspace admission complexity analysis](workspace-admission-complexity.md).
The [API/algorithm simplification audit](api-algorithm-simplification-audit.md)
records the before/after targets and distinguishes implementation consolidation
from public method removal: zero immediate SDK deletions, one possible versioned
Commit-variant removal, and one compatibility-gated `PinRead` wire candidate.
These are source-based plans, not new benchmark results. They distinguish
already-adopted #40 mechanisms from remaining transfers and keep native and
Workspace implementation ownership separate while serializing measurements.

Use the [Phase 1 agent handoff prompt](phase-1-handoff.md) to coordinate execution
of all Stage 1 issues with performance-first collection and limited verification.

The [bulk-create/delete optimization notes](bulk-create-delete-optimization-notes.md)
record the 2.7-second v0.1.1 initialization reference, proposed reuse of its
construction pipeline, and aggressive targets for later optimization discussion.

Central roadmap: [#21](https://github.com/Ephemeral-AI-Lab/layerfs/issues/21).
Its fourteen sub-issues are shared infrastructure
[#22](https://github.com/Ephemeral-AI-Lab/layerfs/issues/22), one issue for each of the twelve families,
and consolidated initial results [#35](https://github.com/Ephemeral-AI-Lab/layerfs/issues/35). Scenario IDs stay inside their family issue; do not
create 130 individual benchmark issues.

**Stage 1:** commit/freeze specifications, build on the existing infrastructure,
qualify fixtures and oracles, then execute and record each family's initial
performance and correctness outcomes. Product optimization is deferred. The
infrastructure issue comes first; family implementation can then proceed
independently, while resource-sensitive timing remains isolated.

**Stage 2:** review the consolidated evidence, choose actual shared root causes,
and create focused product correctness/performance/storage improvements.
Re-measure identical scenarios against the retained baseline. Release
qualification and publication follow; Stage 1 completion does not close the
central release issue or establish a passing release candidate.

See [Stage 1 completion rules](testing-rules.md#stage-1-build-and-collect-the-initial-baseline).
Valid slow results and product failures are findings to retain. Unimplemented
cases, missing observability and unexecuted slots keep the owning build issue
open. The family release gates remain unchanged even when its initial-baseline
issue is complete.

## One file per family

These twelve files are authoritative for family membership. The
[testing rules](testing-rules.md) own common infrastructure, preparation, tiers,
seeds, size bounds, timing, verification and fast-iteration requirements.

| # | Family specification | Family ID | New timed cases | Standalone proof recipes | Stage 1 issue |
| ---: | --- | --- | ---: | ---: | --- |
| 1 | [Payload creation and random reads](payload-create-read.md) | `payload_create_read` | 8 | 0 | [#23](https://github.com/Ephemeral-AI-Lab/layerfs/issues/23) |
| 2 | [Tiny-file operations and bulk churn](tiny-file-churn.md) | `tiny_file_churn` | 20 | 0 | [#24](https://github.com/Ephemeral-AI-Lab/layerfs/issues/24) |
| 3 | [Directory construction and whole-Workspace reads](directory-construction-traversal.md) | `directory_construction_traversal` | 12 | 0 | [#25](https://github.com/Ephemeral-AI-Lab/layerfs/issues/25) |
| 4 | [Git workflow](git-tool-workflow.md) | `git_tool_workflow` | 4 | 0 | [#26](https://github.com/Ephemeral-AI-Lab/layerfs/issues/26) |
| 5 | [Populated subtree mutation](namespace-mutation.md) | `namespace_mutation` | 4 | 0 | [#27](https://github.com/Ephemeral-AI-Lab/layerfs/issues/27) |
| 6 | [Workspace change locality](workspace-change-locality.md) | `workspace_change_locality` | 16 | 0 | [#28](https://github.com/Ephemeral-AI-Lab/layerfs/issues/28) |
| 7 | [Complete agent work episodes](mixed-load-bearing-workload.md) | `mixed_load_bearing` | 4 | 0 | [#29](https://github.com/Ephemeral-AI-Lab/layerfs/issues/29) |
| 8 | [Cross-file CAS deduplication](dedup-cross-file.md) | `dedup_cross_file` | 10 | 0 | [#30](https://github.com/Ephemeral-AI-Lab/layerfs/issues/30) |
| 9 | [CDC locality and resynchronization](dedup-cdc-locality.md) | `dedup_cdc_locality` | 20 | 1 | [#31](https://github.com/Ephemeral-AI-Lab/layerfs/issues/31) |
| 10 | [Incremental Workspace content reuse](dedup-workspace-reuse.md) | `dedup_workspace_reuse` | 12 | 0 | [#32](https://github.com/Ephemeral-AI-Lab/layerfs/issues/32) |
| 11 | [Single-Branch history storage growth](dedup-branch-history.md) | `dedup_branch_history` | 20 | 0 | [#33](https://github.com/Ephemeral-AI-Lab/layerfs/issues/33) |
| 12 | [Workspace reliability and session endurance](workspace-reliability.md) | `workspace_reliability` | 0 | 12 | [#34](https://github.com/Ephemeral-AI-Lab/layerfs/issues/34) |
| **Total** | **12 families** | **11 performance + 1 proof-only** | **130** | **13** | — |

Each new timed ID has three prescribed samples: **390 initial-baseline sample
slots** in Stage 1, with a matching candidate campaign when optimizing later.
Exact verification is separate and required for every distinct admitted
fixture/schedule variant. The reliability family expands its twelve recipes
into 28 named subcases; the CDC proof contains its own declared boundary
cohorts. Proof recipes are not multiplied by four sizes, and their count is not
the count of assertions, seeds, fault points or all verification executions.

Cross-file CAS uses one common one-file anchor for three profiles, avoiding
three identical executions. Other performance curves use four explicit tiers.
The previous 48/56/72-case discussion totals are superseded by this complete
membership. The [earlier coverage review](coverage-review.md) is historical
rationale and cannot override these specifications.

## Inherited coverage

| Released family | Frozen cases / controls | v0.1.3 disposition |
| --- | ---: | --- |
| `edit_length_preserving` | 12 | Retain singular SDK operation/fixture semantics |
| `edit_length_changing` | 32 | Retain originals as history; version five capped-result replacements for future capped runs |
| `edit_canonical_chunk_count` | 12 | Retain canonical outcome and SDK route semantics |
| `init_namespace` | 4 | Retain released 100/1,000/10,000/100,000-file profiles |
| `store_footprint` | 3 | Retain unique-content, metadata-cardinality and large-object controls |
| **Released total** | **63** | **Account separately from 130 new timed cases** |

Two older 32 MiB create/read anchors appear in the payload document and retain
their own lifecycle, repetition and verification identities; they are not new
cases. Other historical rows follow the
[0.1.x benchmark contract](../benchmarking.md). The superseded POSIX/temp-copy
edit families remain archival, not active SDK admission. Never count inherited
namespace controls twice because several deduplication analyses reference them.

The five inherited 500 MiB growth inputs would exceed the file cap after editing.
The [testing rules](testing-rules.md#inherited-evidence-and-release-scope) specify
new capped-result definitions, with shorter deterministic inputs prepared
outside timing. Do not run oversized originals under this cap, rewrite old raw
evidence, claim unchanged complete-family admission, or pool unlike definitions.

## Shared load and preparation

Use `[1, 10, 100, 500]` and the existing binary units. Each workload file is at
most **500 MiB**; total logical workload content is strictly below **1 GiB**
at every initial, intermediate and final state. Temporary files, Git objects,
sparse logical lengths, and conservative hard-link alias lengths count. Physical
Store/spool/cache/harness disk budgets are separate and explicitly reported.

The [shared tree](testing-rules.md#shared-workspace-fixture) uses 200 files per
1 MiB shard, reaching 100,000 files and 500 MiB. Its fixed wide directory and
128-component spine exercise paging and depth without a Cartesian shape matrix.
Payload cases reuse released flat-file fixtures; history uses one small fixed
tree so 500 retained Commits still fit the represented-history bound.

Reuse `fs-bench-pro`, the shared workload helper, existing runner/custody/report
machinery, qualified generators and immutable prepared inputs. New family
adapters may extend shared helpers where necessary; they must not duplicate
the framework. Prepare only the requested case/tier's dependencies and acquire
each compatible master once. Every sample receives an independent writable
copy or clone. Measured creation/import/history still performs every operation.

## Fast iteration and qualification

An ordinary selected warm-prepared run aims for **1–5 seconds**, subject to the
untouched baseline. This is a development objective, not a promise that every
500-tier case, full family or exhaustive proof completes in a few seconds.
Report command wall and preparation/cache costs alongside inner product times.

1. Product-free identity, schedule and byte-bound self-check.
2. One selected case, seed and arm plus the focused regression check.
3. Inspect counters/phases, fix the shared cause, and rerun the selected case.
4. Expand to relevant siblings only when needed to resolve remaining risk.
5. Collect complete affected-family performance when the candidate is ready.
6. Run independent verification and required extended qualification before
   release; retain every valid slow result and failure.

History, heavy fault-boundary and sustained-session cases are explicitly
selectable extended work. No default invokes the whole release or endurance.
Required extended cases cannot be omitted from full qualification. Performance
never runs added benchmark manifests, verification hashes, object census,
reopen or injection. Intrinsic CAS/Git hashing remains measured product work.
A report-only edit regenerates reports from raw evidence rather than rerunning
product operations. Preparation and verification do not run concurrently with
latency measurement unless an explicitly frozen resource profile permits it.

Correctness and resource bounds precede measurement. Numerical targets and
separate preparation/selected/performance/verifier deadlines that need a
baseline must be frozen before candidate optimization or sampling. Do not
inflate timeouts or silently shorten workloads after observing a failure.

## Registration and completion

Each family's implementation issue must bind its complete definition, exact
fixtures/oracles, source/build identities, public route, timings, samples,
resource caps and budgets under the
[general benchmark rules](../../../general/benchmark_rules.md). This planning
index records planning and issue ownership; issue creation does not register
scenarios, run benchmarks or establish a passing release gate.

- [ ] One canonical specification per family; obsolete namespace-dedup and
  link-timing drafts replaced, with links and counts consistent.
- [ ] Reuse existing infrastructure and lazy compatible preparation; include
  cache invalidation, interruption/corruption and sample-isolation checks.
- [ ] Qualify unique scenario IDs, complete membership, nested schedules and
  every intermediate workload byte bound before admission.
- [ ] Verify all unchanged paths and required intermediate observations with
  independent oracles; preserve authentic SDK versus POSIX/tool routes.
- [ ] Record all 390 initial-baseline sample outcomes and required proof variants/subcases,
  with separate performance, correctness, resource, cleanup and custody status.
- [ ] Run capped inherited replacements and applicable regressions with explicit
  identity; retain all prior evidence without silent relabeling.
- [ ] Demonstrate complete required ordinary and extended coverage, with no
  unexplained regression or unsupported storage/durability claim.
