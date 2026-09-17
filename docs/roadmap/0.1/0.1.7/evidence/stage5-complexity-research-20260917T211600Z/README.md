# Stage 5 complexity and round-trip research (2026-09-17)

> **Status:** research evidence for
> [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176) (sub-issue of
> #170, feeding #171). Static source reading only: no builds, no tests, no
> measurements, no product changes. Append-only; nothing here was edited after
> it was written.

| | |
| --- | --- |
| Deliverable | [`complexity-and-roundtrip-research-20260917.md`](../../component-decoupling/complexity-and-roundtrip-research-20260917.md) — the comparison, the tiered opportunity register, the round-trip summary and the corrections to closed records |
| Companion study | [`parallelism-and-batching-study-20260918.md`](../../component-decoupling/parallelism-and-batching-study-20260918.md) (the owner's study: SQLite configuration and parallelism; complementary, not duplicated) |
| Trees read | core `5e45897dd` (product crates byte-identical through `2d93edf0e`); reference `crates/layerfs-layerstack-store` byte-identical to the v0.1.6 tag |

## Reports (one per research agent, each the agent's only write)

| Report | Scope | Most significant finding |
| --- | --- | --- |
| `report-A-c1-file-content.md` | C1 file pipeline: CDC, mapping build/read, construction, edit routes, canonical object layer | the chunked edit's discarded sibling load (`tree.rs:711`), the compare-then-construct double traversal, the ~3n whole-file edit peak, per-segment re-traversals, the ungated hint walk |
| `report-B-c1-filesystem-engine.md` | sorted engine, directories, inodes, validation, operation driver, reads, telemetry | branch rows carry no child summaries (per-level reads format-irreducible) but waves are capped at 32 against a 4,096 ceiling; `page_from_wire` unbatched; validate's per-binding lookups; the `pages_read` counter undercount |
| `report-C-attributes-ordering.md` | attributes engine; ordering (pending, runs, merges, reducer, release) | **the n^1.58 ordering amplification is mostly the lookup path (read residual ≈ n^1.9), rooted in `spill()` resetting every tier's scan**; lever math L1-L7; parity safety proven by the threshold test |
| `report-D-c2-storage.md` | C2: admission, save, transactions, packs, delta, pooling, reads, SQLite | whole-BLOB pack append rewrite (confirmed); per-record group decompression on the ordinary path; connection + pragmas per read wave; per-object dependency SQL; bounded quadratics |
| `report-E-reference-c2.md` | reference C2 (`crates/layerfs-layerstack-store`, v0.1.6-identical) | producer pool vs core's single owner; bulk multi-row INSERTs; the SQLite scratch-DB value index; wave/budget constants; cleanup and connection differences |
| `report-F-reference-c1-workspace.md` | reference C1 + the workspace update pipeline | frontier spill triple I/O; uncached namespace-root decodes; the O(total-namespace) reconcile walk; whole-file digests on both sides; spool write+read-back |
| `report-G-roundtrips.md` | cross-tree round trips (16-row register) | per-page point reads across the C1←provider seam compounded by connection-per-wave; the seam-map over-breadth ("no per-node RPC" holds only for mapping navigation); avoidable `copy_run`; double hashing |

## Adjudication

The main agent re-read the six highest-impact findings at their `path:line`
before entering them in the deliverable, and confirmed each in source:
`runs.rs:231/299` (reset-all-scans vs spill touching only tiers ≤ level),
`sorted/page.rs:28` (`BATCH_CHILDREN = 32`), `edit/tree.rs:711` (the discarded
`load_node`), `directory/read.rs:208` + `validate.rs:175` (per-page/per-binding
point reads), `sqlite/write.rs:88-97` (whole-BLOB `UPDATE`). The ordering
decomposition was re-derived from the existing grid receipt's numbers.
