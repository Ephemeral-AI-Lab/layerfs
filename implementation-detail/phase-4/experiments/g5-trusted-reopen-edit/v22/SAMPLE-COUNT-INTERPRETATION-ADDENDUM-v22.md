# G5-1 v22 sample-count interpretation addendum

This note is controlling for any abbreviated statement of v22 sample intent.

The handoff requires at least 20 matched observations for the primary class,
five adjacent pairs for each secondary shape, and two distinct comparisons. It
does not unambiguously require the complete minimum to be repeated separately
inside each comparison.

V22 voluntarily preserves the deliberately stricter v1-lineage design:

```text
per comparison: 20 primary pairs + 6 * 5 secondary pairs = 50 pairs = 100 arms
two comparisons: 2 * 100 arms = 200 arm observations
```

This is a prospective method choice, not an exact restatement of the user
minimum. No row, pair, comparison, or shape may be removed after observation.

Primary order is 10 AB/10 BA. Within each comparison, `same-middle`,
`one-byte-middle`, and `plus1-early` are AB-first five-pair blocks;
`one-byte-early`, `one-byte-late`, and `plus1-middle` are BA-first blocks. The
six secondary blocks therefore aggregate to exactly 15 A-first and 15 B-first
positions.

The first and final observation for both roles in all 14 gate sequences are
fixed `CompleteRoundTrip` checkpoints:

```text
14 sequences * 2 roles * 2 positions = 56 checkpoints
```

All intervening observations are `CaptureOnly`; their latency remains in the
same 200-arm distributions and their root, transition, rooted reachable state,
small sidecars, work, transaction, Q, and cleanup gates remain exact.

Before any screen, the durable zero-row dry-run must record the half-slower
hash calibration, exact planned external hash bytes, all fixed time components,
zero-row counters, and a complete-wrapper forecast `<=150 s`. Forecast failure
preserves v22 as `PREMEASUREMENT_REVISE` without row reduction. A narrower
sample interpretation requires a new prospective v22 method.
