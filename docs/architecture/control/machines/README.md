# Machines

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

A machine is a kernel Sandboxes run on: `layerfs-machine` keeps its record
and calls, and `layerfs-platform` makes, starts, stops and removes it
through one adapter per platform.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-machine` (records, calls, worker client), `layerfs-platform` (driver: profiles, adapters, setup, support table) |
| Source | [`layerfs-machine/src`](../../../../control/crates/layerfs-machine/src), [`layerfs-platform/src`](../../../../control/crates/layerfs-platform/src) |
| Uses | [Home](../home/README.md), [Sandboxes: backends](../sandboxes/backends.md) (`Backend`, `platform_policy`) |
| Used by | [Stores and projects](../stores-and-projects/README.md), [Sandboxes](../sandboxes/README.md), [Doctor](../doctor/README.md), [API: machine](../../api/sdk/machine.md) |
| Flows | [Call](../../flows/call.md), [Workspace](../../flows/workspace.md) |

## Picture

```text
 layerfs-machine  machine.create/start/stop/remove/update/status/list/get/
                  platforms: records, home lock, op intent, InUse
      |  Spec (every path and setting) + Bundle + Tools
      v
 layerfs-platform setup/calls.rs: spec.validate(), check_bundle()
      |                               (before any effect)
      +--> adapter(platform)  one match -> platforms/<p>/adapter.rs ADAPTER
      +--> Platform::profile() one match -> platforms/<p>/profile.rs PROFILE
      |                                       |
      |                                       v
      |                               support.rs rows -> SUPPORT.json
      v
 setup/vm.rs      one provider CLI call (limactl, container, docker)
 setup/guest.rs   a root shell in the guest (Guest enum)
 setup/worker.rs  prepare -> install -> launch -> ready
      |
      v
 Running { endpoint: host loopback address + secret, lasting state }
      --> layerfs-machine writes worker.json and record.json
```

`layerfs-platform` knows nothing of the home: the caller passes every path
and setting in a `Spec` and writes every record
([`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs)).

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Keeps machine records | `machine/<name>/record.json`: platform, size, images, lasting guest state, recorded state, the `op` of a create or remove, built-in settings; read once and kept whole | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) |
| Built-in machines | `machine/native/` and `machine/docker/` exist from the first call and are never removed | [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) `ensure_built_ins` |
| `machine.create` | home lock only to claim and to record; the claimed record holds the `op` while the VM boots, and the worker host port its VM forwards (`lasting.port`) | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) |
| `machine.start` | the only call that starts a machine: dials the recorded worker; if none is recorded, or it does not answer and the provider reports the machine not running, starts machine and worker under `start.lock` and replaces `worker.json`. A recorded worker that does not answer on a running machine is never started over (that would end it and every Sandbox on it): the machine is recorded `broken` `not_answering` and the start refused `Unavailable`, fix a stop and a start ([Workers and reach](workers-and-reach.md#lifecycle)) | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs), [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| `machine.stop` | stops worker and VM (disks stay); refused `InUse` while Sandboxes are recorded on it | [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| `machine.remove` | `op` in the record, VM deleted under `start.lock`, folder removed; refused `InUse` while it holds Stores or Sandboxes; `outcome: removed`. Of a removed platform's machine only its record is removed (`{ok: true}`, `message` "only its record was removed (<reason>); nothing on its host was touched") | [`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs) |
| `machine.update` | changes settings; refused `InUse` while Sandboxes run on it. A resize (`cpus`, `memory`) is not built: on a VM machine it is refused `Unsupported` ("resizing a <platform> machine is not built yet; nothing was changed"), after the `InUse` check; on a built-in machine `InvalidInput` | [`update.rs`](../../../../control/crates/layerfs-machine/src/update.rs) |
| `machine.list`, `machine.get` | records and the row's `supports`; with `--with live` one probe of each machine's provider and one hello to its worker (at most 10 s, else `unknown`), starting nothing; the record is written only when the observed state word or reason changed | [`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs) `list`, `get`; [`live.rs`](../../../../control/crates/layerfs-machine/src/live.rs); [`machine.rs`](../../../../control/crates/layerfs-control/src/machine.rs) |
| `home.get --with platforms` | this host's support rows plus live `ready`, `missing`, `version` from `probe`; starts nothing (`layerfs_machine::platforms`, called from `layerfs-control` `home.rs`) | [`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs), [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) |
| Docker default image | with no image named, the bundle's `sandbox-image.tar` is loaded into Docker once; the record keeps `loaded_image: {id, bundle}`, and an Engine tag `layerfs-cache/sandbox:<config hex>` reuses it across homes after `docker image inspect` | [`image.rs`](../../../../control/crates/layerfs-machine/src/image.rs), [`docker/images.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/images.rs) |
| Finds the bundle | `LAYERFS_BUNDLE`, else the installed runtime's `bundle/linux-<arch>/` beside its `bin/layerfs`, else `<cache>/layerfs/bundle/<version>/linux-<arch>/`, each with `bundle.json`; none is `Unsupported` naming `tools/linux_bundle.py`; one sealed from another commit than this build's (`source.head`, when the build names its commit) is `Unavailable` | [`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs) |
| Bundle contents | `layerfs` (guest worker), `layerfs-daemon`, `layerfs-sandbox-helper`, `sandbox-image.tar`, and the Sandbox runtime layer `runtime/` (R5): `lib/` the loader and every library `ldd layerfs-daemon` lists, copied from the daemon's build image, and `bin/` that image's `getent` and Debian's static `bash` and `busybox`; `licenses/` holds the Debian copyright files of each package they come from. `bundle.json` lists every file's sha256 in `files` (so the bundle `hash` covers the runtime) and `sandbox_runtime: {hash, loader, glibc, packages}`; `seal` refuses a library `ldd` lists that it did not copy | [`tools/linux_bundle.py`](../../../../tools/linux_bundle.py), [Sandboxes: backends](../sandboxes/backends.md) |
| Reaches a running machine | worker client, PostgreSQL `reach`, daemon `forward` | [Workers and reach](workers-and-reach.md) |

### Platforms and profiles

```text
 Platform::ALL = [Native, Docker, AppleContainer, Lima]
   profile() -> &'static Profile     (platforms/<p>/profile.rs PROFILE)
   adapter() -> &'static dyn Adapter (platforms/<p>/adapter.rs ADAPTER)
```

| `Profile` field | Meaning |
| --- | --- |
| `name` | what callers write (`layerfs machine create dev lima`); `parse` takes the exact name only |
| `tool` | the provider CLI, if any |
| `how` | `BuiltIn` (one each, never created or removed) or `Created` |
| `hosts` | the hosts a machine of this platform can be on |
| `backend(host)`, `kernel(host)` | the Sandbox backend and kernel route per host |
| `executor(host)` | who runs a sqlite Store's calls: `Local` (the caller) or `Worker`; `route()` maps it to `InProcess` or `Worker` |
| `worker_bind` | the guest address the worker binds |
| `options` | what `machine create` takes, each `int`, `size` or `text` |
| `description` | one line |

The per-platform values are in [Platforms](platforms.md).

### The sealed adapter

```text
 pub(crate) trait Adapter: Sync          no default methods
   probe(tools, host, &mut missing) -> Option<version>   starts nothing
   guest(spec)                      -> Guest
   create(spec, bundle)             -> Running
   remove(spec)                     -> ()
   start(spec, bundle)              -> Option<Running>   None: nothing to start
   stop(spec)                       -> ()
   provider_state(spec)             -> State   no worker or guest contact
   status(spec)                     -> Status  starts nothing
   reach(spec, url)                 -> String  PostgreSQL URL to dial
   forward(spec, guest_port)        -> Forward
```

| Public call (`setup/calls.rs`) | Checks first | Then |
| --- | --- | --- |
| `create`, `start` | `validate`, `check_bundle`; `start` answers `None` when the route is `InProcess` | the adapter; `create` forwards `spec.state.port` when the caller recorded one (`worker_port`), `start` refuses `InvalidInput` without it on `lima` and `apple-container` |
| `stop`, `remove`, `status` | `validate` (`InUse` is the caller's check) | the adapter |
| `status_within`, `provider_within` | as `status` | every command killed with its process group at the deadline, answered `Unknown` |
| `reach` | `validate` | no message holds the URL |
| `forward`, `forward_close` | `validate`; port 0 is `InvalidInput` | the adapter; close ends only a pid whose command line still names the port |
| `load_image`, `cached_image` | `validate`; `docker` only | `docker load -i`, answers `sha256:<64 hex>` |
| `copy_in`, `copy_out` | `validate`; the platform's `guest` | one `tar` stream in, `cat` out to a new 0600 file |

`Status` is `{state, kernel, worker, free_disk, tool_version, down_reason}`:
`kernel` is what the provider said, `None` when it did not answer (the
Docker Engine is not running; `state` is then `Stopped`); `worker` is
whether the worker answered its hello, asked only when the kernel runs
(`Some(false)` with no endpoint, `None` on an in-process machine).
`Status::word` and `Status::reason` say it in the call surface's words: no
provider answer is `broken` `engine_down`; a VM's kernel state passes
through `with_worker`, where running with no worker hello is `broken`
`not_answering` ("the machine runs but its worker does not answer"), never
`stopped`; provider `Failed` is `broken` `failed`; `Missing` is `broken`
`missing`; else running is `ready` and stopped `stopped`
([`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs)).

### Spec

| Item | Value | Source |
| --- | --- | --- |
| `Spec` | `name`, `kind`, `vm_name` (the record's `vm_name` only, `layerfs-<id8>-<machine>`; empty for `native`, whose adapters do not read it; a record without one reads `broken` `outdated` and is refused `Failed` by `provider_spec`, fix `layerfs machine remove <machine>` (record only; the built-in `docker` is recorded again by the next call)), `cpus`, `memory`, `disk`, `image` (Sandbox image archive), `os_image` (the VM's own system), `state: Lasting {port, gateway}`, `worker`, `work_dir`, `tools` | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) |
| `WORKER_PORT` | 7070, the worker's port in the guest | same |
| Guest paths | `/layerfs/data` (data disk), `/layerfs/bundle`, `/layerfs/state`, `/layerfs/image` | same |
| `Tools::from_env` | `PATH`, then the fallback folders (macOS: Homebrew's, `/usr/local/bin`, Docker Desktop's; `LAYERFS_TOOL_FALLBACKS` replaces them), also every provider command's `PATH`; Docker socket `/var/run/docker.sock`, which the machine layer replaces with the docker machine's socket ([Tools and the docker socket](platforms.md#tools-and-the-docker-socket)) | same |
| `parse_size` | bytes or `K`/`M`/`G`/`T` (powers of 1024), optional `iB`/`B`; 0 refused | same |
| Record to `Spec` | an absolute-path `image` becomes `Spec::image`; `Spec::os_image` comes from `os_image`, or on `docker` from `image` | [`layerfs-machine/src/spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One crate-private sealed trait and one exhaustive match for behaviour; one match for facts; no registry or factory | [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs) `adapter`, [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) `profile` |
| Every refusal of a platform, host and Store kind comes from the one support table | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) `sandbox_allowed` |
| A spec and the bundle are checked before any effect | [`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |
| A create that fails before its VM exists removes what it made; after, it keeps the VM (`vm_exists`, recorded `failed`); `Unknown` keeps everything with an uncertain `op`. The worker host port the VM forwards (`worker_port`) is recorded as `lasting.port` with the claim, before the VM is made, so `machine start` can start a kept VM on it | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs), [`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs) |
| The home lock is never held while a VM boots; `start.lock` serialises start, stop, create and remove of one machine | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs), [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| One attempt; a readiness wait is not a retry | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) `ready`, [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) `run` |
| No guest mounts the host home: Lima `mounts: []`; docker mounts its data volume and the bundle only, never the Engine socket; apple-container mounts only its volume | [`lima/template.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/template.rs), [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs), [`apple_container/create.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/create.rs) |
| No Sandbox command can create a user namespace: VM guests set `user.max_user_namespaces=0` and read it back; every namespaces command runs under a seccomp filter (the docker worker relies on the filter alone) | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs) |
| A removed platform's record (`firecracker`, `microsandbox`, `ssh`) is shown, refused `Unsupported`, and `machine.remove` removes only its record | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `removed`, [`platform_policy.rs`](../../../../control/crates/layerfs-sandbox-backend/src/platform_policy.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Host-only build | `layerfs-platform` is `#![cfg(unix)]`; `layerfs-machine` builds its calls only with the `machine` feature, which the guest worker lacks | [`layerfs-platform/src/lib.rs`](../../../../control/crates/layerfs-platform/src/lib.rs), [`layerfs-machine/src/lib.rs`](../../../../control/crates/layerfs-machine/src/lib.rs) |
| Missing tool or host fact | probe pushes `tool:<cli>` or `host:<fact>`; create refuses `Unsupported` with `missing`; version query 10 s | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) |
| VM size | at least 1 CPU, 256 MiB memory, 1 GiB disk (`InvalidInput`) | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) `validate` |
| Names | machine and VM names are plain names; `work_dir` absolute (`InvalidInput`) | same |
| Provider calls | 30 s per call, 120 s create, 300 s boot | [`setup/vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| Guest command, copy | 60 s, 600 s | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| Host command output | 1 MiB kept per stream; exit polled every 1 ms growing to 20 ms; a timeout is `Unknown` | [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| Another process starting the machine | waits up to 600 s on `start.lock`, then `Unavailable` | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| Bundle lacks `layerfs`, `layerfs-daemon` or `layerfs-sandbox-helper`, or hash not hex | `InvalidInput`, `missing: bundle:<name>` | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) `check_bundle` |
| Errors | one `Code`: `Exists`, `InUse`, `Unsupported`, `Unavailable`, `Denied`, `Failed`, `Unknown`, `InvalidInput`, `NotFound`; with `missing` and `vm_exists` | [`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs) |
| Live `op` / dead uncertain `op` on the machine | `Busy` / `Unresolved` | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) |

## Demonstration

The platform tests run real lifecycle calls against stand-in tools (shell
scripts on `Spec.tools.path` that log and refuse): no VM, Engine or root.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| `layerfs-platform/tests/conformance.rs` | `cargo +1.85.1 test --locked -p layerfs-platform --test conformance` | macOS, Linux | over `Platform::ALL`: profile against the support rows and `SUPPORT.json`; each lifecycle call through its tool or fixed refusal; `reach`, `forward`; options against `Spec::validate` |
| `layerfs-platform/tests/machine_adapter.rs` | `cargo +1.85.1 test --locked -p layerfs-platform --test machine_adapter` | macOS, Linux | built-in machines refuse create and remove before any tool runs |
| `layerfs-platform/tests/machine_platform.rs` | `cargo +1.85.1 test --locked -p layerfs-platform --test machine_platform` | macOS, Linux | names, backend, route, bind and options per platform and host |
| `layerfs-machine/tests/bundle_preflight.rs` | `cargo +1.85.1 test --locked -p layerfs-machine --features machine --test bundle_preflight` | macOS, Linux | a bad bundle is refused before any effect |

## Pages

| Page | What it covers |
| --- | --- |
| [Platforms](platforms.md) | `native`, `docker`, `apple-container`, `lima`; the support table, `SUPPORT.json`, removed platforms and adding a platform |
| [Workers and reach](workers-and-reach.md) | Worker setup and protocol, the worker server, PostgreSQL `reach` and daemon forwards |
