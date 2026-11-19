# Checks

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

This page covers the home's records check (`home.records`, which
`home.doctor` makes: the scan, the record checks and a layout 2 home's
conversion blockers), the leftovers `home.leftovers` lists and
`home.prune`, the one call that removes one, and the report all six doctors
answer, within [Doctor](README.md).

| | |
| --- | --- |
| Source | [`layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor): `host_target.rs`, `records.rs`, `scan.rs`, `checks.rs`, `convert.rs`, `leftovers.rs`, `leftover_list.rs`, `leftover_prune.rs`, `report.rs`, `check_report.rs` |

## The records check

```text
 home.doctor -> home.layout
   detect(home)             layout read only
     unreadable    -> home.layout fails
     Empty         -> no home.layout, no home.records; the home stays absent
     Old (1.1.0)   -> home.layout warning, fix `layerfs home init`; no records
     Previous (2)  -> home.layout warning; home.records = convert::check
     Current (3)   -> home.records = scan -> checks::check_local
   inventory fingerprint before and after: changed or unreadable -> no verdict
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
| no verdict | a record or lease that cannot be read, a record set that changed while it was read, or the home's budget ending: `home.records` is `not_checked` | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) `observed` |
| leftovers | in no report: `home.leftovers` lists them; see [Leftovers](#leftovers) | [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs) |

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
| Workspace folder | `warning` for `workspace/<id>/` with no `<id>.json` beside it (its Workspace was unmounted); fix `rm -r <folder>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |
| command record | `warning` when its owner process is gone; fix `layerfs workspace cancel <sandbox>:<workspace> --exec <id>` (the message adds that the Workspace's unmount or the Sandbox's removal also ends it); `error` when a file under `exec/` does not read as one (no fix: repair or remove it by hand) | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |

Hidden (dot) entries are skipped. The scan ends no command and removes no
record. Built-in `native` and `docker` count as present without a record.

## Record checks

```text
 Scanned -> machines   daemon file, op
         -> stores     op, placement, partial copies, leftover copy, leases
         -> sandboxes  op, project, Store, machine
```

| Entity | Finding | Severity | Fix it names |
| --- | --- | --- | --- |
| machine | recorded `daemon` file missing | error | none; the message names `layerfs machine update <m> --daemon <path>` |
| store | record cannot be read | error | none: repair the record by hand |
| store | SQLite Store on a non-built-in machine with no record | error | none: restore the machine's record, or delete the Store folder by hand |
| store | 1.1.0 Store whose machine is unknown | warning | `layerfs import-env` |
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

## Conversion

```text
 layout 2 home -> layout_2_blockers(home)   the conversion's own survey
   each blocker: live lease or Store move/removal  -> store
                 live or uncertain Sandbox          -> sandbox
                 create intent of unknown outcome   -> project
                 machine removal                    -> machine
     -> error on that thing, no fix: the message says what ends it
 home.layout says the conversion is still to come (warning, fix `layerfs home init`)
```

| Case | Finding | Source |
| --- | --- | --- |
| survey fails | `error` on the home: the first call on the home reports what refuses the conversion | [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs) |
| blocker | `error` naming the state and the step that ends it (let the process finish, close the Sandbox with the build that started it, remove what an intent left); the survey's messages come from [`migrate/v2.rs`](../../../../control/crates/layerfs-control/src/migrate/v2.rs) `survey`, whose Sandbox step still names that older build's command, `layerfs sandbox close <x>` | [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs) |
| conversion still to come | `home.layout` warning, "a layout 2 home: it is converted on its first call; nothing was converted" | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |

Stale leases stay; nothing is converted.

Worker reach and Sandbox daemon reach are not records checks: a machine's
worker is `machine.doctor`'s (`machine.worker.*`), a Sandbox's daemon
`sandbox.doctor`'s (`sandbox.daemon_*`), and disk room `home.capacity` and
`machine.capacity` ([Doctor](README.md)).

## Leftovers

What an interrupted operation left on disk, found by one bounded walk of
each side's own folders, listed by `home.leftovers` and removed only by
`home.prune`. No doctor's report holds them.

```text
 layerfs home leftovers [--machine M]        layerfs home prune <leftover>
   native: walk(home)                          parse <machine>:<kind>:<name>
   each recorded machine (or M):               place::on_machine(machine)
     stopped / held -> not_listed (blocked_by)   native -> prune_on(home, ..)
     place::on_machine -> worker                 worker -> home.prune
       doctor.sandbox {leftovers: true,                   {kind, name, known}
                       known, timeout_ms}      derive the path from kind and
         walk(worker root)                     name, find the entry again,
       no answer / walk at its bound           check its guard, remove, once
         -> not_listed (reason: timeout)       -> { freed_bytes, outcome:
   -> { items: [{ leftover, modified_at,           "removed", leftover, kind,
        size, report_only, removable_at,           machine, path }
        kind, machine, path }],
        not_listed, observed_at }
```

The walk is a read: it takes no lock, lease or daemon connection, follows no
link, and opens no file but a run folder's `owner` marker and the Sandbox
records. It lists only where LayerFS puts its own entries, never the home or
the worker root itself, so what an integration keeps in a home (`secret/`,
`runtime/`, `job/`, `stat/`, `preset/`) is neither listed nor read. It never
opens a Store's files, a lock or lease file or an overlay database, never
enters or `stat`s a run folder's `workspaces/` (its Workspace mount points),
and does not descend into a directory on another device. Nothing is started:
a machine that is stopped or held by an operation is in `not_listed` with
`blocked_by`, one that does not answer or whose walk stopped at its bound
with `reason: timeout` (what that walk found is still listed), and a refused
ask with `reason: busy` or `refused`. When a machine's worker does not
answer, its provider is asked once, bounded like the walk, with the
provider-only probe (`provider_within`: no worker dial, no guest command);
a VM or container it says is stopped is `blocked_by {machine, reason:
"stopped"}` ("machine `<m>` is stopped"), else the entry is `reason:
timeout`
([`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs) `provider_stopped`). An unknown `--machine` is `NotFound`.

| Kind (`kind`) | Entry | Listed when | `home.prune` removes it when (else `InUse`) |
| --- | --- | --- | --- |
| `run_folder` | `machine/native/run/<name>/` | no Sandbox record names it: none of this side's records holds it as its daemon's run folder, and its Sandbox has no record that is still being created, here or (sent as `known`) in the caller's home | no record names it; its `owner` marker is there and older than 60 s; the record that marker names has no live daemon (pid, start time and boot); the mount table lists nothing at or under it or its `<name>.root`. Removed entry by entry, through no link, entering only directories on the folder's own device |
| `empty_root` | `machine/native/run/<name>.root/` | it is empty and its run folder `<name>/` is gone | its run folder is gone and it is not a mount point (mount table, and its device is its parent's); `rmdir` only |
| `init_copy` | `incoming/init-<16 hex>/` (a worker root) | always; `report_only` unless a line of the home's `log/uncertain.jsonl` names the machine and the copy with a deadline | more than an hour has passed since that line's `deadline` (`removable_at`); then the worker's own `project.stage` discard removes it. A copy no line names is report-only (`InvalidInput`) |
| `backup_staging` | `store/<s>/backup.staging` (the home), `store/<s>/store.sqlite.backup` (the Store's machine) | the file is there and no live process holds an operation on that Store | the Store's `backup.lock` in the home can be taken without waiting; it is held for the removal |
| `image_tmp` | `machine/native/run/images/.tmp-<hex>.2-<id>/` | always | the image's `<hex>.2.lock` can be taken without waiting; the lock file stays |
| `export_container` | an image export container in the host's Docker: labelled `layerfs.role=image-export` with this home's id, or a legacy unlabelled `layerfs-export-<machine>-<hex>` (listed with `native`'s entries; `path` and `size` null) | an image copy has run from this home (`run/image-lock/` exists) and Docker answers; `report_only` while the copy lock's owner record of its image and machine names a live process. Docker not answering puts `native` in `not_listed` (`blocked_by {machine: docker, reason: engine_down}`); no Docker lists none | the copy lock of its image on its machine can be taken without waiting; `docker rm -v` removes it with its anonymous volumes ([`image_exports.rs`](../../../../control/crates/layerfs-sandbox/src/image_exports.rs)) |

Image `<hex>.2.lock` files and Workspace `.unmount` markers are LayerFS's own
and are neither listed nor removed. A copy's staged `.recv-<hex>.part` is not
a leftover (spec v19): `machine.images` shows it as a `broken`/`partial` item
and `machine.prune --unused` removes it.

| Field or rule | Meaning | Source |
| --- | --- | --- |
| `size` | the file's size, or of every file under the folder (a run folder without its `workspaces/`); `null` when a bound ended the walk before it was measured | [`leftovers.rs`](../../../../control/crates/layerfs-control/src/doctor/leftovers.rs) `measure` |
| `modified_at` | the newest modification time (`mtime`) of the entry and of what was measured under it | [`leftovers.rs`](../../../../control/crates/layerfs-control/src/doctor/leftovers.rs) `measure` |
| `leftover` | `<machine>:<kind>:<name>`, the same for an entry in every list; `name` is the run folder's or root's name, `<store>/<file>`, the copy's 16 hex digits, `<hex>.2-<id>` or the part's digest. It admits no separator, `..` or leading dot, and holds no path | [`leftovers.rs`](../../../../control/crates/layerfs-control/src/doctor/leftovers.rs) `id`, `parse`, `Kind::path` |
| `report_only`, `removable_at` | `report_only` for a folder copy no uncertain line names with a deadline; `removable_at` is that deadline plus one hour | [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs) |
| bound | each side's walk stops after 20,000 directory entries or 2 s (`BOUNDS`); the machine is then also in `not_listed` (`reason: timeout`) | [`leftovers.rs`](../../../../control/crates/layerfs-control/src/doctor/leftovers.rs), [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs) |
| the uncertain line | a `project init` whose part or install answer is lost appends `{ at, call: "project.init", store, project, machine, copy, phase, deadline, message }`: `deadline` is when the lost request went out plus the Init's `timeoutMs` | [`send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) `log_left`, [`parts.rs`](../../../../control/crates/layerfs-store/src/project/parts.rs), [`family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |
| a worker's answer | read as data: a kind or name the walk does not give is dropped, and the path shown is never used to remove | [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs) `List::item` |

`home.prune` is the one mutation here: `layerfs home prune <leftover>`. It
makes one attempt and sends nothing twice.

| `home.prune` answers | When | Source |
| --- | --- | --- |
| `{ freed_bytes, outcome: "removed", leftover, kind, machine, path }` | the entry was removed (`freed_bytes` null for a folder copy and an image temporary) | [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) `prune` |
| `NotFound` | a leftover no walk gives (a path, `..`, an unknown kind or machine), an entry that is gone, or a link under the entry's name or on the way to it | [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) `prune_on`, `beneath` |
| `InUse` | the kind's guard does not hold; the message says which part | [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) |
| `InvalidInput` | a folder copy that is only reported | [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) |
| `Unavailable` | the machine is stopped; nothing starts it | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `on_machine` |
| `Unknown` | the worker's answer was lost; the entry may or may not be there, and `layerfs home leftovers` shows which | [`client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

The image temporary is removed by the unpack's own taker
([`rootfs/cache.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/cache.rs)
`remove_temporary`) and the staged snapshot under `backup create`'s own
lock ([`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs)
`Creating::take`). A Sandbox's private root is mounted only inside that
Sandbox's own mount namespace, so the home's mount table cannot show it: the
`empty-root` guard therefore also requires the run folder to be gone, and
the `run-folder` guard the daemon to be gone.

## Check report

Every doctor answers one report ([`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs)).
A kind's checks write rows into `Checks`
([`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs));
`Checks::fill` turns them into the thing's node.

```text
 long form (--detail):
 { "ok", "findings": [ { <thing>, check, severity, message, fix?, from?, evidence } ],
   "not_checked": [ { <thing>, check?, blocked_by? | reason? } ],
   "status", "counts": { status: things }, "observed_at",
   "tree": { <thing>, status, coverage, checks: [ { check, outcome, from? } ],
             blocked_by?, reason?, machines|stores|projects|sandboxes|workspaces|buckets: [ node ] } }
 short form: { "ok": true } alone, or { findings?, not_checked? } (each only with entries)
```

| Row written | Node outcome | Goes to | Source |
| --- | --- | --- | --- |
| `pass` | `pass` | nothing more | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| `fail` with severity `error`, `warning` or `notice` | that severity | `findings`, with the row's `fix` and `from` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| `unknown` (incl. `lost`, `timeout`, `busy`) | `not_checked` | `not_checked` with `reason`; with none given, `timeout` when the budget ended, else `blocked_by` the thing itself (`failed`) | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| `blocked` by something outside the thing (`waits`) | `not_checked` | `not_checked` with `blocked_by` the root of the block | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| `skipped`, or `blocked` by a failed check of the same thing | left out | nothing: the check does not apply | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |

| Field | Rule | Source |
| --- | --- | --- |
| `ok` | true only when every check was made and nothing was found; a notice makes it false | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `status` | a thing's: `ok` when every check passed, else the worst of `error`, `warning`, `not_checked`, `notice`; the answer's: the worst thing in it | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `counts` | things per status, each counted once | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `coverage` | `none` (blocked or no verdict), `shallow`, or `full` when a check of the kind's Full list was actually made | [`frame.rs`](../../../../control/crates/layerfs-control/src/doctor/frame.rs) `visit` |
| one row per check | repeats of a check (and `from`) merge into one row with the worse outcome; each still gives its own finding | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| `fix` | one runnable shell line, present only when a command fixes it | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) `Finding` |
| retention | at most 1024 rows per `Checks`; messages cut at 4096 characters; evidence over 16 KiB replaced by `{ truncated: true }` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| list membership | a check id outside its kind's list panics in a debug build | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |

Callers redact evidence before adding it; `evidence` appears in the long
form only.

## Related

- [Doctor](README.md): the six doctors, each kind's checks, invariants and limits
- [Home](../home/README.md): the records, modes, intents and leases the scan reads
- [Stores and projects](../stores-and-projects/README.md): placement, leases and transfers
- [Workers and reach](../machines/workers-and-reach.md): the hello frame
- [Sandboxes](../sandboxes/README.md): Sandbox records and operations
- [SDK: home family](../../api/sdk/home.md#homedoctor): `home.doctor`, [`home.leftovers`](../../api/sdk/home.md#homeleftovers) and [`home.prune`](../../api/sdk/home.md#homeprune)
