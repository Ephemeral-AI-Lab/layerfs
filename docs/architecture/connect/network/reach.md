# Reach

[Architecture](../../README.md) › [Connect](../README.md) › [Network](README.md) › Reach

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a Sandbox daemon reaches the caller's PostgreSQL server.

| | |
| --- | --- |
| Component | [Network](README.md) |
| Source | [`connect/crates/layerfs-machine/src/reach.rs`](../../../../connect/crates/layerfs-machine/src/reach.rs), [`connect/crates/layerfs-sandbox/src/postgres_url.rs`](../../../../connect/crates/layerfs-sandbox/src/postgres_url.rs), each platform's `adapter.rs` under [`platforms/`](../../../../connect/crates/layerfs-machine/src/platforms) |

A PostgreSQL Store's URL is written for the caller's host. A daemon on a
machine may see the caller's loopback under another name, or not at all.
`machine::reach(spec, url)` answers the URL that machine's daemon dials. It
starts nothing.

## URL rewrite per platform

```text
 reach(spec, url) --> adapter(spec.kind).reach --> dial(spec, url, Loopback)

 dial:
   Shared? ------------------------------------------> url as given
   parse: one postgres[ql]://[user@]host[:port]/... ---no--> InvalidInput
   ?host= or ?hostaddr= present ----------------------yes--> Unsupported
   host empty (a Unix socket) ------------------------yes--> Unsupported
   host not localhost / 127.0.0.1 / ::1 -------------yes--> url as given
   Refused(why) -------------------------------------yes--> Unsupported(why)
   sslmode=verify-full ------------------------------yes--> Unsupported
   Alias(a) | Lookup(read) --> with_host(a) : every other byte kept
```

| Platform | `Loopback` | Loopback host becomes | Why | Source |
| --- | --- | --- | --- | --- |
| `native` | `Shared` | unchanged (the URL is not parsed) | The daemon shares the caller's loopback. | [`native/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/native/adapter.rs) |
| `docker` | `Alias` | `host.docker.internal` | Docker Desktop's name; on Linux each Sandbox container gets `host.docker.internal:host-gateway`. | [`docker/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/docker/adapter.rs), [`backend/docker/create.rs`](../../../../connect/crates/layerfs-sandbox/src/backend/docker/create.rs) |
| `lima` | `Alias` | `192.168.5.2` | The gateway of Lima's user-mode network. The address, not `host.lima.internal`: the Sandbox root has no such hosts entry. | [`lima/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/lima/adapter.rs) |
| `firecracker` | `Alias` | `192.168.5.2` | The Lima host's own host address; the Lima host masquerades the microVM's traffic. | [`firecracker/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/firecracker/adapter.rs) |
| `microsandbox` | `Alias` | `host.microsandbox.internal` | The guest's per-sandbox gateway; the machine is created with `--net public,private,host` so host egress is allowed. | [`microsandbox/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/microsandbox/adapter.rs), [`microsandbox/create.rs`](../../../../connect/crates/layerfs-machine/src/platforms/microsandbox/create.rs) |
| `apple-container` | `Alias`, else `Lookup` | the gateway its last start recorded (`Lasting::gateway`, #66), else `status.networks[0].ipv4Gateway` from `container inspect` | No host alias. Each start reads the gateway once and the machine record keeps it, so a reach runs no subprocess; a record without one (from before #66) reads it while the machine runs, only for a loopback URL. A stale recorded gateway is dialled as recorded and fails there; it is not looked up again. | [`apple_container/power.rs`](../../../../connect/crates/layerfs-machine/src/platforms/apple_container/power.rs), [`apple_container/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/apple_container/adapter.rs) |
| `ssh` | `Refused` | — `Unsupported` | "an ssh machine cannot reach the caller's loopback". | [`ssh/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/ssh/adapter.rs) |

Notes from the receipts:

- On `apple-container` the gateway reaches a host server only when it
  listens on the bridge (`0.0.0.0`), not on `127.0.0.1` alone. The API adds
  that as a hint to a failed reach check
  ([`sandbox/route.rs`](../../../../api/crates/layerfs-control/src/sandbox/route.rs)).
- Verified live per platform in the
  [G7 live receipt](../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md);
  network facts per guest in the
  [G7 reach receipt](../../../../core/docs/issues/535/receipts/g7-reach/RECEIPT.md).

### What `PostgresUrl` reads

| Part | Read as | Source |
| --- | --- | --- |
| scheme | everything before `://` | [`postgres_url.rs`](../../../../connect/crates/layerfs-sandbox/src/postgres_url.rs) |
| userinfo | everything before the last `@` of the authority, password included; kept byte for byte | same |
| host | `[v6]` without brackets, else before the last `:`; a host list `h1,h2` reads as one host and `dial` refuses it | same |
| port | `:` then 1 to 5 digits, else the parse fails | same |
| query | `key=value` pairs, not percent-decoded | same |

`with_host` brackets an IPv6 alias. `PostgresUrl` has no `Debug`, and no
message from `reach` holds any part of the URL: `refuse` names only the
platform, machine and reason.

## The verify-full rule

```text
 caller's URL     postgres://u:p@127.0.0.1:5432/db?sslmode=verify-full
                                 \_______/
 rewrite would give               192.168.5.2      <- not in the server's
                                                      certificate
 => Unsupported: "give the Store a URL whose host every machine can verify"
```

| Case | Result |
| --- | --- |
| Loopback host, `sslmode=verify-full`, platform rewrites | `Unsupported`: the certificate does not name the alias. |
| Loopback host, `sslmode=verify-full`, `native` | Used as given (`Shared` answers before parsing). |
| Non-loopback host, `sslmode=verify-full` | Used as given on every platform. |
| Other `sslmode` values | Not checked; a rewrite goes ahead. |

Source: `dial` in [`reach.rs`](../../../../connect/crates/layerfs-machine/src/reach.rs).
The check order puts `Refused` first, so an `ssh` machine reports its own
reason, not the verify-full one.

## Related

- [Credentials](credentials.md): the password inside the URL, resolved
  before `reach` runs.
- [Forwards](forwards.md): the other per-platform network answer.
- [Machines: Adapter](../machines/adapter.md): the `Adapter::reach` method each platform answers.
- [Engine: Store](../../engine/store/README.md) and [its PostgreSQL page](../../engine/store/postgresql.md): what the daemon opens with the URL.

---

← [Network: Workers](workers.md) · [Up: Network](README.md) · [Network: Forwards](forwards.md) →
