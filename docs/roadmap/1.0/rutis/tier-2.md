# Tier 2: remote worker

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

## Delta

Tier 1's plugin runs in a long-lived Rust worker on the Linux machine holding
the Store volume. The main host links to it with `rutis-bridge/peer`.

No new service methods, no new LayerFS behavior. The worker is the Store and
Workspace owner; the consumer does not own the process.

## Topology

```text
Main rutis host
  └─ rutis-bridge/peer (import layerfs)
       ▼
Linux worker: layerfs-rutis-worker
  ├─ Tier 1 plugin
  ├─ layerfs-session
  └─ Store volume, Docker socket, daemon
```

Rust has no `runtimes.remote` language entry; the worker is a second rutis
host, not a `<runtime>:<plugin>` row. The main host may be a stock
`rutis-host`; the worker must be a Rust host that embeds the plugin.

## Configuration

Main host:

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

Worker: Tier 1 config plus peer id, listen/dial address, token and TLS
certificate/key. Tokens are transport credentials, not service arguments.

One worker per project. Multi-project workers are out of scope.

## Contract gate

The worker opens the Store directly, but it serves other processes. The
current LayerFS contract forbids a host/server adapter in the data path and
allows the macOS worker only for dsh Live.

- **Linux:** owner must explicitly allow the Store-service topology.
- **macOS:** not allowed.
- **If not allowed:** Tier 2 is not shipped; Tier 0/1 remain Rust-only.

## Lifecycle and failure

- Worker starts, opens the Store, exports `layerfs`; consumer rows start when
  the service appears.
- Link down: service withdrawn; in-flight calls fail once; no replay.
- Worker restart: new generation; Store and Workspaces persist; Workspaces are
  left mounted. No implicit Commit or unmount.
- An in-flight call whose link broke stays unknown to the caller; inspect
  before retrying.

## Security

- The worker has the Store volume and Docker socket; run it as a dedicated
  user and keep it on a private network.
- Use `wss://` with `RUTIS_TOKEN` / `RUTIS_TOKEN_<PEER>` for non-loopback
  links.
- Export only to a trusted peer. `layerfs.call` is part of the service and
  grants the full Store table; do not export it to ordinary consumers.
- One worker per project is the authorization boundary.

## Deployment

- systemd or container with the Store volume, Docker socket and daemon at the
  paths the daemon expects.
- Pin worker, plugin, Store schema and daemon protocol to one release.
- Upgrade by stopping the worker, replacing the binary and starting it;
  rutis reconnects. No application-level retry loop.

## Tests and acceptance

- Two rutis hosts: service appears, consumer starts, disconnect withdraws it,
  reconnect re-provides it.
- In-flight call during disconnect fails once; no duplicate write.
- Worker restart leaves Workspaces mounted; no implicit Commit.
- Token/TLS refusal is `auth-rejected`; no extra retry storm.
- Large output uses the Tier 1 sink path.
- `rutis-host check` on the main host sees the imported service.

## Open decisions

1. Is the Linux Store-service topology allowed by the owner?
