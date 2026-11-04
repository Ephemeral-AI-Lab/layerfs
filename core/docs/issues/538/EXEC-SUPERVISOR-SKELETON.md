# Exec session supervisor skeleton

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract.

This note describes only the skeleton implemented on top of the unmerged #499
private-mount-namespace work. It adds a hidden process model, a control socket
and `wait4`. It deliberately does not implement client-presence detection, the
daemon session registry, force-kill orchestration or `close(force)`
integration.

## Why

#499's hidden `ns-exec` is a one-shot helper: it joins the daemon's mount
namespace, drops to the command identity and `exec`s the command, so the helper
process *is* the command. A session that can be asked for status and can be
signalled needs a process that survives next to the command. That process is
`ns-exec-supervisor`.

## The supervisor-must-not-setns invariant

The supervisor stays outside the Workspace. It **never** calls `setns` and
**never** `chdir`s into the Workspace, because a process inside the Workspace's
mount namespace can keep a FUSE mount referenced and make `unmount` return
`Busy`. It holds the daemon's mount-namespace descriptor only to pass it to its
child and closes its own copy immediately after the spawn.

The child is the only process that joins the namespace. It performs #499's
sequence, then takes a new session:

1. reopen the inherited descriptor through `/proc/self/fd` and check that it is
   a mount namespace;
2. `setns(fd, CLONE_NEWNS)`, then close both descriptors;
3. `chdir(workspace)` when `--directory` is given;
4. drop supplementary groups, gid, uid to the command identity;
5. `setsid()` so the child is its own session and process-group leader;
6. set `PR_SET_PDEATHSIG` to `SIGKILL` and verify the supervisor is still the
   parent;
7. `exec` the command.

## Control socket

The supervisor binds a Unix socket at `--control` with mode `0600` before it
spawns the child. Frames are a four-byte big-endian length and one kind byte
plus a fixed small body; an unknown kind or an out-of-range length is an error.
The socket admits a peer with the supervisor's own effective uid. Any further
authorization (pid namespace, cgroup, Workspace membership) is deferred; the
socket mode and the private directory are the whole boundary for now.

| Message | Direction | Body |
| --- | --- | --- |
| `Started {pid, pgid}` | supervisor → caller | pid, pgid |
| `Exited {code, signal, group_alive}` | supervisor → caller | code, signal, group_alive |
| `Signal {signo}` | caller → supervisor | signo |
| `Status` | caller → supervisor | — |
| `StatusReply {running, pid, pgid, code, signal, group_alive}` | supervisor → caller | all fields |
| `Stop {grace_ms}` | caller → supervisor | grace_ms |

A caller that connects after the child started receives the current
`Started`/`Exited` snapshot under the same lock that registers it, so it cannot
miss or duplicate the events. `Signal` and `Stop` are applied with
`kill(-pgid, signo)` and only to the child's own group: the supervisor's own
group and every other pid are refused. `Stop` is a thin SIGTERM wrapper with a
bounded grace before SIGKILL; it is not presence or force policy.

## wait4 and group behaviour

The supervisor reaps the direct child once with `waitpid`, reports its exit code
or terminating signal, and exits `128 + signal` when the child was signalled (a
process cannot exit by signal without re-raising it). Before the `Exited`
frame it probes the group with `kill(-pgid, 0)`: `group_alive: true` says that
members remained after the child exited. Grandchildren are **not** killed in
this slice.

`PR_SET_PDEATHSIG` narrows the crash gap: if the supervisor dies, the child is
killed. The supervisor also re-checks `getppid` after setting the signal, so a
child whose supervisor died inside that window refuses instead of running
unsupervised. The remaining gap is a supervisor that dies before the child sets
the signal; the check catches only the window after it.

## Deliberately deferred

- client-presence detection and reconnect (does the caller still exist?);
- a daemon-side session registry and session ids;
- force-kill orchestration, kill policy, grace defaults and escalation;
- `workspace.close(force)` integration and container-level recovery;
- reaping or killing grandchildren;
- authorization beyond the socket mode and the peer uid;
- retaining session state after the supervisor exits. The control socket lives
  only as long as the supervisor, so a caller that has not connected before a
  very short child exits cannot observe that session. Keeping the record (and
  reconnecting to it) is the deferred daemon registry work.

## Tests

`engine/crates/layerfs-daemon/tests/exec_supervisor.rs` is Linux-gated and runs
in the mount lane (root with `CAP_SYS_ADMIN`), like `ns_exec.rs`:

- a child joins the namespace served over `SCM_RIGHTS`, enters the Workspace
  directory, reads a marker that exists only there, and the supervisor exits 7
  for `sh -c 'cat marker; sleep 1; exit 7'`; `Started` reports `pgid == pid` and
  a pid different from the supervisor's (the short hold keeps the fast command
  observable, since the supervisor exits with its child);
- the supervisor's own mount namespace and cwd stay outside the Workspace;
- the command runs as `65534:65534` when the helper is told to drop there;
- `sleep 30` plus `Signal(SIGTERM)` ends in `Exited {signal: 15}` and supervisor
  exit 143;
- `Status` reports the running child and `Signal(SIGKILL)` ends in exit 137;
- `sh -c 'sleep 5 & sleep 1'` reports `group_alive: true` instead of a clean
  exit.
