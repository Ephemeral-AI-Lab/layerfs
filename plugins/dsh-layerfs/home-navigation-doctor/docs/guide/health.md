# Doctor

> **Where:** `LayerFS › Doctor` · **Opens from:** the Doctor button (shield icon) in the [Center nav bar](components.md#nav-bar); **Diagnose** on a machine, Store or Sandbox page · **Source:** see the end

The detail views below the current entry point come from the earlier demo setup.

![1 Doctor, 2 Logs and 3 Settings (isolated fixture data)](img/home-navigation.png)

*Isolated fixture data.*

The Doctor page shows the whole LayerFS home as one tree (this install, its machines, Stores,
projects, Sandboxes and backup buckets) and what LayerFS's doctors last said about each. Come
here when something looks wrong, to check it, and to run the one fix LayerFS proposes for it.
Opening the page runs no doctor: every row reads **Not checked** until you press **Check**,
**Full check** or **Check all**.

![The Doctor page, numbered](img/health-overview.png)

| # | Component | What it is |
|---|---|---|
| 1 | Headline | The home at its worst: **All clear**, **No problems**, ***n* notices**, ***n* problems**, **Not checked**, **Checking *i* of *n***, with counts below (**11 OK · 1 notice**) |
| 2 | **Check all** | Runs every doctor once, side by side |
| 3 | This install | **LayerFS *version***: the home row (install, home folder, free disk) |
| 4 | Kind tabs | **Machines** · **Stores** · **Projects** · **Sandboxes** · **Backups**, each with a count and a dot for its worst row |
| 5 | A row | Name, status word, facts, then what is on it when opened |
| 6 | Check marks | One dot per check the last doctor made: green passed, amber notice, red failed, grey not checked |
| 7 | **Check** | Runs this row's doctor |
| 8 | Source line | **Read from records: `layerfs home get --with tree`. Opening this page runs no doctor.** |

Above the page is the [nav bar](components.md#nav-bar): `Home › Doctor`. Its refresh button
reads the tree again.

## Layout

The headline and **Check all**, the home row, the kind tabs, the tab's rows, the source line.
A row opens (chevron, or click its name) to its details and the things directly on it, so a
Store is found both under its machine and on the Stores tab. Results of earlier checks are
kept by the host, so the page shows them when it opens or reconnects.

## Body

The rows answer "is it healthy, and if not, what fixes it". The tree comes from records only
(`home get --with tree --detail`, read, shared by every reader for 2 s); each check is one
LayerFS doctor call, one attempt, that reports progress per thing as it goes.

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| Tree | The home's machines, Stores, projects, Sandboxes (with Workspaces) and buckets | Open a row → its details and what is on it | `GET /api/doctor/tree` → `home get --with tree --detail` (read) | **Reading the records…**; **The records could not be read** with **Try again**; **This LayerFS has no Doctor yet** (a runtime before 2.0) |
| Kept results | Each row's last outcome | — | `GET /api/doctor/results` (no LayerFS call: host memory) | |
| **Check all** | | Press → every doctor once: `home doctor`, `store doctor` per Store, `sandbox doctor` per Sandbox (the home doctor quick-checks each machine, PostgreSQL Store and bucket) | `home doctor`, `store doctor <store>` ×n, `sandbox doctor <sandbox>` ×n (reads) | Busy while it runs; off while a Full check or a fix runs (the tip says which). Left out: everything under a stopped or failed machine, and project data. Toast **Check all finished** |
| **Check** | Its tip names what it covers and the exact call | Press → that row's doctor; a row with things on it opens so they land in sight | home: `home doctor`; machine: `machine doctor <m>`; Store: `store doctor <s>`; project: `project doctor <s/p>`; Sandbox: `sandbox doctor <b>` (its Workspaces too); bucket: `backup doctor <b>` (reads) | **Checking**; **Waiting to start** with **Stop** while another call holds its turn |
| **Full check** | Projects and buckets only | Press → the deep check: a project reads everything its Branches reach (can take minutes); a bucket writes a small object and removes it | `project doctor <s/p> --full`, `backup doctor <b> --full` | Elapsed time and **Stop** while it runs; **Full check stopped at *time*.** |
| **Stop** | | Press → ends that doctor's process; nothing is written | — (the host ends the CLI process) | The row keeps its last outcome |
| Status word | **OK**, **Notice**, **Problem**, **Checking**, **Not checked**, **Stopped**, **Could not check** | Hover → when it was checked, data read / write tested, why it waits | — | **Could not check**: **Timed out.**, **Busy: another call was holding it.**, **The reply was lost.** |
| Waiting cause | **waiting on *name***, linked | Click → goes to that row (switching tab) | — | A row under a stopped machine waits on it |
| Details (open row) | **Call** (the doctor's command, **Copy**), the checks with **passed** / **notice** / **failed** / **not checked** and LayerFS's note, **Fix** (the fix line, **Copy**) | **Copy** → the command on the clipboard; runs nothing | — | Before any check: the quick checks it would make |
| Fix button | The one fix LayerFS proposed, on the row: **Start**, **Restart worker**, **Delete project**, **Back up now** | Press → runs the fix, then checks the thing again | See [Fixes](#fixes) | Busy while it runs; **Refused** (amber) with LayerFS's reason; toasts ***name* is ready**, ***name* was deleted**, ***name* was backed up**, ***fix* refused: *name*** |

![guide-notes on the Stores tab: its check, the notice and its fix](img/health-store.png)

Opened (1), a Store shows its **Call** (3), `layerfs store doctor guide-notes`, and its six
checks. Here `store.backups` is a notice, *its backup in guide-backups is behind*, so the
row reads **Notice · Backup behind** and offers **Back up now** (2), whose tip lists what it
runs: `layerfs backup create guide-notes guide-backups`, then `layerfs store doctor
guide-notes`. The **Fix** line (4) is the same command to copy.

![Projects: Full check and Check](img/health-projects.png)

### Fixes

The page runs a fix line only when a report or the records offered it for that very thing,
and only these four. Any other fix line stays a line to copy.

| Fix | Offered when | Runs | Then | Undo |
|---|---|---|---|---|
| **Start** | A machine is stopped (on purpose or not) | `machine start <m>` | `machine doctor <m>` and what is on it | `machine stop` |
| **Restart worker** | A machine's worker is outdated (`machine.worker.compatibility`) | `machine stop <m>` then `machine start <m>` | the machine's checks | — |
| **Delete project** | A project's record names it but its Store does not list it (`project.listed`) | confirm, then `project delete <s/p>` | its Store's doctor | None |
| **Back up now** | A Store's backup in a bucket is behind (`store.backups`) | `backup create <store> <bucket>` (as a Storage job) | `store doctor <store>` | — |

Each step is one attempt. A refusal is certain (nothing changed; the button tries again); a
lost reply may have worked, so nothing runs it again: check the row to see where it stands.

## Variants

- **Machines**: native shows three checks (record, operation, state); a Docker or Lima
  machine ten, through its worker.
- **Sandboxes**: Workspaces are checked by their Sandbox's doctor (**Never checked. Its
  Sandbox checks it.**).
- **Buckets**: **Full check** also writes and deletes a small test object.
- Tabs with nothing on record read **Nothing of this kind is on record.**

## Modals opened here

| Modal | Opened by | What it does | LayerFS call | Undo |
|---|---|---|---|---|
| **Delete *project*?** | **Delete project** on a project row | *The records name this project, but its Store does not list it. This runs `layerfs project delete <s/p>`.* **Cancel** · **Delete project** | `project delete <s/p>`, then the Store's doctor | None |

No fixture project is missing from its Store, so this [confirm dialog](components.md#confirm-dialog)
has no screenshot.

## States and signals

- A dot on a kind tab: something in it has a problem (red), a notice (amber), is not
  checked (grey) or is being checked; its tip counts what is checking, stopped or could not
  be checked.
- **1 notice inside** on a machine: something on it has a notice; open the row.
- **Could not check *name*: *reason*** toast: the doctor did not answer (timed out, busy, or
  its reply was lost). Press **Check** again.
- **Waiting to start**: another LayerFS call on that Store holds its turn; **Stop** ends the
  wait.

## Where next

- [Machine](machine.md), [Store](store.md), [Sandbox](sandbox.md), [Bucket](bucket.md):
  the pages a row is about (their **Diagnose** opens this page).
- [Logs](logs.md): the doctor and fix calls this page ran.
- [Components](components.md#fix-buttons): fix buttons elsewhere.

## Source

`src/client/doctor/`: `DoctorPage.tsx`, `Doctor.tsx`, `Row.tsx`, `checks.ts` (each doctor's
checks), `tips.tsx`, `model.ts`, `use-doctor.ts`, `words.ts`, `locale.ts`. Host:
`src/host/routes/doctor.ts` (`/api/doctor/tree`, `/results`, `/runs`, `/runs/:id/stop`,
`/fixes`), `src/shared/doctor.ts` (`doctorArgv`, `parseFix`).
