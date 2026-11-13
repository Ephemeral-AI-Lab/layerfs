# Core

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** What is a file, a tree, a Commit and a
stored object, independent of any process, database or kernel?

Core holds the algorithms and formats: canonical objects, file and tree
construction, compare and merge, physical encoding into packs, the history
model and timing. Nothing in core opens a database, a socket or a mount; the
engine supplies those through the ports core defines.

## How the topics relate

```text
 +-------------------+     +-------+
 | Compare and merge |---->| Files |
 +-------------------+     +-------+
           |                 ^   |
           v                 |   |
 +-------------------+       |   |
 | Filesystem trees  |-------+   |
 +-------------------+           |
           |                     |
           v                     v
 +-----------------------------------------+
 |                 Objects                 |
 +-----------------------------------------+
           ^                     ^
           |                     |
 +-------------------+   +---------------+
 | Physical storage  |   |    History    |
 +-------------------+   +---------------+
```

Every topic reports timing through [Telemetry](telemetry/README.md); the
diagram leaves those edges out.

## Topics

| Topic | Crate and source | What it covers |
| --- | --- | --- |
| [Objects](objects/README.md) | `layerfs-content` `object/`, `contract/` | Canonical immutable objects, their IDs, roles, admission and the read/write ports. |
| [Files](files/README.md) | `layerfs-content` `file/` | Complete-file construction, content-defined chunking, bounded reads and localized edits. |
| [Filesystem trees](filesystem-trees/README.md) | `layerfs-content` `filesystem/` | Directories and inodes as sorted B+trees: attributes, symlinks, hard links, updates and validation. |
| [Compare and merge](compare-and-merge/README.md) | `layerfs-content` `compare/`, `merge/` | Diffing two trees or files, and three-way merges with conflict markers, `.lww` and merge units. |
| [Physical storage](physical-storage/README.md) | `layerfs-storage` | How objects become records in immutable packs: Saves, reads, delta encoding and the persistence ports. |
| [History](history/README.md) | `layerfs-history` | LayerStacks, Branches, Commits and Layers, the history catalog contract and inode-serial allocation. |
| [Telemetry](telemetry/README.md) | `layerfs-telemetry` | Bounded parent/child timing trees, waits, and optional platform sampling and output. |

## Which root you hold

Every layer above passes `ObjectId`s around. The same 32-byte type names very
different things, so carry each root with its meaning.

```text
 raw file bytes
   |  construct_bytes / construct_stream / construct_runs        (Files)
   v
 FILE ROOT          a WholeFile object, or a FileState over an
   |                extent tree of Chunks
   |  content_root of one inode row                   (Filesystem trees)
   v
 INODE              (scope, serial) -> kind, reference count,
   |                content root, metadata
   |  a row of the inode table; directory trees bind names to serials
   v
 FILESYSTEM ROOT    FilesystemRoot: profile, scope, root serial,
   |                inode-table root
   |  CommitRecord.root / LayerRecord.root                       (History)
   v
 COMMIT             immutable: root, parent, base Layer, second parent
   |  BranchRecord.head_commit; publishing it adds a Layer to the stack
   v
 BRANCH             base Layer + head Commit inside one LayerStack

 Physical storage encodes every object below the Commit line into packs.
 An ObjectId names canonical bytes; a pack id names physical placement.
```

| You hold | It is | It is not | Owner |
| --- | --- | --- | --- |
| File root | The result of constructing one file | A namespace root; it carries no name, mode or mtime | [Files](files/README.md) |
| Inode serial | One inode inside one allocation scope; new serials come from `reserve_inodes`, never a local counter | Comparable across scopes | [Filesystem trees](filesystem-trees/README.md), [History](history/README.md) |
| Filesystem root | The complete namespace: every inode, directory, file root and attribute | History: it records no parent or Branch | [Filesystem trees](filesystem-trees/README.md) |
| Commit | An immutable history record naming a filesystem root and its ancestry | A Layer: Commit publication and Layer publication are separate | [History](history/README.md) |
| Expected head | The Branch head a Commit request was built against: the publication precondition, or with `force` the parent of an overwrite | A lock or token | [History](history/README.md) |

A valid object ID alone does not prove that it represents the file,
filesystem or authority the caller intends.

## Rules for this layer

| Rule | Where |
| --- | --- |
| Binding rules for code in this layer | [core guide](../../../core/AGENTS.md) |
| Repository rules (one attempt, no retry, production LOC, test ceilings) | [repository guide](../../../AGENTS.md) |
| Dependency direction `core < engine < api`, checked by a guard | [`tools/check_product_boundary.py`](../../../tools/check_product_boundary.py) |
| No core crate opens a database, pack file, socket or mount | Each topic's Invariants table |

These pages explain the design; they do not replace those rules.
