# LayerFS Research Index

This directory contains external-architecture research distilled for LayerFS.
Research notes inform future specifications; they do not override accepted
identities, measured milestone evidence, sealed artifacts, or terminal
retain/revise/revert decisions.

## Notes

| Note | Primary value | Best phase | Use now? |
|---|---|---|---|
| [Cloudflare Computer architecture](cloudflare-computer-architecture.md) | Container/FUSE execution lessons, storage-model comparison, platform reality, performance evidence, and an immutable-object remote-client roadmap | After Stage 1.2; Linux/OCI/remote planning | Borrow execution patterns; reject fixed chunks, mutable sync authority, and macFUSE dependency |
| [Cursor — Git at any scale](cursor-git-at-any-scale.md) | Local-storage guardrails, physical locality, atomic publication, and limits on pack/carrier conclusions | Phase 4 and Phase 8 | Use as doctrine and experiment filtering, not as the current F4 plan |
| [Mem9/Drive9 layered filesystem](mem9-drive9-layered-filesystem-distilled.md) | Workspace lifecycle, native staging, root/ref publication, open-handle generations, audit history, and lazy hydration | Phases 5–7, then Phase 8 | Use as a primary input when writing Phase 5/6 specifications |
| [Phase 4 optimization research](phase-4/index.md) | Code/evidence-first directions for canonical identity, CAS+CDC+COW, materialization, compression, and residual SQLite work | Phase 4 and Phase 5 handoff | Use the decision map to assign future specialists; no report is implementation authority |

## Recommended reading order

### During Phase 4

1. Read the accepted [WP4-M progress ledger](../implementation-detail/phase-4/wp4m/progress.md).
2. Read the current [F4 report](../implementation-detail/phase-4/wp4m/f-series/f4/report.md).
3. Use the [Cursor note](cursor-git-at-any-scale.md) to reject unsupported
   storage shortcuts.
4. Do not import workspace, overlay, remote, or distributed features into the
   active storage qualification.

### When preparing Phase 5 and Phase 6

1. Read the repository [implementation plan](../IMPLEMENTATION_PLAN.md).
2. Read the [Mem9/Drive9 note](mem9-drive9-layered-filesystem-distilled.md).
3. Extract only the smallest capability needed by the first real
   materialize/capture caller.
4. Keep immutable LayerFS roots as the sole authoritative filesystem state.

### When preparing Linux, OCI, and remote execution

1. Finish Stage 1.2 and freeze the accepted local Store behavior first.
2. Read the [Cloudflare Computer note](cloudflare-computer-architecture.md).
3. Borrow its container-local FUSE, write-buffer, range-read, RPC, and real
   workspace-testing patterns.
4. Preserve LayerFS immutable roots, expected-head publication, extent
   locality, authentication, and bounded memory.
5. Do not promote fixed chunks, last-write-wins sync, a polling shim,
   OverlayFS copy-up, macFUSE, or a remote pack without separate measured
   authority.

### During Phase 8

Read both notes when evaluating:

- lazy hydration;
- APFS `clonefile` or reflink-style acceleration;
- root-aware caches;
- repeated-edit storage growth;
- SQLite physical profiles;
- optional backend work; or
- retention and compaction driven by measured history growth.

## Reusable synthesis

The two notes agree on a useful separation:

```text
immutable LayerFS root
  = authoritative resolved filesystem state

local workspace / shadow
  = temporary mutable projection

canonical delta
  = semantic transition between roots

named ref + generation
  = optional workspace concurrency control

event stream
  = audit and explanation, not ordinary read state

cache
  = rebuildable acceleration verified by immutable identity
```

## Adopt selectively

High-value post-Phase-4 ideas include:

- explicit workspace lifecycle;
- immutable base root plus one writable stage;
- destination/base provenance;
- metadata-only updates that reuse content identity;
- whiteout and opaque-directory staging semantics;
- subtree rename reuse;
- stale open-handle generation checks after checkpoint;
- atomic object + delta + root + ref publication;
- typed concurrent-ref conflict;
- append-only committed audit events separate from the read model;
- verified lazy hydration keyed by `ObjectId`; and
- bounded reachability/retention once multiple refs or checkpoints exist.

Adoption remains caller-driven. Named refs, fork, travel, audit actors, policy,
search, and remote hydration should wait until a concrete workflow requires
them.

## Reject or defer

Do not infer authorization for:

- a new carrier merely because Git or backup systems use packs;
- SQLite WAL mode from Cursor's logical WAL design;
- remote per-object lookup in the local-first path;
- cross-object delta compression;
- deep overlay chains or log replay for ordinary reads;
- whole-file durable copy-up for bounded edits;
- payload-bearing audit logs;
- live mutable-base fallback after workspace fork;
- best-effort publication into mutable shared state;
- host inode/xattr/file-handle identity in canonical objects;
- source-sized staging or unbounded caches;
- FUSE, branch/merge, policy UI, GC workers, or distributed coordination in
  the first materialize/capture slice; or
- compaction before measured segment/history growth harms reads.

## Roadmap placement

```text
Phase 4
  finish durable storage, profile/backend selection, and causal optimization
  Cursor note is a guardrail

Phase 5
  materialize immutable roots into safe native directories
  use provenance, metadata reuse, bounded hydration, and staging semantics

Phase 6
  capture native edits into a new root and delta
  use workspace lifecycle, generation checks, and atomic ref publication

Phase 7
  expose the minimal open/materialize/capture/discard SDK
  defer broader workspace UX until a caller exists

Phase 8
  optimize the integrated system and qualify optional platform/backend work
  both notes become measurement inputs
```

## Authority and freshness rules

Before turning a research idea into implementation work:

1. identify the controlling accepted LayerFS specification and milestone;
2. verify that the idea preserves canonical bytes, IDs, roots, deltas, exact
   ranges, typed errors, bounded memory, and publication semantics;
3. name the real caller;
4. define one falsifiable correctness/performance gate;
5. refresh external sources when their pinned revisions may be stale; and
6. create a separately authorized implementation specification.

If research conflicts with accepted local evidence, the accepted evidence
controls until a new prospective experiment proves otherwise.
