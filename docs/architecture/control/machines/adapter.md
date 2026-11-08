# Adapter

[Architecture](../../README.md) › [Control](../README.md) › [Machines](README.md) › Adapter

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker, Microsandbox and SSH machines were removed in 2.0; a record
an older home still holds for one is shown, refused and only forgotten.
See [removed platforms](../support-matrix/disabled-platforms.md).

The sealed Adapter every platform implements.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](README.md) |
| Source | [`control/crates/layerfs-platform/src/adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs), [`control/crates/layerfs-platform/src/probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs), [`control/crates/layerfs-platform/src/facts.rs`](../../../../control/crates/layerfs-platform/src/facts.rs), [`control/crates/layerfs-platform/src/setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |

## The sealed Adapter

```text
 pub(crate) trait Adapter: Sync          no default methods (#535 G2)
   probe(tools, host, &mut missing) -> Option<version>
   guest(spec)                      -> Guest
   create(spec, bundle)             -> Running
   remove(spec)                     -> ()
   start(spec, bundle)              -> Option<Running>
   stop(spec)                       -> ()
   status(spec)                     -> Status
   reach(spec, url)                 -> String      PostgreSQL URL to dial
   forward(spec, guest_port)        -> Forward

 pub(crate) fn adapter(platform) -> &'static dyn Adapter {
     match platform {
         Native         => &native::ADAPTER,
         Docker         => &docker::ADAPTER,
         AppleContainer => &apple_container::ADAPTER,
         Lima           => &lima::ADAPTER,
     }
 }
```

The trait and `adapter()` are crate-private; nothing outside
`layerfs-platform` can add a platform or call an adapter directly. A new
`Platform` variant does not compile until it has an arm here, one in
`Platform::profile()`, and one in each conformance-kit match.

A machine's lifecycle, and which calls each platform takes:

```text
            probe (starts nothing)
              |
              v
     create ------------------> running <-------+
              |                   |   ^         |
              |                 stop  start     | status, reach, forward,
              |                   v   |         | copy_in/out (running)
              |                 stopped --------+
              v                   |
     remove <---------------------+   (from running or stopped)
```

| Call | `native` | `docker` | `apple-container` | `lima` |
| --- | --- | --- | --- | --- |
| `create` | refused | refused | yes | yes |
| `remove` | refused | refused | yes | yes |
| `start` | `None` (macOS, Linux) | yes | yes | yes |
| `stop` | `Ok`, nothing | worker only | yes | yes |
| `status` | `Running` | yes | yes | yes |
| `guest` | refused | yes (macOS, Linux) | `container exec` | Lima ssh |
| `reach` | as given | Desktop: worker's `host.docker.internal` address; Linux: as given | gateway lookup | `192.168.5.2` |
| `forward` | port as it is | `Unsupported` | `Unsupported` | `ssh -L` |

A refusal from `adapter::refused` is `InvalidInput`, "the <platform>
machine <name> <why>", raised before any tool runs:

| `why` | Raised by | Platforms |
| --- | --- | --- |
| `is built in and never created` | `create` | `native`, `docker` |
| `is built in and never removed` | `remove` | `native`, `docker` |
| `runs in-process; its files are the host's` | `guest` (so `copy_in`, `copy_out`) | `native` |
| `loads no Docker image` | `setup::load_image` | all but `docker` |

`reach` is described in [Network: Reach](../network/reach.md) and
`forward` in [Network: Forwards](../network/forwards.md).

## Probe

```text
 probe(kind, tools)                          probe.rs
   missing = []
   version = adapter(kind).probe(tools, HostOs::current(), &mut missing)
   Probe { kind, ready: missing.is_empty(), missing, version }
```

| Platform | Pushes onto `missing` | Version from |
| --- | --- | --- |
| `native` | Linux: `host:root`, `host:/dev/fuse`, `host:cgroup2`; Windows: `host:macos-or-linux`; macOS: nothing | none |
| `docker` | Windows: `host:wsl2`; `tool:docker`; `host:<socket>` when the Engine does not answer | `docker -H unix://<socket> version` (server) |
| `apple-container` | `host:macos`; `host:apple-silicon` on a non-`aarch64` Mac; `tool:container` | `container --version` |
| `lima` | `host:macos`; `tool:limactl` | `limactl --version` |

A probe starts nothing. Each version query is one command with a 10 s
deadline (`ANSWER`); its first non-empty line is the version.
`machine.platforms` shows each probe as `ready`, `missing` and `version`.

## Facts

Facts read from provider and guest output, in
[`facts.rs`](../../../../control/crates/layerfs-platform/src/facts.rs):

| Function | Reads | Answers | Used by |
| --- | --- | --- | --- |
| `json_string(text, path)` | one string at a key path in a JSON answer; an array on the way stands for its first element | `None` when absent, not a string, or longer than 128 bytes; an error past depth 32 | apple-container `status.state` and `status.networks.ipv4Gateway`; Lima `disk list --json` names |
| `df_free(stdout)` | the last line of `df -kP <path>` | free bytes | `worker::free_disk` (`Status::free_disk`) |
| `listening(proc_net_tcp, port)` | `/proc/net/tcp` or `tcp6` text | whether a socket in state `0A` (listen) holds the port | `worker::ready` |

`json_string` walks the answer with `layerfs_sandbox_backend::json::Json`, a
streaming reader, skipping what it does not need; it builds no tree.

## Setup calls

The public calls in
[`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs),
re-exported from the crate root:

| Call | Checks first | Then |
| --- | --- | --- |
| `create(spec, bundle)` | `validate`, `check_bundle` | `adapter.create` |
| `start(spec, bundle)` | `validate`; `Ok(None)` when the route is `InProcess`; `check_bundle` | `adapter.start` |
| `stop(spec)` | `validate` | `adapter.stop` |
| `remove(spec)` | `validate` (`InUse` is the caller's check) | `adapter.remove` |
| `status(spec)` | `validate` | `adapter.status`; starts nothing |
| `status_within(spec, deadline)` | as `status` | every command killed with its process group at `deadline` and answered `Unknown` |
| `reach(spec, url)` | `validate` | `adapter.reach`; no message holds the URL |
| `forward(spec, guest_port)` | `validate`; port 0 is `InvalidInput` | `adapter.forward` |
| `forward_close(spec, forward)` | none | ends the `ssh -L` pid only if its command line still names that port |
| `load_image(spec, archive)` | `validate`; `docker` only | `docker load -i`, answers `sha256:<64 hex>` |
| `copy_in`, `copy_out` | `validate`; the platform's `guest` | one `tar` stream in, `cat` out to a new 0600 file |

`Status` is `{ state, free_disk, tool_version, down_reason }`. A VM
machine's kernel state goes through `with_worker`:

| Kernel state | Worker answers its hello | `state` | `down_reason` |
| --- | --- | --- | --- |
| `Running` | yes | `running` | none |
| `Running` | no | `stopped` | "the machine runs but its worker does not answer" |
| `Stopped` | — | `stopped` | "the machine is stopped" |
| `Failed` | — | `failed` | "the provider reports it broken" |
| `Missing` | — | `missing` | "the provider holds no such VM" |

The hello is the worker protocol's first step only: the secret as one
length-prefixed frame, answered `{"ready":true}` within 2 s. A bare TCP
connect is not enough, because a forward accepts while no worker listens.
Errors carry one `Code` (`Exists`, `InUse`, `Unsupported`, `Unavailable`,
`Denied`, `Failed`, `Unknown`, `InvalidInput`, `NotFound`), `missing`, and
`vm_exists` ([`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs)).

## Related

- [Platforms and profiles](platforms-and-profiles.md): `Platform`, `Profile`, `Spec`.
- [Adding a platform](adding-a-platform.md): where a new `ADAPTER` goes.
- [Network: Workers](../network/workers.md): the worker that setup starts.
- [Network: Reach](../network/reach.md) and [Network: Forwards](../network/forwards.md).
- [Support matrix](../support-matrix/README.md): refusals that come before any adapter call.

---

← [Machines: Platforms and profiles](platforms-and-profiles.md) · [Up: Machines](README.md) · [Machines: Native](platforms/native.md) →
