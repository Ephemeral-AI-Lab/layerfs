# file.blame

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › blame

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows, for each line of a file, the Commit that last changed it, walking first parents back from a Branch, `main` or a Commit.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/blame.rs`](../../../../../api/crates/layerfs-api/src/file/blame.rs) |
| Implemented in | `layerfs_store::calls::file_blame` → `file`, Store method `fs.blame` ([`methods/file/blame.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/blame.rs)); `from_line` becomes `fromLine` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.blame(project, branch, path, commit?, { from_line });
```

```python
# Python
layerfs.file.blame(project, branch, path, commit=None, from_line=None)
```

```bash
# CLI
layerfs file blame <store>/<project> <branch> <path> [<commit>] [--from-line N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | a regular file; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `from_line` | Int | 1 | the first line to blame; at most 2000 lines and 40 KiB per answer | [`read/blame.rs`](../../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |

## Returns

```text
 { kind: "ok", snapshot, root, path,
   lines: [ { n, text, line_truncated, clipped, commit, message } ],
   origins: [], versions, comparison_coarse, next_line? }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `n`, `text` | int, text | line number and text | [`read/blame.rs`](../../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `commit`, `message` | text | the owning Commit and its message | same |
| `versions` | int | first-parent revisions visited | same |
| `comparison_coarse` | bool | some step compared only equal byte prefix and suffix (a side was larger than the window) | same |
| `next_line` | int | the first line not in this answer | same |

```text
 tip file --> parent (Commit parent, or base Layer; Layer: source Commit,
              else parent Layer) --> ... until every line has an owner
   absent / not regular  -> remaining lines owned here, stop
   same content root     -> step back
   both texts in window  -> line LCS;  else coarse prefix/suffix
```

The saved history itself has no fixed depth limit. Per-request work admission is described below.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.blame        --------> snapshot: commit if given, else branch
                                      ("main" = head Layer); then reads by
                                      immutable root only
```

A read on the Store's read-only session: it never takes the publication
lane and never sees a live Workspace. PostgreSQL Stores and sqlite Stores
on `native` run in this process; other sqlite Stores in their machine's
worker (`store.call`), with nothing started. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project, path or option; a path outside a folder scope (`out of scope: path`) | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the Branch, Commit or Layer is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; an unreachable PostgreSQL server | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `NoSuchPath`, `NotFile` | the path is missing or not a regular file | no | [`read/blame.rs`](../../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const b = await layerfs.file.blame("shop/api", "agent", "/src/app.ts", { from_line: 40 });
```

```python
for l in layerfs.file.blame("shop/api", "agent", "/README.md")["lines"]:
    print(l["n"], l["commit"][:12], l["text"])
```

```bash
layerfs file blame shop/api agent /src/app.ts --from-line 40
```

## Related

- After: [commit.get](../commit/get.md) for an owning Commit.
- [Engine: reads, blame](../../../engine/store-calls/reads.md#blame).

---

← [file.grep](grep.md) · [Up: file](README.md) · [file.diff](diff.md) →

## Typed owners and bounded windows

Add `line_count` (1..2000, default 2000) alongside existing `from_line`. The result
includes `at: { id, kind: "Commit" | "Layer", branch, root }`; reuse `at.id` as the
existing saved-version argument on later windows. Each line adds nullable
`owner: { ref, message }`, with a full typed Commit/Layer ref, and `origins`
deduplicates returned owners. Existing commit/message fields remain. Owner branch
names are recorded facts or null; no human author is inferred.

`coverage` reports status, exact/coarse comparison, and reasons such as
`history_budget` or `line_clipped`. Unknown owners remain null; unresolved lines
are never assigned to the tip. Content's LCS fallback is coarse, including when
text windows themselves fit. A published line follows its source Commit; Init
can own lines as a Layer. Publication time is not line ownership.

The tip window streams only the requested/admitted prefix; a large total file
size does not itself refuse a small first-line query. A request admits 10,000
ancestry visits and 64 MiB logical payload work across its window and ancestor
comparisons. An ancestry comparison that cannot fit leaves unresolved owners
unknown with partial coverage. If the byte budget ends before the requested line,
return `Capacity` rather than an exhausted empty answer. Clipped lines remain
explicit. Responses admit complete line and deduplicated origin records together
within 48 KiB, accounting for repeated messages and JSON escaping; `next_line`
identifies the first unreturned line. A mandatory line/owner that cannot fit
refuses with Capacity. See [saved queries](../saved-queries.md).
