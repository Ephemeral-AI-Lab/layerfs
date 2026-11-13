# logs.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [logs](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists the calls made against this home, newest first, from its call log
(`trace/calls.jsonl`, then `trace/calls.1.jsonl`).

| | |
| --- | --- |
| Family | [logs](README.md) |
| Library entry | [`api/crates/layerfs-api/src/logs/list.rs`](../../../../../api/crates/layerfs-api/src/logs/list.rs) |
| Implemented in | `layerfs_home::logs::read` ([`logs.rs`](../../../../../control/crates/layerfs-home/src/logs.rs)) |
| Runs in | the caller's process; reads two files of the home; starts no machine; writes no line |

## Signature

```js
// JavaScript
await layerfs.logs.list({ op?, project?, client?, tag?, outcome?, since?, cursor?, limit? });
```

```python
# Python
layerfs.logs.list(op=None, project=None, client=None, tag=None, outcome=None, since=None, cursor=None, limit=None)
```

```bash
# CLI
layerfs logs list [--op <family[.call]>] [--project <store>/<project>] [--client <c>] [--tag <t>] [--outcome <o>] [--since <ms>] [--cursor <next>] [--limit <n>]
```

## Arguments

None.

## Options

Every option given must match.

| Name | Kind | Rule |
| --- | --- | --- |
| `op` | Text | `family` or `family.call` (`file`, `file.write`; `-` and `_` are the same) |
| `project` | Project | lines whose `argv` names this project |
| `client` | Name | `cli`, `node`, `python` or `rust` |
| `tag` | Text | an exact tag, or a prefix ending in `:` (`dsh:s_4d2a:`) |
| `outcome` | Text | `ok`, or a refusal code |
| `since` | Int | lines whose `at` is at or after this time (ms since the epoch) |
| `cursor` | Text | `next` from a previous page |
| `limit` | Int | 1 to 1000; default 100 |

## Returns

```text
 { lines: [ { v, at, ms, client, argv, outcome, kind?, commit?, layer?, tag? }, ... ],
   next?: "<opaque>", has_more, skipped }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `lines` | list | the matching lines, newest first, as written ([the line](../../../control/management/home-and-records.md#the-call-log)) |
| `next` | text | present when `has_more`: pass it as `cursor` for the next page |
| `has_more` | bool | another matching line follows |
| `skipped` | int | lines read that do not parse (a torn tail after a crash); never answered |

Lines are in completion order; `at` is when a call started.

## Errors

| Code | When | Uncertain? |
| --- | --- | --- |
| `InvalidInput` | an option of the wrong kind; `limit` outside 1 to 1000; a malformed cursor; a stale cursor (the log was rotated or reset since: list from the start) | no |
| `Failed` | a log file cannot be read | no |

## Examples

```bash
layerfs logs list --project demo/app --op file.write --limit 50
LAYERFS_LOGS_TAG=dsh:s1:r1 layerfs store list && layerfs logs list --tag dsh:s1:
```

```js
const page = await layerfs.logs.list({ tag: "dsh:s1:" });
```

## Related

- [logs.reset](reset.md); [Home and records: the call log](../../../control/management/home-and-records.md#the-call-log).

---

[Up: logs](README.md) · [logs.reset](reset.md) →
