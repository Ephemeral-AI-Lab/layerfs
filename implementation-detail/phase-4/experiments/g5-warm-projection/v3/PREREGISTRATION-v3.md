# G5-2 v3 preregistration

## Bounded preparation contract

All four prepared modes (`self-check`, `screen-count`, `screen`, and `gate`)
use exactly 250,000 payload bytes. Preparation targets at most 20,000,000,000 ns
and covers all four modes under one hard 60,000,000,000 ns complete-wall bound,
before any campaign lock or product
row. Each mode has at most eight files, every file is at most 100,000,000 bytes,
embedded token/edit metadata remains at most 8,388,608 bytes, and the completed
four-mode input root must be at most 10,000,000 apparent and allocated bytes.
Any overrun fails, removes the entire partial root, fsyncs its parent, and emits
no input manifest. The earlier 1/2/4/4 MB schedule is timing NO-GO history and
is not execution authority. Its exact 240,386,892,251 ns failure is preserved
under `attempts/preparation-attempt-2`.

Status: `PREMEASUREMENT_REVISE`.

V3 prospectively supersedes terminal v2. It changes the product evidence schema
and method authority: requested policy populations and executed routes are
orthogonal, and every attempted product process is durably captured before any
evaluation. No v2 product row, screen, gate, or measurement is reused.

The current disposition is `PREMEASUREMENT_REVISE`: preparation, freeze,
screen, and gate remain closed until the final release binding and current-byte
readiness audit pass. Benchmark-private promotion work is a later nonblocking
limitation under the narrowed G5-2 mechanism contract.

This experiment tests the bounded, process-lifetime warm projection service. It
does not reopen G5-1 and does not claim cold I/O, persistent cross-process seed
reuse, physical-byte uniqueness, GC, a general scheduler, or a caller-owned
editable destination.

## Frozen execution

Input preparation is outside every protected campaign. All four modes are
exactly 250,000 bytes. Preparation fsyncs every file and directory, sets files
to `0444` and directories/root to `0555`, then reopens and rehashes the sealed
tree inside the preparation wall. Immediately before every APFS clone, the
runner reopens, rehashes, and mode-checks the named source fixture. The private
clone is permission-patched only after exact clone inventory is recorded. Under
one lock and one complete wall, screen runs `self-check`, `screen-count`, then
`screen`; gate runs the same two sentinels then `gate`:

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

- exactly five product processes and five hard-gated product JSON reports:
  three scheduled performance reports plus `clone-failure` and
  `after-rename-lost-ack` fault reports;
- every normal and fault report is exactly 250,000 bytes, has route class
  `CompositePredeclaredExactCloneSparsePatchAndFullFallback`, and independently
  matches its predeclared population; the phase's `screen` or `gate` report is
  the primary latency population;
- combined peak RSS `<=33,554,432 bytes`;
- one worker, in-flight high-water `<=1`, pending high-water `<=1`;
- `submitted = coalesced + started`;
- `started = published + cancelled + failed + stale`;
- exact and latest populations remain distinct and exact;
- projected root equals last requested root after successful drain;
- projection SQLite writes/transactions/COMMITs are `0/0/0`;
- the separate contention sentinel foreground writer is exactly one
  transaction/one COMMIT and overlaps worker execution;
- reader state is exactly `[autocommit, scope_live]=[1,1]` at the barrier and
  `[1,0]` at foreground COMMIT; overlap is established by the separately
  recorded worker/foreground intervals. The edit `t0`, canonical ACK `t1`,
  enqueue `t2`, worker-start `t3`, and native ACK `t4` ordering does not claim
  that the foreground COMMIT occurs inside the `t3` to `t4` interval;
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
times every scheduled 250,000-byte population, one four-times 400 ms fallback
allowance, and 10 seconds for clone, checkpoint,
analyzers, custody, and cleanup. It is a
prospective feasibility bound, not measured product timing. Actual complete
wall is authoritative.
