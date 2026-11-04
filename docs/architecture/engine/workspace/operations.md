# Operations

[Architecture](../../README.md) › [Engine](../README.md) › [Workspace](README.md) › Operations

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Filesystem operations on a Workspace.

| | |
| --- | --- |
| Component | [Workspace](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/operations`](../../../../engine/crates/layerfs-workspace/src/operations) |

## Namespace operations

```text
  lookup(parent, name)                      SourceView::lookup
     |
     v
  overlay name row for (parent, name)? --yes--> bound serial or whiteout
     | no                                       (active row, else lower)
     v
  parent created above the installed floor? --yes--> ENOENT, no base read
     | no
     v
  BaseView::child(parent, name) -------------> serial or ENOENT
     |
     v
  stat(serial): overlay inode row, else base inode
```

Reads compose through `SourceView`, which pairs the Workspace's current
`BaseView` with one owned `BaseSource` window
([`workspace/view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs)).
Changes are one `Operation` each, decided by the [Mutation](mutation.md)
driver.

| `Operation` | POSIX call | Effect published in one transaction | Source |
| --- | --- | --- | --- |
| `Create`, `Mkdir`, `Symlink` | creat, mkdir, symlink | New inode (reserved serial), parent entry and counts, symlink cell | [`create.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/create.rs) |
| `Link` | link | `nlink + 1`, new parent entry; files only | [`create.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/create.rs) |
| `Unlink`, `Rmdir` | unlink, rmdir | Whiteout or removed row, inode `nlink - 1` (directory to 0), parent counts | [`remove.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/remove.rs) |
| `Rename` | rename, renameat2 no-replace | Unbind source, bind destination, dereference a replaced inode, parent counts | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |
| `SetAttributes`, `SetOpenAttributes` | chmod, utimens, truncate | One inode row | [`attributes.rs`](../../../../engine/crates/layerfs-workspace/src/operations/attributes.rs) |
| `Write`, `WriteOpen`, `StoreOpen` | write, pwrite, mmap store | Inode size/mtime and one payload window | [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs) |

| Rule | Detail | Source |
| --- | --- | --- |
| A removed inode keeps a row with `nlink` 0 | Serial-addressed access does not fall through to the base | [`remove.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/remove.rs) |
| Directory move to another parent needs ancestry proof | A path caller passes `destination_path`, walked from the root; a kernel caller uses the connection's retained parent index | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |
| Rename of one inode onto another name of itself | `Unchanged`, as POSIX requires | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |
| A file renamed over a file | Records `predecessor`, offered to Commit as a delta base | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |
| Parent counts | `entries` and `subdirs` move in the same row as the binding; leaving range is an error, never clamped | [`mutation/eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs) |

### Listing

```text
  base page (<= 64)    active rows (<= 64)    captured rows (<= 64)
        b ---------+         a ---------+           c ---------+
                   v                    v                      v
              three-way merge by binary name; for a shared key the
              captured row overrides base, the active row overrides both
                   |
                   v
  ViewListing { entries, continuation = last visited name, visited <= 64 }
```

`SourceView::list` visits at most 64 distinct keys per window, whiteouts
included, so a window can be empty and still continue. A name bound for the
whole enumeration is returned exactly once; names changed meanwhile may or
may not appear ([`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs)).

## Attributes

| Field | Rule | Refusal |
| --- | --- | --- |
| `mode` | Permission bits only: `0o777` for files, `0o1777` for directories | Symlink: `Unsupported`; other bits: `NotPermitted` |
| `size` | Files only, up to `i64::MAX`; a change sets mtime | Directory: `IsDirectory`; symlink: `Invalid`; larger: `TooLarge` |
| `mtime` | Caller's `Time`; nanoseconds below 10^9 | `Invalid` |
| ctime | Not stored: portable metadata reports mtime as ctime | — |

A smaller size is a logical cut-off: discarded bytes are hidden at once and
never read again after a regrow. A removed file still accepts attribute
changes through an open descriptor
([`attributes.rs`](../../../../engine/crates/layerfs-workspace/src/operations/attributes.rs)).
Access checks belong to the mount's default-permissions identity, not to this
layer ([`types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs)).

## Files

```text
  one read window of file F at [offset, offset+len)      len <= 128 KiB
  +-----------------------------------------------------------------+
  | local cells (written bytes)   | inherited mask | reference mask   |
  +-----------------------------------------------------------------+
        |                                |                 |
        | owner job: LocalRead           | outside owner:  | outside owner:
        | (data, span, masks, refs,      | one base range  | BaseContent of
        |  base_root)                    | covering span   | each ref source
        v                                v                 v
  bytes = cell byte | base byte (zero past base EOF) | committed byte
```

| Piece | Who decides it | Source |
| --- | --- | --- |
| Written byte | Overlay payload cell, read in one owner job | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |
| Inherited byte | Base file at the window's `base_root`, one range outside the owner; past the base end it is zero | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |
| Reference byte | Committed content a claimed range names, at its source offset; past its end is an error | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |
| No local row | The whole window is a base read | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |

A write stores only its own bytes; it reads no base payload, and bytes it does
not cover stay inherited through the layer's cut-off
([`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs)).
`Position::End` resolves against the size current in the publishing job, so
concurrent appends never overlap. A mapped store (`StoreOpen`) is clipped to
the current size and never extends the file.

Before its first owner visit a kernel write may pass through recognition:
`WriteRun::recognize` compares whole chunks of the write with the file's own
committed chunks and turns matches into `WriteClaims`. The claimed ranges
become reference pieces instead of stored bytes. A pass that finds nothing or
cannot read a page changes nothing about the write
([`file/recognize.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/recognize.rs)).

| Recognition constant | Value |
| --- | --- |
| `RECOGNIZE_BYTES` | One minimum chunk: shorter writes are not examined |
| `UNMATCHED_BYTES` | 1 MiB of consecutive unclaimed writes stops examination of a file |

## Kernel reads and visits

```text
  kernel request (no source, no lease)
     |
     v
  visit = one owner job over current rows
     |  needs a base fact?
     |   +-- resident in the canonical cache --> supply, re-decide
     |   |   (at most ROUNDS = 4 per visit)
     |   +-- not resident --> return Needs; nothing written
     v                           |
  decided                        v
                  request reads the fact outside the owner
                  (VisitFacts, tagged with the base root), visits again
```

| Visit | Operations | Source |
| --- | --- | --- |
| `kernel_read_visit` | LOOKUP, GETATTR | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| `kernel_open_visit`, `kernel_opendir_visit` | OPEN of a regular file, OPENDIR; record the descriptor | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| `kernel_data_visit` | READ, READLINK: copies local bytes, names the base root for the rest | [`kernel_data.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_data.rs) |
| `kernel_directory_visit` | READDIR | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
| `kernel_mutation_visit` | Every mutating request | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |

Each visit uses the same evaluator as the path route
([`kernel_read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_read.rs)).
Facts carried between visits are used only while the Workspace still has the
base they were read from, so an install between visits costs another read and
never a stale answer. OPEN names an inode the kernel still references: a file
whose last name is gone stays openable under that reference.

A data visit holds nothing after its job: the reply is the file as it stood at
the visit, over the one base root the visit named. `KernelWindow::whole`
returns a fully local reply; `inherits` says whether a Store read is needed.

## Kernel directories

```text
  READDIR(handle, offset)
     |
     v
  reading visit (read-only job): descriptor fence, cursor at offset,
  one window of local names and kinds
     |  base names in memory?  yes: merge in the job
     |                         no:  merge outside over one Store reader
     v
  KernelDirectoryWindow.finish(base) -> KernelDirectoryBatch
     |  reuse a published reply with the same names and offsets, or
     |  take a fresh offset range (KernelCookieOffer)
     v
  publishing visit records exactly the names the reply accepted
```

| Case | Result | Source |
| --- | --- | --- |
| Offset from before the handle was last listed from 0 | `rewound()`: lists nothing | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
| Window of whiteouts only | Empty with a continuation; the request reads the next window | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
| Only accepted names | Become resume cookies | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |

Nothing is held between the reading and the publishing visit, or after the
reply.

## Kernel mutations

`kernel_mutation_visit` wraps the ordinary `NamespaceJob` in one owner job.
The descriptor the kernel names becomes the `OpenFile` of a handle-addressed
operation (`KernelInput::operation`). On publication, `effect` hands the reply
its kernel custody
([`kernel_mutation.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_mutation.rs)).

| Operation | Kernel custody the applied reply takes |
| --- | --- |
| `Create` with open | `KernelEffect::Open` (the new descriptor, read-only or writable) |
| `Create` without open, `Symlink`, `Link` | `KernelEffect::Entry`, not a directory |
| `Mkdir` | `KernelEffect::Entry`, directory |
| Others | `KernelEffect::None` |

The result is `KernelMutationOutcome { result, file }`; `file` is set only for
an applied create-and-open.

## Related

- [Base](base.md): base reads composed here.
- [Mutation](mutation.md): how one `Operation` is decided and published.
- [Ports](ports.md): the overlay jobs these operations call.
- [Overlay: Namespace](../overlay/namespace.md), [Overlay: Payload](../overlay/payload.md), [Overlay: Open state](../overlay/open-state.md): the rows behind each answer.
- [Kernel routes](../kernel-routes/README.md): FUSE and NFS callers of the visits.

---

← [Workspace: Base](base.md) · [Up: Workspace](README.md) · [Workspace: Mutation](mutation.md) →
