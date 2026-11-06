# project.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [project](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.list` pages through the projects the home records, for every Store
or for one Store, without opening any Store.

| | |
| --- | --- |
| Family | [project](README.md) |
| Library entry | [`project/list.rs`](../../../../../api/crates/layerfs-api/src/project/list.rs) |
| Implemented in | `layerfs_control::project::family::run` → `list` ([`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs)) |
| Runs in | the caller's process, reading home records only |

## Signature

```js
// JavaScript
await layerfs.project.list(store?, { page_size, page_pos });
await client.project.list(store?, { page_size, page_pos });
```

```python
# Python
layerfs.project.list(store=None, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs project list [<store>] [--page-size <page-size>] [--page-pos <page-pos>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `store` (optional) | Name | a Store name (`StoreName::parse`); its record must exist, else `NotFound`. Omitted: every Store folder in the home. | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) `list` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | clamped to 1..=1000, never refused | `project/family.rs` `list` |
| `page_pos` | Int | 0 | items to skip; a negative value is taken as 0 | `project/family.rs` `list` |

## Returns

```text
 { items: [ { project, store, name, kind, machine, created }, ... ],
   next_page_pos: <int> | null }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `items` | list | one project summary each, ordered by Store name, then project name |
| `items[].project` | text | `<store>/<name>` |
| `items[].kind`, `items[].machine` | text | from the Store's record; `machine` is `native` when none is recorded |
| `items[].created` | int or null | when the home recorded the project; null when its record is missing or unreadable |
| `next_page_pos` | int or null | the `page_pos` of the next page; null after the last |

There is no `has_more` field here, unlike the Store-backed lists. The
summary is `ProjectRecord::to_sdk`
([`project/record.rs`](../../../../../control/crates/layerfs-store/src/project/record.rs)).

## Where it runs

```text
 caller's process
   store given? --> StoreName::parse, store/<store>/record.json exists?
   else         --> every folder under <home>/store/
   for each Store with a record:
     for each folder under store/<store>/project/:  summary
   skip page_pos, take page_size
```

The whole home listing is built, then paged in memory. No lease, worker,
Store file or daemon is involved, on any Store kind or platform. A Store
folder without a readable record is skipped. A project folder is listed even
when its `record.json` is missing (for example, the entry a failed local
[`project.init`](init.md) keeps): it then shows `created: null`. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `store` is not a valid Store name | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `NotFound` | `store` names a Store with no record | no | `project/family.rs` |
| `Failed` | a home folder cannot be read | no | `project/family.rs` (`failed`) |

The home's own refusals (layout, lock) come first; see
[refusals](../../call-table/errors.md#refusals).

## Examples

Not run; written from the call table.

```js
const page = await layerfs.project.list("shop", { page_size: 20 });
```

```python
page = layerfs.project.list("shop", page_size=20)
```

```bash
layerfs project list shop --page-size 20
```

## Related

- Before: [`project.init`](init.md), or `store.connect`, which records the projects a Store holds.
- After: [`project.get`](get.md), [`branch.list`](../branch/list.md).
- [Management: home and records](../../../control/management/home-and-records.md), [Control: families](../../../control/management/families.md#project).

---

← [project.init](init.md) · [Up: project](README.md) · [project.get](get.md) →
