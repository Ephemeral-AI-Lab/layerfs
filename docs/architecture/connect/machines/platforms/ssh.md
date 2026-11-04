# SSH

[Architecture](../../../README.md) › [Connect](../../README.md) › [Machines](../README.md) › SSH

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The SSH platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`connect/crates/layerfs-machine/src/platforms/ssh`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh) |

## How it is made

A connected machine: an existing Linux host reached over ssh, never
created. The user is `root` by default, so setup and the worker run as
root ([`ssh/observe.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh/observe.rs)).

```text
 connect(spec, bundle)                       ssh/connect.rs, power.rs
   accept_key: ssh-keygen -F <host|[host]:port> -f <known_hosts>
       no line -> Denied; else copy lines to <work_dir>/known_hosts (0600)
   ssh <opts> user@host -- uname -s     fails -> Unavailable
                                        not Linux -> Unsupported
   DATA_ROOT: mkdir; findmnt FSTYPE in ext4|xfs|btrfs  else Unsupported
              a root other than /layerfs/data is bind-mounted there
   forward::open  ssh -N -L 127.0.0.1:<free>:127.0.0.1:7070 user@host
   worker::bring_up over ssh (pid of the forward in the endpoint)
       failure -> the forward is ended
   Running { endpoint, lasting { port } }

 <opts> = -o BatchMode=yes -o ConnectTimeout=15
          -o StrictHostKeyChecking=yes -o UserKnownHostsFile=<accepted>
          -p <port> [-i <identity>]
```

| Call | Does | Source |
| --- | --- | --- |
| `connect` | as above; nothing is created on the host but `/layerfs/bundle`, `/layerfs/state` and the data root's folders | [`ssh/connect.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh/connect.rs) |
| `start` | ends any old forward, accepts the key again, `bring_up` | [`ssh/power.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh/power.rs) |
| `stop` | ends the forward and the worker; the host keeps running | same |
| `disconnect` | ends the forward and worker; removes `/layerfs/bundle` and `/layerfs/state`; unbinds `/layerfs/data` when the root differs; never the data root | `connect.rs` |
| `status` | forward not running: `stopped`, "its worker forward is not running"; else `running` through `with_worker`, with free disk | `observe.rs` |
| `host_identity` | the host key's `SHA256:` fingerprint, else `machine-id:<id>` | `observe.rs` `identity` |
| `forward` | a detached `ssh -N -L 127.0.0.1:<free>:127.0.0.1:<guest port>` checked against the accepted key | `observe.rs` `daemon_forward` |

The worker is ended only if its recorded pid's command line is still
exactly `/layerfs/bundle/layerfs worker`, since pids are reused after a
reboot (`END_WORKER`).

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | a Linux host over ssh, connected and never created; Sandboxes run in Linux namespaces and a cgroup |
| linux | `namespaces` | `fuse` | same |
| windows | `namespaces` | `fuse` | same |

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `worker` | `in_process` |
| windows | `worker` | `in_process` |

The worker binds the host's loopback (`worker_bind` `127.0.0.1`), which the
`ssh -L` forward reaches. Workspace calls skip the worker through a second
forward.

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `true` | `null` |
| windows | `true` | `null` |

`reach` refuses a loopback URL `Unsupported`: "an ssh machine cannot reach
the caller's loopback" (`Loopback::Refused`). A URL naming any other host
is dialled as given, so a PostgreSQL Store on `ssh` needs a server address
the host can reach.

## Options

| Option | Kind | `SshSpec` field | Default (`SshSpec::new`) |
| --- | --- | --- | --- |
| `user` | text | `user` | `root` |
| `port` | int | `port` | 22 |
| `identity` | text | `identity` | none: ssh-agent and the default keys |
| `known_hosts` | text | `known_hosts` | set by the caller |
| `root` | text | `data_root` | `/layerfs/data` |

## Live receipts

| Receipt | What ran |
| --- | --- |
| [K2](../../../../../core/docs/issues/535/receipts/fe6f85013b/K2/RESULTS.md) | first terminal pass, from macOS against a throwaway Lima VM as the host |
| [L2](../../../../../core/docs/issues/535/receipts/35ed671b6c/L2/RESULTS.md) | final pass: sqlite and PostgreSQL remote cells, power |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | the merged verdict (loopback refused by design) |

Not run from a Linux or Windows caller.

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: tool:ssh` | no `ssh` on the search path (`ssh-keygen` is needed too) | adapter probe, [`ssh/host_key.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/ssh/host_key.rs) |
| `Denied`: "the host key of … is not in …; add it there first" | the host's key is not in `known_hosts` | `host_key.rs` |
| `Unavailable`: "ssh to … failed …" | login refused or host unreachable (`BatchMode` never prompts) | `connect.rs` |
| `Unsupported`: "… answers …, not Linux" | `uname -s` is not `Linux` | `connect.rs` |
| `Unsupported`: "the data root … is on …, not ext4, xfs or btrfs" | data root filesystem (O18) | `power.rs` `DATA_ROOT` |
| `Failed`: "ssh -L 127.0.0.1:<port> ended at once" | the forward died; `<work_dir>/forward.log` | [`setup/forward.rs`](../../../../../connect/crates/layerfs-machine/src/setup/forward.rs) |
| `status` `stopped`: "its worker forward is not running" | the caller's `ssh -L` is no longer running | `observe.rs` |

## Related

- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md).
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md), [Network: Credentials](../../network/credentials.md).
- [Adapter](../adapter.md), [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Firecracker](firecracker.md) · [Up: Machines](../README.md) · [Machines: Adding a platform](../adding-a-platform.md) →
