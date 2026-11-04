# Writes

[Architecture](../../README.md) › [Engine](../README.md) › [Store calls](README.md) › Writes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

File writes, edits and recursive changes.

| | |
| --- | --- |
| Component | [Store calls](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/write`](../../../../engine/crates/layerfs-store-service/src/write) |

## File writes

```text
files::commit(service, project, branch, changes, message)
  1 resolve      project -> stack; branch == "main" -> ReadOnly
                 Branch by name                     -> NoSuchBranch
  2 bind         WorkspaceId from a fresh key; service.bind(id, branch)
                 (private Workspace: never listed, mounted or shared)
  3 change       Mutation::new; apply each Change in order;
                 first refusal stops, detail gets {index}
                 mutation.finish() releases the base source
  4 commit       workspace.commit_captured_via(message, CommitVia::Files)
                   head moved -> merged in the same Save (Commit pages)
  5 close        service.release(workspace)
  answer         Committed { head, commit } | UpToDate { head, root }
```

| Outcome | What happens to the private Workspace | Answer | Source |
| --- | --- | --- | --- |
| A change is refused | Closed; nothing committed | That change's refusal, with `index` | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| A change ends `Unknown` | Kept as it is; not closed | `Unknown` | same |
| Commit fails, settled locally | Closed | `HeadMoved`, `Busy`, `Capacity` or `Failed` | same |
| Commit outcome uncertain | Kept by `StoreService::retain` | `Unknown`, with Workspace id and namespace | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| Head moved and a changed path conflicts | Closed; nothing published | `Conflict` with `branch`, `head`, `conflicts` | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| Published | Closed | `Committed` or `UpToDate` | same |

The default message is `Apply filesystem changes`. The Commit itself (capture,
construction, merge with a moved head, publish) is the same code a mounted
Workspace uses; see [Commit](../commit/README.md).

| Change | Fields | Effect | Source |
| --- | --- | --- | --- |
| `write` | `path`, `content` (text) or `bytes`, `mode?` | Replace the file's bytes; a new file is 0o644, an existing one keeps its mode unless `mode` is given. | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `edit` | `path`, `oldText`, `newText` | Replace the one occurrence. | [`write/edit.rs`](../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `mkdir` | `path`, `mode?` | Create it and missing parents; `path` gets `mode` (default 0o755), parents 0o755. | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `chmod` | `path`, `mode` | Set permission bits. | same |
| `rm` | `path` | Remove, recursively. | [`write/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/write/recursive.rs) |
| `cp` | `from`, `to` | Copy, recursively, keeping hard links. | same |
| `mv` | `from`, `to` | Rename. | same |

A path must be absolute, at most 4096 bytes, without NUL, without a trailing
`/`, and without empty, `.` or `..` components (`files::path`). A mode is an
integer 0..=0o7777; set-user-ID, set-group-ID and sticky on a file, or any mode
on a symlink, is refused.

A write is applied in pieces of at most `MAX_PLAINTEXT_BYTES` (65,519 bytes)
so each overlay job stays bounded whatever the file size. When the file
already has the new length, each piece is compared first and an equal piece is
not written ([`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs)).

## Edits

```text
edit(path, old, new)
  old empty                          -> InvalidInput
  path not a regular file            -> NotFile
  scan file in 32 KiB windows (KMP over old)
    NUL or invalid UTF-8             -> Binary
  matches == 0                       -> NoMatch
  matches > 1                        -> AmbiguousMatch {matches}
  old == new                         -> no change
  shift tail by (len(new) - len(old)) in 32 KiB windows
    growing: from the end backwards; shrinking: from the tail forwards
  write new at the match; SetAttributes { size }
```

The edit reads and writes only bounded 32 KiB windows, so memory does not grow
with file size ([`write/edit.rs`](../../../../engine/crates/layerfs-store-service/src/write/edit.rs)).

## Recursive writes

```text
rm(dir)                                 cp(from, to)
  stack of Frame{path, serial, after}     overlap(from, to)?  -> InvalidInput
  list one page of the directory          to exists?          -> Exists
  file child   -> Unlink                  acquire operation record owner
  dir child    -> push Frame              copy_one(top) then walk from:
  frame done   -> pop, Rmdir                regular file seen before
                                              (record key = source serial)
mv(from, to)                                  -> Link to the first copy
  overlap? -> InvalidInput                  else Create/Mkdir/Symlink,
  to exists? -> Exists                        copy bytes in 32 KiB windows,
  one Rename (replace: false)                 record source -> new serial
                                          release the owner (kept if Unknown)
```

| Rule | Source |
| --- | --- |
| Directory reads are pages from the view's `list`, so the stack holds one page per level, not the whole tree. | [`write/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/write/recursive.rs) |
| The hard-link map lives in the private overlay's indexed operation records (kind `0x53444b43`), not in memory. | same |
| `from` and `to` overlap when equal, when `from` is `/`, or when `to` lies under `from`. | same |
| `rm /` and replacing the root are refused `InvalidInput`. | same, [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |

## Mutations

```text
Mutation::apply(operation)
  serial = reserve from pool if operation creates (64 at a time)
  plan = workspace.prepare_mutation(view, operation, now, serial)
  loop:
    result = OverlayJobs::namespace(overlay, plan.job())
    plan.accept(result):
      Applied { publication, stat } -> overlay.reply_attempted; done
      Unchanged { stat }            -> done
      None                          -> plan.supply(view); next job
```

`Mutation` holds the private Workspace's `StoreOperation`, one `SourceView` of
its base and one timestamp for the whole call. Each overlay job is one short
locked job on the private overlay; no lock spans the call
([`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs)).
Serials come from `StorePorts::reserve_pooled`, which serves them from the
Store's in-memory pool when it can, so a small file call often opens no writer
transaction for them
([`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs)).

| Workspace error | Refusal code |
| --- | --- |
| `Refused(Missing)` or `PathNotFound` | `NoSuchPath` |
| `Refused(Exists)` | `Exists` |
| `Refused(IsDirectory)` | `IsDirectory` |
| `Refused(NotDirectory)` | `NotDirectory` |
| `Refused(NotEmpty)` | `NotEmpty` |
| Other `Refused` | `InvalidInput` |
| A Store port's definite no-space refusal | `Capacity` |
| Anything else during a change | `Unknown` (the Workspace is kept) |
| Anything else during a read of the view | `Failed` (a read changes nothing) |

A name below a file or symlink is `NotDirectory` at that entry, as `mkdir`
answers; a name the name grammar refuses is `InvalidInput`.

## Related

- [Service](service.md) — `bind`, `release` and `retain`.
- [Commit](../commit/README.md) — what `commit_captured_via` does.
- [Workspace: Mutation](../workspace/mutation.md) — `prepare_mutation` and plans.
- [Overlay: Operation records](../overlay/operation-records.md) — the hard-link map's storage.
- [SDK facade](sdk-facade.md) — how `fs.*` arguments become `Change` values.

---

← [Store calls: Reads](reads.md) · [Up: Store calls](README.md) · [Store calls: Branches](branches.md) →
