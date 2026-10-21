# Tier 2: remote Rust worker and node link

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

## Scope

Tier 1's plugin runs in a long-lived Rust worker on the Linux machine that
holds the Store volume. The main rutis host links to it with
`rutis-bridge/peer`; the consumer does not own the worker process.

Tier 2 changes deployment only. The service contract is Tier 1's; no new
LayerFS product behavior and no new core change.

## Topology

```text
Main rutis host (any machine)
  └─ peer row: rutis-bridge/peer
       │  import layerfs; dial or accept
       ▼
Linux worker: layerfs-rutis-worker (Rust rutis host)
  ├─ layerfs-rutis plugin (Tier 1)
  ├─ layerfs-session
  │    ├─ StoreService (direct)
  │    └─ Sandbox/Workspace session
  └─ Store volume, Docker socket, daemon executable
```

Rust has no `runtimes.remote` language entry. The worker is a second rutis
host, not a `<runtime>:<plugin>` row. The main host may be a stock
`rutis-host`, a Rust host or another rutis node; only the worker must embed the
Rust plugin.

## Worker

`layerfs-rutis-worker` is a small Rust host binary:

- embeds the Tier 1 plugin;
- starts a rutis peer listener or dialer (`rutis-bridge`, `rutis-loader`);
- exports the `layerfs` service (and only that service by default);
- owns the project's Store, Sandboxes and Workspaces;
- exposes a health/readiness signal through rutis row state, not a custom
  HTTP endpoint.

One worker per project is the recommended shape: store path, volume, Docker
prefix and authorization are unambiguous. One worker per LayerFS home with
project-scoped services is a later option; it needs service isolation and a
per-project authorization design.

## Main-host configuration

```json
{
  "id": "layerfs-store-1",
  "name": "rutis-bridge/peer",
  "config": {
    "peer": "layerfs-store-1",
    "dial": "wss://store-1.internal:7443/rutis",
    "import": ["layerfs"]
  }
}
```

The worker side exports `layerfs`; the main host imports it. A consumer row
uses `inject: ["layerfs"]` and starts when the service appears.

The worker config carries:

```text
project, home, store_path, store_policy, trace
deploy.docker, deploy.image, deploy.volume, deploy.daemon,
deploy.daemon_host, deploy.container_prefix
peer id, listen/dial address, token, certificate/key
```

Tokens and TLS material are deployment credentials. They are not passed in
service arguments and never appear in logs or events.

## Service surface

Same methods as Tier 1. Split the admin surface:

| Service | Methods | Export |
| --- | --- | --- |
| `layerfs` | `info`, `project_list_branches`, `fs_*`, `branch_*`, `workspace_*` | allowed for trusted consumers |
| `layerfs.admin` | `project_init`, `call` | not exported by default |

`project_init` and `call` are admin operations: Init creates a Store, and
`call` reaches the full Store table. A normal consumer should not receive
them. If a deployment needs remote Init, export `layerfs.admin` only to a
dedicated admin peer and require an explicit configuration flag.

## Contract gate

The worker is the process that opens the Store, so the literal "SDK's own
process opens the Store" is satisfied. The open question is the LayerFS rule
against a host/server adapter in the data path:

- **Linux:** technically possible; product permission is an owner decision.
  The worker must not be treated as allowed merely because rutis can link it.
- **macOS:** excluded. Only dsh Live may route Store calls through a Store
  container; a general rutis worker is not covered.
- **If not allowed:** Tier 2 is not shipped. Tier 0/1 remain Rust-only and
  host-local.

## Lifecycle and failure

- Worker starts, opens the Store, provides `layerfs`; the main host's consumer
  rows start when the service is visible.
- Link down: the service is withdrawn; calls in flight fail once with the
  transport error. No replay, no guessed success.
- Link up: a new session provides the service; consumers restart according to
  rutis.
- Worker restart: a new plugin generation. Store and Workspaces persist;
  the shutdown policy decides whether clean Workspaces are unmounted. No
  implicit Commit.
- A call that was in flight when the worker died stays unknown to the caller;
  the caller must inspect before retrying. The worker does not replay.
- Health is the rutis row/link state plus a `layerfs.info` readiness call;
  no second health protocol.

## Performance and limits

- Every call crosses a link: serialization, network and backpressure.
- Same-host loopback is the fast case; remote links pay network latency.
- `fs.read` is already bounded by the SDK (2000 lines / 48 KiB per answer).
- `fs.write` content, `fs.apply` changes and `exec` output must be bounded or
  streamed. Tier 2 uses `workspace_exec_stream` callbacks or a bounded file
  route; never an unbounded value.
- The worker shares one Store, one overlay owner and one connection pool
  across all consumers. A slow consumer can affect the others; fair queueing
  or admission is a worker concern, not a LayerFS Store change.
- Link loss does not cancel a command; the command continues in the Sandbox.

## Security

- The worker has the Store volume and the Docker socket; treat it as a
  privileged service.
- Run it as a dedicated user; restrict the socket, the Store path and the
  LayerFS home.
- Use `wss://` for non-loopback links; `RUTIS_TOKEN` on the worker and
  `RUTIS_TOKEN_<PEER>` on the main host; verify the certificate.
- One worker per project limits the blast radius. Do not export
  `layerfs.admin` or `call` to ordinary consumers.
- The link token is transport authentication, not a LayerFS project grant.
  A per-project worker is the first authorization boundary.

## Deployment

- systemd service or container with the Store volume, Docker socket and
  daemon executable mounted at the same paths as the daemon expects.
- Pin the worker, plugin, Store schema and daemon protocol to the same
  release; no mixed-version Store service.
- Restart policy: restart the worker, let rutis reconnect; do not add an
  application-level retry loop.
- Upgrade: stop the worker after consumers are idle, replace the binary,
  start it; consumers see service withdrawal and return.
- Keep the link private (loopback, VPN or private network); do not expose the
  worker to the public internet.

## Tests and acceptance

- Two rutis hosts: service appears after worker start; consumer starts;
  disconnect withdraws it; reconnect re-provides it.
- A call in flight during disconnect fails once; no duplicate write and no
  replay.
- Worker restart: Workspaces remain; the configured shutdown policy runs;
  no implicit Commit.
- Token and TLS refusal: `auth-rejected`; no retry storm beyond rutis link
  backoff.
- Large output: streamed path works; no unbounded value.
- One worker per project: a second project uses a second worker; no shared
  Store or Workspace state.
- `rutis-host check` on the main host: peer imported, consumer dependency
  satisfied; the worker row is not a stock `rutis-host` row.
- No test exceeds 120 seconds; network tests are selected individually.

## Open decisions

1. **Owner gate:** is a Linux Store service allowed, or is Tier 2 forbidden?
2. **Worker granularity:** one per project (recommended) versus one per home.
3. **Admin surface:** is remote `project_init`/`call` needed, and for whom?
4. **Shutdown policy:** leave, unmount clean, or discard only with explicit
   confirmation.
5. **Streaming:** callback pages versus a bounded file route for `exec` and
   large writes.
6. **Dylib:** is hot replacement worth the pinned SDK/toolchain bundle, or is
   worker restart enough?
