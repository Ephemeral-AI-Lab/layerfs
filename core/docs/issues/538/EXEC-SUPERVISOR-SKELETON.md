# Exec session supervisor: a modular architecture

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract.
>
> **Scope:** the `#538` skeleton stacked on the unmerged `#499`
> (`feat/ns-exec-helper`, `386f1b22`). This note describes the modules, the
> interfaces between them and the sequences that connect them. It does not
> change behaviour and it does not promise that the deferred modules exist.

The note is written in layers: the module map first, then the interfaces, then
the sequences, then the failure model and the implementation/test status. A
reviewer can stop after §1 for the shape, after §2 for the contracts, or read
all six sections for the full design. Terms such as *supervisor*, *child*,
*session* and *caller* are defined in §1 and used with the same meaning
throughout.

## 1. Module map

The design has four modules that exist in this skeleton and two future seams.
The seams are named here so the skeleton's boundaries are visible; neither has
code in this PR.

| Module | State | Source |
| --- | --- | --- |
| Supervisor process | implemented | [`ns_exec_supervisor.rs`](../../../../engine/crates/layerfs-daemon/src/application/ns_exec_supervisor.rs) |
| Child launcher | implemented | same file, `run_child` |
| Control link and protocol | implemented | [`ns_exec_control.rs`](../../../../engine/crates/layerfs-daemon/src/application/ns_exec_control.rs) |
| Process table and session state | implemented | `Session` in `ns_exec_supervisor.rs` |
| Launcher / orchestrator (daemon side) | future | — |
| Session registry and presence | future | — |

### 1.1 Supervisor process

- **Responsibility:** own exactly one command session for its whole life.
  Receive the daemon's mount-namespace descriptor, bind the control socket,
  spawn the child, observe the child's session, serve callers, reap the child
  once, report the exit and exit with the child's status. It forwards signals
  to the child's process group and never runs any part of the command itself.
- **Input:** `--socket <path>` (the daemon namespace-descriptor socket),
  `--control <path>`, optional `--directory <path>`, optional
  `--uid <uid> --gid <gid>`, and `-- <command> [args...]` after `--`. The
  mount-namespace descriptor arrives over `--socket`, not over argv.
- **Output:** at most one `Started` and at most one `Exited` control frame to
  each connected caller, `StatusReply` answers, the child's exit status as its
  own exit code, and the child's stdout/stderr (inherited, never proxied).
- **Owned resources:** the accepted control listener, the control socket path
  (created `0600`, removed on exit), the one-slot `Session` record, one receiver
  thread per connected caller, the grace-timer thread for `Stop`, and the
  child pid record. It owns the namespace descriptor only between receipt and
  spawn.
- **Not owned:** membership in the Workspace mount namespace, the Workspace
  current directory, the command process image after `exec`, command output
  framing, exit-policy, client presence, the daemon session registry, or any
  other session's child. It is expressly not a mount holder (§4, I1).

### 1.2 Child launcher

- **Responsibility:** perform the only privileged move in the design. Reopen
  the inherited descriptor, validate it as a mount namespace, `setns` into it,
  enter the Workspace directory when asked, drop to the command identity, take
  a new session, arm `PR_SET_PDEATHSIG`, verify the parent, and `exec` the
  command. After `exec` the child *is* the command and the supervisor keeps no
  code in that process.
- **Input:** `--namespace-fd <fd>` (an inherited raw descriptor),
  `--parent <pid>` (the supervisor's pid), optional `--directory`, optional
  `--uid/--gid`, and the command argv. Everything is passed once; the child
  re-reads nothing from the daemon.
- **Output:** the command process and its normal stdout/stderr/stdin; an exit
  code of `125` when the child refuses before `exec`; `128 + signal` is not
  produced here because the child is replaced by the command.
- **Owned resources:** its mount-namespace membership, its current directory,
  its uid/gid and supplementary groups, its session and process group, and the
  `PDEATHSIG` relationship to the supervisor.
- **Not owned:** the control socket, the process table, reaping, exit
  reporting, signal delivery, the namespace descriptor once reopened, or its
  own lifetime after the supervisor disappears (that is the PDEATHSIG
  contract, not an ownership claim).

### 1.3 Control link and protocol

- **Responsibility:** frame, bound-check and write the control messages; bind
  the `0600` Unix socket; admit only a peer with the supervisor's effective
  uid; expose one blocking connection per caller.
- **Input:** encoded caller frames (`Signal`, `Status`, `Stop`); connection
  attempts from callers.
- **Output:** `Started`/`Exited` broadcasts, `StatusReply`, and a per-client
  stream that carries them in frame order.
- **Owned resources:** the frame format, the listener, the peer-credential
  check and the per-client write lock. It owns no process state.
- **Not owned:** process identity, lifecycle, signal policy, retries,
  reconnect, presence detection, or authorization beyond the socket mode and
  the peer uid (§4, I6).

### 1.4 Process table and session state

- **Responsibility:** hold the only authoritative in-memory record for one
  supervisor's child, and serialize event delivery with caller registration so
  a caller cannot miss or duplicate `Started`/`Exited`.
- **Input:** the spawned pid, the observed pgid, the `wait4` outcome and the
  `group_alive` probe; caller registrations.
- **Output:** `snapshot()` (`Started`, then `Exited` when known), the `Status`
  answer, and the broadcast list used by `announce_started`/`finish`.
- **Owned resources:** `pid`, `pgid`, `started`, `exit`, and the connected
  client list, all behind one mutex.
- **Not owned:** pids of other sessions, durable state, session ids, presence,
  client liveness, or state after the supervisor exits. A second client is a
  second consumer of the same record, not a second record.

### 1.5 Launcher / orchestrator (future, ownership open)

- **Responsibility:** start one supervisor per requested command, hand it the
  mount-namespace descriptor obtained from the daemon, connect the control
  socket, relay `Started`/`Exited`/output to the Engine or SDK, and decide when
  a session may be abandoned. In this skeleton that role is played by the test
  harness; production code does not start a supervisor.
- **Input:** a command request (argv, directory, identity), the daemon's
  namespace endpoint, and a control-path allocation.
- **Output:** supervisor start/stop, control connections, session ids (when
  they exist), and the command outcome.
- **Owned resources (proposed):** the supervisor lifetime and its control path,
  the association between a request and one supervisor, and the policy for
  retries and cancellation at the request boundary.
- **Not owned (proposed):** the child process, the mount namespace itself, the
  supervisor's wait/reap duty, command output bytes, or Store/Workspace data.
- **Open question:** the current daemon is documented as having "no command
  supervisor" (`application/mod.rs`) and the declined launcher proposal kept
  exec ownership in the Sandbox runtime. Placing this module in the daemon
  would change that ownership rule and needs an explicit decision before any
  code is written.

### 1.6 Session registry and presence (future)

- **Responsibility:** keep a durable-enough index of live sessions (session id
  → control socket and identity), detect that a caller has gone away, allow a
  later caller to reconnect to an existing session, and garbage-collect records
  when the supervisor exits.
- **Input:** supervisor announcements, caller connect/disconnect, `Exited`, and
  supervisor death.
- **Output:** session lookup by id, reconnect, presence verdicts, and cleanup.
- **Owned resources (proposed):** registry entries and their lifetime, session
  ids, and presence state.
- **Not owned (proposed):** process lifecycle (`wait4` stays with the
  supervisor), signal policy, control framing, or the command itself.
- **Why the skeleton cannot do it:** the control socket lives only as long as
  the supervisor. Once the supervisor exits, the path is removed and the
  session cannot be looked up; a caller that never connected before a short
  command exits cannot observe that session at all (§4, F4).

## 2. Interfaces

### 2.1 Mount-namespace descriptor handoff (`SCM_RIGHTS`)

The daemon is not dumpable and a Sandbox has `CAP_SYS_ADMIN` but not
`CAP_SYS_PTRACE`, so root inside the container cannot open
`/proc/<daemon>/ns/mnt`. The daemon opens its own namespace and serves the
descriptor on a private `0600` Unix socket. The implementation is
[`ns_fd.rs`](../../../../engine/crates/layerfs-daemon/src/application/ns_fd.rs).

**Server side (`NsFd`):**

1. Bind the socket once, after the private mount namespace exists, before the
   daemon serves normal control traffic. Remove a stale path first and set mode
   `0600`.
2. Accept one connection, read `SO_PEERCRED`, and admit only uid `0` **and** a
   peer whose `/proc/<pid>/ns/pid` link equals the daemon's own. A refusal
   closes without sending.
3. Open `/proc/self/ns/mnt` and send exactly one descriptor with a one-byte
   payload `sendmsg(SCM_RIGHTS)`. One message and one close per connection.

**Client side (supervisor):** `receive_inheritable` connects, receives the
descriptor, closes any extra descriptors, checks that the file label from
`/proc/self/fd/<n>` starts with `mnt:`, and returns the raw descriptor
**without** `CLOEXEC`. `SCM_RIGHTS` descriptors are not close-on-exec and the
supervisor needs the child to inherit it. The one-shot `ns-exec` uses the
sibling `receive`, which reopens the descriptor as an owned close-on-exec
`File` instead.

**Lifetime and ownership:**

| Step | Holder | Action |
| --- | --- | --- |
| 1 | daemon | open `/proc/self/ns/mnt`, `sendmsg`, close its copy when `send` returns |
| 2 | supervisor | hold the raw descriptor while it binds the control socket and spawns the child |
| 3 | supervisor | child inherits it across `exec`; `NamespaceFd` closes the supervisor's copy on every path, including a failed spawn |
| 4 | child | reopen through `/proc/self/fd` as an owned `File` (label checked), close the raw inherited descriptor, `setns(CLONE_NEWNS)`, drop the `File` |
| 5 | command | holds no namespace descriptor after `exec` |

Failures: connect or receive failure, a non-`mnt:` label, or a `setns` failure
refuses the helper with exit `125` before a command runs. The supervisor is
never a receiver of the namespace itself: it does not call `setns` and does not
`chdir`.

### 2.2 Control socket and messages

The socket is bound **before** the child is spawned, so a listener failure
cannot leave an unsupervised child. A bind failure after the descriptor was
received closes the descriptor and exits `125`. The path is caller-chosen; the
`0600` mode and a private parent directory are the whole boundary.

**Framing:** a four-byte big-endian length followed by one kind byte and a
fixed body. The length counts the kind byte and must be in `1..=24`; the largest
body is 18 bytes (`StatusReply`). An unknown kind, a wrong body length or a
truncated read is an error, and the connection handler ends. Integers are
big-endian `i32`; booleans are a single `u8` (`0` false, non-zero true).

| Kind | Message | Direction | Body (bytes) | Meaning |
| --- | --- | --- | --- | --- |
| 1 | `Started {pid, pgid}` | supervisor → caller | 8 | the child has its own session; `pgid == pid` |
| 2 | `Exited {code, signal, group_alive}` | supervisor → caller | 9 | the direct child was reaped; exactly one is broadcast |
| 3 | `Signal {signo}` | caller → supervisor | 4 | signal the child's process group |
| 4 | `Status` | caller → supervisor | 0 | ask for the current snapshot |
| 5 | `StatusReply {running, pid, pgid, code, signal, group_alive}` | supervisor → caller | 18 | answer to `Status` |
| 6 | `Stop {grace_ms}` | caller → supervisor | 4 | `SIGTERM` the group, then `SIGKILL` after the bounded grace |

**Admission and delivery:** a peer is admitted only when its `SO_PEERCRED`
uid equals the supervisor's effective uid; the `0600` socket makes that check
explicit. A registration and the `snapshot()` written to that client happen
under the same `Session` mutex as `announce_started` and `finish`, so a caller
either registers before an announcement and receives it as a broadcast, or
registers afterwards and receives it in the snapshot. A failed write drops that
client. Multiple clients are allowed, each in its own receiver thread; the
protocol has no per-session ownership check.

`Started` and `Exited` received from a caller are ignored; the supervisor only
answers with `StatusReply` to `Status`. `Signal`/`Stop` have no acknowledgement
of their own: the outcome is observed through `Exited` or a later `Status`.

### 2.3 Process identity `(pid, pgid, starttime)`

The skeleton's identity is the pair `(pid, pgid)` announced in `Started`.
`pgid == pid` by construction (`setsid` in the child) and is not assumed: the
supervisor polls `/proc/<pid>/stat` until the `pgrp` field equals the pid before
announcing, with a five-second timeout. `pid` is the direct child, never the
supervisor, and never a caller-supplied value.

`starttime` is **not** captured in this slice. `/proc/<pid>/stat` field 22 is
the process start time in clock ticks after boot; recording it at `Started`
would make a later signal safe against pid reuse. Today no signal is sent to a
single pid, only to `kill(-pgid, signo)` validated against the session record,
so the missing field is not currently exploitable across processes. It is a
required piece of the future registry/cancel contract:

- identity for a registry or a cross-process cancel is
  `(pid, pgid, starttime)`;
- `starttime` is read once when the identity is published and re-read before
  any later signal; a mismatch means the pid was reused and the signal is
  refused;
- the skeleton must not claim this guarantee until the field is captured and
  tested.

### 2.4 Exit and `group_alive` semantics

The supervisor calls `waitpid(pid, 0)` exactly once (retrying `EINTR`), then
classifies the status:

| `wait4` result | `Exited.code` | `Exited.signal` | supervisor exit |
| --- | --- | --- | --- |
| `Exited(code)` | `code` | `0` | `code` (a code above 255 becomes `125`) |
| `Signaled(signal)` | `0` | `signal` | `128 + signal` |
| anything else | `125` | `0` | `125` |

The supervisor does not re-raise the signal; `128 + signal` is the shell
convention, not a claim that the supervisor died by that signal. A refusal
before or instead of a command also exits `125`.

`group_alive` is the answer to `kill(-pgid, 0)`, guarded by `pgid > 1` and
`pgid != supervisor's own pgid`. It means "a process group with this id is
observable right now":

- on `Exited`, `true` means members survived after the direct child was reaped
  (a grandchild); this slice reports that fact and does **not** kill them;
- on `Status` while the child is running, `true` is normally expected because
  the child itself is in the group;
- `false` means `ESRCH` (or a permission refusal) and must not be read as proof
  that every descendant is gone, nor as a licence to forget the group.

`Status` before `Started` reports `running: false`, `pid: 0`, `pgid: 0`,
`code: 0`, `signal: 0`, `group_alive: false`. After `Exited`, `running` is
`false` and the stored code/signal are returned.

## 3. Startup, signal and exit sequences

### 3.1 Startup

1. **Runtime / test harness** creates the private mount namespace and serves its
   `/proc/<pid>/ns/mnt` descriptor on a `0600` socket (in production the daemon
   owns this endpoint; the test harness mimics it).
2. **Runtime** starts
   `layerfs-daemon ns-exec-supervisor --socket <ns-fd> --control <path>
   [--directory <ws>] [--uid <u> --gid <g>] -- <command>`.
3. **Supervisor** parses the arguments, connects to `--socket`, receives the
   descriptor with `SCM_RIGHTS`, and validates the `mnt:` label. Failure exits
   `125` and no child exists.
4. **Supervisor** binds the control socket `0600`, removing a stale path first.
   Failure closes the descriptor and exits `125`.
5. **Supervisor** records its own pgid and starts the control accept thread.
6. **Supervisor** spawns the same binary in child mode with the descriptor's
   numeric fd and its own pid. The child inherits the raw descriptor. The
   supervisor closes its copy immediately after `spawn` returns, including on
   the error path.
7. **Child** reopens the descriptor through `/proc/self/fd` (label checked),
   closes the raw inherited fd, calls `setns(fd, CLONE_NEWNS)`, closes the
   owned fd, optionally `chdir`s to `--directory`, drops supplementary groups,
   gid and uid (`--uid` and `--gid` must be given together and must be non-zero),
   calls `setsid()`, sets `PR_SET_PDEATHSIG(SIGKILL)`, verifies
   `getppid() == --parent`, and `exec`s the command.
8. **Supervisor** polls `/proc/<child>/stat` until `pgrp == pid` (two
   millisecond interval, five second deadline). On success it announces
   `Started {pid, pgid: pid}` under the session lock; on timeout it logs and
   continues without a `Started`.
9. **Caller** may connect before or after step 8. The accept loop admits it and
   registers it under the session lock, then writes the current snapshot, so it
   receives `Started` (and `Exited`, if already known) exactly once.
10. **Supervisor** blocks in `waitpid`.

Who calls what: only the **child** calls `setns` and `chdir`. Only the
**supervisor** forks (`Command::spawn`), binds/serves the control socket and
calls `waitpid`. Only the **daemon** opens `/proc/self/ns/mnt`. The command
never sees the control socket or the namespace descriptor.

### 3.2 Signal and status

1. A caller writes `Signal {signo}` or `Stop {grace_ms}` on its control
   connection; its receiver thread decodes the frame.
2. `signal_group` checks, in order: the child has not exited; `Started` has
   happened and `pgid > 1`; `pgid` is not the supervisor's own process group;
   `signo` is a valid signal. It then calls `kill(-pgid, signo)`.
3. `Stop` first sends `SIGTERM` through the same check, then a timer thread
   sleeps for `min(grace_ms, 60_000)`; if the session still has no recorded
   exit it sends `SIGKILL` to `-pgid`. `Stop` has no presence, retry or
   Workspace policy and no acknowledgement.
4. `Status` builds `StatusReply` under the session lock:
   `running = started && exit.is_none()`, the stored pid/pgid,
   `code = signal = 0` when not exited, and a fresh `group_alive` probe.
5. When the child dies, `waitpid` returns and the exit sequence runs.

Invalid targets are refused, not clamped: no signal is sent to an arbitrary
pid, to a positive pid, or to the supervisor's own group. Because the protocol
carries no pid field, the only target the wire can name is the child's group.

### 3.3 Exit

1. `waitpid` returns the one wait status (`EINTR` retried).
2. The supervisor classifies it (table in §2.4) and probes `group_alive`.
3. `finish` stores `Exited` in the session and writes it to every connected
   caller under the same lock used for registration. A caller that connected
   after the exit still receives `Started` (if known) followed by `Exited`.
4. The supervisor drops the control path guard, which removes the socket file
   even on an error path after the bind.
5. It reaps exactly once; `Child::try_wait` afterwards only releases the pid
   record and cannot wait twice.
6. It exits with `code` or `128 + signal`, or `125` for an unexpected status.
7. Control connections then see EOF. A caller that never connected before this
   point cannot connect afterwards: the path is gone and the process is gone.

### 3.4 Supervisor crash

`PR_SET_PDEATHSIG(SIGKILL)` kills the child when the supervisor dies, and the
child's `getppid` check refuses a supervisor that died before the call. The
remaining gap is the interval between the supervisor's `spawn` and the child's
`prctl`: a supervisor that dies inside it leaves a child that has not yet
armed the signal, but that child then sees a different parent in the
`getppid` check and refuses before `exec`. A very narrow pid-reuse window
remains if the supervisor's pid is recycled before that check; it is recorded
as a known limit, not as a guarantee.

A crashed supervisor does not run its `Drop` guards, so the control socket path
can remain. `listen` removes a stale path on the next bind, but until then a
caller gets `ECONNREFUSED` on the stale path. No process reaps a grandchild and
no registry exists to clean the record.

## 4. Failure modes and invariants

| # | Failure | Detection / behaviour | Covered now |
| --- | --- | --- | --- |
| F1 | supervisor crashes with the child alive | `PDEATHSIG` kills the child; `getppid` refuses the pre-`prctl` window; narrow pid-reuse gap remains | no crash-injection test |
| F2 | daemon or control socket EOF | A caller disconnect does not stop the supervisor; it keeps waiting and exits with the child. A daemon death after handoff has no effect; before handoff the supervisor exits `125`. | no test |
| F3 | short command / socket race | The listener is bound before the spawn, but a caller that connects only after a very short child exits finds the path removed and cannot observe the session. Tests add `sleep 1` to keep it observable. | partially (test comment) |
| F4 | late caller / session lost | Once connected, the snapshot prevents missed or duplicated events. A caller that never connected cannot recover the session: no registry, no reconnect. | no test |
| F5 | signal races the exit | `signal_group` refuses when `exit` is recorded; a kill that races the child's death gets `ESRCH`, which is logged and not retried. `Stop`'s timer re-checks `exit` before `SIGKILL`. | no test |
| F6 | kill before reap | A zombie is not signalable; `kill(-pgid, 0)` can be `ESRCH` while the direct child's status is still unreaped. `Exited` is only sent after `wait4`, so the reported event does not depend on the probe. | indirect |
| F7 | `group_alive` misread | It is an observable-group probe, not ownership, not a clean-exit signal, and `false` on `EPERM`. Grandchildren are left running. | test 4 |
| F8 | arbitrary pid / own group | The protocol has no pid field; `pgid == own pgid` and `pgid <= 1` are refused; only `kill(-pgid, signo)` is reachable. | test 1/2/3 indirectly |
| F9 | supervisor becomes a mount holder | The supervisor never calls `setns` and never `chdir`s; it closes its descriptor copy after the spawn. | test 6 |
| F10 | `Started` never announced | If the five-second session poll times out, no `Started` is sent although the child may briefly exist; `Status` keeps reporting `running: false`, `Signal` refuses with "no session yet", and only `Exited` follows. | no test |
| F11 | control path collision | Two supervisors given the same `--control` path unlink each other's socket; the design requires a unique path per session. | no test |
| F12 | oversized/unknown frame | Length outside `1..=24`, unknown kind, wrong body size or a truncated read ends that connection handler. | no test |
| F13 | child refuses before `exec` | `setns`, `chdir`, identity drop, `setsid`, `prctl` or `exec` failure exits `125`; the supervisor reports it as a normal `Exited`. | partially (identity test) |

**Invariants:**

- **I1.** The supervisor never calls `setns` and never `chdir`s into the
  Workspace; only the child joins the namespace. A supervisor that held the
  mount would keep a FUSE mount referenced and make `unmount` return `Busy`.
- **I2.** The supervisor's namespace-descriptor copy is closed on every path
  after the spawn; the child's copy is closed before `exec`.
- **I3.** The control listener is bound before the child is spawned.
- **I4.** The direct child is reaped exactly once, and `Exited` is broadcast at
  most once.
- **I5.** A signal can only reach the child's own process group; the
  supervisor's group and arbitrary pids are unreachable through the wire.
- **I6.** One supervisor owns one session and one control path; all state is
  in memory and dies with the process.
- **I7.** Event delivery and caller registration share one lock, so a connected
  caller sees no duplicate and no gap between `Started` and `Exited`.
- **I8.** The supervisor's exit status reflects the child: its code, or
  `128 + signal`.
- **I9.** Command output is inherited, not proxied: the supervisor does not
  frame, buffer or truncate it.

## 5. Implemented in the skeleton vs deferred

`green` means implemented and exercised in this PR; `amber` means implemented
but not fully covered or intentionally incomplete; `deferred` means no code.

| Capability | State | Where / gap |
| --- | --- | --- |
| Process model: supervisor owns one child, child `exec`s the command | green | `run`, `spawn`, `run_child` |
| Namespace handoff over `SCM_RIGHTS`; supervisor never `setns`/`chdir` | green | `ns_fd`, `NamespaceFd`, test 6 |
| Child sequence: `setns`, `chdir`, drop groups/gid/uid, `setsid`, `PDEATHSIG`, `getppid` check, `exec` | green | `run_child`, tests 1/5 |
| Control socket: `0600`, bounded frames, peer-uid admission, snapshot, broadcasts | green | `ns_exec_control`, `serve`, tests 1–6 |
| `wait4`: reap once, code/signal, `128 + signal`, `125` refusal | green | `reap`, `outcome`, `exit_code` |
| `group_alive` probe reported on `Exited` and `Status` | green | `group_alive`, test 4 |
| `Signal` to the child's process group only | green | `signal_group`, tests 2/3 |
| `Stop` (`SIGTERM`, bounded grace, `SIGKILL`) | amber | implemented in `stop`; no test, no presence/policy |
| Process identity `(pid, pgid)` and observed own session | green | `Started`, `await_own_session` |
| Process identity `starttime` | deferred | not captured; needed for pid-reuse-safe registry/cancel |
| Client-presence detection and reconnect | deferred | no code; socket lives only with the supervisor |
| Daemon session registry and session ids | deferred | no code; `Session` is one in-memory slot |
| Force-kill orchestration / kill policy / escalation | deferred | `Stop` is a thin wrapper, not a policy |
| `workspace.close(force)` integration | deferred | no Engine/daemon integration |
| Killing or reaping grandchildren | deferred | only reported via `group_alive` |
| Engine / Sandbox-runtime wiring | deferred | hidden CLI modes only; `workspace.exec` unchanged |
| Authorization beyond socket mode + peer uid | deferred | no pid-namespace/cgroup/Workspace check |
| Output proxying, truncation, session multiplexing | deferred | output is inherited from the supervisor |

## 6. Test matrix

The tests live in
[`exec_supervisor.rs`](../../../../engine/crates/layerfs-daemon/tests/exec_supervisor.rs)
and are `#![cfg(target_os = "linux")]`. They need root with `CAP_SYS_ADMIN` and
`unshare`/`mount`, so they run in the privileged mount lane, not in the default
developer lane. They build a private mount namespace with `unshare -m`, mount a
tmpfs there, write a marker that exists only in that namespace, serve the
namespace descriptor over `SCM_RIGHTS`, and drive the real
`layerfs-daemon` binary over the control socket.

| Test | What it proves |
| --- | --- |
| `a_command_joins_the_namespace_enters_the_directory_and_reports_exit_7` | child joins the served namespace, `chdir`s to the Workspace, reads the namespace-local marker, `Started` has `pgid == pid`, the pid differs from the supervisor, `Exited` is `(7, 0, false)`, supervisor exits `7` |
| `a_signal_reaches_the_child_group_and_is_reported` | `Signal(SIGTERM)` on `sleep 30` yields `Exited {signal: 15}` and supervisor exit `143` |
| `status_reports_the_running_child_and_a_kill_still_reaches_it` | `StatusReply` reports the running child and a live group; `Signal(SIGKILL)` yields signal `9`, exit `137` |
| `a_surviving_grandchild_is_reported_rather_than_a_clean_exit` | `sh -c 'sleep 5 & sleep 1'` exits `0` with `group_alive: true`; the test cleans the grandchild up because this slice does not |
| `the_child_runs_as_the_identity_it_drops_to` | a `65534:65534` drop reports `65534` for both `id -u` and `id -g` |
| `the_supervisor_stays_outside_the_workspace_namespace_and_directory` | the supervisor's `/proc/<pid>/ns/mnt` and `cwd` stay equal to the caller's, not the private namespace's |

The `0600` mode of the control socket is asserted by every test when it
connects. These tests do not cover the following, which need later CI:

| Missing coverage | Why it matters |
| --- | --- |
| `Stop` (SIGTERM → grace → SIGKILL) | the only escalation path, currently untested and unpoliced |
| supervisor `SIGKILL` with a live child (`PDEATHSIG`) | the crash guarantee is argued, not observed |
| caller EOF / mid-frame disconnect | proves a lost caller does not disturb the session |
| connect-after-exit / short-command race | proves the documented "session lost" outcome |
| `Started`-missing timeout path (`F10`) | no test of a live-but-unannounced child |
| stale socket after a crashed supervisor | no test of rebinding/cleanup |
| multiple simultaneous callers | snapshot and broadcast are only single-client tested |
| peer uid rejection | admission is only exercised by a matching uid |
| pid reuse / `starttime` mismatch | no identity field yet |
| daemon/supervisor orchestration end to end | no Engine wiring exists |
| non-Linux build of the hidden modes | the `#[cfg(not(target_os = "linux"))]` stubs are compile-only |

## 7. Open questions

1. **`starttime`.** The target identity is `(pid, pgid, starttime)`, but the
   skeleton publishes only `(pid, pgid)`. A registry or cross-process cancel
   cannot be pid-reuse-safe until field 22 is captured and checked.
2. **Launcher ownership.** "Daemon-side launcher" conflicts with
   `application/mod.rs` ("no command supervisor") and with the declined
   launcher proposal that kept exec ownership in the Sandbox runtime. Where the
   future orchestrator lives must be decided before the seam is filled in.
3. **`Exited` without `Started`.** A five-second session-poll timeout produces
   a report with no pid. Callers must either tolerate it or the supervisor must
   refuse instead of continuing; the current choice is "log and continue".
4. **`group_alive` meaning is overloaded.** While running it mostly says "the
   child exists"; after exit it says "descendants remain". A future protocol
   may want two distinct fields.
5. **`Stop` is policy-shaped but untested.** It is a bounded SIGTERM wrapper,
   not presence or force policy; whether it belongs in the supervisor or the
   future orchestrator is open.

> Items 1, 2 and 5 are resolved by [Presence + registry decisions
> (locked)](#presence--registry-decisions-locked); they stay listed as the
> record of what was open at this checkpoint.

## Presence + registry decisions (locked)

This section locks the decisions that §1.5, §1.6 and §7 leave open. They are
design decisions for the phases after this skeleton; they do not add code to
this PR and they do not change any behaviour described above.

1. **Launcher.** Engine exec spawns the supervisor and owns stdio and the exit
   code. The daemon only receives the registration and owns the registry,
   presence and policy. The daemon does not become a general command I/O broker
   and does not spawn the supervisor in this phase.
   *Rationale:* §1.5's open question — "where the orchestrator lives" — is
   answered without reversing the current rule that the daemon has no command
   supervisor and without reintroducing the declined launcher proposal. The
   daemon's new surface stays a record-and-observe one, so a daemon restart or
   policy decision cannot alter a running command's byte streams or exit status.

2. **Lease granularity.** One dedicated lease connection per client process per
   Sandbox, multiplexing all of that process's sessions over it — not one
   connection per exec. Session ids and per-session tokens identify the
   individual sessions. The connection budget is one lease connection plus
   pooled RPC slots.
   *Rationale:* per-exec connections make the connection count scale with
   concurrent commands, which is exactly the quantity a client controls; a
   single multiplexed lease bounds it to one per client process per Sandbox and
   gives presence a natural unit ("this client process is gone") instead of
   inferring it per exec.

3. **Daemon restart.** P1 does not adopt old sessions. On a daemon restart the
   registry and the lease are lost; supervisors treat control EOF according to
   their foreground (cancel/cleanup) versus background (keep) policy; only host
   records remain. This is documented as **lost-session semantics**, not
   recovery.
   *Rationale:* adopting sessions would require durable, replayable session
   state and a supervisor that can be re-addressed after the daemon restarts —
   neither exists and neither is needed for the first phase. Stating it as
   lost-session semantics keeps the guarantee honest: after a daemon restart the
   command may still run, but no session is addressable through the daemon.

4. **Lease auth.** Require `SO_PEERCRED` **plus** a daemon-issued per-session
   capability/token delivered over the authenticated control channel. Do not put
   the token in argv: same-uid processes can read `/proc/<pid>/cmdline`. If
   same-Sandbox hostile multi-tenancy is in scope, user/namespace isolation is
   additionally required.
   *Rationale:* the skeleton's peer-uid check (§2.2, §5 "Authorization beyond
   socket mode + peer uid: deferred") is admission, not authorization: every
   process with the same uid in the Sandbox can pass it. A per-session token
   bound to an authenticated channel narrows authority to the session; argv is
   not a channel because `/proc/<pid>/cmdline` is readable by same-uid peers.
   A token alone still does not defend against a same-uid attacker who can read
   the process's memory or fds, so hostile multi-tenancy additionally needs
   user or namespace isolation, which is out of scope here.

5. **Background.** Do not overload one boolean. Keep `mode: foreground |
   background` and a separate `onDisconnect: cancel | keep | reconnectWindow`.
   This phase ships `background` with `cancel`; `reconnectWindow` comes later.
   `detachOnDisconnect` is not its own state: it is `foreground` + `keep`.
   *Rationale:* "background" answers *who waits for the command*; "what happens
   when the client disappears" is a different question with different
   mechanisms. Collapsing them into one flag produces cases (background +
   cancel, foreground + keep) that cannot be expressed, and `reconnectWindow`
   is a third policy value, not a fourth boolean.

6. **Cancelling.** No 500 ms reversible *Cancelling* window. The reconnect
   window is the *Orphaned* phase (5 s default); once *Cancelling* starts it is
   one-way; the grace period is only `TERM` → `KILL` cleanup time.
   *Rationale:* a reversible *Cancelling* state means a cancel can be undone
   after signals may already have been delivered, which makes the observed
   outcome race the client's intent. Two distinct phases — *Orphaned*, which is
   recoverable, and *Cancelling*, which is not — keep "may I resume this?" and
   "is this being torn down?" answerable without inspecting timers. The grace
   period then has one meaning: how long `TERM` gets before `KILL`.

### Impact on phases

The decisions above map onto the phases as follows; only P1's registry and
`starttime` capture are preconditions for anything else.

| Phase | Adds | Enabled by |
| --- | --- | --- |
| P1 | daemon session registry, `(pid, pgid, starttime)` identity | decision 1 (daemon records, does not broker), decision 2 (lease unit), decision 3 (no adoption) |
| P2 | presence detection, orphan reaper | decision 2 (client-process lease), decision 4 (authenticated lease), decision 6 (*Orphaned* phase) |
| P3 | reconnect, background/detach policy | decision 5 (`mode` / `onDisconnect`), decision 6 (one-way *Cancelling*) |
| P4 | Engine wiring | decision 1 (Engine spawns, owns stdio and exit code) |
