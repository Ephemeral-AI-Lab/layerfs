# Core

[Architecture](../README.md) › Core

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** What is a file, a tree, a Commit and a stored object, independent of any process, database or kernel?

Core holds the algorithms and formats: canonical objects, file and tree construction, compare and merge, physical encoding into packs, the history model and timing. Nothing in core opens a database, a socket or a mount.

## How the components relate

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
 | Physical storage  |   | History model |
 +-------------------+   +---------------+
```

Every core component reports timing through [Telemetry](telemetry/README.md); the diagram leaves those edges out.

## Components

| Component | What it covers | Topics |
| --- | --- | --- |
| [Objects](objects/README.md) | Canonical immutable objects and their IDs: the units every other component names, stores and compares. | [Identity](objects/identity.md), [Kinds](objects/kinds.md), [Access](objects/access.md) |
| [Files](files/README.md) | Complete-file construction, content-defined chunking, localized edits and bounded logical reads. | [Chunking](files/chunking.md), [Construction](files/construction.md), [Edits](files/edits.md), [Reads and views](files/reads-and-views.md) |
| [Filesystem trees](filesystem-trees/README.md) | Directories as canonical objects: entries, attributes, symlinks, hard links, ordering and validation. | [Representation](filesystem-trees/representation.md), [Attributes and links](filesystem-trees/attributes-and-links.md), [Updates](filesystem-trees/updates.md), [Validation](filesystem-trees/validation.md) |
| [Compare and merge](compare-and-merge/README.md) | Diffing two trees or files, and merging Branches: decisions, conflict markers and revert. | [Compare](compare-and-merge/compare.md), [Merge](compare-and-merge/merge.md) |
| [Physical storage](physical-storage/README.md) | How objects become bytes on disk: encoding, delta bases, packs, saves, reads and the persistence ports. | [Encoding](physical-storage/encoding.md), [Packs](physical-storage/packs.md), [Saves](physical-storage/saves.md), [Reads](physical-storage/reads.md), [Ports](physical-storage/ports.md) |
| [History model](history-model/README.md) | LayerStacks, Branches, Commits, Layers and stages: the immutable history metadata. | [Entities](history-model/entities.md), [Catalog and queries](history-model/catalog-and-queries.md), [Allocation](history-model/allocation.md) |
| [Telemetry](telemetry/README.md) | Environment-independent parent/child timing trees, and how they are printed or written as JSON. | [Timers](telemetry/timers.md), [Runtime](telemetry/runtime.md), [Output](telemetry/output.md) |

## Rules for this layer

The [core guide](../../../core/AGENTS.md) holds the binding
rules for code in this layer. These pages explain the design; they do not
replace those rules.

---

← [Glossary](../glossary.md) · [Up: Architecture](../README.md) · [Core: Objects](objects/README.md) →
