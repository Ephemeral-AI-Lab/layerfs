# #428 LayerFS home: where LayerFS keeps its files

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract. One owner decision is recorded:
> [delete `sealed.sqlite` after Init](#4-sealedsqlite-is-deleted-after-init)
> (owner, 2026-10-23).

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Related: [README.md](README.md),
[layerfs-backend-runtime.md](layerfs-backend-runtime.md),
[layerfs-agent.md](layerfs-agent.md), [layout.md](layout.md).

This document describes the files LayerFS keeps on a user's machine on Linux,
macOS and Windows: what each file is, who writes it and how long it lives.
What exists on the SDK branch `claude/issue428-sdk-packages-cd9412` at
`cdc9493982` is marked **current**; changes are marked **proposed** or
**decided**.

## 1. Why this document

On 2026-10-23 a Mac restart emptied the system temp folder. dsh's LayerFS
plugins kept their LayerFS home there (`$TMPDIR/dsh-layerfs/home`), so every
project's `project.record` was lost. The projects' committed history was
untouched in their Docker volumes, but without the record no Sandbox could be
started on them, and simulated projects were lost entirely.

The rule this document proposes: **small records go in one persistent
per-user folder, `~/.layerfs`, on the machine that runs the SDK; each
project's database goes in its own Docker volume; and each volume carries
enough to rebuild its project's record.**

## 2. Two places, one rule

| Place | Holds | Why there |
|---|---|---|
| LayerFS home (`~/.layerfs`) | project, Sandbox, Workspace and process records; the worker's address; traces; the runtime's own records | small files the SDK and runtime read and write; must survive restarts |
| One Docker volume per project | the project's Store (`store.sqlite`) and, proposed, a copy of its manifest | every opener of a Store must share one Linux kernel and a local ext4, xfs or btrfs filesystem (`kernel_guard.rs`); a volume is that on every platform |

A Store can't live in the home on macOS: the home is on the Mac's disk, which
containers reach only through Docker Desktop's file sharing, and the guard
refuses it. On Windows the same holds for a Windows folder. On Linux it would
work, but using a volume everywhere keeps one layout.

A project's Store holds everything: metadata, history and file content. File
content is stored in `pack` / `pack_unit` rows; there are no pack files
outside the database (`AGENTS.md`, "One global database").

## 3. Layout

Each listing shows one project, `shop`, with one Sandbox. `+` marks a
proposed path, `-` the current path it replaces.

### 3.1 macOS

```diff
  Mac disk
- $TMPDIR/dsh-layerfs/home/
+ ~/.layerfs/
    shop/
      ownership.lock
      project.record
      sealed.sqlite
      sandboxes/
        1/
          sandbox.record
          workspace-<id>.record
          process-<id>.record
    worker/
      endpoint.json
      worker.log
    traces/
    .runtime/
      init-failures.jsonl
  ~/.dsh/profiles/web/dsh-layerfs/
    bindings.json
    lines/

  Docker Desktop's Linux VM
  /var/lib/docker/volumes/
    layerfs-shop-store-<id>/
      _data/
        global/
          store.sqlite
          store.sqlite-wal
+         manifest.record
  containers
    layerfs-worker
    layerfs-shop-1
      /layerfs-store/global/store.sqlite
      /layerfs-local/overlay/
```

### 3.2 Linux

```diff
- <home given by the caller>/
+ ~/.layerfs/
    shop/
      ownership.lock
      project.record
      sealed.sqlite
      sandboxes/
        1/
          sandbox.record
          workspace-<id>.record
          process-<id>.record
    traces/
    .runtime/
      init-failures.jsonl
  ~/.dsh/profiles/web/dsh-layerfs/
    bindings.json
    lines/
  /var/lib/docker/volumes/
    layerfs-shop-store-<id>/
      _data/
        global/
          store.sqlite
          store.sqlite-wal
+         manifest.record
  containers
    layerfs-shop-1
      /layerfs-store/global/store.sqlite
      /layerfs-local/overlay/
```

Init on Linux writes into Docker's volumes folder, so it runs as a user that
can, such as root; `~` is that user's home.

### 3.3 Windows

```diff
  Windows disk
- <home given by the caller>\
+ C:\Users\<you>\.layerfs\
    worker\
      endpoint.json
      worker.log
    traces\
    .runtime\
      init-failures.jsonl
  C:\Users\<you>\.dsh\profiles\web\dsh-layerfs\
    bindings.json
    lines\

  WSL (Ubuntu)
- /var/lib/layerfs-windows/<hash>/
+ ~/.layerfs/
    shop/
      ownership.lock
      project.record
      sealed.sqlite
      sandboxes/
        1/
          sandbox.record
          workspace-<id>.record
          process-<id>.record
  /var/lib/docker/volumes/
    layerfs-shop-store-<id>/
      _data/
        global/
          store.sqlite
          store.sqlite-wal
+         manifest.record
  containers
    layerfs-shop-1
      /layerfs-store/global/store.sqlite
      /layerfs-local/overlay/
```

Windows keeps the UI, bindings and traces; WSL keeps project discovery and
the Stores (`AGENTS.md`, dsh Live on Windows).

`sealed.sqlite` is shown because it exists today; under
[section 4](#4-sealedsqlite-is-deleted-after-init) it exists only during Init.

## 4. `sealed.sqlite` is deleted after Init

**Decided (owner, 2026-10-23).**

`sealed.sqlite` is the Store as Project Init builds it: Init scans the source
folder into a new Store in `<home>/<project>/sealed.sqlite`, publishes the
first commit, then seals it (`Handles::seal()`: finish the save, fold the
write-ahead log into the main file, close it). The result is one closed,
self-contained file. Init then copies it into the project's volume as
`global/store.sqlite` (`install_local`), the live Store every later commit
goes to.

After the install nothing reads the sealed file (checked at `cdc9493982`):

- Init reads it once, to install it (`api/binding/src/project.rs`).
- Starting a Sandbox uses the installed Store and the installed manifest
  (`existing_store` in `api/binding/src/sandbox.rs`); the core create path
  returns before it would read the sealed file.
- A failed install is not retried from it: starting a Sandbox refuses with
  "its Import did not finish, so import it again".

Kept, it is a full-size copy of the project at Init time, content included,
so it doubles the project's disk use.

The change:

1. Init builds and seals `<home>/<project>/sealed.sqlite` as now.
2. Init installs it into the volume and writes `project.record` with the
   install record.
3. **Only when the install succeeded** does Init delete `sealed.sqlite`.
   When the install failed or its outcome is uncertain, the file stays for
   diagnosis, like the home and volume of an uncertain Init.
4. Init's answer no longer names the sealed file as its `store`; it gives the
   installed Store's path instead, or drops the field.

## 5. Proposed changes

### 5.1 Default home `~/.layerfs`

`LayerFSRuntime.open` requires an absolute `home` today, and dsh's plugins
pass `$TMPDIR/dsh-layerfs/home`. Proposed: the home defaults to `~/.layerfs`
on the machine that runs the SDK, overridable by `LAYERFS_HOME` (settings
come from the environment, [layerfs-agent.md](layerfs-agent.md)).
`traceDir` and `stateDir` keep their defaults under the home
(`traces/`, `.runtime/`). On Windows the WSL side's default moves from
`/var/lib/layerfs-windows/<hash>` to `~/.layerfs` in the WSL user's home.

Integrations stop choosing a home of their own. dsh keeps its bindings and UI
state in its profile, as now.

### 5.2 `manifest.record` in each volume

`project.record` holds the Store manifest and the install record; opening the
live Store needs the manifest's keys (`direct.rs`). Today that record exists
only in the home. Proposed: the install also writes the manifest to
`global/manifest.record` in the volume, beside `store.sqlite`, so the
runtime can rebuild a lost `project.record` by scanning the volumes. The home
then lists projects; the volume is the single copy of each project's data.

## 6. File reference

| File | Written by | Lifetime | Size |
|---|---|---|---|
| `<project>/ownership.lock` | SDK, every call on the project | while the project exists | empty |
| `<project>/project.record` | Init | while the project exists | small |
| `<project>/sealed.sqlite` | Init | until the install succeeds (decided) | full Store |
| `<project>/sandboxes/<n>/sandbox.record` | Sandbox start | until the Sandbox is removed | small |
| `<project>/sandboxes/<n>/workspace-<id>.record` | mount | until unmount | small |
| `<project>/sandboxes/<n>/process-<id>.record` | Exec | until the process record is released | small |
| `worker/endpoint.json`, `worker.log` | runtime (macOS, Windows) | while the worker runs | small |
| `traces/` | SDK calls with a trace | until cleaned | grows |
| `.runtime/init-failures.jsonl` | runtime | appended per failed Init | small |
| volume `global/store.sqlite` (+ `-wal`) | Init install, then daemons and the SDK | while the project exists | the project's data |
| volume `global/manifest.record` | Init install (proposed) | while the project exists | small |
| Sandbox `/layerfs-local/overlay/` | daemon | one overlay database per mounted Workspace, deleted at unmount | changes since mount |

## Open decisions

1. **Home name.** `~/.layerfs` (hidden, like `~/.dsh` and `~/.docker`) or a
   visible `~/layerfs`?
2. **Rebuilding records.** Should the runtime rebuild a missing
   `project.record` from `manifest.record` automatically, or only through an
   explicit call?
3. **Existing projects.** Projects whose record was lost before
   `manifest.record` existed can only be imported again, losing history, or
   recovered by a one-off tool. Which?
