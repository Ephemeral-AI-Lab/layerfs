# Rust rutis plugin: in-process and remote architectures

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

This document compares two Rust-first ways to run LayerFS as a rutis plugin.
It replaces the Node adapter as the primary proposal; the Node adapter remains
useful only as a stopgap for Node rows.

The product constraint that decides between them is the Store access contract
in [AGENTS.md](../../../AGENTS.md): SDK Store calls run in the SDK's own
process, which opens the Store directly. A process that runs the Rust session
layer is an SDK process. A process that only calls a service provided by
another process is not.

## Host compatibility first

| Host | In-process Rust plugin (A) | Remote Rust worker (B) |
| --- | --- | --- |
| A Rust application embedding `rutis` / `rutis-loader` | Yes | Yes, as a remote node |
| Stock `rutis-host` CLI | No: it loads Node, Bun, Python, Go and Swift, not Rust built-ins | Yes: the worker is a separate Rust host linked over a node link |
| `rutis-cli` with the `dylib-plugins` feature | Yes, as a dylib plugin | Yes, with the same dylib host |
| A Rust host using `rutis-dylib` | Yes, as a dylib plugin | Yes |

Rust has no `runtimes.remote` language entry. A remote Rust plugin is a
second rutis host (the worker) linked to the main host; it is not a
`<runtime>:<plugin>` row.

## Architecture A: in-process Rust built-in plugin

```text
Rust rutis host process
  ├─ layerfs-rutis plugin
  │    ├─ LayerFs service (typed)
  │    ├─ layerfs-session
  │    │    ├─ StoreService (direct, Linux)
  │    │    └─ Workspace/Sandbox session
  │    └─ per-project state and lanes
  └─ Rust consumer plugins (same process)
```

The plugin is linked into the host. Rust consumers call it through the rutis
typed registry (`ctx.require::<LayerFs>()`), so no serialization or IPC is on
the data path. The host process is the SDK process and opens the Store
directly.

If the service is also marked `#[rutis::service]`, Node, Python, Go or remote
rows can call it by name. Those calls cross a process or machine and therefore
become proxies; see **Contract gate** below. The default for A is Rust-only.

### A: components

- `layerfs-session` (new library, extracted from `sdk/js/native/src`): project
  discovery, Store open and call, Sandbox placement, Workspace mount/exec/
  status/commit/unmount, daemon connection pool. No napi types.
- `layerfs-rutis` (new integration crate): configuration, `LayerFs` service,
  typed methods, error mapping, lifecycle, lanes.
- Host wiring: a Rust host registers the plugin with its config. No Node
  runtime, no native addon, no JS SDK.

### A: service surface

Typed Rust methods, all async:

| Method | LayerFS operation |
| --- | --- |
| `info` | project, platform, Store path presence, capabilities |
| `call(method, args)` | exact Store table (`fs.*`, `branch.*`, `project.list_branches`) |
| `project_init(source)` | `ProjectApi::init` |
| `project_list_branches(page_size, page_pos)` | `project.list_branches` |
| `fs_read`, `fs_list`, `fs_stat`, `fs_grep`, `fs_glob`, `fs_diff_files`, `fs_blame`, `fs_write`, `fs_edit`, `fs_apply`, `fs_mkdir`, `fs_rm`, `fs_cp`, `fs_mv` | `fs.*` |
| `branch_fork`, `branch_list_commits`, `branch_diff_commits`, `branch_diff_branches`, `branch_merge`, `branch_publish` | `branch.*` |
| `workspace_mount`, `workspace_exec`, `workspace_status`, `workspace_commit`, `workspace_unmount` | Workspace session |

`#[rutis::service]` writes camelCase names for other languages
(`fs_read` → `fsRead`), matching the Node adapter's service vocabulary.

### A: lifecycle and ownership

- `apply` validates configuration, creates the project state and provides the
  service. The Store opens lazily so `project.init` can create it first.
- The host process owns the Store and every mounted Workspace. Cleanup closes
  the Store and drains lanes; it does not Commit and does not unmount.
- One plugin instance can hold several projects in one process. The session
  layer takes configuration directly, not process environment, so there is no
  one-project-per-process limit.
- Store and sandbox calls are synchronous in the LayerFS crates. Run them on
  `spawn_blocking` (or one actor per project if a type is not `Send + Sync`);
  never hold a mutex across `.await` or Store I/O.
- Per-Workspace calls run in order; different Workspaces run in parallel.
- Each call is attempted once. Refusals and unknown results keep their
  original code and data; no retry, replay or busy handler.

### A: when to choose

- The consumers are Rust plugins in the same host.
- Latency and throughput matter; no IPC is wanted.
- The host can be a custom Rust binary.
- You want to avoid the Store-proxy authorization question entirely.

Cost: the plugin is compiled into the host, so a plugin change rebuilds the
host; the stock `rutis-host` CLI cannot load it.

## Architecture B: remote Rust worker plus node link

```text
Main rutis host (any machine)
  └─ peer row (rutis-bridge/peer)
       │  dial or listen, export layerfs services
       ▼
Linux worker: Rust rutis host
  ├─ layerfs-rutis plugin (same crate as A)
  ├─ layerfs-session
  │    ├─ StoreService (direct, Linux)
  │    └─ Workspace/Sandbox session
  └─ Docker socket, Store volume, daemon executable
```

The worker is a long-lived Rust host on the Linux machine that holds the Store
volume. The main host does not own the plugin process; rutis manages the link,
withdraws the service when the link goes away and re-provides it after
reconnection.

### B: deployment shape

- One worker per project is the recommended first shape: the store path,
  Docker volume, Sandbox prefix and authorization scope are unambiguous.
- One worker per LayerFS home with several project services is possible, but
  it needs project-scoped service names or rutis scopes and a per-project
  authorization design. Do not start there.
- The worker mounts the Store volume and the Docker socket, and has the daemon
  executable path. It runs as a dedicated user; the Docker socket is root
  authority and must not be exposed to untrusted consumers.
- The main host imports only the services it needs. `layerfs.call` is an admin
  escape hatch and should not be exported to untrusted peers.

Example main-host row:

```json
{
  "id": "layerfs-store-1",
  "name": "rutis-bridge/peer",
  "config": {
    "peer": "layerfs-store-1",
    "dial": "wss://store-1.internal:7443/rutis",
    "export": ["layerfs"]
  }
}
```

The worker is started with the same rutis link settings on its side
(`--id`, `--peer`, token, certificate). The token and TLS files are deployment
credentials, not plugin configuration.

### B: lifecycle

- Worker starts the plugin, opens the Store and exports the service.
- A consumer row starts when the service appears and stops when it is
  withdrawn. A disconnect fails calls once; they are not replayed.
- Worker restart is a new plugin generation. Workspaces are not unmounted or
  committed by the restart; the explicit shutdown policy decides.
- A consumer's `ctx.signal` is not a LayerFS command cancellation. `exec`
  cancellation and background processes need their own design.

### B: failures and limits

- The worker is a single point for its project. It needs health reporting,
  restart policy, version lockstep with the Store schema and daemon protocol,
  and a clear "unknown" result when the link breaks mid-call.
- Every call crosses a link: page large results, stream `exec` output through a
  callback or a bounded file route, and keep `fs.write` content bounded or
  streamed. Do not put an unbounded stdout or file content into one service
  value.
- The worker holds the Store and all workspaces for its project. Its open-file
  state, overlay connections and memory budget are shared by every consumer;
  that is the cost of centralization.
- Authorization must be per project and per method. A link token alone is a
  transport credential, not a LayerFS project grant.

### B: when to choose

- Consumers are on other machines or in languages that cannot run the Rust
  session layer.
- The Store volume lives on one Linux machine and consumers must not mount it.
- The owner explicitly allows the Store-service topology on Linux.

macOS remains excluded unless the owner changes the dsh-only exception: the
current contract says nothing else may route Store calls through a worker.

## Contract gate

A is the default-compliant form for Rust consumers: the host process runs the
Rust session layer and opens the Store itself. The `#[rutis::service]` export
and B put the Store call in a process other than the consumer; rutis can carry
that call, but whether LayerFS permits it is a product decision, not a rutis
capability question.

- **Linux:** the literal requirement ("the SDK's own process opens the Store
  directly") is satisfied by the plugin or worker process. The remaining
  question is the prohibition on a host/server adapter in the data path. The
  owner must say whether a LayerFS rutis service is that adapter.
- **macOS:** explicitly excluded. Only dsh Live may route Store calls through
  the Store container; a general rutis worker is not covered by that exception.
- **If not allowed:** A stays Rust-only (no `#[rutis::service]` export), B is
  not shipped, and the Node adapter remains a stopgap rather than a product
  answer.

## Scope ladder: the first tier needs no LayerFS change

The large refactor is not a prerequisite for a Rust plugin. It is the cost of
reusing the existing Workspace/Sandbox placement logic instead of writing it
twice.

| Tier | Coverage | LayerFS changes | Where the work is |
| --- | --- | --- | --- |
| 0 | `project.init`, `project.list_branches`, `fs.*`, `branch.*` | None; use public `layerfs-api` and `layerfs-store-service` | `layerfs-rutis` only |
| 1 | Tier 0 plus `workspace.mount/exec/status/commit/unmount` | Reuse or extract the session logic; no product behavior change | `layerfs-session` plus the plugin |
| 2 | Tier 1 plus a remote Rust worker and node link | None beyond Tier 1 | worker host and link configuration |

Tier 0 is a real plugin, not a stub: `StoreService::open_direct`,
`layerfs_api::call`, `ProjectApi::init` and `ProjectApi::install_local` are
public. The plugin still has to read the project record, check the private
LayerFS home, and convert between `SdkValue` and the service values. That is
plugin code, not a change to the core crates.

Tier 1 is where the existing native binding has the placement and discovery
logic. Reusing it is cheaper and safer than a second implementation, but it is
a refactor of `sdk/js/native/src`, not a change to the Store, daemon, FUSE or
content products. The extracted crate can live under `sdk/` or a new
integration workspace; it does not have to enter `core/`. The earlier
`core/crates/layerfs-session` placement in this document was too strong.

If Tier 0 is enough for the first Rust plugin, ship it first. Do not take the
session refactor until Workspace support is actually required.

## Common prerequisite: extract `layerfs-session`

For the full Workspace surface (Tier 1), both A and B need the same Rust
session layer. The logic exists today in `sdk/js/native/src` but is
`pub(crate)` and napi-coupled at the top:

- project records and discovery;
- Store open/call registry;
- Sandbox placement, start, reclaim and connection pool;
- Workspace mount/status/commit/unmount records;
- exec spool and output;
- Init request/counters.

Extract that logic into a napi-free crate, for example under `sdk/` or a
small integration workspace; it does not have to enter `core/`. The Node
binding becomes a thin napi wrapper. The rutis plugin depends on the same
crate, so there is one deployment and placement implementation, not a second
one.

This is a relocation, not a rewrite. The production LOC total should stay
close to unchanged; the commit must report the migration and keep the
`layerfs-node` behavior identical. The native binding's JavaScript conversion
(`convert.rs`) stays in the napi crate; the session crate exposes `SdkValue`
or a small serde conversion.

When Tier 1 is needed, the extraction should be its own PR before the
Workspace PR. Tier 0 does not wait for it.

## Shared service contract

Both architectures expose the same logical service. The implementation
differs, not the names.

- `layerfs` is the typed service. `layerfs.call` is the exact Store-method
  escape hatch.
- `project_init` and the Workspace methods are not `call`; `call` opens the
  Store first and cannot install a missing Store.
- Store methods keep their SDK argument objects and result shapes.
- `LayerFsError` carries a stable LayerFS code, a message and structured data.
  For Rust consumers this is a typed enum. For cross-language
  `#[rutis::service]`, exact codes need either a hand-written `HostDispatch`
  with `rutis_bridge::exports::failure` or a rutis error-code extension;
  otherwise `code` is the error category and `data.code` holds the exact
  LayerFS code. Deciding this is required before B is useful across languages.
- No implicit Commit, unmount, retry, refresh or replay.
- One Workspace's calls are sequential; different Workspaces are parallel.
- The operator surface is not part of the first service. It can become a
  separate, explicitly enabled service for management and diagnostics.

## Store and ownership model

| | A: in-process | B: remote worker |
| --- | --- | --- |
| Store opener | host process | worker process |
| Consumer | same process (Rust) | another process/machine |
| Contract | direct, compliant | needs owner decision on the Store-service topology |
| macOS | host must be Linux | worker must be Linux; dsh-only exception still applies |
| Workspace owner | host | worker |
| Shutdown | caller unmounts; plugin closes Store | worker policy decides; default leave and report |

The macOS dsh worker is the existing example of B, but it is explicitly
limited to dsh Live. B is not a general permission; it is the same topology
with a different authorization scope.

## Decision matrix

| Requirement | A | B |
| --- | --- | --- |
| Rust consumers, same host | Best | Unnecessary hop |
| Consumers on other machines | Not possible without a link | Best |
| Consumers in Node/Python/Go | Proxy only; use with contract review | Natural, but owner gate |
| No custom Rust host | No | Main host can stay `rutis-host` |
| Lowest latency | Best | Link and serialization cost |
| Hot plugin replacement | Rebuild host, or dylib variant | Restart worker; link reconnects |
| One project per process | No; multi-project allowed | Recommended one worker per project |
| Operational surface | Small | Worker, link, TLS, health, restart |

## Implementation phases

1. **Tier 0: Rust Store-only plugin.** Add `layerfs-rutis` with `info`,
   `call`, `project_init`, `project_list_branches`, `fs.*` and `branch.*`.
   No LayerFS core changes. Test against a temporary Store and the existing
   native fixtures.
2. **Decide whether Workspace support is required.** If not, stop here; the
   plugin is a complete Rust Store service. If yes, continue.
3. **Tier 1: extract `layerfs-session`.** No behavior change; native binding
   tests stay green. Report relocation and production LOC. This is a separate
   PR from the plugin.
4. **Tier 1: add Workspace methods (A).** Reuse the session crate; add
   per-Workspace lanes; test mount/exec/status/commit/unmount against the real
   Linux deployment.
5. **Expose `#[rutis::service]` only if needed.** Decide Rust-only versus
   cross-language. If cross-language, settle the error-code mapping and the
   contract review first.
6. **Tier 2: add the remote worker (B).** Reuse the A plugin in a Rust host
   binary, configure the node link, and test disconnect/reconnect, worker
   restart, auth failures and bounded output.
7. **Optional dylib variant.** Build the same plugin with `rutis-sdk` and the
   dylib toolchain only if hot replacement without rebuilding the host is a
   requirement.

## Verification

- Session extraction: existing native binding tests, unchanged behavior,
  no napi import in the session crate.
- A: temp Store tests for every Store method; real Docker test for one
  Workspace lifecycle; host reload; multi-project clients; no retry on refusal.
- B: two rutis hosts over a link; service appears/disappears; a call in flight
  when the link drops fails once; worker restart; token/TLS refusal; large
  output streaming; no automatic replay.
- Both: exact code/data on refusals; no implicit Commit or unmount; per-
  Workspace ordering; Store closed on unload.

## Open decisions

1. **Store topology.** Does the owner allow B's worker to serve Store calls on
   Linux? If not, only A is permitted, and A must stay Rust-only or same-process.
2. **Error codes.** Exact `NoSuchPath`/`Conflict`/`Dirty` codes through
   `#[rutis::service]` need a decision: hand-written `HostDispatch` or a rutis
   extension.
3. **Worker granularity.** One worker per project (recommended) or one per
   LayerFS home with project-scoped services.
4. **Shutdown policy.** On worker unload: leave Workspaces, unmount clean ones,
   or discard only with explicit confirmation.
5. **Cross-language surface.** Is the service Rust-only, or do Node/Python/Go
   rows need it? The second answer makes the Store-proxy decision mandatory.
6. **Dylib.** Is hot replacement worth the pinned compiler, SDK bundle and
   packaging pipeline?

## Relationship to dsh

dsh is a JavaScript host, so it puts the SDK in each agent's bash process on
Linux and uses a Linux Store container only on macOS under an explicit owner
exception. A is the Rust equivalent of the Linux side: the caller process owns
the Store. B is the Rust equivalent of the macOS worker, generalized. That
generalization is exactly what needs owner approval.

The earlier Node adapter is a stopgap for Node rows; it is not the Rust plan
and should not be grown into the product answer.
