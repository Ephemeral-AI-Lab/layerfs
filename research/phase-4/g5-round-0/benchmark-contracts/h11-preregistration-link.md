# H11 preregistration and evidence links

The first measured package is preserved at [`implementation-detail/phase-4/experiments/g5-foundation-h11/v1/`](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v1/). Its result root `target/phase4-g5-foundation-h11-20260822-v1/` is an immutable-by-policy `REVISE` method attempt; it must not be promoted or overwritten.

The v2 contract is [`PREREGISTRATION-v2.md`](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/PREREGISTRATION-v2.md), with method custody in [`METHOD-MANIFEST-v2.tsv`](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/method/METHOD-MANIFEST-v2.tsv). It reuses the hash-identical v1 executable, 1-MiB fixture, 1,001-row oracle, operation log, and eight-row schedule; only the invalid genesis/non-genesis and allocated-block analyzer comparisons were repaired before any v2 row. Final audit later found the independent hard Q blocker, so v2 is not terminal G5-C authority.

Terminal evidence is local at `target/phase4-g5-foundation-h11-20260822-v2/`. The terminal, verification, payload-manifest, and final-hash SHA-256 values are:

```text
36a02f356b506cefc2568ef3ab0324ba24e7d503327086a6eb8d972b4c33f712
d1337e182c7d7ee72b9a9afe38ef080b2d0efd8dbe8a221239df86ccf7602198
f62d3ae939c3e39450efe265fc3ef960ee5d48d9992f191519e142061103b935
c2e8e857eb74ec5d072d5a6b41f63820acde503409573b0d77704c3483f27180
```

The result directories are access-restricted (`0700`) but not claimed to be filesystem-immutable. Integrity is established by the hash inventory and fresh rehashes; preservation is a repository/workflow policy.

Post-terminal hash-bound audits live under [`v2/audits/`](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/audits/). `EMITTED-WORK-AUDIT-v1.json` confirms every non-timing field inside the six nested operation objects; `FINAL-Q-AUDIT-v1.json` sets the authoritative `H11_REVISE_EXACT_BLOCKER`. These files do not rewrite the sealed target result.
