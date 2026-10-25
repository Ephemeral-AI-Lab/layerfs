# Native NFS helper and local Sandbox teardown

Status: implemented on the macOS native adaptation branch. This describes the
NFS route in `layerfs-daemon`, not the upstream FUSE force-drain protocol in
[forced teardown](80-forced-teardown.md). No persistence profile, wire format,
mount option or Store interface changes here.

## Helper ownership and deadlines

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

The Node binding keeps Sandbox discovery ownership while a control mutation is
in flight. Its existing single deadline covers both the in-process turn and
the cross-process `ownership.lock` acquisition. That wait is 70 seconds: the
60-second helper plus five seconds of post-KILL observation and five seconds
of return/cleanup headroom. Lock scope, ordering, one-attempt semantics and Busy
refusal are unchanged. Releasing ownership early would allow another call to
observe or mutate discovery records before the original result is recorded.

This budget covers one helper, not an arbitrary queue or a Stop over several
mounts; a longer holder can still cause Busy. A successful lock acquisition is
readiness to begin the new operation, never permission to replay a failed one.

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
- These tests cannot simulate an unkillable kernel U-state process: userspace
  cannot ignore SIGKILL. Bounded observation of an unreaped U-state helper is
  source-checked; the original long kernel hang has not been reproduced.
- A retained helper is reaped opportunistically on a later helper invocation,
  not by a background reaper. Helper cleanup and kernel resource recovery remain
  distinct facts. Stop may take the sum of deadlines over several mounts;
  there is no promise that a multi-mount stop completes in one helper budget.
