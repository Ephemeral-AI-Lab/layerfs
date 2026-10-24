# #428 UI and frontend: outside LayerFS

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract.

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Related: [README.md](README.md), [layerfs-agent.md](layerfs-agent.md),
[layerfs-backend-runtime.md](layerfs-backend-runtime.md),
[layout.md](layout.md).

LayerFS ships no UI and no HTTP. Its packages are JavaScript calls that take
data and return data. Each integration builds its own routes, authentication,
caching, view model and screens on those calls, the way dsh does today.
This document draws that line, lists what LayerFS gives a UI builder, and
shows dsh's routes as the worked example.

## 1. Direction

Raised by the owner in the #428 discussion (2026-10-23): UI APIs and HTTP are
out of LayerFS's responsibility. This document proposes how; the owner has not
yet accepted its details.

1. **No `@layerfs/frontend`.** dsh's `model`, `view` and `client` packages stay
   in dsh-layerfs. LayerFS ships three packages: `agent`, `backend`,
   `runtime`.
2. **No HTTP in `@layerfs/runtime`.** `runtime.http.create_handler` and
   runtime's `http/` folder are dropped. LayerFS serves no route and opens no
   listener for a UI.
3. **The worker is not a UI API.** It is a transport for SDK calls on macOS
   and Windows: loopback only, behind a secret, framed JSON, not HTTP
   ([README.md §4.2](README.md#42-worker-current-on-macos-and-windows-for-any-integration)).
   It stays.
4. **LayerFS knowledge becomes SDK calls, not route code.** Where today's
   route code encodes a LayerFS rule (how a Layer compares, how a truncated
   snapshot is completed), that rule moves into an SDK call, so no
   integration has to copy it (section 4).
5. **The `AGENTS.md` UI rule is unchanged.** The LayerFS UI is still the dsh
   plugin. It is now built entirely in dsh-layerfs, on the public SDK.

## 2. The line

| LayerFS provides | The integration owns |
| --- | --- |
| `@layerfs/agent`, `@layerfs/backend`, `@layerfs/runtime`: calls returning records and pages | URLs, methods, query parsing, request bodies |
| The versioned method list: names, arguments, results, refusal codes | Mapping refusal codes to HTTP status codes |
| The trace-line format and `runtime.events.subscribe` | Pushing events to a browser (SSE, WebSocket, polling) |
| Contract facts a UI can rely on (section 5) | Caching, dedup and timeouts built on those facts |
| The worker on macOS and Windows | Sessions, cookies, origin checks, authentication |
| | Who made a change, as the integration knows it (sessions, agents) |
| | View model, labels, layout, screens, theming |

An integration's UI talks only to its own backend. Its backend talks to
LayerFS only through the three packages. Neither touches the Store file or a
daemon socket directly.

```text
browser ──HTTP──► integration backend ──► @layerfs/runtime ─┐
                  (routes, auth, cache)   @layerfs/backend ─┼──► @layerfs/agent ──► layerfs.node or the worker
                                          @layerfs/agent ───┘
```

## 3. What a UI can read and do

Every call below is a plain async function from the three packages. None of
them is shaped for one screen.

| A UI wants | Calls |
| --- | --- |
| Settings, Docker, the worker, disk | `runtime.status` |
| Start or restart the worker | `runtime.worker.start`, `runtime.worker.restart` |
| The projects in a home | `runtime.project.list` |
| Create a project and follow its Init | `runtime.project.start_init`, `runtime.project.get_init` |
| Remove a project | `runtime.project.remove` |
| One project's live picture: History, Sandboxes, Workspaces | `runtime.project.get_state` |
| History alone, starting no Sandbox | `backend.store.project.get_history` |
| A folder page, a file window, line blame, a file's line diff | `shop.fs.list`, `shop.fs.read`, `shop.fs.stat`, `shop.fs.blame`, `shop.fs.diff_files` |
| Paths that changed between two saved ids | `backend.store.project.get_comparable` (new, section 4) then `shop.branch.diff_commits` |
| Store totals and what each Commit cost | `backend.store.project.get_storage`, `backend.store.branch.list_commit_storage` |
| A project's Sandboxes, Workspaces and processes | `shop.project.list_sandboxes`, `shop.sandbox.list_workspaces`, `backend.daemon.process.list` |
| Unmount or discard a Workspace, stop a process | `shop.workspace.unmount`, `backend.daemon.process.cancel` |
| Clean up stale or leftover Sandboxes | `runtime.sandbox.reclaim`, `runtime.sandbox.clean_leftovers` |
| Live calls as they happen | `runtime.events.subscribe` |

`runtime.project.get_state` returns LayerFS records: Branches, Commits, Layers,
Sandboxes and Workspaces under LayerFS's own ids. It carries no display labels
(dsh's `C1`, `L0`, `sb_01` come from its `Mirror`) and no agent attribution.
Those belong to the integration.

## 4. Rules that move out of route code

Two pieces of today's dsh Host code encode LayerFS behaviour. Copying them
into every integration would let them drift, so they become SDK behaviour.

**Comparing a Layer.** `shop.branch.diff_commits` takes Commits, and the Files
pane compares saved ids, which may be Layers. dsh's
[reads.js](../../../../dsh/packages/host/src/reads.js) (`asCommit`) maps a
Layer to the Commit with the same files: a listed Commit with the same root,
else the Commit it was published from, which must then still have that root
(a publish that merged has other files). An Init Layer has none and is refused
`NotComparable`. Proposed call:

```js
await backend.store.project.get_comparable(id);   // + { commit, root, exact }; refuses NotComparable
```

**Completing a truncated snapshot.** Today's snapshot holds only each
Branch's newest Commits and names the Branches it cut in `more`. dsh's
`olderCommits` ([layerfs-client.js](../../../../dsh/packages/host/src/layerfs-client.js))
pages the rest with `branch.list_commits`. `runtime.project.get_state` either
completes this itself or documents `more` and the paging as part of its
result (open question 2).

What stays in dsh because it is policy, not a LayerFS rule:

- **Files read checks** (absolute path, id format, page size of 1000): the
  SDK already checks its arguments and refuses; dsh checks first to answer
  `400` before a call.
- **Binary fallback**: `fs.read` refuses `Binary`; dsh then answers the size
  from `fs.stat`.
- **Storage summary** ([storage.js](../../../../dsh/packages/host/src/storage.js)):
  which Branches and how many Commits to show is a screen choice.

## 5. Contract facts a UI can build on

These are LayerFS guarantees, stated in the method list. The integration
chooses what to do with them.

- **Saved ids never change.** A read by Commit or Layer id always returns the
  same answer, so it can be cached forever. dsh keeps the newest 500 Files
  reads ([trees.js](../../../../dsh/packages/host/src/trees.js)).
- **Store reads start no Sandbox.** `backend.store.*`, `shop.fs.*` on saved
  ids and `shop.branch.*` reads answer from the Store alone.
- **Daemon reads need the Sandbox running.** `backend.daemon.*` and the
  Workspace half of `get_state` ask running Sandboxes and can be slow or fail
  if one stops. dsh runs one such read per project at a time and waits 4 s
  before drawing History without Workspaces (`late`).
- **Reads are bounded.** A folder page holds at most 1,000 names; a file
  window at most 2,000 lines or 48 KiB. Nothing walks a whole tree, so a
  screen costs the same in a small project and a huge one.
- **A call is attempted once.** A lost reply is `Unknown`; the integration
  shows it and never resends a mutation
  ([README.md §3.3](README.md#33-calls-are-attempted-once)).
- **One client per project is enough.** A client keeps the Store and daemon
  connections open; dsh releases it after a quiet minute. Open connections do
  not keep an idle Sandbox from closing.
- **Trace lines hold ids, never file content.** Their format is in
  [api/how-to-use/backend.md, "The call trace"](../../../../api/how-to-use/backend.md#the-call-trace);
  dsh's `model` replays them into its view.

## 6. dsh as the worked example

Each dsh route maps onto calls from section 3. After #428 the routes, their
checks, caching and response shapes live in dsh-layerfs; LayerFS only answers
the calls. Routes from [route.js](../../../../dsh/packages/host/src/route.js),
under dsh's `/layerfs` prefix:

| dsh route | LayerFS calls behind it | dsh adds |
| --- | --- | --- |
| `GET status` | `runtime.status` | dsh settings, migration notes |
| `POST init`, `GET init/<job>` | `runtime.project.start_init`, `runtime.project.get_init` | the folder and workspace defaults |
| `GET projects` | `runtime.project.list` | each folder's default project |
| `GET project/<name>/state` | `runtime.project.get_state` | `Mirror` labels; who made each node, from its trace collector |
| `GET project/<name>/history` | `backend.store.project.get_history` | as `state` |
| `GET project/<name>/{list,read,blame,diff}` | `shop.fs.list`, `read`, `stat`, `blame`, `diff_files` | argument checks, kept reads |
| `GET project/<name>/changes` | `backend.store.project.get_comparable`, `shop.branch.diff_commits` | kept reads |
| `GET project/<name>/storage` | `backend.store.project.get_storage`, `shop.project.list_branches`, `shop.branch.list_commits` | which Commits to show |
| `GET project/<name>/runtime` | `shop.project.list_sandboxes`, `shop.sandbox.list_workspaces`, `backend.daemon.process.list` | image names |
| `POST project/<name>/runtime` | `shop.workspace.unmount`, `backend.daemon.process.cancel` | confirmation for discard |
| `POST project/<name>/remove` | `runtime.project.remove` | confirmation by name |
| `GET runtime`, `POST runtime/store`, `runtime/store/restart`, `runtime/leftovers` | `runtime.status`, `runtime.worker.start`, `runtime.worker.restart`, `runtime.sandbox.clean_leftovers` | every project's Sandboxes in one answer |
| `session/<id>/…`, `GET`/`POST workspace` | the same reads, for the session's project | sessions and folders: dsh only |

What stays in dsh-layerfs, at commit `cea713e767`, production LOC from
`tools/production_loc.py --files` (scope `dsh`):

| dsh code | Prod LOC | Role |
| --- | ---: | --- |
| `packages/view/src` | 2,672 | the LayerFS tab as DOM |
| `packages/client/src` | 1,978 | the dsh client plugin: panels, dialogs, API calls to its routes |
| `packages/model/src` | 960 | view model: `Mirror`, attribution, trace replay |
| Host `route.js` | 112 | the routes above |
| Host `trees.js`, `reads.js`, `storage.js` | 199 | per-project clients, kept reads, read checks, storage summary |
| Host `collector.js` | 213 | trace collection and per-session records |

Of these, `trees.js`, `reads.js`, `storage.js` and `route.js` were planned to
move into `@layerfs/runtime`'s `http/`
([layout.md §4](layout.md#4-layerfsruntime)). Under this proposal they stay.

## 7. Changes to the other #428 documents

These places were updated to match this proposal:

- [layerfs-backend-runtime.md](layerfs-backend-runtime.md):
  - §1 decision 1 and §2: three packages, no `@layerfs/frontend`; the diagram
    loses `◄──HTTP── frontend`;
  - §4.2: drop `runtime.http.create_handler`; reword `get_state` as "one
    project's live picture" rather than "the UI's view";
  - §6: `trees.js`, `reads.js`, `storage.js` and `route.js` stay in dsh;
  - §8: question 3 (HTTP authentication) closes.
- [README.md](README.md): §2 drops the HTTP handler, view-model and view from
  the integration kit; §5.2 and §6.1 lose the frontend; §6.3 becomes "dsh's
  routes" and points here.
- [layout.md](layout.md): §4 drops runtime's `http/` folder; the
  out-of-scope note no longer mentions `@layerfs/frontend`.
- [layerfs-agent.md](layerfs-agent.md): decision 1 lists three packages.
- [backend](layerfs-backend-runtime.md#31-backendstore-committed-history):
  `store.project.get_comparable` is added.

## 8. Open questions

1. **`get_comparable` or Layer ids in diffs.** A separate call keeps
   `shop.branch.diff_commits` unchanged. The alternative lets the diff calls
   accept Layer ids and refuse `NotComparable` themselves, which changes the
   agent surface but saves a call.
2. **`get_state` and truncated History.** Complete every Branch's Commits in
   `get_state` (one call, but it grows with History), or return the newest
   Commits with `more` and leave paging to the caller, as today.
3. **Non-JavaScript integrations.** With no HTTP from LayerFS, an integration
   in another language writes a small JavaScript server over the SDK or waits
   for an SDK in its language. Is that acceptable for #382 and #425?
4. **A shared UI kit.** If a second integration wants dsh's tab, dsh-layerfs
   can publish its `model`, `view` and routes as its own packages, outside
   LayerFS's release and versioning. Should that be planned now or when a
   second integration asks?
