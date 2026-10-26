# Native NFS helper and local Sandbox teardown

Status: implemented on the macOS native adaptation branch. This describes the
NFS route in `layerfs-daemon`, not the upstream FUSE force-drain protocol in
[forced teardown](80-forced-teardown.md). No persistence profile, wire format,
mount option or Store interface changes here.

## Helper ownership and deadlines

The helper budget remains 60 seconds plus five seconds for post-KILL exit
observation. A 15-second cutoff is not qualified: a normal slow writeback may
exceed it, and interrupting a helper cannot prove its kernel effect was undone.
Removing Sandbox ownership from the wait resolves the serialization problem
without trading it for earlier unknown outcomes.

[`control/export_helper.rs`](../../crates/layerfs-daemon/src/control/export_helper.rs)
owns each NFS mount, unmount, force and optional flush helper attempt. The
public `Helper` and `HELPER_WAIT` exports remain available from `control`.
One attempt gets 60 seconds of exit observation. At expiry or an observation
error it sends one KILL and polls `try_wait` for at most five more seconds.
It never calls an unconditional blocking `wait` after KILL: macOS can leave an
unmount helper in kernel wait where a signal does not establish process exit.

An unobserved exit is an error with PID, KILL result and retained-child detail.
The process-wide export serving owner retains the `Child`; a later helper call
can reap an exit already observed with `try_wait`, but never signals or reruns
that attempt. No claim is made that the kernel released a mount or that KILL
rolled back a helper's effects. At daemon exit, process ownership passes to the
operating system as for any remaining child.

Output pipes are nonblocking, drained in bounded chunks during observation and
capped at 2048 retained bytes. An exited helper's descendant can keep a pipe
open, so EOF is not a completion requirement. Output beyond the cap is discarded;
continuous output cannot postpone the exit deadline. A normal nonzero helper
answer containing `busy` keeps the existing reversible Busy disposition. An
interrupted attempt is never classified Busy from partial diagnostic output.

The control layer keeps a timed-out unmount's export and engine mount at
`Retained/Detach`; subsequent requests report the same custody without replay.
A timed-out **mount** may already have reached the kernel, so Attach also retains
the serving export, engine mount and directory rather than revoking or deleting
through an unknown client mount. No Ready acknowledgement is issued for it.

## Discovery ownership wait

The Node binding registers a Workspace-scoped unmount intent under Sandbox
`ownership.lock`, then drops Sandbox ownership before waiting for the daemon.
The existing NAPI worker waits synchronously on one control connection: no
new background helper or client process is introduced. The daemon still owns
its helper, reap deadline and export custody. Another Workspace can use another
pooled control connection; Sandbox discovery reads also proceed during the wait.

`api/binding/src/unmount.rs` owns a private `workspace-<id>.unmount` result
record and an exclusive nonblocking `workspace-<id>.unmount-lock` flock. The
lease spans registration, the lock-free exchange and publication. It is explicitly
unlocked on drop, including when another fork briefly inherited its descriptor.
Lock order is Sandbox guard → nonblocking Workspace lease at admission; on
completion it publishes the known result using only its Workspace lease, then
reacquires the Sandbox guard for shared-record cleanup. A cleanup failure adds
`cleanupError` to a known successful response rather than replacing success with
Busy. Close checks the lease even for a published result, so it cannot delete or
reuse the Sandbox directory while that cleanup is in progress. Observers use
nonblocking shared flock probes, so they neither wait on the Workspace lease nor
mistake another observer for the exclusive operation owner.
The shared discovery readiness deadline is back to 60 seconds; it no longer
needs to cover the 60+5-second helper wait. Other long discovery operations or
control-pool exhaustion can still cause waiting; this is not a global latency
promise for all calls.

State transitions:

- No intent → Unmounting (path and start time saved before sending a request).
- Unmounting → Unmounted after known daemon success (including an explicit
  Missing answer); shared-record cleanup follows. Repeated unmount reads the
  saved result without sending another request.
- Busy/Dirty → no intent, preserving their reversible semantics.
- A control connection failure before the unmount exchange removes the intent;
  no operation was sent. Other failure → Retained with the original refusal
  saved, never replayed.
- Unmounting with no lease holder → Unknown/Retained, including client crash
  before send or after daemon completion. Observation cannot decide whether an
  effect occurred. It neither removes the Workspace nor repeats the request.

The public method signatures and daemon protocol are unchanged. During active
unmount, existing `Busy` refusals carry `detail.state = "Unmounting"` and
`elapsedMs`; owner loss uses existing `Unknown` with `state = "Retained"`.
Workspace token/directory consumers and backend reads check this state before
sending a call; Sandbox close refuses an active intent. A new mount creates a
different Workspace and remains admissible. This detail shape and completed
unmount replay behavior need maintainer alignment before upstream publication.

Terminal result records remain until Sandbox deletion; they are bounded by
that Sandbox's lifetime rather than silently evicted. Known token lookups use a
direct filename, not a scan of completed records; unknown-path recovery and
Sandbox close scan the saved records. An explicit Sandbox close can take over an
orphaned or terminal Retained unmount while the daemon is still healthy, provided
no unrelated live Workspace remains. It performs the existing Stop/Delete,
never a replay of the uncertain Workspace operation. If the daemon dies, the
existing stopped-Sandbox reclaim path owns physical detach and record removal.
Stop still detaches before daemon termination, and Delete retains its checked
detach fence. A failed result-file write is reported as cleanupError on known
success; durable recovery of that lost result is not claimed.

## Stop before daemon termination

[`backend/local/sandbox.rs`](../../crates/layerfs-sandbox/src/backend/local/sandbox.rs)
attempts detach below that Sandbox's own directory before TERM/KILL, while its
NFS server can still answer client writeback. A detach failure is logged and
does not prevent the existing daemon and process-group stop. Stop remains crash
teardown, with no Commit or graceful daemon-wide drain. It does not delete the
Sandbox directory. Delete retains its checked detach before directory removal;
failed detach never authorizes removal through a mount.

Linux continues using `MNT_DETACH`, deepest mount first; macOS uses one
`umount -f` per mount, deepest first. The macOS force helper likewise gets a
60-second observation and a five-second bounded post-KILL reap. Unreaped children
are retained, and failure includes the exact PID and observation rather than
claiming a successful detach. The mount table is checked after ordinary helper
completion. No global NFS operation or cleanup of another Sandbox is performed.

## Evidence and limits

- `layerfs-nfs/tests/helper_deadline.rs` drives the real control Service with
  process helpers: descendant-held pipe, verbose Busy answer, stopped helper
  crossing the real 60-second deadline and repeated Retained requests.
- `layerfs-sandbox/tests/local_stop_mount.rs` is an opt-in macOS test using an
  explicitly supplied disposable Sandbox. It proves Stop removes the mount
  before Delete and leaves the directory. It never selects an existing user
  Sandbox automatically. The normal suite lists this test as ignored.
- `api/binding/tests/unmount-concurrency.mjs` requires explicitly supplied
  disposable paths and binaries. It runs real NFS with a paused helper, checks
  another Workspace and discovery read below one second, checks same-Workspace
  refusals and cross-process progress, reversible Busy/Dirty, saved failures,
  publication while discovery cleanup is blocked, and explicit close takeover
  after client-owner crash or retained failure.
- These tests cannot simulate an unkillable kernel U-state process: userspace
  cannot ignore SIGKILL. Bounded observation of an unreaped U-state helper is
  source-checked; the original long kernel hang has not been reproduced.
- A retained helper is reaped opportunistically on a later helper invocation,
  not by a background reaper. Helper cleanup and kernel resource recovery remain
  distinct facts. Stop may take the sum of deadlines over several mounts;
  there is no promise that a multi-mount stop completes in one helper budget.
