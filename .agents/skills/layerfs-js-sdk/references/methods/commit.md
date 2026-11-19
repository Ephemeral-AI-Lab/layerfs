# commit calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. `commit.list` absorbs 1.x `branch.commits`,
> `commit.get` absorbs `branch.get_commit`, and `commit.diff` absorbs
> `branch.diff` and `file.diff` (no aliases). Outputs marked "seen" were
> printed by the built `layerfs` command on a throwaway home on 2026-11-18
> (ids shortened); the rest is from the table, the spec rows and the manual.

```js
commit.list(project, branch, { at, limit, cursor })
commit.get(project, commit, { branch })
commit.diff(project, ref, { from, path, limit, cursor, with: ["patch"] })
```

A Commit id is 66 hex characters and unique in its project. Where a call
takes a Commit it accepts the full id or a prefix of 8 or more characters,
resolved by the Store in one call: a prefix more than one Commit starts with
is `InvalidInput` with `matches` (a count), one no Commit starts with is
`NotFound`. Where a call takes a `ref` or `from`, a Branch of that name comes
first, then a Commit. Every id in an answer is the full id. All three calls
are refused under a folder scope (`out of scope: history`).

## `commit.list`

A Branch's Commits, newest first, from its head or from `at` (`NotFound` when
`at` is not in the Branch's history). On `main` it lists `main`'s
publications. `limit` is 1 to 1000, default 100.

Row short form: `{ commit }`, with `message`, `message_truncated` (a list cuts
the message at 128 characters; `get` answers it whole) and `source_branch` (a
publication's Branch) when set. The long row adds `branch`, `parent`,
`second_parent`, `base`, `root`, `via`, `created_at`, `merged_from`,
`source_commit`, `size`, `new_canonical_bytes`, `new_stored_bytes` and
`new_metadata_bytes`. `via` is how the Commit was made: `workspace`, `file`
or `merge`, and on `main` `publish` or `init`.

```bash
$ layerfs commit list shop/api agent --limit 2                                 # seen
{"items":[{"commit":"12d12fbd…","message":"Apply filesystem changes"},{"commit":"1244862d…","message":"Apply filesystem changes"}],"next_cursor":"eyJmb3Ii…"}
$ layerfs commit list shop/api main                                            # seen
{"items":[{"commit":"32220c54…","source_branch":"agent"},{"commit":"32919f55…"}]}
```

One long row, seen with `--detail`:

```json
{"commit":"12d12fbd…","message":"Apply filesystem changes","message_truncated":false,"source_branch":null,
 "branch":"agent","parent":"1244862d…","second_parent":null,"base":"32919f55…","root":"a6dd2e42…",
 "via":"file","created_at":"2026-11-18T22:04:16Z","merged_from":null,"source_commit":null,"size":16,
 "new_canonical_bytes":787,"new_stored_bytes":373,"new_metadata_bytes":334}
```

The `next_cursor` carries the Commit the first page started at, so a later
page lists the same history while the Branch moves on; it is `NotFound` once
that Commit is gone. After an uncertain outcome (`Unknown`), this is how you
check whether a Commit happened.

```js
const log = [];
let cursor;
do {
  const page = await layerfs.commit.list("shop/api", "agent", { limit: 1000, cursor });
  log.push(...page.items);
  cursor = page.next_cursor;
} while (cursor);
```

## `commit.get`

One Commit of the project, by id or prefix, without naming its Branch;
`branch` refuses `NotFound` unless the Commit is in that Branch's history.
Short: `{ branch, parent, message }`, each when set. The long form has the
list row's keys (no `message_truncated`) plus `displaced`, `overwritten` (the
paths a merge took from the side with the later write, the first 20),
`overwritten_count` (only when cut) and `units`. Not run in this pass.

```bash
layerfs commit get shop/api 12c7ef81 --detail
```

## `commit.diff`

The paths that differ between `from` and `ref`. Without `from` the side
compared from is a Branch's fork point, a Commit's parent (or, for a
Branch's first Commit, the base it was forked at), and for `main`'s first
publication the empty tree. `path` keeps only that file or folder and what
is under it.

Row short form: `{ path, change }` (`added`, `removed`, `modified`); the long
row adds `type` and `content`, `metadata`, `identity` (which of them differ).
`with: ["patch"]` adds each changed text file's lines as `hunks` (rows
`{ op, text }`, `op` one of `" "`, `"-"`, `"+"`) and `hunks_omitted`. The
answer's long form adds `from`, `to`, `from_root` and `to_root`. The
`next_cursor` carries both Commits the first page compared and the filters.

```bash
$ layerfs commit diff shop/api agent                                           # seen: since agent's fork point
{"items":[{"path":"/README.md","change":"modified"},{"path":"/docs/a.txt","change":"modified"},{"path":"/x.txt","change":"added"},{"path":"/y","change":"added"}]}
$ layerfs commit diff shop/api agent --with patch --path /docs/a.txt           # seen
{"items":[{"path":"/docs/a.txt","change":"modified","hunks":[{"op":" ","text":"one"},{"op":"-","text":"two"},{"op":"+","text":"three"},{"op":" ","text":""}]}]}
```

```js
// what one file looks like on two Branches (was file.diff)
const d = await layerfs.commit.diff("shop/api", "agent", { from: "main", path: "/src/index.js", with: ["patch"] });
for (const row of d.items) console.log(row.change, row.path, (row.hunks ?? []).filter((h) => h.op !== " ").map((h) => h.op + h.text).join("\n"));
```

Comparing two different paths (1.x `file.diff`'s left and right paths) is
gone; line ownership is `file.blame` on each side.
