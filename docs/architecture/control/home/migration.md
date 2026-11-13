# Migration

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the first call on a home creates it or converts an older layout, within
[Home](README.md).

| | |
| --- | --- |
| Source | [`layerfs-control/src/migrate/`](../../../../control/crates/layerfs-control/src/migrate), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs), [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs), [`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs) |

## Opening a home (`open.rs`)

```text
 open(home)
   worker root? ----------------------> answer the caller's home id; no write
   detect(home)  (reads only)
     Current(3) ----------------------> finish(): if log/layout-3.pending,
                                        delete layout 2 paths; use the home
     Empty | Old | Previous(2)
       take layerfs.lock, detect again
         Empty       -> create layerfs.json (layout 3, new id)
         Previous(2) -> v2::convert
         Old (1.1.0) -> v110::convert -> layerfs.json at layout 2
                        -> log/migration.json -> v2::convert
     newer layout -> Unsupported;  unreadable -> Failed
   a conversion that stops -> log/migration-failures.jsonl; the next call
                              continues it
```

| Layout | Recognised by | Source |
| --- | --- | --- |
| Empty | no `layerfs.json` and none of 1.1.0's records (a `layerfs.lock` alone is still empty) | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) |
| Old (1.1.0) | no `layerfs.json`, and one of `.stores`, `stores.json`, `stores.v1.json`, `stores.json.lock`, `remotes`, `remote-state`, `.runtime`, `worker`, `traces`, or a `<p>/project.record` | same, `OLD` |
| Previous (2) | `layerfs.json` with `layout: 2` | same, `PREVIOUS_LAYOUT` |
| Current (3) | `layerfs.json` with `layout: 3` and a 32-hex `id` | same, `LAYOUT_VERSION` |

Every call opens the home through `open`
([`migrate/open.rs`](../../../../control/crates/layerfs-control/src/migrate/open.rs));
a current home costs one read of `layerfs.json` and one look for the
pending marker.

## 1.1.0 to layout 2 (`v110.rs`, `volume.rs`)

| Old record | Becomes | Source |
| --- | --- | --- |
| `.stores/<s>/` | `store/<s>/` with its record and projects | [`migrate/v110.rs`](../../../../control/crates/layerfs-control/src/migrate/v110.rs) |
| `<home>/<p>/` (a project with its own Store) | Store `<p>` holding project `<p>` | same |
| `stores.json`, `stores.v1.json` | folded into each Store's record | same |
| `remote-state/<s>` | the Store's backup link | same |
| `remotes` | one `backup/<bucket>/record.json` per remote | same |
| `.runtime/` | Init failures into `log/` | same |
| `traces/` | `trace/` | same |
| a sqlite Store in a Docker volume (`<volumes>/<volume>/_data/global/store.sqlite`, volumes default `/var/lib/docker/volumes`) | linked to `store/<name>/store.sqlite` (a `native` Store) where readable; else copied and its sha256 checked before the rename | [`migrate/volume.rs`](../../../../control/crates/layerfs-control/src/migrate/volume.rs) |

Every old file read is moved unchanged under `log/1.1.0/`; nothing is
deleted. An unknown file is left and reported. A Store file with a
`-wal` beside it is not placed (its log would not follow it) and keeps its
`volume` in the report. `layerfs.json` is written last, so a stopped
conversion continues on the next call and a second run changes nothing.

## Layout 2 to layout 3 (`v2.rs`)

```text
 survey (read only)
   live?  Sandbox runtime running or uncheckable, create in flight,
          unknown create (create.intent), live Store lease,
          live Store move/remove (in_progress), live machine removal
     yes -> InUse, naming each with what ends it; nothing changed
     no:
 1 write  sandbox/<name>/record.json (from machine/<m>/sandbox/<name>/),
          Store records without an ended in_progress,
          "v": 1 into every machine, Store, project and bucket record,
          drop machine layout-2 intents (removing, creating, unknown)
 2 log/layout-3.pending, then layerfs.json at layout 3   <- the flip
 3 delete machine/*/sandbox/, store/*/project/*/sandboxes/;
   rename machine/native/sandbox-root/ -> machine/native/run/ (images kept);
   remove the marker; write log/migration-2.json
```

| Case | What happens | Source |
| --- | --- | --- |
| A Sandbox whose runtime ended | written with no `state` or `pid`; its stopped runtime is reclaimed once | [`migrate/v2.rs`](../../../../control/crates/layerfs-control/src/migrate/v2.rs) |
| A create whose outcome was unknown | written with an uncertain `op` | same |
| A Sandbox of the retired docker backend | forgotten, as `sandbox close` forgets it; `migration-2.json` notes its container and the `docker rm -f` that removes it | same, `forgotten` |
| A crash before the flip | the home is still layout 2 and converts again; every write is idempotent | same |
| A crash after the flip | the next `open` sees the marker and finishes step 3 | same, `finish` |

`layerfs doctor` lists the same blockers read-only, without a lock
(`blockers`, [Doctor](../doctor/README.md)).

## 1.1.0 environment (`import_env.rs`)

`layerfs import-env` writes records from a 1.1.0 environment once, so the
home then works with `LAYERFS_HOME` alone
([`migrate/import_env.rs`](../../../../control/crates/layerfs-control/src/migrate/import_env.rs)).

| It fills (only when not recorded yet) | From |
| --- | --- |
| built-in machines' settings: `native`'s daemon and image archive, `docker`'s socket and image | `LAYERFS_DAEMON`, `LAYERFS_IMAGE_ARCHIVE`, `LAYERFS_DOCKER`, `LAYERFS_IMAGE` |
| a bucket `remote` | `LAYERFS_REMOTE_*` |
| the machine of each Store a 1.1.0 home left unknown | `LAYERFS_SANDBOX` / `LAYERFS_PLACEMENT` |

It never makes a VM and never writes a secret: for those it answers the
command to run. `LAYERFS_SSH_HOST` is ignored (the ssh platform was
removed). Every name read and what became of it is in the answer and in
`log/import-env.jsonl`.

## Worker roots (`home/guest.rs`)

A worker's root is not converted. On its first start from a layout 3
bundle, `retire_layout_2` deletes what a layout 2 bundle left: numbered
Sandbox state (`store/*/project/*/sandboxes/`), `machine/*/sandbox/`, old
local Sandbox directories (the `images` cache is kept and
`machine/native/sandbox-root/` renamed `run/`), and rewrites full Store
record copies thin (`{v, name, kind}`). Each deletion is logged to the
worker's stderr; one that fails is logged and left. The marker `.layout-3`
stops it running again
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## Inventory (`inventory.rs`)

The relationships no record stores, read when a call needs them
([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)):

| Read or check | Answers |
| --- | --- |
| `machines`, `stores_on`, `postgresql_on`, `sandboxes_on`, `hosted_on` | every machine record; the sqlite Stores, attached PostgreSQL Stores and Sandboxes of a machine; old Firecracker records a Lima machine hosted |
| `sandboxes_of_store`, `sandboxes_of_project` | the Sandboxes of a Store or project |
| `can_remove_machine` | `machine.remove`: `InUse` while it holds Stores, attached PostgreSQL Stores, Sandboxes or hosted Firecracker records |
| `can_stop_machine`, `can_change_machine` | `machine.stop`, `machine.update`: `InUse` while Sandboxes are recorded on it |
| `store_idle` | `store.push`, `pull`, `remove`, `disconnect`, `backup.restore`: `InUse` while another live process holds a lease or a project's Sandbox is recorded (removes stale leases) |
| `store_idle_on` | `store.detach`: `InUse` while a Sandbox of the Store is recorded on that machine |
| `can_remove_project` | `project.remove`: `InUse` while a Sandbox of the project is recorded |

Each `InUse` lists what holds the entity in `detail.related` as
`{kind, name}`.

Layout 3 keeps each Sandbox at `sandbox/<name>/`, so a Sandbox by name is
one read; only the relationships list `sandbox/*`.

## Related

- [Home](README.md): the layout and record rules this page converts into.
- [Doctor](../doctor/README.md): reports layout 2 blockers.
- [Machines](../machines/README.md): the built-in machines `import-env` fills.
- User guide: [migration](../../../versioned/2.0.0/migration.md).
