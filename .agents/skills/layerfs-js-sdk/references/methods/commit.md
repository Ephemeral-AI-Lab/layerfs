# commit calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. New in 2.0 as a family; `list` and `diff` were
> 1.1.0's `branch.list_commits` and `branch.diff_commits`. Results were captured
> from the built `layerfs` command on 2026-11-02; ids are shortened.

```js
commit.list(project, branch, { page_size, page_pos })
commit.get(project, branch, commit)
commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor })
```

A Commit id is 66 hex characters and unique in its project. All three calls are
refused under a folder scope (`out of scope: history`).

## `commit.list`

`commit.list(project, branch, { page_size, page_pos })`: a Branch's Commits,
newest first along first parents. `page_size` is 1–1000, default 50.

```js
const log = await layerfs.commit.list("shop/api", "work", { page_size: 10 });
console.log(log.items.map((c) => `${c.id.slice(0, 12)} ${c.message}`).join("\n"));
```

Each item is a Commit record:

| Field | Meaning |
| --- | --- |
| `id` | Commit id |
| `branch` | Branch name |
| `parent`, `second_parent` | Parents (`second_parent` only for merges) |
| `base` | The `main` Layer underneath |
| `root` | Filesystem root id |
| `via` | What made it: `fs` for `file.*` writes, `exec` for a Workspace Commit |
| `message` | Commit message |
| `merged_from` | Merge source, for merge Commits |
| `time` | Commit time, ms since the epoch |
| `logical`, `new_canonical`, `new_stored`, `new_metadata` | Storage of this Commit (see the observe skill's [storage metrics](../../../layerfs-sdk-observe/references/storage-metrics.md)) |
| `displaced`, `lww`, `lww_truncated` | A forced Commit's displaced head; `/.lww` decisions |

`commit.list` on `main` is refused `NoSuchBranch`: `main` is a Layer chain,
and 2.0 has no call that lists its Layers. After an uncertain outcome
(`Unknown`, `Failed`), this is how you check whether a Commit happened.

## `commit.get`

`commit.get(project, branch, commit)`: one Commit of that Branch, as the record
above. It also takes an id prefix of 8 or more characters. A Commit that is not
on that Branch's first-parent line is refused `NotFound`
(`no Commit <id> on Branch <branch>`). It pages the Branch's history to find
the id, so it is slower on a long Branch.

```bash
layerfs commit get shop/api work 12c7ef811a
```

## `commit.diff`

`commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor })`:
files that differ between two Commits. No Branch is needed. Use
`branch.diff` for two Branch heads.

```js
const [newest, previous] = (await layerfs.commit.list("shop/api", "work", { page_size: 2 })).items;
const d = await layerfs.commit.diff("shop/api", previous.id, newest.id);
console.log(d.items.map((c) => `${c.kind} ${c.path}`).join("\n"));
```

```json
{"kind":"ok","from":{"commit":"12c7ef811a49…","root":"0a12a8d94231…"},
 "to":{"commit":"12a99d99faf5…","root":"f55fa9660a73…"},
 "items":[{"path":"/src/index.js","kind":"Modified","type":"file",
   "before":{"type":"file","inode":6},"after":{"type":"file","inode":6},
   "content":true,"identity":false,"metadata":true,
   "hunks":[{"op":"-","text":"export const retries = 3;"},{"op":"+","text":"export const retries = 1;"},{"op":" ","text":""}],
   "hunks_omitted":false}],
 "has_more":false,"next_page_pos":null,"next_cursor":null}
```

| Item field | Meaning |
| --- | --- |
| `path` | Changed path |
| `kind` | `Added`, `Removed` or `Modified` |
| `type` | `file`, `dir` or `symlink` |
| `before`, `after` | `{ type, inode }`, or `null` on the side where it is absent |
| `content`, `metadata`, `identity` | What changed: bytes, mode or attributes, file identity |
| `hunks`, `hunks_omitted` | Line diff for text files (`null` with `paths_only`) |

Folders show up as items too (an added folder is `Added`, a folder whose
entries changed is `Modified` with `metadata: true`). Resume a long diff with
`cursor: next_cursor`.

**Refusals** (seen live): `NoSuchCommit` for a full id that doesn't exist;
`InvalidInput` (`diff_commits takes two saved Commits`) for Branch names;
`NoSuchBranch` for a short id prefix, which is read as a Branch name. Pass full
ids.
