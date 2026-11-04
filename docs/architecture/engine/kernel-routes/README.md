# Kernel routes

[Architecture](../../README.md) › [Engine](../README.md) › Kernel routes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the kernel reaches a Workspace: route-neutral mount ports, FUSE and loopback NFSv3.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-mount-ports`, `layerfs-fuse`, `layerfs-nfs` |
| Source | [`engine/crates/layerfs-mount-ports/src`](../../../../engine/crates/layerfs-mount-ports/src), [`engine/crates/layerfs-fuse/src`](../../../../engine/crates/layerfs-fuse/src), [`engine/crates/layerfs-nfs/src`](../../../../engine/crates/layerfs-nfs/src), [`engine/patches/fuser-0.18.0`](../../../../engine/patches/fuser-0.18.0) |
| Uses | [Workspace](../workspace/README.md), [Overlay](../overlay/README.md) |
| Used by | [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Ports](ports.md) | Route-neutral ports and operations shared by FUSE and NFS. |
| [FUSE](fuse.md) | The FUSE route. |
| [fuser patches](fuser-patches.md) | The three authorized patches to fuser 0.18.0. |
| [NFS](nfs.md) | The loopback NFSv3 route. |
| [Coherence](coherence.md) | Keeping kernel caches coherent with the Workspace. |

## The picture

```text
  any process with the mount visible (shell, Exec, external executor)
          |  ordinary filesystem calls
          v
  +----------------- kernel -------------------+
  |  FUSE driver (/dev/fuse)  |  NFSv3 client   |
  +---------------------------+-----------------+
          |                          |  TCP 127.0.0.1:<port>
          v                          v
  +-------------------+     +---------------------+
  | FUSE              |     | NFS                 |
  |  KernelSession    |     |  Server             |
  |  (patched fuser   |     |  Kernel (synthetic  |
  |   0.18.0)         |     |   lookups, opens)   |
  |  KernelFilesystem |     |  RPC / XDR / MOUNT  |
  |  MountRequests    |     |  AppleDouble        |
  +-------------------+     +---------------------+
          |   Coherence rules       |
          +-----------+-------------+
                      v
  +---------------------------------------------+
  | Ports  (layerfs-mount-ports)                |
  |  MountServices -> RequestServices, Fence    |
  |  KernelRead KernelData KernelMutation       |
  |  DirectoryStream   Phase timing             |
  +---------------------------------------------+
                      |  implemented by the Daemon (Mounted)
                      v
  +---------------------------------------------+
  | Workspace visits  --->  Overlay (own SQLite)|
  |       |  base reads                         |
  |       v                                     |
  |    Store (committed base)                   |
  +---------------------------------------------+
```

One engine mount of a Workspace is served by exactly one of the two routes.
Both drive the same [ports](ports.md); the [daemon](../daemon/README.md)
chooses the route, attaches it and tears it down.

## What it does

The route for each platform comes from the `kernel` field of
[`connect/SUPPORT.json`](../../../../connect/SUPPORT.json), generated from
[`support.rs`](../../../../connect/crates/layerfs-machine/src/support.rs):

| Host | Platform | `backend` | `kernel` |
| --- | --- | --- | --- |
| macos | native | `native` | `"nfs"` |
| macos | docker | `docker` | `"fuse"` |
| macos | apple-container, microsandbox, lima, firecracker, ssh | `namespaces` | `"fuse"` |
| linux | native, microsandbox, ssh | `namespaces` | `"fuse"` |
| linux | docker | `docker` | `"fuse"` |
| linux | apple-container, lima, firecracker | `null` | `null` (unsupported on this host) |
| windows | microsandbox, ssh | `namespaces` | `"fuse"` |
| windows | native, docker, apple-container, lima, firecracker | `null` | `null` (unsupported on this host) |

So NFS serves exactly one row, the macOS native platform; every other
supported row runs a Linux kernel and uses FUSE. The table is owned by the
[support matrix](../../connect/support-matrix/README.md).

| Given to both routes by the ports | FUSE adds | NFS adds |
| --- | --- | --- |
| Owner visits for LOOKUP, GETATTR, OPEN, OPENDIR, READ, READLINK, READDIR and every mutation | A `/dev/fuse` mount, INIT negotiation and two receive loops | One loopback TCP export with its own ONC RPC, XDR and MOUNT code |
| Base reads between visits, never under the Workspace lock | The kernel's own lookup, open and FORGET ownership, served as requests | A synthetic kernel that holds lookup references and implicit descriptors for a stateless client |
| Serial reservation, write recognition, post-reply ticket release | Per-mount request owner, retained failures, drain receipt | Stable 25-byte handles; reference collection and re-resolution |
| `Fence` and `FailedDemands` for forced teardown | Detach, abort control, forced drain | Silly renames and AppleDouble companions kept out of the Workspace |
| Kernel-independent refusals (`Declined`, `Absent`) | Cache invalidation after a merged install | Client refresh by mode-preserving SETATTR after a merged install |

## Invariants

| Rule | Enforced by |
| --- | --- |
| Kernel permission checks decide access: `allow_other,default_permissions`; the library uid filter is off (`SessionACL::All`). | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs), [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| Every inode shows the configured command uid and gid; a chown to anything else is `EPERM`. Mode bits beyond `0o7777` and special files are refused. | [`unsupported.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/unsupported.rs), [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs) |
| Stable identity: one serial per inode for the mount's life, root = inode 1, generation = mount owner; NFS handles are stable per mount and stale across mounts. | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs), [`handle.rs`](../../../../engine/crates/layerfs-nfs/src/handle.rs) |
| Exact request ownership: each received unit has one disposition; a failure is retained with its future and inputs, never replayed. | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs), [`failure.rs`](../../../../engine/crates/layerfs-fuse/src/request/failure.rs) |
| Exact open and lookup ownership: OPEN/OPENDIR record a descriptor in their deciding job, RELEASE/RELEASEDIR/FORGET give it back, a stopped fence never refuses disposal. NFS keeps one reference per handed-out serial. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| A publication ticket is released only after the single reply attempt; a published mutation is never undone. | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs), [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| One attempt per syscall: one plain detach, one abort write; no lazy or forced unmount syscall. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| Kernel writeback stays off: `FUSE_WRITEBACK_CACHE` is never selected. | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs); [engine guide](../../../../engine/AGENTS.md) |
| Only the owner-authorized fuser 0.18.0 patches: signed timestamps, scoped receive-loop lifecycle, negotiated receive buffer. | [`check_fuser_integrity.py`](../../../../tools/check_fuser_integrity.py); [engine guide](../../../../engine/AGENTS.md#fuser-provenance-and-authorized-patch-checks) |
| Filesystem access needs no Exec registration: any process that sees the mount is served the same way. | [engine guide](../../../../engine/AGENTS.md#filesystem-access-and-execution-ownership) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| READ and WRITE window | 128 KiB (`READ_WINDOW`, `WRITE_WINDOW`); larger is `EINVAL` | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs), [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| FUSE input per request | `MAX_INPUT_BYTES` = `WRITE_WINDOW + 510`; more is `ENOTCONN` | [`types.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/types.rs) |
| FUSE receive buffer | `max_write + 4096` per receive loop, two loops per mount; the INIT buffer stays at fuser's 16 MiB + 4 KiB | [fuser patches](fuser-patches.md#receive-buffer) |
| Name length | 255 bytes; longer is `ENAMETOOLONG` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Symlink target | Shorter than one page (FUSE: negotiated page size; NFS: `LINK_PAGE` 4096); else `ENAMETOOLONG` | [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Serial allocator contended | `EAGAIN` (FUSE), `NFS3ERR_JUKEBOX` (NFS); nothing created | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Overlay full | `ENOSPC` / `NFS3ERR_NOSPC`, only for a known rollback | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| Failed base read of one request | `EIO` for that request; the mount keeps serving; recorded in `FailedDemands` | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| Forced teardown | Waiting requests answer `ENOTCONN` | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| Not implemented | xattrs, locks, ACCESS (FUSE), READDIRPLUS (FUSE), FALLOCATE, COPY_FILE_RANGE, IOCTL, POLL, LSEEK, BMAP, FLUSH, FSYNC: `ENOSYS` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| STATFS / FSSTAT | Fixed declared values (2^30 blocks and files); not capacity | [`inline.rs`](../../../../engine/crates/layerfs-fuse/src/request/inline.rs), [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs) |
| NFS record | 2 MiB per RPC record; 1 MiB per WRITE call | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| NFS synthetic kernel | 65,536 lookup references (default), 256 file and 256 directory descriptors | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| SQLite databases inside an NFS Workspace (macOS) | A WAL application database fails its first query with `SQLITE_CANTOPEN` (14): macOS SQLite's NFS VFS has no `xShmMap`. Convert it to `journal_mode=DELETE` before import. This concerns application databases, not the Store. | [SQLite on network filesystems](../../../general/sqlite-on-network-filesystems.md) |
| FUSE route outside Linux | `layerfs-fuse` builds its session, mount and request modules on Linux only | [`lib.rs`](../../../../engine/crates/layerfs-fuse/src/lib.rs) |

## Demonstration

Commands are written from source and were not run while writing this page.
Run each under a wall timeout of at most 120 s.

```text
 kernel_mount / kernel_coherence  (Linux, root, /dev/fuse, SYS_ADMIN)
   fixture Store + Branch -> Harness (daemon Service, Owner)
     -> Mount -> Attach (real FUSE mount) -> ordinary fs calls,
        child commands as uid 65534 (COMMAND; 65533 for a refusal),
        never registered with the daemon
     -> Unmount (detach, drain) -> checks
 requests.rs   two MountRequests over Overlay files; no kernel
 protocol.rs   pure XDR / RPC / handle values; no socket server
```

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`kernel_mount.rs`](../../../../engine/crates/layerfs-daemon/tests/kernel_mount.rs) | `layerfs-daemon` | Ready evidence; the complete root read through ordinary calls with exact modes, mtimes, owners, listings, bytes and link targets; refusals; reversible Busy; full drain; two Workspaces mounted and unmounted independently. |
| [`kernel_coherence.rs`](../../../../engine/crates/layerfs-daemon/tests/kernel_coherence.rs) | `layerfs-daemon` | Shared-mapping stores are published and never change size; removed names keep exact state; a LOOKUP racing create/unlink/rename never binds a stale inode; size never shrinks under concurrent append; mutations racing the unmount probe persist. |
| [`requests.rs`](../../../../engine/crates/layerfs-fuse/tests/requests.rs) | `layerfs-fuse` | A parked request resumes on its own thread and drain waits for it; `stop_service` wakes an unattempted wait; a panic keeps its original future and the other mount stays usable. |
| [`protocol.rs`](../../../../engine/crates/layerfs-nfs/tests/protocol.rs) | `layerfs-nfs` | XDR round trips and bounds, fragment joining and `RECORD_LIMIT`, `AUTH_UNIX` call headers, handles stable per mount and stale after it. |
| [`write.rs`](../../../../engine/crates/layerfs-nfs/tests/write.rs) | `layerfs-nfs` | Every changing NFS procedure through an in-process export, tickets released only after the reply, silly renames and AppleDouble names. |

```sh
cargo +1.85.1 test --locked -p layerfs-daemon --test kernel_mount -- --test-threads=1
cargo +1.85.1 test --locked -p layerfs-daemon --test kernel_coherence -- --test-threads=1
cargo +1.85.1 test --locked -p layerfs-fuse --test requests
cargo +1.85.1 test --locked -p layerfs-nfs --test protocol
cargo +1.85.1 test --locked -p layerfs-nfs --test write
```

| Test | Needs |
| --- | --- |
| `kernel_mount`, `kernel_coherence` | `#![cfg(target_os = "linux")]`; they mount FUSE, so `/dev/fuse` and `SYS_ADMIN`. The nightly lane runs each binary alone in a fresh container with `--device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined` and `--test-threads=1` ([`nightly.yml`](../../../../.github/workflows/nightly.yml), [`ci_tests.py`](../../../../tools/ci_tests.py)). `map_close_store_exit_process` is a helper that does nothing unless `LAYERFS_R3_MAP_CLOSE_STORE_EXIT` is set by its parent case. |
| `requests` | No platform gate; temporary Overlay files only. |
| `protocol` | No platform gate; no network. |
| `write` | No platform gate; a loopback TCP export inside the test process over a fixture Store (the [rig](../../../../engine/crates/layerfs-nfs/tests/support/rig.rs)); no kernel NFS mount. |

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`engine/patches/fuser-0.18.0/README.md`](../../../../engine/patches/fuser-0.18.0/README.md) — covered in [fuser patches](fuser-patches.md); it stays as the patch record.
- [`docs/general/sqlite-on-network-filesystems.md`](../../../general/sqlite-on-network-filesystems.md) — summarized in [Limits and failures](#limits-and-failures); the guide keeps the preparation steps.
- [`core/docs/architecture/73-native-read-custody.md`](../../../../core/docs/architecture/73-native-read-custody.md) — covered in [Ports](ports.md); historical only for its R2 port names.
- [`core/docs/architecture/75-native-request-service.md`](../../../../core/docs/architecture/75-native-request-service.md) — covered in [FUSE: Dispatch](fuse.md#dispatch) and [Ports](ports.md).
- [`core/docs/architecture/76-native-mount-session.md`](../../../../core/docs/architecture/76-native-mount-session.md) — covered in [FUSE: Session startup](fuse.md#session-startup) and [FUSE: Drain and invalidation](fuse.md#drain-and-invalidation).
- [`core/docs/architecture/77-native-mutation-coherence.md`](../../../../core/docs/architecture/77-native-mutation-coherence.md) — covered in [Coherence](coherence.md); its "no notification" statement predates invalidation after a merged install.
- [`core/docs/architecture/nfs-native-lifecycle.md`](../../../../core/docs/architecture/nfs-native-lifecycle.md) — the export is covered in [NFS](nfs.md); its helper deadlines and teardown belong to the [daemon](../daemon/README.md).

---

← [Commit: Install](../commit/install.md) · [Up: Engine](../README.md) · [Kernel routes: Ports](ports.md) →
