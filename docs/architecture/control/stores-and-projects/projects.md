# Projects

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a project is added to, exported from and removed from its Store on
whichever side runs that Store's calls, within
[Stores and projects](README.md).

| | |
| --- | --- |
| Source | [`layerfs-store/src/project/`](../../../../control/crates/layerfs-store/src/project), [`layerfs-control/src/project.rs`](../../../../control/crates/layerfs-control/src/project.rs) |

A project is always `<store>/<project>`. Its record is
`store/<store>/project/<project>/record.json`: name, creation time, a
PostgreSQL project's LayerStack and a sqlite project's manifest
([`record.rs`](../../../../control/crates/layerfs-store/src/project/record.rs)).
`project.list` and `get` read these records, by Store then name
([`family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs)).
Canonical construction is the engine's
[Project Init](../../engine/project-init/README.md); this page is where it
runs and how the folder gets there.

## Init (`family.rs`, `add.rs`, `postgres_init.rs`, `init.rs`)

```text
 project.init(<store>/<project>, folder, {timeout, on_progress})
   folder absolute? (InvalidInput)   Store record? (NotFound)  unblocked
   progress check
   PostgreSQL Store -----------> lease; live add in this process  install
                                  (postgres_init.rs); record      record
   sqlite, Executor::Local ----> lease; live add through the Store's
                                  writer lane (add.rs); record
   sqlite, Executor::Worker --> project record exists? (Exists)
                                walk + measure the folder here
                                room on the machine (Capacity)
                                project.stage x N  (<= 4 at once)  send
                                project.init -> worker adds
                                  <root>/incoming/init-<id>, removes it
                                record the project in the home     record
```

| Rule | Detail | Source |
| --- | --- | --- |
| Live add | Init adds a folder as a new project to an existing Store through its writer; it does not seal | [`family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs), [`add.rs`](../../../../control/crates/layerfs-store/src/project/add.rs) |
| Where | where the Store's calls run: this process for PostgreSQL and a `Local` sqlite machine, else the machine's worker; the folder is always on the caller's host | same |
| Time limit | default 1 hour (`timeout` in ms) | same, `INIT_TIMEOUT_MS` |
| Progress | `{phase, done, total}`: `check`, `send` (bytes, worker route only), `install`, `record` | same |
| Lease | this process holds the Store's lease for the whole Init, on every route | same |
| Lost answer | `Unknown`; the home gains no record (`project list` does not show it; the Store may hold it) | same |
| PostgreSQL | the schema's URL is the call's and never stored | [`postgres_init.rs`](../../../../control/crates/layerfs-store/src/project/postgres_init.rs) |

## Folder copy to a worker (`send.rs`, `parts.rs`, `tree.rs`, `staged.rs`)

```text
 one project.stage part = run of records (big-endian; paths are bytes)
   D path                          a directory (kept if already made)
   W path offset:u64 bytes         a file piece at its offset
   L path target                   a symbolic link
   M path mode:u32 secs:i64 ns:u32 permission bits and mtime, after
                                   content; "" is the folder itself
 D/W/L parts: up to 4 at once, each on its own connection and thread,
              applied in any order
 M parts:     their own parts, each sent after every earlier part answered
```

| Rule | Source |
| --- | --- |
| The walk refuses before anything is sent what Init would refuse: an entry that is not a file, directory or link; a name or target LayerFS cannot hold; set-user-ID, set-group-ID or sticky bits; a file that changes while read | [`send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) |
| Room: the machine needs the copy's bytes plus `max(1 GiB, total/10)` free, else `Capacity` naming both | same, `room` |
| A request holds at most 8 MiB of records | same, `CHUNK` |
| One attempt per part; after the first failure no new part is sent, parts in flight are waited for, and that failure is the copy's. A certain failure discards the copy with one more request; a lost part answer leaves the copy and names it | [`parts.rs`](../../../../control/crates/layerfs-store/src/project/parts.rs) |
| A copy that may remain (a lost part answer, or a lost answer of the Init itself) is named in the home's `log/uncertain.jsonl`: `{ at, call: "project.init", store, project, machine, copy, phase: "send" \| "install", deadline, message }`, `deadline` being when the lost request went out plus the Init's `timeoutMs`. `layerfs home leftovers` (`home.leftovers`) lists the copy, and `layerfs home prune <leftover>` (`home.prune`) removes it only when such a line names it and that deadline passed more than an hour ago ([Leftovers](../doctor/checks.md#leftovers)) | [`send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) `log_left`, [`parts.rs`](../../../../control/crates/layerfs-store/src/project/parts.rs), [`family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |
| The worker writes into `<root>/incoming/init-<id>`, takes no lease and opens no Store; paths stay inside the staging folder and go through no link; the folder is removed on every outcome it knows | [`staged.rs`](../../../../control/crates/layerfs-store/src/project/staged.rs) |
| Carried: kind, permission bits, mtime, link target, file bytes. Not carried: owners, access times, extended attributes, hard links | [`tree.rs`](../../../../control/crates/layerfs-store/src/project/tree.rs) |

Each worker frame (`project.stage`, `project.init`, `project.export`,
`project.delete`, `project.delete.admit`, `project.delete.entry`,
`project.delete.release`) is
decoded once into its typed request; the worker sets the home to its own
root
([`frame.rs`](../../../../control/crates/layerfs-store/src/project/frame.rs)).

## Export (`export/`)

`project.export(project, destination, {branch})` writes a project's
committed Branch snapshot (default `main`) to a local folder on the
caller's host, on all three routes: `native` SQLite and PostgreSQL in this
process, SQLite behind a worker
([`export/route.rs`](../../../../control/crates/layerfs-store/src/project/export/route.rs)).

```text
 check destination (before any Store or staging)
 resolve, lease::claim
 pin the Branch's snapshot -> depth-first walk -> tree records
     (here, or worker op project.export streaming parts as events)
 one writer applies records strictly into <parent>/.layerfs-export-<id>
 rename staging -> destination           progress prepare/export/finalize
```

| Rule | Value | Source |
| --- | --- | --- |
| Destination | absolute; no `.` or `..` components; absent with a real parent directory, or an existing empty real directory | [`export/destination.rs`](../../../../control/crates/layerfs-store/src/project/export/destination.rs) |
| Destination refusals | `InvalidInput`, `Exists`, `NotEmpty`, `NotFound` (parent), `NotDirectory` | same |
| Staging | hidden sibling `.layerfs-export-<id>`, removed (including 0555 folders) unless committed by the one rename | same |
| Strict apply | a folder or file already there is `Conflict` (including `A` and `a` on a case-insensitive host), never merged; every folder on a record's way must already be a real directory | [`staged.rs`](../../../../control/crates/layerfs-store/src/project/staged.rs) `write_exported` |
| Bounds | part at most 2 MiB; file piece at most 256 KiB; directory entries read 64 at a time | [`export/walk.rs`](../../../../control/crates/layerfs-store/src/project/export/walk.rs) |
| Progress | `prepare`, `export`, `finalize`, at most every 200 ms | [`export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs) |

## Delete (`delete/`)

`project.delete(project)` retires project `project` in its Store, here or in
the Store's worker, and removes only the caller's home entry; other projects
stay. The Stack's marker (`mark_stack_deleted`) is the commit point, so a
lost reply is finished by a same-home retry of the durable intent, whose
Store side classifies the marker as `Marked`, `Retired` or `Absent`. It is
refused `Busy` while a Sandbox of the project is recorded
(`layerfs-control` passes that check in), and `MountsActive`/`Busy`/
`Unresolved` while a live mount or operation holds the project scope
([`family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs),
[`delete.rs`](../../../../control/crates/layerfs-store/src/project/delete.rs),
[`layerfs-control/src/project.rs`](../../../../control/crates/layerfs-control/src/project.rs)).

A record the Store does not list is cleared by the same call: when the
Store answers `NoSuchProject` and its own list, read once more, does not
hold the record's stack id (or its name, for a record without one), the
home entry is removed and the call answers as a delete; a list that cannot
be read leaves the entry and the refusal. An Init through a machine's
worker whose outcome is not known leaves a record with no stack id: the
delete reads the Store's list once in that worker, retires the Stack it
names there, then removes the entry; a project the list does not hold only
loses its entry, and a lost answer is `Unknown` with the entry kept
([`delete/remote.rs`](../../../../control/crates/layerfs-store/src/project/delete/remote.rs)
`listed_on_worker`).

## Discovery (`discovery.rs`)

The side that runs a Sandbox call keeps a project's private discovery
records under the Sandbox's folder: the home a call names, the project's
folder and record, and the record file format. Calls on one folder take
turns on `serve.lock` (this process first, then the file lock), held
across a Sandbox start, a mount or a close; past 60 s the call is `Busy`,
not retried
([`discovery.rs`](../../../../control/crates/layerfs-store/src/project/discovery.rs)).

## Related

- [Stores and projects](README.md): records, routes and leases.
- [Project Init](../../engine/project-init/README.md): what the live add constructs.
- [Machines: workers and reach](../machines/workers-and-reach.md): the `project.*` worker operations.
- [API: project](../../api/sdk/project.md).
