# sandbox.create

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [sandbox](README.md) › create

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Starts a named Sandbox for one project on a machine: a daemon process
(in its own namespaces on Linux) running that project's Workspace daemon, ready for
[`workspace.mount`](../workspace/mount.md).

| | |
| --- | --- |
| Family | [sandbox](README.md) |
| Library entry | [`sandbox/create.rs`](../../../../../api/crates/layerfs-api/src/sandbox/create.rs) |
| Implemented in | `layerfs_sandbox::family::create` → `create::create` ([`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs)), then `lifecycle::create` on the machine ([`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs)) |
| Runs in | the caller's process (records, locks), then the caller's process for a local machine or the machine's worker (`sandbox.host`) |

## Signature

```js
// JavaScript
await layerfs.sandbox.create(project, name, machine?, { image? })
await layerfs.client({ home }).sandbox.create(project, name, machine?, { image? })
```

```python
# Python
layerfs.sandbox.create(project, name, machine=None, image=None)
layerfs.client(home=...).sandbox.create(project, name, machine=None, image=None)
```

```bash
# CLI
layerfs sandbox create <project> <name> [<machine>] [--image <image>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; the Store and project must exist (`NoSuchStore`, `NoSuchProject`) | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs), [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `name` | Name | 1–63 lowercase letters, digits, `_ . -`, starting and ending with a letter or digit; not a Windows device name; not `native` or `docker`; unique across the home | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `machine` | Name, optional | default: a sqlite Store's machine, else `native` (PostgreSQL Store). A sqlite Store's Sandbox must run on the Store's machine; the support table's `sandbox_allowed` must accept the platform and Store kind | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `image` | Text | the machine record's image (not on `docker`, whose record's image is its worker container's); else the bundle's Sandbox image archive | travels in the deploy settings, not as a top-level argument. Only a digest `sha256:<64 hex>` is used; it names an image the Sandbox image archive holds (on `docker`, the bundle's `sandbox-image.tar` in the worker, as on a VM) | [`deploy.rs`](../../../../../control/crates/layerfs-sandbox/src/deploy.rs), [`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `start` |

Under a folder scope the call is refused `InvalidInput` (`out of scope:
sandbox.create`) ([`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs)).

## Returns

```text
 { name, project, machine, created,          from the claim
   id, platform, runtime, image, backend,    from the machine's answer
   route,
   state: "running",                         derived
   forward_failed?  }                        a direct forward did not open
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `name`, `project`, `machine` | text | as claimed |
| `created` | int | claim time, ms since the epoch |
| `id` | text | the name the machine knows the Sandbox by (its name) |
| `platform` | text | `docker` (the docker machine, D-8), `mac` or `linux`, implied by the deployment ([`runtime.rs`](../../../../../control/crates/layerfs-sandbox/src/runtime.rs) `platform`) |
| `runtime`, `backend` | text | `native` (macOS host process) or `namespaces` (Linux, including the `docker` machine's worker container) |
| `image` | text | the namespaces root's digest, or `local` |
| `route` | text | the kernel route: `nfs` for a macOS host process, else `fuse` ([`files.rs`](../../../../../control/crates/layerfs-sandbox/src/files.rs) `route`) |
| `state` | text | `running` once recorded ([`record.rs`](../../../../../control/crates/layerfs-sandbox/src/record.rs) `state`) |
| `forward_failed` | text | why the caller could not reach the daemon directly; its Workspace calls then go through the worker |

The record hides `v`, `runtime_id` and `daemon`
([`record.rs`](../../../../../control/crates/layerfs-sandbox/src/record.rs) `HIDDEN`).

## Where it runs

```text
 caller's process
  1 parse name; refuse native / docker
  2 home lock: resolve project, Store unblocked, pick machine,
    sandbox_allowed, Store's machine check, name free?
    claim sandbox/<name>/record.json with op sandbox.create (claim)
  3 op phase -> start (Sandbox lock only)
  4 route::on_machine(record, "sandbox.create", [image])
       |
       +-- Executor::Local --> dispatch::call --> lifecycle::create (here)
       +-- Executor::Worker -> "sandbox.host" --> worker: in_worker
                                 --> lifecycle::create, answer + keys
  5 direct::open: forward the daemon port (ssh, lima), before any
    lock; elsewhere (docker among them) calls stay on the worker
  6 home + Sandbox lock: record.started(answer), drop op, one write
```

| Machine | Who starts it | Sandbox | Source |
| --- | --- | --- | --- |
| `native`, macOS host | caller's process | daemon host process, loopback NFS | [`launch/local.rs`](../../../../../control/crates/layerfs-sandbox/src/launch/local.rs) |
| `native`, Linux host | caller's process | namespaces, FUSE | `launch/local.rs` |
| `docker` | the docker machine's worker, every host | namespaces, FUSE, inside the worker container (D-8); no direct route | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs), [`deploy.rs`](../../../../../control/crates/layerfs-sandbox/src/deploy.rs) |
| any other machine | that machine's worker | the guest's own `native` | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) `in_worker` |

For a PostgreSQL Store the caller resolves the password and sends the URL
only in this request; the side that launches runs a reach check first. The
reach check reads the schema's binding and finds the project (one
statement, no transaction, #96) in one read session, the process's held host session for that Store (#65:
[`postgresql/reader.rs`](../../../../../control/crates/layerfs-store/src/postgresql/reader.rs)),
opened with the light check if none is held, so the host side of a create
opens at most one PostgreSQL connection and no Store service. A project the
schema does not hold is `NoSuchProject` there, before anything starts. The
same session serves a status's head (a mount's caller reads nothing since
#64 option B); exec, Commit, unmount and close open none. The caller's Store lease covers each
call; the session holds none and closes after 60 s unused, or when the Store
is disconnected in that process.
The new daemon opens its own Store session, with the light check (one
statement on one connection), before it reports ready
([`application/owner.rs`](../../../../../engine/crates/layerfs-daemon/src/application/owner.rs)),
so a create makes 3 statements on 2 connections, or 2 on 1 when the
launching side already holds the session. After it reports ready the
daemon opens one idle Store reader (one more connection and light check),
which is not on the create's path. Each create starts a new daemon
process; with `sslmode=disable` it builds no TLS connector (#96, see
[PostgreSQL](../../../engine/store/postgresql.md)).
Nothing starts a stopped machine. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad name, `native`/`docker`, malformed image digest, folder scope | no | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs), [`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `NoSuchStore`, `NoSuchProject`, `NotFound` | missing Store, project or machine | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs), `create.rs` |
| `NoStore` | the project has no installed Store | no | `lifecycle.rs` `installed` |
| `Exists` | the name is taken in the home, or the machine already records a started Sandbox there (a record of the retired docker backend is forgotten instead, as `sandbox.close` forgets it, D-8) | no | `create.rs`, `lifecycle.rs` |
| `Unsupported` | platform cannot run this Store kind; sqlite Store on another machine; no image archive in the bundle | no | `create.rs`, [`deploy.rs`](../../../../../control/crates/layerfs-sandbox/src/deploy.rs) |
| `Busy`, `Unresolved` | the Store holds an `op` (push, pull, remove) | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | worker not recorded or not answering (nothing sent); a lock held too long; PostgreSQL server not reached | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs), [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Failed` | the start failed and every effect is known: the Sandbox was stopped and deleted, `detail {container, removed, log}` | no | [`launch/local.rs`](../../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `abandon` |
| `Unknown` | **the worker's answer was lost; or the start's effect is unknown; or it started but could not be recorded** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs), `launch/local.rs` `abandon` (its cleanup failed) |

After `Unknown` the record keeps its `op` with `uncertain: true`, a line
goes to `log/uncertain.jsonl`, and the state is `unresolved`. Read
[`sandbox.get`](get.md) before acting again; [`sandbox.close`](close.md)
clears it. Nothing is retried.

## Examples

Not run; written from the call table.

```js
const sb = await layerfs.sandbox.create("shop/api", "tests");
console.log(sb.state, sb.route);
```

```python
sb = layerfs.sandbox.create("shop/api", "tests", "docker", image="sha256:<64 hex>")
```

```console
$ layerfs sandbox create shop/api tests
```

## Related

- Before: [`project.init`](../project/init.md), [`machine.start`](../machine/start.md) for a VM machine, [`branch.fork`](../branch/fork.md).
- After: [`workspace.mount`](../workspace/mount.md), [`sandbox.get`](get.md), [`sandbox.close`](close.md).
- [Control: Sandboxes](../../../control/sandboxes/README.md), [backends](../../../control/sandboxes/backends.md), [Engine: Daemon startup](../../../engine/daemon/startup.md).
- [A Workspace session](../../../flows/workspace.md), [Routing table](../../../control/routing/routing-table.md).

---

← [SDK: sandbox](README.md) · [Up: sandbox](README.md) · [sandbox.list](list.md) →
