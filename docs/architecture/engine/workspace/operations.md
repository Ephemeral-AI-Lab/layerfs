# Operations

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What each filesystem operation on a [Workspace](README.md) reads and publishes, for path callers (SDK) and kernel callers (FUSE and NFS).

| | |
| --- | --- |
| Source | [`layerfs-workspace/src/operations`](../../../../engine/crates/layerfs-workspace/src/operations) |

## Namespace

```text
 lookup(parent, name)                       SourceView::lookup
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

| `Operation` | POSIX call | Published in one transaction | Source |
| --- | --- | --- | --- |
| `Create`, `Mkdir`, `Symlink` | creat, mkdir, symlink | New inode on a reserved serial, parent entry and counts, symlink target cell | [`create.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/create.rs) |
| `Link` | link | `nlink + 1` and a new name; regular files and symlinks only, never following a symlink target | same |
| `Unlink`, `Rmdir` | unlink, rmdir | Whiteout or removed row, inode `nlink - 1` (directory to 0), parent counts | [`remove.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/remove.rs) |
| `Rename` | rename; `replace: false` is the no-replace form | Unbind source, bind destination, dereference a replaced inode, parent counts | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |

| Rule | Source |
| --- | --- |
| A removed inode keeps a row with `nlink` 0, so serial-addressed access does not fall through to the base | [`remove.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/remove.rs) |
| Moving a directory to another parent needs ancestry evidence: a path caller passes `destination_path`, walked from the root and verified; a kernel caller uses the connection's retained parent index | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs), [`types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Renaming one name of an inode onto another name of the same inode succeeds and changes nothing | [`rename.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/rename.rs) |
| A file renamed over a file records a `predecessor`, offered to Commit as a delta base | same |
| Parent `entries` and `subdirs` move in the same row as the binding; leaving range is an error, never clamped | [`eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs) |
| Aliases of a `Link` share one serial, target bytes and link count; removing one name keeps the others; removing the last drops the inode from the committed namespace | [`create.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/create.rs), [`remove.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/remove.rs) |

## Listing

```text
 base page            active rows          captured rows
 (<= PAGE_ROWS)       (<= PAGE_ROWS)       (<= PAGE_ROWS)
       \                    |                    /
        three-way merge by binary name; for a shared key the captured
        row overrides base and the active row overrides both
                            |
                            v
 ViewListing { entries, continuation = last visited name, visited }
```

`SourceView::list` visits at most `PAGE_ROWS` (1,024) distinct keys per
window, whiteouts included, so a window can be empty and still continue. A
name bound for the whole enumeration is returned exactly once; names changed
meanwhile may or may not appear. A directory created above the installed
floor makes no base demand
([`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs)).

## Attributes

| Field | Rule | Refusal |
| --- | --- | --- |
| `mode` | Permission bits only: `0o777` for files, `0o1777` for directories | Symlink: `Unsupported`; other bits: `NotPermitted` |
| `size` | Files only, up to `i64::MAX`; a change sets mtime unless one is given | Directory: `IsDirectory`; symlink: `Invalid`; larger: `TooLarge` |
| `mtime` | Caller's `Time`; nanoseconds below 10^9 | `Invalid` |
| ctime | Not stored: portable metadata reports mtime as ctime | — |

`SetAttributes` addresses a serial; `SetOpenAttributes` an open descriptor,
which also works on a removed file. A smaller size is a logical cut-off:
discarded bytes are hidden at once and never read again after a regrow.
Access checks belong to the mount's default-permissions identity
([`attributes.rs`](../../../../engine/crates/layerfs-workspace/src/operations/attributes.rs),
[`types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs)).

## Files

```text
 one read window of file F at [offset, offset+len)      len <= 128 KiB
 +------------------------------------------------------------------+
 | local cells (written bytes) | inherited mask | reference mask     |
 +------------------------------------------------------------------+
       | owner job: LocalRead        | outside owner:   | outside owner:
       | (data, masks, refs,         | one base range   | BaseContent of
       |  base_root)                 | over the span    | each ref source
       v                             v                  v
 byte = cell byte | base byte (zero past base EOF) | committed byte
```

| Case | Rule | Source |
| --- | --- | --- |
| `Write`, `WriteOpen` | One window of at most `WRITE_WINDOW`; stores only its own bytes and reads no base payload; uncovered bytes stay inherited | [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs) |
| `Position::End` | Resolves against the size current in the publishing job, so concurrent appends never overlap | same |
| `StoreOpen` | A shared-mapping store, clipped to the current size; never extends the file | [`types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Read, no local row | The whole window is one base read | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |
| Reference byte past its source's end | An error, not zero | same |

Before its first owner visit a kernel write may pass recognition:
`WriteRun::recognize` compares whole chunks of the write with the file's own
committed chunks and turns matches into `WriteClaims`, stored as reference
pieces instead of bytes. A pass that finds nothing, or cannot read a page,
leaves the write unchanged
([`file/recognize.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/recognize.rs)).

| Recognition constant | Value |
| --- | --- |
| `RECOGNIZE_BYTES` | One minimum chunk; shorter writes are not examined |
| `UNMATCHED_BYTES` | 1 MiB of consecutive unclaimed writes stops examination of a file |

## Kernel visits

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

| Visit | Requests | Source |
| --- | --- | --- |
| `kernel_read_visit` | LOOKUP, GETATTR | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| `kernel_open_visit`, `kernel_opendir_visit` | OPEN of a regular file, OPENDIR; record the descriptor | same |
| `kernel_data_visit` | READ, READLINK: copies local bytes and names the base root for the rest; `whole` is a fully local reply, `inherits` says a Store read is needed | [`kernel_data.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_data.rs) |
| `kernel_directory_visit` | READDIR | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
| `kernel_mutation_visit` | Every mutating request | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |

Each visit uses the same evaluator as the path route
([`kernel_read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_read.rs)).
Facts carried between visits are used only while the Workspace still has the
base they came from, so an install between visits costs another read, never
a stale answer. OPEN names an inode the kernel still references, so a file
whose last name is gone stays openable. A data visit holds nothing after its
job.

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
| Window of whiteouts only | Empty with a continuation; the request reads the next window | same |
| Names the kernel buffer accepted | Become resume cookies; nothing is held between the two visits or after the reply | same |

## Kernel mutations

`kernel_mutation_visit` wraps the ordinary `NamespaceJob` in one owner job.
The descriptor the kernel names becomes the `OpenFile` of a handle-addressed
operation; on publication, `effect` hands the reply its kernel custody
([`kernel_mutation.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_mutation.rs)).

| Operation | Custody the applied reply takes |
| --- | --- |
| `Create` with open | `KernelEffect::Open`: the new descriptor, read-only or writable |
| `Create` without open, `Symlink`, `Link` | `KernelEffect::Entry`, not a directory |
| `Mkdir` | `KernelEffect::Entry`, directory |
| Others | `KernelEffect::None` |

The result is `KernelMutationOutcome { result, file }`; `file` is set only
for an applied create-and-open.

## Related

- [Workspace](README.md): the base view, mutation plans, state and ports these operations use.
- [Overlay](../overlay/README.md) and [Overlay: Payload](../overlay/payload.md): the rows behind each answer.
- [Kernel routes](../kernel-routes/README.md): the FUSE and NFS callers of the visits.
- [Store calls](../store-calls/README.md): the SDK callers of path operations.
