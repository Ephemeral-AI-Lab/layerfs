# Startup

[Architecture](../../README.md) › [Engine](../README.md) › [Daemon](README.md) › Startup

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Starting the daemon and reaching readiness.

| | |
| --- | --- |
| Component | [Daemon](README.md) |
| Source | [`engine/crates/layerfs-daemon/src/application`](../../../../engine/crates/layerfs-daemon/src/application), [`engine/crates/layerfs-daemon/src/bin`](../../../../engine/crates/layerfs-daemon/src/bin) |

```text
layerfs-daemon --config <file>
  1 secure_process          umask 077; Linux: not dumpable;
                            macOS: RLIMIT_CORE 0
  2 config::read            protected file -> DaemonSetup
  3 TcpListener::bind(listen)
  4 Application::start      overlay template, instance key, Store,
                            kernel or NFS serving, Service
  5 stdout: LAYERFS_DAEMON_LISTEN <addr>
  6 Application::serve      accept loop, one worker per slot
```

Source: [`application/cli.rs`](../../../../engine/crates/layerfs-daemon/src/application/cli.rs),
[`bin/layerfs-daemon.rs`](../../../../engine/crates/layerfs-daemon/src/bin/layerfs-daemon.rs).

## Config and CLI

The only argument is `--config <protected-file>`. Arguments and environment
carry no key and no command.

| Check | Refusal | Source |
| --- | --- | --- |
| Config opened `O_NOFOLLOW`; a regular file owned by the daemon user, no group/other mode bits; its directory private | `private config ownership/mode/type` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| At most 8192 bytes; decodes as one `DaemonSetup` record | `private config record size`, `private config fields` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs), [`daemon_setup.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_setup.rs) |
| Route: `fuse` or `nfs`; macOS accepts only `nfs` | `kernel route fuse is not served on macOS; only nfs` | [`filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| `command_uid` differs from the daemon uid, except on the NFS route | `command and daemon identities must differ` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| `serial_low_water` below `layerfs_workspace::SERIAL_REFILL` | `serial low-water must be below the refill window` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| Store, overlay and config paths outside the mount home; Store is not the overlay file | `separate protected backing and mount paths` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| Overlay and Store parents are private `0700` directories (created once if absent); an installed Store file is protected | `private directory ownership/mode/type` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |

The `DaemonSetup` fields are listed on [Bridge: Daemon setup](bridge.md#daemon-setup).

## Connection

```text
accept --> free slot? --no--> drop the socket (no worker)
              | yes: Slot::Running, thread "layerfs-control-<slot>"
              v
   read/write timeout = handshake_ms
   duplex::accept(stream, private_key, control_peer)    (Noise KK)
   loop:
     record = receive(); InitialRecord::decode
       Install(manifest) -> install (one per daemon)
       Control(call)     -> drop declared write bodies
                            outcome = control or observed control
                            answer_call(id, reply)
                            EndSession: wait peer EOF, end
     after the first record: socket timeouts cleared
   error -> Slot::Retained(ConnectionFailure) until taken
```

| Rule | Source |
| --- | --- |
| Slots are `connections + 1`; the last is reserved for the installer while the phase is `InstallPending` or `Installing`. | [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs), [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| A failed connection keeps its slot and receipt (`ConnectionFailure`) until `take_connection_failure`; a failure before any decoded request is written to stderr and the slot freed. | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs), [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/application/failure.rs) |
| An operation whose refusal carries unresolved custody also keeps the slot after its reply is sent. | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| Write bodies a call declares (`fs.write`, `fs.apply`) are read and dropped: Store calls are not served here. | [`bodies.rs`](../../../../engine/crates/layerfs-daemon/src/application/bodies.rs) |
| `Request::Observed` runs the wrapped call once and writes numeric diagnostic records; a traced call also answers `Reply::Observed` with telemetry. | [`diagnostics.rs`](../../../../engine/crates/layerfs-daemon/src/application/diagnostics.rs), [`telemetry.rs`](../../../../engine/crates/layerfs-daemon/src/telemetry.rs) |

## Owner

`Application::start` builds everything once, in this order. A failure after
step 1 returns `ApplicationError::AfterOverlay`, keeping the started owner.

| Step | What | Source |
| --- | --- | --- |
| 1 | `Owner::start_observed(overlay, ProfileConfig { pager_kib, max_pages: None }, OwnerConfig { namespaces })`: creates and closes the overlay template | [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs), [Overlay: Database](../overlay/database.md#template-and-startup) |
| 2 | `generate_keypair()`: the public half is the daemon `instance` | [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |
| 3 | Open the Store if one is named: an installed SQLite file with the `Disposable` profile, or PostgreSQL (feature `postgresql`) | [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs), [`Cargo.toml`](../../../../engine/crates/layerfs-daemon/Cargo.toml) |
| 4 | FUSE route (Linux): protect `/dev/fuse` to `0600`, check the mount home, mount the private `fusectl` control tree beside the overlay (best effort), start `KernelServing` | [`filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| 5 | NFS route: start `ExportServing`, force off client mounts a previous daemon left (`LAYERFS_NFS_RECLAIM` lines) | [`filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| 6 | If a Store is open: `Service` over Store, owner and serving; phase `ControlReady`. Otherwise `InstallPending` | [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |

When `fusectl` cannot be mounted, the daemon writes
`LAYERFS_DAEMON_ABORT_UNBOUND` once to stderr and starts with no abort
control bound; [forced unmount](forced-teardown.md) is then refused before
any effect. NFS helper overrides come from `LAYERFS_NFS_MOUNT`,
`LAYERFS_NFS_UNMOUNT`, `LAYERFS_NFS_FORCE` and `LAYERFS_NFS_FLUSH`.

## Serve and readiness

```text
             start with Store                       start without Store
                    |                                       |
                    v                                       v
              ControlReady  <--- Installed (Reply) --- InstallPending
                    ^                                       | Install record
                    |                                       v
                    +---- failure after final ack ----- Installing
                                                            | other failure
                                                            v
                                                         Retained
```

| `DaemonPhase` | Control calls | Source |
| --- | --- | --- |
| `InstallPending` (1) | `Busy`: "Store control Service not ready" | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| `Installing` (2) | Same; a second installer is refused `Exists` | [`serve.rs`](../../../../engine/crates/layerfs-daemon/src/application/serve.rs) |
| `Retained` (3) | `Unknown`: "original Store startup custody retained" | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| `ControlReady` (4) | Served by `Service::execute_control` | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |

`Hello` returns `DaemonStatus { instance, phase, overlay_sqlite,
store_sqlite }`. With `expected_instance` it refuses a different daemon
incarnation. With `wait_for_store` it blocks on a condition variable until
the phase leaves `InstallPending`/`Installing`: a readiness wait, not a
retry. `Hello` never claims a kernel mount is Ready; that is `Attach`.
Source: [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs),
[`daemon_types.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_types.rs).

## Related

- [Bridge](bridge.md): the channel, records and `DaemonSetup`.
- [Operator: Installs](operator.md#installs): the install conversation.
- [Control calls](control-calls.md): what `ControlReady` serves.
- [Overlay: Database](../overlay/database.md): the template made in step 1.
- [Store: Lifecycle](../store/lifecycle.md): opening the global Store.
- [Control: Routing and workers](../../control/routing/README.md): who connects.
- [Glossary](../../glossary.md).

---

← [Engine: Daemon](README.md) · [Up: Daemon](README.md) · [Daemon: Control calls](control-calls.md) →
