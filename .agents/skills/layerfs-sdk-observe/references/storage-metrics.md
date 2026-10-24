# Storage metrics over a long history

> **Status:** Current general guide. Part of the `layerfs-sdk-observe` skill. The backend
> statistics and the trace `sizes` and `commit` fields (#333) are on `main`.

## Sources

| Source | Gives | When |
| --- | --- | --- |
| `backend.store.project.get_storage()` | Store totals: `logical`, `canonical`, `stored`, `metadata`, `file`, `free`, `dedupe`, `compression`, and `init`, the import counts and Store totals Init left | Snapshot at start, every N Commits, end |
| `backend.store.branch.list_commit_storage(branch, pageSize, pagePos)` | Per Commit, newest first along first parents: `commit`, `logical`, `newCanonical`, `newStored`, `newMetadata` | Once after the run, for every Branch; dedupe by `commit` |
| Trace `commit` and `sizes` | The same per-Commit counters on the line of the call that made the Commit, plus its step and time | During the run, free |

| Term | Meaning |
| --- | --- |
| logical | Bytes of regular files as written. Per Commit: the whole tree's size. Store: summed over every Commit and Layer record, counting each hardlink path |
| canonical | Distinct registered object bytes, after content-addressed dedupe |
| stored | Pack bodies after compression and delta encoding |
| metadata | Metadata pack bodies plus SQLite space outside pack bodies |
| file / free | Committed database pages / reusable free pages, in bytes |
| dedupe | logical / canonical (`null` when canonical is 0) |
| compression | canonical / stored (`null` when stored is 0) |

`newCanonical`, `newStored` and `newMetadata` describe that Commit's acknowledged
Save. `newStored` includes metadata-domain packs; `newMetadata` is that subset,
not additional bytes. Store `metadata` adds SQLite overhead to that same subset.
Do not add `stored + metadata` to derive physical space. The exact accounted
SQLite overhead is `file - free - stored`. They are not exclusive per-path allocation. Per-file physical bytes are not
reported.

## The curve over history

Order the Commits oldest first. For Commit i:

- cumulative canonical C_i = C_0 + Σ newCanonical, cumulative stored S_i = S_0 + Σ
  newStored, cumulative metadata M_i = M_0 + Σ newMetadata;
- `logical_i` is the tree size at that Commit;
- C_0, S_0 and M_0 come from the start snapshot of `store.stats()` (Init and anything
  before the run).

Then check the end snapshot: `store.stats().canonical` should be close to C_n. Report any gap rather than hiding it. Logical Store totals also include
publication Layers. Per-Commit `newMetadata` omits SQLite overhead growth, so
compare the metadata gap against the change in `file - free - stored`. Unrecorded
Saves, failed constructions or omitted histories can leave other gaps; preserve
that uncertainty. `store.commits("main")` refuses because main is a Layer chain.

## Blow-up signals

| Signal | Healthy | Investigate when |
| --- | --- | --- |
| `newMetadata` per Commit vs paths changed in that Commit | Grows with paths changed | It grows with tree size at a steady change size |
| `newStored` per Commit vs bytes changed | Roughly bytes changed ÷ compression | Much larger than the change: a rewrite where a delta was expected |
| `newCanonical` on a re-install of unchanged `node_modules` | Near 0 (deduplicated) | Near the size of `node_modules` |
| Store `dedupe` over time | Rises as history repeats content | Falls steadily |
| Store `compression` over time | Stable | Falls steadily |
| `file - free - stored` | SQLite overhead, separate from pack bodies | Overhead grows disproportionately to object and history counts |

Paths changed per Commit: `branch.diff_commits(parent, commit)` (count `items`; skip
directories). Bytes changed: sum the sizes of changed files from `fs.stat` on both
sides, or use `sizes.logical` deltas as a rough proxy.

## One row per step

Join the trace (by `step` and `commit`), `store.commits` (by `commit`) and the decoded
daemon records (by `scope`, see [daemon logs](daemon-logs.md)):

| step | method | ms | slowest phase | logical | new canonical | new stored | new metadata | cumulative dedupe | cumulative compression |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |

A short Node script over `trace.jsonl` and the saved `store.commits` pages is enough;
keep it with the run's evidence.
