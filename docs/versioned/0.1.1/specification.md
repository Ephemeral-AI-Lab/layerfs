# LayerFS 0.1.1 release-candidate specification

> **Status:** Candidate delta over the released
> [0.1.0 specification](../0.1.0/specification.md). Not yet normative.

LayerFS 0.1.1 retains the 0.1.0 product model: one local SQLite Store per
Client; immutable Layers and Commits; named Branches; ephemeral Workspaces;
explicit Commit and End; authenticated canonical objects; CDC; structural
copy-on-write; materialized or real-FUSE projections; and bounded queries,
reads, output, and admission.

The candidate changes implementation rather than public semantics:

- existing-directory initialization constructs final canonical state through
  bounded parallel preparation and visibility-last admission;
- exact operation-local metadata reuse is bounded and never persisted;
- a localized content edit plans only its touched canonical frontier, with the
  safe whole-manifest path retained for topology, type, and link changes;
- Workspace Create loads authenticated bootstrap objects on demand rather than
  scanning all Store objects; and
- proxy reads retain bounded per-node and aggregate read-ahead.

The five-table Store schema, canonical encodings and identities, CDC profile,
SDK and CLI behavior, daemon/proxy protocol, acknowledgement boundary, and
Workspace lifecycle remain compatibility requirements. The exact candidate
must pass the [release verification record](../../../release-notes/0.1.1/verification.md)
before this document can be frozen.
