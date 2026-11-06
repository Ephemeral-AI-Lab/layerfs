# project.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [project](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.get` answers one project's summary from the home's records: its
Store, the Store's kind and machine, and when it was recorded.

| | |
| --- | --- |
| Family | [project](README.md) |
| Library entry | [`project/get.rs`](../../../../../api/crates/layerfs-api/src/project/get.rs) |
| Implemented in | `layerfs_control::project::family::run` → `"get"` arm ([`project/family.rs`](../../../../../control/crates/layerfs-control/src/project/family.rs)), `store::resolve::target` ([`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs)) |
| Runs in | the caller's process, reading home records only |

## Signature

```js
// JavaScript
await layerfs.project.get(project);
await client.project.get(project);
```

```python
# Python
layerfs.project.get(project)
```

```bash
# CLI
layerfs project get <project>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<name>`, checked by `names::project` before dispatch | [`calls/check.rs`](../../../../../control/crates/layerfs-control/src/calls/check.rs), [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |

## Options

None.

## Returns

```text
 { project: "<store>/<name>", store, name,
   kind: "sqlite" | "postgresql", machine, created: <ms> | null }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `project` | text | `<store>/<name>` |
| `store`, `name` | text | its two parts |
| `kind` | text | the Store record's kind; empty when the record names no kind LayerFS writes |
| `machine` | text | the Store record's machine; `native` when none |
| `created` | int or null | when the home recorded the project |

The answer is `ProjectRecord::to_sdk` with `View::Full`
([`project/record.rs`](../../../../../control/crates/layerfs-control/src/project/record.rs)),
the same shape as one [`project.list`](list.md) item. It holds no Branch,
Layer or size: read those with [`branch.list`](../branch/list.md).

## Where it runs

```text
 caller's process
   resolve::target(home, "<store>/<name>")
     store/<store>/record.json ............ else NoSuchStore
     store/<store>/project/<name>/record.json  else NoSuchProject
   summary(View::Full)
```

No lease, worker, Store file or daemon, on any Store kind or platform. The
answer says what the home records, not whether the Store still holds the
project. See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `project` is not `<store>/<name>` with valid names | no | [`calls/check.rs`](../../../../../control/crates/layerfs-control/src/calls/check.rs) |
| `NoSuchStore` | no record for the Store | no | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchProject` | no `record.json` for the project, including the entry a failed local Init keeps | no | `store/resolve.rs` |

## Examples

Not run; written from the call table.

```js
const p = await layerfs.project.get("shop/api");
```

```python
p = layerfs.project.get("shop/api")
```

```bash
layerfs project get shop/api
```

## Related

- Before: [`project.init`](init.md) or [`project.list`](list.md).
- After: [`branch.list`](../branch/list.md), [`project.remove`](remove.md).
- [Management: home and records](../../../control/management/home-and-records.md), [Control: families](../../../control/management/families.md#project).

---

← [project.list](list.md) · [Up: project](README.md) · [project.remove](remove.md) →
