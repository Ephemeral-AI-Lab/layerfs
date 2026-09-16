# Owner acceptance of the v0.1.6 stopping point

> **Status:** LayerFS 0.1.6 release record. Every entry is a disposition of a
> measured result, never a re-measurement.

On 2026-09-16 the owner directed the v0.1.6 closure, the release preparation, the
closure of the v0.1.6 issue set and the `v0.1.6` tag. What that accepts:

1. **The sandbox-local direction, at one construction worker.** The owner accepted
   the measured one-worker cost (B1/B2/B3 in
   [the roadmap record](../../docs/roadmap/0.1/0.1.6/README.md), ledger L20): B1
   with one informational container-CPU line over its sub-gate, B2 with its
   Commit/CPU-sum deltas inside the accepted tolerance and its sandbox-memory line
   recorded as not measurable on this harness, and B3 with both absolute 25k gates
   and both separate verifications passing.
2. **The six single-worker material regressions** (dedup/CDC construction,
   1.50–1.65×) recorded in the [#152 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)
   §4, diagnosed to the removed four-way small-content parallelism and accepted as
   recorded, not repaired.
3. **The waived cold `namespace-100000` Init target** (4.986 s, 1.13× against
   v0.1.5) — an explicit waiver, not a pass.
4. **One declared verification exception**: `v016-branch-mixed-500mb-30000-k100-v1`
   carries a 30 s complete-command ceiling instead of 25 s, with its measured wall,
   keyed by the exact registered ID and asserted against widening by a harness
   test (ledger L12). Every other regular verification keeps the unchanged 25 s
   ceiling and the 15 s family target is still reported separately.
5. **The three declared ≤25 s performance/verification exceptions**
   (`mixed`, `workspace`, `branch` `…500mb-30000-k100-v1`), each reported with its
   measured wall in [verification](verification.md) and in the
   [closeout](benchmark-closeout.md).
6. **The inherited `historical_access` artifact stays `NOT_RUN`**: its sealed v2
   Store (`store_sha256 f323de0e…`) was removed before the #152 campaign and is not
   recoverable from this tree. The six `historical_access` cases that v0.1.6
   actually registers are a separate implementation with their own sealed
   producers and all six are measured and verified.

Acceptance retires optimization scope for this release. It does not relabel any
`FAIL`, `TIMEOUT`, `NOT_RUN` or `N/A` row as a pass, and it does not lower a target
for any other case or for a later release. The limitations in
[the manual](../../docs/versioned/0.1.6/limitations.md) remain in force.
