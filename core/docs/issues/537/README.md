# #537 LayerFS Connect on the #535/#536 shape: Storage, Compute and projects

> **Status:** Proposal; target LayerFS Connect with LayerFS 2.x; not a released contract.
> Second sub-issue of [#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535);
> **starts after [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536)'s terminal pass.**
> The code lives in
> [layerfs-connect](https://github.com/Ephemeral-AI-Lab/layerfs-connect) (branch
> `main`); this page is the work order. Written 2026-11-03 from four read-only
> reviews of Connect `main` (Compute page, Storage page, host and SDK
> integration, projects and navigation) against #535 and #536. Re-read
> Connect's `main` and LayerFS's final #536 commit before starting.

## 1. Outcome

Connect's Storage, Compute and project pages show and manage LayerFS exactly
as #535 and #536 shape it, through the SDK only.

- **Storage** shows each Store once, where its one live file is (sqlite) or
  "usable from every machine" (PostgreSQL), with its derived state and any
  operation in flight; moves a sqlite Store with **Push** (native → machine)
  and **Pull** (machine → native), and keeps bucket backups as a separate,
  point-in-time family.
- **Compute** shows machines with their platform facts from
  `machine.platforms`, their derived state and power (**Start**/**Stop**),
  the Stores placed on them and their Sandboxes, plus a **top-level
  Sandboxes** table.
- **Projects** show their Store's state and link project ↔ Store ↔ machine ↔
  Sandboxes ↔ Workspaces.
- **Nothing boots a VM by being looked at**: pages use list/get/status calls
  only and ask before starting a machine.
- **Every refusal is actionable**: `Busy`, `Unresolved` and `InUse` show the
  entity, phase and what is in the way (`detail.related`), and name the call
  that clears it.

Done means: the page designs of §5–§7 are implemented and documented in
`docs/ui/`; no Connect code calls `store.move`, guesses a state the SDK
provides, or hard-codes platform support; the terminal pass (§11) passes.

## 2. Settled decisions

| # | Decision |
| --- | --- |
| U1 | Connect reaches LayerFS only through the SDK (the `layerfs` Node package) and the `layerfs` CLI for `import-env`; it never opens a LayerFS home file, Store file or machine directory. (Already true; kept as a rule.) |
| U2 | UI vocabulary (§4) follows #535/#536: Push/Pull only for Store placement, Back up/Restore only for buckets, "platform" not "kind", "operation" for an SDK `op` and "job" only for Connect's own view of operations it started. |
| U3 | Pages never start a machine implicitly. Browsing a Store on a stopped machine shows "dev-lima is stopped" with **Start**; only after `machine.start` are Store calls made. |
| U4 | States are read from the SDK (`state`, `op`), never re-derived. |
| U5 | A move between two machines is one UI action that runs `store.pull` then `store.push` as two operations with two progress halves; if the push fails the Store stays on native and the UI says so. |
| U6 | Sandboxes are a top-level list (by name), as in the #536 home layout. |

## 3. Facts at the base

Connect `main`, read 2026-11-03.

- Connect links `layerfs` at `link:../layerfs-issue533-v2/api/node`
  (`package.json:35`); all SDK use goes through one `client({ home })`
  (`host/layerfs.js:16-19`); the CLI is used only for `import-env`
  (`host/layerfs.js:62-76`). No code reads `store.sqlite`, `record.json` or
  `worker.json`.
- `store.move` is called at `host/storage.js:330-335`,
  `host/machines.js:143-146`, `host/runs/folder.js:47-58`, and tests
  `host/test/{layerfs2,platform-real}.test.js`; UI entry points
  `storage-page.tsx:174-176`, `storage-dialogs.tsx:135-141`,
  `new-project-dialog.tsx:208-220,287-300`, `compute-page.tsx:112`.
- States are guessed: machine `ready/idle/starting/down` (`compute.ts:62-74`,
  `machines.js:32,102`), Store `moving` (`storage-page.tsx:33-38`,
  `host-api.ts:221`), Sandbox docker states and `reach`
  (`sandbox-list.tsx:27-42`, `projects.js:305-310`).
- Refusals: `server.js:16-41` has no `Unresolved`; `machines-table.tsx:98`
  drops the code; `storage-page.tsx:437` treats `Unknown` as "both copies
  were kept"; `detail.related` is never shown.
- Stale policy: PostgreSQL Sandboxes only on native/docker
  (`project-storage.tsx:283-288`, `projects.js:323`, `runs/folder.js:37,53`,
  `workspaces.js:86`); `project.init` only into Stores on native
  (`store-facts.tsx:64-67`, `storage-page.tsx:180-183`); a pinned O36 image
  digest as the Sandbox image default (`compute.ts:50`).
- 1.1 wording remains in `new-dialog.tsx` ("Load…"), `docs/ui/layerfs-projects.md`
  (SyncPill, "linked copy … next push"), `docs/ui/project-storage.md` (Docker
  volume path), `src/renderer/src/i18n/{projects,storage}.ts`, `README.md`
  (`vm:`, `placement`), `host/README.md` (`notePostgresql`, `storage-sync.js`,
  `sandboxContainer`), scenarios using `@layerfs/agent` and `backend.daemon.process.*`.
- Already consistent: `<store>/<project>` identity; sqlite Store on one
  machine; PostgreSQL out of machine inventory; backups separate, restore as a
  new Store on native; password references only; a job/progress model
  (`storage.js:292-328`, SSE) reusable for push and pull.

## 4. Glossary for UI copy

| Say | Means | Do not say |
| --- | --- | --- |
| Store | a Store: sqlite (one file on one machine) or PostgreSQL | remote, destination, sync target |
| machine / platform | a named machine / its kind (native, docker, lima, …) | runtime, placement, `vm:`, kind |
| project | `<store>/<project>`, data in a Store | — |
| Sandbox | compute bound to one project on one machine | container, VM, instance |
| Workspace | a mounted Branch in a Sandbox (optional name) | worktree |
| Push / Pull | sqlite Store placement: native → machine / machine → native | move, send, load, publish |
| Back up / Restore | Store ↔ bucket, point-in-time | push/pull, sync, copy |
| Connect / Disconnect | PostgreSQL Stores, buckets, ssh machines | link, remote add |
| Start / Stop | machine power | wake, boot |
| idle · in use · busy · down · unresolved | Store states | moving, syncing |
| creating · running · stopped · removing · failed · missing · unresolved | machine states | ready, idle, down |
| creating · running · closing · gone · unresolved | Sandbox states | exited, dead |
| operation | an SDK `op` in flight on an entity | job (only Connect's own) |

## 5. Storage page

```
Storage                                                        [New Store ▾] [Refresh]
┌ Operations (while any op or job is live) ───────────────────────────────────────────┐
│ Push app → dev-lima   check ✓ snapshot ✓ send 640 MB/1.2 GB ▸ verify install record  │
│ Pull big ← box        uncertain at record · see store status big · clear: store pull │
└──────────────────────────────────────────────────────────────────────────────────────┘
sqlite Stores                       (grouped by machine; native first)
  native                                                     running · 120 GB free
    docs   idle · 2 projects · 340 MB · backups: r2 ✓        [Push to…] [Back up] [Add project] [⋯]
  dev-lima (lima)                                            stopped [Start]
    app    down (machine stopped) · 1 project · 1.2 GB       [Pull to native] [Move to machine…] [⋯]
PostgreSQL Stores                   usable from every machine
  pg     in use (2 leases) · 3 projects · db.internal/main   [Add project] [Disconnect]
Buckets and backups                 point-in-time copies
  r2-main   12 files · listed 10:40   [List] [Disconnect]   file … [Restore as new Store]
Needs attention (layerfs doctor)    2 findings  [Show]
```

- **Rows** read `store.list` (each item has `state`, `op`) and `store.status`
  only when a row is opened; never a Store call while the machine is stopped.
- **Push to…** appears only on native, idle Stores; the dialog lists machines
  whose `supports.sqlite` is true, shows the target's free disk against the
  Store size, and lists what blocks it (Sandbox records, leases) from a
  pre-check or the refusal's `related`.
- **Pull to native** appears on Stores on a machine; **Move to machine…** runs
  pull then push (U5).
- **busy** rows show the op name and phase; **unresolved** rows show what
  `store.status` reports and a button for the clearing call the refusal names.
- **Add project** is offered on every sqlite Store (any machine, through its
  worker; starts nothing until confirmed if the machine is stopped, U3) and on
  PostgreSQL Stores.
- **Remove** for sqlite (typed name), **Disconnect** for PostgreSQL;
  `InUse` lists `related`.
- **Backups** keep today's states and "Restore as new Store on native"; copy
  says "point-in-time backup".
- **Empty states:** no Stores → "Create a Store on native, add projects, push
  it to any machine"; no buckets → connect one.

## 6. Compute page

```
Compute                                       [New machine ▾] [Refresh]
4 running · 2 stopped · 1 creating · ⚠ 1 needs you · 1 operation in flight
⚠ box: unresolved machine.create — see machine status box · clear: machine remove box
Machines
  Machine   Platform              State        Size          Stores   Sandboxes
  native    native · host_process running     —             docs     a1, a2      ▾
            (nfs) · sqlite ✓ · PostgreSQL ✓
  docker    docker · fuse         running     —             —        b1
  dev-lima  lima · namespaces     stopped     2 CPU · 4 GB  app      —      [Start]
  ▾ dev-lima: facts (tool 2.2.1, free disk 38 GB, image, worker version ✓),
              Stores on it [Pull to native], Sandboxes on it, [Stop] [Resize] [Remove]
Sandboxes
  Name  Project        Machine   Backend     Route  State     Workspaces
  a1    shop/web       native    native      nfs    running   2/16: task-42 (main), fix (fix-x)  [Close]
  b1    pg/api         docker    docker      fuse   creating  0/16
Add a machine                    (one row per machine.platforms entry)
  lima           ready · limactl 2.2.1                         [Create]
  firecracker    missing: host:apple-m3                         [How to]
  ssh            ready                                          [Connect]
```

- **Platform facts, readiness and form options** come from `machine.platforms`
  (`backend`, `kernel`, `sqlite`, `postgresql`, `ready`, `missing`, `options`,
  `description`); only marks and short blurbs stay in Connect.
- **Forms** render the platform's declared `options` (VM: cpus, memory,
  disk, image, nested for Lima, host for firecracker; ssh: host, user, port,
  identity, known_hosts, root). The image field defaults to "the bundle's
  image"; no pinned digest. Built-ins: Update (image, docker socket, WSL).
- **State** from `machine.list`; `op` overrides with its name and phase.
  `failed`, `missing`, `unresolved` count as "needs you".
- **Start/Stop** call `machine.start`/`machine.stop`; `Stop` while Sandboxes
  run shows `InUse` with them.
- **Sandboxes table** from `sandbox.list` (each item has `state`, `op`,
  `backend`, `route`); the row opens Workspaces with names; Close / Force
  close; an uncertain create shows its clearing call.
- **Remove/Disconnect** show `related` on `InUse`.

## 7. Projects

- **New project dialog:** pick any Store (sqlite on any machine, or
  PostgreSQL) or create a sqlite Store on native; after Init on native, offer
  **Push Store to…**. If the Store's machine is stopped, ask to start it (U3).
- **Project page banner** for its Store: `busy` (op and phase; writes and new
  Sandboxes refused), `down` (machine stopped → Start), `unresolved` (what
  clears it). Reads from records (`project.get`) still render.
- **New Sandbox:** machine optional. sqlite → the Store's machine (fixed);
  PostgreSQL → any machine whose `supports.postgresql_sandbox` is true
  (default native); unsupported ones are shown disabled with `reason`.
- **Sandboxes tab:** states and ops as on Compute; Workspaces show names;
  "Mount Workspace" takes an optional name. Chat Workspaces are mounted with
  `name: chat-<id>`.
- **Cross-links:** StoreBadge → Storage row; Store row → machine row on
  Compute; Sandbox tile → machine row and project Sandboxes tab; Compute Store
  rows → project pages.
- **Store line** shows name, machine (or "PostgreSQL"), size, state; no file
  path, no sync pill.

## 8. Host (`host/`) and types

In order:

1. Relink `layerfs` to the LayerFS build with #535 + #536 (the CLI parser,
   scope client and CLI shim read the call table and follow).
2. `store.move` → `store.push` / `store.pull`: `storage.js` jobs of kind
   `push`, `pull`, `move` (pull+push), routes `POST /api/stores/:name/push`,
   `/pull`, `/move`; `machines.js`; `runs/folder.js` (`placeRun`).
3. States: delete the guesses (`machines.js:32,102`, `projects.js:305-310`,
   `storage.js:88`); carry `state`, `op`, `supports`, `backend`, `route`,
   Workspace `name`/`ws` through `MachinesView`, `StoreRow`, `SandboxInfo`
   (`src/shared/host-api.ts`).
4. `machine.platforms`, `machine.start`, `machine.stop`: routes
   `GET /api/platforms`, `POST /api/machines/:name/start|stop`; `can_create`
   use removed.
5. Refusals: `server.js` maps `Unresolved` (409) and `Busy` (409, naming the
   op); job errors and dialogs carry `code`, `detail.entity`, `phase`,
   `related`. Connect's own per-host job guard is renamed so it is not
   confused with the SDK's `Busy`.
6. Drop the PostgreSQL native/docker restriction and the init-on-native
   restriction; drop the pinned image digest.
7. `layerfs.doctor()` → `GET /api/doctor`.
8. First-call conversion: an `InUse` from the home conversion (layout 2 → 3)
   shows a full-page "Upgrading this home: close Sandboxes a1, b1" with a
   Close action, instead of failing every page.
9. Polling: the list calls carry state, so the pages poll `machine.list`,
   `store.list`, `sandbox.list` (plus SSE for Connect's own jobs); no per-item
   status loops.

## 9. Docs and copy

Rewrite `docs/ui/layerfs-storage.md`, `layerfs-compute.md`,
`project-sandboxes.md`, `project-storage.md`, `layerfs-projects.md`,
`dialog-new-project.md`, `sidebar.md`; mark `storage-redesign.md` and
`storage-links.md` as describing 1.1 bucket sync, not placement; update
`docs/layerfs-2.0-integration.md` (call map, data movement, no-start/stop
note, docker image), `README.md`, `host/README.md`; regenerate i18n strings;
update or retire scenarios that use `@layerfs/agent`, `backend.*` or
`store move`; fix 1.1 fields in `host/test/{runs,heavy-queue}.test.js`.

## 10. Work order

One commit per step; each passes `pnpm typecheck`, `pnpm test:agents` and the
host tests (`host/test`).

1. Relink and types (§8.1, §8.3 types only).
2. Push/pull in host and UI (§8.2, §5 menus, U5), with tests.
3. States and operations (§8.3, chips, banners, notices).
4. Refusals (§8.5) and the conversion screen (§8.8).
5. `machine.platforms`, Start/Stop, forms from options (§6, §8.4).
6. Top-level Sandboxes table and Workspace names (§6, §7).
7. Policy relaxations (§8.6): PostgreSQL machines, Add project anywhere,
   image default.
8. Doctor (§8.7) and cross-links (§7).
9. Docs, copy, i18n, scenarios (§9).
10. Terminal pass (§11).

## 11. Terminal pass

On one Connect commit against one LayerFS build with #535 + #536, recorded
in `docs/live/<date>-535-ui.md` with screenshots:

1. Storage: create a sqlite Store on native, add a project, push it to a VM
   (progress shown), pull it back, move it VM → VM (two halves), back up and
   restore as a new Store; a push blocked by a Sandbox shows `related`.
2. Compute: create or connect one machine per available platform from
   `machine.platforms`; Stop and Start a VM; browsing its Store while stopped
   asks before starting; remove refuses with `related` while it holds a Store.
3. PostgreSQL: connect a Store; create Sandboxes on every machine whose
   `supports.postgresql_sandbox` is true; unsupported machines show `reason`.
4. Operations: a push started from the CLI shows as `busy` in Connect; a
   killed push shows `unresolved` with its clearing call.
5. Projects: new project into a Store on a VM (through its worker); project
   banner for `busy`/`down`/`unresolved`; named chat Workspaces.
6. Conversion: a 2.0 home with a running Sandbox shows the upgrade screen;
   closing it converts the home.
7. Copy: no "Move", "Load", "sync", "Kind", "remote" in the UI strings for
   placement; `grep` of host code finds no `store.move`.

## 12. Risks

- Connect must ship together with a LayerFS build that has #536 (the
  `store.move` removal is breaking); pin the link in one commit.
- A two-machine move leaves the Store on native if the push half fails; the
  UI says so and offers Push again.
- Polling list calls with `op` is cheap but not instant; a watch stream is
  deferred (#536 H8).

## 13. Not in this issue

Labels, a home-wide status call, a watch/event stream (#536 H8); per-machine
PostgreSQL URL overrides (#536 H9); desired-state files; Windows Docker (WSL2)
pages beyond showing it unsupported.
