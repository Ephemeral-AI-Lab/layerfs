# Coherence

[Architecture](../../README.md) › [Engine](../README.md) › [Kernel routes](README.md) › Coherence

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Keeping kernel caches coherent with the Workspace.

| | |
| --- | --- |
| Component | [Kernel routes](README.md) |
| Source | [`engine/crates/layerfs-fuse/src/coherence`](../../../../engine/crates/layerfs-fuse/src/coherence), [`engine/crates/layerfs-fuse/src/session/invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs), [`engine/crates/layerfs-nfs/src/refresh.rs`](../../../../engine/crates/layerfs-nfs/src/refresh.rs) |

FUSE keeps the kernel's caches exact without notifications for ordinary file
access: every change reaches the daemon as a request on the same connection.
Notifications are sent only after a Commit installs a root other than the
one the Workspace showed. NFS has no notification channel and relies on the
client's revalidation plus a refresh after such an install.

## Attribute and page coherence

```text
 FUSE kernel caches            source of every value
 +-----------------------+     +----------------------------------------+
 | dentry -> inode       |<----| LOOKUP/CREATE/MKDIR/... reply, TTL 60s |
 | negative dentry       |<----| LOOKUP Missing: node 0, entry TTL 60s  |
 | inode attributes      |<----| reply built from the published ViewStat|
 | page cache (per inode)|<----| READ replies; WRITE updates it itself  |
 +-----------------------+     +----------------------------------------+
   one serial = one inode = one attribute set = one page cache,
   for every hard-link name
```

| Rule | FUSE | NFS | Source |
| --- | --- | --- | --- |
| Attributes come from published state | `published(value, expected)`: no inode is `Incoherent::Unpublished`; a different serial than the one addressed is `Incoherent::Foreign`; the reply is refused, never composed from request inputs | Reply composed only when the published stat names the expected serial; else `NFS3ERR_IO` and the mutation is retained | [`coherence/attributes.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/attributes.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Entry and attribute lifetime | `TTL` 60 s; it bounds how long the kernel may answer alone, it never makes an answer correct | Client `actimeo=60` and close-to-open | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Cached pages on open | `FOPEN_KEEP_CACHE` on, `FOPEN_DIRECT_IO` off, for every opened or created file | Client's own cache | [`coherence/pages.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/pages.rs) |
| WRITE count | Full count or an error, never short; a short reply would make the kernel shrink the size or resend a published tail | Short only after a later window failed; the earlier windows stay published | [`coherence/pages.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/pages.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Hard links | Aliases share one serial, so one kernel inode | `fileid` is the serial | [`link.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/link.rs), [`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs) |
| Changed directory | Reply of the mutation | `wcc_data`: pre-op and post-op attributes from separate visits; an unexplained difference makes the client drop its cache | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |

## Reply order

```text
 request A (mutation)            request B (read class)
   mutate_visit ----+
                    | Workspace lock: one owner job at a time
                    v
   COMMIT of A's job (publication P_A, ticket T_A)
   reply A composed from P_A       observe_visit runs after B was
   reply A attempted               received; sees P_A if after it
   reply_attempted(T_A)  <-- only after A's reply attempt
                                   reply B from its own job
 kernel: drops an attribute reply sampled before a newer inode
         version; the daemon relies on that, not on wire order
```

| Rule | Source |
| --- | --- |
| A mutation takes effect at the commit of its single owner job; the one SQL owner orders every publication. | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs) |
| The reply is computed from that job's published values, so it is never older than the state it reports. | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs) |
| fuser reports no send result, so release follows the reply *attempt*; a published mutation is never undone. | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs) |
| A failed mutation keeps every owner it still has; nothing is released, resent or rolled back on a guess. | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs) |
| READDIR publishes exactly the accepted names as offsets before its reply. | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |
| NFS: one call at a time per connection; tickets released after `write_record` returns. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |

## Invalidation

```text
 Commit installed root R' while the Workspace showed R   (R' != R)
   daemon control/invalidate.rs  Service::invalidate
     |
     +-- NFS entry?  installed_export:
     |     Server::installed(): close file + directory descriptors
     |     stale_after_install(R, R') -> serials (at most REFRESHED=4096)
     |     Server::refresh(serials): mark them, return their paths
     |     thread layerfs-nfs-refresh: chmod(path, same mode) per path
     |       client SETATTR -> export: marked + mode-only?
     |         yes: answer current attributes, change nothing
     |       client sees new post-op attrs -> drops its cache
     |
     +-- FUSE entry (Linux):
           stale_after_install(R, R') ->
             Stale::Inode(serial)        -> Invalidator::inode
             Stale::Entry{parent, name}  -> Invalidator::entry
```

| Item | Rule | Source |
| --- | --- | --- |
| When | Only after an install of a root other than the one shown (a clean merge or a merge with markers), before the caller's answer. An install that changes nothing the kernel showed sends nothing. | [`invalidate.rs` (daemon)](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs), [`coherence/pages.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/pages.rs) |
| `Invalidator::inode(serial)` | One `inval_inode(inode, 0, 0)`: attributes and every cached page or listing | [`session/invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs) |
| `Invalidator::entry(parent, name)` | One `inval_entry`: the cached binding of one name | [`session/invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs) |
| Uncached target | `ENOENT`, which fuser reports as success; nothing is retried | [`session/invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs) |
| Negative entries | Every create, link, mkdir, symlink and rename through the mount replaces them; a merged install invalidates every added name | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs) |
| NFS refresh mark | Taken by one mode-only SETATTR; ends after `MARK_LIFE` (30 s); a serial the client never saw is left out | [`refresh.rs`](../../../../engine/crates/layerfs-nfs/src/refresh.rs) |
| NFS refresh off | `LAYERFS_NFS_REFRESH=0` leaves the client to `actimeo` and close-to-open | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| NFS enumeration | A continuing READDIR after an install answers `NFS3ERR_BAD_COOKIE`; the client lists again | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |

The refresh never writes into the Workspace and never reverts a mode the
install changed: the export answers the marked SETATTR with what it already
holds.

## Writeback off

```text
 FUSE                                   NFS
 write(2) -> WRITE request              write(2) -> client page cache
          -> published by its owner job           -> WRITE (FILE_SYNC answer:
          -> reply -> write(2) returns                each window published)
 msync / mapped store -> WRITE          Commit: flush helper (sync(2) by
   with FUSE_WRITE_CACHE (cached=true)          default) first, then
 FLUSH/FSYNC/FSYNCDIR -> ENOSYS once            close_files, then Commit
```

| Rule | Source |
| --- | --- |
| `FUSE_WRITEBACK_CACHE` is never selected: the adapter keeps fuser's Linux defaults `FUSE_ASYNC_READ` and `FUSE_BIG_WRITES`, plus `FUSE_MAX_PAGES` when offered. | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| Every `write(2)` is published by its own WRITE before it returns; nothing is buffered in the daemon for FLUSH or FSYNC to push. | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs) |
| FLUSH, FSYNC and FSYNCDIR answer `ENOSYS`; the kernel then returns success for them and stops sending them. No engine job, Store seal or durability claim follows. | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs), [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| A shared-mapping store arrives as WRITE with `FUSE_WRITE_CACHE`; it is counted in `store_units`. | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| The Overlay is disposable and is never synchronized to stable media. | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs) |
| NFS WRITE answers `FILE_SYNC`, which means published, not durable. The client may still hold writes, so the daemon flushes the client before each Commit. | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |

## Related

- [Ports](ports.md): `KernelMutation::replied` and `publish_cookies`.
- [FUSE](fuse.md) and [NFS](nfs.md): the two routes.
- [Commit: Install](../commit/install.md): the install that triggers
  invalidation.
- [Workspace: State](../workspace/state.md): the published state replies
  are built from.
- [Daemon](../daemon/README.md): who calls `invalidate`.
- [Glossary](../../glossary.md).

---

← [Kernel routes: NFS](nfs.md) · [Up: Kernel routes](README.md) · [Engine: Daemon](../daemon/README.md) →
