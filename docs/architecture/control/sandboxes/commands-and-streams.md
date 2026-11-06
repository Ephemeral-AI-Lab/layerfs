# Commands and streams

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Commands and streams

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Running a command and returning its streams and status.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-isolation/src/runtime.rs`](../../../../control/crates/layerfs-isolation/src/runtime.rs), [`control/crates/layerfs-isolation/src/types`](../../../../control/crates/layerfs-isolation/src/types), [`backend/docker/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/exec.rs), [`backend/docker/streams.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/streams.rs), [`backend/host_process/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/exec.rs), [`backend/namespaces/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/namespaces/exec.rs) |

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
   Docker       create exec -> start (hijacked) -> copy -> inspect
   HostProcess  spawn as uid/gid, own process group -> copy -> wait
   Namespaces   spawn helper exec (enters the namespaces) -> copy -> wait
```

| Field check | Rule | Source |
| --- | --- | --- |
| Identity | uid and gid non-zero, else `Protocol("nonroot command uid/gid")`. | [`types/execution.rs`](../../../../control/crates/layerfs-isolation/src/types/execution.rs) |
| Arguments | At least one, first non-empty, no NUL. No shell is added by the runtime; `bash -c` comes from the API. | [`types/request.rs`](../../../../control/crates/layerfs-isolation/src/types/request.rs) |
| Environment | `NAME=value`, not starting with `=`, no NUL. Nothing else of the runtime's environment reaches the command but `PATH` (local backends). | same; [`host_process/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/exec.rs) |
| Directory | Absolute, no NUL. | same |
| Stdin | Local backends refuse it (`Unsupported`); Docker `run` refuses it, while the Docker Exec API itself supports it (see the example below). | [`host_process/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/exec.rs), [`docker/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/exec.rs) |
| Timeout | None. A command runs until it ends. | each `exec.rs` |

Docker's steps, each attempted once
([`exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/exec.rs),
[`streams.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/streams.rs)):

| Step | Request | Connection |
| --- | --- | --- |
| create | `POST /containers/<id>/exec`, `Tty: false`, `User: "uid:gid"` | kept |
| start | `POST /exec/<exec>/start` | upgraded (hijacked) |
| output | demultiplex the 8-byte-header frames to the two sinks | the same stream |
| inspect | `GET /exec/<exec>/json` | kept |

## Streams

```text
 command process / Engine exec
   stdout pipe ----+                  Docker: one hijacked stream,
   stderr pipe --+ |                  frames [type,0,0,0,len u32] split
                 | |                  to stdout / stderr
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
| Counts are what each sink accepted (`ExecExit::stdout_bytes`, `stderr_bytes`), not what was queued. | [`local/streams.rs`](../../../../control/crates/layerfs-isolation/src/local/streams.rs), `OutputProgress` in [`streams.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/streams.rs) |
| A sink failure closes the command's pipes, still waits for its exit, and returns the sink's error. | [`host_process/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/exec.rs), [`namespaces/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/namespaces/exec.rs) |
| End of stream is transport completion only, never the exit. | `copy_to` in [`streams.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/streams.rs) |
| No output is cut: a stream over the 1 MiB inline window is spooled (`truncated` is always false). | `INLINE` in [`sandbox/output.rs`](../../../../control/crates/layerfs-control/src/sandbox/output.rs), [`sandbox/exec.rs`](../../../../control/crates/layerfs-control/src/sandbox/exec.rs) |
| The API also emits the output as `stdout` / `stderr` events after the call, for front ends that stream. | [`workspace/calls.rs`](../../../../control/crates/layerfs-control/src/workspace/calls.rs) |

## Exit status

| Backend | `ExecExit.code` | `pid` | Source |
| --- | --- | --- | --- |
| Docker | `ExecInspection::known_root_exit`: the inspected exit code, only when not running. Unknown is `ExecExitUnknown`, keeping the handle; nothing is polled or replayed. | the Engine's, diagnostic only, maybe `None` | [`docker/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/exec.rs), [`types/execution.rs`](../../../../control/crates/layerfs-isolation/src/types/execution.rs) |
| HostProcess | the exit code, or 128 + signal | the spawned pid | [`host_process/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/exec.rs) |
| Namespaces | the helper's exit code, which is the command's code or 128 + its signal; 125 is the helper's own failure; a helper ended by a signal is a failure, not an exit code | the helper's pid | [`namespaces/exec.rs`](../../../../control/crates/layerfs-isolation/src/backend/namespaces/exec.rs), [`bin/…/main.rs`](../../../../control/crates/layerfs-isolation/src/bin/layerfs-sandbox-helper/main.rs) |

The exit is the root process's. Descendants, open descriptors and the
filesystem drain are separate facts ([`execution.rs`](../../../../control/crates/layerfs-isolation/src/types/execution.rs));
a command's exit does not prove its children ended. Docker has no
per-Exec cancellation: `ExecHandle::cancel` refuses `Unsupported` before any
effect.

## Keep-alive and stop

```text
 Docker client          one kept idle connection (Exec create, inspect);
                        idle at most 30 s; used only when quiet; a failure
                        on it is the request's own, never resent
 Sandbox lifetime       no idle stop: a Sandbox runs until sandbox.close

 stop(timeout_seconds)  attempted once; delete needs a known stop
   Docker      POST /containers/<id>/stop?t=<s>   (304 = already stopped)
   HostProcess detach mounts; SIGTERM, wait <s>; SIGKILL, wait 5 s;
               then KILL every named process group, wait 5 s
   Namespaces  write cgroup.kill; wait max(<s>, 5 s) until the init is
               gone and the cgroup has no process
```

| Step | Docker | HostProcess | Namespaces |
| --- | --- | --- | --- |
| `stop` twice | refused | refused | refused |
| `delete` before a known stop | refused | refused | refused |
| `delete` | `DELETE /containers/<id>?force=false&v=false`; the Store volume stays | kill groups if not yet, detach mounts, remove the directory | refuse while the cgroup holds a process; remove cgroup and directory |
| Source | [`docker/sandbox.rs`](../../../../control/crates/layerfs-isolation/src/backend/docker/sandbox.rs) | [`host_process/sandbox.rs`](../../../../control/crates/layerfs-isolation/src/backend/host_process/sandbox.rs) | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-isolation/src/backend/namespaces/sandbox.rs) |

Neither stop nor delete is retried. On `Drop` a local owner only reaps a
daemon or init stage that already exited; a running one is left for `stop`.

## Related

- [Docker](docker.md) and [Native and namespaces](native-and-namespaces.md): the backends behind `run`.
- [Engine: Daemon](../../engine/daemon/README.md): what serves the Workspace the command reads and writes.
- [A Workspace session](../../flows/workspace.md): `exec` in the whole session.

---

← [Sandboxes: Native and namespaces](native-and-namespaces.md) · [Up: Sandboxes](README.md) · [Sandboxes: Adding a backend](adding-a-backend.md) →
