# Agent guide history

> **Status:** Archived; retained for historical evidence only.

Text moved word for word out of the agent guides when they were reorganized
on 2026-11-16, so the guides keep instructions only. It records how the
engine reached its current shape; it is not a current rule or a product
contract. Current behaviour is in the
[architecture guide](../../../architecture/README.md), and the rules are in
the [root](../../../../AGENTS.md) and [engine](../../../../engine/AGENTS.md)
guides. Only relative links were rewritten for the new location.

A second pass on 2026-11-16 condensed the guides' wording without moving more
text here; the guides as they stood before it are at commit
[`84743424f1`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/84743424f1/AGENTS.md),
and before any reorganization at
[`99ffd262a3`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/99ffd262a3/AGENTS.md).

## Engine scope and ownership: implementation status (S6 to R9c)

From `engine/AGENTS.md`, "Scope and ownership", as of `layerfs_v2_0_0` at
`02188f0593`.

- Active cluster-one members are content, storage, history, persistence, project
  Init and telemetry. The initial cluster-two SQLite engine is `layerfs-overlay`;
  its active membership does not complete Workspace/FUSE/daemon integration.
  [Cargo.toml](../../../../Cargo.toml) is the membership authority.
  Workspace now exposes immutable base binding/read, effective source-qualified
  lookup/stat/paged name merge, prepared actor install and atomic ordinary
  namespace operations, bounded writes/truncate/regrow and composed payload
  reads. Independent file/captured/operation custody and bounded live reclamation
  are implemented through S6, including indexed automatic reclamation (its
  physical reservation and headroom were removed by #321: no space is reserved
  ahead of use, and a full device rolls the write back as no space);
  native/runtime/integrated exits remain
  unfinished. See [S6 audit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/S6-EXIT-AUDIT.md). Its relocated
  predecessor `layerfs-workspace-legacy` was removed by R7-retire.
  Since #321 the daemon holds one overlay file and connection per mounted
  Workspace, each behind its own lock, with the calling thread running each
  short job; the fair SQL owner thread, its queue, credits and shared memory
  budget are removed. See the [terminology guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/64-overlay-terminology.md)
  and the [#321 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/321/CHECKLIST.md).
  Daemon also exposes a direct global Store adapter with a
  mount-time reader per Workspace, exact quarantine and read-only bind snapshots,
  one immutable cache shared across Workspaces, bounded root
  binding and operation-owned exact failures. Concrete opening stays outside
  the provider-independent adapter; see the
  [direct Store boundary](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/61-direct-store-adapter.md).
  The host-mediated SDK/upstream transport is retired; see
  [the retirement record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/PRE-S8-F12-20261007.md).
  Workspace calls return typed values directly and count SQL/payload/allocation
  work under the same lock. Exact attempted failures retain their cause and
  backing; before-effect refusals retain their original input. Overlay/Daemon
  also expose bounded indexed operation records in the same database, with guarded atomic changes and automatic last-owner
  cleanup. Content now exposes a backed file editor over these records and
  Workspace supplies an explicit operation/file adapter with first-original
  failure custody. Run-aware localized edits reuse the frozen sparse scanner
  and bounded positive immutable zero evidence. Streamed directory inputs share
  the canonical filesystem algorithm; backed serial state supplies new-parent
  membership, rebuilt roots and initial counts/final row streams. On the backed
  route validation classifies in 64-row windows and keeps its topology evidence
  in indexed records; resident routes keep their resource-derived limits.
  Captured local inode/run points retain exact reader root/floor and bounded
  forward metadata progress. Captured regular-file normalization now derives
  authenticated base facts, coalesces final changes into indexed records and
  consumes them once through the retained FileView/fallible edit source. Backed
  reference reduction/release uses fixed rows, sealed membership passes and
  guarded FIFO/cursor transitions. Captured namespace normalization is the
  `CapturedNamespace` producer
  ([captured namespace construction](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/78-captured-namespace-construction.md));
  the product Commit constructor calls it
  ([product Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/79-product-commit.md)). The Store
  half of Commit is implemented with exact original failure/unknown custody;
  see [Store Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/65-store-commit-composition.md).
  The external E01 example records original startup work, diagnostics and Stop
  without creating a Workspace route. It supplies diagnostic receipt consistency,
  not E1/E2 performance admission. Its relocated predecessor was removed by
  R7-retire; standalone native executable/Exec integration is unfinished.
  Pre-S8 direct-operation accounting now has actual registered macOS/Linux engine
  and Store-half Commit runs, independent family/count validation, and supported
  phase observations. Numerical latency/residency and exact continuous peaks
  remain unqualified; see the [accounting scope](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/PRE-S8-F14-20261007.md).
  SDK creates/imports the initial Store, publishes the first Branch and consumes
  seal, returning a closed file plus explicit provider/locator/profile metadata.
  It retains no Store handle or data service. Native install now streams the
  sealed file once, opens it in the daemon and returns both actual SQLite versions;
  see [F5 handoff](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/PRE-S8-F5-20261007.md). Authenticated mount
  binding, Store-half Commit, status, terminal close, fork and history now compose
  the direct ports; see [native control](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/68-native-workspace-control.md).
  The prior client/runtime, daemon upstream, Bridge logical data framing and
  excluded API-core are retired. Historical receipts keep their original topology
  and verdicts. R7-retire (2026-10-10)
  removed all seven excluded directories as retirement: `layerfs-server`,
  `layerfs-fuse-legacy`, `layerfs-sandbox-legacy`, `layerfs-sdk-legacy`,
  `layerfs-daemon-legacy`, `layerfs-bridge-legacy` and
  `layerfs-workspace-legacy`. The root workspace lists the active
  members across core, engine and api; the boundary guard refuses the return of a removed directory or
  package name, and each source is recoverable from the commit named in the
  [R7-retire record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R7-RETIRE-COMPLETION-20261010.md).
  Pinned read-only SDK views (303/08 O-10) are deferred, not dropped: no
  implementation is in the tree, and a kept feature needs a new design on the
  SQLite overlay. Do not restore the predecessor code to supply them.
  Project native import preserves opaque symlinks, complete membership and hard
  links, with indexed acquisition state. The accepted host WAL baseline retains
  its recorded speed FAIL; no new timing is implied by SDK composition.
  Bridge retains pinned authenticated KK channels, bounded native records and
  independent fences and bounded native install/control. FUSE/Exec qualification
  and complete kernel mount readiness remain S8.
  The replacement Sandbox is an active member with ordinary Engine execution,
  owned lifecycle and authenticated SDK/daemon startup. Optional privileged
  command cancellation/client provenance is outside the current owner scope.
  The replacement Fuse is now active with a fixed shared dispatcher, actual
  deferred Owner/Store ports, a read-path callback adapter with complete opcode
  accounting, direct mount/session ownership and connection drain.
  Its predecessor `layerfs-fuse-legacy` was removed by R7-retire; the last
  commit holding that source is `15e33e22c`. The daemon
  composes Attach/Locate/Ready and reversible normal unmount; see
  [native request service](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/75-native-request-service.md) and
  [native mount session](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/76-native-mount-session.md).
  Ordinary native mutation is served through the existing Workspace plan and
  one publishing Overlay job, with no notification and no writeback cache; see
  [native mutation and kernel coherence](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/77-native-mutation-coherence.md).
  A control Commit runs the captured namespace producer through the unchanged
  driver, and takes the Workspace's overlay lock for each of its jobs; the mounted
  proofs and their limits are in the
  [R5 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R5-COMPLETION-20261009.md). Forced unmount of one
  Workspace is implemented as `ForceUnmount`: a before-effect guard, one abort
  write on the connection's own control, the terminal fence, the local drain,
  one plain detach, then Revoke and Close; see
  [forced teardown](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/80-forced-teardown.md). It is refused where
  no abort control is bound, which includes the current Sandbox topology, and a
  stop after an effect is `Retained` with no way back. No daemon-wide graceful
  drain exists and none is planned. Source acquisitions have their own owner
  slots, and a failed cold read ends its own request with `EIO` while the
  mount keeps serving; the concurrency, teardown and failure-scope proofs and
  their limits are in the [R6 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R6-COMPLETION-20261009.md).
  A create that leaves fewer unconsumed inode serials than the daemon's
  configured low-water makes one early reservation; every deployment and
  harness sets 0 today, which makes none. A new Mount is refused `Capacity` at
  `mount:debt` while maintenance is stopped. Both, the proofs added around
  them and the rows still open are in the
  [R8b record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R8B-COMPLETION-20261010.md). R8b closed not
  qualified and R9b without removal. R8c then closed qualified on function,
  counts and resources at the owner-rescoped scope, with the deployed
  low-water at 256 ([R8c record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R8C-COMPLETION-20261010.md)),
  and R9c retired the root reference
  ([R9c record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R9C-COMPLETION-20261010.md)).
  Add members only with real product boundaries and implementation.

## Overlay schema history (schemas 17 to 37)

From `engine/AGENTS.md`, "Architecture and public contracts".

Overlay schema19 uses `directory_entry` and `operation_record` terminology,
including owned/indexed variants, throughout SQL and the active public ports.
Through schema 33 Workspaces shared physical tables partitioned by namespace;
since #321 (schema 34) every mounted Workspace has its own file with the same
tables, copied from a closed empty template at mount and deleted at terminal
unmount. The naming
change authorizes no physical table redesign; generic temporary buffers and
historical receipts retain their distinct terms. See the
[terminology guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/64-overlay-terminology.md).
The #320 product sweep (2026-10-14, schema 37) removes accounting triggers,
Workspace/allocator SQL rows and generic caller-minted leases. Workspace
lifecycle/frontier/dirty/base-reader counters are one journaled runtime value,
restored only after known whole-job rollback; unknown outcomes retain custody.
Commit runs/readers/base pins are journaled owner maps; captured generation
holds use a count index. Daemon-bound routing is checked once per call; Overlay
accepts no Route and stores no namespace. Cleanup debt remains SQL-backed after
the [memory-placement review](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/320/CLEANUP-MEMORY-DESIGN-20261014.md).
Resources computes row/length totals on demand and
retains exact owner counts from memory. See the current
[runtime state](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/64-overlay-terminology.md#runtime-workspace-state-and-remembered-sql-custody).

Schema17 adds native mount/source/lookup/read associations, atomic Workspace
read observations and bounded revoked-lookup retirement. Schema18 adds native
file associations over the existing OpenFile owner and atomic open decisions; see
[native read custody](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/73-native-read-custody.md). This component
does not establish Ready, kernel service or full drain. Schema19 adds native
directory ownership, exact name-boundary cookies, retained parent updates and
bounded live cleanup; see [directory custody](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/74-native-directory-custody.md).

## Source ownership review, 2026-10-08 (R2 to R5)

From `engine/AGENTS.md`, "Filesystem access and execution ownership".

Reviewed target source ownership2026-10-08: the replacement `layerfs-fuse` owns
its complete native connection/request service, including bounded dispatch,
parking/resumption, kernel handlers/replies and coherence. Daemon assembles one
shared Fuse service with its existing SQL/Store services, retaining registry,
overall Ready/unmount and Commit composition. Dependency is daemon -> fuse ->
workspace, never Fuse -> daemon. Request dispatch/read ports, mount/Ready and
per-Workspace normal drain are implemented ([R2 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R2-COMPLETION-20261008.md)),
as is ordinary native mutation ([R3 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R3-COMPLETION-20261008.md));
as is the captured namespace producer with its bounded validation
([R4 record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R4-COMPLETION-20261008.md)), which the Commit
driver reaches through the product constructor
([product Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/79-product-commit.md)). See the
[reviewed ownership](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md).
Moving blocking OwnerClient calls behind a port does not establish deferred service.

## Root guide: Store routes as first written

From `AGENTS.md`, "LayerFS mental model", as of `layerfs_v2_0_0` at
`7a6da3e4f8`. The current rules are in the root guide's "Where calls run".

- Target (owner direction 2026-10-07, Store calls revised 2026-10-18): every
  daemon opens the global Store directly from a volume the daemons share and
  reads, saves and publishes in-process. Project Init adds a project to an
  existing Store as a live add (2.0, #533). **sqlite Store calls run in a
  process on the Store's kernel; postgresql Store calls, and bucket (s3) I/O,
  run in the caller's process on any OS (owner, 2026-11-02, #533 G2).** A
  sqlite Store's openers share one kernel and a local filesystem holding the
  Store (ext4, xfs, btrfs on Linux; apfs, hfs on macOS); a process on another
  kernel reaches it only through that machine's worker. Anywhere else sqlite
  Store calls fail `Unsupported`, with no fallback; the daemon does not serve
  Store calls. Workspace calls (mount, Exec, Commit,
  status, unmount) go to the daemon. Do not revive `layerfs-server` or build a
  host/server adapter in the data path; the host-mediated SDK runtime is
  retired, not extended. See the
  [integration contract](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/06-cluster-one-integration.md)
  and [direct Store access](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/352/DIRECT_STORE.md).
  **Exception (owner, 2026-10-20, revised the same day): dsh Live on macOS
  only.** The dsh-layerfs plugin may give macOS processes a ported SDK with
  the SDK's API whose calls run in one long-lived worker per LayerFS box: a
  Linux container running the real SDK, which opens each project's Store
  directly and keeps it and its daemon connections open while the project is
  in use (closed after a quiet minute). The worker listens on loopback only,
  behind a secret. The product SDK and daemon do not change. (Widened to any
  integration on 2026-10-24; see below.)
- **Additional exception (owner, 2026-10-22): dsh Live on Windows.** The
  Windows dsh-layerfs adapter may forward its SDK binding calls to one
  authenticated loopback worker per LayerFS home in a selected WSL2 Linux
  distribution. The worker opens the shared Store directly on that
  distribution's local ext4/xfs/btrfs filesystem and uses its Docker Engine
  for Sandbox daemons. Windows owns the UI, bindings and traces; Linux owns
  native handles and project discovery. This is independent of the macOS
  adapter and changes neither the product SDK nor the daemon. A lost reply
  is never replayed, and an uncertain Init retains its Linux home and volume.
- **Additional exception (owner, 2026-11-01, #527 D5; renamed 2026-11-02,
  #533 G2): the `native` machine on macOS.** For a Store on the `native`
  machine on macOS, the library runs in the caller's own process and opens the project's Store directly on a local apfs or hfs
  directory (`kernel_guard` admits those two there, #427). Its Sandboxes are
  daemon processes serving loopback NFS to this Mac's own client, one user,
  no container (see the agent guide's native Sandboxes). Every opener is a
  process on this one Mac, so the one-kernel, local-filesystem rule holds;
  network and other filesystems are still refused. The Docker, worker and
  `vm:` routes on macOS are unchanged.
- **Plugin-neutral LayerFS (owner, 2026-10-24; revised 2026-11-02, #533
  G2).** LayerFS is agnostic to dsh and every other plugin. The `layerfs`
  package routes by the home's records: a call's route comes from its
  Store's and machine's records, so one process uses Stores on several
  machines at once, and the worker routes above are open to any caller of
  the package. LayerFS reads `LAYERFS_HOME` and the trace settings only.
  There is no adapter name, no integration check and no code that names a
  plugin. An integration that can set only its own prefixed names, such as
  dsh's `DSH_*`, maps them itself. The worker's rules above (loopback,
  secret, one attempt, no replay) are unchanged.
- **Machines (owner 2026-11-02, #533 G2; replaces the 2026-10-31 VM
  placement, V1).** A VM machine holds many Stores, with one worker per
  machine. Machines are created, removed and started by the `machine` calls,
  and the routes come from the home's records (`machine/<name>/record.json`,
  `store/<name>/record.json`). `native` and `docker` are built in; VM kinds
  (`apple-container`, `lima`) are created by name. Each VM
  machine's data disk is mounted at
  `/layerfs/data` and holds `store/<name>/store.sqlite` per Store. Its
  worker runs as root behind its own secret, reached through one loopback
  port on the host, with one attempt per call and no replay, the same rules
  as the macOS and Windows routes. Guests never read the home: the caller
  sends each worker call the settings it needs and writes every record.
  **Every machine uses both Store kinds (owner, 2026-11-03, #535 D9, G1,
  G3):** a machine holds any number of sqlite Stores, and every PostgreSQL
  Store is usable from it. A sqlite Store is on exactly one machine; a
  PostgreSQL Store's calls still run in the caller's process, and it is
  usable from the machines it is attached to (`store.attach`, each checked
  from that machine; supersedes #535 G3), where the support table
  (`layerfs-platform` `support.rs`, `control/SUPPORT.json`) marks the
  platform supported; elsewhere it is refused with the table's reason. The password is resolved only in the
  caller and travels only in one worker request and the Sandbox's private
  daemon setup file. This supersedes #533 O3/G13 ("VMs and ssh later").
  See [#533](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/README.md), [#535](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/535/README.md)
  and the [proposal](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/host-platform-connections.md).
- **Removed platforms (owner, 2026-11-08; supersedes the layerfs-project#7
  restriction of 2026-11-06): Firecracker, Microsandbox and SSH machines are
  removed.** Their code is gone and no platform is disabled with its code
  kept. An old record of one is shown as unsupported with the reason
  (``platform `<name>` was removed in LayerFS 2.0``), every other call on it
  is refused, and `machine remove` only forgets it. No runtime setting or
  feature flag brings one back; that requires a later explicit owner
  decision and source change. Historical evidence is unchanged. Native,
  Docker, Lima and Apple Container retain their existing support rules. See
  the [restriction contract](../../../../docs/architecture/control/machines/platforms.md#removed-platforms-platform_policyrs).

## Root guide: rebase notes

From the end of `AGENTS.md`; the steps now live in the
[parallel worktree guide](../../../general/parallel-worktrees.md).

After rebasing across #463, update each selected worktree's own target registration and prepare again
for the root `target/` path; never migrate another worktree's resource record.
After rebasing across #686 (fuser from its Git fork), or any later fuser `rev` change, run
`tools/worktree.py prepare` again for each platform before offline builds; a Cargo
home prepared earlier has no checkout of that revision and fails `--offline`.
