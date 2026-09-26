# #241 baseline Exec liveness diagnostic v1

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Frozen before the first attempt on 2026-09-25.

This is one diagnostic-only public SDK control call, not a registered
performance sample or a replacement for the retained
`10mib-delete-band13` failure. The command is exactly
`printf baseline > .position-baseline`; there is no range EDIT or Commit.
One fresh Branch and Sandbox use an independent writable byte copy of the
closed, validated 10 MiB v3 master. The copied Store/history and master
receipt hashes, source commit/tree, test executable hash and immutable image
ID must be recorded before execution. The output directory is exclusively
created and its first attempt retained, including failure or partial output.

The test records SDK mount, one Exec result and elapsed context, unmount,
Sandbox deletion and absence. Existing daemon LFT1 telemetry records the
request's dispatch, Exec spawn and output scopes when they complete. On Exec
failure, `docker inspect`, `top` and `logs` are saved before deletion. The
raw test stderr retains telemetry. Frame-level BEGIN/END_INPUT and FUSE
syscall events are **not observed** by this version; a successful call or a
missing telemetry row cannot identify why the historical Exec failed.

The original five-second transport progress limit, 30-second Exec deadline,
one construction worker and cache policy remain unchanged. Cache state is
uncontrolled; this is functional diagnostic evidence with
`admission_eligible=false`. Any elapsed time is context, never a latency
PASS. The 243 PASS / 1 FAIL / 20 NOT_RUN integrated selection remains intact.
If this first attempt is inconclusive, the next diagnostic must add the
missing request-scoped events under a new identity; it may not replay this
one to select a better outcome.
