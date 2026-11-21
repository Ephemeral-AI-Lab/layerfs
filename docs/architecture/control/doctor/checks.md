# Checks

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

This page covers the home's records check (`home.records`, which
`home.doctor` makes: the scan and the record checks), where LayerFS removes what interrupted operations
left on disk, and the report all six doctors answer, within
[Doctor](README.md).

| | |
| --- | --- |
| Source | [`layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor): `host_target.rs`, `host_identity.rs`, `records.rs`, `scan.rs`, `checks.rs`, `report.rs`, `check_report.rs` |

## The records check

```text
 home.doctor -> home.layout
   detect(home)             layout read only
     unreadable    -> home.layout fails, the OS error, no fix
     Empty         -> no home.layout, no home.records; the home stays absent
     Foreign       -> home.layout fails, fix `mv <home> <home>.old && layerfs home init`;
                      no records (not a LayerFS 2.0 home)
     WorkerRoot    -> home.layout fails, fix `unset LAYERFS_HOME`; no records
                      (a machine's data disk, never moved)
     DataWithoutMarker -> home.layout fails, fix `layerfs home init --adopt`;
                      no records (a home's data with no layerfs.json, kept)
     Current (3)   -> home.layout passes; home.records = scan -> checks::check_local
   inventory fingerprint before and after: changed or unreadable -> no verdict
   when the scan gave a stable verdict:
     id cross-check (host_identity): layerfs.json's id against every machine
     record's vm_name and every sandbox record's daemon name
       mismatch -> home.layout fails, error, no fix
   -> Findings [ { severity, entity {kind, name}, message, fix } ]
   -> each one row of home.records on the thing it is about
```

| Field or case | Meaning | Source |
| --- | --- | --- |
| a finding | one `home.records` row: an `error` or `warning` finding on its own thing (`checks.about`), with `fix` when one runnable line repairs it; empty `fix` when none does, the message then saying what to do | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) `publish`, [`records.rs`](../../../../control/crates/layerfs-control/src/doctor/records.rs) |
| `severity` | `error`: a call on the thing is refused or does the wrong thing; `warning`: left over or out of date, nothing is refused for it | [`records.rs`](../../../../control/crates/layerfs-control/src/doctor/records.rs) |
| thing | `home`, `machine`, `store`, `project`, `sandbox`, `workspace` or `bucket`; a command record's finding names its Workspace | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs), [`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs), [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `about` |
| order | findings in the order the checks made them | [`records.rs`](../../../../control/crates/layerfs-control/src/doctor/records.rs) |
| nothing found | `home.records` passes | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| id mismatch | `home.layout` fails (error, no fix): a machine `vm_name` or a Sandbox `daemon.name` is not the provider name this home's `layerfs.json` id gives. The message names the record path and the recorded and expected names; no line fixes it, since moving a whole home aside would not repair a record or a marker that belongs to another home. A bare home, a record with no provider name (`native`, an earlier machine, an unstarted Sandbox) and a deleted record pass. Eight-hex names (`layerfs-<id8>-`, `lfs-<id8>-`) compare only their first 8 digits (32 bits) | [`host_identity.rs`](../../../../control/crates/layerfs-control/src/doctor/host_identity.rs), [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) `matches_provider_name` |
| no verdict | a record or lease that cannot be read, a record set that changed while it was read, or the home's budget ending: `home.records` is `not_checked` | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) `observed` |
| leftovers | in no report: LayerFS removes them itself; see [Leftovers](#leftovers) | [`sweep.rs`](../../../../control/crates/layerfs-sandbox/src/sweep.rs) |

The check takes no lock, so a home that changes meanwhile may be reported
part way; the inventory fingerprint only catches a change that is still
there at the second read. It dials no worker or daemon.

## Scan

```text
 home/ layerfs.json   machine/ store/ sandbox/ backup/   folder modes
   machine/<m>/  record.json (v 1)   worker.json
   store/<s>/    record.json (v 1)   project/<p>/record.json (v 1)
   sandbox/<x>/  record.json (v 1)   control.json
                 workspace/<id>.json (v 1)
                 workspace/<id>/exec/<exec id>.json (v 1)
   backup/<b>/   record.json (v 1)
 -> Scanned { machines, machine_names, stores, store_names,
              projects, sandboxes }
```

| Read | Finding | Source |
| --- | --- | --- |
| each record | `error` when it does not parse ("repair or remove it by hand"), or when an entity record's `v` is not 1 ("use the LayerFS build that wrote it") | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `record` |
| record file mode | `error` when group or others may read it; fix `chmod 600 <path>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `file_mode` |
| folder mode | `error` when others may list or change it or its group may use it (0701 is allowed, for a namespaces Sandbox, except on a Store's own folder; a symlink is reported as a link with `make it a real folder`, never `chmod`, which would change the target); fix `chmod g-rwx,o-rw <path>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `folder`, `link` |
| Store folder mode | `error` when a Store's own folder has any group or other bit (`0701` is not allowed; fix `chmod 700 <path>`) or is a symlink (scanned under `store/`, reported without descending into `record.json` or `project/`, and without suggesting `chmod`) | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `store_folder`, `store_names` |
| Store folder | `error` on each project in a Store folder with no Store record; fix: restore the record or delete the folder by hand | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `store` |
| Project folder | `error` on each real project folder whose Store record exists but whose `record.json` does not; the fix is `layerfs project prune <store>/<project>`, which removes only such a folder. A symlink entry is skipped (the scan descends no link) and is never a prune fix | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `store` |
| Workspace folder | `warning` for `workspace/<id>/` with no `<id>.json` beside it (its Workspace was unmounted); fix `rm -r <folder>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |
| command record | `warning` when its owner process is gone; fix `layerfs workspace cancel <sandbox>:<workspace> --exec <id>` (the message adds that the Workspace's unmount or the Sandbox's removal also ends it); `error` when a file under `exec/` does not read as one (no fix: repair or remove it by hand) | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |

Hidden (dot) entries are skipped. The scan ends no command and removes no
record. Built-in `native` and `docker` count as present without a record.

## Record checks

```text
 Scanned -> machines   daemon file, op
         -> stores     op, machine record, partial copies, leftover copy, leases
         -> sandboxes  op, project, Store, machine
```

| Entity | Finding | Severity | Fix it names |
| --- | --- | --- | --- |
| machine | recorded `daemon` file missing | error | none; the message names `layerfs machine update <m> --daemon <path>` |
| store | record cannot be read | error | none: repair the record by hand |
| store | SQLite Store on a non-built-in machine with no record | error | none: restore the machine's record, or delete the Store folder by hand |
| store | stale lease of an ended process (read, never removed) | warning | `rm <home>/store/<s>/open/<pid>` (the next call on the Store also removes it) |
| store | local `store.sqlite.incoming` | warning | `rm <path>` |
| store | a copy a move did not retire (from the record) | warning | none: the next `layerfs store move <s>` deletes it first |
| sandbox | record names no project, or its Store, project or machine is gone | error | `layerfs sandbox remove <x>` |
| machine | ended owner's uncertain `op` | error | `layerfs machine remove <m>` after a create or remove, else `layerfs machine get <m> --with live` (`layerfs_machine::state::settle`) |
| store | ended owner's uncertain `op` | error | the Store's clearing call: `layerfs store move <s> <to>`, `store remove <s>`, `store compact <s>`, else `store get <s> --with live` (`intent::clearing`) |
| sandbox | ended owner's uncertain `op` | error | `layerfs sandbox remove <x>` |

A live owner's `op` is not a finding, and neither is an ended owner's
certain one: it holds nothing, every read ignores it and the next operation
replaces it. A running Store transfer owns its partial copy, so `incoming`
is not read while one runs; an unresolved intent's own finding names its
clearing call, so no leftover copy is reported then. `checks.rs` `leftover`
skips a copy on its target only when the leftover's call is `store.push`
(meant for `incoming` to report); `resolve::leftover` names every transfer
`store.move` now, so a move's target-side partial copy is reported as a
copy the move did not retire. Source:
[`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs),
[`resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs).

The records check runs these through `check_local`, which skips the remote
`stat`: a move target's `store.sqlite.incoming` on another machine (read by
the worker's `stat`, fix `layerfs store move <s> <machine>` or `layerfs store
move <s> native`) is in `check_scope`'s remote branch, which no doctor calls
today.

## Foreign homes and worker roots

A folder that is not a LayerFS 2.0 home (a `layerfs.json` of another
layout, or anything but `layerfs.lock`, the integration folders and
`.DS_Store` with no `layerfs.json`) gets no
records check: `home.layout` fails, "not a LayerFS 2.0 home: this build
writes nothing in it; move it aside and make a new home", with the fix
`mv <home> <home>.old && layerfs home init`, and `home.records` is not run.
The conversion and its blocker survey were removed (owner, 2026-11-19;
[`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs)
`layout`, [opening a home](../home/README.md#opening-a-home)).

A folder whose entries are a home's own data and which has no
`layerfs.json` is not foreign and is not moved: it is `DataWithoutMarker`
(`store/`, `machine/`, `sandbox/`, `backup/`, `secret/`, `trace/`, `log/`,
`incoming/`, `generation`, at least one and each data name a folder but
`generation`; layerfs-project#752, c6). `home.layout` fails, "a LayerFS
home's data with no layerfs.json: this build writes nothing in it; adopt it
and keep the data", with the fix `layerfs home init --adopt`, and
`home.records` is not run. The adopt writes only the marker; the doctor
still has no records verdict until the next run.

A machine's data disk that holds a worker's content is not a foreign home
and is not moved. Its shape, read only and before the empty/foreign
decision: at least one of `store/`, `sandbox/` and `machine/native/run/`;
`incoming/init-<16 hex>/` or `log/worker.log`; and no `trace/`, `backup/`
or `layerfs.json` ([`layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs)
`worker_root_shape`). `home.layout` fails, "a machine's worker root, not a
LayerFS 2.0 home: this build writes nothing in it; point `LAYERFS_HOME` at
your home", with the fix `unset LAYERFS_HOME`; `home.records` is not run
and the doctor writes nothing. A real home missing its `layerfs.json`
keeps `trace/` or `backup/` and is not a worker's root: it is
`DataWithoutMarker`, and `home.layout` names the adopt fix instead.

A current-layout home whose id disagrees with its records is not a foreign
home and is not moved: `home.layout` fails with no fix, naming the record
path and both the recorded and expected names, because the record or
`layerfs.json` came from another home. The cross-check reads only what the
records scan already holds and runs only when that scan gave a stable
verdict ([`host_identity.rs`](../../../../control/crates/layerfs-control/src/doctor/host_identity.rs)).

Worker reach and Sandbox daemon reach are not records checks: a machine's
worker is `machine.doctor`'s (`machine.worker.*`), a Sandbox's daemon
`sandbox.doctor`'s (`sandbox.daemon_*`), and disk room `home.capacity` and
`machine.capacity` ([Doctor](README.md)).

## Leftovers

What an interrupted operation left on disk, LayerFS removes itself (spec
v20, owner 2026-11-19): a later call sweeps it at a fixed point, and only an
entry it can prove nothing owns. No call lists or removes them and no
doctor's report holds them; `home.leftovers` and `home.prune` were removed
with no alias. `machine stats --with space` still counts run folders as
`other_runs`.

```text
 machine start (worker serve)  ----------> run root sweep (worker root)
 sandbox create on a machine (worker) ---> run root sweep (worker root)
 sandbox create on native (this process) -> run root sweep (the home)
 machine load-image, sandbox create --image, when it copies an image:
   PruneGate, own ImageLock -> export containers of free images (Docker)
                            -> image.artifact step "sweep": .tmp-* of free images
 backup create (Creating lock + lease) ---> backup.staging, store.sqlite.backup
 backup restore replacing the Store (lease) -> the same, in the home
 the call that settles an Init ----------> its incoming/init-<id> copy
 each removal or failure -> one line of <side root>/log/sweep.jsonl
```

Each sweep is one attempt and never fails the call it rides on: a removal
that fails is logged (`{ at, kind, path, outcome: "failed", message }`) and
left for the next sweep, and a removed entry is logged with
`outcome: "removed"` (and `bytes` when measured). The run root sweep stops
after 20,000 directory entries or 2 s (`BOUNDS`); the image sweep gets the
same time bound; a removal already started finishes. A sweep takes no lease
or daemon connection of its own, follows no link, never opens a Store's
files, a lock file or an overlay database, never enters a run folder's
`workspaces/` while only listing it, and does not descend into a directory
on another device. What an integration keeps in a home (`secret/`,
`runtime/`, `job/`, `stat/`, `preset/`) is never swept.

| Kind | Entry | Swept by | Removed when (else kept) | Test |
| --- | --- | --- | --- | --- |
| `run_folder` | `run/<name>/` in a side's run root (`machine/native/run` in the home or a worker root) | a worker when it starts serving ([`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) `serve`, before it accepts) and before each `sandbox create` it runs ([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `in_worker`); on `native`, before `sandbox create` ([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_op`); all through [`sweep.rs`](../../../../control/crates/layerfs-sandbox/src/sweep.rs) `run_root` | no Sandbox record of that side names it and none still being created; its `owner` marker is there and older than 60 s (`OWNER_QUIET`); the record that marker names is this side's and has no live daemon (pid, start time and boot), or is gone; the mount table lists nothing at or under it or its `<name>.root`. Removed entry by entry, through no link, only on the folder's own device, the `owner` marker last | `sweep_run_root.rs`, `sweep_points.rs` `a_sandbox_create_on_native_sweeps_its_run_root_first` |
| `empty_root` | `run/<name>.root/` | the same sweep | it is empty, its run folder `<name>/` is gone and it is not a mount point (mount table, and its device is its parent's); `rmdir` only | the same |
| `image_tmp` | `run/images/.tmp-<hex>.2-<id>/` in a machine's image cache | a copy of an image to that machine, under the machine's prune gate after it took its own copy lock: [`image_sweep.rs`](../../../../control/crates/layerfs-sandbox/src/image_sweep.rs) `sweep` sends the `image.artifact` step `sweep` ([`image_artifact.rs`](../../../../control/crates/layerfs-sandbox/src/image_artifact.rs), `image_sweep::temporaries`) | the image's copy lock in the home is free (images a live copy holds are sent as `locked`) and its unpack lock `<hex>.2.lock` can be taken without waiting ([`rootfs/cache.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/cache.rs) `remove_temporary`); the lock file stays | `image_sweep.rs`, `lp176_image_export.rs` |
| `export_container` | an image export container in the host's Docker labelled `layerfs.role=image-export` with this home's id | the same copy, before the step ([`image_sweep.rs`](../../../../control/crates/layerfs-sandbox/src/image_sweep.rs) `exports`) | the copy lock of its image on its machine can be taken without waiting (the copy's own image and machine: under the lock it holds); `docker rm -v` removes it with its anonymous volumes. An unlabelled container of an older build names no home and is left alone | `lp176_image_export.rs` `real_docker_next_copy_sweeps_a_killed_copy_without_touching_active` |
| `backup_staging` | `store/<s>/backup.staging` and `store/<s>/store.sqlite.backup` in the home; `store.sqlite.backup` beside the Store on its machine | the next `backup create` of that Store, under its `backup.lock` and lease ([`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) `stale_staging`, the machine's file by `StoreStep::Unstage`); a `backup restore` that replaces the Store, under its lease (`stale_on_native`) | the Store's `backup.lock` is held by the sweeping call (a running create holds it: the restore keeps them, a second create is `Busy`) | `sweep_points.rs` `the_next_backup_create_or_restore_removes_stale_staging` |
| `init_copy` | `incoming/init-<16 hex>/` in a worker root | never by a guess: the call that settles its Init. The Init removes its copy when it ends; `project delete` of a project whose Init's owner is gone sends the worker a `project.stage` discard for the copy the Init's op recorded (`{copy, machine}`; [`project.rs`](../../../../control/crates/layerfs-control/src/project.rs) `discard_copy`) | nothing holds the copy's folder lock ([`staged.rs`](../../../../control/crates/layerfs-store/src/project/staged.rs) `hold`, which a running Init holds while it adds); a held copy answers `{discarded: false, held: true}` and stays | `init_copy_hold.rs`, `worker_fault_store.rs` `a_lost_init_names_its_copy_and_its_delete_removes_it` |

Image `<hex>.2.lock` files and Workspace `.unmount` markers are LayerFS's own
and are never swept. A copy's staged `.recv-<hex>.part` is not a leftover
(spec v19): `machine.images` shows it as a `broken`/`partial` item and
`machine.prune --unused` removes it, as it removes `.tmp-*`. A Sandbox's
private root is mounted only inside that Sandbox's own mount namespace, so
the home's mount table cannot show it: the `empty_root` rule therefore also
requires the run folder to be gone, and the `run_folder` rule the daemon to
be gone.

## Related

- [Doctor](README.md): the six doctors, each kind's checks, invariants and limits
- [Home](../home/README.md): the records, modes, intents and leases the scan reads
- [Stores and projects](../stores-and-projects/README.md): placement, leases and transfers
- [Workers and reach](../machines/workers-and-reach.md): the hello frame
- [Sandboxes](../sandboxes/README.md): Sandbox records and operations
- [SDK: home family](../../api/sdk/home.md#homedoctor): `home.doctor`
