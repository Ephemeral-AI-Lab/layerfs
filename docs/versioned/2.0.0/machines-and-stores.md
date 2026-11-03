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

| Kind | How many | Made by | Sandboxes run as | Tool |
| --- | --- | --- | --- | --- |
| `native` | one, built in | — | macOS: host processes with a loopback NFS mount. Linux: namespaces (needs root). Windows: none; `native` holds only records (O5) | — |
| `docker` | one, built in | — | Docker containers on the image `machine update docker --image` records; see Routes | `docker` |
| `apple-container` | many | `machine create` | namespaces inside the VM | `container` |
| `microsandbox` | many | `machine create` | namespaces inside the VM | `msb` |
| `lima` | many | `machine create` | namespaces inside the VM | `limactl` |
| `firecracker` | many, inside a `lima` host | `machine create --host <lima>` | namespaces inside the VM | `limactl` |
| `ssh` | one per remote host | `machine connect` | namespaces on the host | `ssh` |

- "Platform" is only a machine's kind. A caller never chooses how a
  Sandbox runs; the kind decides.
- `machine list` answers every machine, each with `supports` (`sqlite`,
  `postgresql_sandbox` and the `reason` when not), its platform's row of the
  support table. `machine platforms` answers each platform on this host:
  backend, kernel route, where Store calls run, whether a PostgreSQL Sandbox
  runs here and why not, `ready`, `missing` (such as `tool:msb`), the tool's
  `version` and the options `machine create` takes; the same rows as
  `connect/SUPPORT.json`. Neither starts a machine. A VM or ssh machine's `state` is read live, as
  `machine status` reads it, for all of them at once within 10 s; it starts
  nothing and writes nothing. A machine whose probe fails or does not answer
  in time is `unknown`, with the reason in `error`; one being created or
  removed keeps its recorded state. `machine get` answers the same state.
- `native` and `docker` are never created, removed or disconnected.

### Creating a VM machine

```bash
# not run
layerfs machine create dev lima --cpus 4 --memory 8G --disk 40G
layerfs machine create host1 lima --nested
layerfs machine create fc1 firecracker --host host1
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
  `--image` (and `machine update native|docker --image`) chooses another.
  The image is not published yet; a build places it with the bundle.
- `nested` is for `lima` only: a VM able to host Firecracker machines.
  `firecracker` requires `host`, a `lima` machine made with `--nested`. The
  `lima` machine is `InUse` while it hosts any.
- A create checks the kind's tools first; a missing tool is `Unsupported`
  with `detail.missing`.
- A create needs the sealed Linux bundle (O16): the guest worker, the
  daemon, the Sandbox helper and the Sandbox image, in
  `<cache>/layerfs/bundle/<version>/` with `bundle.json` holding its hash.
  `<cache>` is `XDG_CACHE_HOME`, else `~/.cache`. Without it the create is
  `Unsupported` with `detail.missing` = `bundle:<folder>`. Fetching the
  bundle is not built; a build places it.
- A create that fails before the VM exists removes what it made. Once the
  VM exists, a failure keeps it with state `failed` and its `error`.
  `Unknown` keeps everything and is logged.
- The home lock is held only to claim the name and to record the outcome,
  never while the VM boots. A create also holds the machine's `start.lock`
  (taken before the home lock), so an on-demand start cannot overwrite the
  new machine's secret.
- A `microsandbox` create refuses `Exists` when a `<vm>-data` volume of its
  name is already there (as `apple-container` does), so its cleanup after a
  failure never deletes a volume it did not make.

### Connecting an ssh host

```bash
# not run
layerfs machine connect ec2 10.0.0.5 --identity ~/.ssh/id_ed25519
```

- `user` defaults to `root`, `port` to 22.
- The host's key must already be known. `known_hosts` defaults to
  `~/.ssh/known_hosts` and is copied to `machine/<name>/known_hosts`; it is
  checked strictly.
- `root` defaults to `/layerfs/data`. Its filesystem may be ext4, xfs or
  btrfs (O18).
- A second machine on the same host is `Exists`.
- `disconnect` forgets the host. Its data root and the Stores in it stay.

### Removing and updating

- `machine remove` (VMs) and `machine disconnect` (ssh) are `InUse` while
  the machine holds Stores, Sandboxes or hosted Firecracker machines. Each
  is `InvalidInput` on the other kind.
- A remove takes the machine's `start.lock` first, then, under the home
  lock, marks the record `removing` with its process id. The VM is deleted
  outside the home lock. While that process lives, every call that would
  start the machine, move a Store to it, create a Sandbox on it, update or
  remove it is `InUse`. A refused delete puts the record back as it was;
  one whose outcome is unknown records `state: "unknown"` and the error. A
  remover that is gone holds nothing.
- `machine update native` and `machine update docker` set `image` and
  `docker` (the Docker socket); `wsl` is a Windows host's setting. Any
  update is `InUse` while Sandboxes run on the machine, naming them.
- Resizing a VM with `machine update` is `Unsupported` ("not built yet");
  nothing is changed.

### Start on demand and idle stop

- A machine starts on the first call that needs it. The call dials the
  worker `worker.json` records, once, with no ping first (#535 R1). Only
  when nothing was sent (the connection, the secret or its answer failed)
  does the caller start the machine and its worker, wait until it is
  ready, record `worker.json` and send the call once. That wait is a
  readiness wait, not a retry: the first attempt never reached a worker. A
  call whose answer is lost after it was sent is `Unknown`; it starts
  nothing and is not resent. `store move` and `store remove` make their
  machines ready before they mark the Store.
- `machine start <name>` starts a stopped machine and its worker and waits
  until it is ready; a running machine answers at once. `machine stop
  <name>` stops the worker and, where there is one, the VM (`docker`: its
  worker container; `ssh`: the worker and its forward); disks and Stores
  stay. It is `InUse` while Sandboxes run on the machine (`detail.related`
  names them). `native` answers both at once (#535 P1).
- No list, get or status call starts a machine (#535 P4): every `*.list`,
  `*.get` and `*.status` call and `machine platforms` answer from records
  and live probes, and a stopped machine reads as `stopped` (`store status`:
  `down`; `sandbox get`: no Workspaces and `down_reason`; `workspace
  status`: `Unavailable`). Calls that need the worker start it on demand:
  Store calls on a sqlite Store there, `project remove`, `sandbox create`,
  the Workspace calls that change something, `store move`, `store remove`
  and `backup create`.
- Two processes that need the same stopped machine take turns on its
  `start.lock`, which `machine create` and `machine remove` also take. The
  second waits up to 600 s, then is refused `Unavailable`. The lock is
  always taken before the home lock, and the start re-reads the record once
  it holds it.
- A `firecracker` machine's first call starts its `lima` host the same way
  (its `start.lock`, readiness wait and `worker.json`) before the microVM.
- The worker stops after 10 minutes (O2) with no open Store, no Sandbox, no
  call in flight and no call. It counts the Sandboxes it runs and every one
  recorded under `/layerfs/data/store/<s>/project/<p>/sandboxes/`, and on a
  `lima` host each live Firecracker microVM; an unreadable entry counts as
  in use. In a VM guest it then powers the guest off; a Firecracker microVM
  resets instead (`sync; reboot -f`), since its VMM ignores a power-off. On
  an ssh host only the worker stops.
- `machine status` answers `state`, `free_disk`, `tool_version` and
  `down_reason` (O34). A stopped VM answers without being started. There is
  no change event; callers poll.

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
- A worker holds a Store's lease for as long as it keeps the Store open,
  and closes its own open copy before a move, retire or remove reads or
  deletes the file.
- One process can use Stores on several machines at once.
- A Sandbox on `docker` is a container on the docker machine's image. On
  macOS and Linux the docker worker (a container on that image) starts it
  beside itself, mounting only its Store's folder of the worker's data
  volume. A PostgreSQL Store's Sandbox on `native` or `docker` is started by
  the caller.
- Every machine other than `native` is reached through its worker, `docker`
  on Linux included (its sqlite Stores live in a Docker volume). Checked in
  the terminal pass on a Linux VM.

## Stores

A sqlite Store is one file on one machine at a time. A postgresql Store is
one database. There is no conversion between the two kinds (D2). Every
sqlite Store is Disposable (WAL, `synchronous=OFF`); its record says so.

### sqlite Stores

```console
$ layerfs store create shop          # run
$ layerfs store move shop dev --progress   # not run
```

- `store create` makes an empty sqlite Store on `native`, at
  `store/<name>/store.sqlite`. Its record has `name`, `kind`, `machine`,
  `profile`, `created` and `sqlite` (the SQLite version); `store get` and
  `store list` add `projects` and never show the Store's keys.
- `store move <name> <machine>` moves the file to another machine. It runs
  its phases once each: check, snapshot, send, verify, install, record and
  retire. The bytes pass through the calling process in pieces, so a move
  between two machines other than `native` is one call (O12).
  - It is `InUse` while any process holds the Store, a Sandbox of one of
    its projects runs, or another live process moves it.
  - A Store call made while a move or remove runs is `InUse`, from the
    mover's own process too: the lease is taken, and the record re-read,
    under the home lock. The mover writes `in_progress` under the lock,
    closes its own open copy, then re-checks the leases (another thread's
    included) and Sandboxes under the lock again before the snapshot.
  - The home lock covers only the checks and record writes, never a machine
    start, a Store step or the close of an open Store. A move whose record
    phase fails (its lock wait expired, say) keeps `in_progress` with both
    copies and logs the uncertain outcome.
  - It is `Exists` when the target already holds a Store file of that name,
    and `InvalidInput` for the Store's own machine or a postgresql Store.
  - A failure names its phase in the message and in `detail.phase`, and
    keeps whatever exists.
  - An `Unknown` step keeps both copies, leaves the record's `in_progress`
    set and is logged to `log/uncertain.jsonl`.
  - While it runs, `store status` shows `state: "moving"` and
    `in_progress`.
- `store remove` deletes a sqlite Store wherever it is, with every project
  in it. Its backups stay in their buckets. It is `InUse` while the Store is
  held. A Store whose machine is unknown is `Unavailable` (as for a move).
  A stopped machine is started (or the call refused) before the home lock is
  taken; a Store whose machine changed meanwhile is `Conflict`. A certain
  failure clears `in_progress`.
- `store status` starts no machine. A Store whose machine is not running,
  or whose recorded worker does not answer, has `state: "down"` with
  `down_reason`, and `file.unavailable` (O34).
- A lease records its process's start time and boot, so a lease or move
  whose process id was reused by another process is stale. A lease written
  without them is matched by id alone.
- Init writes only into a sqlite Store on `native`. To add a project to a
  Store on another machine, move it to `native`, Init, and move it back.
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
- `store disconnect` forgets the Store; its data stays on the server. It is
  `InUse` while the Store is held.
- `project init` into a PostgreSQL Store runs in the caller's process.
- PostgreSQL needs a build with the `postgresql` feature; without it these
  calls are `Unsupported`.
- A PostgreSQL Store's Sandboxes run on `native` (macOS, a daemon built
  with `postgresql`) or `docker`, started from the caller's process, which
  resolves the password and hands the URL to the daemon. Under Docker
  Desktop a loopback URL reaches the container's daemon as
  `host.docker.internal`. On a VM or ssh host they come later (O3).
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
  `InUse`. A Store on another machine is `Unsupported` (move it to
  `native` first). The home's project records are rebuilt from the Store.
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
