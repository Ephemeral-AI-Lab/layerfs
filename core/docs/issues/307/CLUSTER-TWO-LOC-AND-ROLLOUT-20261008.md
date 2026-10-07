# Cluster-two completion: expected production LOC and rollout

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Prepared 2026-10-08 against local `main`
> `e765b39af0b40ebddb14556dafb2497d6219fa62`.
> Current counts are exact; future ranges and checkpoints are planning estimates.
> This document starts no implementation, build, measurement or retirement.

The recommended finished product is roughly **65,000–72,000 production LOC**,
including both clusters, with about **68,000** as a planning reference. Cluster
two itself is roughly **24,000–29,500** of that total. These are source-reading
estimates, not a budget enforced by shrinking code or removing validation.

The [complete file-level destination](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md)
owns the proposed homes. This plan uses the owner's latest direction: the SDK
exposes Project/Workspace/Sandbox APIs; ordinary execution belongs to the
caller/sandbox runtime; the filesystem daemon owns no Exec supervisor, launcher,
per-Exec cgroups, command registration or Exec stream protocol. Global Store
execution remains explicit Disposable/WAL/OFF. Branch publication uses the
approved overwrite policy and large Saves use explicitly counted block refills.

## 1. Exact current source counts

The pinned counter reads the exact committed product source plus shipped runtime
SQL. It excludes tests, examples, fixtures, benchmarks, development tools,
documentation, manifests, generated files and third-party code. Per-package
counts and classification are in the
[current count receipt](checks/cluster-two-loc-rollout-20261008/02-current-package-loc.json).

| Scope | Current production LOC |
| --- | ---: |
| Active cluster-one libraries: Content, Storage, History, Persistence, Project, Telemetry | 39,630 |
| Active cluster-two product: Overlay, Workspace, Daemon, Bridge, SDK | 17,892 |
| **Active product** | **57,522** |
| Excluded core predecessors | 36,325 |
| Excluded core integration: old FUSE, Sandbox and Server | 6,549 |
| **All core product source, including excluded code** | **100,396** |
| Root reference implementation | 65,417 |
| **Combined current source** | **165,813** |

The core exclusion subtotal is 42,874: 36,325 predecessors plus 6,549 old
integration. Its removal requires replacement coverage. The 65,417-line root
reference remains through final qualification and is retired at S13.

## 2. Remaining implementation estimate

Each row is an incremental active-product estimate. FUSE and Sandbox are new
active implementations; their old excluded source is already in the retirement
subtotal above. SDK folder moves are relocation with zero growth; actual API
wrappers and backend integration are counted separately in the row below.

| Owner | Active LOC now | Estimated remaining growth | Estimated active LOC after | Required work behind the range |
| --- | ---: | ---: | ---: | --- |
| Content | 19,085 | +1,000 to +2,500 | 20,085–21,585 | Remaining backed validation, incremental namespace topology and owning bounded change inputs |
| Overlay | 7,542 | +700 to +1,800 | 8,242–9,342 | Indexed native lookup/group ownership, captured namespace cursor jobs, revocation/retirement and debt accounting |
| Workspace | 3,902 | +1,600 to +3,200 | 5,502–7,102 | Consistent read plans and complete captured namespace normalization/assembly over existing file construction |
| Daemon | 4,427 | +900 to +1,800 | 5,327–6,227 | Native composition, request admission/fair steps, completion notifiers, read service, full filesystem drain and control integration |
| Bridge + SDK | 2,021 | +500 to +1,000 | 2,521–3,021 | Native mount/drain records and thin Project/Workspace/Sandbox facades; no host Store service or custom Exec protocol |
| Replacement FUSE | 0 active | +1,300 to +2,000 | 1,300–2,000 | fuser conversion, checked replies, direct mount/abort/plain detach and native observations |
| Replacement Sandbox | 0 active | +1,100 to +1,800 | 1,100–1,800 | Actual Docker lifecycle/identity/visibility and ordinary runtime execution/streams |
| Other cluster-one libraries | 20,545 | +0 to +500 | 20,545–21,045 | Explicit allowance for any required public-port/composition corrections; existing implementations reused |
| **Total** | **57,522** | **+7,100 to +14,600** | **64,622–72,122** | Planning arithmetic; use the rounded range rather than interpreting the bounds as exact forecasts |

At the planning midpoint this is 68,372, approximately 68,000. Cluster-two
growth is 6,100–11,600, yielding 23,992–29,492 for that scope. Cluster-one growth
is 1,000–3,000, yielding 40,630–42,630. Both sums reconcile with the total.

The largest uncertainty is complete namespace normalization/topology and exact
ownership/disposal across the native adapter. Re-estimate these when their
deepest-file implementation plans are concrete. Required validation, exact
failures, streaming bounds, permissions and lifetime checks are never removed
to fit the range. The 108 proposed destination paths include small declarations
and focused splits; file count is not a LOC multiplier or a requirement to create
108 independently substantial implementations.

The earlier 20–27k cluster-two range predates the current implemented overlay,
captured construction, direct Store and accounting scope. The range here starts
from actual current counts and replaces it for this completion proposal. No
performance result or historical source count is relabelled.

## 3. Growth versus retirement

| Point in the rollout | Approximate combined production LOC | What changes |
| --- | ---: | --- |
| Current tree | 165,813 exact | Includes active product, excluded core and root reference |
| Full replacement with all old source still retained | 172,913–180,413 | Add the estimated 7,100–14,600 active implementation lines |
| Replacement covered; all 42,874 excluded core lines retired | 130,039–137,539 | Completed core product plus the 65,417-line root reference |
| S12 accepted; root reference retired at S13 | 64,622–72,122 | Completed product without obsolete reference implementation |

Covered core predecessor removals can happen earlier in individual checkpoints,
so the second row is a coexistence accounting scenario, not a promised peak.
The final net decrease from today's combined source is 93,691–101,191 lines,
principally retirement of obsolete implementations. Report that as retirement,
never as an algorithmic simplification or a measured speedup.

The host-mediated transport's 8,270-line retirement is already reflected in
today's baseline and is not subtracted again. Moving the SDK facade files changes
their home and public organization, not the source-size total by itself. No LOC
credit is assigned to the proposed removal of daemon Exec components: they were
never implemented in the current active daemon.

## 4. Combined implementation rollout

One continuous S8/S10 effort is recommended, with reviewable local checkpoints
and independent milestone evidence. S11 cleanup accompanies each covered
replacement. S12 freezes and qualifies the completed product. S13 follows
acceptance. This sequence is an implementation rollout, not remote deployment.

| Checkpoint | Deliverable and owning files | Required completion evidence |
| --- | --- | --- |
| R0 — reconcile current owner direction | Update S8 specification, file/proof plans, handoff and affected primary contracts: SDK Project/Workspace/Sandbox shape; ordinary caller/runtime execution; remove daemon supervisor/launcher/cgroups/registration/Exec wire | Current plans agree. Retire old managed-Exec selections prospectively, preserving original IDs/receipts/verdicts. Keep ordinary Bash, output/backpressure and filesystem-lifetime proofs under their real owners. No old launcher/cgroup task dispatched |
| R1 — real SDK and Sandbox composition | SDK facade files; actual Sandbox backend/lifecycle/access modules; activate only real replacement code and reviewed dependencies | Project Init/seal/install reuses its qualified route. An owned sandbox has a ready daemon/control route; ordinary runtime execution works with standard streams/status; command identity and protected Store/overlay visibility proved. No daemon command registration |
| R2 — native read floor | FUSE mount/profile/session/request files; daemon native/control/request service; consistent reads, lookup/open owners and normal teardown | Installed Store → mount Ready → ordinary externally launched Bash reads/stats → unmount. Exact profile, stable identity, permissions, checked lookup acquisition/decrements, bounded handoff/receive slots and complete daemon drain. Busy probe keeps filesystem service usable |
| R3 — mutations and kernel coherence | Native mutation steps; existing Workspace/Overlay operations with required owning entry acquisitions | Writes, append, truncate/regrow, mappings, rename/link/unlink, removed cwd/O_PATH, permissions and times are exact. Changes made without SDK Exec registration behave identically. No per-WRITE notification deadlock or early ownership reclamation |
| R4 — captured namespace construction | Workspace construction namespace/scratch/cursors; owning Content backed validation/topology changes; Overlay captured cursor jobs | Complete names, links, metadata and changed file roots constructed from an exact capture. Wide/deep/sparse/alias/cycle cases pass; small incremental changes avoid a whole-base walk. Canonical algorithms and actual policy reused, resident windows bounded |
| R5 — real live Commit and install | Existing daemon store/commit orchestration plus the completed Workspace constructor and mounted install checks | Mount → ordinary Bash changes → Commit → fresh mount full oracle. Committed/UpToDate, overwrite publication/captured parent, missing dependency, real Busy and known/unknown/install-failure custody. Later active writes and retained reads survive installation |
| R6 — concurrency, sustained ownership and forced filesystem teardown | Shared request/Store service, native ownership/debt maintenance, exact abort/plain-detach/drain paths | Several Workspaces/processes, writers during Commit, output-backpressured callers, finite fair arrivals, repeat mount/Commit cycles and live/idle cleanup. Force refuses active control producers before effects; attempted daemon work drains before retirement. Caller-owned processes are not killed or supervised by daemon teardown |
| R7 — finish covered integration cleanup (S11) | Retire excluded core predecessors/old Server/FUSE/Sandbox wiring as replacements become covered; reconcile manifests/APIs/source docs/tests | One supported product path, actual members/exports and no reference fallback. Exact source-size classification of each replacement/relocation/retirement. Root reference retained for S12 |
| R8 — frozen integrated qualification (S12) | Existing core harness families/registry/oracles and scoped checks at a frozen source/build/image/workload/cache identity | Full roots, ordinary commands, Commit survival, fresh/persistent mounts, concurrency/resources and all applicable registered family outcomes. One sample per case/arm, correct cache treatment, preserved historical failures/unrun cases and only exact unaffected evidence reuse. Required gates satisfied or explicitly owner-disposed |
| R9 — root retirement and final verification (S13) | Remove root reference and obsolete wiring after dependency audit; preserve required shared configuration/provenance/evidence | Core remains independent, scoped final builds/checks pass, exact removal LOC recorded and all previous receipts retained. No release, push or deployment implied |

R4's component design/construction can proceed alongside R2/R3; its integrated
mounted proof and R5 require the real filesystem. R1's thin SDK facades can evolve
with those operations. All Cargo/test/measurement invocations remain serialized
in this primary checkout. This is dependency overlap, not one unchecked bulk
implementation commit.

Each code checkpoint has a deepest-file plan, locked build before bounded tests,
host checks where applicable and the affected Linux/native proof. Retain every
failed/ineligible/unrun attempt in append-only receipts, then perform the scoped
final Clippy/fmt/boundary checks and exact LOC comparison. Use a 100 s explicit
test stop (120 s ceiling), diagnose hangs before changed reruns, and export
`LAYERFS_CONSTRUCTION_WORKERS=1`. Normal namespace Init retains its separately
supported construction profile. Measurement budgets/owner rulings retain their
own scopes and no new timing or memory threshold is invented here.

## 5. Decisions retained and preparation scope

The [pre-S8 completion](PRE-S8-COMPLETION-20261007.md) and
[focused growth investigation](PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md) are
reused at their exact scopes; E04 is not reopened. New native filesystem,
ordinary execution and mounted namespace Commit dimensions get their own proofs.
The [Branch overwrite decision](BRANCH-OVERWRITE-DECISION-20261007.md), counted
Save refills and current profile direction override old conflicting plan text.
Broader conflict resolution remains owner-deferred. No process/memory/socket
checkpoint or restart-custody product is added to make Commit complete.

Source count preparation, arithmetic, links/anchors, changed-file scope and
preserved-state checks are retained under
[the rollout receipts](checks/cluster-two-loc-rollout-20261008/).
This checkpoint changes documentation only. No new implementation, native proof,
benchmark or retirement is performed. Both already-modified AGENTS guides and
the three protected untracked notes remain unchanged and unstaged by this work.
