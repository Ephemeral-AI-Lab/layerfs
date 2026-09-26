# #237: prerequisite-to-inode allocation treatment

> **Status: Research; informative and not a product contract.** Frozen
> before product source edit or candidate sampling. This is a distinct
> allocation treatment after the rejected entry-lifetime experiment, not a
> repeat of either arm or a speed admission claim.

## Mechanism and scope

At source `0ab1a6e9b`, `build_namespace` retains a `PreparedEntry` slice and
calls `prerequisites`, which allocates a metadata-root and content-root vector
with one 32-byte `ObjectId` slot per entry. It then allocates `inodes` and
copies those roots into each `InodeUpdate`. The retained 100k phase diagnostic
reserved **3,232,032 B in each root vector**, 6,464,064 B together, while
the 11,534,336-B inode vector was built. Unlike dropping the entry vector
after use, avoiding these root-vector allocations changes the process's
allocation demand during the peak interval.

Make `prerequisites` take the already-built serial slice and produce the
same ordered `Vec<InodeUpdate>` directly as it constructs each metadata and
optional symlink root. Preserve its one-entry metadata memo, file-role
validation, progress/deadline checks, Save finish/abort and all typed inode
fields. Remove only the two intermediate root vectors and the subsequent
copying pass. Keep `PreparedEntry` borrowed; do not combine this treatment
with the rejected early-drop code. Do not alter public APIs, four workers,
C1 validation, C2 policy, C5 allocation, source scans or Save boundaries.

## One-shot decision

The retained matched control is
[`c3-entry-memory-20260924/control`](evidence/c3-entry-memory-20260924/control/receipt.json),
with instrumented product seal
`c2e50eecfed39038a1d6f943acd6741ebe5fa85c57e57846d12af21428c83c3a`
and harness seal
`96e74ab69821c13c33649c5873003151c72d0502a7657980860686047c1614a6`.
Apply the same temporary phase probes and identical archived release driver/
runner under a new candidate product seal. Keep the same seed-1 SHAKE
manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`,
a fresh independent source copy and Store/History, and require zero resident
source payload pages immediately before the one real SDK call. The metadata
cache remains unqualified, so all timing observations are diagnostic only.

**Keep the candidate only if the one-shot whole-call peak RSS falls at least
4 MiB** versus that control, the two root-vector reservations disappear,
the full separate reopened oracle passes all 101,001 paths and 500,000,000
bytes, and no Store ownership/cleanup regression appears. Report scan,
file-loop, prerequisite, tree-input, tree-build and call-end peak/current
RSS, vector capacities, full command wall and Store geometry, including
non-passing lines. A miss is retained and the code reverted; do not repeat
either arm to obtain a better number. The #229 sparse-history gate and
registered #236 debug SDK selection remain separate and unrun here.
