# Machines, Stores and backups

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources:
`engine/crates/layerfs-api/src/machine/`, `src/family/` and `src/store/`,
and `engine/crates/layerfs-sandbox/src/machine/` on branch `issue533-v2`,
with the owner decisions in
[DECISIONS.md](../../../core/docs/issues/533/DECISIONS.md).

Storage is separate from compute (D1). A **machine** is one kernel where
Sandboxes run. A **Store** holds projects and their history. A **backup** is
a copy of a sqlite Store in a bucket.

## Machines

Firecracker, Microsandbox and SSH machines were removed in 2.0; see
[removed platforms](#removed-platforms-firecracker-microsandbox-and-ssh) and
the [contract](../../architecture/control/support-matrix/disabled-platforms.md).

| Kind | How many | Made by | Sandboxes run as | Tool |
| --- | --- | --- | --- | --- |
| `native` | one, built in | — | macOS: host processes with a loopback NFS mount. Linux: namespaces (needs root). Windows: none; `native` holds only records (O5) | — |
| `docker` | one, built in | — | namespaces inside its worker container (D-8), rooted in the bundle's Sandbox image. The image `machine update docker --image` records, else the bundle's Sandbox image loaded into Docker once (#535 P5), is the worker container's; see Routes | `docker` |
| `apple-container` | many | `machine create` | namespaces inside the VM | `container` |
| `lima` | many | `machine create` | namespaces inside the VM | `limactl` |

- "Platform" is only a machine's kind. A caller never chooses how a
  Sandbox runs; the kind decides.
- `machine list` answers every machine, each with `supports` (`sqlite`,
  `postgresql_sandbox` and the `reason` when not), its platform's row of the
  support table, and its live `state` as `machine status` reads it
  (`native` is always `ready`; the built-in `docker` shows `stopped` until
  `machine start docker`). `machine platforms` answers each platform on this host:
  backend, kernel route, where Store calls run, whether a PostgreSQL Sandbox
  runs here and why not, `ready`, `missing` (such as `tool:limactl`), the tool's
  `version` and the options `machine create` takes; the same rows as
  `control/SUPPORT.json`. Neither starts a machine. A VM machine's `state` is read live, as
  `machine status` reads it, for all of them at once within 10 s; it starts
  nothing and writes nothing. A machine whose probe fails or does not answer
  in time is `unknown`, with the reason in `error`. A machine that an
  operation holds (its record's `op`, below) is not probed: it is
  `creating` or `removing` while that process lives, and `unresolved` once
  it is gone with the outcome unknown. `machine get` answers the same
  state.
- `native` and `docker` are never created or removed.

### Removed platforms: Firecracker, Microsandbox and SSH

Firecracker, Microsandbox and SSH machines were removed in 2.0, and with
them Lima's `nested` option (it only hosted Firecracker). `machine create` and
`sandbox.create`'s `platform` refuse them `Unsupported`; there is no
`machine connect` or `machine disconnect`. A home may still hold a record of
one:

- `machine list` and `machine get` answer it: `platform` as recorded,
  `backend: "-"`, `supports` false for both with the reason, `state:
  "unsupported"` and `error` the reason (``platform `ssh` was removed in
  LayerFS 2.0``, or the same for `firecracker` or `microsandbox`), plus its
  recorded `ssh` settings, `host` or `vm_name`.
- Every other call on it, its Sandboxes or a sqlite Store on it is
  `Unsupported` with that reason.
- `machine remove <name>` only forgets the record (`InUse` while Stores or
  Sandboxes are recorded on it, as for any remove). Its `note` says nothing
  on the host was touched. For microsandbox, remove the VM and its volume
  yourself: `msb remove --force <vm>` and `msb volume remove <vm>-data`.
  For firecracker, the note names the microVM's folder
  `/layerfs/data/firecracker/vms/<vm>` and data disk
  `/layerfs/data/firecracker/disks/<vm>.ext4`, left in its Lima host;
  removing that Lima machine removes them with its data disk.

### Creating a VM machine

```bash
# not run
layerfs machine create dev lima --cpus 4 --memory 8G --disk 40G
```

- Defaults: 2 CPUs, 4 GiB memory, 20 GiB disk. Sizes are bytes, or text
  with `K`, `M` or `G`.
- Each VM machine has one VM named `lfs-<first 8 digits of the home
  id>-<name>`, one data disk mounted at `/layerfs/data` that holds many
  Stores, and one worker.
- `image` is the image its Sandboxes run in. The default Sandbox image (owner
  decision O36) is LayerFS's runtime image
  `layerfs-352-runtime:node24-https-apt-20261024` (config digest
  `sha256:afa1ef9c…f38e`: Node 24, Rust 1.85.1, the SQLite headers,
  Chromium and Selenium), sealed into the bundle as `sandbox-image.tar`.
  `--image` (and `machine update native --image`) chooses another. On
  `docker` the recorded `image` is only its worker container's image; its
  Sandboxes' root is the bundle's `sandbox-image.tar` (D-8).
  The image is not published yet; a build places it with the bundle.
- A create checks the kind's tools first; a missing tool is `Unsupported`
  with `detail.missing`.
- A create needs the sealed Linux bundle (O16): the guest worker, the
  daemon, the Sandbox helper and the Sandbox image, in
  `<cache>/layerfs/bundle/<version>/` with `bundle.json` holding its hash.
  `<cache>` is `XDG_CACHE_HOME`, else `~/.cache`. Without it the create is
  `Unsupported` with `detail.missing` = `bundle:<folder>`. Fetching the
  bundle is not built; a build places it.
- From the claim until the outcome is recorded, the record holds the
  create's `op` (`machine.create`): its id,
  phase and process (process id, start time and boot). While that process
  lives, every other call on the machine is `Busy`.
- A create that fails before the VM exists removes what it made. Once the
  VM exists, a failure keeps it with state `failed` and its `error`.
  `Unknown` keeps everything, its `op` marked `uncertain` with the
  `error`, and is logged to `log/uncertain.jsonl`. Once that process is
  gone, every call on the machine but `machine status`, `machine get`,
  `machine list` and the remove is `Unresolved`, naming
  `layerfs machine status <name>` and `layerfs machine remove <name>`.
- The home lock is held only to claim the name and to record the outcome,
  never while the VM boots. A create also holds the machine's `start.lock`
  (taken before the home lock), so an on-demand start cannot overwrite the
  new machine's secret.
- An `apple-container` create refuses `Exists` when a `<vm>-data` volume of
  its name is already there, so its cleanup after a failure never deletes a
  volume it did not make.

### Removing and updating

- `machine remove` is `InUse` while the machine holds Stores or Sandboxes;
  `detail.related` names each as `{kind: "store" | "sandbox", name}`.
- A remove takes the machine's `start.lock` first, then, under the home
  lock, writes its `op` (`machine.remove`) into the record. The VM is deleted outside the home lock. While that
  process lives, every call that would start the machine, push a Store to
  it, create a Sandbox on it, update or remove it is `Busy`; `machine get`
  and `machine status` answer `removing`. A refused delete puts the record
  back as it was; one whose outcome is unknown keeps the `op` marked
  `uncertain` with the `error`, and is logged, as for a create. A remover
  that is gone after a known outcome holds nothing.
- A remove is refused only while another process's `op`
  lives: it is the call that clears an `unresolved` machine.
- `machine update native` and `machine update docker` set `image`,
  `docker` (the Docker socket) and `daemon` (the daemon their Sandboxes
  run: an absolute path to a file, replacing what `import-env` recorded;
  `machine get` shows it); `wsl` is a Windows host's setting. Any
  update is `InUse` while Sandboxes run on the machine, naming them.
- A recorded daemon that is not there (its build moved or was removed)
  refuses a Sandbox on that machine `Unavailable` before any launch,
  naming `machine update <name> --daemon <path>`; `layerfs doctor` reports
  it as an error.
- Resizing a VM with `machine update` is `Unsupported` ("not built yet");
  nothing is changed.

### Starting and stopping (manual, #535 D11)

- A machine runs only between `machine start <name>` and `machine stop
  <name>`. Nothing starts or stops one by itself: no call starts a stopped
  machine, and a running machine, its worker and its Sandboxes stay up
  however long they are unused. A Store is still closed after a quiet
  minute (the worker's and the caller's Store connection reaper); that
  closes a file, not a machine. Before that, when the last call on a
  sqlite Store returns or a Sandbox on it closes, its write-ahead log is
  checkpointed into the file and truncated (#86): `store status` right
  after the last Sandbox closes shows `wal_bytes` 0, unless another process
  was still writing or reading it then.
- A call that needs a stopped machine is refused `Unavailable`, naming the
  start command: "machine <m> is not running; start it with `layerfs
  machine start <m>`". It dials the worker `worker.json` records once, with
  no ping first (#535 R1); when that dial sent nothing (the connection, the
  secret or its answer failed), the call is refused the same way ("machine
  <m>'s worker did not answer (…); start it with `layerfs machine start
  <m>`") and nothing is started or resent. A call whose answer is lost
  after it was sent is `Unknown`; it is not resent.
- The calls that need the machine's worker: Store calls on a sqlite Store
  there, `project remove`, `sandbox create`, the Workspace calls that change
  something, `store push`, `store pull`, `store remove` and `backup create`.
  `store push`, `store pull` and `store remove` check their machines before
  they mark the Store.
- `machine start <name>` starts a stopped machine and its worker, waits
  until it is ready and records `worker.json`; a running machine answers at
  once. `machine stop <name>` stops the worker and, where there is one, the
  VM (`docker`: its worker container);
  disks and Stores stay. It is `InUse` while Sandboxes run on the machine
  (`detail.related` names them). `native` answers both at once (#535 P1).
- No list, get or status call starts a machine (#535 P4): every `*.list`,
  `*.get` and `*.status` call and `machine platforms` answer from records
  and live probes, and a stopped machine reads as `stopped` (`store status`:
  `down`; `sandbox get`: no Workspaces and `down_reason`; `workspace
  status`: `Unavailable`).
- Two `machine start` calls on the same machine take turns on its
  `start.lock`, which `machine create`, `machine stop` and `machine remove`
  also take. The second waits up to 600 s, then is refused `Unavailable`.
  The lock is always taken before the home lock, and the start re-reads the
  record once it holds it.
- `machine status` answers `state`, `free_disk`, `tool_version` and
  `down_reason` (O34). A stopped VM answers without being started. While a
  record holds an `op`, it also answers `op` (`id`, `name`, `phase`,
  `started`, `uncertain`), probes nothing and answers the state above; it
  never clears the `op`. There is no change event; callers poll.

### Routes

- Calls on a sqlite Store on `native`, and calls on a PostgreSQL Store, run
  in the caller's process.
- Calls on a sqlite Store on any other machine go to that machine's worker,
  behind a secret, over a forwarded loopback port. The caller sends the
  Store's record and the home id with the call; the worker never reads the
  home, and its own root (`/layerfs/data`) is never treated as a home.
- A connection starts with a small frame holding only the secret (at most
  4 KiB, read within 10 s); the worker answers it, or refuses `Denied` and
  closes, before it reads the request. A caller that gets no answer in 10 s
  is `Unavailable`; in both cases the request was not sent. Only a
  connection lost after the request was sent is `Unknown`. One attempt;
  nothing is resent.
- The worker's answer to the secret (its hello) says which build it is:
  `{ ready: true, protocol, bundle }`, the worker protocol it speaks (2)
  and the hash of the sealed bundle it runs from (`bundle.json` beside its
  `layerfs`). The caller compares both with its own protocol and the bundle
  it deploys before it sends anything. A worker of another protocol or
  bundle, or an older worker whose hello names no protocol, is refused
  `Unavailable` (`detail.worker` and `detail.expected` give both sides),
  naming the fix: "replace its worker with this build's: `layerfs machine
  stop <m>`, then `layerfs machine start <m>`" (a start copies this build's
  bundle in). `machine start` alone does not replace a running worker,
  whatever its build. A worker run from no sealed bundle (a development
  build) is checked by protocol alone. [`layerfs doctor`](calls.md#layerfs-doctor)
  reports a running worker of another build as a warning.
- A worker holds a Store's lease for as long as it keeps the Store open,
  and closes its own open copy before a push, pull, retire or remove reads or
  deletes the file.
- One process can use Stores on several machines at once.
- A Sandbox on `docker` runs the `namespaces` backend inside the docker
  machine's worker container, as a Sandbox in a VM does (D-8,
  layerfs-project#111). The worker container is privileged, in a cgroup
  namespace of its own, and mounts only its data volume and the bundle
  (never the Docker Engine socket). A Sandbox's root is the bundle's
  `sandbox-image.tar`; `--image sha256:<64 hex>` names a digest that
  archive must hold. Its record keeps `platform: "docker"` and shows
  `backend: "namespaces"`. A PostgreSQL Store's Sandbox starts the same way
  as a sqlite Store's on that machine; it binds no Store file. A Sandbox
  recorded under the retired docker backend (one container per Sandbox) is
  refused `Unsupported` with a message to recreate it; `sandbox close`
  forgets it and names its container for `docker rm`.
- Every machine other than `native` is reached through its worker, `docker`
  on Linux included (its sqlite Stores live in a Docker volume). Checked in
  the terminal pass on a Linux VM.
- Workspace calls skip the worker where the caller reaches the Sandbox's
  daemon (#535 G4). On `lima`, `sandbox.create` (through the
  worker) answers the daemon's port and keys, and the caller forwards the
  port to its own loopback (`ssh -L`). The
  keys go to the home's `sandbox/<name>/control.json` (0600), the forward
  to its record. `workspace.mount`, `commit`, `unmount`, `status` and
  `sandbox.get`'s Workspace list then go from the caller straight to the
  daemon; the Workspace records are in the home's `sandbox/<name>/`. A
  Branch lookup (mount) or head read (status) on a sqlite Store still goes
  to the worker as a Store call. `workspace.exec` still goes through the
  worker: the command runs through the Sandbox's runtime in the machine,
  and the caller sends the directory its record names. `sandbox.create`
  and `sandbox.close` go through the worker; a close ends the forward.
  `docker` (its worker container's ports are published only when it
  starts) and `apple-container` forward nothing:
  every call goes through the worker. A forward that failed at create
  leaves the Sandbox on the worker route (`forward_failed` in the answer).
  On a stopped machine these calls are `Unavailable`, naming `machine
  start`; a forward that has ended is `Unavailable` too, naming `machine
  start` and `sandbox close`.

## Stores

A sqlite Store is one file on one machine at a time. A postgresql Store is
one database. There is no conversion between the two kinds (D2). Every
sqlite Store is Disposable (WAL, `synchronous=OFF`); its record says so.

### sqlite Stores

```console
$ layerfs store create shop          # run
$ layerfs store push shop dev --progress   # run (2026-11-08: docker, lima, apple-container)
$ layerfs store pull shop --progress       # run (2026-11-08: docker, lima, apple-container)
```

- `store create` makes an empty sqlite Store on `native`, at
  `store/<name>/store.sqlite`. Its record has `name`, `kind`, `machine`,
  `profile`, `created` and `sqlite` (the SQLite version); `store get` and
  `store list` add `projects` and never show the Store's keys; each
  `store list` item also ends with the `state` `store status` shows.
- `store push <name> <machine>` sends the file from `native` to a machine;
  `store pull <name>` brings it from its machine back to `native` (#536
  H1). Both run the same phases once each: check, snapshot, send, verify,
  install, record and retire; only the sides swap. The bytes pass through
  the calling process in pieces. Between two machines, pull, then push:
  each half is its own call. Retire deletes the source's copy: on `native`
  the file with its `-wal`, `-shm` and any partial copy (the folder keeps
  the home's records); on another machine the Store's whole folder under
  its worker's root, with the project entries the worker kept there (a
  symlink in it is removed, never followed).
  - A push is `InvalidInput` unless the Store is on `native` (the message
    names `store pull`), and for `native` as the target; a pull is
    `InvalidInput` when the Store is already on `native`. Both are
    `InvalidInput` for a postgresql Store; a push to an unknown machine is
    `NotFound`. A stopped machine is `Unavailable`, naming `layerfs machine
    start <m>` (#535 D11).
  - Its check runs before it marks the Store: both machines answer, the
    target holds no Store file of that name (`Exists`), and the target has
    the Store's bytes (with its write-ahead log) plus a headroom of 1 GiB,
    or a tenth of the Store when that is more, free. Too little is
    `Capacity`, naming the bytes needed and free (`detail.needed`,
    `detail.free`). A push measures the machine's free disk in its worker,
    a pull on this host.
  - It is `InUse` while any other process holds the Store or a Sandbox of
    one of its projects is recorded; `detail.related` names each as
    `{kind: "lease", name: <pid>}` or `{kind: "sandbox", name}`.
  - While a push, pull or remove runs, the Store's record holds one `op`
    (#536 §7.1): its id, `name` (`store.push`, `store.pull`,
    `store.remove`), `phase`, `args` (`from`, `to`), owner process and
    `uncertain`. Every other operation, lease and
    Store call on the Store is `Busy` while that process lives, from the
    caller's own process too: the lease is taken, and the record re-read,
    under the Store's lock, `store/<name>/lock`. The caller writes `op`
    under the home lock and the Store's lock, closes its own open copy,
    then re-checks the leases (another thread's included) and Sandboxes
    under both locks again before the snapshot, and writes each phase into
    `op` as it starts.
  - The locks cover only the checks and record writes, never a machine
    start, a Store step or the close of an open Store.
  - A certain failure drops `op`. An uncertain one (an `Unknown` step, or
    any failure of the record phase, after both copies exist) keeps both
    copies, keeps `op` with `uncertain: true` and is logged to
    `log/uncertain.jsonl`. The install phase is written uncertain before
    it starts, so a call that dies during install or record (the target
    may hold a copy the record does not name) is uncertain too. The record
    write that puts the Store on its new machine records `last_transfer`
    (`call`, `from`, `to`, `at`, `bytes`, `sha256`, `heads`, `retired:
    false`) and keeps `op`, certain, at phase retire while the source's
    copy is deleted; then `op` goes and `retired` becomes `true`. A retire
    that fails or whose process ends leaves the Store in use on its new
    machine and the old copy, which the doctor reports and the next push
    or pull deletes first.
  - Once that process is gone, an uncertain `op` makes every later
    operation, lease and Store call on the Store `Unresolved`, naming
    `layerfs store status <name>` and the call that clears it: a new
    `layerfs store push <name> <machine>` (with the machine the push named,
    never the opposite direction or another machine)
    or `layerfs store pull <name>`. A new push or pull first deletes the
    copy left on whichever of the two machines the record does not name (on
    `native` its file and any partial copy; on another machine the Store's
    folder under its worker's root), then runs as usual. `store remove`
    clears it too, deleting both the copy on the record's machine and any
    copy or partial `.incoming` copy on the other machine (`incoming_on`). A
    certain `op` holds nothing; a certain failure at `send`, `verify` or
    `install` records `incoming_on: <machine>` so `layerfs doctor` reports
    the partial `.incoming` bytes and the next `store push` or `store remove`
    cleans it up.
  - During `snapshot`, an idle Store whose crashed opener left a non-empty
    `-wal` is automatically checkpointed (`seal`) under the transfer's
    exclusive lease before hashing. Every step runs once: a lost worker
    answer (`Unknown`), a read's included, fails the transfer uncertain and
    nothing is resent.
  - A failure names its phase in the message and in `detail.phase`, and
    keeps whatever exists.
  - While it runs, `store status` shows `state: "busy"` and `op`.
- `store remove` deletes a sqlite Store wherever it is, with every project
  in it. Its backups stay in their buckets. It is `InUse` while the Store is
  held (`detail.related` as for a push) and `Busy` while a live process runs an operation on it. A Store
  whose machine is unknown is `Unavailable` (as for a push or pull). A stopped
  machine is refused `Unavailable`, naming `machine start`, before the
  home lock is taken; a Store whose machine changed meanwhile is
  `Conflict`. It is the call that clears an unresolved `op`: a gone
  process's intent does not refuse it. Its remove phase is written
  uncertain before it starts. A certain failure drops its `op`, putting
  back the unresolved one it would have cleared.
- `store status` starts no machine. A Store whose machine is not running,
  or whose recorded worker does not answer, has `state: "down"` with
  `down_reason`, and `file.unavailable` (O34).
- A Store's `state` is `idle`, `in_use` (another live process holds its
  lease), `busy` (a live `op`), `down` or `unresolved` (a gone process's
  uncertain `op`). `store status`, `store get` and each `store list` item
  show `op` as `{id, name, phase, started, uncertain}`.
- A lease or `op` records its process's start time and boot, so one whose
  process id was reused by another process is stale. A lease written
  without them is matched by id alone.
- A process that ends with Stores open leaves their leases. Taking a
  lease removes the Store's stale ones first, under the Store's lock, and
  logs each (`log/leases.jsonl`), as a push, pull, remove or disconnect
  does; reads leave them. A process that is about to exit calls
  `layerfs.close()` (Node and Python, [SDK](sdk.md)) to close its open
  Stores and drop its leases.
- Init into a sqlite Store on another machine reads the folder on the
  caller's host and copies it, in pieces of at most 8 MiB, into
  `incoming/init-<id>` under that machine's worker root; the worker runs
  Init on the copy under the Store's lease there and deletes the copy
  when Init ends (#536 H11, H11a). The machine needs the copy's bytes plus
  `max(1 GiB, 10%)` free first. The caller holds its lease meanwhile and
  records the project when the worker answers. A failure while copying
  deletes the copy; a lost answer is `Unknown`, names the copy that may
  remain, and records nothing in the home. Not run on a real VM.
- A Sandbox of a sqlite project runs only on the Store's machine.

### PostgreSQL Stores

```bash
# not run
export LAYERFS_PG_PASSWORD=...
layerfs store connect pg-main "postgresql://lfs@db.example:5432/layerfs" --password env:LAYERFS_PG_PASSWORD
layerfs project init pg-main/load ./load
```

- The URL starts `postgresql://` or `postgres://` and names its user. A URL
  that holds a password is `InvalidInput`, before any effect: in its user
  part, or as any query parameter whose name contains `password`
  (`password`, `sslpassword`, any case). Give the password as a reference
  with `--password` (`env:NAME` or `file:/absolute/path`). It is resolved on
  each call and never written to the home.
- `synchronous_commit` is `off` (the default), `local`, `remote_write`,
  `on` or `remote_apply`.
- The Store lives in the fixed schema `layerfs`. An empty database gets the
  schema with a new identity. A database that already holds the `layerfs`
  schema keeps its identity, and its projects become this home's project
  records. A Store name already in the home is `Exists`, and so is the same
  database under a second name: checked before any request by the URL's
  server and database (as libpq reads them; port 5432 and the user's
  database by default, the host in lower case), and again under the home
  lock by the schema's binding, which catches another spelling of the same
  host or a copy of the schema. The schema is made or read before the home
  lock, which covers the re-checks and the record writes only; a connect
  refused under the lock after it made the schema leaves that empty schema.
- Not verified: the decisions log (O6) says a database whose one LayerFS
  schema has another name is opened and one with several is refused. The
  connect code names only the schema `layerfs`.
- `store status` adds a `server` field from one connection.
- A process that makes calls on a PostgreSQL Store holds its lease while
  it keeps the Store open, as for a sqlite Store. `store disconnect`
  forgets the Store; its data stays on the server. It is `InUse` while
  another process holds the Store or a Sandbox of it is recorded;
  `detail.related` names each as `{kind: "lease", name: <pid>}` or
  `{kind: "sandbox", name}`, as for a push.
- `project init` into a PostgreSQL Store runs in the caller's process,
  under the Store's lease.
- PostgreSQL needs a build with the `postgresql` feature; without it these
  calls are `Unsupported`.
- A PostgreSQL Store's Sandboxes run on the machines it is attached to.
  `store connect` attaches `native` with the Store's own connection and
  records its check (`machines.native.checked`) when the role may write;
  `store attach <store> <machine>` attaches another machine, or attaches
  one again with a new connection:

  ```bash
  # not run
  layerfs store attach pg-main docker
  layerfs store attach pg-main build-vm --url "postgresql://lfs@10.0.0.5/layerfs" --password file:/run/secrets/pg-vm
  layerfs store detach pg-main docker
  ```

  Without `--url` and `--password` the machine dials the Store's URL and
  password (its loopback host rewritten as below); with either, its own.
  The check runs from the machine (in this process for `native`, else in
  the machine's worker, which must be from a bundle with `store.reach`):
  the server answers, the database holds the `layerfs` schema with the
  Store's binding (another one is `Unavailable` "holds another Store"),
  and the role may use the schema, make temporary tables and read and
  write every LayerFS table and sequence (else `Denied`, naming the first
  privilege missing). A refused check records nothing; a passed one
  records the attachment with `checked: {at, server_version,
  latency_ms}`. The record keeps the URL without a password and the
  password as a reference, as `store connect` does. The platforms are
  `native` (macOS and Linux), `docker`, `lima` and `apple-container`; an
  attach to a machine the support table refuses is `Unsupported` with its
  reason. `store detach` is `InUse` while a Sandbox of the Store is
  recorded on that machine. `sandbox create` on a machine the Store is not
  attached to is `Unsupported`, naming the `store attach` that fixes it;
  `machine remove` is `InUse` while a PostgreSQL Store is attached to the
  machine. `machine list` and `machine get` show a machine's attached
  PostgreSQL Stores as `postgresql`.
- A Sandbox starts where the machine starts Sandboxes (the daemon and the
  worker are built with `postgresql`). The caller resolves the password and
  rewrites a loopback host for the machine (`docker`:
  `host.docker.internal`, given to the worker container on a Linux Engine
  as its `host-gateway`); the URL travels only in that one request.
  `sandbox create` checks again, from the machine, that the server
  answers with the Store's schema binding.
  The server always runs off the machine (on this host or elsewhere);
  nothing installs PostgreSQL in a machine (D12). The VM platforms'
  rewrites, each proven by a live run (#535 `receipts/g7-live/`):
  - `lima`: `192.168.5.2` (the guest's `host.lima.internal`; the address,
    because the Sandbox's own root has no hosts entry for the name), which
    reaches a server on this host's loopback.
  - `apple-container`: the container network's gateway, read from
    `container inspect` (192.168.64.1 by default). The server must listen
    on that bridge (`0.0.0.0`, as Docker Desktop publishes a port), not on
    127.0.0.1 alone, and admit connections from it.
- Not built: `project remove` from a PostgreSQL Store, and backups of
  PostgreSQL Stores (back it up with PostgreSQL's own tools).
- The step 10a and 10b commits record `store connect` and Init into a
  throwaway `postgres:17` container on macOS. Not run for this manual.

## Backups

Backups are for sqlite Stores. Buckets are r2 or MinIO; 1.1.0's
`local_folder` provider is not in 2.0. Each Store has one link per bucket
(O23), so it can be backed up to several buckets.

```bash
# not run
export LAYERFS_BUCKET_LAB_ACCESS_KEY=... LAYERFS_BUCKET_LAB_SECRET_KEY=...
layerfs backup connect lab http://127.0.0.1:9000/layerfs/backups
layerfs backup create shop lab
layerfs backup list lab --text
layerfs backup restore shop lab
```

- **`backup connect <name> <url>`.** The URL is
  `http(s)://<host>[:port]/<bucket>/<prefix>` for MinIO, or
  `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
  `key` is a reference; it defaults to `env:LAYERFS_BUCKET_<NAME>` (the name
  in upper case, `-` and `.` as `_`). `region` defaults to `us-east-1` for
  MinIO and is `auto` for r2. `part_size` defaults to 64 MiB. One listing
  checks the bucket and key before anything is recorded. A name already
  connected is `Exists`.
- **`backup create <store> <bucket> [file]`.** `file` defaults to the
  Store's name. The snapshot is taken on the Store's own machine; its bytes
  pass through the caller's process, so machines never hold bucket keys.
  The object is `<prefix>/<file>/store.sqlite` (the whole file, or a
  manifest) plus pieces `store.sqlite.piece-NNNNNN`, with its sha256,
  heads, binding and `layerfs-exported-from: <home id>/<machine>` as
  metadata. An object there whose `layerfs-exported-from` names another
  home is `Exists`; one that is not the copy this home last moved through
  the Store's link is `Conflict`. A link that records no sha256 of its last
  copy expects the file absent. Both checks run before the snapshot and
  again before the upload; nothing is uploaded on a refusal.
- **`backup restore <store> <bucket> [file]`.** It downloads the file,
  verifies it and installs it as the sqlite Store `store` on `native`: a new
  Store, or one that replaces an idle Store on `native`. Replacing a Store
  that changed since its last backup or restore through that bucket is
  `Conflict`. Replacing one that a process holds open, whose SQLite `-wal`
  or `-shm` file exists, or one of whose projects has a running Sandbox is
  `InUse`. A Store on another machine is `Unsupported` (`store pull` it
  to `native` first). The home's project records are rebuilt from the Store.
  - The download, its verification and the read of its projects run before
    the home lock, into a hidden staging folder under the home's `store/`
    (removed when the call ends). The lock covers the re-checks, the install
    (one rename, or a hard link for a new Store) and the record writes.
  - Of two restores under the same new name, the first to install wins;
    the other is `Exists` and changes nothing.
- **`backup list [bucket]`.** Every Store file on the connected buckets,
  checked live: one item per Store of this home linked to it (a file linked
  by several Stores is listed once per Store), each with a `state`. A linked
  copy compares both sides with the link's last transfer: the Store's heads
  now against the link's heads, and the copy's sha256 from the listing
  against the link's `last.sha256`.

  | State | Meaning |
  | --- | --- |
  | `current` | linked; neither the Store nor the copy changed since |
  | `store_changed` | linked; the Store changed, the copy did not |
  | `bucket_changed` | linked; the copy changed (written by another Store or home), the Store did not |
  | `both_changed` | linked; both changed |
  | `unknown` | linked, but one side cannot be read here; `reason` says why |
  | `unlinked` | no Store of this home links it |
  | `missing` | linked, but gone from the bucket |

  `unknown`'s `reason` names a Store on another machine (heads are read on
  `native` only), a PostgreSQL Store, a link without heads or without a
  sha256, or a copy whose sha256 the listing did not read (it stats a
  bounded number of copies per listing).

- **`backup disconnect <name>`** forgets the bucket and this home's links
  to it. Its objects stay. LayerFS never deletes a bucket object.
- The step 11 commit records the flow against MinIO (create, the four
  states it had then, `Conflict`, restore as a new Store, a Store behind a
  worker). The review fixes add MinIO tests of the seven states
  (`943363f5b0`), a foreign copy and an unrecorded link (`dfe87f9b68`) and
  concurrent restores (`842b4058b9`). R2 was `NOT_RUN` (no credentials).
  Not run for this manual.

## Secret references

| Reference | A bucket key | A PostgreSQL password |
| --- | --- | --- |
| `env:NAME` | reads `NAME_ACCESS_KEY` and `NAME_SECRET_KEY`; `NAME` must start with `LAYERFS_` | reads `NAME` |
| `file:/absolute/path` | reads lines `access_key=…` and `secret_key=…`; refused if anyone but its owner can read the file | reads the file's text |

No other scheme exists; there is no keychain (O1). A reference is resolved
at each call and its value is never written to the home. Anyone who can
read the home can make every call.
