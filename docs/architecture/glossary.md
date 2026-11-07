# Glossary

[Architecture](README.md) › Glossary

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Every LayerFS term used in these pages, in one table, with the page that owns it.

Names in `code` are the source's own. Where one word has two meanings in
LayerFS (backend, install, scope), the row gives both.

| Term | Meaning | Defined in |
| --- | --- | --- |
| Acquisition | The working rows and operation record Project Init keeps in the Store's acquisition tables while it imports a folder; they are removed before anything is published. | [Project Init: import](engine/project-init/import.md#backing) |
| Admission | The first-come, first-served turn each Store session gives one thread at a time before its read or write transaction runs. Waiting for it is readiness, not a retry. | [Store: backends](engine/store/backends.md#admission) |
| Backend | (1) Sandbox backend: how a Sandbox's daemon runs on a machine's kernel, `native` (a host process, macOS) or `namespaces` (Linux, including the `docker` machine's worker container). (2) Store backend: SQLite or PostgreSQL behind one seam. | [Sandboxes: backends](control/sandboxes/backends.md), [Store: backends](engine/store/backends.md) |
| Backup | A copy of one whole sqlite Store, with every project in it, held in a bucket as `<file>/store.sqlite`. | [Backups](control/backups/README.md) |
| Base | The committed root a Workspace reads below its overlay: the Branch snapshot it was bound to, then each root a Commit installs. | [Workspace: base](engine/workspace/base.md) |
| Binding key | The key that identifies a Store; every opener, daemon and restore checks it. It is not a secret; the cursor key stays with its holder. | [Store: lifecycle](engine/store/lifecycle.md#open-and-handles) |
| Branch | A named line of Commits in a project, with a base Layer and an optional head Commit. `main` is not a Branch. | [History model: entities](core/history-model/entities.md#branches) |
| Bucket | A named place the home records where backups are stored, reached through a driver (local folder or S3 API). Drivers know files by name and nothing about Stores. | [Backups: buckets](control/backups/buckets.md) |
| Call table | The 62 calls in nine families that every front end exposes, with their arguments and the one dispatcher they all go through. | [Call table](api/call-table/README.md) |
| Capture | The Commit step that seals a Workspace's active overlay generation as its published frontier and opens the next one. It copies no rows. | [Commit: capture](engine/commit/capture.md) |
| Commit | An immutable history record: a complete filesystem root, its parent, the base Layer it was built from, an optional second parent and metadata. Also the call (`workspace.commit`) that makes one from a Workspace. | [History model: entities](core/history-model/entities.md#commits-and-layers), [Commit](engine/commit/README.md) |
| Construction | Building canonical file and tree objects from input and saving them: Init from a native folder, a Commit from its captured generation. A Commit has one construction producer. | [Commit: construction](engine/commit/construction.md), [Project Init: construction](engine/project-init/construction.md), [Files: construction](core/files/construction.md) |
| Custody | What an operation keeps after a failure whose effect is not settled (a capture, a published Commit, a retained mount owner). While it is kept, the next operation is refused instead of guessed. | [Commit: capture](engine/commit/capture.md#custody), [Publish and merge: settle](engine/commit/publish-and-merge.md#settle) |
| Daemon | `layerfs-daemon`: the process in a Sandbox that serves its Workspaces to the kernel and runs their Commits. It opens the global Store directly, supervises no commands and serves no Store calls. | [Daemon](engine/daemon/README.md) |
| Disposable profile | The only permitted SQLite Store profile: WAL journal and `synchronous=OFF`. Durable execution is disabled by the owner. | [Store: lifecycle](engine/store/lifecycle.md#the-disposable-profile) |
| Effective view | What a mounted Workspace shows: its base plus the overlay rows above the installed floor. | [Workspace](engine/workspace/README.md#the-effective-view) |
| Exec | `workspace.exec`: the Sandbox runtime runs `bash -c <command>` in a Workspace directory and returns its exit code and output. Optional: file access needs no Exec. | [Sandboxes: commands and streams](control/sandboxes/commands-and-streams.md) |
| Family | One group of calls in the call table: `project`, `branch`, `commit`, `file`, `sandbox`, `workspace`, `machine`, `store`, `backup`. | [Call table](api/call-table/README.md), [Control: families](control/management/families.md) |
| Fork | Making a new Branch at a Layer or a Commit: one Branch row, no tree copied. | [Store calls: branches](engine/store-calls/branches.md#fork) |
| Forward | A host loopback port carrying a Sandbox daemon's port, so Workspace calls can dial the daemon directly. | [Network: forwards](control/network/forwards.md) |
| Frontier | The published frontier: every change filesystem requests have already published to a Workspace's overlay, from any process. A Commit captures exactly this. | [Commit: capture](engine/commit/capture.md#the-published-frontier), [Overlay: lifetimes](engine/overlay/lifetimes-and-reclamation.md#generations-and-frontier) |
| Generation | A numbered set of overlay rows: the active one takes writes, a captured one is sealed for a Commit, and rows at or below the installed floor leave the view. | [Overlay: lifetimes](engine/overlay/lifetimes-and-reclamation.md#generations-and-frontier) |
| Genesis Layer | The first Layer of a LayerStack, written by Project Init with the imported root. | [History model: entities](core/history-model/entities.md#layerstacks) |
| Home | The caller's LayerFS folder (`LAYERFS_HOME`, else `~/.layerfs`) with the records of machines, Stores, projects, Sandboxes and buckets, and their leases and secrets. Guests never read it. | [Management: home and records](control/management/home-and-records.md) |
| Install | (1) Commit install: advancing a Workspace's base to the published root while rows written after the capture stay. (2) Store install: the one-time stream of a sealed Store file to a daemon. | [Commit: install](engine/commit/install.md), [Project Init: seal and install](engine/project-init/seal-and-install.md#install) |
| Intent | The `op` field a multi-step operation writes into its entity's record before its first effect. A live owner makes other operations `Busy`; a gone owner's uncertain intent makes them `Unresolved`. | [Management: home and records](control/management/home-and-records.md#records) |
| Kernel route | How the kernel reaches a Workspace: a FUSE session (Linux) or a loopback NFSv3 export (macOS). | [Kernel routes](engine/kernel-routes/README.md) |
| Layer | A published root on a project's main line. `branch.publish` adds one; publication is overwrite-only. | [History model: entities](core/history-model/entities.md#commits-and-layers) |
| LayerStack | One project's linear timeline of Layers, with its Branches and Commits and one allocation scope. | [History model: entities](core/history-model/entities.md#layerstacks) |
| Lease | The file `store/<name>/open/<pid>` a process holds while it uses a Store. Moves, removes and disconnects refuse `InUse` while a live lease exists. | [Management: home and records](control/management/home-and-records.md#locks-and-leases) |
| Machine | Where Sandboxes and sqlite Stores live: the built-in `native` and `docker`, created VMs, or connected ssh hosts. Every machine except `native` on macOS and Linux has one worker. | [Machines](control/machines/README.md) |
| `main` | A project's line of Layers (its LayerStack's head Layer). It is not a Branch: it is not mounted or written, only forked from and published to. | [Store calls: branches](engine/store-calls/branches.md) |
| Manifest | The text object, uploaded last under a backup's name, that lists each piece's size and sha256 and the whole file's sha256. | [Backups: pieces and manifest](control/backups/pieces-and-manifest.md#sha256-manifest) |
| Merge | `branch.merge`: a new Branch from two sides, fast-forward or a merge Commit; also the step a Commit takes when its Branch head moved. | [Store calls: branches](engine/store-calls/branches.md#merge), [Compare and merge: merge](core/compare-and-merge/merge.md) |
| Overlay | A Workspace's own SQLite file with its changed metadata, payload and operation records. It is copied from the daemon's template at mount and deleted at terminal unmount. | [Overlay](engine/overlay/README.md) |
| Piece | One fixed-size part, `<name>.piece-NNNNNN`, of a backup too large for one bucket object. | [Backups: pieces and manifest](control/backups/pieces-and-manifest.md#pieces) |
| Platform | A machine's kind (`native`, `docker`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh`). It fixes the Sandbox backend, the worker's address and what the support matrix allows. | [Machines: platforms and profiles](control/machines/platforms-and-profiles.md#platform) |
| Project | A named LayerStack in a Store, written `<store>/<project>`. | [Project Init](engine/project-init/README.md) |
| Publish | `branch.publish`: puts a Branch's head Commit on `main` as a new Layer. | [Store calls: branches](engine/store-calls/branches.md#publish) |
| Publish lane | The Store's admission turn plus its cross-process writer lock (the `flock` on the Store's directory on Linux; the `store_lane` row on PostgreSQL), held by a Commit across its head check, last merge step and publish. | [Store: history publication](engine/store/history-publication.md#the-publish-lane) |
| Reach | How a daemon on a machine dials a PostgreSQL Store: the URL rewritten for that platform and checked once at Sandbox create. | [Network: reach](control/network/reach.md) |
| Sandbox | One daemon started by a backend on a machine, plus the commands run in it. The daemon serves the Workspaces; the Sandbox runtime owns the commands. | [Sandboxes](control/sandboxes/README.md) |
| Save | A Storage write session: it accepts objects, packs them and `Save::finish` completes them while the Store stays open. | [Physical storage: saves](core/physical-storage/saves.md) |
| Scope | (1) Folder scope: a client option `{ project, folder, branch? }` that allows only some calls and rewrites paths and Branch names; it is not filesystem isolation. (2) Allocation scope: the identity range a LayerStack's inode serials are reserved from. | [Call table: scopes](api/call-table/table.md#scopes), [History model: allocation](core/history-model/allocation.md) |
| Seal | Closing a Store's sole session with a checkpoint so no `-wal` or `-shm` remains. `store.create` seals a new empty Store; a live daemon's Store is never sealed. | [Project Init: seal and install](engine/project-init/seal-and-install.md#seal), [Store: lifecycle](engine/store/lifecycle.md#seal) |
| Serial | An inode number, unique within its allocation scope and never reused. | [Trees: representation](core/filesystem-trees/representation.md) |
| Store | The one global database, a SQLite file or a PostgreSQL schema, that holds every project's committed history and objects, with one writer at a time. | [Store](engine/store/README.md) |
| Store kind | `sqlite` or `postgresql`. It decides where Store calls run: a sqlite Store's on its machine's kernel, a PostgreSQL Store's in the caller's process. | [Routing table: by Store kind](control/routing/routing-table.md#by-store-kind) |
| Support matrix | The one table of platform × host OS × Store kind that allows or refuses a machine or Sandbox, with the reason; `SUPPORT.json` is generated from it. | [Support matrix](control/support-matrix/README.md) |
| Terminal unmount | `workspace.unmount`: detach the kernel mount, drain requests, revoke, close the overlay connection and delete its file. Committed history is untouched. | [Daemon: control calls](engine/daemon/control-calls.md#unmount) |
| UpToDate | A Commit outcome when nothing changed: no new Commit is made. Also a `branch.publish` outcome when that Commit was already published as a Layer. | [Publish and merge: UpToDate](engine/commit/publish-and-merge.md#uptodate) |
| Whiteout | An overlay directory-entry row whose inode is NULL: a name removed from the base. | [Overlay: namespace](engine/overlay/namespace.md#directory-entries) |
| Worker | `layerfs worker`, the one process on a machine that runs the machine's Store calls and Sandbox operations for callers outside it (every machine except `native` on macOS and Linux). It listens behind a secret on a port carried to the host's loopback and runs one request per connection. | [Network: workers](control/network/workers.md), [Routing: workers](control/routing/workers.md) |
| Workspace | A complete mutable filesystem view over one immutable committed root, served by a daemon and changed by any process that sees its mount. | [Workspace](engine/workspace/README.md) |

---

← [Architecture](README.md) · [Up: Architecture](README.md) · [Core](core/README.md) →
