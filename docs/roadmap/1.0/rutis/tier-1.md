# Tier 1: add Workspace support

> **Status:** Proposal with an implemented, unreleased Tier 1; target LayerFS
> 1.0.0; not a released contract.

## Delta

Tier 0 plus five methods:

| Method | Result |
| --- | --- |
| `workspace_mount({ branch })` | `{ sandbox, path, image }` |
| `workspace_exec({ sandbox, path, command, message?, sink? })` | exit status and streams; `sink` receives pages |
| `workspace_status({ sandbox, path })` | the SDK status value |
| `workspace_commit({ sandbox, path, message?, force? })` | the SDK Commit or Merge result |
| `workspace_unmount({ sandbox, path, discard? })` | the SDK unmount result |

One `exec` method, optional sink. No separate stream method. Still Rust-only,
still in the host process.

## Why this needs a session layer

Tier 0 only opens a Store. Workspace support also needs:

- Sandbox placement (lowest running Sandbox with a free slot, else start the
  next; capacity 16);
- Sandbox creation, authentication, records and reclaim;
- Workspace mount/status/commit/unmount records;
- daemon connection pool;
- exec spool/output.

That logic lived only in `sdk/js/native/src`. Reusing it needs a napi-free
`layerfs-session` crate. Writing it again in the plugin is the second
implementation this design avoids.

## Session extraction

Move the native binding's discovery, sandbox, workspace, pool, exec and
project modules into `layerfs-session`; `layerfs-node` becomes a thin napi
wrapper. `convert.rs` stays in the napi layer. The crate can live under
`sdk/session` or an integration workspace; it does not have to enter `core/`.

Implemented as `sdk/session`, a member of the core workspace. It carries
discovery, Sandboxes, Workspaces, the control-connection pool, exec, output
spooling, direct Store calls, tracing and the two method tables. `layerfs-node`
keeps only `convert.rs`, the napi `Job` and the public entry points. The move
is behavior-preserving: the discovery/reclaim tests moved with the modules and
the JavaScript packaging, exec-output and trace checks still pass.

## Configuration

Add:

```text
deploy.docker          Docker Engine socket
deploy.image           exact local image digest sha256:<64 hex>
deploy.volume          named volume holding the Store
deploy.daemon          host path to the LayerFS daemon
deploy.daemon_host     optional loopback host for the daemon
deploy.container_prefix optional Sandbox name prefix
```

Validate the digest, volume, daemon path and Docker socket at `apply`. Missing
deployment leaves Store calls working and makes `workspace_*` fail
`InvalidInput`. The Rust configuration is `layerfs_rutis::Deploy`; the exact
digest is lowercase `sha256:<64 hex>`, all paths must be absolute, the Engine
path must be an existing socket and the daemon path an existing file.

## Lifecycle

- `mount` reads the Branch id in this process's Store, places and binds the
  Workspace, records it, returns `{ sandbox, path }`.
- `status` asks the daemon and reads the Branch head in this process.
- `commit` sends one Commit; a moved head is merged by the daemon; a conflict
  is returned, not retried.
- `unmount` is explicit. A dirty Workspace refuses unless `discard: true`.
- One lane per `{ sandbox, path }` for `exec`, `status`, `commit` and
  `unmount`; different Workspaces run in parallel.
- Unload closes the Store and leaves Workspaces. No implicit Commit, unmount
  or discard. No configurable shutdown policy in Tier 1.
- The plugin tracks mounted `{ sandbox, path, branch }` for diagnostics only.

## Exec

- `workspace_exec` with no sink returns the SDK result for bounded output. If
  a stream exceeds the SDK's inline window, the call releases the spool and
  fails `InvalidInput`; Tier 1 exposes no page method, so large output must
  use a sink.
- A Rust rutis method cannot carry a callback. The implemented sink is
  `{ stdout?: <absolute path>, stderr?: <absolute path> }`; both paths must be
  different absolute files. They are opened (`O_NOFOLLOW`, 0600, truncated)
  before the command runs, so a bad sink fails first. Each page is written in
  order after the command ends, and the result names both files and their byte
  counts. `workspace_exec` is not a streaming callback API.
- The sink does not apply backpressure to the command: output is spooled to a
  temporary file while the command runs, then copied to the sink files. The
  spool is bounded by free space, not by the sink's speed.
- `ctx.signal` is not command cancellation. Background processes and
  cancellation are out of scope.

## Failure

- Mount failure keeps its acknowledged binding and original failure; no
  implicit unmount or retry.
- Conflict, dirty unmount and unknown outcomes keep their exact code/data.
- A lost host is not proof that a command or descendant ended.

## Security

- Linux host with Docker socket and Store volume; run as a dedicated user.
- Docker socket is root-equivalent; do not export this service to untrusted
  rows.
- No command output, file content or messages in logs.

## Tests and acceptance

- Tier 0 suite unchanged.
- Session extraction: native tests unchanged; no napi import in the crate.
- Real Linux + Docker: mount, exec, status, commit, unmount; conflict; dirty
  unmount; large output through a sink.
- Placement: two Workspaces share a Sandbox; the 17th starts the next; a
  stopped Sandbox is reclaimed once.
- Reload: mounted Workspaces remain; a new plugin can list and unmount them.
- No test over 120 seconds; Docker tests selected individually.

Run at this implementation: Tier 0 (15 tests), Tier 1 without Docker (5
tests: deployment validation, missing deployment, unknown Sandbox, sink
argument validation, private-project enforcement), `layerfs-session` (6 moved
tests), JavaScript `packaging/lazy`, `packaging/dispatch`,
`test/exec_output.mjs` and `test/trace.mjs off`. The live Docker acceptance
items below are **NOT_RUN**; no LayerFS image is installed on this host.

## Risks

| Risk | Mitigation |
| --- | --- |
| Extraction changes behavior | Move without edits; native tests first |
| Session types not `Send + Sync` | Compile-time assertion; per-project actor |
| Unbounded exec output | Use the sink path; document the no-sink bound |
| Workspace leak on reload | Track mounts; leave + report |
| Two placement implementations | One shared session crate |
