# H11 retained-control preregistration v6

Status: **FROZEN BEFORE ANY V6 MEASURED ROW**. V5 is preserved as a screen `REVISE`; no v5 gate exists.

## V5 blocker and custody

V5 reused the exact v4 Rust executable but mechanically expected a v5 Q-marker schema. The successful child emitted its frozen v4 sample and Q-terminal schemas, so the runner rejected it before raw promotion. The child completed in 3.83 seconds with empty stderr, RSS 13,975,552 bytes, exact identity/storage, Q 705,901 then terminal zero, and no work-root residue. V5 FAILED, failure lock attestation, and lock release are preserved; the global lock is absent.

## V6 one-variable method repair

V6 continues to use the exact v4 executable and binds its payload schemas explicitly:

```text
phase4-g5-h11-sample-v4
phase4-g5-h11-q-terminal-v4
```

Those binary payloads are not renamed or relabeled. Every Python-owned artifact—result roots, raw filename, analyzers, agreement, lock/failure/release, cleanup, manifests, terminal and verification—uses v6.

The v5 analyzer-path and parseable-REVISE fixes are retained unchanged.

## Focused preconditions and ladder

Before measurement:

- compile/import all v6 Python;
- assert the frozen v4 executable’s two payload schemas from the preserved v5 diagnostic stdout;
- rerun positive and synthetic-negative analyzer agreement checks without importing their rows into v6 evidence;
- zero-row v6 dry-run verifies all source/executable/method hashes.

Then run one fresh v6 N=1,000 screen, v6 Python/method/diff custody closure while retaining unchanged v4 Rust static PASS, and one fresh balanced eight-row gate. Failure is preserved in v7; no v4/v5 row is reused.

