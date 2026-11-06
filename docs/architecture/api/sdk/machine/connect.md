# machine.connect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › connect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are **permanently disabled for new work**. Their code
and records remain for inspection and explicit cleanup. See the
[platform restriction](../../../connect/support-matrix/disabled-platforms.md) for refusals, existing-resource handling and
the source change required for re-enabling. Descriptions of their adapter
mechanics below are retained implementation details, not selectable features.

This retained call always refuses `Unsupported` after argument validation and
before home creation or platform effects. The adapter code below is disabled.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/connect.rs`](../../../../../api/crates/layerfs-api/src/machine/connect.rs) |
| Implemented in | `layerfs_control::machine::connect` ([`machine/connect.rs`](../../../../../api/crates/layerfs-control/src/machine/connect.rs)), then `layerfs_machine::connect` ([`setup/calls.rs`](../../../../../connect/crates/layerfs-machine/src/setup/calls.rs)) |
| Runs in | the caller's process; `ssh` to the host |

## Signature

```js
// JavaScript
await layerfs.machine.connect(name, host, { user, port, identity, known_hosts, root });
```

```python
# Python
layerfs.machine.connect(name, host, user=None, port=None, identity=None,
                        known_hosts=None, root=None)
```

```bash
# CLI
layerfs machine connect <name> <host> [--user U] [--port N] \
  [--identity PATH] [--known-hosts PATH] [--root PATH]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | `MachineName::parse`; not built in; not taken | [`machine/create.rs`](../../../../../api/crates/layerfs-control/src/machine/create.rs) `new_name` |
| `host` | Text | the ssh host name or address | [`machine/connect.rs`](../../../../../api/crates/layerfs-control/src/machine/connect.rs) |

`ssh` is disabled on macOS, Linux, Windows and every other host
([`SUPPORT.json`](../../../../../connect/SUPPORT.json)); see the
[ssh platform page](../../../connect/machines/platforms/ssh.md).

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `user` | Text | `root` | the login user | [`machine/connect.rs`](../../../../../api/crates/layerfs-control/src/machine/connect.rs) |
| `port` | Int | 22 | the ssh port | same |
| `identity` | Text | none (ssh's own) | a private key path, passed as `-i` | same, [`ssh/observe.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh/observe.rs) |
| `known_hosts` | Text | `$HOME/.ssh/known_hosts` | copied into `machine/<name>/known_hosts`; the host key is checked strictly against it | [`machine/connect.rs`](../../../../../api/crates/layerfs-control/src/machine/connect.rs) |
| `root` | Text | `/layerfs/data` | the data root on the host for Stores | [`spec.rs`](../../../../../connect/crates/layerfs-machine/src/spec.rs) `DATA` |

## Retained success result (unreachable while disabled)

The machine's summary with `platform: "ssh"`, `state: "running"` and its
`ssh` settings; see [machine.get](get.md#returns) and
[machine.list](list.md#returns). The record also keeps the host's identity
(its host key fingerprint, else `/etc/machine-id`).

## Where it runs

```text
 caller process                                    ssh host
 --------------                                    --------
 known_hosts (option or $HOME) ; probe ssh tool ; bundle()
 home lock: new_name, write record (op "connect", phase identify),
            copy known_hosts
 host_identity ----------------------------------->  host key / machine-id
 home lock: another machine with that identity? --> Exists ("same host")
            record host id, op phase "connect"
 layerfs_machine::connect ------------------------>  /layerfs/bundle,
                                                     /layerfs/state, data
                                                     root folders; worker
                                                     + forward; secret
 home lock: worker.json, state running, drop op     (finish)
```

A second machine on the same host would be one kernel twice, so it is
refused. Outcomes are recorded as for [machine.create](create.md#where-it-runs).
See the [routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad or built-in name; `known_hosts` unset with no `HOME`, or unreadable; a folder-scoped client | no | [`machine/connect.rs`](../../../../../api/crates/layerfs-control/src/machine/connect.rs) |
| `Exists` | the name is taken; another machine is the same host | no | same |
| `Unsupported` | `ssh` not usable here (`detail.missing`); no sealed bundle; no `machine` feature | no | same, [`machine/bundle.rs`](../../../../../api/crates/layerfs-control/src/machine/bundle.rs) |
| `Denied` | the host refuses the login or its key is not the known one | no | [`error.rs`](../../../../../connect/crates/layerfs-machine/src/error.rs) |
| `Unavailable`, `Failed` | the host does not answer; a setup step failed | no | same |
| `Unknown` | **a setup step timed out or its answer was lost** | **yes** | [`machine/create.rs`](../../../../../api/crates/layerfs-control/src/machine/create.rs) `finish` |

**After `Unknown`**, read [machine.status](status.md), then clear it with
[machine.disconnect](disconnect.md).

## Examples

Refusal examples: every call below returns the fixed SSH `Unsupported` reason.

```js
await layerfs.machine.connect("box", "build.example.org", { user: "lfs", identity: "/home/me/.ssh/id_ed25519" });
```

```python
layerfs.machine.connect("box", "10.0.0.7", port=2222)
```

```bash
layerfs machine connect box build.example.org --user lfs --root /srv/layerfs
```

## Related

- Before: [machine.platforms](platforms.md). After:
  [store.push](../store/push.md), [sandbox.create](../sandbox/create.md).
  Undo: [machine.disconnect](disconnect.md).
- [Connect: ssh](../../../connect/machines/platforms/ssh.md),
  [Connect: workers](../../../connect/network/workers.md),
  [Connect: forwards](../../../connect/network/forwards.md).

---

← [machine.create](create.md) · [Up: machine](README.md) · [machine.remove](remove.md) →
