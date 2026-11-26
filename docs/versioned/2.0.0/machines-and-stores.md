# Machines, Stores and backups

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources: [the calls](calls.md), the
control crates `layerfs-machine`, `layerfs-store`, `layerfs-bucket` and
`layerfs-platform` (`control/crates/`) on this branch, and the owner
decisions in
[DECISIONS.md](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/DECISIONS.md).

Storage is separate from compute (D1). A **machine** is one kernel where
Sandboxes run. A **Store** holds projects and their history. A **backup** is
a copy of a sqlite Store in a bucket.

The built-in machine is `macos-host` on macOS and `linux-host` on Linux. Its platform is `host`. macOS Sandboxes use the `process` backend and Linux Sandboxes use `namespaces`. The examples below use a macOS home.

## Machines

Firecracker, Microsandbox and SSH machines were removed in 2.0; see
[removed platforms](#removed-platforms-firecracker-microsandbox-and-ssh) and
the [contract](../../architecture/control/machines/platforms.md#removed-platforms-removed_platformsrs).

| Kind | How many | Made by | Sandboxes run as | Tool |
| --- | --- | --- | --- | --- |
| `host` | `macos-host` or `linux-host`, built in | — | macOS: host processes with a loopback NFS mount (each sqlite Store's directory must be owned by this user and private, mode `0700`). Linux: namespaces (needs root). Windows has only the built-in Docker machine | — |
| `docker` | one, built in | — | namespaces inside its worker container (D-8), rooted in the bundle's Sandbox image or an image copied from the host's Docker (`--image`). The image `machine update docker --image` records, else the bundle's Sandbox image loaded into Docker once (#535 P5), is the worker container's; see Routes | `docker` |
| `apple-container` | many | `machine create` | namespaces inside the VM | `container` |
| `lima` | many | `machine create` | namespaces inside the VM | `limactl` |

- "Platform" is only a machine's kind. A caller never chooses how a
  Sandbox runs; the kind decides.
- `machine list` and `machine get` answer every machine from the home's
  records, starting and probing nothing: its `platform`, its `state` (`ready`,
  `busy` with `op`, `stopped`, or `broken` with `reason` and `fix`; the host
  starts enabled, the built-in `docker` `stopped` until `machine start
  docker`) and, in the long form, `supports` (`sqlite`, `postgresql_sandbox`,
  and `log` when the machine has a worker whose log can be read, never on
  the host machine), its sizes, image, Stores and Sandboxes. `--with live` probes the
  provider and says hello to the worker once (every machine at the same time
  for `list`), within 10 s, starting nothing; a probe that does not answer
  is `unknown` with `observed_at`. `layerfs home get --with platforms`
  answers each platform on this host: backend, kernel route, where Store
  calls run, whether a PostgreSQL Sandbox runs here, `ready`, `missing`
  (such as `tool:limactl`), the tool's `version` and the options `machine
  create` takes; the same rows as `control/SUPPORT.json`. See
  [`machine`](calls.md#machine-14) for every key.
- The host machine and `docker` are built in.
- Every namespaces Sandbox's commands have a network of their own: two
  Sandboxes can both listen on `127.0.0.1:3000`, a Sandbox reaches the
  internet, this computer and the machine's services as before, and it
  cannot reach another Sandbox or the machine's own `127.0.0.1`, nor the
  daemon's or the worker's port at the machine's addresses. LayerFS's
  forwards on this Mac (lima's, Docker Desktop's published worker port)
  stay reachable through `host.lima.internal` or `host.docker.internal`,
  guarded by their secrets, which no command can read (owner to confirm).
  The daemon stays on the machine's network and resolves names with the
  machine's own resolver file, so a PostgreSQL Store URL is dialled as
  before. On a Linux host (`linux-host`,
  `docker` on a Linux Engine) LayerFS adds the firewall table
  `inet layerfs` and one `lfs<id>` link per Sandbox to this host's network,
  never changes `net.ipv4.ip_forward`, and refuses `sandbox.create`
  (`Unavailable`, with the fix) when forwarding is off, when another
  table's forward chain would drop the Sandboxes' traffic (a Docker host's
  `FORWARD` policy, ufw), or when the resolver names no IPv4 server a
  Sandbox reaches (only loopback or IPv6 ones, with no IPv4 upstream). macOS `macos-host` Sandboxes keep this computer's network
  ([Backends](../../architecture/control/sandboxes/backends.md#every-namespaces-sandboxs-own-network)).

### Removed platforms: Firecracker, Microsandbox and SSH

Firecracker, Microsandbox and SSH machines were removed in 2.0, and with
them Lima's `nested` option (it only hosted Firecracker). `machine create` and
`sandbox.create`'s `platform` refuse them `Unsupported`; there is no
`machine connect` or `machine disconnect`. A home may still hold a record of
one:

- `machine list` and `machine get` answer it as `broken` with `reason:
  unsupported` and `fix: layerfs machine remove <name>`; the long form's
  `message` is the reason (``platform `ssh` was removed in LayerFS 2.0``,
  or the same for `firecracker` or `microsandbox`).
- Every other call on it, its Sandboxes or a sqlite Store on it is
  `Unsupported` with that reason.
- `machine remove <name>` removes the record only (`InUse` while Stores or
  Sandboxes are recorded on it, as for any remove) and answers
  `{"ok":true}`; its long form's `outcome` is `removed` and its `message`
  says nothing on the host was touched. For microsandbox, remove the VM
  and its volume yourself: `msb remove --force <vm>` and
  `msb volume remove <vm>-data`. For firecracker, the microVM's folder
  `/layerfs/data/firecracker/vms/<vm>` and data disk
  `/layerfs/data/firecracker/disks/<vm>.ext4` are left in its Lima host;
  removing that Lima machine removes them with its data disk.

### Creating a VM machine

```bash
# not run
layerfs machine create dev lima --cpus 4 --memory 8G --disk 40G
```

- Defaults: 2 CPUs, 4 GiB memory, 20 GiB disk. Sizes are bytes, or text
  with `K`, `M` or `G`.
- Each VM machine has one VM named `layerfs-<first 8 digits of the home
  id>-<name>` (recorded as `vm_name`; a record without one, from an earlier
  build, reads `broken` and is refused with the fix `layerfs machine remove
  <name>`, which removes the record alone, by every call that needs its VM,
  the removes of its Sandboxes and Stores first while some are recorded on it;
  the built-in `docker` is then recorded again), one data disk mounted at `/layerfs/data` that
  holds many Stores, and one worker. A VM name keeps no punctuation, so a
  name that differs from another machine's only in `.`, `_` or `-` is
  refused `Exists`; a new machine's name is at most 46 characters (the VM
  name is at most 63, and the prefix takes 17). A `lima` machine's name
  is usually shorter still: Lima refuses an instance whose socket path
  `<Lima home>/<VM name>/ssh.sock.<16 digits>` reaches 104 characters
  (the Lima home is `LIMA_HOME`, else `~/.lima`, with its links
  resolved). `machine create` checks this before anything is made and
  refuses a longer name `InvalidInput`, stating the longest name that
  fits on this host: 59 characters less the length of the Lima home's
  path (39 where that is the 20-character `/Users/<name>/.lima`).
- `image` is the image its Sandboxes run in. The default Sandbox image (owner
  decision O36) is LayerFS's runtime image
  `layerfs-352-runtime:node24-https-apt-20261024` (image digest
  `sha256:afa1ef9c…f38e`: Node 24, Rust 1.85.1, the SQLite headers,
  Chromium and Selenium), sealed into the bundle as `sandbox-image.tar`.
  `--image` (and `machine update macos-host --image`) chooses another. On
  `docker` the recorded `image` is only its worker container's image; its
  Sandboxes' root is the bundle's `sandbox-image.tar` (D-8) unless
  `sandbox create --image` names another. Any local Docker image (a
  reference or Id) can be named on `docker`, `lima` and
  `apple-container`: it is copied to the machine once and reused
  (`machine images`, `machine load-image`, `machine prune`).
  The image is not published yet; a build places it with the bundle.
- A create checks the kind's tools first; a missing tool is `Unsupported`
  with `detail.missing`.
- A create needs the sealed Linux bundle (O16): the guest worker, the
  daemon, the Sandbox helper and the Sandbox image, with `bundle.json`
  holding its hash. An installed runtime carries its own in
  `bundle/linux-<arch>/` beside its `bin/layerfs`; otherwise it is in
  `<cache>/layerfs/bundle/<version>/`, which every build of the version
  shares. `<cache>` is `XDG_CACHE_HOME`, else `~/.cache`. Without it the
  create is `Unsupported` with `detail.missing` = `bundle:<folder>`. A
  bundle sealed from another commit than the calling build is
  `Unavailable`: a worker started from it would not answer this build's
  calls. Fetching the
  bundle is not built; a build places it.
- From the claim until the outcome is recorded, the record holds the
  create's `op` (`create`): its id, phase and process (process id, start
  time and boot); the machine reads `busy` with `op {name, phase,
  started_at}`. While that process lives, every other call on the machine is
  `Busy`.
- A create that fails before the VM exists removes what it made. Once the
  VM exists, a failure keeps it `broken` with `reason: failed`. `Unknown`
  keeps everything, its `op` marked uncertain, and is logged to
  `log/uncertain.jsonl`. Once that process is gone the machine is `broken`
  `unresolved`, and every call on it but `machine get`, `machine list` and
  the remove is `Unresolved`, naming `layerfs machine get <name>` and
  `layerfs machine remove <name>`.
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
  lock, writes its `op` (`remove`) into the record. The VM is deleted outside the home lock. While that
  process lives, every call that would start the machine, move a Store to
  it, create a Sandbox on it, update or remove it is `Busy`; `machine get`
  answers `busy` with that `op`. A refused delete puts the record
  back as it was; one whose outcome is unknown keeps the `op` marked
  `uncertain` with the `error`, and is logged, as for a create. A remover
  that is gone after a known outcome holds nothing.
- A remove is refused only while another process's `op`
  lives: it is the call that clears an `unresolved` machine.
- `machine update macos-host` and `machine update docker` set `image`,
  `docker` (the Docker socket) and `daemon` (the daemon their Sandboxes
  run: an absolute path to a file, replacing what was recorded;
  `machine get` shows it); `wsl` is a Windows host's setting. Any
  update is `InUse` while Sandboxes run on the machine, naming them.
- A recorded daemon that is not there (its build moved or was removed)
  refuses a Sandbox on that machine `Unavailable` before any launch,
  naming `machine update <name> --daemon <path>`; `layerfs home doctor`
  (check `home.native_daemon`) reports it as an error.
- Resizing a VM with `machine update` is `Unsupported` ("not built yet");
  nothing is changed.

### Starting and stopping (manual, #535 D11)

- A guest machine runs only between `machine start <name>` and `machine stop
  <name>`. Nothing starts or stops one by itself: no call starts a stopped
  machine, and a running machine, its worker and its Sandboxes stay up
  however long they are unused. A Store is still closed after a quiet
  minute (the worker's and the caller's Store connection reaper); that
  closes a file, not a machine. Before that, when the last call on a
  sqlite Store returns or a Sandbox on it closes, its write-ahead log is
  checkpointed into the file and truncated (#86), unless another process
  was still writing or reading it then.
- A call that needs the worker of a machine whose record has no `vm_name`
  is refused before anything is dialled, with the chain of removes of what
  is on it (the machine's own last) as its fix: a machine call `Failed`, a
  call on a sqlite Store on it `Unavailable` with `blocked_by {machine,
  reason: outdated}`, as `store list` shows that Store.
- A call that needs a stopped machine is refused `Unavailable`, naming the
  start command: "machine <m> is not running; start it with `layerfs
  machine start <m>`". It dials the worker `worker.json` records once, with
  no ping first (#535 R1); when that dial sent nothing (the connection, the
  secret or its answer failed), the call is refused the same way ("machine
  <m>'s worker did not answer (…); start it with `layerfs machine start
  <m>`") and nothing is started or resent. A call whose answer is lost
  after it was sent is `Unknown`; it is not resent.
- The calls that need the machine's worker: Store calls on a sqlite Store
  there, `project delete`, `sandbox create`, the Workspace calls that change
  something, `store move`, `store remove`, `store compact` and `backup
  create`. `store move` and `store remove` check their machines before they
  mark the Store.
- `machine start <name>` starts a stopped machine and its worker, waits
  until it is ready and records `worker.json`; a running machine answers at
  once. `machine stop <name>` stops the worker and, where there is one, the
  VM (`docker`: its worker container);
  disks and Stores stay. It is `InUse` while Sandboxes run on the machine
  (`detail.related` names them).
- No list, get, status or doctor call starts a machine (#535 P4): they
  answer from records and live probes. A stopped machine reads `stopped`,
  and what is on it `blocked` with `blocked_by {machine, reason: stopped}`
  (`store get`, `sandbox get`, `project list`); `workspace status` is
  `Unavailable` with `fix: layerfs machine start <m>`.
- Two `machine start` calls on the same machine take turns on its
  `start.lock`, which `machine create`, `machine stop` and `machine remove`
  also take. The second waits up to 600 s, then is refused `Unavailable`.
  The lock is always taken before the home lock, and the start re-reads the
  record once it holds it.
- `machine get <m> --with live` answers the probed `state` and, in its
  `live` section, `disk_free_bytes`, `tool_version` and `worker` (`{protocol,
  bundle}`). A stopped VM answers without being started. While a record
  holds an `op` the machine is `busy` and is not probed; a read never clears
  the `op`. A start or stop whose provider reply was lost leaves the machine
  `broken` `unresolved`, which `machine get <m> --with live` settles by
  observing. There is no change event; callers poll `home get`'s
  `generation`.

The OS host is enabled on a new home. Starting it explicitly enables it and verifies
the host worker. Idle worker exit leaves the host enabled. Stopping records
`stopped` before the worker drains. New local SQLite Store calls, maintenance operations, and new Sandboxes
on this host then receive `Unavailable` with a start command. A previously admitted
call finishes. If holders remain, stop reports `InUse` with the Sandbox names
and Store process IDs, and the host stays stopped. A later stop checks again.
A forced stop performs the host Sandbox teardown and records its outcomes.
A later explicit forced stop can reconcile an uncertain previous stop after
its owner exits. Direct PostgreSQL calls continue while the host is stopped.

### Routes

- Calls on a sqlite Store on a machine whose calls run in the caller's
  process (`macos-host` on macOS and `linux-host` on Linux; Windows has no host machine),
  and calls on a PostgreSQL Store, run in the caller's process.
- Calls on a sqlite Store on any other machine go to that machine's worker,
  behind a secret, over a forwarded loopback port. The caller sends the
  Store's record and the home id with the call; the worker never reads the
  home, and its own root (`/layerfs/data`) is never treated as a home.
- A connection starts with a small frame holding only the secret (at most
  4 KiB, read within 10 s); the worker answers it, or refuses `Unavailable` (raised as `Denied`) and
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
  build) is checked by protocol alone. [`layerfs machine doctor`](calls.md#the-doctors)
  reports a running worker of another build, with the fix `layerfs machine
  stop <m> && layerfs machine start <m>`.
- A worker holds a Store's lease for as long as it keeps the Store open,
  and closes its own open copy before a move, retire or remove reads or
  deletes the file.
- One process can use Stores on several machines at once.
- A Sandbox on `docker` runs the `namespaces` backend inside the docker
  machine's worker container, as a Sandbox in a VM does (D-8,
  layerfs-project#111). The worker container is privileged, in a cgroup
  namespace of its own, and mounts only its data volume and the bundle
  (never the Docker Engine socket). A Sandbox's root is the bundle's
  `sandbox-image.tar`, or the image `--image` names: a digest that archive
  holds, or a host Docker image (reference or Id) that the host exports
  and sends to the worker once, unpacked under the worker's
  `machine/linux-guest/run/images/`. The worker never sees the Docker socket.
  Its record keeps `platform: "docker"` and shows
  `backend: "namespaces"`. A PostgreSQL Store's Sandbox starts the same way
  as a sqlite Store's on that machine; it binds no Store file. A Sandbox
  recorded under the retired docker backend (one container per Sandbox) is
  refused `Unsupported` with a message to recreate it; `sandbox remove`
  removes its records (`already_gone`) and leaves its container, which
  `left` names for `docker rm` when its record keeps the name.
- Every other machine is reached through its worker, `docker`
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
  and `sandbox.remove` go through the worker; a remove ends the forward.
  `docker` (its worker container's ports are published only when it
  starts) and `apple-container` forward nothing:
  every call goes through the worker. A forward that failed at create
  leaves the Sandbox on the worker route (`forward_failed` in the answer).
  On a stopped machine these calls are `Unavailable`, naming `machine
  start`; a forward that has ended is `Unavailable` too, naming `machine
  start` and `sandbox remove`.

## Stores

A sqlite Store is one file on one machine at a time. A postgresql Store is
one database. There is no conversion between the two kinds (D2). Every
sqlite Store is opened with WAL and `synchronous=NORMAL` (owner, 2026-11-17,
ahead of its safety gate: no power-loss guarantee is claimed); its record's
profile label is `normal` (a record that says `disposable` reads as `normal`). A PostgreSQL Store's write setting is
`synchronous_commit`.

### sqlite Stores

```console
$ layerfs store create shop
{"ok":true}
$ layerfs store move shop macos-host
{"error":"InvalidInput","message":"store move, check: Store shop is already on macos-host"}
```

(Run for this manual on macOS. `store move shop dev` to a VM and back was
run as `store push` and `store pull` on 2026-11-08 on `docker`, `lima` and
`apple-container`, before the two became `store move`; not rerun.)

- `store create <store>` makes an empty sqlite Store on the machine whose
  calls run in this process (`macos-host` on macOS and `linux-host` on Linux), at
  `store/<store>/store.sqlite`; `--machine M` makes it on a running machine
  directly, through its worker. On a host with no such machine (Windows) a
  create without `--machine` is `Unsupported`. `store get` and `store list` answer its
  `engine`, `state`, `machine` and `projects`, and never the Store's keys.

Opening a new local SQLite connection for ordinary Store calls waits for
the Store directory's writer lane before SQLite attaches to the WAL. A new
connection may wait while a write or maintenance holds the lane. Reads on
an open connection continue without that wait.

For a local SQLite Store held open by a host worker, move, remove,
compact and upgrade record a `worker-close` phase and ask that worker to
release its Store. The phase keeps new calls out until the caller verifies
release. A held Store record lock yields `InUse` before the worker closes
its handle. A lease-file removal error appears as a close failure. A lost
or malformed close reply yields `Unknown` with a request ID and leaves
the Store `unresolved`. The same maintenance command can proceed once the
original worker releases its lease or is proved dead. Compaction refuses
a live host worker lease it cannot verify and displays its process ID.
SDK and Sandbox holders retain their normal admission rules. The worker
records close and checkpoint outcomes in
`log/worker-close.jsonl`.

- `store move <store> <machine>` moves the file between the machine whose
  calls run in this process (`macos-host` on macOS and `linux-host` on Linux) and another
  machine: from the host machine to a machine, or from its machine back to the host machine
  (#536 H1). A Store on a machine moves to another machine by `store move
  <store> macos-host` (or `linux-host`) first; asked directly it is `InvalidInput`, naming that
  call. On a host with no such machine every move is `Unsupported`. The phases are `check`, `snapshot`, `send`, `verify`, `install`,
  `record` and `retire`, once each. The bytes pass through the calling
  process in pieces. Retire deletes the source's copy: on the host machine (in this process) the file
  with its `-wal`, `-shm` and any partial copy (the folder keeps the home's
  records); on another machine the Store's whole folder under its worker's
  root (a symlink in it is removed, never followed). Short: `{moved_bytes}`.
  - A move of a PostgreSQL Store, or to where the Store already is, is
    `InvalidInput`; to an unknown machine `NotFound`; to or from a stopped
    machine `Unavailable`, naming `layerfs machine start <m>` (#535 D11).
  - Its check runs before it marks the Store: both machines answer, the
    target holds no Store file of that name (`Exists`), and the target has
    the Store's bytes (with its write-ahead log) plus a headroom of 1 GiB,
    or a tenth of the Store when that is more, free. Too little is
    `Capacity`, naming the bytes needed and free (`detail.needed`,
    `detail.free`).
  - It is `InUse` while any other process holds the Store or a Sandbox of
    one of its projects is recorded; `detail.related` names each as
    `{kind: "lease", name: <pid>}` or `{kind: "sandbox", name}`.
  - While a move, remove, compaction, backup or restore runs, the Store's
    record holds one `op` (#536 §7.1) and the Store reads `busy` with
    `op {name, phase, started_at}` (`move`, `remove`, `compact`, `backup`,
    `restore`). A move or remove holds the Store exclusively: every other
    operation, lease and Store call on it is `Busy` while that process
    lives. The lease is taken, and the record re-read, under the Store's
    lock, `store/<store>/lock`; the locks cover only the checks and record
    writes, never a machine start, a Store step or the close of an open
    Store.
  - A certain failure drops the `op`. An uncertain one (an `Unknown` step,
    or a failure of the install or record phase) keeps both copies, keeps
    the `op` uncertain and logs it to `log/uncertain.jsonl`. The record
    write that puts the Store on its new machine records `last_transfer`
    (`call`, `from`, `to`, `ended_at`, `size`, `sha256`, `heads_sha256`,
    `retired`) and keeps the `op` at phase retire while the source's copy
    is deleted.
  - Once that process is gone, an uncertain `op` leaves the Store `broken`
    `unresolved`, with `fix` the same `layerfs store move <store>
    <machine>`; every later operation, lease and Store call on it is
    `Unresolved` until that move, or `store remove`, clears it. A new move
    first deletes the copy left on whichever of the two machines the record
    does not name, then runs as usual. A partial `.incoming` copy a failed
    move left is reported by `store doctor` and cleared by the next move or
    remove.
  - During `snapshot`, an idle Store whose crashed opener left a non-empty
    `-wal` is checkpointed under the move's exclusive lease before hashing.
    Every step runs once: a lost worker answer (`Unknown`) fails the move
    uncertain and nothing is resent.
- `store remove <store>` deletes a sqlite Store wherever it is, with every
  project in it. Its backups stay in their buckets. It is `InUse` while the
  Store is held and `Busy` while a live process runs an operation on it; a
  stopped machine is `Unavailable`, naming `machine start`. It is the call
  that clears an unresolved move or remove. A Store whose record this build
  does not read, or on a machine whose record has no 2.0 name, goes by its
  records alone, and `left` says what may remain (its file).
- `store get` starts no machine. Its state comes from the home's records;
  `--with live` reads the Store where it lives (its file through its
  machine) and settles an uncertain backup or restore whose owner is gone;
  `--with stats` opens it for its counters. See
  [`store`](calls.md#store-11) for every key and the compaction phases.
- A lease or `op` records its process's start time and boot, so one whose
  process id was reused by another process is stale. A process that ends
  with Stores open leaves their leases; taking a lease removes the Store's
  stale ones first, under the Store's lock, and logs each
  (`log/leases.jsonl`), as a move, remove or disconnect does; reads leave
  them. A process that is about to exit calls `layerfs.close()` (Node and
  Python, [SDK](sdk.md)) to close its open Stores and drop its leases.
- Init into a sqlite Store on another machine reads the folder on the
  caller's host and copies it, in pieces of at most 8 MiB, into
  `incoming/init-<id>` under that machine's worker root; the worker runs
  Init on the copy under the Store's lease there and deletes the copy
  when Init ends (#536 H11, H11a). The machine needs the copy's bytes plus
  `max(1 GiB, 10%)` free first. A failure while copying deletes the copy; a
  lost answer is `Unknown`, names the copy that may remain (in its message
  and in a line of `log/uncertain.jsonl`, with the Init's deadline), and
  keeps the project `broken` `unresolved` (`project delete` clears it).
  `project delete` sends the worker a discard of that copy, which removes
  it unless an Init still holds it ([`home`](calls.md#home-5)). Not run on
  a real VM.
- A Sandbox of a sqlite project runs only on the Store's machine.

### PostgreSQL Stores

This computer is the one place a PostgreSQL Store's address and login are
saved (owner, 2026-11-24). Machines are attached to the Store and dial with
both; no machine has an address or a login of its own. The calls were run
for this manual's [`store`](calls.md#store-11) section against a local
`postgres:17` on macOS (`connect`, `list`, `get`, `attach`, `doctor`,
`detach`, `disconnect`). In short:

```console
$ printf '%s' "$PG_PASSWORD" | layerfs store connect shop-pg 'postgresql://127.0.0.1:55437/shop?sslmode=disable' --user postgres --password-stdin
{"ok":true}
$ layerfs store attach shop-pg --machine macos-host
{"outcome":"attached","latency_ms":1}
$ layerfs project init shop-pg/load ./load        # not run here
$ layerfs store attach shop-pg --machine build-vm  # not run: needs a reachable address
$ layerfs store connect shop-pg 'postgresql://127.0.0.1:55437/shop?sslmode=disable' --synchronous-commit off
$ layerfs store doctor shop-pg
{"ok":true}
```

On macOS and Linux, the CLI sends `file read`, `file stat` and `file list`
on a PostgreSQL Store to the host worker on this computer, which keeps
read-only PostgreSQL connections open between calls, so a later read logs
in nothing. The CLI sends the saved password in its request; the worker
saves it nowhere, overwrites its own copies once a call no longer needs
them, keeps it only in the settings of that Store's open connections (and
in memory its PostgreSQL client used, freed but not overwritten), and runs
with core dumps off (on Linux it also cannot be read by your other
processes; macOS has no such setting). Each connection
checks the schema and the Store binding when it opens, using the TLS mode
from the Store's URL, and repeats a short check of the Store's identity
before each later call; if that check fails, the worker closes the Store's
idle connections and opens one new one. A Store keeps at most 4 such
connections and a worker 8; they close after a minute unused, and `store
connect` (a new password) or `store disconnect` closes them. PostgreSQL
reads wait on their own threads, and a read waits for a connection of its
Store without holding a thread, so reads of other Stores never wait behind
a busy Store. A read that has not started yet does not keep `store
disconnect` or `store upgrade` waiting. When 64 PostgreSQL reads are
already waiting or running, the next one is refused `Unavailable` within
about 5 s, without being run; a read whose caller went away before it
started is never run. A Store connected before this release has no login revision:
it keeps the direct route, one read-only connection per call opened and
checked the same way, until `store connect` is run for it again. So do a
stopped host machine, other platforms, and SDK calls.

- **`store connect <store> <url> --user U --password-stdin`.** The user and
  password are required on a Store's first connect and are checked from
  this computer before anything is saved. The record keeps the URL, the
  user and `synchronous_commit`; the password is kept in the home's secret
  folder (`secret/login/<store>.json`, mode 0600, in a 0700 folder), comes
  on stdin only (Node and Python pass `password`) and is never answered,
  logged or traced. `store get` says whether it is saved
  (`password_saved`) and when it was last changed (`password_changed_at`).
  The URL starts `postgresql://` or `postgres://` and holds host, port,
  database and TLS settings, read only as the PostgreSQL client reads it,
  and is recorded and shown exactly as given: LayerFS never cleans or
  rewrites a URL. One that may hold a login is refused `InvalidInput`,
  before any effect: it holds an `@`, the client reads a user, a password
  or a host holding an `@` in it (a login written percent-encoded,
  `postgres://alice%3Ap%40db/app`), the client does not read it (a key it
  does not know, such as
  `sslpassword`, `sslrootcert`, `sslkey` or `sslcert`, or an `sslmode`
  other than `disable`, `prefer` or `require`), or its percent-decoded text
  holds a login key and `=` anywhere, in any case (`user`, `password`,
  `passfile`, `sslkey`, `sslcert`, `sslpassword`, `oauth_client_secret`,
  `scram_client_key`, `scram_server_key`:
  so `postgres://h/db?application_name=me?password=p` and
  `postgres://h/db&password=p` are refused too). One holding a `#` is
  refused as well (write a `#` or `@` that is part of a database name or a
  value `%23` or `%40`; no host holds an `@`). The fix names the address `<url>` and the user `<user>`: `store
  connect <store> <url> --user <user> --password-stdin`. Other text the
  client reads as part of a host, database or value is the client's
  (`postgres://h/db&sslmode=require` names the database
  `db&sslmode=require`). The refusal names the TLS key or `sslmode` value
  it met (`sslmode=verify-full`), or the `#` that ends a value it would
  otherwise read (`sslmode=require#f`); `sslmode=require` verifies the
  server's certificate chain and host name. A private CA is trusted where
  the Store is dialled: on Linux (OpenSSL) in the system's trusted roots
  (`SSL_CERT_FILE` reaches only calls in the process that reads it: a
  Sandbox's daemon starts with a cleared environment), on macOS in the
  login or system keychain. An empty database gets the `layerfs` schema
  with a new identity; a database that already holds it keeps its
  identity, and its projects become this home's project records. A Store
  name already in the home is `Exists` (a sqlite Store's too), and so is
  the same database under a second name, whatever address or login reaches
  it: a database is known by the binding (the id) in its `layerfs` schema,
  read on connect and checked again under the home lock. A replica or a
  copy of the database carries the same binding, so it is the same Store;
  `store get`'s long form shows the binding's first 16 hex characters.
  `synchronous_commit` is `off`, `local` (the default), `remote_write`,
  `on` or `remote_apply`.
- **`store connect` again** changes a connected Store's address, user,
  password or `synchronous_commit`; an omitted user or password keeps the
  saved one. The result must reach the same Store (its binding) before
  anything is saved. Attached machines dial the new address and login from
  their next connection, so a new address, user or password is checked
  from each of them too: one that does not reach it refuses the connect
  `Conflict`, nothing saved, naming the machines (`machines`) with
  `layerfs store detach <store> --machine <m>` as the fix. `--dry-run` makes
  the same checks and saves nothing, answering `outcome: "tested"` with
  `here` (this computer's check) and one row per attached machine.
- **`store attach <store> --machine M`** lets machine `M` run Sandboxes of
  the Store: `M` must exist and its platform run PostgreSQL Sandboxes, and
  the Store is checked from `M` with the URL and login saved here before
  `M` is recorded as attached; the check is kept as `M`'s reach. Attaching
  again checks again (`outcome: "checked"`); `--dry-run` checks and
  records nothing (`outcome: "tested"`). A refused check names the
  `machine` and the connection `step` it stopped at (`configuration`,
  `transport`, `tls`, `authentication`, `database`, `schema`, `identity`,
  the doctor's `store.connection.<step>`). The host machine is attached
  like any other (`store connect` attaches nothing); its platform decides
  whether a loopback URL is allowed, and its reach is its own, apart from
  this computer's. `store detach <store> --machine M` removes it,
  refused while a Sandbox of the Store's projects is on `M`; `machine
  remove` detaches a machine from every Store.
- **Who connects.** `store get` answers `reach` (this computer's) and
  `machines`, one row per attached machine with `reached` (`ok`, `failed`,
  or absent when nothing was seen) and, in the long form, the last reach
  (`at`, `ok`, `latency_ms`, `server_version`, `error`); `store list` names
  the attached machines. Nothing about a database is stored on a machine;
  reaches are kept beside the Store's record.
- **No migration.** A record an earlier build wrote is read as it is, with
  no state of its own: a call that dials a record without a `user`, or one
  whose password this home does not hold, is refused `InvalidInput` (the
  latter with `store connect <store> <url> --password-stdin` as its fix).
- **`store disconnect <store>`** forgets the Store, its saved password and
  its attachments; its data stays on the server. It is `InUse` while a
  Sandbox or Workspace uses the Store.
- `project init` into a PostgreSQL Store runs in the caller's process,
  under the Store's lease. PostgreSQL is built into every artifact.
- A PostgreSQL Store is on no machine and has no default machine. A
  Sandbox of it runs on the machine `sandbox create --machine M` names, or,
  with none named, on the one machine attached to the Store (none attached
  is refused with `store attach` as its fix; several are listed and one
  must be named). The machine must be attached to the Store (refused
  `InvalidInput`, with `store attach` as its fix, otherwise) and its
  platform must run PostgreSQL Sandboxes (`host` on
  macOS and Linux, `docker`, `lima`, `apple-container`); `sandbox create`
  checks from the machine that the server answers and holds this Store.
  The URL is dialled exactly as configured, never rewritten. On a machine
  that does not share this computer's loopback (`docker` on Docker
  Desktop, `lima`, `apple-container`) a URL naming it (`localhost`,
  127.0.0.0/8, `0.0.0.0`, `::1`, in any spelling) or a Unix socket is
  refused `Unsupported`, naming what the machine calls this computer
  (`host.docker.internal`; `host.lima.internal`; `apple-container`: its
  network's gateway, on which the server must listen) and advising to
  connect the Store again with an address the machine reaches (`layerfs
  store connect <store> <url>`); the host machine and `docker` on a Linux Engine
  share the loopback and dial it as given. A Sandbox resolves names as its
  machine does. Only the calls that dial the URL (`sandbox create`,
  Commit, a Workspace status) are refused for it: a Sandbox whose URL its
  machine cannot dial can still be read and removed. A forced `sandbox
  remove` there still ends the mount intent each of its Workspaces held:
  the machine closes the Sandbox without the URL and hands those intents
  back in its answer, and this computer ends each once with the Store's
  address (PostgreSQL Store calls run in the caller), so `branch remove`
  and `store compact` are not refused for them afterwards; one it could
  not end is a sentence of `left`. The intents travel only in that answer
  and nothing records them, so they stay if the remove ends between the
  machine stopping the Sandbox and this computer ending them: the answer is
  lost (the remove reports `Unknown`), the machine cannot delete the
  Sandbox's folder after stopping and deleting its runtime (the remove is
  refused `Failed`), or this computer's process ends first. A retried
  `sandbox remove` then answers `already_gone` with an empty `left`, and
  `branch remove` of the Branch and `store compact` stay refused (`InUse`,
  `mounts active`), since nothing can prove a PostgreSQL mount intent's
  owner gone. Any other reason the URL cannot be made (the password is not
  saved, or the record names no user) refuses the remove, forced or not,
  with that reason and its fix: connect the Store again with its login and
  the remove goes through. The login travels only in the one request. The
  server always runs off the machine; nothing installs PostgreSQL in a
  machine (D12).
- `store doctor <store>` checks the Store from this computer and then from
  every attached machine (`--machine M`: from `M` alone, which must be
  attached), each as one `store.connection` row with `from`, and records
  each reach; when this computer cannot connect, no machine is checked.
- Not built: backups of PostgreSQL Stores (back it up with PostgreSQL's own
  tools).

## Backups

Backups are for sqlite Stores. Buckets are r2 or MinIO (an S3 endpoint);
1.1.0's `local_folder` provider is not in 2.0. Each Store has one link per
bucket (O23), so it can be backed up to several buckets.

```bash
# not run for this manual
printf '%s' "$SECRET" | layerfs backup connect lab http://127.0.0.1:9000/layerfs/backups --access-key "$KEY_ID" --key-stdin
layerfs backup create shop lab
layerfs backup list --bucket lab --text
layerfs backup restore lab shop shop
layerfs backup doctor lab --full
```

- **`backup connect <name> <url> --access-key K --key-stdin`.** The URL is
  `http(s)://<host>[:port]/<bucket>/<prefix>` for MinIO, or
  `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2. Both
  keys are required; the secret comes on stdin only (Node and Python pass
  `secret_key`) and is kept in the home's secret folder
  (`secret/bucket/<bucket>`, 0600), never answered or logged. One
  listing checks the bucket and key before anything is recorded: a bucket
  that does not answer or refuses the key is `Unavailable`. To change a
  key, connect again under the same name at the same URL (the host's case,
  a default port and slashes around the bucket and prefix aside): the new key is
  proven first (nothing changes when it fails), then it replaces the old
  one and the Stores' links stay (`outcome: "rotated"`). A taken name at
  another URL is `Exists`, with the fix `layerfs backup disconnect <name>`;
  a new name works too. A bucket record written before 2.0 has no key id
  and is refused with the fix that connects it again at its URL.
- **`backup create <store> <bucket> [--file F]`.** `F` defaults to the
  Store's name. The snapshot is taken on the Store's own machine; its bytes
  pass through the caller's process, so machines never hold bucket keys.
  The object carries its sha256, heads, binding and the home it was
  exported from as metadata. An object there exported from another home is
  `Exists`; one that is not the copy this home last moved through the
  Store's link is `Conflict`. Both checks run before the snapshot and again
  before the upload. Progress: `check`, `snapshot`, `send` (from a
  machine), `hash`, `upload`. Short: `{file, size}`.
- **`backup restore <bucket> <file> <store>`.** It downloads the file,
  verifies it and installs it as the sqlite Store `store` on the machine
  whose calls run in this process (`macos-host` on macOS and `linux-host` on Linux;
  `Unsupported` on a host with none): a new Store, or one that replaces an
  idle Store there. Replacing a Store
  that changed since its last backup or restore through that bucket is
  `Conflict`; one that a process holds open, or one of whose projects has a
  running Sandbox, is `InUse`. A Store on another machine is `Unsupported`
  (`store move <store> macos-host` first). Progress: `check`, `download`,
  `verify`, `install`. Short: `{projects}`, or `{"ok":true}` for an empty
  Store.
- A backup or restore is the Store's `op` (`backup`, `restore`), so every
  client reads the Store `busy`; a backup runs beside the Store's readers,
  a restore holds the Store. A failure writes what was tried to the link's
  `attempt` (`store get --detail` shows it); a lost reply leaves the Store
  `broken` `unresolved` until `layerfs store get <store> --with live`
  settles it.
- **`backup list [--bucket B]`.** One row per Store file in each bucket,
  `{bucket, file}` with `size` and the linked `store`; `behind` says the
  Store changed since its last backup, `changed` that the bucket's file is
  not the one the link last moved, `missing` that a linked file is gone. A
  bucket that cannot be listed is named in `not_listed` and the others are
  still listed; one whose key is missing or refused carries the `fix` that
  connects it again with its key.
- **`backup disconnect <bucket>`** forgets the bucket and this home's links
  to it (`unlinked` names the Stores). Its objects stay. LayerFS never
  deletes a bucket object.
- **`backup doctor <bucket> [--full]`** checks the record, the key, the
  transport and a listing; `--full` also writes, reads and deletes one test
  object.
- MinIO tests cover create, the link states, `Conflict`, restore as a new
  Store, a Store behind a worker and concurrent restores. R2 was `NOT_RUN`
  (no credentials). Not run for this manual.

## Secrets

A PostgreSQL Store's password (`store connect --password-stdin`) and a
bucket's secret key (`backup connect --key-stdin`) are kept in the home's
secret folder (`secret/`, 0700; each file 0600), read from stdin only in the
shell, and never answered, logged or traced; the call log records a secret's
byte count. A PostgreSQL Store's record keeps its user; its password is
`secret/login/<store>.json`. Anyone who can read the home can make every
call. There is no keychain (O1). A reference an older record names
(`env:NAME` or `file:/absolute/path`) is not read: a PostgreSQL Store whose
password this home does not hold is refused when it is dialled, with the
fix `layerfs store connect <store> <url> --password-stdin`, and a bucket
record with no key id is refused with the fix that connects it again
(`layerfs backup connect <name> <url> --access-key <key> --key-stdin`).

### Host worker transport

The host-worker transport uses private Unix sockets and checks the user of
the other process before exchanging any operation bytes. A CLI call chooses
its route before sending. A lost answer after an operation send starts is
reported as `Unknown` with a request ID. The CLI makes one attempt.
Two installations sharing a home may need to wait for an existing worker's
idle exit before the other installation can use that home worker.

`layerfs machine get macos-host --with live` reports `live.machine_state`
and `live.worker.health` separately. The host machine is enabled at this
stage. The probe reports an absent, stale, unreachable, ready or draining
endpoint, with worker identity and activity when available. Before a
worker publishes its socket, the probe can observe only an absent endpoint.

`layerfs machine doctor macos-host --detail` uses one authenticated probe and
includes its evidence. An idle home passes the endpoint check. A worker
of another build yields a compatibility notice before the ping. The
diagnostic never starts or replaces a worker. `home doctor` checks the
host-worker directory's ownership and permissions. `store doctor` shows
an exact live worker lease beside other lease holders. Store operations
perform stale lease cleanup under the Store lock.
