# Ports

[Architecture](../../README.md) › [Engine](../README.md) › [Kernel routes](README.md) › Ports

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Route-neutral ports and operations shared by FUSE and NFS.

| | |
| --- | --- |
| Component | [Kernel routes](README.md) |
| Source | [`engine/crates/layerfs-mount-ports/src/ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs), [`engine/crates/layerfs-mount-ports/src/operations`](../../../../engine/crates/layerfs-mount-ports/src/operations), [`engine/crates/layerfs-mount-ports/src/phase.rs`](../../../../engine/crates/layerfs-mount-ports/src/phase.rs) |

`layerfs-mount-ports` holds no kernel protocol
([`lib.rs`](../../../../engine/crates/layerfs-mount-ports/src/lib.rs)). It
names what one kernel request may ask of a mounted Workspace, and it drives
each request through those calls. FUSE and NFS both call it; the daemon
implements it.

## Ports

```text
  layerfs-fuse                      layerfs-nfs
  KernelFilesystem                  Kernel (synthetic kernel)
        |                                 |
        |  request(&Fence)                |  request(&Fence)
        v                                 v
  +--------------------------------------------------------+
  | MountServices        one shared mounted Workspace      |
  |   request(fence) --> Arc<dyn RequestServices>          |
  +--------------------------------------------------------+
  | RequestServices      one kernel request                |
  |   acquiring (refused once the fence is stopped):       |
  |     observe_visit  read_visit  mutate_visit            |
  |     directory_visit  publish_cookies  base             |
  |     recognize  reserve_serial                          |
  |   disposal (never refused by the fence):               |
  |     reply_attempted  close_file  close_directory       |
  |     forget                                             |
  |   failure custody:                                     |
  |     provider_failure  failed_base_read                 |
  +--------------------------------------------------------+
        ^
        | impl MountServices for Mounted
  layerfs-daemon  service/filesystem_port.rs
  (owner jobs under the Workspace lock, Store readers)
```

| Item | What it is | Source |
| --- | --- | --- |
| `MountServices` | One mounted Workspace. `request` creates a fresh per-request scope with no provider I/O, no new cache and no whole-Workspace lock. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `RequestServices` | The calls one kernel request makes. No daemon command, registry or configuration crosses it. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `Fence` | One mount's terminal stop. Only forced teardown sets it, once; nothing clears it. It also keeps the mount's `FailedDemands` record. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `Fenced` | A stopped fence refused a call before its attempt. No job was submitted. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `FailedDemands` | Count of failed base demands, the first cause and the latest cause, in fixed slots that are never cleared. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `BaseDemandFailed` | A Store read made for one request failed. Only `Fence::failed_demand` builds it, so every marker has its record. It says nothing about the mount. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `ServiceReply<T>` | A value plus the service's completion receipt. The value drops before the receipt. | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `Mounted` | The daemon's implementation over a `BoundWorkspace`. It checks the fence on entry to and on every poll of the owner-admission and Store-reader waits, and on entry to `reserve_serial`. | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |

A job that was already admitted is awaited to its original result, even after
the fence stops. A stopped mount starts nothing new and still gives back what
it holds.

## Operations

```text
  LOOKUP / GETATTR / OPEN / OPENDIR       KernelRead::prepare
    loop:
      observe_visit(facts) ---- owner job, records no request source
        Value(Decided) -> answer (OPEN/OPENDIR wrote a descriptor;
                          a LOOKUP also carries the directory it named)
        Refused(r)    -> refusal
        Needs(n)      -> drop receipt; base(); facts.supply(n); repeat

  READ / READLINK                          KernelData::read
      read_visit ------------------- owner job: local window bytes
        whole?  -> reply from the receipt
        else    -> copy window; base() only if bytes are inherited

  mutation                                 KernelMutation::perform
      reserve_serial (creating only) -- None => Declined::Contended
      recognize (WRITE, once) -------- claims for committed chunks
      loop:
        mutate_visit -------------- owner job
          Applied{publication} -> owes one reply_attempted
          Unchanged / Refused  -> no ticket
          Needs(n)             -> base(); facts.supply(n); repeat
      ... reply attempt ...
      replied() -> reply_attempted(publication), exactly once

  READDIR                                  DirectoryStream
      directory_visit -> window -> finish (base() if not in memory)
      batch -> kernel buffer takes k entries
      accept(k) -> publish_cookies(offer, first k names), before reply
```

| Type | Kernel requests | Holds in the engine after its visits | Source |
| --- | --- | --- | --- |
| `KernelRead` | LOOKUP, GETATTR, OPEN, OPENDIR | Nothing; the kernel's lookup and open owners persist on their own. | [`lookup.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/lookup.rs) |
| `KernelData` | READ, READLINK | Nothing. Dropping the value ends the request. | [`read.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/read.rs) |
| `KernelMutation` | every changing request | One `Publication` ticket when it published, until `replied` | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| `DirectoryStream`, `DirectoryBatch` | READDIR | Nothing; only accepted names become valid offsets. | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |

Base reads always happen between owner visits, after the visit's receipt is
dropped, so a request never holds the Workspace lock while it waits on the
Store.

The small modules turn kernel arguments into Workspace `Operation`s and
decide the refusals that need no engine work:

| Module | Builds or decides | Source |
| --- | --- | --- |
| `create` | CREATE, MKNOD (regular only), MKDIR, SYMLINK. Mode is masked to `0o7777`. A symlink target of at least one page is `Declined::TargetTooLong`. | [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs) |
| `write` | WRITE at the kernel's offset (at most `WRITE_WINDOW`); SETATTR of mode, size and mtime; signed `time`. | [`write.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/write.rs) |
| `link`, `remove`, `rename` | LINK, UNLINK, RMDIR, RENAME. Rename flags `0` replace, `1` (no-replace) does not, any other flag is `Invalid`. | [`link.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/link.rs), [`remove.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/remove.rs), [`rename.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/rename.rs) |
| `flush` | FLUSH, FSYNC, FSYNCDIR answer `Synchronize::NotImplemented`: nothing is buffered in the daemon. | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs) |
| `unsupported` | Extended attributes, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE are `Absent::NotImplemented`. FIFOs, sockets and devices are `NotPermitted`. A uid or gid other than the configured identity is `NotPermitted`. | [`unsupported.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/unsupported.rs) |

`Declined` is a definite answer with no publication: `Refused(Refusal)`,
`NoSpace`, `Contended` or `TargetTooLong`. Every failure type
(`ReadFailure`, `MutationFailure`, `DirectoryFailure`) keeps the request's
identity and what it still owns. Its `relinquish` ends only a request that
was fenced or whose base demand failed; any other failure comes back
unchanged for the route to retain. Nothing is retried, and a published
mutation is never undone.

## Phase timing

```text
  receive worker thread
  +--------------------------------------------------------------+
  | FUSE callback                                                |
  |   Timed: phase::enter(mount Phases) ... phase::restore(prev) |
  |     observe_visit ----- charge(ReadVisit)                    |
  |     base + supply ----- charge(ReadSupply)                   |
  |     mutate_visit ------ charge(MutateVisit)                  |
  |     ...                                                      |
  +--------------------------------------------------------------+
```

| `Phase` | Charged around | Source |
| --- | --- | --- |
| `ReadVisit` | LOOKUP/GETATTR/OPEN/OPENDIR owner visit job | [`lookup.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/lookup.rs) |
| `ReadSupply` | Base facts read for an undecided read visit | [`lookup.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/lookup.rs) |
| `MutateVisit` | Mutation owner visit job | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| `MutateSupply` | Base facts read for an undecided mutation visit | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| `MutateRelease` | The `reply_attempted` job after a published mutation's reply | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| `DirectoryVisit` | READDIR window job | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |
| `DirectoryFinish` | Base listing read to finish a window | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |
| `Forget` | FORGET owner job | [`inline.rs`](../../../../engine/crates/layerfs-fuse/src/request/inline.rs) |
| `CloseDirectory` | RELEASEDIR owner job | [`request/directory.rs`](../../../../engine/crates/layerfs-fuse/src/request/directory.rs) |

`Phases` holds one count and one nanosecond total per phase
([`phase.rs`](../../../../engine/crates/layerfs-mount-ports/src/phase.rs)).
The running callback's `Phases` sits in a thread-local; `charge` adds to it,
and does nothing when no callback is current. Only the FUSE adapter's `Timed`
guard makes a mount current
([`accounting.rs`](../../../../engine/crates/layerfs-fuse/src/request/accounting.rs)),
so NFS calls charge nothing.

## Related

- [FUSE](fuse.md) and [NFS](nfs.md): the two routes that call these ports.
- [Coherence](coherence.md): reply order and the post-reply ticket release.
- [Workspace: Ports](../workspace/ports.md) and
  [Workspace: Operations](../workspace/operations.md): the visit types behind
  `RequestServices`.
- [Overlay: Open state](../overlay/open-state.md): descriptors and lookup
  references the visits record.
- [Daemon](../daemon/README.md): the `Mounted` implementation.
- [Glossary](../../glossary.md).

---

← [Engine: Kernel routes](README.md) · [Up: Kernel routes](README.md) · [Kernel routes: FUSE](fuse.md) →
