# #535/#536 final terminal pass, lane L2 (VM group A): apple-container, microsandbox, ssh

> **Status:** Lane receipt, 2026-11-03 08:48–09:08 UTC. Not release evidence; the lead merges it into
> `core/docs/issues/535/TERMINAL-PASS.md`. Product commit `35ed671b6c`; no product change. Redoes first-pass lane
> K2 (`../../fe6f85013b/K2/`) with its scripts (copied into `scripts/`, `item3/`, `item6/`, `parity/`, adapted).

- **Builds (read only):** frozen macOS debug builds `k-macos/` (`PRODUCT` = `35ed671b6c34…`; `SHA256SUMS` checked OK
  for `bin/layerfs`, `bin/layerfs-daemon`, `node/layerfs.node`, `python/src/layerfs/_native.abi3.so`): the CLI, the
  Node package (`node/test/verify/cells/cell.mjs`) and the Python package. Sealed Linux bundle
  `k2-bundle/2.0.0/linux-aarch64` (hash `764e4977…4f63`) through `LAYERFS_BUNDLE`. No Linux build of my own.
- **Machines**, made once in one shared home (a 0700 scratch folder, removed at the end), `setup.log`:
  `lfs535l2a` = `machine create … apple-container` (10.0 s; container CLI 1.6.1);
  `lfs535l2m` = `machine create … microsandbox` (28.9 s; msb 0.7.11; made at this commit, so with the
  `--net public,private,host` of 4b00569b55); `lfs535l2s` = `machine connect lfs535l2s 127.0.0.1 --port <lima ssh
  port> --identity <key> --known-hosts <file>` (22.2 s) on a throwaway Lima vz VM `lfs535l2s` (Ubuntu 24.04.5,
  kernel 6.8.0-142, 2 CPUs, 4 GiB, 20 GiB; `scripts/lfs535l2s.yaml` with the key redacted): K2's recipe (#533's
  `lfc-ssh` minus its Store disk): root key login with a generated ed25519 key, `user.max_user_namespaces = 0`, no
  PostgreSQL, no Docker, all guest ports unforwarded; host key from `ssh-keyscan`. At most two VMs at once:
  apple-container + microsandbox first, microsandbox removed (09:01) before the Lima VM booted (09:02).
  One failed `machine connect` (empty `--port`: a lane script defect, `ssh` read the rest of my script's stdin) is
  kept in `setup.log`, followed by the real connect.
- **PostgreSQL** (owner rule D12): one throwaway `postgres:17-bookworm` container `lfs535l2-pg` on the Mac,
  `0.0.0.0:55552`, generated password (scratch file only, deleted; `grep` of this folder finds it nowhere),
  `log_connections=on`. One database per platform, cell and URL form (a cell's Store data stays in the server):
  loopback `postgres://postgres@127.0.0.1:55552/<p>{cell}` (rewritten by `reach`), `<p>` = `a`, `m`, `s`; "remote"
  `postgres://postgres@192.168.2.123:55552/<p>{cell}`, `<p>` = `ar`, `mr`, `sr`. The remote URL is the same server
  through the Mac's LAN address (no second host exists; it stands in for a remote server). `pg-server-summary.txt`:
  connections per database; the only FATAL lines are the three the fix checks provoke.
- **Runner:** `scripts/cells.sh` (K2's: one cell per command, 115 s `perl alarm`, machine lock + cell-number lock);
  two queues at once on different machines (`scripts/q1.sh` apple-container, `q2.sh` microsandbox), then `q3.sh`
  (ssh) alone; queue logs `queue-q1..3.log`. Other CLI steps: `scripts/run.sh` (one `layerfs` call per invocation,
  UTC start, load1, wall ms, exit; password and paths scrubbed). One attempt per call; one changed second attempt
  after a diagnosis (`fixes/fixes2.mjs`), both kept.
- **Disk** (`tools/cleanup.py check`): 230.31 GB free at start, 178.26 GB mid-run, 177.41 GB at the end (other lanes
  run alongside; this lane leaves no VM, image or volume). **Load (1 min):** 3.89 at start, 7.88 at the end; 8–19
  during the cells (in each receipt).
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`. All Stores Disposable.

## Verdicts

| Item | Platform / Store | Result | First error / note |
| --- | --- | --- | --- |
| 9 machine power | apple-container `lfs535l2a` | **PASS** | `power-lfs535l2a.log`; `container list -a`: stopped, then running |
| 9 machine power | microsandbox `lfs535l2m` | **PASS** | `power-lfs535l2m.log`; `msb list`: stopped, then running |
| 9 machine power | ssh `lfs535l2s` | **PASS** | `power-lfs535l2s.log`; no `layerfs worker` on the host and no `ssh -L` on the Mac while stopped, both back after start |
| 9 → cells after start | the three platforms | **36/36 PASS** | item 1's sqlite run below ran right after each start (not run twice) |
| 1 sqlite | apple-container | **12/12 PASS** | pushed native → machine and pulled back in every cell |
| 1 sqlite | microsandbox | **12/12 PASS** | idem |
| 1 sqlite | ssh | **12/12 PASS** | idem |
| 2 PostgreSQL loopback | apple-container | **12/12 PASS** | URL rewritten by `reach` |
| 2 PostgreSQL remote | apple-container | **12/12 PASS** | |
| 2 PostgreSQL loopback | microsandbox | **12/12 PASS** | |
| 2 PostgreSQL remote | microsandbox | **12/12 PASS** | was 0/12 at fe6f85013b (K2 D1); fixed |
| 2 PostgreSQL remote | ssh | **12/12 PASS** | |
| 2 PostgreSQL loopback | ssh | expected refusal, recorded once | `ssh-pg-loop-C01.json`: `Unsupported the ssh machine lfs535l2s cannot reach the PostgreSQL Store's server: an ssh machine cannot reach the caller's loopback` |
| 3 many Stores (VM part) | microsandbox | **PASS** (first attempt) | `item3/microsandbox-many-stores.json`: 2 sqlite (pushed) + 2 PostgreSQL (one loopback, one remote) |
| 6 R1 (trace of a worker-routed Store call) | apple-container | **PASS** | `LAYERFS_TRACE`: exactly one `worker.connection` line per `file read`, `file stat`, `branch list`; a native call writes no file |
| 6 R2 (`workspace.exec` without a worker hop) | apple-container | **FAIL (expected, recorded spec difference)** | each exec is one worker connection (`op: sandbox.host`); the Sandbox daemon listens on guest loopback only |
| 7 binding parity | apple-container, sqlite, C01 C05 C09 C12 | **PASS** (12/12 identical) | CLI now forks **before** the push, as Node and Python; `store push` after a CLI `branch fork` works (K2 D3 fixed) |

Every PostgreSQL cell's Commit was found in its own database (`pg-commits-in-own-database.txt`: 55 timed Commit or
head ids found, 0 missing; the five C03 cells are UpToDate with no head).

## Fixes since fe6f85013b reached by this lane

| Fix | Result | Evidence |
| --- | --- | --- |
| Linux docker worker `--network host` | NOT_RUN | Linux only (lane K3) |
| `machine create` of a platform not on this host refused in the table's words | NOT_RUN | on macOS every platform is on the host (`connect/SUPPORT.json`, `support.rs:76-78` restrict apple-container, lima, firecracker to macOS); Linux/Windows only |
| `LAYERFS_TRACE` one `{at, event:"worker.connection", op, worker, sent, ms}` line per worker connection | **PASS** | `item6/roundtrips.log`: 1 line (`op: store.call`, `sent: true`) per `file read`/`file stat`/`branch list`; guest TCP counters agree (one inbound connection each, `roundtrips-decoded.txt`); the native `file read` creates no trace file; exec and the refused mount are one line each (`op: sandbox.host`) |
| missing PostgreSQL database `Unavailable` "the server refused the session …" | **PASS** | `fixes/fixes.json` `missing-database` (no Store folder left), `wrong-password` (same text); at the reach check, after `DROP DATABASE … WITH (FORCE)`: `machine lfs535l2a cannot reach PostgreSQL Store fxr's server at 192.168.64.1: the server refused the session: the database does not exist, or the user or password is wrong; <apple-container hint>` (`reach-refusal-server-refused-session`) |
| `workspace.mount(sandbox, "main")` refused `InvalidInput` | **PASS** | CLI (`item6/roundtrips.log` `fix-mount-main`) and Node (`fixes/fixes2.json`): "main is the project's published line, not a Branch, and is not mounted: fork a Branch …"; mounting `lane` works |
| `machine list`/`get` show the built-in docker's live state | **PASS** (stopped state only) | `fixes/docker-live-state.log`: status, get and list all `stopped` (this home started no docker worker; Docker Desktop runs); the running state was not exercised in this lane |
| the CLI closes its Stores before exit | **PASS** | `fixes/cli-wal-after-fork.log`: no `store.sqlite-wal` after CLI `store create`, `project init`, `branch fork`; `store push` then succeeds; same in `item6` and the CLI parity cells (fork before push) |
| `store.status`/`store.list` change nothing | **PASS** | `fixes/fixes.json` `store-reads-change-nothing`: every path, size and mtime under the home identical before/after `status` of a native, a PostgreSQL and a machine Store and `list`; no lock file; no `log/leases.jsonl` |
| `project.init` `{progress:{phase,done,total}}` on every route | **PASS** | native sqlite and PostgreSQL: `check, install, record` (null counts); worker route (Store on `lfs535l2a`): `check, send 0/12, send 12/12, install, record` |
| `project.remove` / `store.disconnect` / `backup.restore` `InUse` fill `detail.related` | **PASS** / **PASS** / NOT_RUN | `fixes2.json`: `project.remove` → `InUse` "Sandbox fxwsb of fxw/p is running", `related [{kind: sandbox, name: fxwsb}]` (attempt 1 could not make the Sandbox: defect L2-D1); `fixes.json`: `store.disconnect` → `InUse`, `related [{kind: sandbox, name: fxpsb}]`. `backup.restore` needs a bucket and this lane may not use `layerfs-minio` |
| push/pull stopped at install/record or retire (`Unresolved`, `retired: false`) | NOT_RUN | crash injection is lane K1's item 7 |
| retiring from a worker removes the Store's whole folder | **PASS** | `fixes/worker-store-folders-after-sqlite-cells.log`: after the sqlite cells no `c<nn>` sqlite folder is left on apple-container, microsandbox or ssh; `pwa`/`pws` gone after their pulls (`cleanup.log`). Folders left are PostgreSQL Stores' records: defect L2-D2 |
| K2 D1 microsandbox reaches private addresses (4b00569b55) | **PASS** | microsandbox pg-remote 12/12; item 3's remote PostgreSQL Store |
| K2 D2 reconnected PostgreSQL name never reuses the old database (2d607fc066) | **PASS** | `item3/stale-name.log`: `e1` → `i3pg5`, then within seconds `e1` → `i3pg6`: `mount e1 lane2` works, a Commit on it reads back from the caller, `mount e1 lane` (the old database's Branch) → `NoSuchBranch` |
| K2 D3 CLI push after CLI fork (9fd05b8367) | **PASS** | as "the CLI closes its Stores" above |

## Product defects found

**L2-D1. A project Inited into a sqlite Store that is already on a VM machine (the worker Init route) can never get a
Sandbox: `sandbox.create` is refused `NoSuchProject: no complete project manifest`.** Reproduced with one CLI call per
step (`fixes/worker-init-then-sandbox.log`): `store create fxi`, `store push fxi lfs535l2a`, `project init fxi/p`
(through the worker), `branch fork` (works), `sandbox create fxi/p fxisb lfs535l2a` → refused. The worker's
`store/fxi/project/p/record.json` holds the project's `manifest` after Init; the caller's record (written by Init's
`record` phase) holds only `{v, name, stack, created}`. `api/crates/layerfs-control/src/sandbox/route.rs:388-397`
treats the worker's record as stale when its `created` differs from (or lacks) the caller's and replaces it whole
with the caller's record, dropping `manifest`; `sandbox/files.rs:303-305` then refuses. After the refusal the
worker's record is the caller's (`{v, name, stack, created}`). The cells never hit it (they Init natively, then push),
nor did the first pass; the lines are unchanged since `810dc2e115`, so it is not a regression of the fixes. It blocks
the documented worker Init route (#536 H11) for every later Sandbox.

**L2-D2. `store.disconnect` leaves a PostgreSQL Store's records on every machine that ran its Sandboxes; a later sqlite
Store of the same name pushed there has `project.init` refused `Exists`.** A PostgreSQL Sandbox's call writes the
thin Store record and the project record under the worker's root (`sandbox/route.rs:376-397`,
`<root>/store/<name>/record.json` `{"kind":"postgresql"}` and `project/<p>/record.json`); `store/connect.rs:264-283`
(`disconnect`) removes only the caller's folder. After the cells, every machine kept `store/c01..c12` (and `e1`,
`i3p1`, `i3p2`, `fxp`, `fxr`) (`fixes/worker-store-folders-after-sqlite-cells.log`, `cleanup.log`). A sqlite Store
`c01` pushed to `lfs535l2a` then: `project init c01/p` → `Exists: project p exists in Store c01`
(`store/identity.rs:345-351`), `branch fork`/`file read` → `NoSuchProject` (`fixes/pg-worker-record-reuse.log`). Its
pull removed the folder (whole-folder retire). The PostgreSQL analogue of K1's item 9 note b.

## Item 9: machine power

Per machine (`scripts/power.sh`, K2's): a sqlite Store `pw<x>` created and pushed; `machine stop`; `machine list`;
`machine status`; `store status pw<x>`; `store get`; `machine list` and `machine status` again; an external check;
`machine start`; the external check; `machine status`; `store status`.

| Machine | stop (ms) | list / status after | `store status` while stopped | start (ms) | after start |
| --- | ---: | --- | --- | ---: | --- |
| `lfs535l2a` | 648 | stopped / stopped (`down_reason: the machine is stopped`), twice; `container list -a`: stopped | `state: down`, "machine lfs535l2a is not running; start it with `layerfs machine start lfs535l2a`" | 1,394 | running, Store `idle`; container running |
| `lfs535l2m` | 571 | stopped / stopped, twice; `msb list`: stopped | idem (`lfs535l2m`) | 1,015 | running; `msb list`: running; Store `idle`, `wal_bytes: 0` |
| `lfs535l2s` | 235 | stopped / stopped (`its worker forward is not running`), twice; no `layerfs worker` on the host, no `ssh -L` on the Mac | idem (`lfs535l2s`) | 1,572 | running; `/layerfs/bundle/layerfs worker` on the host, `ssh -N … -L 127.0.0.1:<port>:127.0.0.1:7070` on the Mac |

No call started a machine (D11); `store get` answered from the home while stopped.

## Item 3: many Stores on one VM (microsandbox)

`item3/many-stores.mjs` (K2's): two sqlite Stores created, Inited, forked and pushed to `lfs535l2m`; two PostgreSQL
Stores connected, `i3p1` through the loopback URL (rewritten by `reach`) and `i3p2` through the remote URL (possible
now, D1 fixed); four Sandboxes created at once; then all four at once: mount `lane`, exec (write a file, `sleep 2`),
Commit, unmount; then `file.read` of each new file from the Mac. **PASS** on the first attempt: four `Committed`, the
execs overlapped (latest start 09:00:00.659, earliest end 09:00:02.446), concurrent phase 2,793 ms, each file read
back. The D2 recheck (`item3/stale-name.sh`) ran on the same machine right after.

## Item 6: roundtrips (apple-container)

`item6/roundtrips.sh` (K2's, plus a native call and the `main` mount): each measured call with `LAYERFS_TRACE=<file>`,
and the guest's `/proc/net/snmp` TCP counters and `/proc/net/tcp` before and after (decoded in
`roundtrips-decoded.txt`). The Branch is forked before the push (D3 fixed).

| Call | trace lines | trace line | inbound guest connections | new guest socket |
| --- | ---: | --- | ---: | --- |
| `file read trc6/p main /hello.txt` (sqlite on `lfs535l2a`) | 1 | `op: store.call, worker: 127.0.0.1:58118, sent: true, ms: 11` | 1 | `…:7070 ← 192.168.64.1` |
| `file stat …` | 1 | `store.call`, 9 ms | 1 | `…:7070 ← 192.168.64.1` |
| `branch list trc6/p` | 1 | `store.call`, 8 ms | 1 | `…:7070 ← 192.168.64.1` |
| `file read trc6n/p main /hello.txt` (native Store) | 0 (no file) | — | 0 | none |
| `workspace mount trc6 main` (refused `InvalidInput`) | 1 | `sandbox.host`, 4 ms | 1 | `…:7070 ← 192.168.64.1` |
| `workspace exec <ws> 'echo exec on the VM; uname -n'` | 1 | `sandbox.host`, 42 ms | 1 | `…:7070 ← 192.168.64.1` |
| `workspace exec <ws> 'echo again'` | 1 | `sandbox.host`, 26 ms | 1 | `…:7070 ← 192.168.64.1` |

R1: one worker connection per worker-routed Store call, shown by the trace and matched by the guest counters. R2: an
exec is one hop to the worker (the Sandbox daemon listens on `127.0.0.1:42681` inside the guest only): the recorded
spec difference (536 DECISIONS: exec stays on the worker). `worker` in the trace is the Mac-side forwarded address.

## Item 7: binding parity (apple-container, sqlite)

Node = item 1's receipts; CLI and Python = `parity/cell.py` (K2's driver; same bodies, exported from the runner's own
`workloads.mjs` by `parity/export.mjs`, identical to K2's export; Stores `cli<nn>`/`py<nn>`). The CLI driver's K2
workaround (fork after the push) is removed: all three bindings fork, then push. `parity/compare.txt`:

| Cell | Node | CLI | Python | timed Commit | verifier |
| --- | --- | --- | --- | --- | --- |
| C01 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |
| C05 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |
| C09 | PASS | PASS | PASS | UpToDate ×3 (setup Committed ×3) | `VERIFY OK` ×3 |
| C12 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |

Same body sha256 and Sandbox kernel (`Linux 6.18.35`) for all three bindings.

## Every cell

Times are diagnostic (debug builds, one sample, shared host). `push`/`pull` are the cell's `store.push`/`store.pull`.

| Receipt | Verdict | timed exec ms | Commit ms (kind) | push ms | pull ms | load1 | started (UTC) | first error |
| --- | --- | ---: | --- | ---: | ---: | ---: | --- | --- |
| apple-container-pg-loop-C01 | PASS | 745.695 | 737.398 (Committed) | - | - | 10.0 | 08:53:29 |  |
| apple-container-pg-loop-C02 | PASS | 1436.645 | 829.826 (Committed) | - | - | 9.8 | 08:53:36 |  |
| apple-container-pg-loop-C03 | PASS | 1534.605 | 145.111 (UpToDate) | - | - | 10.3 | 08:53:45 |  |
| apple-container-pg-loop-C04 | PASS | 1565.03 | 1113.543 (Committed) | - | - | 10.4 | 08:53:51 |  |
| apple-container-pg-loop-C05 | PASS | 1304.772 | 1075.874 (Committed) | - | - | 10.5 | 08:53:58 |  |
| apple-container-pg-loop-C06 | PASS | 1221.417 | 5662.532 (Committed) | - | - | 10.6 | 08:54:05 |  |
| apple-container-pg-loop-C07 | PASS | 2124.878 | 10792.216 (Committed) | - | - | 10.0 | 08:54:21 |  |
| apple-container-pg-loop-C08 | PASS | 1275.31 | 5421.999 (Committed) | - | - | 9.6 | 08:54:39 |  |
| apple-container-pg-loop-C09 | PASS | 168.745 | 35.602 (UpToDate) | - | - | 12.1 | 08:54:49 |  |
| apple-container-pg-loop-C10 | PASS | 1172.332 | 222.203 (Committed) | - | - | 11.7 | 08:54:59 |  |
| apple-container-pg-loop-C11 | PASS | 225.984 | 1499.36 (UpToDate) | - | - | 10.5 | 08:55:11 |  |
| apple-container-pg-loop-C12 | PASS | 464.68 | 842.03 (Committed) | - | - | 10.6 | 08:55:22 |  |
| apple-container-pg-remote-C01 | PASS | 699.797 | 735.79 (Committed) | - | - | 10.6 | 08:55:26 |  |
| apple-container-pg-remote-C02 | PASS | 1602.049 | 829.022 (Committed) | - | - | 14.3 | 08:55:32 |  |
| apple-container-pg-remote-C03 | PASS | 1218.303 | 99.867 (UpToDate) | - | - | 14.0 | 08:55:39 |  |
| apple-container-pg-remote-C04 | PASS | 988.841 | 849.45 (Committed) | - | - | 13.3 | 08:55:43 |  |
| apple-container-pg-remote-C05 | PASS | 1304.325 | 898.646 (Committed) | - | - | 12.6 | 08:55:48 |  |
| apple-container-pg-remote-C06 | PASS | 1059.405 | 6036.784 (Committed) | - | - | 12.8 | 08:55:54 |  |
| apple-container-pg-remote-C07 | PASS | 3179.991 | 12297.369 (Committed) | - | - | 11.7 | 08:56:08 |  |
| apple-container-pg-remote-C08 | PASS | 1086.197 | 6424.308 (Committed) | - | - | 12.2 | 08:56:29 |  |
| apple-container-pg-remote-C09 | PASS | 213.072 | 14.942 (UpToDate) | - | - | 12.2 | 08:56:40 |  |
| apple-container-pg-remote-C10 | PASS | 2000.424 | 296.852 (Committed) | - | - | 11.8 | 08:56:51 |  |
| apple-container-pg-remote-C11 | PASS | 233.324 | 1669.663 (UpToDate) | - | - | 18.6 | 08:57:09 |  |
| apple-container-pg-remote-C12 | PASS | 639.7 | 493.525 (Committed) | - | - | 18.5 | 08:57:22 |  |
| apple-container-sqlite-C01 | PASS | 554.833 | 552.144 (Committed) | 91.916 | 159.439 | 16.3 | 08:51:50 |  |
| apple-container-sqlite-C02 | PASS | 1412.658 | 583.517 (Committed) | 90.993 | 158.405 | 14.6 | 08:51:59 |  |
| apple-container-sqlite-C03 | PASS | 1024.828 | 18.901 (UpToDate) | 86.71 | 59.516 | 14.3 | 08:52:04 |  |
| apple-container-sqlite-C04 | PASS | 917.786 | 591.553 (Committed) | 72.851 | 104.542 | 13.7 | 08:52:07 |  |
| apple-container-sqlite-C05 | PASS | 992.413 | 598.957 (Committed) | 85.095 | 115.97 | 13.7 | 08:52:10 |  |
| apple-container-sqlite-C06 | PASS | 1083.327 | 5302.562 (Committed) | 91.033 | 59.638 | 13.9 | 08:52:17 |  |
| apple-container-sqlite-C07 | PASS | 2102.677 | 10559.017 (Committed) | 85.09 | 59.207 | 12.2 | 08:52:35 |  |
| apple-container-sqlite-C08 | PASS | 1149.991 | 5468.812 (Committed) | 69.259 | 60.996 | 11.3 | 08:52:51 |  |
| apple-container-sqlite-C09 | PASS | 120.453 | 3.227 (UpToDate) | 70.191 | 61.749 | 10.6 | 08:52:59 |  |
| apple-container-sqlite-C10 | PASS | 1159.652 | 153.44 (Committed) | 70.779 | 59.775 | 10.0 | 08:53:07 |  |
| apple-container-sqlite-C11 | PASS | 200.804 | 1444.656 (UpToDate) | 66.257 | 60.691 | 9.6 | 08:53:18 |  |
| apple-container-sqlite-C12 | PASS | 464.944 | 261.106 (Committed) | 69.057 | 120.087 | 10.0 | 08:53:27 |  |
| microsandbox-pg-loop-C01 | PASS | 694.098 | 1598.726 (Committed) | - | - | 9.6 | 08:53:16 |  |
| microsandbox-pg-loop-C02 | PASS | 1270.283 | 1625.987 (Committed) | - | - | 10.1 | 08:53:24 |  |
| microsandbox-pg-loop-C03 | PASS | 1192.178 | 111.108 (UpToDate) | - | - | 9.8 | 08:53:32 |  |
| microsandbox-pg-loop-C04 | PASS | 1241.605 | 1030.727 (Committed) | - | - | 10.2 | 08:53:37 |  |
| microsandbox-pg-loop-C05 | PASS | 1362.884 | 1116.758 (Committed) | - | - | 10.3 | 08:53:43 |  |
| microsandbox-pg-loop-C06 | PASS | 1378.099 | 5961.058 (Committed) | - | - | 10.4 | 08:53:50 |  |
| microsandbox-pg-loop-C07 | PASS | 2492.853 | 11119.752 (Committed) | - | - | 10.6 | 08:54:02 |  |
| microsandbox-pg-loop-C08 | PASS | 1077.573 | 5439.324 (Committed) | - | - | 10.0 | 08:54:21 |  |
| microsandbox-pg-loop-C09 | PASS | 171.053 | 5.092 (UpToDate) | - | - | 9.6 | 08:54:31 |  |
| microsandbox-pg-loop-C10 | PASS | 1202.784 | 191.719 (Committed) | - | - | 9.6 | 08:54:41 |  |
| microsandbox-pg-loop-C11 | PASS | 214.83 | 1428.864 (UpToDate) | - | - | 11.7 | 08:54:53 |  |
| microsandbox-pg-loop-C12 | PASS | 368.465 | 504.569 (Committed) | - | - | 11.1 | 08:55:04 |  |
| microsandbox-pg-remote-C01 | PASS | 858.071 | 1456.295 (Committed) | - | - | 10.5 | 08:55:08 |  |
| microsandbox-pg-remote-C02 | PASS | 1183.716 | 1465.711 (Committed) | - | - | 11.0 | 08:55:15 |  |
| microsandbox-pg-remote-C03 | PASS | 1147.502 | 85.193 (UpToDate) | - | - | 10.6 | 08:55:23 |  |
| microsandbox-pg-remote-C04 | PASS | 979.7 | 882.264 (Committed) | - | - | 10.0 | 08:55:28 |  |
| microsandbox-pg-remote-C05 | PASS | 1315.375 | 888.129 (Committed) | - | - | 14.3 | 08:55:33 |  |
| microsandbox-pg-remote-C06 | PASS | 1097.151 | 5482.12 (Committed) | - | - | 14.0 | 08:55:39 |  |
| microsandbox-pg-remote-C07 | PASS | 2222.92 | 11312.797 (Committed) | - | - | 12.6 | 08:55:49 |  |
| microsandbox-pg-remote-C08 | PASS | 1274.515 | 6594.17 (Committed) | - | - | 11.7 | 08:56:08 |  |
| microsandbox-pg-remote-C09 | PASS | 158.83 | 7.915 (UpToDate) | - | - | 12.2 | 08:56:20 |  |
| microsandbox-pg-remote-C10 | PASS | 1465.362 | 228.003 (Committed) | - | - | 12.2 | 08:56:31 |  |
| microsandbox-pg-remote-C11 | PASS | 516.902 | 1999.65 (UpToDate) | - | - | 11.8 | 08:56:45 |  |
| microsandbox-pg-remote-C12 | PASS | 815.199 | 874.403 (Committed) | - | - | 15.5 | 08:57:01 |  |
| microsandbox-sqlite-C01 | PASS | 645.575 | 625.884 (Committed) | 68.89 | 164.502 | 18.5 | 08:51:40 |  |
| microsandbox-sqlite-C02 | PASS | 1361.062 | 601.871 (Committed) | 58.607 | 155.308 | 16.3 | 08:51:50 |  |
| microsandbox-sqlite-C03 | PASS | 1136.255 | 70.238 (UpToDate) | 56.456 | 58.993 | 15.5 | 08:51:56 |  |
| microsandbox-sqlite-C04 | PASS | 1237.987 | 681.861 (Committed) | 60.902 | 104.976 | 14.6 | 08:51:59 |  |
| microsandbox-sqlite-C05 | PASS | 970.586 | 653.254 (Committed) | 58.712 | 104.046 | 14.3 | 08:52:04 |  |
| microsandbox-sqlite-C06 | PASS | 1140.051 | 5535.371 (Committed) | 58.248 | 60.136 | 13.7 | 08:52:08 |  |
| microsandbox-sqlite-C07 | PASS | 2233.423 | 10575.593 (Committed) | 64.073 | 58.614 | 13.9 | 08:52:17 |  |
| microsandbox-sqlite-C08 | PASS | 1135.704 | 5315.928 (Committed) | 60.737 | 61.354 | 12.2 | 08:52:35 |  |
| microsandbox-sqlite-C09 | PASS | 186.56 | 47.832 (UpToDate) | 57.465 | 58.284 | 11.7 | 08:52:44 |  |
| microsandbox-sqlite-C10 | PASS | 1290.906 | 214.465 (Committed) | 58.393 | 72.744 | 10.9 | 08:52:52 |  |
| microsandbox-sqlite-C11 | PASS | 276.395 | 1493.68 (UpToDate) | 70.84 | 73.397 | 10.4 | 08:53:04 |  |
| microsandbox-sqlite-C12 | PASS | 437.496 | 290.494 (Committed) | 69.786 | 103.724 | 9.8 | 08:53:14 |  |
| parity/apple-container-sqlite-C01-cli | PASS | 2221.107 | 1475.281 (Committed) | 247.383 | 426.146 | 16.8 | 09:01:10 |  |
| parity/apple-container-sqlite-C01-python | PASS | 1167.754 | 837.941 (Committed) | 110.054 | 241.753 | 13.7 | 09:01:50 |  |
| parity/apple-container-sqlite-C05-cli | PASS | 3554.121 | 1476.27 (Committed) | 164.429 | 340.301 | 15.6 | 09:01:22 |  |
| parity/apple-container-sqlite-C05-python | PASS | 1549.381 | 1038.091 (Committed) | 109.175 | 143.259 | 13.1 | 09:01:56 |  |
| parity/apple-container-sqlite-C09-cli | PASS | 173.449 | 15.132 (UpToDate) | 213.16 | 86.629 | 15.0 | 09:01:33 |  |
| parity/apple-container-sqlite-C09-python | PASS | 157.423 | 3.667 (UpToDate) | 93.227 | 79.275 | 11.9 | 09:02:02 |  |
| parity/apple-container-sqlite-C12-cli | PASS | 493.47 | 354.649 (Committed) | 132.346 | 137.909 | 13.7 | 09:01:47 |  |
| parity/apple-container-sqlite-C12-python | PASS | 438.565 | 293.622 (Committed) | 102.29 | 149.75 | 11.1 | 09:02:13 |  |
| ssh-pg-loop-C01 | FAIL | - | - | - | - | 8.6 | 09:07:18 | sandbox.create: Unsupported the ssh machine lfs535l2s cannot reach the PostgreSQL Store's server: an ssh machine cannot reach the caller's l |
| ssh-pg-remote-C01 | PASS | 984.809 | 881.351 (Committed) | - | - | 10.9 | 09:05:17 |  |
| ssh-pg-remote-C02 | PASS | 2629.216 | 1203.994 (Committed) | - | - | 11.5 | 09:05:26 |  |
| ssh-pg-remote-C03 | PASS | 1386.052 | 102.135 (UpToDate) | - | - | 12.4 | 09:05:39 |  |
| ssh-pg-remote-C04 | PASS | 993.751 | 891.096 (Committed) | - | - | 13.3 | 09:05:46 |  |
| ssh-pg-remote-C05 | PASS | 941.864 | 883.379 (Committed) | - | - | 13.0 | 09:05:52 |  |
| ssh-pg-remote-C06 | PASS | 1080.766 | 5410.598 (Committed) | - | - | 12.6 | 09:05:58 |  |
| ssh-pg-remote-C07 | PASS | 2255.77 | 10728.142 (Committed) | - | - | 12.0 | 09:06:08 |  |
| ssh-pg-remote-C08 | PASS | 1086.726 | 5524.254 (Committed) | - | - | 11.5 | 09:06:27 |  |
| ssh-pg-remote-C09 | PASS | 135.368 | 6.264 (UpToDate) | - | - | 10.6 | 09:06:39 |  |
| ssh-pg-remote-C10 | PASS | 1186.387 | 205.343 (Committed) | - | - | 9.8 | 09:06:49 |  |
| ssh-pg-remote-C11 | PASS | 197.389 | 1461.721 (UpToDate) | - | - | 9.0 | 09:07:02 |  |
| ssh-pg-remote-C12 | PASS | 354.552 | 487.852 (Committed) | - | - | 8.7 | 09:07:14 |  |
| ssh-sqlite-C01 | PASS | 626.244 | 545.729 (Committed) | 62.717 | 159.485 | 8.6 | 09:03:36 |  |
| ssh-sqlite-C02 | PASS | 1275.985 | 563.907 (Committed) | 60.188 | 163.156 | 8.2 | 09:03:45 |  |
| ssh-sqlite-C03 | PASS | 1117.391 | 17.833 (UpToDate) | 60.828 | 68.922 | 8.0 | 09:03:51 |  |
| ssh-sqlite-C04 | PASS | 1024.192 | 599.485 (Committed) | 61.039 | 105.299 | 8.1 | 09:03:54 |  |
| ssh-sqlite-C05 | PASS | 1111.981 | 601.809 (Committed) | 61.073 | 108.05 | 8.3 | 09:03:59 |  |
| ssh-sqlite-C06 | PASS | 1080.253 | 5576.917 (Committed) | 67.133 | 72.116 | 8.4 | 09:04:03 |  |
| ssh-sqlite-C07 | PASS | 2688.995 | 10983.043 (Committed) | 68.763 | 80.054 | 9.0 | 09:04:13 |  |
| ssh-sqlite-C08 | PASS | 1052.066 | 5302.297 (Committed) | 62.452 | 68.985 | 10.3 | 09:04:32 |  |
| ssh-sqlite-C09 | PASS | 107.525 | 1.746 (UpToDate) | 61.997 | 68.042 | 9.6 | 09:04:42 |  |
| ssh-sqlite-C10 | PASS | 1252.707 | 165.96 (Committed) | 61.176 | 104.177 | 9.0 | 09:04:50 |  |
| ssh-sqlite-C11 | PASS | 283.126 | 1541.013 (UpToDate) | 61.855 | 77.8 | 9.2 | 09:05:02 |  |
| ssh-sqlite-C12 | PASS | 574.243 | 256.934 (Committed) | 66.432 | 109.325 | 10.1 | 09:05:14 |  |

`ssh-pg-loop-C01` is the expected refusal (not a failure of the item).

## Resources

Created and removed: machines `lfs535l2a` (apple-container) and `lfs535l2m` (microsandbox) by `machine remove`
(afterwards `container list -a`, `container volume list`, `msb list`, `msb volume list` show nothing of this lane);
`lfs535l2s` by `machine disconnect`, then the Lima VM `lfs535l2s` by `limactl delete -f`; the PostgreSQL container
`lfs535l2-pg` (`docker rm -fv`, with all its databases); the shared home; the generated password, ssh key,
known_hosts, keyed Lima config and trace files in scratch. Left: nothing but this folder and non-secret scratch
scripts. No `docker` worker was started by this home. `cleanup.log` holds the commands and the worker folder
listings taken before removal.
