# dsh-layerfs: per-project storage stats

The files from dsh-layerfs (the LayerFS 2.0 plugin for DeepSeek Harness) that
make up the project page's **Stats** tab. The paths below this folder match
the plugin's own tree. This is not the whole plugin, so it does not build on
its own: these files import the plugin's other modules (`api.ts`, `files/`,
`ui/`, `layerfs/cli.ts` and others).

## What the tab shows

LayerFS reports byte totals per Store only (`store stats`). `project stats`
counts entries in one saved tree. Per project, the only byte figures are each
Commit's counters: `new_canonical`, `new_stored` and `new_metadata` (see
`.agents/skills/layerfs-sdk-observe/references/storage-metrics.md`). Summed
over one project's Commits, they give the bytes that project's saves wrote
first. Summed over every project in a Store, they match the Store's
`store stats` totals (checked on a demo Store with two projects: 54.10 MB vs
54.11 MB canonical, 5.08 MB vs 5.09 MB stored).

The tab draws:

- **Headline:** the bytes packed by this project, its number of saves, and how
  much smaller that is than its raw snapshots.
- **Staged flow:** saved snapshots, then new unique content (deduplication),
  then packed bytes (compression). `new_metadata` is part of `new_stored`.
- **Share of the Store:** this project's packed bytes against the Store's
  `stored` total, beside the Store file's size.
- **Facts:** main's latest tree (`project stats`), saves, merges and
  branches, and publications to main.
- **Packed bytes over history:** the cumulative sum of `new_stored`.
- **What each save added:** `new_stored` for the newest 30 saves.

## How it reads them

No LayerFS call sums the counters, so the host keeps a ledger per project
(`src/host/routes/project-storage.ts`), with one row per Commit id (a Commit
never changes).

- **A refresh:** lists the Branches, walks each moved Branch newest first
  until it reaches a Commit the ledger already has, and reads only the new
  Commits with `branch.get_commit`. It also counts main's publications.
- **The first read:** runs in the background. For a project with 10,056
  Commits it takes about two minutes, and the route reports progress while it
  runs.
- **The route:** `GET /api/projects/<key>/storage` (`?refresh=1` forces a
  refresh).

It replaces the project storage-report analysis and the `storage-history` and
`storage-report` routes. Those needed `project.storage_report`, `store.storage_report` and
`branch commits --include-storage`, which the pinned LayerFS CLIs do not
have.

## A LayerFS call would make it cheap

A `project.storage(project)` call that returns these sums, kept as running
per-project totals the way the Store totals behind `store stats` are kept,
would replace the ledger. An `include_storage` option on `branch.commits`
would let the bars be read in one call.
