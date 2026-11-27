# Inspection

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The route of the reads a client polls to watch a machine and its Sandboxes:
one worker operation, `inspect`, with bounded admission and deadlines, within
[Sandboxes](README.md).

| | |
| --- | --- |
| Source | [`crates/layerfs-inspect/src`](../../../../control/crates/layerfs-inspect/src), [`layerfs-sandbox/src/backend/confined.rs`](../../../../control/crates/layerfs-sandbox/src/backend/confined.rs) with `os/{linux,macos}/confined.rs` (`root_of`: a process's root), [`src/log/tail.rs`](../../../../control/crates/layerfs-home/src/log/tail.rs), [`admin/route/executor.rs`](../../../../control/crates/layerfs-store/src/admin/route/executor.rs) `on_machine_read`, `Worker::read`, [`guest/client.rs`](../../../../control/crates/layerfs-machine/src/route/guest/client.rs) `send_within`, [`worker/ops.rs`](../../../../api/crates/layerfs-api/src/worker/ops.rs) |
| Used by | [API: `machine.log`](../../api/sdk/machine.md#machinelog), [`sandbox.log`](../../api/sdk/sandbox.md#sandboxlog), [`sandbox.files`](../../api/sdk/sandbox.md#sandboxfiles), [`machine.files`](../../api/sdk/machine.md#machinefiles), [`machine.stats`](../../api/sdk/machine.md#machinestats) (and with `--with space`), [`sandbox.get`](../../api/sdk/sandbox.md#sandboxget) of a forwarded Sandbox |

## Picture

```text
 call (machine.log, sandbox.log, sandbox.files, machine.files, machine.stats,
       machine.stats --with space)
   |  inspect::ask(home, machine, kind, args)
   v
 on_owner = place::on_machine_read     the machine's record and worker record,
   |                                   read as files: no intent check, no lock
   +-- Executor::Local  (host) ------> admit -> serve(Side::Home(home))
   |
   +-- Executor::Worker -------------> worker op "inspect" { kind, .. }
         one connection, one attempt     |  place::READ: connect 2 s, hello 2 s,
         no answer in time: Unavailable  |  answer 8 s
                                         v
                                 in the worker: admit -> serve(Side::Worker(root))

 admit (owner.rs), per process:
   2 permits ---- none free -------------------------> Busy, at once
   helper thread "inspect" owns the permit until it ends
   answer within 6 s (HARD) --- else ----------------> Unavailable, "abandoned";
                                                       the helper keeps its permit
   a panic in the helper ----------------------------> Failed
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| One route | A request is `{ kind, .. }`. `ask` runs it in this process for a machine whose calls run here, and sends it once as the worker operation `inspect` otherwise; a Sandbox whose daemon the home reaches directly (`lima`) is asked through its machine's worker too, where its run folder is. | [`route/inspect.rs`](../../../../control/crates/layerfs-sandbox/src/route/inspect.rs) `ask`, `in_worker` |
| Kinds | `machine_log` (`log/worker.log` and `worker.1.log` under the worker's root), `sandbox_log` (`daemon.log` in the Sandbox's run folder), `sandbox_files` and `machine_files` (a Sandbox's or a machine's roots, a folder page or a file window: [Files](#files)), `overlay_sizes`, `machine_stats`, `machine_space` (`machine.stats --with space`), `sandboxes_live` (the four below; no 2.0 call asks `sandboxes_live`, which the retired `live` option of `sandbox.list` used). Another kind is `InvalidInput`. | `serve`; [`log.rs`](../../../../control/crates/layerfs-inspect/src/files/log.rs), [`walk.rs`](../../../../control/crates/layerfs-inspect/src/files/walk.rs), [`files.rs`](../../../../control/crates/layerfs-inspect/src/machine/files.rs) |
| `overlay_sizes` | `{ sandbox, namespaces }` to `{ overlay_sizes: [{ namespace, bytes }], daemon, usage, usage_reason? }`: one `lstat` per Workspace overlay file, and the Sandbox's daemon process and cgroup usage, which ride the same answer (`namespaces` may be empty). What a get reads beside the run folder, here for a forwarded Sandbox. | [`overlay.rs`](../../../../control/crates/layerfs-sandbox/src/status/overlay.rs), [`run_folder.rs`](../../../../control/crates/layerfs-sandbox/src/status/run_folder.rs) `observed` |
| `machine_stats` | `{ stores: [name], sandboxes: [name] }`, the sqlite Stores and Sandboxes the home records on the machine, to `{ observed_at, worker, resources, data_disk, stores, sandboxes: { live, items }, complete, reason? }`: processors and memory from the kernel (`/proc/meminfo`, and one read of `/proc/stat` for the processor counters `cpu_busy_usec`, `cpu_total_usec` and the boot time; macOS: `sysctl` for memory, no processor counters, `cpu_unsupported`), one `statvfs` of the side's root, one `lstat` of each Store's `store.sqlite` and `-wal`, and the liveness check below. `items` has one row per named Sandbox, `{ name, live, live_reason?, started_at, usage, usage_reason? }`: the liveness check, the start of the process its daemon record names (placed with that one boot time), and its cgroup usage as below; a row the soft budget did not reach stays, null with `deadline` (layerfs-project#182). The home adds the machine's platform, the `scope` of each measure, the provider's volume name and the recorded count. | [`stats.rs`](../../../../control/crates/layerfs-inspect/src/machine/stats.rs), [`kernel.rs`](../../../../control/crates/layerfs-inspect/src/machine/kernel.rs) |
| `sandboxes_live` | `{ sandboxes: [name] }` to `{ sandboxes: [{ name, live, reason? }], complete, reason? }`: each Sandbox's daemon record under the side's root, and whether the process it names is that process (pid, start time, boot) and runs. No record or no daemon record is `live: false`. | [`live.rs`](../../../../control/crates/layerfs-inspect/src/machine/live.rs), [`stats.rs`](../../../../control/crates/layerfs-sandbox/src/status/stats.rs) `live` |
| `machine_space` | `{ stores: [name], sandboxes: [name] }`, as `machine_stats`, to `{ observed_at, disk, layerfs, outside, parts, stores, sandboxes, unreadable, complete, reason? }`: one `statvfs` of the side's root and one walk of LayerFS's folders under it, each size `st_blocks` × 512 by `lstat` (a file with several names once): each named Store's folder, each named Sandbox's run folder (found as `run_folder` does) split into `overlays`, `writes`, `home`, `log`, `runtime`, the image cache, the run root's other entries, `log/` and `trace/`, and `incoming/`, `backup/`, `sandbox/`. A run folder's `workspaces/` and a private root `<name>.root` are neither entered nor `stat`ed, no other device is descended into, no file is opened and the root itself is never listed. It stops at `ENTRIES` (250,000) or the soft budget with `complete: false`, each number then a lower bound. The home adds the platform and the disk's `scope`. | [`space.rs`](../../../../control/crates/layerfs-inspect/src/machine/space.rs) |
| Who the worker is | The worker's `inspect` operation puts `worker: { version, pid, started_at, protocol, bundle }` on every request it serves; `machine_stats` answers it. On `Side::Home` there is no worker and it is `null`. | [`worker/ops.rs`](../../../../api/crates/layerfs-api/src/worker/ops.rs) `about` |
| A Sandbox's process and usage | `daemon: { pid, started_at }` from the daemon record (`started_at` only while the process runs). `usage` from six files of `/sys/fs/cgroup/layerfs/<container>` for a `namespaces` Sandbox (`cpu.stat` `usage_usec`, `memory.current`, `memory.peak`, `memory.stat` `file`, `pids.current`, `memory.events` `oom_kill`), each value `null` where the file or line is absent, with `usage_reason` `no_controller` when `memory.current` or `pids.current` is the absent one; `usage: null` with `usage_reason` `host_process` (runtime `process`), `not_supported` (any other runtime), `cgroup_v1` or `no_cgroup` otherwise. Every read is this side's own `/proc` and `/sys/fs/cgroup`, never the Sandbox's root or namespaces, and runs no tool from its image, so every image reads the same. The recorded process of a `namespaces` Sandbox is its supervisor, not the daemon under it. | `process.rs` |
| Find the run folder | The `directory` of the daemon record in `<root>/sandbox/<name>/record.json` on the side that runs the Sandbox, read as a file. It is used only when it is a folder directly under that side's run root; a missing record or folder is `NoSuchSandbox`. | `run_folder` |
| Admit | Two permits per process (`PERMITS`). A request with none free is `Busy`. The work runs on a helper thread that owns the permit until it ends, so a read stuck in the kernel holds one permit and one thread and nothing else. | [`inspect.rs`](../../../../control/crates/layerfs-sandbox/src/route/inspect.rs) `admit` |
| Deadlines | Soft 4 s (`SOFT`): `Budget::spent()` is what a kind made of many units checks between them (a folder page looks before each entry and every 1024 names, and then answers what it has with `has_more: true, reason: "deadline"`). Hard 6 s (`HARD`): the caller is answered `Unavailable` and the helper is left to end by itself. The caller of a worker waits at most 8 s for the answer (`place::READ`), then reports `Unavailable`; a read changes nothing, so it is never `Unknown`. | `owner.rs`; [`executor.rs`](../../../../control/crates/layerfs-store/src/admin/route/executor.rs) `READ`, `Worker::read` |
| Panic | In the helper: `Failed`, "an inspect request panicked and changed nothing", and the permit is free. On the worker's connection thread before any frame: `Failed`, as every read-only operation (`ops::READ_ONLY`). | `admit`; [Workers and reach](../machines/workers-and-reach.md#one-connection-never-stops-the-worker) |
| Read a log | One window per request: the last lines, the lines after a cursor, or the lines before one, over a file and the one file it rotates to. At most 64 KiB is read per request; files are opened `O_NOFOLLOW | O_NONBLOCK` and must be regular. | [`tail.rs`](../../../../control/crates/layerfs-home/src/log/tail.rs) `read` |

## The cursor

```text
 c1.<file>.<inode hex>.<first-line hash hex>.<offset hex>
      0 = the current file, 1 = the rotated one

 worker.1.log  [ older lines ............... ]   offset
 worker.log    [ newer lines ..... | held-back partial line ]
                                   ^ cursor of a tail read
```

A cursor names a byte offset in one file of the pair and that file's
identity: its inode and a hash of its first line. A read with a cursor whose
file is no longer that file (rotated, replaced, shorter than the offset) is
`InvalidInput`, "the cursor is stale"; the caller reads again without one. A
backward read that reaches the start of the current file goes on in the
rotated file; a forward read that reaches the end of the rotated file goes on
in the current one. A current file that does not exist yet has the cursor
`c1.0.0.0.0`, which reads the file from its start once it is there.

A line longer than the window is answered in pieces, each `{ text,
continued: true }` but the last; windows are cut on UTF-8 boundaries, so the
pieces join to the line. Control characters are written as Rust escapes
(`\t`, `\u{1b}`) and bytes that are not UTF-8 as U+FFFD.

## Files

`sandbox.files` and `machine.files` are served by one walker
([`walk.rs`](../../../../control/crates/layerfs-inspect/src/files/walk.rs)).
A view is a `Scope`: a base folder, the path its entries are shown under,
the mount points beneath it, whose hard links are read, and its `Rules`.
`files::answer(scope, relative, request, spent)` serves a folder page or a
file window in any of them.

```text
 sandbox.files s1                      roots::sandbox
   workspaces  <- Workspace records    never stat'ed: kind "mount"
   home, log   <- stat in the run folder

 sandbox.files s1                      roots::sandbox
   ... then "/" (role root) when the record's backend is namespaces

 sandbox.files s1 <path>               files::sandbox_page
   under <run folder>/tmp ? -- yes --> Scope::open: Base (the home's
        | no                           descriptor), mount table, Rules::beneath
        +-- host-process Sandbox ---------------------> InvalidInput
        +-- namespaces Sandbox --> root::page
              the record's process runs ? ---- no ----> Unavailable
              confined::root_of(pid): /proc/<pid> opened once; through it
                root, mountinfo, then stat (start time)
              not root, or may not read it -----------> Unsupported
              start time is not the record's ---------> Unavailable
              Scope::over: the root as "/", that namespace's mount table,
                View::InRoot, system names, hard links owned in the run folder

 machine.files m                       roots::machine
   the LayerFS folders that are there, by role (roots::folders), then "/"

 machine.files m <path>                machine::machine_page
   in a LayerFS folder ? -- yes --> Scope of that folder, Rules::beneath
        | no
        +-- Side::Home (host) ------------------------> InvalidInput
        +-- Side::Worker --> Scope of "/": View::InRoot, system names,
                             the world-readable rule, LayerFS folders apart

 answer (every view):
     every folder above the path:  system? a mount not entered? kept private?
                                   not for every user (world rule)? -> InvalidInput
     the entry, stat'ed in its folder:  system, mount, restricted, unreadable,
                                        log, link, FIFO, socket, device
                                                                 -> InvalidInput
                                        a file with several names
                                        its commands do not own  -> InvalidInput
     confined open, then stat of the descriptor (and, from "/", its mount id)
       folder -> page     file -> window
```

### The views

| | A Sandbox's home | A LayerFS folder of a machine | A machine's `/` | A Sandbox's root |
| --- | --- | --- | --- | --- |
| Call | `sandbox.files` | `machine.files` | `machine.files`, on a worker's side only | `sandbox.files`, a `namespaces` Sandbox, a path outside its home |
| Base | `<run folder>/tmp` | the outermost of `roots::folders` that holds the path: `/layerfs/data`, `state`, `bundle`, `image`; in a home `store`, `sandbox`, the run root `machine/<host>/run`, `log` | `/` | the root of the Sandbox's init process (`/proc/<pid>/root`), shown as `/` |
| Mount table | this process's | this process's | this process's | the init's (`/proc/<pid>/mountinfo`) |
| `View` | `Beneath` | `Beneath` | `InRoot` | `InRoot` |
| Links | none followed | none followed | on the way: resolved inside the base; at the end: never | as a machine's `/` |
| Mounts | none entered | none entered | entered when the type is one of `files::CROSSED` and the mount is not in a system folder, a LayerFS folder, a path LayerFS keeps, or a mount that is not entered itself | the same rule: the private `/tmp`, `/.layerfs/runtime`, the run folder and a bound Store's folder are entered, a Workspace's FUSE mount is not |
| `system` | — | — | `proc`, `sys`, `dev`, `run` of the base: never `stat`ed | `proc`, `sys`, `dev` |
| World rule (`Rules::world`) | — | — | a file must be `o+r`, a folder `o+rx`, and so must every folder above it; else `unreadable` | — (the Sandbox's files are its own) |
| LayerFS folders (`Rules::roles`) | — | listed with their `role` | listed with their `role`, never entered (`Rules::apart`) | — (the run folder and a Store's folder are at their own paths, judged by `roots::kept`) |
| Hard links | a regular file with several names must be the command uid's | any | any | the command uid's in the run folder (`Scope::owning`); any elsewhere: an image's files keep their links |
| Needs | the run folder | the folder | — | the Sandbox running, and a reader that is root |

A machine's LayerFS folders are the ones `roots::folders` names from the
side's root alone: the root, what is in it, and beside it `state`, `bundle`
and `image` (a worker's root is `/layerfs/data`, so these are `/layerfs/state`
and the others). In a home only four are named, and there is no `/` view:
a home is on its owner's own computer, which no machine's view lists whole. The roots answer opens each as a base
and `stat`s the descriptor, whatever filesystem it is on (Docker Desktop
binds `/layerfs/bundle` from a `fakeowner` share), and gives the data root
`free_bytes`.

An entry of a run folder's `workspaces/` is a `mount` in every view and is
never `stat`ed: the Workspaces are mounted in their Sandbox's own mount
namespace on Linux, and a stopped daemon's NFS mount on macOS would wait.
Its `workspace` id comes from the Workspace records: those under the side's
root, read once when a listing first meets such an entry, and those the
request carried (`workspaces: [{ id, path }]`, which the home sends for
Sandboxes whose daemon it reaches directly, as it keeps their records).

**Where an open ended.** A view that resolves links can end where the
names of its path do not say: a command can make `/tmp/k` a link to its run
folder's `config/`. So in an `InRoot` view every open (`Scope::opened`, of a
path and of each folder a walk enters) asks the kernel where the descriptor
is (`Opened::path`: its `/proc/self/fd` link, which for a root of another
mount namespace is the path inside that root), and when that is not the
path asked for, `roots::kept` judges it and every folder above it; what it
keeps is refused. The hard-link rule of a Sandbox's root goes by that path
too.

What a machine's `/` view relies on, and what it does not: a path in a
LayerFS folder is always read in that folder's own view. From `/`, the
same files can be named another way through a link (judged as above) or
through a second mount of the same disk (Lima mounts the data disk at
`/mnt/lima-<name>` and binds it at `/layerfs/data`); there the world rule
is what applies, with `store.sqlite*` restricted by name wherever it is.
The secrets LayerFS writes are not world-readable (`control.json`, the
worker's `secret`, a daemon's setup file).

**A Sandbox's root** (security rule 4). The daemon record of a
`namespaces` Sandbox names its init process: the first stage of
`layerfs-sandbox-helper`, whose root became the Sandbox's when the second
stage pivoted. `confined::root_of(pid)` opens `/proc/<pid>` once and
everything else through that descriptor: `root` (the one magic link an
inspection read follows), `mountinfo`, and last `stat` for the start time.
A descriptor of a `/proc` entry is of one process: the kernel answers
`ESRCH` through it once that process has ended, and a later process with
the same id has another entry, so the three are of the same process and the
last read says it still ran. `root::page` then requires that start time to
be the record's and the record's process to run (id, start time and boot,
`process::live`), else `Unavailable`. `pidfd_open` is not used: no
dependency wraps it without `unsafe` (rustix's is behind its `process`
feature, which this crate does not enable); the `/proc` descriptor gives
the same binding. The reader must be root, as the start of a namespaces
Sandbox is; on a `linux-host` home that is not, and where the kernel
refuses the read of the init's root, the view is `Unsupported` with the
reason. The roots answer offers `/` from the record alone.

### A folder page

```text
 depth 2, limit 4, cursor <base>/a/x          tree order:  a  a/x  a/y  b  c  c/q

 folder <base>   cursor names [a, x]
   enter a  (it is still a folder)            names of a after "x":   y
   then the names of <base> after "a":        b, c         keep limit - listed + 1
     b   stat, list
     c   stat, list, enter (depth 2): names of c:  q
       q  stat, list            page: a/y  b  c  c/q    next: <base>/c/q
```

- Names are in byte order and a folder's entries follow it, so the list is
  the tree a client draws. The cursor is the path of the last entry
  answered: a position in that order, not in a directory stream.
- A page reads the names of each folder on the cursor's path once
  (`readdir` on the folder's descriptor, no `stat`), keeping in a heap the
  smallest names it still has room for after the cursor's, and one more:
  the extra name is how a full page knows whether anything follows
  (`has_more`). It then `stat`s only the entries it answers. A page costs
  the names of those folders, at most `limit` `stat`s and `limit` names of
  memory. Reading a whole folder of n entries page by page reads its names
  n / `limit` times.
- A walk goes on from its cursor whatever changed: an entry that is gone is
  skipped, a cursor whose entry or folder is gone resumes after where it
  was, a new name after the cursor is listed and one before it is not.
- An entry is judged before it is `stat`ed: a system name of the base is
  `system`; a mount point named by the mount table that the view does not
  enter is `mount` with its `fs`; an entry of a run folder's `workspaces/`
  is `mount` with its `workspace`: none of the three is `stat`ed. Every
  other entry is told from its own `stat`: `restricted` where
  `roots::kept` keeps it, `mount` where it is in another mount than its
  folder and the table did not name it, `unreadable` under the world rule,
  else `file`, `folder`, `symlink` (with `target`) or `other`. A folder is
  entered only when it is a `folder`, and not a LayerFS folder read apart.

### What is kept private

`roots::kept(root, path)`
([`roots.rs`](../../../../control/crates/layerfs-inspect/src/files/roots.rs))
is the one place a path is judged against LayerFS's layout; a read asks it
of the path and of each folder above it, a listing of each entry.

| Kept | Paths | Shown as |
| --- | --- | --- |
| `Restricted` | `store.sqlite*` anywhere (a Store's database and its companions). In a run folder (`machine/<any folder>/run/<name>/`, not only the side's own run root): `config/` (the daemon's private key) and `overlay/` (the Workspaces' databases). Elsewhere under the side's root: `control.json`, `worker.json`, `lock`, `*.lock`, a Store's `open/` (leases). Beside a worker's root: `state/secret` | `restricted`: name, `size`, never entered or read |
| `Log` | a run folder's `daemon.log`; `log/worker.log` and `worker.1.log` | a `file` with `role: "log"`; read by `machine.log` and `sandbox.log` only |
| `Workspace` | what is under a run folder's `workspaces/` | `mount`, never `stat`ed |
| `Open` | everything else; a run folder's `tmp/` is its commands' own, so a `Cargo.lock` there is a file | by its `stat` |

### The platform split

Only three things differ by platform, in
[`layerfs-sandbox/src/backend/confined.rs`](../../../../control/crates/layerfs-sandbox/src/backend/confined.rs)
and its `os/` files: the confined open, the `stat`, and the mount table.

| | Linux | macOS |
| --- | --- | --- |
| Open under the base, `View::Beneath` (`Base::open_at`) | one `openat2` from the base's descriptor: `O_RDONLY \| O_NONBLOCK \| O_NOFOLLOW \| O_CLOEXEC \| O_NOCTTY` with `RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS \| RESOLVE_NO_MAGICLINKS \| RESOLVE_NO_XDEV`. A kernel before 5.6 is `Unsupported`; nothing else is tried | one `openat(O_NOFOLLOW \| O_NONBLOCK)` per name; each folder on the way on the base's device (`st_dev`); the last descriptor's path (`F_GETPATH`) under the base's |
| `View::InRoot` | the same call with `RESOLVE_IN_ROOT \| RESOLVE_NO_MAGICLINKS`: links resolve with the base as `/`. Then the mount id of what was opened (`statx` `STATX_MNT_ID`) must be one the view crosses (`Base::cross`: the base's own, and those `Scope::over` chose from the mount table), else `Refused::Mount`. A kernel before 5.8 names no mount id: `Unsupported` | opens as `Beneath`: macOS serves no machine's `/` in a product run (a worker runs in a Linux machine), so no link is resolved there |
| A folder a path leads through (`Base::enter`) | `O_DIRECTORY` in place of `O_NOFOLLOW`: in `InRoot` a link at the end resolves as one on the way | as `open_at`; it must be a folder |
| After the open | a `stat` of the descriptor: only a regular file or a folder is answered; a FIFO opens at once and is refused | the same |
| `stat` of an entry (`Opened::stat_of`) | `statx` (rustix) with `AT_SYMLINK_NOFOLLOW \| AT_NO_AUTOMOUNT \| AT_STATX_DONT_SYNC`, `AT_EMPTY_PATH` for a descriptor, asking `STATX_BASIC_STATS \| STATX_MNT_ID`. Every `stat` of the walker is this call | `fstatat(AT_SYMLINK_NOFOLLOW)`, `fstat` for a descriptor |
| Names of a folder (`Opened::names`) | `getdents64` (rustix `RawDir`): no `fstat` of the folder, which libc's `fdopendir` would make | `readdir` (`nix::dir::Dir`) |
| Two mounts (`Stat::apart_from`) | by mount id: a file of an overlay's lower layer or a btrfs subvolume has its own device and is no mount | by device |
| Another process's root (`confined::root_of`) | `/proc/<pid>` opened as a folder, then `root`, `mountinfo` and `stat` through it; root only | `Unsupported`: no Sandbox has a root of its own |
| Mount table (`confined::mounts`, `parse_mounts`) | `/proc/self/mountinfo`, read as a file: each mount's id, point and type | one run of `/sbin/mount` per files request with a path: fixed path, no environment but `LC_ALL=C`, no input, 500 ms, then killed and reaped (`darwin::mount_table`). A run that fails or is late answers no table, and a mount is then told by its device after a `stat` |

`getfsstat(MNT_NOWAIT)`, the system call under `mount`, is not called: no
dependency of `layerfs-sandbox-backend` wraps it without `unsafe`, and the
crate's one `unsafe` module stays `seccomp.rs`. `mount` reads the same
table and asks no mounted server. `rustix` is a Linux-only dependency of
the crate for `statx`, which `nix` does not wrap, and for `RawDir`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| An inspect request takes no lock or lease, opens no Store, asks no daemon, starts nothing and writes nothing, on the home and on the machine. | the module uses none of `registry`, `lease`, `open::hold`, `daemon::pool` or `route`'s host operations; [`logs.rs`](../../../../api/crates/layerfs-api/tests/inspect/logs.rs) and [`stats.rs`](../../../../api/crates/layerfs-api/tests/inspect/stats.rs) compare every file of both sides before and after |
| A Store file is sized by `lstat` of its path and never opened: closing a descriptor on it would drop its owner's locks. | `stats.rs` `store_bytes` |
| A kind made of many units answers what it has at the soft budget: `complete: false`, `reason: "deadline"`, the units not reached `null`. | `live::checked`, `stats::serve`; [`machine_stats.rs`](../../../../control/crates/layerfs-inspect/tests/standalone/machine_stats.rs) |
| It is not refused while an operation holds the machine or a Store. | `place::on_machine_read` skips the intent check `on_machine` makes |
| At most two run per process; a third is `Busy` at once. | `owner::admit` |
| A stuck or panicking request never stops the worker's other operations. | the helper thread; [`inspect.rs`](../../../../api/crates/layerfs-api/tests/worker_fault/inspect.rs) |
| Only `worker.log`, `worker.1.log` and `daemon.log` are read as logs; never `log/uncertain.jsonl`. | `log.rs` |
| A record cannot point a read outside the run root. | `run_folder` |
| A files read stays under its base: a `..` is refused before any system call; `Beneath` follows no link and enters no other mount; `InRoot` resolves links inside the base, never follows the last name, and ends only in a mount the view chose from the table, checked by the mount id of what was opened. | `confined::Base::open_at`, `enter`, `cross`; [`confined.rs`](../../../../control/crates/layerfs-sandbox/tests/local/confined.rs) |
| On Linux no `stat` is made but `statx` with `AT_STATX_DONT_SYNC \| AT_SYMLINK_NOFOLLOW \| AT_NO_AUTOMOUNT`. | `os/linux/confined.rs` `stat`, the walker's only one |
| From a machine's `/`, nothing is opened that not every user of the machine may read, and no system folder or LayerFS folder is entered. | `Rules::opens`, `Rules::system`, `Scope::enters` |
| On the host machine, nothing outside the home's LayerFS folders is read. | `machine::machine_page` |
| A Sandbox's root is read only while the process its record names runs, and only as that process's root. | `root::page`, `confined::root_of` |
| What LayerFS keeps private is not read under another name. | `Scope::opened`: the kernel's path of every descriptor an `InRoot` view opens, judged by `roots::kept` |
| Only regular files and folders are opened for an answer; a FIFO is never waited on. | every open is `O_NONBLOCK` and followed by a `stat` of the descriptor |
| A Workspace mount is never `stat`ed, opened or read through; nor is a mount point the mount table names. | `roots::sandbox`, `roots::kept`, `Scope::class` |
| A Store's database, an overlay database, a key, a lease or a lock is never opened. | `roots::kept`, asked for a path and for each folder above it |
| A file a command linked into its home from elsewhere is not read. | `Scope::own_link`: a regular file with several names not owned by the record's numeric command uid |
| `machine.log`, `sandbox.log`, `sandbox.files`, `machine.files` and `machine.stats` (with or without `--with space`) write no call-log line. | the dispatcher skips a `quiet` call: `home.get`, `home.log`, `home.clear_log`, `sandbox.files`, `sandbox.log`, `machine.stats`, `machine.files` and `machine.log` are ([`call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs), [Calls](../../api/calls/README.md#the-call-log)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Requests at a time | 2 per process (`PERMITS`), else `Busy` | `owner.rs` |
| Request time | soft 4 s, hard 6 s, then `Unavailable` | `owner.rs` |
| Caller's wait for a worker | connect 2 s, hello 2 s, answer 8 s (`place::READ`), then `Unavailable` | `place.rs`, `worker/client.rs` `Budget` |
| Window | 64 KiB of file per request (`CHUNK`); 200 lines by default, 1000 at most | `tail.rs` |
| `overlay_sizes` request | at most 1024 namespaces, else `InvalidInput` | `overlay.rs` |
| Folder page | 500 entries by default, 1000 at most; 1 level by default, 32 at most | `files.rs` `ENTRIES`, `MAX_ENTRIES`, `DEPTH`, `MAX_DEPTH` |
| File window | 256 KiB by default, 1 MiB at most; one `pread` loop of that size | `files.rs` `WINDOW`, `MAX_WINDOW` |
| Names | a name that is not UTF-8 is not listed | `Walk::names` |
| Path outside the view, link, special file, mount, system folder, kept private, not world-readable | `InvalidInput` | `files.rs` `answer`, `refusal` |
| `machine.files` request | at most 1024 Workspace records sent with it | `machine.rs`, `roots::MOST` |
| Path that is not there | `NotFound` | `files.rs` `refusal` |
| Linux before 5.6 | `Unsupported` (no `openat2`) | `confined.rs` `Refused::Unsupported` |
| A Sandbox's root, stopped Sandbox or a reused process id | `Unavailable`; its home is still read | `root.rs` |
| A Sandbox's root, reader not root or refused by the kernel | `Unsupported` with the reason | `confined::root_of` |
| Linux before 5.8, a view that crosses mounts | `Unsupported` (`statx` names no mount id) | `confined.rs` `Base::over` |
| macOS mount table | one run of `/sbin/mount`, 500 ms; none read: mounts told by device | `darwin::mount_table` |
| `machine_stats`, `machine_space`, `sandboxes_live` request | at most 4096 names in a list (`MOST`), each a valid Store or Sandbox name, else `InvalidInput` | `live.rs` |
| A worker of a build without a kind | `InvalidInput`, "no inspect kind"; `sandboxes_live` (`live.rs`) shows it as `live: null`, `live_reason: "invalid_input"` | `route.rs` `serve` |
| macOS memory | one run of `/usr/sbin/sysctl` per `machine_stats` request on a macOS side; a failed run is `memory_total` and `memory_used` `null`, `resources.reason` `memory_not_read` | `host.rs` |
| Stopped or unreached machine | `Unavailable`, naming `layerfs machine start`; nothing starts it | `place::on_machine_read` |
| A machine whose calls run in the caller's process | `machine.log` is `Unsupported`, `detail.reason` `no_worker` | `log.rs` |

## Adding a kind

One arm in `inspect::serve` (`route.rs`), taking `(side, request)` and, for a kind made of
many units of work, the `Budget`; and one `inspect::ask(home, machine,
"<kind>", args)` in its home-side function. A kind that can run out of
budget is to answer what it has, marked incomplete (`has_more: true` as a
folder page does, or `complete: false` with `reason: "deadline"` as
`machine_stats` and `sandboxes_live` do), rather than run to the hard
deadline.

A files view is a serving function that builds a `files::Scope` with its
`files::Rules` and calls `files::answer`, as `machine.rs` and `root.rs` do;
its roots answer goes in `roots.rs`, beside `roots::sandbox` and
`roots::machine`. A view of another mount namespace gives `Scope::over`
that namespace's table (`confined::parse_mounts`), as `root.rs` does. What
it keeps private is one line of `roots::kept`.

## Demonstration

| Test | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`tail.rs`](../../../../control/crates/layerfs-home/tests/log/tail.rs) | `cargo +1.85.1 test --locked -p layerfs-home --test log tail` | Unix | tail, forward and backward windows over a rotated pair; long lines in pieces; a held-back partial line; escaping; stale cursors |
| [`inspect_admission.rs`](../../../../control/crates/layerfs-inspect/tests/standalone/inspect_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test inspect_admission` | Unix | the third request is `Busy` and a stuck one is abandoned at the hard deadline; work past its soft budget answers what it has; a panic is `Failed` and frees its permit; a host-machine Sandbox's log from its recorded run folder; no run folder is `NoSuchSandbox`; overlay sizes by `lstat` |
| [`machine_stats.rs`](../../../../control/crates/layerfs-inspect/tests/standalone/machine_stats.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test machine_stats` | Unix | `machine_stats` and `sandboxes_live` on a scratch root: Store sizes by `lstat` with a sparse file, live and ended daemons, names past the soft budget answered `null` with `complete: false`, over 4096 names refused; cgroup usage from a scratch cgroup tree (with `memory.stat` `file` and `memory.events` `oom_kill 1`), `cgroup_v1`, `no_cgroup`, `not_supported`; `/proc/stat` counters and boot time from a stand-in text; each Sandbox's row on a worker root (live, no daemon record, unreadable record, not recorded) and past the soft budget (rows kept) |
| [`space.rs`](../../../../control/crates/layerfs-inspect/tests/reads/space.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test reads space` | Unix | `machine_space` on a scratch home: each part equal to the test's own blocks sum (Store folders, a run folder split by part, the image cache with a file under two names counted once, an unrecorded run folder, logs, records); a `workspaces/` folder nobody may list, a private root and the home's `runtime/` not counted; Store files nobody may open; nothing written; a folder that cannot be listed counted as `unreadable`; bad names refused |
| [`stats.rs`](../../../../api/crates/layerfs-api/tests/inspect/stats.rs) | `cargo +1.85.1 test --locked -p layerfs-api --features machine --test inspect stats` | Unix; this build's worker on a scratch root | `machine.stats` on the host-machine and worker routes with a Store's lock held and the machine held: nothing written, no lease, no call-log line; a stopped machine is `Unavailable` and its Sandboxes `machine_stopped`; `machine.stats --with space` through a worker with the machine and a Store held: split by part, `workspaces/` not counted, nothing written on either side, no lease |
| [`logs.rs`](../../../../api/crates/layerfs-api/tests/inspect/logs.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test inspect logs` | Unix; this build's worker on a scratch root | both calls on the host-machine and worker routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; rotation; a stopped machine; a forwarded Sandbox's run folder |
| [`inspect.rs`](../../../../api/crates/layerfs-api/tests/worker_fault/inspect.rs) | `cargo +1.85.1 test --locked -p layerfs-api --features worker-fault --test worker_fault inspect` | Unix | two stuck reads hold two helpers, the third is `Busy`, Store and Sandbox operations go on, descriptors come back; a panic is `Failed`; a silent worker is `Unavailable` at 8 s |
| [`logs.rs`](../../../../api/crates/layerfs-cli/tests/command_line/logs.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test command_line logs` | Unix | the call log's lines from the CLI, `layerfs home log` and `layerfs home clear-log`; `layerfs sandbox log` by cursor, usage errors and refusals; `layerfs sandbox files` and `layerfs machine files`: roots, a page continued, a window, refusals, no call-log line |
| [`confined.rs`](../../../../control/crates/layerfs-sandbox/tests/local/confined.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test local confined` | Linux and macOS | an open stays beneath its base and follows no link, out or in; a FIFO is refused at once; a device where one can be made; Linux: `/proc` is refused as another mount and listed in the table; a root view resolves links inside its base and ends only in the mounts allowed by id, with a mount table given as text; macOS: the table from one bounded run of `mount` |
| [`files.rs`](../../../../control/crates/layerfs-inspect/tests/reads/files.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test reads files` | Linux and macOS | 3000 entries paged by name through additions and removals; tree order at depths 1 to 4; byte order; a page cut by its budget; file windows, binary, a sparse 10 GiB file; every refusal; hard links by owner; the roots with Workspace mounts never touched; a stopped Sandbox's home |
| [`machine_files.rs`](../../../../control/crates/layerfs-inspect/tests/reads/machine_files.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test reads machine_files` | Linux and macOS | a machine's roots by role; a LayerFS folder with what LayerFS keeps and its Workspace mounts from the records; a leftover run folder under another machine folder kept private as a run folder; a home's folders only; pages and the soft budget; `/`: system folders, a closed file, a program read through a merged-`/usr` link; Linux: the world rule, links, and a mount that is not entered where the host has one |
| [`root_files.rs`](../../../../control/crates/layerfs-inspect/tests/reads/root_files.rs) | `cargo +1.85.1 test --locked -p layerfs-inspect --test reads root_files` | Linux as root (a child process stands in for a Sandbox's init, in this host's mount namespace) and macOS | the roots offer `/` for a namespaces Sandbox only; a stopped Sandbox reads its home and answers `Unavailable` for its root; Linux: the root's listing, the run folder and a Store's folder restricted at their own paths and through links, hard links, a mount that is not entered, a reused process id; macOS: `Unsupported` |
| [`files.rs`](../../../../api/crates/layerfs-api/tests/inspect/files.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test inspect files` | Unix; this build's worker on a scratch root | `machine.files` on the host-machine, worker and forwarded routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; a stopped machine |
| [`sandbox_files.rs`](../../../../api/crates/layerfs-api/tests/inspect/sandbox_files.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test inspect sandbox_files` | Unix; this build's worker on a scratch root | `sandbox.files` on the host-machine, worker and forwarded routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; a stopped machine; a namespaces Sandbox's roots and root through its machine's worker |

## Related

- [API: `machine.log`](../../api/sdk/machine.md#machinelog) and [`sandbox.log`](../../api/sdk/sandbox.md#sandboxlog).
- [Workers and reach](../machines/workers-and-reach.md).
- [Sandboxes](README.md).
