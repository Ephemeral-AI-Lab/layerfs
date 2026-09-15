# Frozen design: deterministic multiscale history anchors

Frozen before the formal runs on 2026-09-04.

## Question

For a growing immutable LayerFS Commit history, can a small deterministic
sidecar index reduce the number of history nodes inspected by ancestor checks
and Branch-Commit Diff planning, without changing canonical identity, public
semantics, Store bytes, or the draft v0.1.4 benchmark contract?

## Fixture

- Commit depths: 1, 10, 100, and 1000.
- One immutable Branch history per depth.
- Each Commit deterministically replaces one 4 KiB `counter` file. This is the
  fixed small-edit schedule used at every depth.
- The depth-1000 case is an extended diagnostic.

## Operations

- latest Commit lookup;
- early Commit lookup;
- middle Commit lookup;
- early-Commit ancestor check;
- adjacent Branch-Commit Diff;
- distant Branch-Commit Diff;
- complete paginated history traversal;
- index build and reconnect-plus-rebuild.

Wall time is the median of 31 warm-cache samples after five warmups. The
explanatory metric is the exact number of ancestry nodes inspected. Direct
Commit lookups inspect zero parent-history nodes.

## Strategies

1. `baseline`: current public Store operations.
2. `fixed-10`: a derived sidecar stores one 10-Commit jump at eligible
   ordinals; other steps follow the parent.
3. `multiscale`: at ordinal `i`, the sidecar stores one deterministic jump of
   `lowbit(i)` Commits when that distance exceeds one. This creates sparse
   power-of-two scales without an adaptive policy.

Both sidecars are rebuilt solely from public canonical history. They are not
written to the Store, are never authoritative, and can be deleted without
affecting results.

## Metadata accounting

Logical compact bytes, not Rust allocator size or a proposed SQLite encoding:

- 33-byte Commit ID plus 8-byte ordinal per indexed Commit;
- 33-byte target Commit ID plus 8-byte distance per extra anchor.

## Gates

- Baseline ancestry nodes must increase with history depth.
- At depth 100, multiscale anchors must reduce distant Diff planning ancestry
  nodes by at least 75%.
- Candidate membership, Diff entries, and history order must exactly match the
  public baseline.
- At depth 1000, logical metadata must be at most 1% of Store database bytes.
- Store counts, canonical object bytes, database length, database contents,
  Commit IDs, and public behavior must remain unchanged.
- No production code, schema, public API, dependency, or frozen benchmark ID
  may change.

If any gate fails, the verdict is `NO-PR`.
