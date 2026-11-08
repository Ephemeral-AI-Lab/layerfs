# machine.platforms

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker is **permanently disabled for new work**. Its code and records
remain for inspection and explicit cleanup. Microsandbox and SSH machines
were removed in 2.0; a record an older home still holds for one is shown,
refused and only forgotten. See the [platform restriction](../../../control/support-matrix/disabled-platforms.md) for
refusals, existing-resource handling and the source change required for
re-enabling. Descriptions of Firecracker's adapter mechanics below are
retained implementation details, not selectable features.

`machine.platforms` answers the support table for this host: each platform,
how its machines come to exist, where their Store calls run, the options
`create` takes for it, and whether this host has what it needs.
It starts nothing.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/platforms.rs`](../../../../../api/crates/layerfs-api/src/machine/platforms.rs) |
| Implemented in | [`layerfs_control::machine::platforms`](../../../../../control/crates/layerfs-machine/src/platforms.rs) |
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
  platforms: [                          five rows, in Platform::ALL order
    { platform, how, backend, kernel, sqlite,
      postgresql: { store_calls, sandbox, reason },
      ready, missing: [text], version,     <- live probe
      options: [ { name, kind } ],
      description } ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `host` | text | the host this build runs on | [`platform.rs`](../../../../../control/crates/layerfs-platform/src/platform.rs) `HostOs` |
| `platform` | text | the name `machine create` takes | `Profile.name` |
| `how` | text | `built_in` (`native`, `docker`) or `created` | `How` |
| `backend` | text or null | `native` or `namespaces` (also on `docker`, D-8); null when the platform is not on this host | [`support.rs`](../../../../../control/crates/layerfs-platform/src/support.rs) |
| `kernel` | text or null | the kernel route its Sandboxes' daemons serve: `fuse` or `nfs` | `support.rs` |
| `sqlite` | text or null | where a sqlite Store's calls run: `in_process` or `worker`; null when not on this host | `support.rs` |
| `postgresql` | map | `store_calls` (always `in_process`), `sandbox` (bool), `reason` (why not) | `support.rs` |
| `ready` | bool | `missing` is empty | [`probe.rs`](../../../../../control/crates/layerfs-platform/src/probe.rs) |
| `missing` | list | `tool:<cli>` not on the search path, `host:<fact>` the host lacks | `probe.rs` |
| `version` | text or null | the provider tool's version line, when it answers | `probe.rs` |
| `options` | list | the options `create` takes for this platform, `{ name, kind }` | [`options.rs`](../../../../../control/crates/layerfs-platform/src/options.rs), `platforms/*/profile.rs` |
| `description` | text | the platform, then how its Sandboxes run | `support.rs` |

The rows without the live fields are
[`control/SUPPORT.json`](../../../../../control/SUPPORT.json), generated from
`support.rs`. Per host:

| Platform | `how` | macOS | Linux | Windows | `options` |
| --- | --- | --- | --- | --- | --- |
| [`native`](../../../control/machines/platforms/native.md) | built_in | native, nfs | namespaces, fuse | records only | — |
| [`docker`](../../../control/machines/platforms/docker.md) | built_in | namespaces, fuse | namespaces, fuse | not designed | — |
| [`apple-container`](../../../control/machines/platforms/apple-container.md) | created | namespaces, fuse | — | — | `cpus`, `memory`, `disk`, `image` |
| [`lima`](../../../control/machines/platforms/lima.md) | created | namespaces, fuse | — | — | `cpus`, `memory`, `disk`, `image`, `nested` |
| [`firecracker`](../../../control/machines/platforms/firecracker.md) | created | disabled | disabled | disabled | — (profile: `cpus`, `memory`, `disk`, `image`, `host`) |

"—" means the platform is not on that host; its row says why in
`postgresql.reason` ("lima runs on macOS hosts only"); a disabled one says
its policy reason on every host. Microsandbox and SSH were removed in 2.0
and have no row.

## Where it runs

```text
caller's process
  HostOs::current()
  for each of the five platforms:
    support(platform, host)           fixed table, no I/O
    probe(platform, tools)            tool lookup and version query
                                      (each command bounded at 10 s)
  answer { host, platforms }          nothing is written
```

All in the caller ([routing table](../../../control/routing/routing-table.md)). The
docker probe uses the socket the `docker` machine's record names
([`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `tools`).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `Unsupported` | a build without the `machine` feature | no | [`layerfs-api/src/machine/platforms.rs`](../../../../../api/crates/layerfs-api/src/machine/platforms.rs) |
| `Failed` | the docker machine's record cannot be read | no | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `tools` |

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

- Next: [`machine.create`](create.md)
  with a platform whose row is `ready`.
- [Control: Support matrix](../../../control/support-matrix/README.md) and
  its [generated JSON](../../../control/support-matrix/generated-json.md);
  [Control: Machines](../../../control/machines/README.md).

---

← [machine.list](list.md) · [Up: machine](README.md) · [machine.get](get.md) →
