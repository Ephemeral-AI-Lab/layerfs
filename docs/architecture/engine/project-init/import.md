# Import

[Architecture](../../README.md) › [Engine](../README.md) › [Project Init](README.md) › Import

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reading a native directory: one scan places every entry in the Store's
acquisition backing, in bounded windows, with the evidence later steps check.

| | |
| --- | --- |
| Component | [Project Init](README.md) |
| Source | [`engine/crates/layerfs-project/src/import`](../../../../engine/crates/layerfs-project/src/import) |

## Scan and observe

```text
 position 0 = root          breadth first, children in name byte order
 +-------------------+
 | directories()     |  next directory (keyset stream over the backing)
 +---------+---------+
           v
   read_dir(path) --> observe(child)   symlink_metadata, never follow
           |
           v
   child buffer (<= 512) ------------------+  buffer reached 512?
           |  no                           |  yes: directory is "wide"
           v                               v
   sort by name, assign positions   write children unpositioned,
   next, next+1, ...                then place_wide(): read unplaced
           |                        children in name order, one read
           v                        window and one write window
   Pending write window (<= 4096 rows, 1 MiB) --> put_entries
```

| Step | What happens | Source |
| --- | --- | --- |
| Root | `check_root` refuses a source that is a symlink or not a directory; the root gets position 0 | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Directory order | Directories are read in position order, so a parent's children are contiguous and sorted | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Observe | Name checked as a `PathName`; kind from `symlink_metadata`; symlink target read once | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Narrow directory | Children sorted in memory and given positions | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Wide directory | At 512 children the buffer is written without positions; the backing orders them and `place_wide` positions them window by window | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Flush before read | A new directory window is read only after every placed entry is written | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |

| Kind | Stored mode | Allowed bits | Row carries |
| --- | --- | --- | --- |
| Directory | `mode & 0o7777` | `0o1777` | host path |
| Regular file | `mode & 0o7777` | `0o777` | host path, `HostIdentity` |
| Symlink | `0o777` (fixed by the grammar) | `0o777` | target root object |
| Other (device, FIFO, socket) | — | — | refused `Unsupported` |

Modification-time nanoseconds outside `0..1_000_000_000` are refused
`SourceRefusal::Mtime`; other disallowed bits are `SourceRefusal::Mode`
([`error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs)).
Metadata is stored inline as `PortableMetadata` (mode and mtime). A symlink's
target object is emitted with Content `emit_symlink` into the scan's own
prerequisite Save while the row is built.

## Sources

| Check | Rule | Failure | Source |
| --- | --- | --- | --- |
| Source root | One real directory, not a symlink | `InvalidInput { NotDirectory }` | [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Backing placement | The provider's directory, resolved through links, must not be the source or below it (ancestor device and inode compared) | `BackingInsideSource`, before `begin` | [`source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Symlink target | `read_link`, then the link is stat'ed again; device, inode, length, mode, mtime and ctime must match | `InvalidInput { Changed }` | [`source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Target bytes | Kept as bytes: no normalization or UTF-8 conversion; checked by `SymlinkTarget::new` | `Content(...)` | [`source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |

Dangling, external and cyclic links are stored as they are; nothing follows
them. A changed source is refused without a second read.

## Namespace and files

```text
 HostIdentity (one per regular-file path)
 +--------+--------+------------------- evidence, 44 bytes ---------------+
 | device | inode  | len u64 | mode u32 | mtime | mtime_ns | ctime | ctns |
 +--------+--------+---------+----------+-------+----------+-------+------+
                     0..8      8..12     12..20  20..28     28..36  36..44
```

| Row field (`NewEntry`) | Meaning |
| --- | --- |
| `key` | `EntryKey { parent, name }`; the root has no parent |
| `position` | Acquisition position; `None` until a wide directory is placed |
| `kind`, `metadata` | `InodeKind` and `PortableMetadata` |
| `target_root` | Symlink target object |
| `host_path` | Directories and regular files |
| `host` | Regular files: `HostIdentity` above |

Source: [`scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs),
[`backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs),
[`rows.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/rows.rs).

The backing answers each write with the *canonical position* of every regular
file's identity: the first path with that device and inode. `Bound::count`
counts a path as unique when its canonical position is its own and as an alias
otherwise. A later path whose evidence differs from the first is refused
`Changed`. After files are built, `check_aliases` re-stats every later path and
refuses one that no longer matches
([`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs)).

| Identity | Constructed | Serial | `namespace_ref_count` |
| --- | --- | --- | --- |
| First path of a host file | once, as one job | `root_serial + position` | 1 + aliases |
| Later path (hard link) | never | the first path's serial | — (binds the same serial) |
| Same bytes, different inode | its own job | its own serial | 1 |

Equal bytes do not make an alias; host identity does
([`complete_root.rs`](../../../../engine/crates/layerfs-project/tests/complete_root.rs)).

## Batches and hints

```text
 worker thread                        owner thread (holds the Save)
 construct_stream --> BatchProducer   recv batch --> Save::accept(object)
   accept(object)      pending batch               --> roots window
   done(index,result)  <= 256 KiB,                     --> complete_files
                       <= 512 objects,
                       <= 512 completions
        flush ----- sync_channel, QUEUE_SLOTS = 4 ----->
```

| Rule | Value | Source |
| --- | --- | --- |
| Batch bytes | 256 KiB; a larger object is sent alone | [`batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs) |
| Objects, completions per batch | 512 each | [`batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs) |
| Batches in flight | 4 (`QUEUE_SLOTS`) | [`batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs) |
| One object | above `CANONICAL_LIMIT` refused `ObjectLimitExceeded` | [`batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs) |

A worker flushes its pending batch before it waits for a job, so the owner
always receives the completions that let it refill the job queue.

After the tree is saved, `record_new_bindings` walks every name binding of
the new root (Content `new_bindings`) and records each as a `BindingHint`
(child, parent, name) through `HistoryCatalog::record_bindings`, 4096 rows per
write ([`hints.rs`](../../../../engine/crates/layerfs-project/src/import/hints.rs)).
Init has no earlier tree, so it records every binding.

## Backing

The acquisition backing is the `Acquisition` port
([`contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs)),
served by the Store's acquisition tables. `Backing` holds only the resident
side: one read window per stream, one write window and counters
([`backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs)).

```text
 begin --> Owner { operation, epoch }
   |
   +-- put_entries / unplaced_children / place_children     (scan)
   +-- jobs / complete_files / entries / job / directory_path (files)
   +-- bindings / set_directory_roots / entries / file_roots  (tree)
   |
   +-- success: dispose(4096) ... until 0 rows, then release
   +-- failure: settle()
         unknown outcome --> Uncertain, rows left as they are
         definite        --> advance(Failed), dispose, release
```

| Stream | Ordered by | Used by |
| --- | --- | --- |
| `directories` | position | scan |
| `jobs` | position (first path of each identity) | file construction |
| `entries` | entry key (equals position order) | alias check, inode table |
| `bindings` | entry key | directory construction |
| `file_roots` | position | inode table |

| Counter (`NamespaceWork`) | Meaning |
| --- | --- |
| `entries`, `jobs`, `unique_files`, `regular_aliases` | backed row counts |
| `frontier`, `directory_children`, `wide_directories` | shape of the tree |
| `child_window_rows`, `read_window_rows`, `write_window_rows`, `write_window_bytes` | resident window high-water marks |
| `read_units`, `write_units`, `discard_units` | port calls made |
| `backing_rows`, `backing_bytes` | working rows and charged bytes removed |

Source: [`work.rs`](../../../../engine/crates/layerfs-project/src/import/work.rs).
These counts describe backed rows and windows; they are not a total importer
memory bound. `InitCounters` gives a separate live reading (step, entries,
files, bytes) for progress display
([`observe.rs`](../../../../engine/crates/layerfs-project/src/import/observe.rs)).

## Related

- [Construction](construction.md): what consumes these rows.
- [Seal and install](seal-and-install.md#uncertain-init): retained working rows.
- [Store: SQLite](../store/sqlite.md) and [Store: PostgreSQL](../store/postgresql.md): the backends behind the acquisition tables.
- [Core: Filesystem trees](../../core/filesystem-trees/README.md): attributes, symlinks and hard links.
- [Glossary](../../glossary.md)

---

← [Engine: Project Init](README.md) · [Up: Project Init](README.md) · [Project Init: Construction](construction.md) →
