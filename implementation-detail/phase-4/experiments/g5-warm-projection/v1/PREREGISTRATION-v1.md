# G5-2 v1 preregistration

Status: `PREMEASUREMENT`.

This experiment tests the bounded, process-lifetime warm projection service. It
does not reopen G5-1 and does not claim cold I/O, persistent cross-process seed
reuse, physical-byte uniqueness, GC, a general scheduler, or a caller-owned
editable destination.

## Frozen execution

Input preparation is outside every protected campaign. Each attempt APFS-clones
each sealed prepared fixture immediately before use. Under one lock and one
complete wall, screen runs `self-check` (1 MiB), `screen-count` (10 MiB), then
`screen` (100 MiB); gate runs the same two sentinels then `gate` (100 MiB):

```text
layerfs-g5-projection-child --g5-projection-run ATTEMPT_ROOT screen|gate
```

`/usr/bin/time -l` measures the complete foreground-plus-worker process. The
screen complete wall is `<20,000,000,000 ns`; the gate complete wall is
`<=150,000,000,000 ns`. A zero-product-process forecast must pass before either.
The global benchmark lock is exclusive, intent-bound, fsynced, and released on
every exit. Result roots are one-shot and never overwritten.

Screen population is exactly two exact-root requests and two latest-following
requests. Gate population is exactly 64 exact-root and 100 latest-following
requests. Preparation, populations, thresholds, and equations may not change
after a product row exists.

## Hard acceptance

- exactly three product processes and three hard-gated product JSON reports;
- the final 100-MiB report is the sole primary decision population;
- combined peak RSS `<=33,554,432 bytes`;
- one worker, in-flight high-water `<=1`, pending high-water `<=1`;
- `submitted = coalesced + started`;
- `started = published + cancelled + failed + stale`;
- exact and latest populations remain distinct and exact;
- projected root equals last requested root after successful drain;
- projection SQLite writes/transactions/COMMITs are `0/0/0`;
- the separate contention sentinel foreground writer is exactly one
  transaction/one COMMIT and overlaps worker execution;
- SQLite Busy/Locked events are `0/0`;
- individual owned buffer `<=1,048,576 bytes`;
- terminal in-flight, pending, workers, active/successor descriptors, temporary
  residue, and Q are all zero;
- exact-root service p50/p95 `<=5/8 ms`;
- sparse service p50/p95 `<=6/10 ms`;
- primary and independently written analyzer normalized decisions agree exactly.

The output is explicitly `WarmUnknownPreparedFixtureAPFSClone`; it is not a
cold-reopen or controlled-device-I/O claim. Exact requests are never replaced
by latest. Latest requests may replace only the single pending latest target;
the in-flight request remains valid. A seed rotates only after successful
publication and root/descriptor verification.

## Forecast

The zero-row feasibility equation uses the frozen service ceilings at four
times their scheduled populations including the 1/10-MiB sentinels, one
four-times 400 ms fallback allowance, and 10 seconds for clone, checkpoint,
analyzers, custody, and cleanup. It is a
prospective feasibility bound, not measured product timing. Actual complete
wall is authoritative.
