# sandbox family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `sandbox` calls: start, list, read and stop a named Sandbox, one project's Workspace daemon on a machine, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/sandbox`](../../../../api/crates/layerfs-api/src/sandbox), [`layerfs-sandbox/src`](../../../../control/crates/layerfs-sandbox/src) (`family.rs`, `create.rs`, `list.rs`, `get.rs`, `close.rs`, `lifecycle.rs`) |
| Runs by default | `list`: the caller's process, home records only. `create`, `get`, `close`: records and locks in the caller's process, then the Sandbox's machine: this process for a local machine, else the machine's worker (`sandbox.host`); `get` may dial a forwarded daemon directly |
| Design | [Sandboxes](../../control/sandboxes/README.md), [daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [backends](../../control/sandboxes/backends.md) |

## Calls

```text
 project.init --> create <project> <name> [machine]  (starts the daemon)
                     |
                     +--> list / get                (records, Workspaces)
                     +--> workspace.mount ... unmount
                     v
                  close <name>   (ends the daemon; clears an unresolved
                                  create or close)
```

| Call | Returns |
| --- | --- |
| [`sandbox.create`](#sandboxcreate) | the Sandbox record, `state: "running"`, `forward_failed?` |
| [`sandbox.list`](#sandboxlist) | `{ items: [record], next_page_pos }` |
| [`sandbox.get`](#sandboxget) | the record, live `state`, one page of `workspaces`, `down_reason?`, `down?`, `daemon_capacity?` |
| [`sandbox.close`](#sandboxclose) | how it ended |

The **record** is `SandboxRecord::to_sdk`
([`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs)):
the stored fields `name`, `project`, `machine`, `created`, `id`, `platform`,
`runtime`, `image`, `backend`, `route` (never `v`, `runtime_id` or
`daemon`), the derived `state` (`creating`, `closing`, `unresolved`, `gone`,
`lost` or `running`) and, while an operation holds it, `op: { id, name,
phase, started, uncertain }`
([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs)). A folder scope
refuses every `sandbox` call (`out of scope: sandbox.<call>`).

### sandbox.create

Starts a named Sandbox for one project on a machine, ready for
`workspace.mount`.

```text
JS      await layerfs.sandbox.create(project, name, machine?, { image, on_progress })
Python  layerfs.sandbox.create(project, name, machine=None, *, image=None, on_progress=None)
CLI     layerfs sandbox create <project> <name> [<machine>] [--image <image>] [--progress]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | Name | unique across the home; not `native` or `docker` (built-in machine names) |
| `machine` (optional) | Name | default a sqlite Store's machine, else `native`; a sqlite Store's Sandbox must run on that machine, and the support table's `sandbox_allowed` must accept the platform and Store kind |
| `image` option | Image | a digest, Docker image Id or Docker reference: one the machine holds (unpacked, or in its archive) starts at once; any other is copied from the host's Docker first, as [`machine.load`](machine.md#machineload) does (progress `inspect`, `export`, `send`, `unpack`, then `start`). Default the machine record's image (not on `docker`), else the archive's; refused on macOS `native` ([`deploy.rs`](../../../../control/crates/layerfs-sandbox/src/deploy.rs), [`image_load.rs`](../../../../control/crates/layerfs-sandbox/src/image_load.rs)) |

| Field | Meaning |
| --- | --- |
| `platform` | `docker` (the docker machine), `mac` or `linux` ([`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs)) |
| `runtime`, `backend` | `native` (macOS host process) or `namespaces` (Linux, the `docker` machine's worker container included) |
| `image` | the namespaces root's id (a Docker image Id or a config digest), or `local`; each create sets that image's sidecar `last_used` |
| `route` | `nfs` for a macOS host process, else `fuse` ([`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs)) |
| `forward_failed` | why the caller could not forward the daemon port; Workspace calls then go through the worker ([`sandbox/create.rs`](../../../../api/crates/layerfs-api/src/sandbox/create.rs)) |

The name is claimed under the home lock with an `op` intent
(`sandbox.create`, phases `claim`, `start`), the machine starts the daemon,
and the record is written once with the answer. For a PostgreSQL Store the
caller resolves the password and sends the URL only in this request; the
launching side checks that it reaches the server and holds the project
first. Nothing starts a stopped machine.

| Error | When |
| --- | --- |
| `InvalidInput` | `native`/`docker` as the name; an image Docker does not have |
| `NoSpace` | copying the image needs `2 × size × 1.05` bytes and the machine has fewer (`detail { need, free }`) |
| `Unavailable` | the machine is stopped (`detail { what: "machine" }`) or Docker cannot be read (`detail { what: "docker" }`) |
| `NotFound` | no machine of that name |
| `NoStore` | the project has no installed Store |
| `Exists` | the name is taken, or the machine already records a started Sandbox of it |
| `Unsupported` | the platform cannot run this Store kind; a sqlite Store on another machine; no image archive in the bundle; an image on macOS `native` (`detail { reason: "host" }`) or for another architecture (`detail { reason: "arch", value }`) |
| `Failed` | the start failed and every effect is known: the Sandbox was stopped and deleted (`detail { container, removed, log }`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `abandon`) |
| `Unknown` | the worker's answer was lost, the start's effect is unknown, or it started but could not be recorded: the record keeps its `op` with `uncertain: true` and `state` is `unresolved` |

### sandbox.list

The Sandboxes this home records, or one project's, from records alone.

```text
JS      await layerfs.sandbox.list(project?, { page_size, page_pos })
Python  layerfs.sandbox.list(project=None, *, page_size=None, page_pos=None)
CLI     layerfs sandbox list [<project>] [--page-size <page-size>] [--page-pos <page-pos>]
```

`page_size` is clamped to 1..=1000 (default 50); a negative `page_pos`
counts as 0 ([`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs) `page`).
Each item is the record (`View::List`). No machine, worker or daemon is
asked, so `state` is `running` for any started Sandbox even when its machine
is down, and never `lost`; `sandbox.get` asks
([`list.rs`](../../../../control/crates/layerfs-sandbox/src/list.rs)).

### sandbox.get

One Sandbox's record, its live state and one page of the Workspaces its
daemon holds, starting nothing.

```text
JS      await layerfs.sandbox.get(name, { page_size, page_pos })
Python  layerfs.sandbox.get(name, *, page_size=None, page_pos=None)
CLI     layerfs sandbox get <name> [--page-size <page-size>] [--page-pos <page-pos>]
```

```text
{ ...record, state,
  workspaces: [ { ws, name, path, workspace, namespace, branch, base,
                  base_layer, root, activity, mounted, directory,
                  head_moved_since_mount, dirty?, pending_merge, revision } ],
  next_page_pos, down_reason?, down?, daemon_capacity? }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `state` | derived with the machine's answer: `lost` when the record has an id, no close intent, and the machine runs no such Sandbox (`sandbox.close` clears it) | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `workspaces[].ws` | `<sandbox>:<name>` when the mount was named, else `<sandbox>:<path>`; the rest is the daemon's read model | [`get.rs`](../../../../control/crates/layerfs-sandbox/src/get.rs), [`operator/inspect.rs`](../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) |
| `down_reason` | the worker or forwarded daemon was not reached, or the daemon's control pool had no vacant slot; `workspaces` is then `[]` | `get.rs` |
| `down` | partial counts from the daemon's pre-handshake capacity notice: `code`, `resource`, `pool_vacant`, `reserved_vacant`, `retained`; absent when the refusal carried none | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) `unanswered` |
| `daemon_capacity` | the daemon's `backend.daemon.connection_failures` read model, read once, best effort, within 2 s on the connection the Workspace list lent; absent when not read | [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs), [`control_capacity.rs`](../../../../engine/crates/layerfs-bridge/src/control_capacity.rs) |

A machine that is down is not an error: it is `down_reason`. A Sandbox with
an uncertain `op` is shown, not refused; `get` is the call `Unresolved`
refusals name. Errors: `NoSuchSandbox`; `Busy` or `StaleSandbox` (a
forwarded daemon admits no connection in time, or a saved daemon is not
ready, [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs));
`Capacity` other than the pool's own notice; `Failed` (the daemon's list
failed); `Unknown` (the worker's answer was lost; no effect for a read).

### sandbox.close

Stops a Sandbox and frees its name; also the call that clears an uncertain
`sandbox.create` or `sandbox.close`.

```text
JS      await layerfs.sandbox.close(name, { force })
Python  layerfs.sandbox.close(name, *, force=None)
CLI     layerfs sandbox close <name> [--force]
```

`force` stops it even with Workspaces mounted or commands running: each
running command is cancelled first (5000 ms grace), and uncommitted changes
and processes end with the Sandbox
([`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) `settle`).

| Case | Answer |
| --- | --- |
| a started Sandbox, stopped | `{ kind: "Closed", sandbox, unknown: [path], closed }`; `unknown` lists, with `force`, the Workspaces still mounted, whose state is not confirmed |
| the machine says it is already gone | `{ closed, was_gone: true }` |
| a record of the retired docker backend | `{ closed, forgotten: true, container, remove: "docker rm -f <container>", left? }`: forgotten, not stopped |
| a record with no id (never started, or create's outcome unknown) | `{ closed, was: "gone" \| "unresolved", left? }` |

The shapes are built in
[`sandbox/close.rs`](../../../../api/crates/layerfs-api/src/sandbox/close.rs).
The intent `sandbox.close` (phase `stop`) is written before the runtime is
stopped (5 s), then deleted, each once. On `Closed` or an already-gone
Sandbox the home record is removed; on `Unknown` it is kept with its `op`
marked uncertain; on any other refusal the `op` is dropped and the Sandbox
is as it was ([`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs)).

| Error | When |
| --- | --- |
| `Busy` | another live operation holds the Sandbox; an unmount is in progress; without `force`, commands still run (`detail.execs`) |
| `MountsActive` | without `force`, Workspaces are mounted (`detail.workspaces`) ([`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs)) |
| `Failed` | the stop or delete failed; a failed stop is not followed by a delete |
| `Unknown` | the machine's answer was lost: the Sandbox may be stopped or not; or with `force` a command's cancel was not confirmed (`detail.execs`) |

## Family errors

`InvalidInput` (a bad name, a folder scope), `NoSuchStore`,
`NoSuchProject`, `NoSuchSandbox`
([`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs)),
`Busy` or `Unresolved` (an operation holds the Sandbox or its Store),
`Unavailable` (the worker is not recorded or does not answer, a lock is held
too long, a PostgreSQL server is not reached,
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)). New
work on a removed platform is `Unsupported` before the home is touched
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).
After `Unknown` from `create` or `close`, a line goes to
`log/uncertain.jsonl` and later calls are `Unresolved`, naming
`sandbox get` (shows it) and `sandbox close` (clears it).

## Related

- [workspace family](workspace.md), [machine family](machine.md).
- [Sandboxes](../../control/sandboxes/README.md) and [backends](../../control/sandboxes/backends.md).
- [Errors and events](../calls/errors-and-events.md).
