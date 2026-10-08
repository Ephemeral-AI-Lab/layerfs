# Forced teardown of one Workspace

> **Status:** Implemented product wiring (R6). Functional proofs and their scope
> are in the [R6 checks](../issues/307/checks/r6-concurrency-teardown-20261009/).
> No timing, cold-cache, storage or resident-memory claim.

`ForceUnmount` closes one attached Workspace without waiting for the processes
that use its mount. It aborts that Workspace's own kernel connection, ends the
requests that had not yet attempted anything, drains the daemon's own work,
detaches once and then runs the same revocation and Close as a normal unmount.
It signals no process, waits for no exit or output, and repeats no step.

The daemon half is [`control/force.rs`](../../crates/layerfs-daemon/src/control/force.rs).
The connection half (the abort write, the terminal fence and the forced drain)
belongs to Fuse and is described in
[native request service](75-native-request-service.md#terminal-fence-and-gated-ports).
The normal path is unchanged; see
[native mount session](76-native-mount-session.md#normal-unmount).

## Request and replies

| Record | Meaning |
| --- | --- |
| `Request::ForceUnmount { token, relinquish_unknown }` | One forced terminal unmount of that incarnation. The flag has no default |
| `Reply::ForceUnmounted { token, outcome }` | Completed: the entry left routing |
| `Reply::Retained(TeardownCustody)` with `forced: Some(..)` | Stopped after an effect; the entry keeps the exact owner |
| `Reply::Refused` at `force:admission`, `force:custody` or `force:capability` | Refused before any effect |

`ForcedOutcome` carries `ForcedFacts`, the connection's last `NativeWork`
counters and one cleanup observation. `ForcedFacts` is
`{ abort, detach, commit, fenced }`: the abort write (`Written`, `Short`,
`Failed`), the detach attempt (`NotAttempted`, `Detached`, `Busy`, `Failed`),
the Commit knowledge copied at admission, and the number of admitted requests
that were ended by one terminal reply. The short count and the errnos have no
wire field; they are in the bounded `detail` text of a `Retained` reply, which
is never parsed.

## Admission

One registry section decides, in this order. A refusal changes nothing: no
phase, activity or epoch moves, and no syscall or engine job is made.

| Condition | Answer |
| --- | --- |
| The token names no entry, another namespace, or a binding still in progress | `Missing`, `Invalid` or `Busy` at `admission`, as for every control operation |
| The entry is `Retained` | The stored custody, as `Retained`. No second abort or detach ever follows it |
| Activity is `Committing`, `Attaching` or `Closing` | `Busy` at `force:admission` |
| No native connection (`Unattached`) | `Invalid` at `force:admission` |
| An Attach outcome is unknown (`Attaching`) | `Unknown` at `force:admission` |
| Activity is `Uncertain` and `relinquish_unknown` is false | `Unknown` at `force:custody` |
| The connection has no abort control | `Failed` at `force:capability`; the Workspace stays Ready |
| Otherwise | Admitted: native phase `Stopping`, activity `Closing` |

The same guard runs on every platform. Outside Linux no entry owns a
connection, so an otherwise admissible request is `Invalid` at
`force:admission`.

A running Commit, Attach or unmount is never interrupted: Force is `Busy`
before any effect and that operation ends with its own outcome.

**Commit knowledge** is copied under the same lock and never settled by a later
read:

| Activity at admission | `commit` in the receipt | Flag |
| --- | --- | --- |
| `Idle` | `Absent`: no Commit custody was retained. It is not a not-published verdict | not needed |
| `LocalFailure` | `Published(outcome)`: the registry's original known publication | not needed |
| `Uncertain` | `Unknown` | `relinquish_unknown` required |

`Uncertain` also covers a settled failure whose reader or operation owner was
not released; the registry cannot tell that from an unknown publication, so the
receipt says `Unknown` for both. Relinquishing is not a resolution: the
namespace is closed with the capture, reader and operation owner still in the
engine, and no release of that custody is attempted.

## Sequence

After admission, outside the registry lock, each step is made once:

1. **Abort.** `NativeSession::abort` makes the connection's one one-byte write
   to the abort control that was bound at Attach. `Short` or `Failed` stops
   here: `Retained` at stage `Abort`, nothing further attempted, request
   service not stopped, `fenced` 0. If the session refuses before the write,
   no effect was made: the same connection returns to Ready with the activity
   it had, and the reply is `Failed` at `force:capability`.
2. **Fence, drain and detach.** A written abort stops the mount's request
   service. `NativeSession::force_drain` waits, bounded by `drain_wait`, for
   every loop to be joined and for no received or admitted request to remain,
   and only then makes one plain `umount2(path, 0)`, releases the lane and
   removes the directory. A stop here is `Retained` at `Join`, `Owner`,
   `Requests`, `Detach` or `Lane` with `abort: Written` and the detach
   disposition. `EBUSY` from the detach is `Detach` with `Busy`: aborted and
   still mounted, because a caller holds a descriptor, working directory or
   mapping. It is never the reversible answer of a normal probe.
3. **Revoke.** Native phase `Draining`, then `NativeJob::Revoke`. Its
   completion is read and dropped before the next job.
4. **Close.** The same logical Close as a normal unmount; its completion is the
   success's completion.
5. **Cleanup observation.** One `CleanupState` job: `Held`, `Queued` or `Gone`.
   If it cannot be submitted or answers anything else, the receipt says
   `Unobserved` and the failure is kept in the success's `observation_failure`.
6. **Routing removal and `ForceUnmounted`.** The token is `Missing` afterwards.

No `MNT_FORCE`, lazy detach, second abort, second detach, retry, process
signal or wait on a caller's exit or output exists on this path. Revoke, Close
and the observation are submitted without waiting for admission, as on the
normal path, and at most one Lifecycle completion is held while another job is
submitted.

## Retained custody

A stop after an effect stores the exact owner in the entry (the session, the
undrained connection, or the drained receipt with the refused completion)
together with the forced facts as they stood. Activity stays `Closing` and the
native phase is `Retained`. A later `Unmount`, `ForceUnmount` or `Attach` on
that entry returns the stored custody with the same stage, detail and forced
facts, and makes no syscall and no engine job. `detached` and `work` in a
returned custody are read live from the connection's maintained counters, so
they can differ between two replies while a retained request is still
finishing.

Status shows only the native phase (`Stopping`, `Draining`, then `Retained`)
and activity `Closing`. The forced facts are not part of status; they are
returned only by a later `Unmount`, `ForceUnmount` or `Attach`.

## Limits

- Forced teardown needs the abort control, which is bound at Attach only when
  the control filesystem is mounted where the daemon runs. The Sandbox
  container does not mount it, so in that topology every Force is refused at
  `force:capability`. Mounting it there is an owner decision.
- There is no way out of `Retained`. A request that ended retained keeps the
  drain at `Requests` with no detach; daemon stop is the exit.
- A Workspace that is `Unattached` with unknown Commit custody has no terminal
  exit: Unmount is `Unknown` and Force is `Invalid`.
- A retained Commit failure can keep Lifecycle completions (its local
  resolution, or a refused owner release) for as long as its holder keeps the
  failure. Each holds one of the Workspace's Lifecycle slots, so the forced
  Revoke or Close can be refused `AdmissionFull`. Force then ends `Retained` at
  `Revoke` or `Close` with that original refusal, after the abort and the
  detach; it never waits for the slot.
- With `relinquish_unknown`, and for a known publication whose reader or owner
  was not released, the closed namespace stays `Held` and its rows are not
  reclaimed in that daemon.
- The cleanup value of a clean teardown is not fixed. The revoked mount row
  holds the closed namespace until maintenance retires it, so the one
  observation reads `Held`, `Queued` or `Gone` depending on the maintenance
  turns that ran before it.
- A terminal reply is not completion of a job that was already attempted: such
  a request keeps its original result and its custody until it is disposed,
  and the drain waits for it.
- No daemon-wide graceful drain or stop operation exists, and none is planned.
- Nothing here is a latency, storage or resident-memory measurement.
