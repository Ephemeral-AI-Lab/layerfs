# sandbox family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › sandbox

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `sandbox` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/sandbox`](../../../../../api/crates/layerfs-api/src/sandbox), [`control/crates/layerfs-control/src/sandbox`](../../../../../control/crates/layerfs-sandbox/src) |

## Calls

```text
 project.init --> create <project> <name> [machine]  (starts the daemon)
                     |
                     +--> list / get                (records, Workspaces)
                     |
                     +--> workspace.mount ... unmount  (up to 16 at once)
                     |
                     v
                  close <name>   (ends the daemon; clears an unresolved
                                  create or close)
```

Four calls, in help order
([`sandbox/mod.rs`](../../../../../api/crates/layerfs-api/src/sandbox/mod.rs)).
Each runs `layerfs_sandbox::family::sandbox`
([`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs)).
A Sandbox runs one project's Workspace daemon on a machine; its name is
unique across the home.

| Call | Arguments | Options | Returns | Where it runs |
| --- | --- | --- | --- | --- |
| [`create`](create.md) | `project` (Project), `name` (Name), `machine?` (Name) | `image` (Text) | the Sandbox record with `state`; `forward_failed` when a direct daemon forward did not open | its machine: here or `sandbox.host` in the worker |
| [`list`](list.md) | `project?` (Project) | `page_size`, `page_pos` (Int) | `{ items: [record], next_page_pos }` | caller's process, home records only |
| [`get`](get.md) | `name` (Name) | `page_size`, `page_pos` | the record with `state`, `workspaces`, `next_page_pos`, and `down_reason` when its machine is down | its machine or its daemon; starts nothing |
| [`close`](close.md) | `name` (Name) | `force` (Bool) | `{ kind: "Closed", sandbox, unknown, closed }`, or `{ closed, was_gone: true }`, or `{ closed, was, left? }` for a record with no id | its machine: here or `sandbox.host` in the worker |

The record is `SandboxRecord::to_sdk`
([`record.rs`](../../../../../control/crates/layerfs-sandbox/src/record.rs)):
its stored fields except `v`, `runtime_id` and `daemon`, plus the derived
`state` and, while an operation holds it, `op` as
`{ id, name, phase, started, uncertain }`
([`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs)).
Each Workspace in `get` carries `ws` (`<sandbox>:<name>` or
`<sandbox>:<path>`), `name` and `path`
([`get.rs`](../../../../../control/crates/layerfs-sandbox/src/get.rs)).

## Arguments and options

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a Sandbox name; `native` and `docker` are refused (they name built-in machines) | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `machine` | Name | default: a sqlite Store's machine, else `native`; a sqlite Store's Sandbox must run on the Store's machine | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `image` | Text | travels in the deploy settings, not as a top-level argument | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `page_size`, `page_pos` | Int | `PAGE`; clamped to 1..=1000 (default 50) and 0 or more | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) |
| `force` | Bool | `close` stops the Sandbox even with Workspaces mounted | [`close.rs`](../../../../../control/crates/layerfs-sandbox/src/close.rs) |

`create` claims the name under the home lock with an `op` intent
(`sandbox.create`, phases `claim`, `start`); `close` records
`sandbox.close` (phase `stop`) before it stops the runtime. Where a
Sandbox may run for its Store kind is the support table's
`sandbox_allowed` answer.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name, or `native`/`docker` as a Sandbox name | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchSandbox` | a missing Store, project or Sandbox | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) |
| `NotFound` | no machine of that name | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `Exists` | the Sandbox name is taken | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `Unsupported` | the machine cannot run a Sandbox of this Store kind, or the sqlite Store is on another machine | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `MountsActive` | `close` without `force` while Workspaces are mounted | [`serve.rs`](../../../../../control/crates/layerfs-sandbox/src/serve.rs) |
| `Busy`, `Unresolved` | another live operation holds the Sandbox or its Store; a gone owner's uncertain `op` | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer; a PostgreSQL server the machine cannot reach | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs), [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Unknown` | **uncertain:** the machine's answer to `create` or `close` was lost | [`create.rs`](../../../../../control/crates/layerfs-sandbox/src/create.rs), [`close.rs`](../../../../../control/crates/layerfs-sandbox/src/close.rs) |

After `Unknown` the record keeps its `op` with `uncertain: true` and a line
goes to `log/uncertain.jsonl`; later calls on the Sandbox are refused
`Unresolved`, naming `sandbox get` (shows it) and `sandbox close` (clears
it). See [error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

```text
 sandbox.create / close / get
   |  route::host_op: Store target, deploy::choose, place::on_machine
   v
 +-- Executor::Local ----> sandbox::dispatch::call(op) in this process
 |                          (host Sandbox code; lease on the Store held)
 +-- Executor::Worker ---> worker op "sandbox.host"
                            { op, store, project, store_record,
                              project_record, deploy, postgresql?, more }
                            route::in_worker on the machine's native
 sandbox.get on a forwarded Sandbox --> direct::list: its daemon, from here
 sandbox.list -------------------------> home records only
```

[`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs)
sends the records a guest needs with the request; a guest never reads the
home. For a PostgreSQL Store the caller resolves the password and sends the
URL only in that request, never in a record. `get` uses `reach` with no
machine start (P4): a machine that is down answers the record with
`workspaces: []` and `down_reason`. See the
[routing table](../../../control/routing/routing-table.md) and
[daemon channel](../../../control/routing/daemon-channel.md).

## Example

Not run; written from the call table.

```js
await layerfs.sandbox.create("shop/api", "tests", "docker", { image: "ubuntu:24.04" });
await layerfs.sandbox.close("tests", { force: true });
```

```python
layerfs.sandbox.create("shop/api", "tests", "docker", image="ubuntu:24.04")
layerfs.sandbox.close("tests", force=True)
```

```console
$ layerfs sandbox create shop/api tests docker --image ubuntu:24.04
$ layerfs sandbox close tests --force
```

## Related

- [workspace family](../workspace/README.md): the calls on a Sandbox's Workspaces.
- [Control: Families](../../../control/management/families.md) and [Routing table](../../../control/routing/routing-table.md).
- [Control: Sandboxes](../../../control/sandboxes/README.md) and [Control: Machines](../../../control/machines/README.md).
- [A Workspace session](../../../flows/workspace.md).

---

← [file.apply](../file/apply.md) · [Up: SDK](../README.md) · [sandbox.create](create.md) →
