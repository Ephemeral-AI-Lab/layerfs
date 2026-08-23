# H11 retained-control preregistration v7

Status: **FROZEN BEFORE ANY V7 MEASURED ROW**. V6 remains byte-preserved with PASS-labeled artifacts, but the fresh terminal correctness audit supersedes its effective disposition to `H11_REVISE_G5_0_NOT_CLOSED`.

## V6 correctness blocker

Measured historical verification created an uncharged `TransitionOperation::Replace` path with `b"file".to_vec()`. Analyzer agreement and a zero marker can only validate the declared ledger; they cannot prove an omitted source allocation. V6 performance, identity, storage, RSS, cleanup, manifests, and lock custody remain diagnostic, not acceptance evidence.

V6 gate anchors:

- terminal `64ca819133cd6205c3c314057dc2edeedc7ea608c085905548d4a363278513ba`;
- final manifest `92d9d9f1f33c61491cc208d4aab97d3ec90fc38c7d45d4881dbfafa2c2c4a95c`;
- raw `3a378d13635a32792e34130224404ec5f49307361f71577e578286a6e1abac4d`;
- complete wall `8,913,045,709 ns`.

## V7 source repair

1. Construct each historical replace operation with the existing `charged_replace_operation`; keep its capacity guard live until after the operation Vec drops, then require product Q zero.
2. Replace H11’s `prepared_edit_point` call—which internally allocates an unreported CDC-sequence String—with an H11-specific edit-point path. Its two sequential FastCDC scans are covered by one exact 32-KiB product-Q charge, and the result is frozen at `53 references / ordinal 26 / offset 523,926 / replacement length 1`.
3. Add a focused regression test for both preparation Q and historical operation Q.
4. Build a fresh v7 executable with native v7 child schemas. No v4 executable/schema or v6 row is reused.

No canonical bytes, expected roots, fixture, schedule, SQLite schema/profile, LayerFS algorithm, transaction/COMMIT, materializer, or threshold changes.

## Ladder

```text
focused v7 edit-point/historical-Q + whole-harness-Q + Store timer tests
-> zero-row v7 dry-run
-> fresh N=1,000/sample=1 v7 screen <20 s
-> one frozen-source workspace/H11 clippy/tests/fmt/Python/diff closure
-> one fresh balanced eight-row v7 gate <20 s
-> fresh independent source/performance/custody audit
```

Any failure is preserved and repaired in v8. V1–v6 source and results are never edited, deleted, rerun, relabeled, or imported.

