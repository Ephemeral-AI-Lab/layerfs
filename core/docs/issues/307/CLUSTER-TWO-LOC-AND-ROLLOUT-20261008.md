# Cluster-two source baseline and reviewed rollout

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Source/membership reviewed2026-10-08 at
> `5be93f6d7f9352eab3cbfa286fedbc861e6e3494`; R1 verified, native R2–R5 unfinished.
> Current counts are exact. This review starts no implementation or measurement.

The [reviewed layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md) and
[ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md) assign the complete
kernel connection/request service to Fuse. Daemon assembles it with the existing
fair SQL owner and direct Store, retaining registry/control and the sole Commit
driver. SDK Project/Workspace/Sandbox organization and ordinary external execution
remain unchanged. Optional admin cancellation/client provenance stays deferred.

## 1. Exact current source counts

The [current inventory](checks/r2-r5-ownership-review-20261008/01-current-source-inventory.json)
and [R1 per-path counter](checks/r1-completion-20261008/29-per-file-production-loc.json)
use exact Cargo membership and production Rust plus shipped SQL, excluding inline/
transitive tests, examples, tools, docs, third-party/generated source. Product tree
`e3a61dfd814a579b58f56b63754ec3dbce83d67e` is unchanged by this review.

| Scope | Current production LOC |
| --- | ---: |
| Active cluster-one libraries | 39630 |
| Active cluster-two: Overlay, Workspace, Daemon, Bridge, SDK, Sandbox | 22752 |
| **Active product,12 crates** | **62382** |
| Excluded core predecessors | 37431 |
| Excluded integration: old FUSE1447 + Server3996 | 5443 |
| **All core product including excluded source** | **105256** |
| Root reference | 65417 |
| **Combined** | **170673** |

Actual replacement FUSE will make13 active crates. Sandbox and the daemon
application/executable already exist; their old planned creation is not remaining
work. Excluded source totals42874; no retirement is performed or credited here.

## 2. Remaining implementation estimate

The earlier65000–72000 finished-product range was a forecast at
`e765b39af0b40ebddb14556dafb2497d6219fa62`, before the verified R1 changes and
this request-service ownership review. Its original [receipts](checks/cluster-two-loc-rollout-20261008/02-current-package-loc.json)
and Git version remain historical. It is not a refreshed current forecast or
size gate; do not carry its stale per-crate allocation or108-path count forward.

| Current owner | Active LOC now | Remaining responsibility, no new numerical estimate |
| --- | ---: | --- |
| Content | 19085 | Backed validation and incremental namespace topology |
| Overlay | 7542 | Atomic indexed native ownership and any missing captured windows/reclamation |
| Workspace | 3902 | Consistent resumable semantic plans and complete captured namespace adapter |
| Daemon | 5384 | Assemble Fuse; narrow async engine ports; aggregate Ready/normal drain; integrate R4 with existing Commit |
| Bridge + SDK | 2932 | Native control vocabulary and public mount/status/Commit integration |
| Replacement FUSE | 0 active | Complete connection/session/dispatch/handlers/replies/coherence service |
| Sandbox | 2992 | Selected FUSE device/capability deployment and actual visibility/protection proof |
| Other cluster-one libraries | 20545 | Reuse owning APIs; only evidenced missing composition changes |
| **Total** | **62382** | Re-estimate from concrete deepest-file plans if requested |

Changing a proposed owner does not imply proportional source growth/shrinkage.
No fresh range is invented. Line ceilings encourage focused responsibilities;
required correctness, validation and custody may not be removed to fit an estimate.

## 3. Growth versus retirement

Each implementation commit separately records active growth, relocation,
duplication and covered retirement against its exact parent/staged/committed
snapshot. The42874 excluded core lines and65417 reference lines remain present.
They cannot be subtracted from a future total before required coverage/acceptance.
Root reference removal remains conditional on R8/R9, beyond this R2–R5 assignment.
Archived uncommitted admin/FUSE drafts are evidence, not production retirement.
The earlier host-runtime retirement is already in this baseline and is never
subtracted twice. No kernel request service has yet been moved in product source.

## 4. Combined implementation rollout

After owner dispatch, R2–R5 is one coordinated S8/S10 effort with reviewable local checkpoints
and independent milestone evidence. S11 cleanup accompanies each covered
replacement. S12 freezes and qualifies the completed product. S13 follows
acceptance. This sequence is an implementation rollout, not remote deployment.

| Checkpoint | Deliverable and owning files | Required completion evidence |
| --- | --- | --- |
| R0 — reconcile current owner direction | Update S8 specification, file/proof plans, handoff and affected primary contracts: SDK Project/Workspace/Sandbox shape; ordinary caller/runtime execution; remove daemon supervisor/launcher/cgroups/registration/Exec wire | Current plans agree. Retire old managed-Exec selections prospectively, preserving original IDs/receipts/verdicts. Keep ordinary Bash, output/backpressure and filesystem-lifetime proofs under their real owners. No old launcher/cgroup task dispatched |
| R1 — real SDK and Sandbox composition | SDK facade files; actual Sandbox backend/lifecycle/access modules; activate only real replacement code and reviewed dependencies | Project Init/seal/install reuses its qualified route. An owned sandbox has a ready daemon/control route; ordinary runtime execution works with standard streams/status; command identity and protected Store/overlay visibility proved. No daemon command registration |
| R2 — native read floor | Fuse mount/session/request/dispatch/operations; daemon filesystem assembly/ports and aggregate control; backed consistent reads/owners and normal drain | Installed Store → mount Ready → ordinary externally launched Bash reads/stats → unmount. Exact profile, stable identity, permissions, checked lookup acquisition/decrements, bounded handoff/receive slots and complete daemon drain. Busy probe keeps filesystem service usable |
| R3 — mutations and kernel coherence | Fuse operation handlers/coherence over existing Workspace/Overlay operations with required owning entry acquisitions | Writes, append, truncate/regrow, mappings, rename/link/unlink, removed cwd/O_PATH, permissions and times are exact. Changes made without SDK Exec registration behave identically. No per-WRITE notification deadlock or early ownership reclamation |
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

R1 is complete at its pinned no-admin scope. This review is documentation only;
R2–R5 execution is assigned by the [next-agent prompt](HANDOFF-R2-R5-IMPLEMENTATION-20261008.md),
not started by this document. Forced teardown/sustained concurrency remain R6,
frozen integrated acceptance R8 and reference retirement R9.

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
The original preparation receipts keep their original source/status scope. This
review changes documents only, preserves all runtime receipts and owner notes,
and establishes no new implementation, native proof, benchmark or retirement.
