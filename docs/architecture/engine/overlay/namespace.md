# Namespace

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Namespace

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Changed inodes and directory entries.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/namespace`](../../../../engine/crates/layerfs-overlay/src/namespace), [`lifetime/generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |

The overlay stores only what a Workspace changed over its immutable
committed base. Every row carries a *generation*. The Workspace's runtime
state (`WorkspaceState`, in memory) says which generations are visible:

```text
generation:  -1        1 .. installed   installed+1 .. captured   active
             orphan    retired: never   sealed by Capture         new rows
             domain    in the view      (read by Commit)          go here
                       |<------------- current view: (installed, active] -->|

lookup of serial s = latest row of s with installed < generation <= active;
no row = the committed base decides.
```

Source: [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs)
(`WorkspaceState`, `Generation`), [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs) (`INODE_LOOKUP`).

## Inodes

| `fs_inode` column / `Inode` field | Meaning |
| --- | --- |
| `inode` (`serial`), `generation` | Primary key; `-1` is the orphan domain ([Orphans](lifetimes-and-reclamation.md#orphans)) |
| `kind` | `File` 1, `Directory` 2, `Symlink` 3 |
| `mode`, `mtime_seconds`, `mtime_nanoseconds`, `nlink`, `size` | Final metadata after Workspace validation |
| `inherited_cutoff` | Bytes below it not written in this generation fall through to the lower view; set by the engine, lowered by every shrink |
| `born` | Generation that created the serial locally; 0 inherits a base inode |
| `entries`, `subdirs` | Exact visible child names, and those that are directories (directories only) |
| `epoch`, `height` | Shrink staircase of this layer ([Payload: Layers](payload.md#layers)) |
| `referenced` | The layer has held a reference piece |
| `sequence` | Content sequence: the revision that last wrote the row |

```text
one compound job (Overlay::apply)
  check_changes        <= 4 inodes, <= 2 names, grammar only, no SQL
  source custody       BaseSource still owned, same root and floor
  rows read once       SourceRows: inode(), name()  (one seek each)
  write active gen     insert if no active row, else update changed cols
  payload (optional)   one Cell or one PayloadWrite (+ WriteClaims)
  settle               revision + 1, dirty counts, one reply ticket
  -> Publication { engine, revision, generation }
```

| Rule | Source |
| --- | --- |
| The overlay checks only representable values (`inode::check`, `check_name`); filesystem semantics belong to Workspace. | [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |
| A `created` serial was reserved by the caller: its row is inserted, and an existing row fails the whole job. | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) `Changes`, [`compound.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/compound.rs) |
| All final values of one operation publish together or not at all, with exactly one ticket. A closed Workspace refuses. | [`compound.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/compound.rs) |
| `keep_live_row` carries an mtime-only change forward into the active generation without counting it dirty. | [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |

## Directory entries

| `fs_entry` column | Meaning |
| --- | --- |
| `parent`, `name`, `generation` | Primary key; `name` is 1–255 binary bytes |
| `inode` | Bound serial, or NULL for a whiteout (name removed) |
| `inherited` | Whether the immediately lower view binds this name |

| Read | Statement | What it returns |
| --- | --- | --- |
| `name(parent, name)` | `NAME_LAYERS` | `NameLayers`: the active row and the latest lower row, in one seek |
| `directory_entry` | `DIRECTORY_ENTRY_LOOKUP` | Final local row; a whiteout differs from "no row" |
| `source_directory_entries` | `SOURCE_NAMES`, `SOURCE_NAMES_KINDS` | `DirectoryEntryWindow`: at most `PAGE_ROWS` (1,024) names of one parent, active and captured, with kinds of their local inodes |

A `Binding` is `Bound { serial, inherited }` or `Removed { inherited }`;
for `Removed`, a lower local row decides before the caller's base fact.
Immutable base names are merged by Workspace outside SQL. Source:
[`directory_entry.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/directory_entry.rs),
[`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs).

## Captured namespace

```text
Capture            active g -> captured g, active g+1; dirty counts 0
                   needs: no capture held, not consolidating, no pending
                   reply tickets; copies no rows
CapturedReader     acquire_captured_reader(capture, request):
                   keeps capture, root and installed floor
Commit reads       reader_inodes / reader_parent_directory_entries /
                   reader_directory_entry / reader_symlink /
                   captured_read / captured_digests / captured_predecessors
known success      install(capture, root): installed = g, base_root = root,
                   enqueue RETIRE g
definite failure   resolve_failed_capture: consolidating = g,
                   enqueue FOLD g (rows move into active over time)
```

| Rule | Source |
| --- | --- |
| Captured pages are keyed by the sealed generation (`fs_inode_capture`, `fs_entry_capture` indexes); later active rows cannot change their membership. | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Reader reads see only the sealed range; nothing falls back to active rows or the base. | [`captured_namespace.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/captured_namespace.rs) |
| A reader stays valid across install and definite failure until its exact release. | [`captured_reader.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs) |
| `install` requires `base_readers == 0`; the owner parks the job and fences new source acquisitions until then. | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |

The `CapturedNamespace` producer that turns these pages into a committed
tree is in Workspace and Commit; see [Commit: Capture](../commit/capture.md).

## Predecessors

```text
rename B over A            fs_predecessor
  A in the base       -->  B -> A            (RECORD)
  A created locally   -->  B -> pred(A) or A (INHERIT), unless that is B
  either way               row of A deleted  (RELEASE)
```

| Rule | Source |
| --- | --- |
| A predecessor is an advisory delta base for the moved file's Commit construction, never a fact about its bytes. | [`predecessor.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/predecessor.rs), [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |
| A chain of replacements keeps one row, ending at the file the base holds. A file is never its own predecessor. | [`predecessor.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/predecessor.rs) |
| Commit reads them with `captured_predecessors`, at most `PAGE_ROWS` serials per call. | [`predecessor.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/predecessor.rs) |

## Related

- [Database](database.md): the tables and the job transaction.
- [Payload](payload.md): bytes attached to inode layers.
- [Lifetimes and reclamation](lifetimes-and-reclamation.md): capture, install, fold and retire.
- [Workspace: Mutation](../workspace/mutation.md) and [Workspace: Base](../workspace/base.md): semantic checks and the immutable base.
- [Commit: Capture](../commit/capture.md).
- [Glossary](../../glossary.md).

---

← [Overlay: Database](database.md) · [Up: Overlay](README.md) · [Overlay: Payload](payload.md) →
