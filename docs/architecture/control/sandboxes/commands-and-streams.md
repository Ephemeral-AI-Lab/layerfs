# Commands and streams

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Commands and streams

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Running a command and returning its streams and status.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-sandbox-backend/src/runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`control/crates/layerfs-sandbox-backend/src/types`](../../../../control/crates/layerfs-sandbox-backend/src/types), [`backend/host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs), [`backend/namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs) |

The Sandbox owns command launch, streams and exit status. The daemon
serves the Workspace's files and supervises no command; a command reaches
the Workspace through ordinary file calls on the mount.

## Exec

```text
 workspace.exec(ws, command)                   API control, sandbox/exec.rs
   ExecRequest {
     identity:    CommandIdentity { uid, gid }    nonroot (0 refused)
     arguments:   ["bash", "-c", command]
     environment: ["HOME=<scratch>"]              /tmp, or <sandbox>/tmp
     directory:   the Workspace's mount path      absolute
     stdin:       false
   }
        |
        v
 Sandbox::run(&request, stdout_sink, stderr_sink) -> ExecExit
   HostProcess  spawn as uid/gid, own process group -> copy -> wait
   Namespaces   spawn helper exec (enters the namespaces, installs the
                user-namespace filter) -> copy -> wait
```

| Field check | Rule | Source |
| --- | --- | --- |
| Identity | uid and gid non-zero, else `Protocol("nonroot command uid/gid")`. | [`types/execution.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/execution.rs) |
| Arguments | At least one, first non-empty, no NUL. No shell is added by the runtime; `bash -c` comes from the API. | [`types/request.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/request.rs) |
| Environment | `NAME=value`, not starting with `=`, no NUL. Nothing else of the runtime's environment reaches the command but `PATH`, and on `HostProcess` on macOS `RUSTUP_HOME`, `CARGO_HOME` and `GIT_CONFIG_GLOBAL` from the caller's environment or `HOME` (`~/.rustup`, `~/.cargo`, `~/.gitconfig` or XDG `git/config`) when present and not overridden by `request.environment`. | same; [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Directory | Absolute, no NUL. | same |
| Stdin | Both backends refuse it (`Unsupported`). | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Timeout | None. A command runs until it ends. | each `exec.rs` |

The macOS `HostProcess` homes are the caller's own, writable ones, not copies:
a command there (`cargo install`, `rustup update`, a git write) changes the
host's toolchain, installed binaries and global Git configuration, as the
same command run by the user would.

## Streams

```text
 command process
   stdout pipe ----+
   stderr pipe --+ |
                 | |
                 v v
 Sandbox::run: copy each stream to its sink as it arrives (8 KiB windows),
               stderr on a scoped thread, stdout on the caller's thread
                 |
                 v
 API Spool (one per stream): inline up to 1 MiB, else spooled to a file
                 |
                 v
 caller: {exitCode, stdout, stderr} or an `output` handle
         (workspace.exec_output), plus stdout/stderr events
```

| Rule | Source |
| --- | --- |
| Counts are what each sink accepted (`ExecExit::stdout_bytes`, `stderr_bytes`), not what was queued. | [`local/streams.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/streams.rs) |
| A sink failure closes the command's pipes, still waits for its exit, and returns the sink's error. | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs), [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs) |
| End of stream is never the exit: the exit comes from waiting on the process. | [`local/streams.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/streams.rs), each `exec.rs` |
| No output is cut: a stream over the 1 MiB inline window is spooled (`truncated` is always false). | `INLINE` in [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs), [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| The API also emits the output as `stdout` / `stderr` events after the call, for front ends that stream. | [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |

## Exit status

| Backend | `ExecExit.code` | `pid` | Source |
| --- | --- | --- | --- |
| HostProcess | the exit code, or 128 + signal | the spawned pid | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Namespaces | the helper's exit code, which is the command's code or 128 + its signal; 125 is the helper's own failure; a helper ended by a signal is a failure, not an exit code | the helper's pid | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`bin/…/main.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/main.rs) |

The exit is the root process's. Descendants, open descriptors and the
filesystem drain are separate facts ([`execution.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/execution.rs));
a command's exit does not prove its children ended.

## Exec records and cancel

Each command has an exec id and, while it runs, a record beside its
Workspace's own (compute-plane 2.0 §5.2, D-10, layerfs-project#113):

```text
 sandbox/<name>/workspace/<workspace id>.json          the Workspace
 sandbox/<name>/workspace/<workspace id>/exec/<id>.json one running command
   { v, id, workspace, path, name, owner, route, stop, started, cancel? }
   owner  the process that runs the exec (pid, start, boot)
   route  local | machine:<name> (a Sandbox this home reaches directly,
          whose command a cancel reaches through its worker, D-11)
   stop   null until spawn, then { cgroup: { path } }
          or { group: { pgid, pid, pid_start, pid_boot } }
```

The record is on the side that keeps the Workspace's record: the home on
`native` and for a forwarded Sandbox, the worker's root otherwise. It is
created new under the Sandbox lock (a live duplicate is `Exists`, with no
scan), told its stop handle once the command runs in its group and
removed when the command ends
([`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs)).

| Backend | Stop handle | At spawn | Cancel | Source |
| --- | --- | --- | --- | --- |
| Namespaces | `Cgroup { path }`: `<sandbox cgroup>/ws-<workspace id>/exec-<exec id>` | the helper stays in the Sandbox's cgroup and opens the command's `cgroup.procs` (`exec --command-cgroup`); the command's process moves itself into it just before exec, so the group holds only the command and what it starts, never the helper, and a command that cannot join never runs. Once the exec succeeded the helper writes one byte to its stdin, a socket the backend holds; only then is the handle told. The backend's wait for it ends when the helper writes or exits: no timer | SIGTERM to each process in it, wait the grace, `cgroup.kill`, check it is empty; the cgroup is removed when the command ends | [`namespaces/cgroup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/cgroup.rs), [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs) |
| HostProcess | `Group { pgid, leader }`: the command's own process group and its leader's identity | after the group is named | SIGTERM to the group (leader and boot checked against id reuse), wait the grace, SIGKILL | [`local/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/groups.rs), [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |

| Rule | Source |
| --- | --- |
| A cancel marks the record (`cancel`) under the Sandbox lock, then signals once outside it; nothing times out, retries or replays. | [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) `cancel`, [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |
| A cancel before the stop handle is known (the record exists, the command does not run yet) answers `cancelled` with `SIGKILL` and only marks the record: it never answers `ended` for a command that then runs. Told the handle, the owner finds the mark (`Running::spawned` refuses it as interrupted) and ends the command at once: `cgroup.kill` (namespaces) or SIGKILL to its group (HostProcess); its exec answers `kind: "cancelled"`, exit code 137. For a forwarded Sandbox the worker sends `{ exec_started: { stop } }` as an event once the command runs; the home's exec records the handle from it, or ends the command in the worker with zero grace when the mark came first. | `exec/record.rs` `Running::spawned`, `namespaces/exec.rs` `told`, `host_process/exec.rs` `run_in`, [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `reporting`, `workspace/exec.rs` `exec` |
| The mark stays when a cancel answers `unknown`: a later cancel tries again, and if the command then ends by a signal its exec answers `kind: "cancelled"`. | `exec/cancel.rs` `cancel` |
| `workspace.unmount` and `sandbox.close` read their records first: a live one refuses them `Busy` (`detail.execs`); `force` cancels each first with the default grace; one not confirmed refuses `Unknown`. | `exec/cancel.rs` `settle` |
| A record whose owner process is gone is stale: the next start, cancel, unmount or close that finds it ends its command (zero grace), removes it and logs it to `log/exec.jsonl`; `layerfs doctor` reports it without acting. A stale `machine:<name>` record (a Sandbox this home reaches directly) is ended in its worker by the same `workspace.exec_cancel` in `sandbox.host`: the caller first takes it over under the Sandbox lock (owner: itself), asks the worker without the lock, then removes it. A worker that answers `NoSuchSandbox` confirms it `ended`; one not reached, not answering or refusing otherwise leaves it `unknown` and the record is put back as it was: unmount and close are refused `Unknown`, and its id stays taken (`Exists`). | `exec/cancel.rs` `stale`, `claim`, `stale_remote`, [`exec/remote.rs`](../../../../control/crates/layerfs-sandbox/src/exec/remote.rs), [`doctor/scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |
| A worker that is not reached, or does not answer, makes a cancel `{ outcome: "unknown" }`: nothing is known to have changed. | [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) `exec_cancel` |
| A terminal unmount removes the Workspace's folder `workspace/<workspace id>/`. | [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |

## Keep-alive and stop

```text
 Sandbox lifetime       no idle stop: a Sandbox runs until sandbox.close

 stop(timeout_seconds)  attempted once; delete needs a known stop
   HostProcess detach mounts; SIGTERM, wait <s>; SIGKILL, wait 5 s;
               then KILL every named process group, wait 5 s
   Namespaces  write cgroup.kill; wait max(<s>, 5 s) until the init is
               gone and the cgroup has no process
```

| Step | HostProcess | Namespaces |
| --- | --- | --- |
| `stop` twice | refused | refused |
| `delete` before a known stop | refused | refused |
| `delete` | kill groups if not yet, detach mounts, remove the directory | refuse while the cgroup holds a process; remove cgroup and directory |
| Source | [`host_process/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/sandbox.rs) | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs) |

Neither stop nor delete is retried. On `Drop` a local owner only reaps a
daemon or init stage that already exited; a running one is left for `stop`.

## Related

- [Native and namespaces](native-and-namespaces.md): the backends behind `run`.
- [Engine: Daemon](../../engine/daemon/README.md): what serves the Workspace the command reads and writes.
- [A Workspace session](../../flows/workspace.md): `exec` in the whole session.

---

← [Sandboxes: Native and namespaces](native-and-namespaces.md) · [Up: Sandboxes](README.md) · [Sandboxes: Adding a backend](adding-a-backend.md) →

### Cancellation before registration

A cancel with no registered command retains a pending mark for 30 seconds
while answering `ended`. `Running::start` consumes it under the Sandbox lock
and ends the command at spawn (`kind: "cancelled"`, exit code 128 + the signal).
The mark
is local to the Workspace and id, removed when consumed and ignored after
expiration. Reuse after consumption is unaffected. An unconsumed mark may
cancel reuse within that interval; use unique ids. Expired unconsumed files
are removed on access or Workspace teardown. No call is retried or replayed.

At-spawn cancellation can finish under SIGTERM before SIGKILL. The exec
reports the actual 128 + signal exit (143 or 137), with `kind: "cancelled"`.
