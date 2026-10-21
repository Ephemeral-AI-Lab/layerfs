# Tier 1: Rust plugin with Workspace support

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

## Scope

Tier 0 plus the full Workspace surface:

- `workspace.mount`
- `workspace.exec`
- `workspace.status`
- `workspace.commit`
- `workspace.unmount`

The plugin still runs in the rutis host process. The difference from Tier 0 is
that it must also own Sandbox placement, Workspace records and daemon
connections. Those exist today only in `sdk/js/native/src`.

## Topology

```text
Rust rutis host process
  └─ layerfs-rutis plugin
       ├─ layerfs-session
       │    ├─ StoreService (direct)
       │    ├─ Sandbox placement / records
       │    ├─ Workspace mount / commit / unmount
       │    └─ daemon connection pool
       └─ workspace lanes
```

The host process remains the Store and Workspace owner. No Node, no remote
worker, no Store proxy.

## Session layer

Create one napi-free `layerfs-session` crate, outside `core/` if preferred.
It owns the orchestration currently spread across the native binding's
discovery, sandbox, workspace, pool, exec and project modules:

| Responsibility | Output |
| --- | --- |
| Project discovery | project record, manifest, installed manifest |
| Store open/call | `StoreService` per project; `layerfs_api::call` |
| Sandbox placement | choose a running Sandbox of the image with a free slot, else start the next |
| Sandbox lifecycle | create, authenticate, record, reclaim stopped containers |
| Workspace lifecycle | mount, status, commit, unmount; per-Sandbox records |
| Daemon pool | bounded control connections; one call per connection |
| Exec | command launch, spool/sink output, exit status |
| Init | `ProjectApi` + `install_local` + project record |

`layerfs-node` becomes a thin napi wrapper over this crate. The crate exposes
`SdkValue`-based calls; the napi layer keeps the JS conversion. There must be
one placement implementation, not a Rust copy and a Node copy.

This is a relocation, not a behavior change. It can live under `sdk/session`
or an integration workspace; it does not have to enter `core/`. The core
crates keep their 1.85 floor; the rutis plugin crate uses 1.88.

## Service surface

Tier 0 methods remain. Add typed async methods:

| Method | Arguments | Result |
| --- | --- | --- |
| `workspace_mount` | `{ branch }` | `{ sandbox, path, image }` |
| `workspace_exec` | `{ sandbox, path, command, message? }` | `{ kind, exitCode, stdout, stderr, message, truncated }` |
| `workspace_exec_stream` | same plus `on_stdout`, `on_stderr` callbacks | same without inline output |
| `workspace_status` | `{ sandbox, path }` | the SDK status value |
| `workspace_commit` | `{ sandbox, path, message?, force? }` | the SDK Commit or Merge result |
| `workspace_unmount` | `{ sandbox, path, discard? }` | the SDK unmount result |

`workspace.exec` has no Writable sinks at the service boundary. In Tier 1
Rust-only, use `workspace_exec_stream` with in-process callbacks for large
output; `workspace_exec` is the convenience path for bounded output. Tier 2
must not turn `workspace_exec` into an unbounded transfer.

`info` reports `workspace: true` and whether deployment is configured.

## Configuration

Tier 0 config plus:

```text
deploy.docker          Docker Engine socket
deploy.image           exact local image digest sha256:<64 hex>
deploy.volume          named volume holding the Store
deploy.daemon          host path to the LayerFS daemon executable
deploy.daemon_host     optional host for a daemon published on loopback
deploy.container_prefix optional Sandbox name prefix
```

Validation:

- image is an immutable digest, not a tag;
- volume and daemon path are non-empty;
- Docker socket exists and is reachable;
- Store parent directory is writable by the host user;
- the project has an installed manifest before a mount;
- no Durable profile.

Missing deployment makes `workspace_*` fail with `InvalidInput`; Store-only
calls still work. `info` says which capability is missing.

## Workspace lifecycle

- `mount` reads the Branch id in this process's Store, places the Workspace in
  a Sandbox of the requested image, binds it, records it, and returns
  `{ sandbox, path }`.
- Placement uses the existing rule: lowest-numbered running Sandbox with a
  free slot; start the next number when all are full; capacity 16.
- A stopped or missing container is reclaimed under the project guard. One
  attempt; no replay.
- `status` asks the daemon and reads the Branch head in this process.
- `commit` sends one Commit to the daemon; a moved head is merged by the
  daemon's publication path; a conflict is returned, not retried.
- `unmount` is explicit. A dirty Workspace refuses unless `discard: true`.
- The plugin tracks mounted `{ sandbox, path, branch, mountedAt }` for
  diagnostics. Unload does not unmount; the default shutdown policy is
  `leave`. An optional policy can unmount clean Workspaces or discard only
  with explicit configuration.
- No implicit Commit, no automatic unmount, no background process management.

## Concurrency

- One lane per `{ sandbox, path }` for `exec`, `status`, `commit` and
  `unmount`; lanes preserve call order and reject nothing.
- `mount` is not lanned; the session's project placement guard serializes it.
- Store calls remain direct; do not hold a lane while waiting for a Store
  call when the same workspace lane is not involved.
- A lane is a promise chain, not a thread. A long `exec` blocks later calls on
  that Workspace only.
- Do not hold a mutex across `.await`; use `tokio::sync` for async state and
  short `std::sync` locks for maps.

## Exec

- Launch the command in the Workspace's Sandbox; the SDK owns the command
  lifecycle, not the filesystem daemon.
- `workspace_exec` collects bounded output for convenience. A caller that
  expects large output uses `workspace_exec_stream`.
- Callbacks receive byte pages in order; a slow callback applies backpressure.
- `ctx.signal` is not command cancellation. Cancellation and background
  processes are out of scope; they need the operator/executor design.
- Exit status and streams are returned exactly; no truncation flag is invented.

## Failure and ownership

- Mount failure keeps the original failure and any acknowledged binding; no
  implicit unmount or retry.
- Commit conflict, dirty unmount and unknown outcomes keep their exact codes
  and data.
- A lost link or host process is not proof that a command or descendant ended.
- A Workspace created by the plugin is caller-owned. If the caller crashes,
  the Workspace remains until explicit unmount or Sandbox idle close.
- Cleanup closes the Store and removes its overlay template; it does not
  delete global history or Workspaces.

## Security

- Linux host with the Docker socket and Store volume.
- Run the host as a dedicated user; the Docker socket is root-equivalent.
- Restrict the LayerFS home and Store path permissions.
- Tier 1 is host-local; do not export the service to untrusted rows.
- Do not log command output, file content or messages beyond the optional call
  trace.

## Tests and acceptance

- Session extraction: native binding tests unchanged; no napi import in the
  session crate; Node binding and Rust plugin produce the same result for a
  fixture call.
- Store: Tier 0 suite passes unchanged.
- Workspace: real Linux + Docker test for mount, exec, status, commit,
  unmount; conflict; dirty unmount; large output through the stream path.
- Placement: two Workspaces share a Sandbox; the 17th starts the next
  Sandbox; a stopped Sandbox is reclaimed once.
- Reload: plugin unload leaves mounted Workspaces; a new plugin can list and
  unmount them.
- No test exceeds 120 seconds; long Docker tests are selected individually.

## Risks

| Risk | Mitigation |
| --- | --- |
| Session extraction changes behavior | Move code without edits; run native tests first |
| `StoreService`/session types are not `Send + Sync` | Compile-time assertion; per-project actor if needed |
| Exec output is unbounded | Stream path first; collect only with a declared bound |
| Workspace leaks on reload | Track mounts, default leave + report; add shutdown policy |
| Two placement implementations | One session crate shared by Node and Rust |
| Method table drifts | Generate/assert against the public SDK table |
