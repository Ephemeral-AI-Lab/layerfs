# Open state

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Open state

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Open files, directories and kernel references, held in memory.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/open`](../../../../engine/crates/layerfs-overlay/src/open), [`contract/custody.rs`](../../../../engine/crates/layerfs-overlay/src/contract/custody.rs), [`lifetime/`](../../../../engine/crates/layerfs-overlay/src/lifetime) |

Open state is the `OpenState` value inside one Workspace's `Overlay`. It is
never written to the file. It changes only inside an atomic job under the
Workspace lock, and it commits or rolls back with that job's SQL.

```text
Overlay.open: RefCell<OpenState>
+---------------------------------------------------------------------+
| mount       Option<(KernelMount, revoked)>   one kernel connection   |
| kernel      (mount owner, serial) -> KernelReference{owner,count,    |
|                                                     implicit}       |
| parents     directory serial -> (mount owner, parent serial)        |
| files       OwnerTable<(mount, request), OpenFile>                  |
| directories OwnerTable<(mount, request), DirectoryState>            |
| cookies     directory owner -> CookieStore (published READDIR pages)|
| reads       OwnerTable<request, FileRead>                           |
| lookups     OwnerTable<request, LookupOwner>                        |
| operations  OwnerTable<request, OperationOwner>                     |
| captured    OwnerTable<request, CapturedReader>                     |
| custody     serial -> FileReferences{opens, lookups, readers}       |
| sources     (owner, class) -> BaseSource                            |
| orphans     serial -> Orphan{root, top, floor}                      |
| kept_live, digests, resumable, waits, captured_generations          |
+---------------------------------------------------------------------+
```

Source: [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs).
`OpenStateCounts` reports the size of each map
([`open/counts.rs`](../../../../engine/crates/layerfs-overlay/src/open/counts.rs)).
These maps grow with open files and kernel references and have no cap; this
is the one resident exception in the [repository mental model](../../../../AGENTS.md).

## Open files

| Token | Meaning | Acquire / release | Source |
| --- | --- | --- | --- |
| `OpenFile` | One open descriptor: engine, owner, serial, `writable` | `open_file` or a kernel open visit / `close_file`, `kernel_close_file` | [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs), [`kernel_file.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_file.rs) |
| `FileRead` | One read window with its own `BaseSource`; survives descriptor close | `acquire_file_read` / `release_file_read` | [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs) |
| `LookupOwner` | One lookup reference, kept until exact release | `acquire_lookup` / `release_lookup` | [`lookup.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/lookup.rs) |
| `BaseSource` | A short window on the current immutable base; class 0 request, 1 file read, 3 visit (no row), 4 diagnostic | `acquire_base_source` / `release_base_source` | [`source.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/source.rs) |

| Rule | Source |
| --- | --- |
| Owner numbers come from one process-wide counter and are never reused, so a stale token is `Stale`. Copying a token acquires nothing. | [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs), [`custody.rs`](../../../../engine/crates/layerfs-overlay/src/contract/custody.rs) |
| Each owner table is also indexed by the caller's request, so `retained_file`, `retained_file_read`, `retained_lookup` observe custody after a lost reply. Absence never permits a repeat. | [`journal.rs`](../../../../engine/crates/layerfs-overlay/src/open/journal.rs) `OwnerTable` |
| `custody` counts opens, lookups and readers per serial. When the total reaches zero the file's custody is dropped, or an orphan's release starts. | [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs) `file_ref` |
| `base_readers` in `WorkspaceState` counts held `BaseSource`s; install waits for it to be zero. | [`workspace.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/workspace.rs) |

## Open directories

```text
OPENDIR visit      -> DirectoryState { KernelDirectory, floor }
READDIR visit      read-only: KernelDirectoryPage (cursor, local names)
offer_kernel_cookies   reuse the latest page after the same name, or a
                       fresh range of PAGE_ROWS cookie numbers
publish_kernel_cookies one CookiePage per reply: exactly the names the
                       kernel buffer accepted
RELEASEDIR         close_kernel_directory: handle and pages detached
                   -> OpenDisposals, dropped after the lock is released
```

| Rule | Source |
| --- | --- |
| Dot entries use fixed cookies 1 and 2; every other cookie keeps its exact name boundary. An offset from before the last rewind is `Rewound` and lists nothing. | [`kernel_directory.rs`](../../../../engine/crates/layerfs-overlay/src/contract/kernel_directory.rs) |
| Immutable names and kinds are merged with the local page outside SQL. | [`kernel_directory.rs`](../../../../engine/crates/layerfs-overlay/src/contract/kernel_directory.rs) |
| A large detached listing is destroyed after the lock is released, so freeing it holds no Workspace lock. | [`open/directories.rs`](../../../../engine/crates/layerfs-overlay/src/open/directories.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |

## Journal

```text
job start         put/remove on JournalMap       job end
   |              rows.insert + undo.push(old)      | ok:  undo.clear()
   |              rows.remove + undo.push(old)      | err: replay undo in
   |                                                |      reverse order
```

| Type | Role | Source |
| --- | --- | --- |
| `JournalMap<K, V>` | `BTreeMap` with an undo list of only the touched keys; never copies the map | [`journal.rs`](../../../../engine/crates/layerfs-overlay/src/open/journal.rs) |
| `OwnerTable<R, V>` | Two journaled maps (owner → value, request → owner); a duplicate request is refused | [`journal.rs`](../../../../engine/crates/layerfs-overlay/src/open/journal.rs) |
| `OpenState::commit` / `rollback` | Called by the job transaction; cookie stores have their own undo log | [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |

On a known whole-job failure the transaction also restores `mount`,
`WorkspaceState`, the `orphan_seen`/`pieces_seen` hints and withdraws the
job's reply tickets. On an unknown outcome nothing is restored and the
engine is quarantined.

## Kernel references

```text
create_kernel_mount(root)    mount = (KernelMount, revoked=false)
                             kernel[(owner, root)] = implicit reference
LOOKUP / CREATE visit        kernel[(owner, serial)].count += 1
FORGET                       kernel_forget(mount, serial, n): count -= n;
                             at 0 (not implicit) the row and its lookup
                             reference on the file go
after detach + drain         revoke_kernel_mount: revoked = true
next attach                  retire_revoked_mount drops leftovers
```

| Rule | Source |
| --- | --- |
| One kernel connection per engine at a time; `KernelMountState` is `Live`, `Revoked` or `Gone`. | [`lifetime/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel.rs), [`contract/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/contract/kernel.rs) |
| A kernel request records nothing of its own: one visit job is its whole window in the owner; a visit that cannot decide changes nothing. | [`kernel_visit.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_visit.rs) |
| A mutation's reply custody (`KernelEffect`: entry or open descriptor) is acquired in the publishing transaction, before any reply attempt. | [`kernel_mutation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_mutation.rs), [`contract/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/contract/kernel.rs) |
| An underflowing FORGET is `Invalid`; a foreign mount or row is `Stale`. Nothing compensates a lost send. | [`lifetime/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel.rs) |
| `parents` keeps a directory's retained parent for `..` and moved directories (`Changes.moved_directory`). | [`kernel_directory.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_directory.rs) |

## Related

- [Lifetimes and reclamation](lifetimes-and-reclamation.md): what a released reference starts.
- [Database](database.md): the job transaction that journals this state.
- [Kernel routes: FUSE](../kernel-routes/fuse.md) and [Kernel routes: Coherence](../kernel-routes/coherence.md): the kernel side of these references.
- [Daemon: Control calls](../daemon/control-calls.md): attach, revoke and close.
- [Glossary](../../glossary.md).

---

← [Overlay: Payload](payload.md) · [Up: Overlay](README.md) · [Overlay: Lifetimes and reclamation](lifetimes-and-reclamation.md) →
