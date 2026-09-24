# #241 baseline Exec liveness diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Frozen after the integrated functional campaign failed on
> 2026-09-24; the first diagnostic attempt is linked below.

The first prospective [baseline-only attempt](evidence/exec-liveness-diagnostic-v1/REPORT.md)
ran on 2026-09-25 and passed in 23 ms of SDK Exec context wall from a fresh
qualified 10 MiB copy. Its limited telemetry did not capture daemon or FUSE
events on the successful path. It is inconclusive about the retained failure;
the request-scoped event vector below remains the next diagnostic requirement.

The [integrated position receipt](evidence/position-sweep-integrated/REPORT.md)
has one Unknown baseline SDK Exec after 45 PASS cases on 10 MiB. Its command
was a small shell write to .position-baseline. It did not reach range EDIT or
either Commit. Unmount returned Io; SDK Sandbox delete and absence checks
passed. The retained history has no head Commit for this case. The current
evidence does not say whether the shell write reached the discarded Workspace
or where the control response was lost.

Source tracing identifies two distinct clocks: WorkspaceApi::exec requests a
30-second control operation, while the native bridge's bidirectional socket
read expires after five seconds without send or receive progress. After
BEGIN, ambiguous delivery for WorkspaceExec becomes Unknown. The daemon may
still be running the child command when the host discards that session; a
fresh unmount session can then fail before reaching daemon dispatch. Those
are possible mechanisms, not measured explanations of this one failure.

## One prospective count-driven diagnostic

Use a new, labelled functional diagnostic identity, not a replay or
replacement of 10mib-delete-band13 and not a registered performance row.
Freeze its source, image, command, private qualified-master copy and output
path before execution. Retain its first attempt whether it succeeds or
fails. Keep the existing five-second progress policy, overall deadline,
single construction worker and cache behavior.

For the single baseline Exec control request, capture a compact event vector
with operation ID and monotonic timestamps:

1. Host native client: checked session reuse or fresh Hello; BEGIN and
   END_INPUT write results; terminal frame count/type; final read errno or
   progress/absolute deadline; session discard count.
2. Daemon control: accepted/refused connection count; accepted request ID;
   shell child spawn PID; child exit and output-pipe drain counts; terminal
   frame send attempt/result.
3. Child/FUSE: baseline file open/create, write syscall entry and return,
   and request-scoped FUSE WRITE entry and reply. The existing aggregate
   write projection counter increments at callback entry and covers other
   mutations, so it cannot by itself certify this write's completion.
4. Before SDK Sandbox deletion on a failure: Docker die/kill/OOM events and
   inspect exit/OOM status, daemon RSS/FD/thread counts and cgroup memory
   breakdown. A cgroup lifetime peak must be labelled lifetime, never
   phase-local.

An external bounded Linux syscall trace can observe the daemon and its child
without changing product behavior if the diagnostic image provides one.
Existing LFT1 Exec scopes are useful when enabled, but the functional
campaign disabled them and the daemon publishes those scopes only after
dispatch returns. Neither substitute for the request-ID frame/event vector.

| Event pattern | Narrow interpretation |
| --- | --- |
| No child spawn after accepted request | Control dispatch or admission path |
| Spawned child with baseline write entered but no return | FUSE/backing path |
| Child exited and pipes drained, no terminal attempted | Daemon response path |
| Terminal attempted/sent, no client terminal received | Native transport/delivery path |
| No terminal while both directions silent for five seconds | Progress-timeout boundary; underlying stalled stage still needs the child/FUSE events |

This diagnostic can localize a future occurrence. A successful diagnostic
does not retroactively turn the failed 10 MiB case into PASS. If the cause
supports a product fix, freeze a new source identity and repeat the entire
functional selection under that identity, retaining the original FAIL and
NOT_RUN rows. Do not change the timeout, worker count or cache policy to
make a miss pass.
