# #550 item 1: operation progress bounds a wedged worker

Status: implemented. This branch is #556's worker liveness plus the
operation-progress frames the issue's item 1 needs, reconciled into one
protocol 3. The worker lease, the health read model and the Sandbox restart
recovery are the other #550 slice and are not claimed here.

## What this branch implements

### One protocol 3, two kinds of frame

`{ claimed: true, op }` is sent exactly once, after the worker read the
request and before its operation runs: the caller stops waiting to be heard
and can tell a worker that never got going from one that is running.

Two frames then report while the operation runs:

- `{ beat: true }` every 2 s is #556's liveness beat: the worker process is
  alive. It is not progress.
- `{ progress: { seq, op, phase, ... } }` is the operation advancing. The
  worker sends a `start` and a `done` phase around every operation, and the
  operation's own path adds its progress: each `Context::progress` value
  (`project.init`, `backup.create`, ...), every chunk of a long command's
  output, and the command's own live child. A command's output reports
  through `sandbox/runtime.rs`'s `Reporting` sink (kept to the spool,
  reported at most every 500 ms), so a long build is visibly progressing and
  is not mistaken for a wedged one. A command that writes nothing still has
  a process: `Sandbox::run_reporting` observes it (a local process's id and
  start time; a Docker Exec's `Running` inspection) and reports
  `{ progress: { seq, op, phase: "running", pid } }` every 5 s while it is
  alive. A zombie is not alive, and neither is a process that has exited
  while a descendant still holds its pipes, so the reports stop with the
  process the command started.
  The `start`, `done` and `running` frames are the worker's own
  bookkeeping: they are marked `worker: true`, they bound the wait, and they
  are not forwarded to the caller's event stream, so a worker call sees the
  same events as a local call.

The two are carried on the one connection, in order, and never duplicate
each other: there is one claim frame, one beat emitter (#556's) and one
progress source (the operation path).

### The caller's wait

`machine/worker/client.rs` waits at most `CLAIM_WAIT` (10 s) for the claim,
then at most `PROGRESS_WAIT` (30 s) for each sign of progress. A `progress`
or `event` frame refreshes that wait; a `beat` does not. When a wait runs
out the call is `Unknown` with a `detail` that records what was observed
(`{ op, claimed, progress, waited_ms, window_ms }`) and a message that names
the next step. The request is never resent: the operation may still run to
its end, and its effects (a Commit, a mount, a written record) stay
observable.

This is not an operation-duration deadline: a 60 s operation that reports
progress completes, and a long command keeps its whole duration. A quiet
`sleep 60` or a build with no output is no longer silent: its live child is
reported every 5 s (`RUNNING`, well inside `PROGRESS_WAIT`), so the caller
keeps waiting and the exec lease it holds is not dropped under a running
command (#566). The bound comes back when the child does not: an operation
that never claims, that claims and never starts a command, or whose command
has exited (a zombie is not alive; a descendant holding the pipes does not
count) sends no more running frames and is `Unknown` within `PROGRESS_WAIT`.
It is not killed -- it may still finish, and its effects stay observable.

Liveness is not progress. A process that is alive but wedged inside cannot
be told apart from a slow one, so it holds the call; only its end (or a
caller's own deadline) bounds it. That is the narrower guarantee this
protocol makes: the worker keeps a live child's lease and never claims to
bound a genuinely long operation. An operation that can be long and silent
but runs no child still reports a phase or output.

The frame-level wait is still #556's: a frame is bounded as a whole (its
length, every read, and its body at 64 KiB/s minimum), and an expiring wait
writes one `layerfs worker: liveness timeout:` line naming the operation,
the worker and the wait (`claim`, `progress` or `answer`).

### Tests

`api/crates/layerfs-control/tests/worker_progress.rs` covers the
operation-progress wait:

- `a_wedged_operation_after_claim_is_unknown_in_bounded_time` -- a claim
  with no progress returns `Unknown` inside the window, with one connection.
- `a_worker_that_never_claims_is_unknown_in_bounded_time` -- the request
  went out but was never claimed; `Unknown` inside `CLAIM_WAIT`.
- `progress_refreshes_the_wait` -- an operation longer than the window
  completes because each progress frame refreshes it.
- `a_liveness_beat_does_not_refresh_the_progress_wait` -- a worker that
  keeps beating while its operation is wedged still returns `Unknown`.
- `a_wedged_operation_is_not_replayed_and_its_result_is_kept` -- exactly one
  request, and the operation still finishes and records its result.
- `a_long_operation_that_reports_progress_completes` -- a 60 s operation
  under the real windows completes: no blanket operation-duration deadline.
- `a_quiet_command_that_keeps_its_process_alive_completes` -- a command
  that writes nothing reports only `phase: "running"` every 100 ms for 1 s;
  it completes under a 250 ms progress window, and those frames are not
  forwarded as the caller's events.
- `a_live_process_that_stops_reporting_is_unknown_in_bounded_time` -- three
  running frames, then nothing: the operation becomes `Unknown` within the
  window even though it once reported.
- `the_worker_claims_and_reports_progress_before_its_answer` -- the real
  worker's frames: one claim, `start` and `done` progress in increasing
  `seq`, then the value.

`connect/crates/layerfs-sandbox/tests/local_liveness.rs` (macOS, a stand-in
native daemon) proves the producer the client test stands in for: a quiet
`/bin/sleep 1` reports its live process at least five times and completes;
`/bin/sh -c '/bin/sleep 2 & exit 0'` sends no report while its shell is
already a zombie and the operation is still blocked on the descendant's
pipes.

The `Reporting` sink has no inline unit test: this repository's
`tools/check_product_boundary.py` forbids test attributes in product source,
so its bytes-arrived reporting is exercised only by a command that produces
output. `tests/worker_liveness.rs` keeps the
frame-level cases (#556's): a worker that dies mid-wait, a trickled frame
and a large answer that keeps arriving. `worker_hello`, `worker_frames`,
`worker_lifecycle`, `worker_trace` and `doctor` stay green.

## Verification

cargo 1.90, on this branch (`CARGO_TARGET_DIR=/tmp/layerfs-target`):

- `cargo test -p layerfs-control --test worker_progress -- --test-threads=1`
  -- 9 passed (65.86 s; the 60 s case is the long one)
- `cargo test -p layerfs-sandbox --test local_liveness -- --test-threads=1`
  -- 2 passed (3.61 s, macOS native)
- `cargo test -p layerfs-control --test worker_liveness --test worker_hello --test worker_frames --test worker_lifecycle --test worker_trace --test doctor`
  -- 3 + 7 + 5 + 1 + 1 + 4 passed
- `cargo check -p layerfs-control --tests` -- exit 0

## B9 checklist

- [x] **a dead worker never leaves a call hanging** -- a worker that stops
  answering its secret is `Unavailable`; one that answers then stops is
  `Unknown` within `CLAIM_WAIT`; a claimed operation that stops progressing
  or whose child is no longer alive is `Unknown` within `PROGRESS_WAIT`.
  Nothing is resent.
- [x] **a live child keeps the operation's lease** -- a quiet command's
  process is progress (`phase: "running"`, every 5 s), so a `sleep 60` or a
  silent build is not cut off and its exec lease is not released; a beat
  still never refreshes the wait, and a live but wedged process is not
  claimed to be bounded.

## Merge note

This branch keeps #556's `{ claimed: true }` as one frame, extended with
`op`, and #556's beat emitter, whole-frame bound and timeout logging. It
adds `{ progress: ... }`, the caller's `PROGRESS_WAIT` (which ignores a
`beat`), the `Reporting` output sink and `tests/worker_progress.rs`. #556's
`HEARTBEAT_WAIT` (10 s between beats, which a beating worker never lets
expire) is replaced by `PROGRESS_WAIT`: the beat means liveness only now.
