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
  (`project.init`, `backup.create`, ...) and every chunk of a long command's
  output. A command's output reports through `sandbox/runtime.rs`'s
  `Reporting` sink (kept to the spool, reported at most every 500 ms), so a
  long build is visibly progressing and is not mistaken for a wedged one.
  The `start` and `done` frames are the worker's own bookkeeping: they are
  marked `worker: true`, they bound the wait, and they are not forwarded to
  the caller's event stream, so a worker call sees the same events as a
  local call.

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
progress completes, and a long command keeps its whole duration; only an
operation that shows no progress for the window is bounded. An operation
that is silent for the whole window (a command that writes nothing and
reports no phase) is indistinguishable from a wedged one and is `Unknown`;
it is not killed -- it may still finish, and its effects stay observable.
An operation that can be long and silent reports a phase or output.

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
- `the_worker_claims_and_reports_progress_before_its_answer` -- the real
  worker's frames: one claim, `start` and `done` progress in increasing
  `seq`, then the value.

`sandbox/runtime.rs` also has a unit test that a command's output write
emits the `output` progress value. `tests/worker_liveness.rs` keeps the
frame-level cases (#556's): a worker that dies mid-wait, a trickled frame
and a large answer that keeps arriving. `worker_hello`, `worker_frames`,
`worker_lifecycle`, `worker_trace` and `doctor` stay green.

## Verification

cargo 1.90, on this branch (`CARGO_TARGET_DIR=/tmp/layerfs-target`):

- `cargo test -p layerfs-control --test worker_progress -- --test-threads=1`
  -- 7 passed (64 s; the 60 s case is the long one)
- `cargo test -p layerfs-control --lib sandbox::runtime::tests` -- 1 passed
- `cargo test -p layerfs-control --test worker_liveness --test worker_hello --test worker_frames --test worker_lifecycle --test worker_trace --test doctor`
  -- 3 + 7 + 5 + 1 + 1 + 4 passed
- `cargo check -p layerfs-control --tests` -- exit 0

## B9 checklist

- [x] **a dead worker never leaves a call hanging** -- a worker that stops
  answering its secret is `Unavailable`; one that answers then stops is
  `Unknown` within `CLAIM_WAIT`; a claimed operation that stops progressing
  is `Unknown` within `PROGRESS_WAIT`. Nothing is resent.
- [x] **heartbeat while an operation runs** -- the progress frames are the
  operation's own heartbeat and the caller times them; #556's liveness beat
  stays, and the caller never lets it refresh the operation wait.

## Merge note

This branch keeps #556's `{ claimed: true }` as one frame, extended with
`op`, and #556's beat emitter, whole-frame bound and timeout logging. It
adds `{ progress: ... }`, the caller's `PROGRESS_WAIT` (which ignores a
`beat`), the `Reporting` output sink and `tests/worker_progress.rs`. #556's
`HEARTBEAT_WAIT` (10 s between beats, which a beating worker never lets
expire) is replaced by `PROGRESS_WAIT`: the beat means liveness only now.
