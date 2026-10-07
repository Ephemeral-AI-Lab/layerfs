# Reviewed cluster-two file layout through R5

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Revised2026-10-08 against `5be93f6d7f9352eab3cbfa286fedbc861e6e3494`;
> implemented product tree `e3a61dfd814a579b58f56b63754ec3dbce83d67e`.
> R1 is verified. This destination is not implemented or proved by this document.

The [ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md) accepts FUSE as
owner of the native connection and complete kernel request lifecycle. Daemon
assembles it, retains the shared SQL/Store services, sole registry, overall
Workspace Ready/terminal unmount and existing Commit driver. Planned daemon
`native/` and kernel `request/steps/` trees are superseded by the Fuse homes below.
The [S8 specification](S8-SPECIFICATION-20261008.md) retains full behavioral authority.

## 1. Current source and legend

The [current inventory](checks/r2-r5-ownership-review-20261008/01-current-source-inventory.json)
contains every tracked active shipped source path and exact membership/counts:
12 active crates,569 active production files and62382 active production LOC.
Core including excluded code is105256; root reference65417; combined170673.
Excluded predecessors37431 and old FUSE/Server integration5443 remain. Sandbox is
already active, as are SDK facades and the daemon application/executable. FUSE
is excluded incompatible predecessor source. Actual replacement FUSE makes13
active crates; do not reactivate its old imports or create empty membership.

The original all-file destination/origin/blob
[manifest](checks/final-cluster-two-layout-20261008/02-destination-manifest.json)
and Git version of this document retain the earlier source pin. That immutable
manifest is historical, not the current membership or dispatch assignment.
Unchanged interiors are collapsed below, rather than moving them to satisfy a
new exhaustive diagram. Original source and API owners remain authoritative.

`[E]` means an existing owning home at the reviewed input; `[R2]`–`[R5]` identify
new/extended responsibility, not implemented behavior. Braced groups are candidate
focused siblings. Add or split only for actual behavior/line limits; names and file
counts are not scaffolding requirements. New production files≤999 physical lines;
lib.rs/mod.rs≤200, declarations/delegation only. Tests stay outside product src.

## 2. Proposed end-of-R5 owning homes

```text
core/
  Cargo.toml, Cargo.lock               [R2] real replacement FUSE and reviewed locked wiring
  crates/
    layerfs-fuse/                     [R2] preserve excluded predecessor before replacement
      Cargo.toml
      src/
        lib.rs
        ports.rs                      [R2] narrow nonblocking services; no daemon imports
        attributes.rs                 [R2/R3] checked native identity/attribute conversion
        diagnostics.rs                [R2] connection/request/credit/drain facts
        mount/
          mod.rs
          config.rs                   [R2] native configuration, no application parsing
          profile.rs                  [R2] negotiation and explicit receipt
          syscalls.rs                 [R2] mount/plain normal detach; force extensions R6
        session/
          mod.rs
          startup.rs                  [R2] connection and receive-loop ownership
          readiness.rs                [R2] actual all-loop serving evidence
          state.rs                    [R2] exact native states/retained failures
          drain.rs                    [R2] connection/request drain; aggregate unmount in daemon
        request/
          mod.rs
          callbacks.rs                [R2/R3] only fuser callback implementation
          decode.rs                   [R2/R3] validate within preadmission receive ownership
          types.rs                    [R2/R3] bounded owned inputs and original identity
          reply.rs                    [R2/R3] one reply attempt/disposal, no inferred delivery
        dispatch/
          mod.rs
          admission.rs                [R2] R handoffs + N receive slots, terminal wakeups
          queue.rs                    [R2] fair runnable native requests across mounts
          workers.rs                  [R2] one fixed K pool per daemon-assembled service
          pending.rs                  [R2] parked original requests/replies/credits
          completion.rs               [R2] resume notification, exact result ownership
        operations/
          mod.rs
          lookup.rs                   [R2] LOOKUP/FORGET via atomic backed engine jobs
          attributes.rs               [R2/R3] semantic plans, getattr/setattr
          directory.rs                [R2] backed handles/cookies, bounded enumeration
          open.rs                     [R2/R3] OPEN/RELEASE and processing association
          read.rs, readlink.rs         [R2] exact source/consumer lifetime
          write.rs                    [R3] write/append; split truncate.rs only if useful
          create.rs, link.rs          [R3] namespace entry handlers
          rename.rs, remove.rs         [R3] native orchestration of Workspace decisions
          flush.rs                    [R3] supported flush/fsync semantics, no Store seal
          unsupported.rs              [R2/R3] explicit refusals/disposal
        coherence/
          mod.rs
          reply_order.rs              [R3] published frontier and reply attempts
          attributes.rs               [R3] aliases/attribute coherence
          pages.rs                    [R3/R5] cached pages and install continuity
      tests/                          [R2/R3] mount/requests/normal teardown/mutations/mmap;
                                      [R5] component install coherence, no separate mmap engine

    layerfs-daemon/
      Cargo.toml                      [R2] daemon -> fuse -> workspace; no reverse edge
      src/
        lib.rs, bootstrap.rs          [E] declarations; concrete Store composition
        install.rs, install_file.rs, install_types.rs [E]
        bin/layerfs-daemon.rs          [E] thin application entry
        application/
          mod.rs, cli.rs              [E]
          config.rs, failure.rs       [E/R2/R5]
          connection.rs, owner.rs     [E] existing application/control ownership
          serve.rs                    [E/R2]
          filesystem.rs               [R2] assemble shared Fuse service/engine ports
        control/
          mod.rs, registry.rs         [E/R2] sole registry/aggregate native disposition
          operations.rs, serve.rs     [E/R2/R5]
          types.rs, failure.rs        [E/R2/R5]
          attach.rs, unmount.rs       [R2] aggregate admission/Ready/normal terminal transition
          status.rs                   [E/R2] compose maintained gauges
          commit.rs                   [R5] control admission/delegation to store/commit.rs
        service/
          mod.rs, startup.rs          [E]
          job_sql.rs, observations.rs [E] SQL owner receipts, not FUSE duplicate diagnostics
          completion.rs               [E/R2] real completion/loss notifier, exact consumption
          filesystem_port.rs          [R2/R3] implement Fuse service ports; no request engine
        overlay/
          mod.rs, owner.rs, queue.rs, credits.rs [E/R2] shared fair SQL service/notifications
          commands.rs                 [E/R2/R3/R4]
          read_port.rs, file_port.rs  [E/R2/R3] preserve existing callers; native async seam
          captured_run_port.rs        [E/R4]
          operation_record_port.rs, indexed_operation_record.rs [E/R4]
          native_ownership_commands.rs [R2/R3] bounded typed jobs on existing owner
          captured_namespace_port.rs  [R4] adapt existing captured page/point jobs
        store/
          mod.rs, open.rs, bind.rs, ports.rs [E/R2/R4] direct Store and admitted demand service
          read_service.rs             [R2, conditional] missing bounded immutable-demand service
          operation.rs, types.rs      [E] fresh exact failure scopes
          commit.rs                   [E/R5] sole capture/Save/finish/publish/install driver
          commit_types.rs, settle.rs  [E/R5] original result/unknown/install custody
      tests/                          [R2/R3] external executor/aggregate normal drain;
                                      [R5] mounted Commit/custody/full fresh-mount oracle

    layerfs-workspace/
      src/
        lib.rs, base/                 [E] current immutable view/cache/client
        mutation/                     [E/R3] reuse decisions in resumable native plans
        operations/
          mod.rs, types.rs, attributes.rs [E/R2/R3]
          read_plan.rs                [R2, conditional] missing resumable source-qualified plans
          file/{mod,read,write}.rs    [E/R2/R3]
          namespace/{mod,list,create,remove,rename}.rs [E/R2/R3]
        ports/
          mod.rs, overlay.rs, files.rs, lengths.rs [E/R2/R3/R4]
          captured_runs.rs, operation_record.rs [E/R4]
          captured_namespace.rs       [R4] provider-neutral captured pages/points
        construction/
          mod.rs, records.rs          [E/R4]
          context.rs, driver.rs, outcome.rs [R4] one captured namespace producer
          captured/                   [E/R4] existing changed-file context/normalize/owner/
                                      scan/source/state; no second file constructor
          namespace/{mod,cursor,directories,inodes,normalize,assemble}.rs [R4]
          scratch/{mod,records,release}.rs [R4, conditional] reuse existing indexed records
        workspace/{mod,state,view,serials,install}.rs [E/R2/R5]
      tests/                          [R4] captured/incremental namespace; [R5] install continuity

    layerfs-overlay/
      sql/
        schema.sql, accounting.sql    [E/R2/R4]
        native_ownership.sql          [R2, conditional] additional atomic indexed statements
      src/
        lib.rs, contract/, database/, payload/ [E/R2/R3/R4]
        namespace/
          mod.rs, inode.rs, directory_entry.rs, compound.rs [E/R2/R3]
          read_compound.rs            [R2] consistent facts + atomic entry/lookup ownership
          captured_namespace.rs       [R4, conditional] only missing indexed captured windows
        lifetime/
          existing source/owner/record modules [E]
          native_group.rs, native_lookup.rs [R2] backed aggregate/incarnation/revocation
          native_open.rs              [R2/R3, conditional] reuse file_owners.rs first
          file_owners.rs, lookup.rs   [E/R2/R3] independent existing owners retained
          orphan.rs, captured_reader.rs, composition.rs, frontier.rs, close.rs [E/R2–R5]
        maintenance/
          existing modules, reclaim.rs, orphan.rs [E/R2–R5]
          native_ownership.rs         [R2/R3] bounded live/terminal last-owner reclamation
        diagnostics/
          existing access/lifetime/payload modules [E]
          native_ownership_plan.rs    [R2, conditional] actual new statement-plan evidence
      tests/                          [R2/R3] native indexed custody; [R4] captured windows

    layerfs-content/
      src/
        lib.rs, contract/, file/, object/ [E] reuse canonical algorithms/formats
        filesystem/
          existing attributes/inode/rows/root modules [E/R4]
          directory/changes.rs        [R4, conditional] only missing bounded changes interface
          references/indexed_validation.rs [R4, conditional] extend existing indexed reducers
          validate.rs                [E/R4] existing validation entry
          validate/{cycles,entries}.rs [E/R4]
          validate/{backed,incremental}.rs [R4] missing backed/topology obligations, no second validator
      tests/                          [R4] extend actual canonical/backed validation coverage

    layerfs-api/sdk/
      src/
        lib.rs, operation.rs, control/, project/, sandbox/ [E]
        workspace/
          mod.rs, api.rs, binding.rs  [E/R2/R5] Bound stays distinct from Ready
          mount.rs, types.rs          [R2/R5] original bind + Attach attempts/custody
          status.rs, unmount.rs, commit.rs [E/R2/R5]
      tests/                          [R2/R5] public control/mounted integration, reuse homes

    layerfs-bridge/
      src/
        lib.rs, native/               [E] authenticated CONTROL transport, no data service
        control.rs, control_request.rs, control_reply.rs, control_types.rs [E/R2/R5]
        control_native.rs             [R2] bounded composed native facts/control vocabulary
        control_history.rs, existing setup/provision/wire modules [E]
      tests/                          [R2/R5] extend current protocol homes

    layerfs-sandbox/
      src/
        lib.rs, types/                [E]
        backend/docker/
          container.rs, container_types.rs [E/R2] actual FUSE device/capability deployment
          endpoint.rs, topology.rs    [E/R2] selected settings/backing/mount visibility
          request.rs                  [E] ordinary Exec creation, no FUSE configuration owner
          existing archive/HTTP/listener/stream modules [E]
      tests/                          [R2] changed deployment permissions/visibility
      no optional admin module/API/dependency

    layerfs-storage/, layerfs-persistence/, layerfs-history/ [E] Save/provider/publication
    layerfs-project/, layerfs-telemetry/ [E] host Init and existing observations
  vendor/fuser-0.18.0/                 existing sole authorized patch, integrity checked
  docs/issues/{303,307}/              contracts/layout/rollout/handoff and append-only receipts
```

## 3. Consolidation and interfaces

`ports.rs` is not permission for an interface per algorithm or a generic executor.
Reuse Workspace and domain contracts; add only missing asynchronous engine service
boundaries. Dependency `daemon -> fuse -> workspace` forbids Fuse importing daemon.
Any needed existing domain-type edge must be concrete/reviewed, with no cyclic
back-edge, copied token schema or extra adapter crate.

Native continuations and wakeups live in Fuse dispatch; real pending SQL completions,
credits and immutable Store read admission retain their engine owners. A fixed
K pool is shared across mounts in one daemon-assembled Fuse service. The shared
SQL owner independently serves native, Commit and cleanup producers. Blocking
OwnerClient read adapters, `try_complete` polling or a thread per parked request
cannot substitute for the required no-worker-wait completion mechanism.

Fuse reports connection-serving/connection-drained evidence. Daemon combines that
with Workspace/service admission and all namespace-bound engine/Store/control work
for overall Ready/terminal unmount. Positive LOOKUP/acquire and FORGET/release are
atomic backed jobs; open/processing/captured owners remain independent. No in-memory
whole-namespace map or implicit drop-based retirement is introduced.

Names can consolidate: lookup+forget, open+release, write+truncate and related
namespace handlers should share a focused file until size/responsibility requires
splitting. Extra native_open/read_service/scratch/query/diagnostic files are conditional
on missing behavior, not new parallel implementations. Keep correct current code
where it is; external tests may reuse existing homes. `native_mmap` tests kernel
callbacks/coherence and creates no separate mmap engine.

## 4. Checkpoints and later retirement

| Checkpoint | Required independent completion |
| --- | --- |
| R2 | Full native serving and aggregate Ready; read/stat/readdir/permissions; indexed lookup/open/request custody; Busy service usability and complete normal drain |
| R3 | Ordinary mutations, kernel-origin mapped writes, aliases/attributes/pages and exact request/reply/lifetime coherence |
| R4 | Complete captured names/links/metadata/file roots with bounded canonical construction/validation and incremental topology; may progress alongside R2/R3 |
| R5 | Existing daemon Commit driver consumes R4; actual mounted Bash changes, publication/known install, fresh-mount full oracle, failures/unknowns and later active-change survival; depends on R2/R3/R4 |
| R6/R8 | Forced teardown, sustained concurrency and frozen integrated acceptance remain later; R2–R5 does not claim them |
| R7/R9 | Covered predecessor cleanup and conditional reference retirement retain rollout ordering and exact accounting; no early deletion |

No implementation, timing, LOC growth or retirement is established by this proposal.
Keep Disposable/WAL/OFF, separate Overlay, writeback off, one construction producer,
locked bounded checks, exact failure custody and authorized fuser integrity. The
[next-agent prompt](HANDOFF-R2-R5-IMPLEMENTATION-20261008.md) carries exact current
R1 evidence/resources and the complete executable assignment after owner dispatch.
