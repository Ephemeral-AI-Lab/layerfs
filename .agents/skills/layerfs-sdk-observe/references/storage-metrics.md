# Storage metrics over a long history

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-sdk-observe` skill. The per-Commit counters and the Store
> totals were printed by the built `layerfs` command on a throwaway home on
> 2026-11-18. The meaning of each counter is unchanged from 1.1.0.

## Sources

| Source | Gives | When |
| --- | --- | --- |
| `commit.list(project, branch, { detail: true })` | Per Commit, newest first: `commit`, `created_at`, `message`, `via`, `size`, `new_canonical_bytes`, `new_stored_bytes`, `new_metadata_bytes` | Once after the run, for every Branch; dedupe by `commit` |
| `commit.get(project, commit, { detail: true })` | The same counters for one Commit | When one step looks odd |
| `store.get(store, { with: ["stats"] })` | Store totals: `size`, `logical_bytes`, `canonical_bytes`, `stored_bytes`, `metadata_bytes`, `file_bytes`, `reusable_bytes`, `dedupe`, `compression`, `init` | Snapshot at start, every N Commits, end |
| `store.list({ with: ["stats"] })` | Each Store's `size` only, from one stat | A cheap look at every Store |

```bash
$ layerfs store get shop --with live,stats
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"],"observed_at":"2026-11-18T22:05:32Z","stats":{"size":307200,"logical_bytes":199,"canonical_bytes":6815,"stored_bytes":3354,"metadata_bytes":278242,"file_bytes":307200,"reusable_bytes":28672,"dedupe":"0.029200","compression":"2.031902"}}
```

In that answer `dedupe` is `logical_bytes / canonical_bytes`
(199 / 6815 = 0.0292) and `compression` is `canonical_bytes / stored_bytes`
(6815 / 3354 = 2.0319); both are text with six decimals. `size` is the file
with its write-ahead log (or the PostgreSQL schema); `reusable_bytes` is
SQLite's free pages (null for PostgreSQL); `init` is the first project
Init's own counts, or null. A tiny project is dominated by metadata and
SQLite overhead, as here.

| Term | Meaning |
| --- | --- |
| logical | Bytes of regular files as written. Per Commit (`size`): the whole tree's size |
| canonical | Distinct registered object bytes, after content-addressed dedupe |
| stored | Pack bodies after compression and delta encoding |
| metadata | Metadata pack bodies |

`new_canonical_bytes`, `new_stored_bytes` and `new_metadata_bytes` describe
that Commit's Save. `new_stored_bytes` includes metadata-domain packs;
`new_metadata_bytes` is that subset, not additional bytes. Do not add them.
They are not per-path allocation; per-file physical bytes are not reported.

## The curve over history

Order a Branch's Commits oldest first. For Commit i:

- cumulative canonical C_i = Σ new_canonical_bytes, cumulative stored
  S_i = Σ new_stored_bytes, cumulative metadata M_i = Σ new_metadata_bytes,
  each over Commits 1..i of this run;
- `size_i` is the tree size at that Commit;
- the run's compression so far is C_i / S_i.

Take C_0, S_0 and M_0 from `store.get(store, { with: ["stats"] })` at the
start (`canonical_bytes`, `stored_bytes`, `metadata_bytes`) to place the run
on the Store's own curve, and check the end snapshot against them: the growth
of `stored_bytes` should be of the order of S_n; the growth of `size` adds
SQLite overhead. Publications and merges add records that no Branch's
`commit.list` attributes; report any gap rather than hiding it.

## Blow-up signals

| Signal | Healthy | Investigate when |
| --- | --- | --- |
| `new_metadata_bytes` per Commit vs paths changed in it | Grows with paths changed | It grows with tree size at a steady change size |
| `new_stored_bytes` per Commit vs bytes changed | Roughly bytes changed ÷ compression | Much larger than the change: a rewrite where a delta was expected |
| `new_canonical_bytes` on a re-install of unchanged `node_modules` | Near 0 (deduplicated) | Near the size of `node_modules` |
| Cumulative C_i / S_i over time, and the Store's `compression` | Stable | Falls steadily |
| Store `size` growth vs Σ new_stored_bytes | Close, plus overhead | Overhead grows out of proportion to Commits |
| Store `reusable_bytes` | Small | Large after many removals: `store.compact` reclaims dead bytes |

Paths changed per Commit: `commit.diff(project, commit)` (from its parent;
count `items`, skipping `type: "dir"` in the long form). Bytes changed:
`file.stat` sizes on both sides, or `size` deltas as a rough proxy.

## One row per step

Join the Commit records by `commit` and take the step from the Commit
message (or from the call log's `tag`):

| step (message) | via | created_at | size | new canonical | new stored | new metadata | cumulative C/S |
| --- | --- | --- | --- | --- | --- | --- | --- |

```bash
layerfs commit list shop/api agent --limit 1000 --detail | jq -r '.items | reverse | .[] |
  [.message, .via, .created_at, .size, .new_canonical_bytes, .new_stored_bytes, .new_metadata_bytes] | @tsv'
```

Page with `--cursor` (the answer's `next_cursor`) past 1000 Commits. Keep the
script and the saved pages with the run's evidence.
