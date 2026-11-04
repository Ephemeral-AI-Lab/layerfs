# State

[Architecture](../../README.md) › [Engine](../README.md) › [Workspace](README.md) › State

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Workspace state, serials, route and view.

| | |
| --- | --- |
| Component | [Workspace](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/workspace`](../../../../engine/crates/layerfs-workspace/src/workspace) |

## Workspace state

```text
  Workspace  (logical binding; SQL state lives in the overlay crate)
  +--------------------------------------------------------------+
  | route             Route                (copy)                |
  | base              Arc<RwLock<BaseView>>  current base root   |
  | serials           Arc<Mutex<Serials>>    reserved ranges     |
  | operation_client  Option<Arc<CanonicalClient>>               |
  +--------------------------------------------------------------+
         |                                   ^
         | base(): clone under a read lock,  | install_prepared_base:
         | no lock held across content I/O   | write lock, one SQL step
         v                                   |
     BaseView for one operation        (see Install below)
```

| Entry | Does | Source |
| --- | --- | --- |
| `Workspace::open(overlay, client, root, scope, incarnation)` | Opens the base (one root acquisition), builds the route, calls `overlay.open_workspace` | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |
| `Workspace::bind(route, base)` | Binds a checked root to an already-open route; used by the Store-service mount | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs), [`store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| `scoped(client)` | Same base cell and serials with one operation's provider; no new namespace, root acquisition or serial range | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |
| `base()` | Clones the current `BaseView` under a read lock | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |

| `WorkspaceError` | Meaning |
| --- | --- |
| `Content`, `Overlay` | The original error of that layer |
| `BindingPoisoned` | A base or serial lock was poisoned |
| `MissingLengthProvider` | No length port where one is required |
| `Service` | A port implementation's own error, kept whole |
| `Refused(Refusal)` | Definite pre-effect refusal |
| `BaseChanged { expected, actual }` | The base root is not the one the caller holds |

Kernel attachment is separate from `open`: the daemon composes it
([Daemon](../daemon/README.md)).

## Serials

```text
  Serials.ranges: [(next, end), ...]     consumed from the last range
                                   +----------------+
  next_serial ---------------------> last range     |-- serial
                                   +----------------+
     no range left?  -> allocator.reserve(1024)  outside every lock
     left < low_water? -> one early reserve, placed behind the
                          remainder; its failure returns beside the serial
```

| Rule | Value or behaviour | Source |
| --- | --- | --- |
| Refill window | `SERIAL_REFILL` = 1,024 serials per allocator call; not a cap on created inodes | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| Allocator | `InodeSerials::reserve` returns a half-open range unique in the scope; Workspace never invents serials | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| Unused serials | Consumed, never recycled | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| Range check | Start and count non-zero; end at most `MAXIMUM_INODE_SERIAL` | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| `prepare_serials` | One initial range before a newly attached kernel Workspace is exposed | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs), [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| Low-water | `next_serial_with_low_water`: the daemon passes its configured `serial_low_water`, which must be below `SERIAL_REFILL` | [`config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| One call, one allocator request | A call never makes two allocator requests | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |

## Route

```text
  Route  (Copy)
  +---------------------------------------------+
  | namespace    i64 > 0   selects the handle   |
  | engine       u64 != 0  the overlay engine   |
  | incarnation  [u8; 32] != 0                  |
  +---------------------------------------------+
```

`Route` is daemon routing metadata, separate from overlay ownership and
filesystem state. Engine and incarnation stop a stale route from selecting its
replacement; every source, file, capture and kernel mount is checked against
`route().engine()` and refused `OverlayError::Stale` when it differs
([`route.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/route.rs)).

## View

```text
  view_for_source(BaseSource)
     | source.engine == route.engine?          else Stale
     | source.root   == current base root?     else BaseChanged
     v
  SourceView { base: BaseView, source: BaseSource }
     stat, lookup, readlink, list, read, read_file, read_captured
```

| Type | Holds | Source |
| --- | --- | --- |
| `SourceView` | One retained `BaseView` and one owner-issued `BaseSource` window; the caller releases the source after its work is fenced, and `Drop` runs no SQL | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |
| `ViewStat` | serial, kind, portable metadata, `namespace_refs`, `logical_len`, `subdirs` | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |

`ViewStat::subdirs` is the number of child directories; a directory's POSIX
link count is two more while it has a name. `namespace_refs` is the canonical
reference count, not POSIX `nlink` for a directory. `stat` reads the overlay
row, then the base, then the overlay row again, so a row published during the
base read still wins. Operations over a `SourceView` are in
[Operations](operations.md).

## Install

After a Commit is known published, or the Workspace is `UpToDate`, the base
advances to the new root in two steps
([`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs)):

| Step | Lock held | Source |
| --- | --- | --- |
| `prepare_base_install(capture, root)` | None; opens the new root outside every lock | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| `install_prepared_base(overlay, prepared)` | Base write lock for one SQL step (`overlay.install`); nothing fallible after it | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |

Changes made after the capture stay in the overlay. Failure returns the
original error with the `PreparedBase`. The full sequence is in
[Commit: Install](../commit/install.md).

## Related

- [Base](base.md): the `BaseView` held in the base cell.
- [Operations](operations.md): reads over a `SourceView`.
- [Mutation](mutation.md): where serials are consumed.
- [Commit: Install](../commit/install.md): advancing the base.
- [Daemon](../daemon/README.md): attach, route and serial configuration.

---

← [Workspace: Ports](ports.md) · [Up: Workspace](README.md) · [Engine: Commit](../commit/README.md) →
