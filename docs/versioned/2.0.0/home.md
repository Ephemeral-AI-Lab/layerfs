# The home

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Source:
`control/crates/layerfs-home/src/home/`.

The home is LayerFS's control center. Every record a host process needs is
in it: machines, Stores, projects, Sandboxes and buckets. Only host
processes read or write it. No guest (a VM or a container)
mounts or reads it; the caller sends a worker what each call needs and
writes every record itself.

## Where it is

| Setting | Meaning |
| --- | --- |
| `LAYERFS_HOME` | the home folder. Unset, it is `~/.layerfs` (O28) |
| `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | tracing. `LAYERFS_TRACE` names a file: each connection to a machine's worker appends `{at, event: "worker.connection", op, worker, sent, ms}` to it. With `LAYERFS_TRACE_STEP` also set (any non-empty value), each call appends one `{at, event: "step", op, step, us, ms}` line per step it timed: the caller's (`dispatch`, `route`, `connect`, `ready`, `request`, `answer`) and, for a worker-routed call, the worker's as `worker.<step>`, which the worker sends back in its answer (a namespaces `workspace.exec` adds `worker.exec.*` and its helper's `worker.helper.*` stages). `LAYERFS_TRACE_START` is still not read |
| `LAYERFS_CONSTRUCTION_WORKERS` | measurement |

LayerFS reads no other user setting. A Node or Python client can name
another home: `layerfs.client({ home })`, `layerfs.client(home=...)`.

A PostgreSQL login and a bucket's secret key are not settings either: they
are kept in the home's secret folder; see
[machines, Stores and backups](machines-and-stores.md#secrets).

## Layout

```
<home>/
  layerfs.json                 layout version and home id
  layerfs.lock                 the home lock (host only)
  machine/<name>/
    record.json                kind, size, lasting state, op (while a create or remove runs)
    worker.json                endpoint, secret, live forward (0600)
    start.lock                 taken while a process starts, makes or removes the machine
    forward.log, forward.1.log what a lima machine's ssh forwards said, and its one rotated file
  machine/native/run/          native Sandboxes' run folders, and their image cache
    layerfs-<id16>-<sandbox>/  one run folder per Sandbox
    layerfs-<id16>-<sandbox>.root/  its private root (Linux; empty from outside)
    images/<64 hex>.2/         an unpacked image, with <64 hex>.2.lock and <64 hex>.2.json
  sandbox/<name>/
    record.json                v, name, project, machine, backend, route, ..., op?
    control.json               the Sandbox daemon's keys (0600; native, and machines reached directly)
    lock                       the Sandbox lock: its record and Workspace record changes
    serve.lock                 held by the call that runs the Sandbox (native, and machines reached directly)
    workspace/<id>.json        one per mounted Workspace: v, id, name?, branch, path, namespace, created
    workspace/<id>.unmount     who owns an unmount of it, with <id>.unmount-lock; kept until the Sandbox is removed
    workspace/<id>/exec/<x>.json  one per running command (exec id <x>): owner, route, stop handle
  store/<name>/
    record.json                kind, machine, profile, backup links
    lock                       the Store lock
    store.sqlite               a sqlite Store's file while it is on native
    store.sqlite.incoming      the partial copy of a store move to native, renamed to store.sqlite when verified
    backup.lock                held by a backup create of this Store
    backup.staging             a backup create's snapshot fetched from a machine, removed when it ends
    store.sqlite.backup        a backup create's snapshot staged beside the file, removed when it ends
    open/<pid>                 leases of processes that hold it open
    project/<name>/
      record.json
  backup/<bucket>/record.json  provider, endpoint, bucket, prefix, region, access key id
  trace/                       calls.jsonl (the call log), calls.1.jsonl, calls.lock
  log/                         uncertain.jsonl, leases.jsonl, exec.jsonl,
                               <name>.1.jsonl (a rotated log), rotate.lock
```

A machine's data disk, `/layerfs/data` inside the machine, is its
worker's root:

```
/layerfs/data/
  .layout-3                    historical: what an earlier bundle left here
                               was cleared; this build writes no such marker
  store/<name>/                record.json (v, name, kind), store.sqlite, store.sqlite.incoming
                               (a store move's partial copy), store.sqlite.backup
  sandbox/<name>/              record.json, control.json, lock, workspace/ as above
  machine/native/run/          the machine's Sandboxes' run folders and image cache, as above
  incoming/init-<16 hex>/      the folder copy of a project init, while it runs
  log/                         worker.log, worker.1.log (private to root), exec.jsonl,
                               exec.1.jsonl, rotate.lock
/layerfs/state/                private to root: secret (the worker's), worker.pid
```

- **Records** are one JSON value each, written to a private temporary file
  (named with the process id and a per-process counter, so threads never
  share one) and renamed into place, so a reader sees the old record or the
  new one. Every writer that reads, changes and writes a record holds a
  lock: the home lock where a name is claimed or another entity is checked,
  and the entity's own lock (`store/<s>/lock`, `sandbox/<name>/lock`) for
  its record's other changes. Locks are taken home first, then a machine's
  `start.lock`, then a Store's, then a Sandbox's (#536 §8).
  Files are 0600 and folders 0700. The run root `machine/native/run/` is
  0755. On Linux, where a Sandbox's commands run as another user, the
  folders above the run root (`machine/native/`, `machine/` and the home)
  also let others pass through, never list: 0701. `layerfs home doctor` accepts
  0700 and 0701 for the home's folders, reports any group bit or any read
  or write bit for others, and wants exactly 0700 for a Store's own folder
  and 0600 for a record. Bytes are stored as `{"hex": "…"}`.
  Every machine, Store, project, bucket, Sandbox and Workspace record
  carries `"v": 1` as its first field; call results never show it.
- **`layerfs.json`** holds `{"layout": 3, "id": "<32 hex digits>"}`. The id
  is written once. The `docker` machine names its worker container with it (O33),
  VM, container and volume names and Sandbox run folders carry its first 16
  digits (`layerfs-<id16>-<name>`; before layerfs-project#752 they carried
  the first 8, `layerfs-<id8>-`, and the oldest names were `lfs-<id8>-`,
  which still read), and backups record it as where they were exported
  from. Eight hex digits pin only 32 bits, so a legacy record can prove no
  more than that two ids share their first 8 digits. `home.doctor`'s
  `home.layout` check compares the id with every machine's `vm_name` and
  every Sandbox's recorded daemon name, once the records scan is stable; a
  record or an id that came from another home fails it as an error with no
  fix (never `mv`: the record or marker, not the home, is what disagrees).
  A folder that is not a home of this layout is not converted or written
  ([below](#an-older-home)).
- **`layerfs.lock`** guards record writes and Sandbox-name uniqueness.
  Taking it waits up to 120 s for another holder; that is a readiness wait,
  not a retry. If it is still held, the call is refused `Unavailable` and
  nothing changes. A thread that already holds it takes it again at once.
  It is held only for checks and record writes, never across a machine
  start, a Store step, a download or a VM's deletion.
- **`machine/native/` and `machine/docker/`** exist from the first call and
  are never removed.
- **`store/<name>/open/<pid>`** is a lease. Every process that uses a sqlite
  Store holds one: the process that opens it, or, for a Store behind a
  worker, the host process that sends it calls; a PostgreSQL Store's
  in-process calls hold one too. `store move` and `store remove` read the
  leases under the Store's lock and refuse `InUse` while one is held. A
  lease is taken under the Store's lock, and refused `Busy` while a move or
  remove of the Store runs (`Unresolved` after one whose outcome is not
  known). It records its process's start time and boot: a lease whose
  process is no longer alive on this host, or whose id now names another
  process, is stale; a move, remove or disconnect of the Store removes
  it under the Store's lock and logs the removal (O35). Reads (`store
  list`, `store get`, the doctors) ignore it and change nothing.
- **`sandbox/<name>/`** is all a Sandbox has in the home, whatever its
  machine: listing Sandboxes reads `sandbox/*` and nothing else, and one
  Sandbox is one folder read. Its `record.json` holds facts: its name,
  project and machine and, once started, its id, `backend`, kernel `route`
  (`fuse`, or `nfs` on a macOS native Sandbox) and `runtime_id`; and,
  while a `sandbox.create` or `sandbox.remove` runs or after one lost its
  answer, one `op` intent (`{id, name, phase, args, owner: {pid,
  pid_start, pid_boot}, started, updated, uncertain}`, #536 §7.1). Its
  state is derived, never stored. Its `lock` is taken after the home lock
  for each change of the record and of its Workspace records, and never
  held across a machine or daemon call; the call that runs a native
  Sandbox takes turns on `serve.lock`, a different file. On `native` the side
  that runs the Sandbox adds what reopens its runtime (`daemon`) to the
  same file, and its daemon keys to `control.json`. A Sandbox on
  `lima` whose daemon the caller reaches directly (#535 G4)
  has its keys in `control.json` too, and a `daemon` of `{ forward:
  { address, machine, pid?, pid_start?, pid_boot? }, name, runtime,
  platform, image }`: the host loopback address the caller dials and, on
  `lima`, the `ssh -L` that carries it, named by id, start time
  and boot. Its Workspace records are then in the home's `workspace/`,
  not the machine's. `runtime_id` and `daemon` are never shown. The
  Sandbox's `id` is its name.
- **`machine/native/run/`** holds each native Sandbox's run folder
  (`layerfs-<id16>-<name>`; earlier `layerfs-<id8>-`, oldest `lfs-<id8>-`)
  and the image cache the namespaces backend
  unpacks once (`images/<64 hex>.2`; `.2` is the cache's layout version).
  A Sandbox's run folder is created by its create and deleted by its
  close. It holds the daemon's setup and log, the Workspaces' mount points
  and overlay databases (`overlay/<namespace>.sqlite`, where `namespace`
  is the number in the Workspace's record `workspace/<id>.json`), and
  `tmp/`, the `HOME` of the Sandbox's commands. Nothing limits what a
  Sandbox writes there: it shares the disk of the home, or of the
  machine's data disk, with the Stores.
- **Command records** (`sandbox/<name>/workspace/<id>/exec/<x>.json`)
  are written new when a command starts and removed when it ends, on the
  side that keeps the Workspace's record: here on `native` and for a
  Sandbox reached directly; in the machine's worker otherwise. Each names
  its owning process, its route (`local` or `machine:<name>`), how it
  is stopped, the log tag of the call that started it and the program its
  command starts with (never its arguments); `sandbox get` lists the ones
  still running. A record whose owner is gone is stale: the next exec of its
  id, cancel, unmount or close ends its command, removes it and appends it
  to **`log/exec.jsonl`**. The Workspace's folder goes with its terminal
  unmount.
- **`log/uncertain.jsonl`** lists every outcome that is not known
  (`Unknown`), with what was kept. A `project init` whose answer was lost
  names the folder copy that may remain on the machine (`machine`, `copy`,
  `deadline`); `project delete` of that project removes the copy. It is
  never rotated.
- **`log/sweep.jsonl`** lists each leftover LayerFS removed itself, and
  each removal that failed and waits for the next sweep: `{ at, kind, path,
  outcome, bytes, message }` ([`home`](calls.md#home-5)).
- **Log sizes.** Every other `log/*.jsonl` file rotates once past 16 MiB
  to `log/<name>.1.jsonl`, replacing the one before, as the call log
  does. A machine's `forward.log` is copied to `forward.1.log` and
  emptied when a forward starts and finds it past that size. A machine's
  worker log is renamed to `worker.1.log` only when its worker starts,
  never while it runs. A Sandbox's `daemon.log` is not rotated: it is new
  with the Sandbox's run folder and deleted with it, so there is no
  `daemon.1.log`.
- **Names outside the home.** A machine's VM or worker container is named
  `layerfs-<id16>-<machine>`, where `<id16>` is the first 16 hex digits of
  the home's id and every character of the machine's name that is not a
  letter or digit becomes `-`; its data volume is that name with `-data`
  (`docker`, `apple-container`). A Sandbox's run folder is
  `layerfs-<id16>-<sandbox>`. Every `machine create`, and a new home's
  `docker` machine, records the name as `vm_name`. An earlier build wrote
  `layerfs-<id8>-` and the oldest `lfs-<id8>-`: a machine `vm_name` that
  keeps one still reads, and a Sandbox is reopened under the current or
  the `layerfs-<id8>-` form (an `lfs-<id8>-` run folder is an earlier
  build's, which this build neither reopens nor deletes); this build
  computes no older form. Eight hex digits are only 32 bits, so such a
  name proves only that two ids share their first 8 digits;
  `matches_provider_name` reads it under that cap, and `home.layout`'s id
  cross-check accepts it for that much and no more. No name is computed
  for a record that lacks one (owner, 2026-11-19): a machine
  record without `vm_name`, made by an earlier build, reads `broken`
  `outdated`, and every machine call that needs its VM or container is
  refused `Failed` (a call on a sqlite Store on it `Unavailable`, with the
  Store's `blocked_by`) with the fix `layerfs machine remove <machine>`, which removes the
  record alone (the built-in `docker` is then recorded again with its 2.0
  name by the next call); nothing outside the home is touched. While
  Sandboxes or Stores are recorded on it, that remove is refused `InUse`,
  so the machine's state and those refusals show the `InUse` refusal's
  chain as their fix instead, which removes them first.
  Because a VM name keeps no punctuation, a new machine whose VM name
  another machine has is refused `Exists` (`a.b` after `a_b`). A new
  machine's name is at most 46 characters (a VM name is at most 63, the
  prefix takes 17); on `lima` it is shorter when the Lima home's path is
  long, since Lima refuses a socket path
  `<Lima home>/<VM name>/ssh.sock.<16 digits>` of 104 characters or more,
  and the refusal states the longest name that fits on the host. A
  Sandbox name ending in `.root` is refused `InvalidInput`
  (`<run folder>.root` is a Sandbox's private root). Three names keep
  forms of their own: a Lima data disk (`lfs<8 hex>`), a Sandbox's cgroup
  (`/sys/fs/cgroup/layerfs/<id>`) and the image build's temporary names.
- **Top-level names.** LayerFS owns `layerfs.json`, `layerfs.lock`,
  `machine/`, `store/`, `sandbox/`, `backup/`, `log/` and `trace/`. An
  integration that shares the folder keeps its own files under `secret/`,
  `runtime/`, `job/`, `stat/` or `preset/`: LayerFS creates none of them
  and no doctor lists or reports them.
- **Leftovers.** What an interrupted call left in these folders (a run
  folder with no record, a staged snapshot, a folder copy, an image
  temporary) is in no [doctor](calls.md#the-doctors)'s report: LayerFS
  removes it itself at fixed points in later calls, only where it can prove
  nothing owns it ([`home`](calls.md#home-5)).

### Where each record lives

| Sandbox on | Its record | Daemon keys (`control.json`) | Workspace and command records | Run folder |
| --- | --- | --- | --- | --- |
| `native` | the home | the home | the home | the home's `machine/native/run/` |
| `lima`, daemon reached directly (its port forwarded to the host) | the home, and the machine | both | the home | the machine |
| any other machine | the home (without `daemon`), and the machine | the machine | the machine | the machine |

Machine, project and bucket records are only in the home.

A machine's own disk (a VM's data disk) is mounted at
`/layerfs/data` inside the machine. It holds `store/<name>/store.sqlite`
with a thin `store/<name>/record.json` (`{"v":1,"name","kind"}`, written
only when it changed), and `sandbox/<name>/` for the Sandboxes on that
machine (`record.json`: `v`, `name`, `project`, `backend`, `runtime_id`
and `daemon`; `control.json`; `workspace/`), with their local directories
under `machine/native/run/`. The worker's own output is in
`/layerfs/data/log/worker.log` on every machine kind, docker included
(private to root; a start that finds it past 16 MiB renames it to
`worker.1.log` first). On its first start from a layout 3 bundle the
worker deletes what an earlier bundle left there, once, and logs each
deletion: numbered Sandbox folders (`store/*/project/*/sandboxes/`),
`machine/*/sandbox/`, old local directories in
`machine/native/sandbox-root/` (renamed `machine/native/run/`, its
`images/` kept) and full Store records (rewritten thin). The marker
`/layerfs/data/.layout-3` keeps it from running again: that marker is a
historical mechanism, this build does not write it, and a data disk
without one is still a worker's root by its contents ([below](#an-older-home)).

## Secrets, and moving or copying a home

- **Secrets in the home.** `machine/<name>/worker.json` holds a worker's
  secret and `sandbox/<name>/control.json` a Sandbox daemon's keys (both
  0600). A started native Sandbox's run folder holds its daemon's setup
  (`config/daemon.setup`, 0600), with the daemon's private key and, for a
  PostgreSQL Store, the URL with its password. A PostgreSQL password or a
  bucket key is otherwise kept only in the secret folder, `secret/`
  ([secrets](machines-and-stores.md#secrets)). An
  integration keeps its own secrets under `secret/`. A copy of a home
  carries all of these.
- **Moving.** No call moves a home, and nothing rewrites a path in a
  record. A started native Sandbox's record names its run folder by
  absolute path, and the run folder names its record's folder: after a
  move every call on it is refused `InvalidInput`, and it is neither used
  nor deleted. Close every native Sandbox first and move the home only
  while nothing in it runs; then name the new path (`LAYERFS_HOME`).
  Machines, and the Stores and Sandboxes on them, are named by `vm_name`
  and a loopback address, not by a path in the home, and are unchanged.
  Files a record names outside the home (a machine's `image_archive`, a
  `file:` secret reference) are not moved with it.
- **Copying.** A copy keeps the original's id and its records, so both
  homes use the same `layerfs-<id16>-` names: the same VMs, the same
  `docker` worker container and volume (a start from one replaces the
  container the other started), and the same Sandbox records and run
  folders on a machine. The `home.layout` id cross-check cannot tell a
  copy from its original: the id and records agree, so both pass while
  they drive the same provider objects. Do not use a copy beside its
  original.

Not run for this manual; the statements are from the source.

## A new home

The first call on an empty or missing folder creates it with
`layerfs.json`, `layerfs.lock` and the two built-in machines. Run:

```console
$ export LAYERFS_HOME=/tmp/lfs-new
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable",…}
$ cat /tmp/lfs-new/layerfs.json
{"layout":3,"id":"2f51a567b6f153b6e914393cab0904b6"}
```

(Run before layout 3; the answer is the same at layout 3 but for the
layout number.)

## A home that lost its `layerfs.json`

A real home can lose its marker: a copy that left `layerfs.json` behind, a
restore that missed it, or a hand-made one. Its data is still there, so it
is neither a new home nor a foreign folder. A folder with no `layerfs.json`
whose entries are all a home's own — `store/`, `machine/`, `sandbox/`,
`backup/`, `secret/`, `trace/`, `log/`, `incoming/` and `generation` (a
file; each other one a folder) — and at least one of them, is
**`DataWithoutMarker`**. So is one that also holds the `layerfs.lock` a
first call takes, the temporary a marker is renamed from, an integration's
`runtime/`, `job/`, `stat/` and `preset/` folders, and macOS's
`.DS_Store`. This build never writes one by accident: every call on it is
refused `Unsupported`, "holds a LayerFS home's data but no layerfs.json",
before the home lock, with the fix

```console
$ layerfs home init --adopt
```

`home get` answers it `broken` `missing` with the same fix and message, and
the home doctor's `home.layout` fails with it, without a records check.

`--adopt` takes the home lock, detects the folder again under it, writes
`layerfs.json` (layout 3 and an id) and audits the home read only. It never
moves the folder, rewrites a record or the data, or initializes an empty
home there. The id is the longest prefix the home's records agree on — a
machine's `vm_name`, a Sandbox daemon's name, `layerfs-<16 hex>-` or the
earlier eight-hex forms — plus a random tail; with no such record it is a
fresh random id. Records that disagree, or that cannot be read, are
refused `Failed` and no marker is written (the home lock may be). The
answer also says `id`,
`id_source` (`records` or `fresh`) and read only `warnings` (a missing part,
an unreadable or missing `*/record.json`, a part not `0700`, a temporary a
killed init left, an `incoming/init-*` copy).

The records keep only a prefix of the home id, so an adopted id keeps that
prefix and gets a random tail: an older backup records the full id it was
exported from (`layerfs-exported-from`), and a restore check that compares
the whole id will not match the adopted one. A record that stored the full
home id would make a later adopt exact; this build does not write one yet.

## An older home

This build converts no older home (owner, 2026-11-19: "cut and cleanup old
incompatible aggressively, no lfs, and fallback to old 2.0 or 1.x", and
"just drop it. we will never need it"). A folder that is not a LayerFS 2.0
home and is not a home that lost its marker is never written: one whose
`layerfs.json` names another layout, or one holding anything else with no
`layerfs.json` (a stray file, a data name that is a file). Every call on it
is refused `Unsupported`, `home get` answers it `broken` `outdated`, and
the home doctor's `home.layout` check fails, each with the fix

```console
$ mv <home> <home>.old && layerfs home init
```

which keeps the old folder and makes a new, empty home. Nothing in the old
folder is carried over. If an adopted home's data is not wanted after all,
that `mv` is the line too: the adopt only writes a marker, so moving the
folder aside is how to start over without it.

A machine's data disk (`/layerfs/data`) is not an older home and is not
moved. This build recognises it by its contents before it would call it
empty, a home's data or foreign: at least one of `store/`, `sandbox/` and
`machine/native/run/`; `incoming/init-<16 hex>/` or `log/worker.log`; and
none of `trace/`, `backup/` or `layerfs.json`. Every call on it is refused
`Unsupported`, "a machine's worker root, not a LayerFS home", with the fix
`unset LAYERFS_HOME` (never `mv` or `layerfs home init`); `home get`
answers it `broken` `unsupported`, and the home doctor's `home.layout`
fails without a records check. The refusal comes before the home lock, so
nothing — not `layerfs.json`, not `layerfs.lock`, not the call log — is
written there. A real home that lost its `layerfs.json` keeps `trace/` or
`backup/`, which a worker's root never does, so it is `DataWithoutMarker`,
not a worker's root.
