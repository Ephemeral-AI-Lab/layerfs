# Interrupted attempt — retained, not a sample

The Q3 A B B A was started at 2026-09-20T21:59Z and stopped by owner direction
("run quick tests, rather than long multi minute tests, and do not use too many
samples") after the child had written a partial Store (27,738,112 bytes of the
51,867,648-byte result) and no `timing.json`. **No receipt exists, no sample was
consumed, and nothing here is quoted as a measurement.**

The directory is retained rather than deleted: a stopped attempt is evidence about
the round's custody, and this lane never removes a run directory.

Its partial `raw/sample.sqlite` (27,738,112 bytes of a truncated file) is **committed
at owner direction on 2026-09-21**, when the tree was asked to be complete. It is
still evidence of nothing — there is no receipt, the child never finished a write,
and nothing may be read from it — and it is committed as an artefact of the attempt
rather than as a measurement. The retained `raw/trace.jsonl` (1.4 KB) is what the
attempt actually produced.

Q3 is instead answered by the mechanism instruments in
[`scratch/step_cost_probe.c`](../../scratch/step_cost_probe.c) — see §4 of the
campaign [README](../../README.md) — and the second A B B A window the handoff asked
for is recorded as `NOT_RUN`.
