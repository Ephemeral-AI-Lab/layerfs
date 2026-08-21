# Phase 4 hypothesis ledger

Status values:

- `RETAINED` — complete prospective LayerFS evidence passed.
- `REFUTED-MECHANISM` — the exact tested implementation missed its gate; the
  broader problem is not declared impossible.
- `OPEN` — plausible but not yet directly bounded or fairly tested.
- `DEFER` — may be useful later, but another experiment must decide first.
- `REJECT` — violates a hard invariant or has no qualifying local use case.

Performance ranges in this ledger are planning priors, not evidence.

| ID | Hypothesis | Local evidence now | Status | Smallest next decision |
|---|---|---|---|---|
| H01 | A transaction-local construction proof can remove the genesis full pre-COMMIT closure replay | F2-v3 cut qualification `387.465 -> 0.051 ms` and durable `916.310 -> 659.593 ms`, with exact semantics and 5/5 | `RETAINED` | Protect it in every later experiment |
| H02 | Fixed 64-row/1-MiB multi-row CAS insertion groups make full create materially faster | F3 reduced statement executions but mapping, durable, RSS, and footprint regressed; terminal revert preserved F2 | `REFUTED-MECHANISM` | Do not revive batching without a new causal mechanism and new authorization |
| H03 | Removing FastCDC's complete scanner-owned chunk materialization exposes >=33 ms | F4-A2 controlling net budget `3.701583 ms`, 0/5 at gate | `REFUTED-MECHANISM` | Retain current scanner; no borrowed-window implementation |
| H04 | Current transient SQLite BLOB binding is a >=5% durable bottleneck | F4-A bind-call upper bound is only `2.745299 ms`; actual internal copy is not isolated | `REFUTED-MECHANISM` for current 5% F-series gate | Revisit only as a tiny cumulative CPU optimization after higher-value work |
| H05 | The private whole-source construction digest is redundant once exact ordered canonical IDs, lengths, transaction-local evidence, root/transition, and fresh verification agree | H05 v7 passed semantic/work gates and won `3/3` pairs with `16.655343%` paired-median durable improvement, but failed its frozen exact allocated-storage equality gate; H05b did not justify an amendment and H05c closed the exact-work A/A question at `6/6` exact pairs | `REFUTED-MECHANISM` | Preserve all rows and retain CP-0009; carry the ordered canonical commitment only as a design/performance prior, never as accepted evidence |
| H05A | A v2 `(length, canonical ObjectId)` reference can replace the separate raw `ChunkId` safely | Canonical ID commits to role/framing/length/raw bytes; the model removes `169,088` mapping bytes and exposes a `95.185147 ms` gross raw-ID lane, while H05 supplies measured ordered-commitment cost evidence but no promotable candidate | `READY-FOR-SHADOW` | Restore the private benchmark source to CP-0009, then build a nonpersistent v1/v2 authority/migration shadow and use short graded exploratory screens before any durable profile decision |
| H05B | The exact accepted FastCDC Gear loop can be materially faster without changing boundaries | CDC-exclusive is `128.723024 ms`; F4-A2 killed only materialization at `3.701583 ms` | `OPEN-SECONDARY` | Boundary-identical hot-loop study after identity authority |
| H06 | A larger SQLite page size reduces overflow/pager event cost for approximately 20-KiB objects | Current 4-KiB database reports 26,676 dirty writes and 6,675 spills; no 8/16-KiB fair local campaign exists | `OPEN` | Fresh-database 4/8/16-KiB physical-profile campaign with exact format/storage/read/edit gates |
| H07 | A self-indexed immutable segment can remove large BLOBs from SQLite's per-object B-tree while SQLite publishes the head | Earlier append-only candidate was noncomparable or slower and had index/reopen amplification; no fair current-workload lower bound exists | `DEFER` | First run an idealized same-work carrier lower-bound; stop if double durability/index cost cannot expose material headroom |
| H08 | A verified operation-local locator receipt can avoid repeated incumbent reads/authentication within one immutable transaction authority | F2 proof shows scoped evidence can be safe; no receipt currently binds exact persisted locator/range for general reuse | `OPEN` | Model adversaries and exact authority fields, then test repeated-content fixture with zero unaided trust |
| H09 | A content-defined/prolly mapping removes fixed-ordinal suffix rewrite for early count-changing edits | CP-0008 proves `O(suffix)` work but measures only `27.141/15.102 ms` 500-MiB early/middle publication, below the current `<50 ms` policy | `DEFER-POLICY` | Establish a real near-constant multi-GiB SLA before a deterministic topology simulator or format code |
| H10 | Directory mutation can avoid cloning/rehashing the full in-memory map by using the durable page/index path directly | Complexity report names the current full-map behavior as unresolved | `OPEN` | Attribute one wide-directory same-size and leading-insert path before choosing structure |
| H11 | Compression or cross-object delta packing improves full-create durable wall | No retained compression-ratio/CPU evidence; it adds representation and compaction cost | `DEFER` | Corpus-only ratio/CPU screen outside the product path; stop unless byte savings dominate CPU and read amplification |
| H11A | Foreground compression or Git delta packing helps the retained fixture | Adaptive zstd-1 saves at most `4.1453%` of final DB bytes with about `147.8 ms` exploratory encode; Git deltas made pack `844` bytes larger | `REFUTED-MECHANISM` | Do not implement; reopen only on a representative corpus with >=20% post-dedup savings |
| H11B | Parent-root-gated delta materialization makes warm updates proportional to changed paths/bytes | Algorithmic contract permits it, but native destination authority is unsolved and no native benchmark exists | `OPEN-LATER-PHASE` | Specify destination custody/invalidation and exact path/mode/publication behavior |
| H11C | Verified native file seeds plus APFS clonefile accelerate repeated materialization to new destinations | APFS supports CoW clones; no LayerFS seed/cache/native wall evidence exists | `OPEN-LATER-PHASE` | Define file-level seed identity/authority and clone/fallback semantics |
| H12 | One file per object improves locality or simplicity | Prior work found per-object filesystem metadata amplification; current SQLite/internal BLOB and carrier evidence do not support it | `REJECT` | None without a qualitatively different measured platform primitive |
| H13 | SQLite WAL or weaker synchronization is the route to the target | Violates the accepted durability profile and Cursor's logical WAL is unrelated to SQLite WAL mode | `REJECT` | None in current Phase 4 contract |
| H14 | Parallel/async hashing or persistence is required | Current contract is synchronous caller-thread; no serial CPU ceiling has yet proven parallelism necessary | `DEFER` | Exhaust same-thread pass/layout work, then request a separate concurrency contract only with a measured ceiling |
| H15 | More statement-count reduction alone predicts speed | F3 and reconstruction evidence show large crossing reductions with weak or negative wall movement | `REJECT` as a prediction rule | Require native prepare, VDBE/pager, binding, and complete-wall causality |
| H16 | A durable cross-reopen authority can avoid the complete first-edit scrub | CP-0009 measures about `245 ms` at 100 MiB and CP-0008 measures `1.21-1.24 s` at 500 MiB; a persisted receipt alone is explicitly insufficient authority | `AUTHORITY-BLOCKED` | Produce a trusted-store/epoch/invalidation adversary proof before any benchmark candidate; do not time an unauthenticated bypass |

## Portfolio rule

Advance the candidate with the largest directly measurable removable budget
per unit of semantic/format risk. Do not stack open hypotheses. One failed
implementation refutes only its mechanism and caps, while one passed
microbenchmark still does not authorize a product change.
