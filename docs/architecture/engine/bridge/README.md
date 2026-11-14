# Bridge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The wire contract between a controller and a Workspace daemon: the authenticated channel, every record they exchange, the daemon's private setup and the Store install records.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-bridge` |
| Source | [`engine/crates/layerfs-bridge/src`](../../../../engine/crates/layerfs-bridge/src), [`duplex/`](../../../../engine/crates/layerfs-bridge/src/duplex), [`process/`](../../../../engine/crates/layerfs-bridge/src/process) |
| Uses | [Core: History](../../core/history/README.md) (ids, Branch and Commit records), [Core: Objects](../../core/objects/README.md) (node ids) |
| Used by | [Daemon](../daemon/README.md) (responder), [Project Init](../project-init/README.md) (installer), [Store calls](../store-calls/README.md) (values, record window), [Control: Sandboxes](../../control/sandboxes/README.md) (initiator, `DaemonSetup`), [Control: Home](../../control/home/README.md) (keys, process identity), [Control: Stores and projects](../../control/stores-and-projects/README.md) (manifest, resolver), [Control: Doctor](../../control/doctor/README.md) (channel probe, capacity model) |
| Flows | [A call, end to end](../../flows/call.md), [A Workspace session](../../flows/workspace.md), [Project Init](../../flows/init.md) |

This is the only page that defines the controller–daemon wire. The machine
worker protocol (`{ secret }`, `{ op, args }`) is a different protocol owned
by [Control: Machines](../../control/machines/workers-and-reach.md).

## Picture

```text
 controller (initiator)                        daemon (responder)
 layerfs-sandbox daemon pool                   layerfs-daemon listener
   TcpStream::connect ------------------------>  accept: free slot?
                                                  no: AdmissionNotice
   <-------------- 24-byte "LAYERFS-CAPACITY" ---    (unauthenticated)
   Noise_KK handshake (2 messages, no payload)    yes: worker thread
   <=========== Connection { Sender, Receiver } ===========>
     record = u16 BE length | AEAD(plaintext <= 65 519) | 16-byte tag

 first record (InitialRecord::decode, by magic)
   "LFSC\x01" Call { id, Request }  --> Answer { id, Reply }  "LFSA\x02"
   "LFSI\x01" StoreManifest         --> InstallReply           "LFSR\x01"
              file bytes ... "LFSF\x01" INSTALL_FINISH

 out of band: protected config file  "LFSD\x09" DaemonSetup
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Authenticates both ends | `Noise_KK`: each side knows the other's static public key in advance; prologue `layerfs/cluster-two/native-channel/v1\0`; two handshake messages with empty payloads; a wrong remote static key is `Invalid("authenticated peer")` | [`duplex/handshake.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/handshake.rs), [`duplex/profile.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/profile.rs) |
| Picks one cipher suite per architecture | `Noise_KK_25519_AESGCM_SHA256` on x86, x86_64, aarch64; `Noise_KK_25519_ChaChaPoly_BLAKE2s` elsewhere. aarch64 refuses to compile without the ARMv8 AEAD flags. No runtime fallback | [`duplex/profile.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/profile.rs) |
| Frames records | `u16` big-endian length, then sealed bytes; plaintext at most `MAX_PLAINTEXT_BYTES` = 65 519. Larger flows are several records | [`duplex/io.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/io.rs), [`duplex/channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| Splits a connection | `Connection { send, receive, peer }`: each direction has its own explicit nonce from 0 and its own buffers; both share one `failed` flag. `TCP_NODELAY` is set | [`duplex/channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| Fences from outside | `CloseHandle::close` shuts both directions once, so an accept loop can end a parked worker | [`duplex/channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| Ends cleanly | `wait_peer_end` reads once for the peer's EOF after the final reply, then fences; a trailing byte or read error is reported with `PeerEndPhase` (`Admit`, `Read`, `Fence`) | [`duplex/ending.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/ending.rs) |
| Counts its work | `ChannelWork`: calls, records, bytes, I/O and crypto attempts, buffer capacity | [`duplex/types.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/types.rs) |
| Says why a socket was refused | `AdmissionNotice`: magic `LAYERFS-CAPACITY`, version 1, reason `NoPoolSlot` or `Starting`, retained-failure count and vacant pool/reserved slots (each saturating at 255). The initiator turns it into `ChannelError::Admission` | [`duplex/admission.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/admission.rs) |
| Encodes fields one way | fixed bytes (ids, keys, big-endian integers); blob = `u16` length + bytes; text = UTF-8 blob; record = magic + fields, at most `LIMIT` = 65 519, no trailing bytes | [`wire.rs`](../../../../engine/crates/layerfs-bridge/src/wire.rs) |
| Routes the first record | `InitialRecord::decode` chooses `Control(Call)` or `Install(StoreManifest)` by magic; anything else is refused with no fallback | [`initial_record.rs`](../../../../engine/crates/layerfs-bridge/src/initial_record.rs) |
| Resolves names with a deadline | `resolve_within` / `resolve_all_within`: numeric IPs directly; otherwise one `getent ahosts` (Linux; in a Sandbox with the runtime layer, `LAYERFS_SANDBOX_RUNTIME`, the bundle's `getent` run by the bundle's loader) or `dscacheutil` (macOS) child, output at most 64 KiB, killed and reaped on every exit; `TimedOut` at the deadline; no shell, no retry, no resolver thread | [`network.rs`](../../../../engine/crates/layerfs-bridge/src/network.rs) |
| Names a local process | `LocalProcess { pid, start, boot }` (Linux, macOS) | [`process/identity.rs`](../../../../engine/crates/layerfs-bridge/src/process/identity.rs) |

### Control records

```text
Call   = "LFSC\x01" | id u64 BE (nonzero) | [12|21, scope[32]] | tag | fields
Answer = "LFSA\x02" | id u64 BE           | reply kind          | fields
```

| Tag | `Request` | Daemon answer on success |
| --- | --- | --- |
| 1 | `Mount { workspace, branch }` | `Bound { token, binding }` |
| 15 | `MountNamed { workspace, project, branch }` | `Bound`, or `SdkRefused` `NoSuchProject` / `NoSuchBranch` |
| 9 | `Attach(token)` | `Ready(ReadyMount)` |
| 2 | `Commit(CommitControl { token, force, message })` | `Committed`, `MergeCommitted` or `Conflict` |
| 3 | `Status(token)` | `Status(WorkspaceStatus)` |
| 10 | `Locate(workspace)` | `Located(WorkspaceStatus)` |
| 13 | `Cleanup(token)` | `Cleanup { token, state }` |
| 4 | `Unmount(UnmountControl { token, discard })` | `Unmounted` or `Dirty(DirtyReport)` |
| 11 | `ForceUnmount { token, relinquish_unknown }` | `ForceUnmounted` |
| 7 | `Hello(HelloRequest)` | `Hello(DaemonStatus)` |
| 8 | `EndSession` | `SessionEnded` |
| 20 | `Sdk(SdkCall { method, args })` | `Sdk(value)` or `SdkRefused` |
| 12 / 21 | `Observed { scope, request, traced }` | the inner reply; traced (21) wraps it in `Observed { reply, telemetry }` |
| 5, 6, 14 | `Fork`, `History`, `Branches` | records kept; the daemon refuses them (Store calls) |

Any operation may also be answered `Refused(ControlRefusal)` or, after an
effect, `Retained(TeardownCustody)`. Source:
[`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs),
[`control_reply.rs`](../../../../engine/crates/layerfs-bridge/src/control_reply.rs),
[`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs).

| Value | Shape | Source |
| --- | --- | --- |
| `WorkspaceToken` | `WorkspaceId` + daemon-local `namespace`; a stale token is refused, never redirected | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| `ControlRefusal` | `ControlCode` (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`, `Unknown`), phase, `moved`, `published`, detail. The code decides; detail is never parsed | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| `WorkspaceStatus` | token, last acknowledged binding, `Activity` (`Idle`, `Committing`, `Closing`, `Uncertain`, `LocalFailure`, `Attaching`), epoch, `published`, `LocalObservation`, `local_failure`, `MountStatus` | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| `MountStatus`, `ReadyMount`, `MountReceipt` | `MountPhase` 1–7 (`Unattached`, `Attaching`, `Ready`, `Probing`, `Draining`, `Retained`, `Stopping`); mount directory at most `MOUNT_DIRECTORY_LIMIT` 1 024 bytes; negotiated kernel facts | [`control_mount.rs`](../../../../engine/crates/layerfs-bridge/src/control_mount.rs) |
| `TeardownCustody` | token, first `TeardownStage` not established (1–9), `detached`, `MountWork`, detail, optional `ForcedFacts` | [`control_mount.rs`](../../../../engine/crates/layerfs-bridge/src/control_mount.rs) |
| `ForceUnmounted` | `ForcedFacts { abort, detach, commit, fenced }`, `MountWork`, `ForcedCleanup` (`Held`, `Queued`, `Gone`, `Unobserved`) | [`control_forced.rs`](../../../../engine/crates/layerfs-bridge/src/control_forced.rs) |
| `ConflictReport`, `MergedCommit` | conflicts and hunk lines written while they fit, `truncated` when some are left out; text fields at most `TEXT_FIELD_LIMIT` 4 096 | [`control_merge.rs`](../../../../engine/crates/layerfs-bridge/src/control_merge.rs) |
| `DirtyReport` | token, changed paths while they fit, inode and entry counts, `pending_merge`, `truncated` | [`control_unmount.rs`](../../../../engine/crates/layerfs-bridge/src/control_unmount.rs) |
| `HelloRequest`, `DaemonStatus` | `expected_instance` (never zero), `wait_for_store`; status = instance, `DaemonPhase` (`InstallPending` 1, `Installing` 2, `Retained` 3, `ControlReady` 4), overlay and Store SQLite versions (both required at `ControlReady`) | [`daemon_types.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_types.rs), [`daemon_wire.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_wire.rs) |
| History pages | `HISTORY_WINDOW` 32 per page | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |

### SDK values

| Item | Rule | Source |
| --- | --- | --- |
| `SdkValue` | `Null`, `Bool`, `Int`, `Text`, `Bytes`, `List`, `Map` (unique names, in order); depth at most `MAXIMUM_VALUE_DEPTH` 16 | [`control_sdk.rs`](../../../../engine/crates/layerfs-bridge/src/control_sdk.rs) |
| `SdkCall`, `SdkRefusal` | method or refusal code at most `MAXIMUM_NAME_BYTES` 64; a refusal is `code` (decides), `message`, `detail` | [`control_sdk.rs`](../../../../engine/crates/layerfs-bridge/src/control_sdk.rs) |
| History values | Commits and Layers as lowercase hex of their tagged identity; `commit_value`, `layer_value` shared by the daemon and the internal API | [`sdk_values.rs`](../../../../engine/crates/layerfs-bridge/src/sdk_values.rs) |
| Merge diagnostics | conflicts, last-writer-wins decisions and unit reports bounded to a 16 KiB window, with `*_truncated`; omitted detail never changes merge semantics | [`sdk_merge_values.rs`](../../../../engine/crates/layerfs-bridge/src/sdk_merge_values.rs) |
| Capacity read model | `CapacityModel { admission, retained }` answered by `backend.daemon.connection_failures`; `backend.daemon.reclaim { slot, generation }`; a missing field reads as zero so versions can differ | [`control_capacity.rs`](../../../../engine/crates/layerfs-bridge/src/control_capacity.rs) |

### Daemon setup

`DaemonSetup` is a private config record (`LFSD\x09`), never sent on a
channel and never in env or argv.

| Field | Meaning |
| --- | --- |
| `listen` | TCP listen address |
| `private_key`, `control_peer` | Daemon Noise secret; the one authorized controller's public key |
| `route` | `KernelRoute::Fuse` or `Nfs` |
| `overlay` | Overlay template path; Workspace files go beside it |
| `mounts` | Mount home; each Workspace mount is a new child |
| `command_uid`, `command_gid` | Identity presented as every inode's owner |
| `limits.connections` | Ordinary control slots; one more is reserved |
| `limits.handshake_ms` | I/O wait during authentication and the first record |
| `limits.idle_ttl_ms` | A slot parked in `receive()` this long is reclaimable under admission pressure |
| `limits.request_deadline_ms` | A request still running this long is expired: slot freed, socket closed, outcome unknown |
| `limits.cache_bytes`, `namespaces`, `pager_kib` | Shared immutable cache allowance; admitted Workspaces; overlay pager suggestion |
| `limits.serial_low_water` | Unconsumed inode serials below which a create reserves early |
| `store` | `Sqlite { locator, install }` or `Postgres { url, schema, synchronous_commit, authority }` |

`listen` must parse as a socket address; keys, `command_uid` and
`command_gid` must be nonzero (never root); `overlay` and `mounts` must be
absolute with no `.` or `..`, at most 4 096 bytes; every limit except
`cache_bytes` and `serial_low_water` must be nonzero.
`Sqlite { install: None }` waits for the one-time install; `Some(manifest)`
opens the installed Store at startup. A PostgreSQL URL and the cursor key
are redacted from `Debug`. Source:
[`daemon_setup.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_setup.rs),
[`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs),
[`support.rs`](../../../../engine/crates/layerfs-bridge/src/support.rs).

### Provisioning

```text
installer (Project Init)                 daemon (one install per process)
  StoreManifest "LFSI\x01" ----------->  Request: Disposable, locator ==
                                          configured path, no daemon_sqlite
                                         Claim: create_new temp, target absent
              <------------- InstallReply::Ready
  file bytes in records -------------->  Transfer: count == manifest.bytes
  INSTALL_FINISH "LFSF\x01" ---------->  Publish: one rename
                                         Open: check initial Branch and root
              <------------- Installed(manifest + daemon_sqlite)
                             or Refused(InstallRefusal)
```

| `StoreManifest` field | Rule | Source |
| --- | --- | --- |
| `provider` | `Sqlite`; `Postgres` has no sealed file and is refused by `check` | [`provision.rs`](../../../../engine/crates/layerfs-bridge/src/provision.rs) |
| `locator` | Absolute path in the daemon's filesystem, at most 4 096 bytes, no NUL, no trailing `/` | same |
| `profile` | The daemon accepts only `Disposable`. Durable execution is `NOT_RUN — disabled by owner until explicit reauthorization` | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `binding`, `cursor_key` | Catalog authority: binding 1–128 bytes, nonzero key; only on the authenticated channel | [`provision.rs`](../../../../engine/crates/layerfs-bridge/src/provision.rs) |
| `stack`, `branch`, `root` | The Init's LayerStack, first Branch and complete root | same |
| `bytes`, `host_sqlite`, `daemon_sqlite` | Nonzero sealed length; SQLite versions on each side (the daemon's filled only on success) | same |

A refusal carries `InstallPhase` (`Request`, `Claim`, `Transfer`, `Publish`,
`Open`, `Reply`), `RefusalKind` (`Invalid`, `Exists`, `Busy`, `Failed`,
`Unknown`), `claim_acknowledged`, `written`, `published`, an OS code and a
detail of at most 2 048 bytes. Neither is a retry instruction; retained
files are not removed. Source:
[`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs),
[`provision_wire.rs`](../../../../engine/crates/layerfs-bridge/src/provision_wire.rs).

### Process identity

| Item | Rule | Source |
| --- | --- | --- |
| `LocalProcess` | `observe` reads pid, kernel start time and boot once; `alive` needs all three to match and the process not a zombie; `signal` sends once, only to an observed live process, never to one recorded without a boot | [`process/identity.rs`](../../../../engine/crates/layerfs-bridge/src/process/identity.rs) |
| Linux | `/proc/<pid>/stat`, `/proc/sys/kernel/random/boot_id`, the `/proc` table | [`process/linux.rs`](../../../../engine/crates/layerfs-bridge/src/process/linux.rs) |
| macOS | `proc_pidinfo` and `proc_listallpids` (not `ps`, which Seatbelt may refuse); `kern.bootsessionuuid`, else `kern.boottime`. The crate's only `unsafe` | [`process/darwin.rs`](../../../../engine/crates/layerfs-bridge/src/process/darwin.rs), [`lib.rs`](../../../../engine/crates/layerfs-bridge/src/lib.rs) |

It lives here so `layerfs-home` and the Sandbox driver share it without
either reaching the other.

## Invariants

| Rule | Enforced by |
| --- | --- |
| No data service: the wire carries control, SDK values and the one install stream, never a Save or object upload | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs), [`initial_record.rs`](../../../../engine/crates/layerfs-bridge/src/initial_record.rs) |
| One attempt: any failed send or receive quarantines both directions; later calls are `Quarantined`. No retry, reconnect or replay | [`duplex/channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs), [`duplex/error.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/error.rs) |
| A too-large record is `RecordLimit` with nothing sent and no nonce used | [`duplex/channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| The capacity notice admits nothing: it precedes the handshake, carries no secret, and every operation still needs authentication | [`duplex/admission.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/admission.rs) |
| Exactly one decoder per record; unknown magic, tag, boolean or trailing byte is refused | [`wire.rs`](../../../../engine/crates/layerfs-bridge/src/wire.rs), [`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs) |
| At most one observation layer; it may not wrap `Hello`, `EndSession` or another observation; a zero scope is invalid | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| A call id is nonzero and is echoed by its answer; a lost answer does not undo the effect | [`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs), [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| Secrets stay out of `Debug`: daemon private key, cursor key, PostgreSQL URL | [`daemon_setup.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_setup.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs), [`provision.rs`](../../../../engine/crates/layerfs-bridge/src/provision.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Plaintext per record | 65 519 bytes (`u16::MAX − 16`) | [`duplex/profile.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/profile.rs) |
| Handshake message | 256-byte buffers | [`duplex/handshake.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/handshake.rs) |
| Channel errors | `Io`, `Noise`, `Invalid`, `RecordLimit`, `NonceExhausted`, `Admission(notice)`, `Quarantined`, `CloseFailed` | [`duplex/error.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/error.rs) |
| Invalid record | `ControlError(<field>)`; failure before send has no remote effect | [`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs) |
| Named-mount names | each up to 65 535 bytes, sent as given; the daemon refuses invalid names | [`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs) |
| Resolver | `TimedOut` at the deadline; `NotFound` with no address; `Unsupported` off macOS/Linux | [`network.rs`](../../../../engine/crates/layerfs-bridge/src/network.rs) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`duplex.rs`](../../../../engine/crates/layerfs-bridge/tests/duplex.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test duplex` | Any (loopback TCP; ARMv8 AEAD flags on aarch64) | Records beyond one window with fixed buffers, wrong peer refused, corrupted records quarantine, EOF and fence outcomes |
| [`admission.rs`](../../../../engine/crates/layerfs-bridge/tests/admission.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test admission` | Any | The 24-byte notice round-trips; any other record is never a notice |
| [`control_records.rs`](../../../../engine/crates/layerfs-bridge/tests/control_records.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test control_records` | Any | Records keep exact identities and failure knowledge |
| [`observed_records.rs`](../../../../engine/crates/layerfs-bridge/tests/observed_records.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test observed_records` | Any | Observation wrappers keep old bytes and refuse nesting |
| [`network_deadline.rs`](../../../../engine/crates/layerfs-bridge/tests/network_deadline.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test network_deadline` | macOS, Linux | System resolver use and an expired budget with no resolver thread |
| [`process_identity.rs`](../../../../engine/crates/layerfs-bridge/tests/process_identity.rs) | `cargo +1.85.1 test --locked -p layerfs-bridge --test process_identity` | macOS, Linux | Start time and boot distinguish processes; a record without a boot is never signalled |
