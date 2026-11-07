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
| Environment | `NAME=value`, not starting with `=`, no NUL. Nothing else of the runtime's environment reaches the command but `PATH`. | same; [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Directory | Absolute, no NUL. | same |
| Stdin | Both backends refuse it (`Unsupported`). | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Timeout | None. A command runs until it ends. | each `exec.rs` |

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
| No output is cut: a stream over the 1 MiB inline window is spooled (`truncated` is always false). | `INLINE` in [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs), [`exec.rs`](../../../../control/crates/layerfs-sandbox/src/exec.rs) |
| The API also emits the output as `stdout` / `stderr` events after the call, for front ends that stream. | [`workspace/calls.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs) |

## Exit status

| Backend | `ExecExit.code` | `pid` | Source |
| --- | --- | --- | --- |
| HostProcess | the exit code, or 128 + signal | the spawned pid | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Namespaces | the helper's exit code, which is the command's code or 128 + its signal; 125 is the helper's own failure; a helper ended by a signal is a failure, not an exit code | the helper's pid | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`bin/…/main.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/main.rs) |

The exit is the root process's. Descendants, open descriptors and the
filesystem drain are separate facts ([`execution.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/execution.rs));
a command's exit does not prove its children ended.

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
