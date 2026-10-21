# Tier 1: add Workspace support

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

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

That logic exists only in `sdk/js/native/src`. Reusing it needs a napi-free
`layerfs-session` crate. Writing it again in the plugin is the second
implementation this design avoids.

## Session extraction

Move the native binding's discovery, sandbox, workspace, pool, exec and
project modules into `layerfs-session`; `layerfs-node` becomes a thin napi
wrapper. `convert.rs` stays in the napi layer. The crate can live under
`sdk/session` or an integration workspace; it does not have to enter `core/`.

The move is behavior-preserving. Run the native tests before and after; one
placement implementation must serve both bindings.

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
`InvalidInput`.

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

- `workspace_exec` with no sink returns the SDK result for bounded output.
- With a sink, raw pages are delivered in order and the result carries the
  byte counts; a slow sink applies backpressure.
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

## Risks

| Risk | Mitigation |
| --- | --- |
| Extraction changes behavior | Move without edits; native tests first |
| Session types not `Send + Sync` | Compile-time assertion; per-project actor |
| Unbounded exec output | Use the sink path; document the no-sink bound |
| Workspace leak on reload | Track mounts; leave + report |
| Two placement implementations | One shared session crate |
