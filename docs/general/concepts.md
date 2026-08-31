# LayerFS concepts

> **Status:** Current general guide. Use the versioned manual for release-specific details.

LayerFS separates durable content from ephemeral execution. A single local
Store contains every durable filesystem snapshot and all content-addressed
objects. Workspaces expose one Branch snapshot for tools and disappear when
their lifecycle ends.

## Durable model

A `LayerStackStore` is one SQLite file and one canonical-object namespace. It
contains:

- **LayerStacks**, named linear sequences of immutable Layers;
- **Layers**, immutable published snapshots;
- **Branches**, named writable lines based on a Layer;
- **Commits**, immutable Branch snapshots;
- **canonical objects**, deduplicated globally within the Store by `ObjectId`.

A project-facing UI may present a LayerStack as a project. The durable entity
is still the LayerStack.

## Workspace model

A Workspace is an ephemeral writable view of one exact Branch head and base
Layer. It has a typed `WorkspaceId`, a projection, active executions, and a
dirty frontier, but no database of its own.

The lifecycle is explicit:

1. Create pins a Branch snapshot and prepares its projection.
2. Exec or Shell starts a fresh process in that projection.
3. Commit captures the final filesystem state and publishes a Commit with a
   compare-and-swap on the Branch head.
4. End cleans the projection. Clean End refuses uncommitted state; Discard End
   abandons it explicitly.

End never commits implicitly.

## Content model

Files and filesystem trees are represented as immutable canonical objects.
The object ID authenticates the complete canonical encoding. Content-defined
chunking, rope extents, and tree objects let a small file edit reuse unaffected
content and rebuild only the modified frontier.

Canonical objects are append-only:

- reads authenticate bytes against their `ObjectId`;
- inserts deduplicate on `ObjectId`;
- object bytes are never updated in place;
- normal operations do not delete objects.

Forking a Branch and publishing a Layer reuse existing roots and copy no
canonical payload.

## Projections and execution

LayerFS supports two Workspace projections:

- **Materialize** writes the visible snapshot to a host directory.
- **FUSE** serves the snapshot through the LayerFS filesystem protocol.

Both projections use the same Store reader and commit to the same canonical
format. Container execution uses an authenticated control daemon, one fresh
FUSE helper per Workspace, and one fresh process per execution. The daemon
owns no Store and keeps no shell or payload cache warm.

## Observation

One Monitor belongs to each SDK `Client`. Public operations produce typed
receipts with outcomes, timing, and candidate insertion/reuse statistics.
Monitor snapshots use retained receipts; exact deduplication analysis is an
explicit Store traversal.
