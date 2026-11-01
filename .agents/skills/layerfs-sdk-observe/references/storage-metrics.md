# Storage metrics over a long history

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-sdk-observe` skill. The per-Commit counters were seen live on
> Commit records on 2026-11-02. 1.1.0's Store totals
> (`backend.store.project.get_storage`) have no 2.0 call, so this page is
> narrower than its 1.1.0 version.

## Sources

| Source | Gives | When |
| --- | --- | --- |
| `commit.list(project, branch)` | Per Commit, newest first along first parents: `id`, `time`, `message`, `logical`, `new_canonical`, `new_stored`, `new_metadata` | Once after the run, for every Branch; dedupe by `id` |
| Write results (`file.*`, `workspace.commit`) | The same record in `commit`, at the moment of the call | During the run, free, if your harness keeps results |
| `store.status(store)` | `file.bytes`, `file.wal_bytes` of a sqlite Store | Snapshot at start, every N Commits, end |

**Not available in 2.0:** Store-wide `canonical`, `stored`, `metadata`,
`file`/`free` pages, `dedupe` and `compression`, and Init's own storage counts.

| Term | Meaning |
| --- | --- |
| logical | Bytes of regular files as written. Per Commit: the whole tree's size |
| canonical | Distinct registered object bytes, after content-addressed dedupe |
| stored | Pack bodies after compression and delta encoding |
| metadata | Metadata pack bodies |

`new_canonical`, `new_stored` and `new_metadata` describe that Commit's Save.
`new_stored` includes metadata-domain packs; `new_metadata` is that subset,
not additional bytes. Do not add `new_stored + new_metadata`. They are not
per-path allocation; per-file physical bytes are not reported.

## The curve over history

Order a Branch's Commits oldest first. For Commit i:

- cumulative canonical C_i = Σ new_canonical, cumulative stored
  S_i = Σ new_stored, cumulative metadata M_i = Σ new_metadata, each over
  Commits 1..i of this run;
- `logical_i` is the tree size at that Commit;
- the run's compression so far is C_i / S_i. A Store-wide dedupe ratio
  (logical / canonical over every record) cannot be rebuilt from these.

1.1.0 started these sums at the Store totals from a start snapshot (C_0, S_0,
M_0). 2.0 has no call for those, so the sums cover the run's Commits only, not
Init or earlier history. Say so in any report. Check the end against
`store.status`: the growth of `file.bytes + file.wal_bytes` over the run
should be of the order of S_n plus SQLite overhead; report any gap rather than
hiding it. Publication Layers and merges add records that no Branch's
`commit.list` attributes.

## Blow-up signals

| Signal | Healthy | Investigate when |
| --- | --- | --- |
| `new_metadata` per Commit vs paths changed in it | Grows with paths changed | It grows with tree size at a steady change size |
| `new_stored` per Commit vs bytes changed | Roughly bytes changed ÷ compression | Much larger than the change: a rewrite where a delta was expected |
| `new_canonical` on a re-install of unchanged `node_modules` | Near 0 (deduplicated) | Near the size of `node_modules` |
| Cumulative C_i / S_i over time | Stable | Falls steadily |
| `store.status` file + WAL growth vs Σ new_stored | Close, plus overhead | Overhead grows out of proportion to Commits |

Paths changed per Commit: `commit.diff(project, parent, id, { paths_only: true })`
(count `items`; skip `type: "dir"`). Bytes changed: `file.stat` sizes on both
sides, or `logical` deltas as a rough proxy.

## One row per step

With no trace in 2.0, join the Commit records by `id` and take the step from
the Commit message:

| step (message) | via | time | logical | new canonical | new stored | new metadata | cumulative C/S |
| --- | --- | --- | --- | --- | --- | --- | --- |

```bash
layerfs commit list shop/api work --page-size 1000 | jq -r '.items | reverse | .[] |
  [.message, .via, .time, .logical, .new_canonical, .new_stored, .new_metadata] | @tsv'
```

Keep the script and the saved pages with the run's evidence.
