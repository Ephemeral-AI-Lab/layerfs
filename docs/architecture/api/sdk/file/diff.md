# file.diff

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › diff

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Compares two files line by line; each side is a path at a Branch, `main` or a Commit, so a file can be compared with itself across history.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/diff.rs`](../../../../../api/crates/layerfs-api/src/file/diff.rs) |
| Implemented in | `layerfs_store::calls::file::diff`, Store method `fs.diff_files` ([`methods/file/diff_files.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/diff_files.rs)); each side becomes `{ commit, path }`, `*_commit` winning over `*_branch` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.diff(project, left_branch, left_path, right_branch, right_path,
                        { left_commit, right_commit });
```

```python
# Python
layerfs.file.diff(project, left_branch, left_path, right_branch, right_path,
                  left_commit=None, right_commit=None)
```

```bash
# CLI
layerfs file diff <store>/<project> <left_branch> <left_path> \
  <right_branch> <right_path> [--left-commit ID] [--right-commit ID]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `left_branch`, `right_branch` | Name | a Branch or `main` per side | [`calls/file.rs`](../../../../../control/crates/layerfs-store/src/calls/file.rs) |
| `left_path` | Text | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `right_path` | Text | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `left_commit` | Name | none | a Commit or Layer id; replaces `left_branch` | [`calls/file.rs`](../../../../../control/crates/layerfs-store/src/calls/file.rs) |
| `right_commit` | Name | none | the same for the right side | same |

## Returns

```text
 { kind: "Equal" | "Text" | "Binary",
   left:  { path, snapshot, inode },
   right: { path, snapshot, inode },
   content_equal, same_inode, hunks_omitted, hunks: [ ... ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | text | `Equal` (same bytes), `Text` (line hunks), `Binary` (differs, not text, or over 1 MiB) | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `content_equal`, `same_inode` | bool | same bytes; same serial | same |
| `hunks`, `hunks_omitted` | list, bool | line hunks within 48 KiB, else omitted | same |

Whole-text comparison is bounded at 1 MiB per side (`MAXIMUM_TEXT_BYTES`).

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.diff_files   --------> snapshot: commit if given, else branch
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
| `NoSuchPath`, `IsDirectory` | a side's path is missing or is a folder (`detail.path`) | no | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const d = await layerfs.file.diff("shop/api", "main", "/src/app.ts", "agent", "/src/app.ts");
```

```python
layerfs.file.diff("shop/api", "agent", "/a.txt", "agent", "/a.txt", left_commit=older_id)
```

```bash
layerfs file diff shop/api main /src/app.ts agent /src/app.ts
```

## Related

- Whole trees: [branch.diff](../branch/diff.md), [commit.diff](../commit/diff.md).
- [Engine: reads, compare](../../../engine/store-calls/reads.md#compare).

---

← [file.blame](blame.md) · [Up: file](README.md) · [file.write](write.md) →

## Pinned content and attribution comparison

The five existing positional arguments and each `left_commit`/`right_commit`
override remain. Add options `include_attribution`, `allow_missing`, `page` and
`page_size`. Both heads resolve once; all returned facts use those exact roots.
`left_ref` and `right_ref` are `{ id, kind: "Commit" | "Layer", branch, root }`.
Reuse their IDs in the existing commit overrides on later pages.

`allow_missing` defaults false, preserving `NoSuchPath`. When true, exactly one
missing side succeeds as Added/Removed with `before` or `after` null. Both missing
sides still refuse. Invalid/inaccessible IDs and paths do not become missing-side
successes. Before/after facts contain type, inode, size and mode.

`changes` has independent `content`, `metadata`, `identity`, `attribution` fields.
The first three compare two present sides; absent sides use null facts and item
kind. Attribution is nullable: unrequested or unresolved is not false. Existing
`Equal`/`Text`/`Binary`, equality and hunk fields remain compatibility fields.
`omission_reason` separately distinguishes `binary`, `text_limit`, unsupported type
and omitted payload; oversized text is not silently asserted to be binary.

With explicit new options, `rows` returns stable line alignment, including equal
lines, in one-based page/page_size windows:

```text
{ index, op: " " | "-" | "+", left_line, right_line, text,
  text_clipped, before_owner, after_owner, owner_changed, attribution_coverage }
```

Row text retains its LF when present, matching blame's logical-line convention;
empty files have no invented line. `owner` is null or `{ ref, message }`, with a
typed full Commit/Layer reference. An unchanged exact aligned row with different
known owners has `owner_changed: true`; content additions/removals have false.
Unknown/coarse owner comparisons have null. This is persisted line provenance,
not human authorship. Publication preserves source Commit ownership.

`content_coverage` and `attribution_coverage` disclose complete/partial/unavailable/
not_requested states, exact/coarse comparison and reasons. Content's bounded LCS
fallback is explicitly coarse even when bytes are fully available. Binary or
oversized line attribution is unavailable; equal bytes/roots alone do not prove
same ownership. Identical saved identity and identical path do prove zero ownership
changes, including binary files. `attribution_changes` is returned only when the
complete exact comparison can establish a count.

Opted comparisons admit 10,000 aligned rows/ancestry visits and a shared 64 MiB
logical provenance-read budget, not a file-size limit. Mandatory row pages must fit
48 KiB including owners and escaped text, otherwise `Capacity`; reduce page_size.
No row is silently skipped. Partial ownership remains unknown. Legacy hunks can
be omitted with `hunks_omitted` when detail rows consume the response budget.
See [saved queries](../saved-queries.md).

New-option mode also admits at most 64 MiB of potential logical content-comparison
payload, separately from the 64 MiB provenance-read budget, before exact
different-root, equal-length regular-file comparisons. Equal
content roots and unequal lengths resolve without spending this payload budget.
If exact equality cannot be admitted, the call returns Capacity rather than
claiming unequal bytes from unequal roots. Calls without the new options retain
the legacy comparison behavior. This is per-request work admission, not a total
file-size limit: large same-root comparisons remain available.
