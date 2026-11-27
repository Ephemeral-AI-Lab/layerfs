# Daemon

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Workspace daemon process: it opens the global Store, owns every mounted Workspace's overlay and kernel serving, and answers control calls over the bridge.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-daemon` |
| Source | [`application/`](../../../../engine/crates/layerfs-daemon/src/application), [`bin/`](../../../../engine/crates/layerfs-daemon/src/bin), [`workspace/`](../../../../engine/crates/layerfs-daemon/src/workspace), [`operator/`](../../../../engine/crates/layerfs-daemon/src/operator), [`src/`](../../../../engine/crates/layerfs-daemon/src), [`receive.rs`](../../../../engine/crates/layerfs-daemon/src/install/receive.rs), [`telemetry.rs`](../../../../engine/crates/layerfs-daemon/src/telemetry.rs) |
| Uses | [Bridge](../bridge/README.md), [Store](../store/README.md), [Store calls](../store-calls/README.md), [Overlay](../overlay/README.md), [Workspace](../workspace/README.md), [Commit](../commit/README.md), [Kernel routes](../kernel-routes/README.md), [Core: Telemetry](../../core/telemetry/README.md) |
| Used by | [Control: Sandboxes](../../control/sandboxes/README.md) (launches it, holds the controller key), [Control: Doctor](../../control/doctor/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

## Picture

```text
 controller (control_peer key)                commands in the Sandbox
   | TCP + Noise KK (Bridge)                    | ordinary file calls
   v                                            v
 +--------------------------- layerfs-daemon ----------------------------+
 | listener -> slots[connections] + 1 reserved -> one worker thread each |
 |   full? AdmissionNotice | idle reap | request deadline | recovery     |
 |   first record: Install(StoreManifest) | Control(Call)               |
 |                                                                       |
 | Application: DaemonPhase, Store open or one-time install,             |
 |              retained connection failures (ring of 64, 1 h)           |
 |      |                                                                |
 |      v                                                                |
 | Service: registry WorkspaceId -> Entry             (Control calls)    |
 |   Mount, Attach, Commit, Status, Unmount, ForceUnmount, backend.*     |
 |      |                 |                       |                      |
 |      v                 v                       v                      |
 |  Store (global,    Owner: overlay           FUSE KernelServing (Linux)|
 |  opened direct)    template + one           or NFS ExportServing      |
 |                    file per Workspace       -> FilesystemPort         |
 +-----------------------------------------------------------------------+
```

## What it does

### Startup

```text
layerfs-daemon --config <file>
  1 secure_process      umask 077; Linux: not dumpable; macOS: RLIMIT_CORE 0
  2 config::read        protected file -> DaemonSetup (Bridge)
  3 bind listen address
  4 Application::start  overlay template, instance key, Store, serving
  5 stdout: LAYERFS_DAEMON_LISTEN <addr>
  6 Application::serve  accept loop
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Takes one argument | `--config <protected-file>`; argv and env carry no key and no command ("no launcher or shell command mode") | [`startup/cli.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/cli.rs) |
| Checks the config | Opened `O_NOFOLLOW`; regular file owned by the daemon user, no group/other bits, private directory; at most 8 192 bytes; one `DaemonSetup` record | [`startup/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/config.rs) |
| Separates identities and paths | `command_uid` differs from the daemon uid (except on NFS); Store, overlay and config sit outside the mount home in private `0700` directories (failing with `private directory ownership/mode/type at <path>: …` and a `chmod 700` hint on mode errors); the Store is not the overlay file; `serial_low_water` below `SERIAL_REFILL` | [`startup/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/config.rs) |
| Chooses the route | `fuse` or `nfs`; macOS accepts only `nfs` | [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs) |
| Starts the overlay owner | Creates and closes the overlay template (`pager_kib`, `namespaces`); every later start failure is `AfterOverlay`, keeping the owner | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs), [Overlay](../overlay/README.md) |
| Takes a fresh incarnation | `generate_keypair()`; the public half is `DaemonStatus.instance` | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) |
| Opens the Store | An installed SQLite file with the `Normal` profile, or PostgreSQL; `Sqlite { install: None }` waits for an install | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs), [`Cargo.toml`](../../../../engine/crates/layerfs-daemon/Cargo.toml) |
| Starts FUSE serving (Linux) | `/dev/fuse` to `0600`, checks the mount home, mounts a private `fusectl` tree for abort controls; if that fails, writes `LAYERFS_DAEMON_ABORT_UNBOUND` and starts without forced unmount | [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs) |
| Starts NFS serving | `ExportServing`; force-unmounts client mounts a previous daemon left (`LAYERFS_NFS_RECLAIM` lines); the same reclaim deletes `<mounts>/.appledouble`, the AppleDouble companions' disposable scratch area (directories `0700`, files `0600`, no fsync, no space reserved ahead), and touches nothing else under the mount home; helper overrides `LAYERFS_NFS_MOUNT`, `_UNMOUNT`, `_FORCE`, `_FLUSH` | [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs) |

### Phases and Hello

```text
   start with Store                        start without Store
          |                                        |
          v                                        v
    ControlReady <-- Installed reply ------- InstallPending
          ^                                        | Install record
          +-- failure after final ack -------- Installing
                                                   | other failure
                                                   v
                                                Retained
```

| `DaemonPhase` | Ordinary control | Source |
| --- | --- | --- |
| `InstallPending`, `Installing` | `Busy` "Store control Service not ready"; a second installer is `Refused(Request, Exists)` | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs), [`startup/install.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/install.rs) |
| `Retained` | `Unknown` "original Store startup custody retained" | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |
| `ControlReady` | `Service::execute_control` | same |

`Hello` returns `DaemonStatus`; `expected_instance` refuses another
incarnation; `wait_for_store` blocks on a condition variable until the phase
leaves `InstallPending`/`Installing` (a readiness wait, not a retry). Hello
never claims a kernel mount; that is `Attach`
([`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs)).

### Connections

| Behaviour | Detail | Source |
| --- | --- | --- |
| Slots | `connections` pool slots plus `RESERVED_SLOTS` = 1; each admitted socket gets thread `layerfs-control-<slot>` | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs), [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |
| Reserved slot | Before `ControlReady`: the one installer. After: a recovery connection, used only when the pool is full; it serves one `Status`, `Unmount`, `backend.daemon.connection_failures` or `backend.daemon.reclaim`, then closes; anything else is `Capacity` | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) (`recovery_request`) |
| Pool full | Expire overdue requests, reap idle slots, try the reserved slot; still none: write one `AdmissionNotice` (`Starting` before ready, else `NoPoolSlot`) and close | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs), [Bridge](../bridge/README.md) |
| Idle reap | A pool slot parked in `receive()` for `idle_ttl_ms` is freed and its socket fenced, only under admission pressure | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) (`reap_idle`) |
| Request deadline | A pool request running past `request_deadline_ms` is expired: slot freed, socket closed, record `Expired`; the caller sees an unknown outcome. Reserved slots are never reaped or expired | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) (`reap_expired`) |
| First record | Socket timeouts are `handshake_ms` until the first record arrives, then cleared | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |
| Write bodies | Bodies a call declares (`fs.write`, `fs.apply`) are read and dropped: Store calls are not served here | [`connection/bodies.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/bodies.rs) |
| Observed calls | Run once; numeric records (at most 64 of 4 096 bytes) go to stderr; a traced call also returns `Reply::Observed` with timing, SQL, delta, FUSE opcode and Commit storage telemetry | [`observed/records.rs`](../../../../engine/crates/layerfs-daemon/src/application/observed/records.rs), [`telemetry.rs`](../../../../engine/crates/layerfs-daemon/src/telemetry.rs) |
| Failure custody | A failed connection frees its slot at once. A channel or I/O failure before any decoded request is logged only; any other (decode error, uncertain refusal, failed reply, expiry, reclaim) is kept as a `RetainedFailure`: at most 64, each visible for 1 h, cause at most 512 bytes | [`connection/retained.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/retained.rs), [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) (`disposition`) |
| Reclaim | `backend.daemon.reclaim { slot, generation }` closes one worker's socket and records `Reclaimed`; a vacant slot or a newer generation is `Invalid`; it never touches a Workspace | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |

### Operator methods

`Request::Sdk` reaches the daemon only for its own method table; every
other SDK name is `SdkRefused("UnknownMethod")`, because Store calls run in
the caller's process ([Store calls](../store-calls/README.md)).

| Method | Answer | Source |
| --- | --- | --- |
| `backend.daemon.connection_failures` | `CapacityModel`: slot counts by class and the retained-failure ring | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs), [`capacity.rs`](../../../../engine/crates/layerfs-protocol/src/daemon/capacity.rs) |
| `backend.daemon.reclaim` | `{ outcome: "reclaimed" }` (see Connections) | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |
| `backend.daemon.workspace.get_storage` | Overlay row counts from `Overlay::resources()`: `inodes`, `entries`, `payload`, `metadataRows`; no base walk | [`operator/list_changes.rs`](../../../../engine/crates/layerfs-daemon/src/operator/list_changes.rs) |
| `backend.daemon.workspace.list_changes` | One page of changed paths (`path`, `kind`, `markers`), `hasMore`, `nextPagePos`; walks base and live trees in bounded windows over one diagnostic base source, released once | [`operator/list_changes.rs`](../../../../engine/crates/layerfs-daemon/src/operator/list_changes.rs) |
| `sandbox.list_workspaces` | History from the Store plus one view per bound Workspace. With `history: false`, the Workspace views alone (`{ kind, version, workspaces }`): the project's stack and one Branch record per mounted Branch are read, and no Branch page, Layer, Commit or Store statistic | [`status/list.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/list.rs), [`operator/list_workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/families/operator/list_workspaces.rs) `workspaces` |
| `sandbox.doctor_workspaces` | A page of 32 registry facts taken with `try_lock`: no Store, SQL or overlay job; `Busy` while the registry is held | [`status/doctor.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/doctor.rs) |

The `backend.daemon.workspace.*` calls select a Workspace by its mount
directory (`path`); only a `Ready` entry matches
([`status/observation.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/observation.rs)).

### Filesystem port

```text
kernel request (FUSE loop or NFS export)
  Mounted(Arc<BoundWorkspace>)::request(fence)
    -> FilesystemPort(StoreOperation, Fence)      one per request
         fence checked only before an attempt
         call(job): one overlay job under the Workspace lock
         base_client(): this Workspace's own Store reader
```

| `RequestServices` method | Runs |
| --- | --- |
| `observe_visit`, `mutate_visit` | One kernel visit job: lookup/getattr, or a mutation with its reply custody |
| `directory_visit`, `publish_cookies`, `close_directory` | READDIR page, cookies, RELEASEDIR |
| `read_visit`, `close_file` | READ window, RELEASE |
| `base` | The immutable base view through the Workspace's Store reader |
| `recognize` | Compare written bytes with committed content to form `WriteClaims`; no SQL, never on the receiving loop |
| `forget`, `reserve_serial`, `reply_attempted` | `kernel_forget`; one inode serial (one early refill at low water); settle a reply ticket |

The fence is consulted only at owner admission, Store reader admission and
`reserve_serial`; an admitted job is always awaited. A failed Store read
ends that request alone (`BaseDemandFailed`). A refill refused `Busy` at
its one `BEGIN` reserved nothing and is not asked again. Source:
[`src/ports.rs`](../../../../engine/crates/layerfs-daemon/src/ports.rs);
the port contract is in [Kernel routes](../kernel-routes/README.md).

### Installs

| Phase | Daemon step | On failure | Source |
| --- | --- | --- | --- |
| Admission | Only in `InstallPending`; phase becomes `Installing` | Second installer `Refused(Request, Exists)` | [`startup/install.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/install.rs) |
| `Request` | Manifest is `Normal`, names the configured Store path, carries no `daemon_sqlite` | Protocol refusal | [`receive.rs`](../../../../engine/crates/layerfs-daemon/src/install/receive.rs) |
| `Claim` | `create_new` of the temporary name; target absent; send `Ready` | Claim name recorded before the attempt | [`receive.rs`](../../../../engine/crates/layerfs-daemon/src/install/receive.rs), [`file.rs`](../../../../engine/crates/layerfs-daemon/src/install/file.rs) |
| `Transfer` | Write each record until `bytes`; then `INSTALL_FINISH` | Partial file kept | [`receive.rs`](../../../../engine/crates/layerfs-daemon/src/install/receive.rs) |
| `Publish` | One `rename` to the Store path, no sync | — | same |
| `Open` | Open with the `Normal` profile; check the initial Branch and root | Opened Store kept in `InstallFailure` | same |
| `Reply` | `Installed(manifest + daemon_sqlite)`; build `Service`, phase `ControlReady` | Published and checked: still `ControlReady`; otherwise `Retained` | [`startup/install.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/install.rs) |

Every failure sends one `Refused(InstallRefusal)` unless the channel itself
failed; files are never removed automatically. `InstallWork` counts records,
writes, bytes, sync calls and the largest record
([`outcome.rs`](../../../../engine/crates/layerfs-daemon/src/install/outcome.rs)).
Durable execution is `NOT_RUN — disabled by owner until explicit
reauthorization`: the `Request` check refuses any non-`Normal` manifest
before the Durable sync branches in `install.rs` can run.

## Invariants

| Rule | Enforced by |
| --- | --- |
| The daemon launches and supervises no command; Sandbox or an external executor owns launch, streams, exit status and cancellation | [`startup/cli.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/cli.rs), [`lib.rs`](../../../../engine/crates/layerfs-daemon/src/lib.rs) |
| The daemon serves no Store calls: `Fork`, `History`, `Branches` are `Invalid`; off-table SDK methods are `UnknownMethod` | [`route/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs), [`operator/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/operator/dispatch.rs) |
| Commands run as a distinct non-root identity; daemon backing files stay outside the mount home | [`startup/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/config.rs), [`daemon_setup.rs`](../../../../engine/crates/layerfs-protocol/src/provision/daemon_setup.rs) |
| A Store install must be `Normal` | [`receive.rs`](../../../../engine/crates/layerfs-daemon/src/install/receive.rs) |
| No connection failure owns a slot; a stale or duplicate finish changes nothing | [`startup/lifecycle.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs) (`Slots::finish`) |
| One attempt per operation; a stop after an effect keeps the exact owner and is reported, never retried or settled by a later read | [`teardown/kernel_detach.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/kernel_detach.rs), [`workspace/receipts.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/receipts.rs) |
| Whole operations run outside the registry mutex; admission checks and marks inside it | [`workspace/registry.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/registry.rs), [`route/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Config record | 8 192 bytes | [`startup/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/config.rs) |
| Control connections | `connections` + 1 reserved; then an `AdmissionNotice` | [`connection/workers.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/workers.rs) |
| Retained connection failures | `LIMIT` 64, `TTL` 1 h, oldest dropped first | [`connection/retained.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection/retained.rs) |
| Workspaces | `namespaces`; one more Mount is `Capacity`; a stopped maintenance failure also refuses Mount with `Capacity` | [`route/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |
| Mount admission and the registry | Mount reads the owner's retained maintenance failure while it holds the registry; the doctor's registry page answers `Busy` meanwhile | [`route/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs), [`status/doctor.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/doctor.rs) |
| Attach and drain observation | `READY_WAIT` 5 s, `DRAIN_WAIT` 5 s; expiry keeps owners `Retained` | [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs) |
| Platform | FUSE serving on Linux only; macOS serves NFS only | [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs) |
| Store kinds | SQLite and PostgreSQL, both always built | [`Cargo.toml`](../../../../engine/crates/layerfs-daemon/Cargo.toml) |

Per-call failures (refusal codes, `TeardownStage`, forced facts) are in
[Control calls](control-calls.md).

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`daemon_slots.rs`](../../../../engine/crates/layerfs-daemon/tests/application/daemon_slots.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test application daemon_slots` | macOS, Linux | The real daemon binary: undecodable records keep capacity, a full pool answers the capacity notice, a failure storm leaks no slot |
| [`slot_state.rs`](../../../../engine/crates/layerfs-daemon/tests/application/slot_state.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test application slot_state` | macOS, Linux | A retained failure owns no slot, stale finishes change nothing, the ring is bounded and timed |
| [`host_install.rs`](../../../../engine/crates/layerfs-daemon/tests/install/host_install.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test install host_install` | macOS, Linux (loopback TCP) | One-time install of a sealed Init Store, then a full-root read; an existing Store is refused before upload; a partial transfer blocks another installer |
| [`doctor_registry.rs`](../../../../engine/crates/layerfs-daemon/tests/workspace/doctor_registry.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test workspace doctor_registry` | macOS, Linux | The registry page reads no Store SQL or overlay job |
| [`named_mount.rs`](../../../../engine/crates/layerfs-daemon/tests/workspace/named_mount.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test workspace named_mount` | macOS, Linux | `MountNamed` binds by names on the Workspace's reader and refuses as the Store lookup does |

## Pages

| Page | What it covers |
| --- | --- |
| [Control calls](control-calls.md) | `control/`: the registry, Mount and Attach, Status, NFS export, Unmount, forced teardown, dirty reports and failure codes. |
