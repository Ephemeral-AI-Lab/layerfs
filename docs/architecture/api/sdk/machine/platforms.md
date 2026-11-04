# machine.platforms

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`machine.platforms` answers the support table for this host: each platform,
how its machines come to exist, where their Store calls run, the options
`create` or `connect` takes for it, and whether this host has what it needs.
It starts nothing.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/platforms.rs`](../../../../../api/crates/layerfs-api/src/machine/platforms.rs) |
| Implemented in | [`layerfs_control::machine::platforms`](../../../../../api/crates/layerfs-control/src/machine/platforms.rs) |
| Runs in | the caller's process (host build with the `machine` feature) |

## Signature

```js
// JavaScript
await layerfs.machine.platforms();
```

```python
# Python
layerfs.machine.platforms()
```

```bash
# CLI
layerfs machine platforms [--text]
```

## Arguments

None.

## Options

None.

## Returns

```text
{ host: "macos" | "linux" | "windows" | "other",
  platforms: [                          seven rows, in Platform::ALL order
    { platform, how, backend, kernel, sqlite,
      postgresql: { store_calls, sandbox, reason },
      ready, missing: [text], version,     <- live probe
      options: [ { name, kind } ],
      description } ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `host` | text | the host this build runs on | [`platform.rs`](../../../../../connect/crates/layerfs-machine/src/platform.rs) `HostOs` |
| `platform` | text | the name `machine create` takes | `Profile.name` |
| `how` | text | `built_in` (`native`, `docker`), `created`, or `connected` (`ssh`) | `How` |
| `backend` | text or null | `native`, `namespaces` or `docker`; null when the platform is not on this host | [`support.rs`](../../../../../connect/crates/layerfs-machine/src/support.rs) |
| `kernel` | text or null | the kernel route its Sandboxes' daemons serve: `fuse` or `nfs` | `support.rs` |
| `sqlite` | text or null | where a sqlite Store's calls run: `in_process` or `worker`; null when not on this host | `support.rs` |
| `postgresql` | map | `store_calls` (always `in_process`), `sandbox` (bool), `reason` (why not) | `support.rs` |
| `ready` | bool | `missing` is empty | [`probe.rs`](../../../../../connect/crates/layerfs-machine/src/probe.rs) |
| `missing` | list | `tool:<cli>` not on the search path, `host:<fact>` the host lacks | `probe.rs` |
| `version` | text or null | the provider tool's version line, when it answers | `probe.rs` |
| `options` | list | the options `create` or `connect` takes for this platform, `{ name, kind }` | [`options.rs`](../../../../../connect/crates/layerfs-machine/src/options.rs), `platforms/*/profile.rs` |
| `description` | text | the platform, then how its Sandboxes run | `support.rs` |

The rows without the live fields are
[`connect/SUPPORT.json`](../../../../../connect/SUPPORT.json), generated from
`support.rs`. Per host:

| Platform | `how` | macOS | Linux | Windows | `options` |
| --- | --- | --- | --- | --- | --- |
| [`native`](../../../connect/machines/platforms/native.md) | built_in | native, nfs | namespaces, fuse | records only | — |
| [`docker`](../../../connect/machines/platforms/docker.md) | built_in | docker, fuse | docker, fuse | not designed | — |
| [`apple-container`](../../../connect/machines/platforms/apple-container.md) | created | namespaces, fuse | — | — | `cpus`, `memory`, `disk`, `image` |
| [`microsandbox`](../../../connect/machines/platforms/microsandbox.md) | created | namespaces, fuse | namespaces, fuse | namespaces, fuse | `cpus`, `memory`, `disk`, `image` |
| [`lima`](../../../connect/machines/platforms/lima.md) | created | namespaces, fuse | — | — | `cpus`, `memory`, `disk`, `image`, `nested` |
| [`firecracker`](../../../connect/machines/platforms/firecracker.md) | created | namespaces, fuse | — | — | `cpus`, `memory`, `disk`, `image`, `host` |
| [`ssh`](../../../connect/machines/platforms/ssh.md) | connected | namespaces, fuse | namespaces, fuse | namespaces, fuse | `user`, `port`, `identity`, `known_hosts`, `root` |

"—" means the platform is not on that host; its row says why in
`postgresql.reason` ("lima runs on macOS hosts only").

## Where it runs

```text
caller's process
  HostOs::current()
  for each of the seven platforms:
    support(platform, host)           fixed table, no I/O
    probe(platform, tools)            tool lookup and version query
                                      (each command bounded at 10 s)
  answer { host, platforms }          nothing is written
```

All in the caller ([routing table](../../routing/routing-table.md)). The
docker probe uses the socket the `docker` machine's record names
([`spec.rs`](../../../../../api/crates/layerfs-control/src/machine/spec.rs) `tools`).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `Unsupported` | a build without the `machine` feature | no | [`layerfs-api/src/machine/platforms.rs`](../../../../../api/crates/layerfs-api/src/machine/platforms.rs) |
| `Failed` | the docker machine's record cannot be read | no | [`spec.rs`](../../../../../api/crates/layerfs-control/src/machine/spec.rs) `tools` |

A missing tool is not an error: the row has `ready: false`.

## Examples

Not run; written from the call table.

```js
const { platforms } = await layerfs.machine.platforms();
const creatable = platforms.filter((p) => p.how === "created" && p.ready);
```

```python
rows = layerfs.machine.platforms()["platforms"]
```

```console
$ layerfs machine platforms
```

## Related

- Next: [`machine.create`](create.md) or [`machine.connect`](connect.md)
  with a platform whose row is `ready`.
- [Connect: Support matrix](../../../connect/support-matrix/README.md) and
  its [generated JSON](../../../connect/support-matrix/generated-json.md);
  [Connect: Machines](../../../connect/machines/README.md).

---

← [machine.list](list.md) · [Up: machine](README.md) · [machine.get](get.md) →
