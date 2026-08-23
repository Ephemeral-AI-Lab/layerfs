# H11 retained-control preregistration v5

Status: **FROZEN BEFORE ANY V5 MEASURED ROW**. V4 is preserved as `SCREEN PASS / STATIC PASS / GATE REVISE`; its eight PASS child rows are diagnostic only and are not imported into v5 evidence.

## Exact v4 blocker

V4 executed all eight scheduled children, then the primary analyzer failed before analysis because `HERE.parents[2]` resolved to `implementation-detail/phase-4/experiments` rather than `.../g5-foundation-h11`. V4 raw SHA-256 is `666bd05f04651d06d23703f232449fc7793bf72f251aad1214eb649afba9e6c8`; FAILED SHA-256 is `641af28267e631d0a22c61c1ad296fa84224d0947f8819fb8d6d40db7616e4ba`. Its owner-bound failure release passed and the global lock is absent. No v4 analyzer/agreement/cleanup/manifest/terminal promotion artifact exists.

## V5 method-only repair

- Correct the primary historical analyzer base from `HERE.parents[2]` to `HERE.parents[1]`.
- Treat analyzer exit codes 0 and 1 as normal parseable PASS/REVISE dispositions; run both analyzers and decide from their outputs. Other codes, missing files, or invalid JSON remain execution failures.
- Reuse the exact v4 Rust sources and release executable by hash. No product/H11 source, fixture, expectation, schedule, Q rule, threshold, or semantic mechanism changes.

Before measurement, both analyzers must:

1. import with existing v1/v2 authority paths;
2. return PASS and exact normalized agreement on the preserved v4 diagnostic raw;
3. return parseable REVISE on a synthetic root mismatch, with exact normalized agreement.

## Ladder

```text
focused positive/negative analyzer checks + Python compile
-> zero-row v5 dry-run
-> fresh N=1,000/sample=1 v5 screen <20 s
-> v5 method/static custody closure (retain unchanged v4 Rust workspace/static PASS)
-> one fresh complete eight-row v5 gate <20 s
```

V4 rows are never selected, copied, or promoted. Any v5 gate failure is preserved and repaired in v6.

