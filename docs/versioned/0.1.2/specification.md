# LayerFS 0.1.2 specification

> **Status:** Released normative delta over the
> [0.1.1 specification](../0.1.1/specification.md).

LayerFS 0.1.2 retains the 0.1.1 product model: one local SQLite Store per
Client; immutable Layers and Commits; named Branches; ephemeral Workspaces;
explicit Commit and End; authenticated canonical objects; CDC; structural
copy-on-write; materialized or real-FUSE projections; and bounded queries,
reads, output, and admission.

The release adds one bounded public operation and changes its shared internal
implementation:

- owner-side `WorkspaceFileRangeEdit` replaces a byte range with inline bytes,
  logical zeros, or deletion; a same-file batch is prevalidated and
  failure-atomic;
- ordinary FUSE write, append, sparse growth, and truncate lower into the same
  implicit piece tree and immutable spool-slice representation;
- Commit emits final normalized replacement runs through the existing
  structural splice and performs one inode update; superseded bytes do not
  enter canonical construction; and
- presentation failure after durable publication remains explicit and
  recoverable without publishing the Commit twice.

The five-table Store schema, canonical encodings and identities, CDC profile,
SDK and CLI behavior, daemon/proxy protocol, acknowledgement boundary, and
Workspace lifecycle remain compatibility requirements. Object packs and other
Store-format changes are not part of v0.1.2. The terminal proof is
recorded in the [release verification record](../../../release-notes/0.1.2/verification.md).
