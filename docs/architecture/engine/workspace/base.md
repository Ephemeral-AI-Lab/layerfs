# Base

[Architecture](../../README.md) › [Engine](../README.md) › [Workspace](README.md) › Base

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The committed base a Workspace reads through.

| | |
| --- | --- |
| Component | [Workspace](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/base`](../../../../engine/crates/layerfs-workspace/src/base) |

## The base view

```text
  BaseView (cheap to clone; no lock held across I/O)
  +--------------------------------------------------+
  | client    Arc<CanonicalClient>  -- reads objects |
  | identity  FilesystemRootId      -- the root ID   |
  | root      FilesystemRoot        -- checked root  |
  |           (inode table, root inode, scope)       |
  +--------------------------------------------------+
        | inode(serial)  child(parent, name)  list(..)
        | stat(serial)   readlink  plan_read  content
        v
  canonical objects of that one root, demanded on use
```

`BaseView::open` acquires the root object once through `FilesystemRead` and
checks its `InodeScope`. It lists nothing, copies nothing and builds no mirror
of the tree. Every later answer is a demand read of the objects that answer
needs ([`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs)).

| Method | Returns | Reads |
| --- | --- | --- |
| `open` | A checked binding | The root object only |
| `inode(serial)` | `Resolved` value, or `PathNotFound` | Inode table pages |
| `child(parent, name)` | The bound child | Directory pages of `parent`, then its inode |
| `list` / `list_after_bytes` | One page of names after a key | Directory pages, bounded by entries and bytes |
| `entries(value)` | Exact entry count of a directory | Its root page only |
| `stat(serial)` | `BaseStat`: value, portable metadata, logical length | Metadata root; file length from the length port |
| `readlink(serial)` | `SymlinkTarget` | The target object |
| `plan_read(serial, offset, len)` | `BaseRead` clamped at EOF | File mapping; `len` at most 128 KiB |
| `content(root)` | `BaseContent` of a committed file by its root | One object, no namespace lookup |
| `with_client(client)` | The same root with an operation's provider | Nothing |
| `at(root)` | This view if `root` is its own, else a rebind | The other root object |

`at` lets a read window that names an older root (an unlinked file, a
captured reader) read that root even after an install moved the Workspace on.
`BaseContent` serves reference pieces: overlay ranges that equal committed
bytes of another file ([`file.rs`](../../../../engine/crates/layerfs-workspace/src/base/file.rs)
classifies a captured file for Commit).

## Cache

```text
  CanonicalCache  (one allowance, shared by every client that holds it)
  +-------------------------------------------------------------+
  | Cache: LRU over three entry kinds, one byte allowance        |
  |   object   id -> canonical bytes      charge = len + 256     |
  |   length   content root -> u64        charge = 8 + 256       |
  |   portable metadata root+kind -> mode, mtime  16 + 256       |
  | DirectoryCounts: content root -> child-dir count (LRU)       |
  | ClientWork: cumulative counters                              |
  | decoded: DecodedPages, own budget <= allowance               |
  +-------------------------------------------------------------+
```

| Rule | Value | Source |
| --- | --- | --- |
| Eviction never changes authoritative state | An evicted entry is read again on demand | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| An object larger than the allowance | Bypasses the cache; the demand still succeeds | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| Objects a Commit built | Staged in `ConstructedObjects` (at most 1/8 of the allowance) and placed with `admit_constructed` only after the root is known published | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| Staged roles | Every role except file payload; a whole file keeps only its length | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| Authority | A cached entry grants none; the caller restricts sharing to one authorization context | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |

## Client

```text
  read_canonical_batch(ids)            (at most 4,096 ids)
     |
     +--> cache hit?  yes --> bytes
     |        | no
     |        v
     |   source.read_canonical_batch(misses)   one upstream demand
     |        |
     |        v
     |   check len <= 16 MiB, ObjectId::for_bytes(bytes) == id
     |        |
     +<-------+ insert into cache, count authenticated bytes
     v
  total returned bytes <= 32 MiB per demand
```

`CanonicalClient` is the authenticated object provider every `BaseView` reads
through ([`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs)).

| Constructor | Upstream | Used for |
| --- | --- | --- |
| `new(source, cache_bytes)` | A Store reader | A client with its own cache |
| `with_cache(source, lengths, cache)` | A Store reader and a length port | One operation's provider over a shared cache |
| `resident(cache)` | None (`NoUpstream`) | Answers from memory only, inside an owner job; objects over 64 KiB are not copied |

| Fact | Rule | Source |
| --- | --- | --- |
| File length | Asked of the owning length port, remembered in the cache; no payload classification fallback | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| `provided_length` | Always asks the port; Commit uses it to check a captured file | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Sibling lengths | A lookup of a file whose length is not remembered (a Mutation's planning or `SourceView::lookup`) also remembers the lengths of up to 128 files listed after it, in one demand | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs) |
| Read-ahead | A base read of chunked content that starts where the client's previous read of that content ended, and passes what was read ahead, reads 2 MiB further in the same demand; the extra bytes are not written, their objects stay in the cache. One stream per client; a cache allowance under 8 MiB and a memory-only client never read ahead | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs), [`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs) |
| Portable metadata | Decoded once per metadata root and kind, then remembered | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Failure | Provider errors stay with the source; none is cached | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |

## Links

The canonical format stores no POSIX link count for a directory: a directory
row is a name and a serial. A directory's link count needs its number of
child directories, so the base derives it
([`links.rs`](../../../../engine/crates/layerfs-workspace/src/base/links.rs)).

```text
  subdirs(directory value)
     |
     +-- entries == 0 ------------------------------> 0, no read
     +-- DirectoryCounts has content root ----------> remembered count
     +-- memory-only client -------------> "not resident": visit undecided
     +-- else list in windows of 256 rows / 64 KiB,
              one batch inode demand per window,
              count kinds == Directory, remember under the root
```

| Constant | Value | Meaning |
| --- | --- | --- |
| `DIRECTORY_COUNT_CAPACITY` | 16,384 | Remembered counts for the whole daemon, shared through the cache owner |
| `REMEMBERED_DIRECTORIES` | 256 | Changed directories one Commit remembers counts for |
| `WINDOW_ENTRIES` / `WINDOW_BYTES` | 256 / 64 KiB | One derivation window |

A count is a pure function of the directory's content root, so it never goes
stale. After a Commit, `remember_constructed_counts` stores the counts of
changed directories under their new roots, so the next change there does not
list the directory again.

## Related

- [State](state.md): the Workspace cell that holds the current `BaseView`.
- [Operations](operations.md): how base answers combine with overlay rows.
- [Mutation](mutation.md): base facts fetched outside the owner job.
- [Core: Filesystem trees](../../core/filesystem-trees/README.md) and [Core: Files](../../core/files/README.md): the canonical objects read here.
- [Store](../store/README.md): the source of authenticated objects.

---

← [Engine: Workspace](README.md) · [Up: Workspace](README.md) · [Workspace: Operations](operations.md) →
