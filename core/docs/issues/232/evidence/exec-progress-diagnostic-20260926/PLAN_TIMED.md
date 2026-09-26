# #232 timed FUSE progress diagnostic, 2026-09-26

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> One new source/image diagnostic after the first attempt showed 320 mounted
> callbacks and a daemon Exec that completed after the caller's five-second
> `Unknown`. The first receipt remains FAIL at its original identity.

Run the same registered `prepend-head-4k-on-10mib-ops-1-exec-v2` command
once at the new clean source. The existing opt-in FUSE callback trace now adds
`at_us`, a monotonic elapsed microsecond counter since that daemon's first
traced callback. It records callback **entry**, not successful reply. The
release driver retains daemon logs with
`LAYERFS_EXEC_PROGRESS_DIAGNOSTIC=1`; the sealed image uses `--fuse-trace`.
Keep the original five-second native progress rule, 30-second Exec request,
one worker, exact shell command, independent byte-copy master, cache policy
and 15-second complete-command budget. No latency or release admission is
claimed. The raw failed or successful attempt is retained, never retried to
select an outcome.

The planned `-02` invocation stopped before the measured child: v2 attempted
to reuse the first diagnostic's append-only case directory. That raw outer
output remains on disk as a non-attempt. The runner now gives v2 the fresh
per-attempt `--out` directory already used by v3/v4. At the new committed
source, freeze the source/tree, product/harness seals, binary and image hashes,
registry/workload identity, validated master bytes, exact command and fresh
output path in a new local `FREEZE.json`. Use
`benchmark-results/fs-bench-pro/issue232-exec-progress-timed-diagnostic-03`
as the output and `--verification skipped`. Compare FUSE callback entry times
with the daemon's Exec interval and caller's five-second failure. If callbacks
continue throughout the interval while the control socket sends no terminal,
the next fix must carry authenticated **observed product progress** without
raising the five-second silence bound. A long shell command with no observed
progress must still expire under that bound.
