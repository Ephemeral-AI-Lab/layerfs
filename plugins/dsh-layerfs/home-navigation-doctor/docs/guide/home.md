# Home

> Images marked “isolated fixture data” show the current components rendered in a private browser with sample data. Other images come from the earlier demo setup.

> **Where:** `LayerFS › Home` · **Opens from:** the sidebar's **LayerFS** row, the first crumb of any Center page, Jump to's Pages · **Source:** see the end

Home is Center's first page: everything this LayerFS home holds, at once. Come here to see
what needs you, to find a project, Store or machine, and to add a new one.

![Home, numbered (current UI, isolated fixture data)](img/home-overview.png)

*Isolated fixture data.*

| # | Component | What it is |
|---|---|---|
| 1 | Nav bar | [Components › Nav bar](components.md#nav-bar): Back, Forward, the crumbs (just **Home** here), **Jump to…**, **Add** |
| 2 | Head card | [Components › Head card](components.md#head-card): LayerFS × DSH, the runtime, what needs your attention, and the preset and Refresh buttons |
| 3 | Tabs | **Projects** *n* · **Stores** *n* · **Machines** *n*, each with a count and a coloured dot when something in it needs you |
| 4 | Search and sort | Filters the projects table and orders it |
| 5 | Project row | One project: its Store, sessions, Sandboxes, last session and commit, size |

## Layout

The [nav bar](components.md#nav-bar) runs across the top of every Center page. Under it,
the [head card](components.md#head-card) says whether anything needs you, then three tabs:
Projects, Stores and Machines. Each tab's action (**New project**, **New machine**) sits at
the right end of the tab bar. Home remembers nothing between visits except the Stores tab's
kind and view and which rows are open.

Home reads the Storage view (`GET /api/storage`: `home get`, `project list`, `store auths`,
`store list`) and the project catalog (`GET /api/projects`: `project list`) when it opens,
again every 15 s while it is visible, and after changes. Storage and project notifications are coalesced over 500 ms. Project changes refresh the catalog as well as storage. A failed action shows a red bar under the head card with **Dismiss**.

## Head card and notices

![The head card (current UI, isolated fixture data)](img/home-head-card.png)

*Isolated fixture data.*

The head card is a shared component; its full table is in
[Components › Head card](components.md#head-card), and the notices it lists are in
[Components › Needs-you notices](components.md#needs-you-notices).

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 LayerFS × DSH | The LayerFS × DSH wordmark | — | — (static) | — |
| 2 Runtime | `LayerFS 2.0.0 · 12345678`, the pinned build | Press → opens [Set up LayerFS](entry-points.md#set-up-layerfs) | `--version --json` (read, runtime discovery) | **LayerFS unavailable**, **Reading LayerFS…** |
| 3 Headline | The most urgent need, or **Nothing needs your attention** with a green dot | Each need's own button (see [Needs-you notices](components.md#needs-you-notices)) | the button's own call | **+{n} more** expands or collapses the remaining needs and their actions |
| 4 Try a preset | The agent-preset icon | Press → the Presets dialog ([Presets](presets.md)) | — (opens a dialog) | — |
| 5 Refresh | A circular arrow; its tip says when the checks last ran | Press → reads Stores, projects and machines again and re-runs the home checks | `home get`, `project list`, `store auths`, `store list`, `project list` (read), then `home doctor` | disabled while the checks run |

![Expanded Home notices and their actions (current UI, isolated fixture data)](img/home-notices.png)

*Isolated fixture data.*

## Nav bar

On the current Home navigation bar, 1 opens Doctor, 2 opens Logs and 3 opens Settings.

![Home navigation with 1 Doctor, 2 Logs and 3 Settings (current UI, isolated fixture data)](img/home-navigation.png)

*Isolated fixture data.*

The earlier multi-Store example below shows the breadcrumb switcher.

![The nav bar on a project page](img/home-nav-bar.png)

Shown here on `Home › Stores › demo-starter › bookshelf`, where it has every part. Full
table: [Components › Nav bar](components.md#nav-bar).

![A crumb's switcher](img/home-crumb-switcher.png)
![Jump to (current UI, isolated fixture data)](img/home-jump.png)

*Isolated fixture data.*

![Session results with titles and IDs (current UI, isolated fixture data)](img/home-jump-search.png)

*Isolated fixture data.*
![The Add menu (current UI, isolated fixture data)](img/home-add-menu.png)

*Isolated fixture data.*

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 Back · 2 Forward | Arrows | Press → the previous or next Center page (up to 50 kept) | — (local history) | disabled at either end |
| 3 Crumbs | `Home › Stores › demo-starter › bookshelf` | Press a crumb → that page | — (navigation) | the last crumb is the page you are on |
| 4 Crumb switcher | A chevron after an ancestor crumb, a caret on the current one, when it has siblings | Press → a menu of its siblings (every Store, every project in the Store, …) → open one | — (from the Storage view already read) | only when there is more than one sibling |
| — Transfers | **{n} transfers** with a progress bar, while moves, backups or restores run | Press → that Store's page (one), or a menu of them (several) | — (the jobs in the Storage view already read) | hidden when nothing runs |
| 5 Jump to… | A search field with **⌘K** | Press or **⌘K** → the Jump to dialog: type, **↑ ↓** to move, **Enter** to open, **Esc** to close | — (searches what Home already read) | **Nothing matches “{q}”**; not while another dialog is open |
| 6 Refresh | A circular arrow, on every page but Home | Press → the page reads again | the page's own reads | a dot while reading |
| 7 Add | **+ Add** | Press → **New session**, **New project**, **Import a folder**, **New Sandbox**, **New Store**, **Connect a database**, **New machine**, **Add bucket**, **Try a preset** | each item's dialog (see [Modals opened here](#modals-opened-here)) | New machine and New Sandbox stay visible with the reason when unavailable |
| Doctor | A shield; a red or orange count when the home checks found something | Press → `LayerFS › Doctor` ([Doctor](health.md)) | `GET /api/doctor/results` (read, host memory) | count red for an error, orange for a warning |
| Logs | A list icon | Press → `LayerFS › Logs` ([Logs](logs.md)) | — (navigation) | — |
| Settings | A gear | Press → [Set up LayerFS](entry-points.md#set-up-layerfs) | `home get` (read, through `setup.status`) | — |

## Projects

![A project row opened](img/home-projects-row.png)

Every project in every Store of this home. Answers "where is it, who works in it, how big is
it, when did it last change".

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| Search | **Search projects and Stores** | Type → rows whose project or Store name match; **/** focuses, **Esc** clears; the tab count reads **{n} of {total}** | — (filters rows already read) | **No project matches “{query}”** with **Clear search** |
| Sort | **Last session, newest first** | Choose **Default order**, **Size, largest first** or **Name, A to Z** | — (local order) | — |
| 1 Row toggle | A chevron | Press → the row opens on its sessions (or Sandboxes) | sessions: `sessions.list` (local only, no call); Sandboxes: `sandbox list` (read, in `GET /api/machines`) | rows that are open stay open |
| 2 Project name | **handbook**, a green **{n} running** pill, and the newest commit's line (`Session ef98eca3 · events 0–59`, or a message and Branch) | Press → the [Project](project.md) page | `project list` (read); commit line `branch get`, `project stats` (read, `GET /api/stores/:id/projects`) | — |
| 3 Store | The Store's glyph and name | Press → the [Store](store.md) page | — (navigation) | — |
| Sessions · Sandboxes | Counts; the sessions count has a green dot when one is active | Press a count → the row opens on that list | as the row toggle | `—` for none |
| Last session · Last commit | Relative times | — | `branch get` (read) | `—` for none |
| Commits · Branches · Size | Numbers | — | `project stats` (read); size `GET /api/projects/:key/size`: `project get`, `project stats`, `store get` (read) | — |
| 4 Session line | **Fake session**, **Active** / **Idle** / **Interrupted** / **In Store**, and when | Press → the conversation opens in DSH | `sessions.open`: none when its log is here; else `branch list`, `project export` (imports it from the Store) | — |
| | **+ New session** in the list head | Press → a new project conversation (see [Entry points › New session in a project](entry-points.md#new-session-in-a-project)) | `sessions.choose`: `project get` (+ `branch list`, `branch fork` the first time) | — |
| 5 List switch | **Sandboxes {n}** / **Sessions {n}** | Press → the other list | as the row toggle | — |
| | A Sandbox line, **Running** / **Stopped** | Press → the [Sandbox](sandbox.md) page | — (navigation) | — |
| 6 ⋯ | **{name} actions** | Press → **Open**, **New session**, **Delete project…** | see below | — |

![The project menu](img/home-project-menu.png)

| Menu item | Result | LayerFS call |
|---|---|---|
| **Open** | The Project page | — |
| **New session** | A new project conversation | `sessions.choose`: `project get` |
| **Delete project…** | [Confirm dialog](components.md#confirm-dialog) **Delete {name}?**: "The project and all its history are deleted from its Store. This cannot be undone." Type the name to enable **Delete**. | `project delete` |

Empty: **No projects yet** — "A project is a folder of files kept in a LayerFS Store." with
**New project** and **Try a preset**.

## Stores

![Stores, SQLite](img/home-stores.png)

The Stores this home holds, by kind. Answers "where does each Store live, what is in it, is it
backed up".

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 Kind | Two tiles: **SQLite** *{n} Stores · size* and **PostgreSQL** *{n} databases* | Press → that kind's Stores; remembered | — (local view state) | — |
| 2 Show | **Stores {n}** / **Buckets {n}** (SQLite only) | Press → Store cards or bucket cards; remembered | — (local view state) | — |
| 3 New Store | **New Store** (SQLite) or **Connect a database** (PostgreSQL) | Press → the New Store / Connect a database dialog | `store create` / `store save-auth`, `store connect`, `store get` | — |
| 4 Store card | Name, size, last write, **Stored on** machine, up to three projects, status, backups | See [Components › Store card](components.md#store-card) and [Store menu](components.md#store-menu); **Push to…** (on native) / **Pull** (on a machine) | `GET /api/storage` (read); **Push to…**/**Pull** `store move` | a [job card](components.md#job-card) while a move or backup runs |

Empty: **No SQLite Store yet.** / **No PostgreSQL database connected yet.**

![Buckets](img/home-buckets.png)

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 Show | **Buckets** selected | — | — | — |
| 2 List bucket files | A refresh button | Press → every bucket is listed again | `backup list` (one per bucket), then the Storage view | a dot while listing |
| 3 Add bucket | **Add bucket** | Press → the Add bucket dialog ([Bucket](bucket.md)) | `backup connect` (keys on stdin) | — |
| 4 Bucket card | Provider, name, `MinIO · 1 Store file · 385 KB · listed {ago}`, its files with their Store and date, **{n} from this home** | Press the name or **Open bucket** → the [Bucket](bucket.md) page; **Restore…** on a file not from this home → the restore dialog | `backup list` (read); **Restore…** `backup restore` | **Files unknown** with **Connect again…** / **List again**; **not listed yet** with **List files** |
| 5 ⋯ | **More actions for {bucket}** | **List files**, **Open bucket** | `backup list` | — |

![Stores, PostgreSQL](img/home-stores-pg.png)

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 PostgreSQL | The tile, selected | — | — | — |
| 2 Connect a database | The kind's add button | Press → Connect a database | `store save-auth`, `store connect`, `store get` | — |
| 3 Cards | One card per database, grouped by login; each group has **Rotate password…** and ⋯ **Remove login**; **Connected from** chips name the machines that reach it | Press a card → the Store page | `GET /api/store-auths` (read, host only); sizes `store get` (read); projects `GET /api/stores/:id/projects` (read); **Rotate password…** `store save-auth`, `store doctor`; **Remove login** `store remove-auth` | **No PostgreSQL database connected yet.** (shown: the fixture has none) |

## Machines

![Machines, docker opened](img/home-machines.png)

Every machine of this home in one table. Answers "is it running, how full is it, what does it
hold".

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| 1 New machine | **+ New machine** | Press → the New machine dialog ([Machine](machine.md)) | `home get`, `machine create` | only when a platform can create one |
| 2 ⋯ | **More machine actions** | Press → **Diagnose this host** → the home checks run and the [Doctor](health.md) page opens on them | `home doctor` | — |
| 3 Row toggle | A chevron | Press → what it holds; remembered | — | open by default when it needs you, is starting, or is the only machine |
| 4 Name | **docker**, `Docker · Linux namespaces` (or `This Mac · Host processes`) | Press → the [Machine](machine.md) page | `home get`, `machine get`, `machine list`, `sandbox list`, `store list` (read) | — |
| State | A chip: **Running**, **Stopped**, **Starting**, … | — | as above | **Blocked** rows say why, with a fix |
| Memory · Disk | Bars | — | memory: `GET /api/metrics` (read, host memory); disk: `machine stats` (read) | `—` while stopped |
| Holds | **1 Store**, **1 Sandbox** | — | as above | — |
| 5 Power | **Stop** / **Start** (not on This Mac) | **Start** → the machine starts. **Stop** asks a second press: the button becomes **Confirm: Stop** (**Confirm: Stop worker** for docker) for a few seconds, like the head card's Stop | `machine stop` / `machine start` | disabled while it settles |
| 6 ⋯ | **More for {name}** | [Components › Machine menu](components.md#machine-menu) | see there | — |
| 7 Add Store | **Add Store** | Press → a dialog to put a SQLite Store on this machine | `store move` (a job) | — |
| 8 New Sandbox | **New Sandbox** | Press → the New Sandbox dialog ([Sandbox](sandbox.md)) | `sandbox create` | — |

![More machine actions](img/home-machines-more.png)

## Variants

- **SQLite vs PostgreSQL.** The Stores tab's kind tiles switch between them. Buckets belong to
  SQLite (a backup is a copy of a Store file), so the **Show** switch and bucket cards appear
  only under SQLite. PostgreSQL cards are grouped by their saved login.
- **No setup yet.** Before LayerFS is set up, Home shows the [Set up card](entry-points.md#set-up-card)
  in place of the tabs.
- **No storage view yet.** **Reading the Stores…** while loading; **Could not read the storage**
  with **Retry** when the read failed.
- **The Refresh button** in the nav bar shows on every Center page but Home, where the head
  card's Refresh does the same and also reruns the checks.

## Modals opened here

| Modal | Opened by | What it does | LayerFS call | Undo |
|---|---|---|---|---|
| New project | **New project** (tab, Add, empty state) | [Entry points › Project dialog](entry-points.md#project-dialog) | `store create`, `project init`, `store move` | delete the project |
| New session | Add › **New session** | Pick a project for the conversation | — (opens the picker) | Cancel |
| Import a folder | Add › **Import a folder** | Open the project dialog with a source folder | `project init` on confirmation | Cancel before creation |
| New Sandbox | Add › **New Sandbox** | Choose an eligible machine, then configure the Sandbox | `sandbox create` on confirmation | Cancel before creation |
| New Store | **New Store**, Add › **New Store** | Makes a SQLite Store on a machine | `store create` | **Remove…** on the Store |
| Connect a database | **Connect a database** | Saves a login and connects a PostgreSQL database | `store save-auth`, `store connect`, `store get` | **Disconnect…** |
| New machine | **New machine** | Creates a Docker, Lima or Apple Container machine | `machine create` | **Remove** in the machine menu |
| Add bucket | **Add bucket** | Connects an S3 bucket for backups | `backup connect` | **Disconnect** on the bucket |
| Presets | **Try a preset**, Add › **Try a preset** | [Presets](presets.md) | the preset's script | — |
| Set up LayerFS | Runtime, **Settings** | [Entry points › Set up LayerFS](entry-points.md#set-up-layerfs) | `home get` | — |
| Jump to | **Jump to…**, **⌘K** | Opens a project, Store, machine, Sandbox, bucket, Center page or available DSH session | — | Esc |
| Delete {name}? | Project ⋯ › **Delete project…** | [Confirm dialog](components.md#confirm-dialog) | `project delete` | none |

## States and signals

- **Tab dots.** A tab's dot takes the colour of the worst thing in it: red for a failed job or a
  broken Store, orange for a stopped machine holding Stores.
- **Nothing needs your attention** with a green dot means no job failed, no machine or Store
  is broken. Doctor findings are counted on the Doctor button only, and a Store never backed
  up is not a need.
- **A disabled power button** is settling after a press; wait for its state to change.
- **{n} running** on a project is the Sandboxes running for it now.

## Where next

[Project](project.md) · [Store](store.md) · [Bucket](bucket.md) · [Machine](machine.md) ·
[Sandbox](sandbox.md) · [Doctor](health.md) · [Logs](logs.md) · [Presets](presets.md) ·
[Entry points](entry-points.md)

## Source

`src/client/storage/StoragePage.tsx` (Center, routing, Jump items, Add menu, polling) ·
`src/client/storage/Nav.tsx` · `src/client/storage/HomeOverview.tsx`, `HomeHero.tsx` (head
card, needs) · `src/client/storage/ProjectTable.tsx`, `ProjectFinder.tsx` ·
`src/client/storage/MachineStores.tsx`, `BucketCards.tsx` · `src/client/machines/Board.tsx` ·
`src/client/doctor/use-doctor-count.ts` · shots: `tools/guide/shots/home.mjs`
