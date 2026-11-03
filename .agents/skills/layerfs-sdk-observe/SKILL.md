---
name: layerfs-sdk-observe
description: Use when observing, measuring or troubleshooting a LayerFS 2.0 run rather than doing the work itself. Activates on reading a project's history and per-Commit storage (`commit.list`, `commit.get`, `commit.diff`, `branch.diff`), Store and machine state (`store.status`, `store.get`, `machine.status`), Workspace state (`workspace.status`, `sandbox.get`), the call trace settings (`LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START`) and old 1.1.0 trace files, daemon logs, or questions about what 1.1.0's `@layerfs/backend` reads became.
---

# Observing a LayerFS run

> **Status:** Current general guide for LayerFS 2.0 (#533, branch `issue533-v2`).
> For whoever watches or troubleshoots a run (an orchestrator, a maintainer),
> not for an agent doing the work; that is the
> [`layerfs-js-sdk`](../layerfs-js-sdk/SKILL.md) skill. 1.1.0's
> `@layerfs/backend` package is deleted; its 14 calls were dropped from 2.0's
> surface (O25 in [the decisions log](../../../core/docs/issues/533/DECISIONS.md)).
> Results quoted here were captured from the built `layerfs` command in a
> private home on 2026-11-02, unless marked "from source".

Everything here is a read: it changes no project. 2.0 has no separate
diagnostic package; the reads are ordinary calls of the nine families, open to
every caller.

| Question | 2.0 source | Page |
| --- | --- | --- |
| What did the run do, step by step? | `commit.list` per Branch, `commit.get`, `commit.diff`, `branch.diff`, `file.blame` | [reads](references/reads.md) |
| Is storage deduplicating and compressing over a long history? | Per-Commit `logical`, `new_canonical`, `new_stored`, `new_metadata` on Commit records; `store.status` for the file size. Store totals have no 2.0 call | [storage metrics](references/storage-metrics.md) |
| Is a Workspace behind its Branch, what does it hold uncommitted? | `workspace.status`, `sandbox.get` | [reads](references/reads.md#workspaces-and-sandboxes) |
| Where do Stores and Sandboxes live, are machines up? | `store.list`, `store.get`, `store.status`, `machine.list`, `machine.status`, `sandbox.list` | [reads](references/reads.md#stores-and-machines) |
| How long did each call take, which phase was slow? | **No 2.0 source yet.** The trace settings are reserved, but this build writes no trace | [tracing](references/tracing.md) |
| What did the Store, SQLite, locks and FUSE do during a call? | A `native` Sandbox's `daemon.log` file (from source); no call reads it | [daemon logs](references/daemon-logs.md) |

## What 2.0 cannot tell you

Said plainly, so nobody looks for it:

- **Call timing and phase trees.** 1.1.0's trace line writer lived in
  `@layerfs/agent` and was deleted with it. No 2.0 code reads `LAYERFS_TRACE`;
  a traced `layerfs file read` wrote nothing (checked). See
  [tracing](references/tracing.md).
- **Store totals:** canonical, stored, metadata, dedupe and compression ratios
  (`backend.store.project.get_storage`). `store.status` gives only the file and
  WAL sizes.
- **Per-file history and storage** (`store.fs.list_revisions`,
  `store.fs.get_storage`), **comparable records** (`get_comparable`),
  **the project's change feed** (`get_history`) and **Layers `main` skipped**
  after the fact (`list_overwritten`; only `branch.publish`'s own result lists
  them).
- **A list of `main`'s Layers.** `commit.list` refuses `main`
  (`NoSuchBranch`); `branch.list` shows only its newest Layer as `base`.
- **A Workspace's changed paths and overlay bytes**
  (`daemon.workspace.list_changes`, `get_storage`). `workspace.status` gives
  counts; an unmount refused `Dirty` lists the paths.
- **Background processes** (`daemon.process.start`, `list`, `cancel`).
- **A Sandbox's daemon log through a call** (`daemon.sandbox.get_log`).
- **An event feed** (`events.subscribe`): O26 makes trace files under the
  home's `trace/` the feed, but nothing writes them yet.

## Collecting a run

For an agent run you want to analyse afterwards:

1. Record identities: LayerFS commit, the `layerfs` binary or addon build, the
   daemon bundle, and `LAYERFS_HOME`. (`layerfs --version` still prints
   `1.1.0` in this build.)
2. At the start and end, save `store.status` and `store.get` for the project's
   Store, and `branch.list` for the project.
3. After the run, page `commit.list` for every Branch and save the pages: each
   Commit record carries its message, time, `via` and storage counters. Join by
   Commit id.
4. Before you close the run's Sandboxes (`sandbox.close` deletes the folder),
   copy each `native` Sandbox's `daemon.log` if you need it
   ([daemon logs](references/daemon-logs.md)).
5. Ask the agent to put its step label in every Commit message (`step 12:
   npm test (exit 1)`); with no trace, the messages are the step record.

Treat every number as diagnostic: one run, no cache control. It is not a
benchmark measurement.

```js
// collect.mjs: node collect.mjs shop/api > run/commits.json
import layerfs from "layerfs";

const project = process.argv[2];
const out = {};
for (const b of (await layerfs.branch.list(project, { page_size: 1000 })).items) {
  if (b.name === "main") continue;                 // main is not a Branch
  out[b.name] = [];
  for (let pos = 0; pos !== null; ) {
    const page = await layerfs.commit.list(project, b.name, { page_size: 1000, page_pos: pos });
    out[b.name].push(...page.items);
    pos = page.has_more ? page.next_page_pos : null;
  }
}
console.log(JSON.stringify(out, null, 2));
```
