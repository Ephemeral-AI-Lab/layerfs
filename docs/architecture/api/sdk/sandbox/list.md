# sandbox.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [sandbox](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists the Sandboxes this home records, or one project's, one page at a
time, from the home's records alone.

| | |
| --- | --- |
| Family | [sandbox](README.md) |
| Library entry | [`sandbox/list.rs`](../../../../../api/crates/layerfs-api/src/sandbox/list.rs) |
| Implemented in | the library entry parses a typed `ListRequest` → `layerfs_sandbox::family::list` → `list::list`, whose typed `Listed` it shapes ([`list.rs`](../../../../../control/crates/layerfs-sandbox/src/list.rs)) |
| Runs in | the caller's process; reads `sandbox/<name>/record.json` only |

## Signature

```js
// JavaScript
await layerfs.sandbox.list(project?, { page_size?, page_pos? })
await layerfs.client({ home }).sandbox.list(project?, { page_size?, page_pos? })
```

```python
# Python
layerfs.sandbox.list(project=None, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs sandbox list [<project>] [--page-size <n>] [--page-pos <n>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project, optional | `<store>/<project>`; keeps only records whose `project` matches. A project with no Sandboxes, or none recorded, gives an empty page | [`list.rs`](../../../../../control/crates/layerfs-sandbox/src/list.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | clamped to 1..=1000 | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) `page` |
| `page_pos` | Int | 0 | a negative value counts as 0 | `family.rs` `page` |

Under a folder scope the call is refused `InvalidInput`
([`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs)).

## Returns

```text
 { items: [ { name, project, machine, created,
              id?, platform?, runtime?, image?, backend?, route?,
              state,
              op?: { id, name, phase, started, uncertain } } ],
   next_page_pos: int | null }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `items` | list | one page of records, each `SandboxRecord::to_sdk(View::List)`: the stored fields but `v`, `runtime_id`, `daemon` |
| `items[].state` | text | `creating`, `closing`, `unresolved`, `gone` or `running`, derived from the record and its `op`; never asks a machine, so never `lost` (that needs the machine's answer: [`sandbox.get`](get.md)) ([`record/sandbox.rs`](../../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `state`) |
| `items[].op` | map | the in-flight or uncertain intent, when the record holds one ([`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) `to_sdk`) |
| `next_page_pos` | int or null | pass back as `page_pos`; null after the last page |

`list` reads no worker and no daemon, so `state` is `running` for any
started Sandbox even when its machine is down; [`sandbox.get`](get.md)
asks the machine.

## Where it runs

```text
 caller's process
   inventory::sandboxes(home)    one read of each sandbox/<name>/record.json
     -> filter by project
     -> to_sdk(View::List) -> page(items, page_size, page_pos)
```

No route decision is made: no machine, worker or daemon is contacted, for
any Store kind or platform. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `project` not `<store>/<project>`; wrong kind of a paging option; folder scope | no | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |
| the home's refusal | the home cannot be opened or migrated | no | [`calls/dispatch.rs`](../../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |

A list never clears an intent and has no uncertain outcome.

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.sandbox.list("shop/api");
```

```python
page = layerfs.sandbox.list(page_size=100)
```

```console
$ layerfs sandbox list shop/api --text
```

## Related

- Before: [`sandbox.create`](create.md).
- After: [`sandbox.get`](get.md) for live Workspaces, [`sandbox.close`](close.md).
- [Control: Home and records](../../../control/management/home-and-records.md), [Control: Sandboxes](../../../control/sandboxes/README.md).

---

← [sandbox.create](create.md) · [Up: sandbox](README.md) · [sandbox.get](get.md) →
