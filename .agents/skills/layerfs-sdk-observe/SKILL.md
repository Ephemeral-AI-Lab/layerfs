---
name: layerfs-sdk-observe
description: Use when observing, measuring or troubleshooting a LayerFS 2.0 run rather than doing the work itself. Activates on reading a project's history and per-Commit storage (`layerfs.commit.list`, `layerfs.commit.get`, `layerfs.commit.diff`, `layerfs.file.history`), Store totals, dedupe and compression (`layerfs.store.get` with `with: ["stats"]`) and Store state (`layerfs.store.get` with `with: ["live"]`), machine state (`layerfs.machine.get` with `with: ["live"]`, `layerfs.machine.stats`), Workspace and Sandbox state (`layerfs.workspace.status`, `layerfs.workspace.list`, `layerfs.sandbox.get`), the home's call log (`layerfs.home.log`, `layerfs.home.clear_log`), daemon and worker logs (`layerfs.sandbox.log`, `layerfs.machine.log`), the doctors (`layerfs.home.doctor` and the others), the trace settings (`LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`) and old 1.1.0 trace files, or questions about what 1.1.0's `@layerfs/backend` reads became.
---

# Observing a LayerFS run

> **Status:** Current general guide for the LayerFS 2.0 call surface (83
> calls in 10 families). For whoever watches or troubleshoots a run (an
> orchestrator, a maintainer), not for an agent doing the work; that is the
> [`layerfs-js-sdk`](../layerfs-js-sdk/SKILL.md) skill, whose method pages
> hold every call's arguments and refusals. 1.1.0's `@layerfs/backend`
> package is deleted. Outputs quoted here were printed by the built `layerfs`
> command on a throwaway home on 2026-11-18 (macOS, `native`; ids shortened),
> unless marked otherwise.

Everything here is a read: it changes no project. 2.0 has no separate
diagnostic package; the reads are ordinary calls, open to every caller. Each
answers its short form; pass `{ detail: true }` (shell `--detail`) for every
key, and `with: [...]` for the sections a call lists.

| Question | 2.0 source | Page |
| --- | --- | --- |
| What did the run do, step by step? | `commit.list` per Branch (`main` lists its publications), `commit.get`, `commit.diff`, `file.history`, `file.blame` | [reads](references/reads.md) |
| How long did each call take, and did it fail? | `home.log` (the home's call log: `duration_ms`, `outcome`, `error` per call) | [tracing](references/tracing.md#the-call-log) |
| Which step of a call was slow? | `LAYERFS_TRACE` + `LAYERFS_TRACE_STEP` step lines (worker-routed calls in detail) | [tracing](references/tracing.md#the-step-lines) |
| Is storage deduplicating and compressing over a long history? | Per Commit `new_canonical_bytes`, `new_stored_bytes`, `new_metadata_bytes`; Store totals from `store.get(store, { with: ["stats"] })` | [storage metrics](references/storage-metrics.md) |
| Is a Workspace behind its Branch, what does it hold uncommitted? | `workspace.status` (changed paths, `branch_moved`), `workspace.list`, `sandbox.get` | [reads](references/reads.md#workspaces-and-sandboxes) |
| Where do Stores and Sandboxes live, are machines up? | `home.get({ with: ["tree"] })`, `store.list`, `store.get`, `machine.list`, `machine.get`, `sandbox.list` | [reads](references/reads.md#stores-and-machines) |
| Is anything broken in the home? | `home.doctor` and the other five doctors | [reads](references/reads.md#doctors) |
| What did the Sandbox's daemon or a machine's worker print? | `sandbox.log`, `machine.log`, `sandbox.files` | [daemon logs](references/daemon-logs.md) |

## What 2.0 cannot tell you

Said plainly, so nobody looks for it:

- **Daemon-side phase trees and counters** (1.1.0's `daemon` trace entries:
  SQL, lock, FUSE, Save counters per call). No front end traces a call's
  daemon exchanges; the call log has no daemon telemetry.
- **Per-file storage** (`store.fs.get_storage`), **comparable records**
  (`get_comparable`), **the project's change feed** (`get_history`) and
  **Layers a forced publish skipped**, after the fact (`list_overwritten`).
- **Background processes** (`daemon.process.start`, `list`, `cancel`).
  `sandbox.get` lists the commands running in each Workspace;
  `workspace.cancel` ends them.
- **An event feed** (`events.subscribe`).

## Collecting a run

For an agent run you want to analyse afterwards:

1. Record identities: LayerFS commit, the `layerfs` binary or addon build
   (`layerfs --version`), the daemon bundle (`home.get({ detail: true })`
   names it), and `LAYERFS_HOME`.
2. At the start and end, save `store.get(store, { with: ["stats"], detail:
   true })` for the project's Store, and `branch.list` for the project.
3. After the run, page `commit.list` for every Branch (and `main`) with
   `detail: true` and save the pages: each long row carries its message,
   `created_at`, `via` and storage counters. Join by Commit id.
4. Save the call log for the run: `home.log({ project, since, detail: true })`
   page by page, or a `tag` you set with `log_tag` on every call.
5. Before you remove the run's Sandboxes (`sandbox.remove` deletes the run
   folder), save each one's daemon log with `sandbox.log`
   ([daemon logs](references/daemon-logs.md)).
6. Ask the agent to put its step label in every Commit message (`step 12:
   npm test (exit 1)`), or tag its calls with `log_tag`.

Treat every number as diagnostic: one run, no cache control. It is not a
benchmark measurement.

```js
// collect.mjs: node collect.mjs shop/api > run/commits.json
import layerfs from "@layerfs/node";

const project = process.argv[2];
async function all(list) {
  const items = [];
  let cursor;
  do {
    const page = await list(cursor);
    items.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  return items;
}

const out = {};
for (const b of await all((cursor) => layerfs.branch.list(project, { limit: 1000, cursor }))) {
  out[b.branch] = await all((cursor) => layerfs.commit.list(project, b.branch, { limit: 1000, cursor, detail: true }));
}
console.log(JSON.stringify(out, null, 2));
layerfs.close();
```
