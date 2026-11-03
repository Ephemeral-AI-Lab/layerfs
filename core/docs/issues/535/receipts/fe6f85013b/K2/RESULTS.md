# #535/#536 terminal pass, lane K2 (VM group A): apple-container, microsandbox, ssh

> **Status:** Lane receipt, 2026-11-03 07:17–07:40 UTC. Not release evidence; the lead merges it into
> `core/docs/issues/535/TERMINAL-PASS.md`. Product commit `fe6f85013b`; no product change.

- **Builds (read only):** frozen macOS debug builds `k-macos/` (`PRODUCT` = `fe6f85013be2…`): `bin/layerfs`, the Node
  package (`node/test/verify/cells/cell.mjs`, unchanged) and the Python package (`python/src`). Sealed Linux bundle
  `k-bundle/2.0.0/linux-aarch64` (hash `a1f0d886…3e927`) through `LAYERFS_BUNDLE`. Node v24.21.0, Python 3.14.3.
- **Machines**, made once in one shared home (`LFS533_HOME`, a 0700 scratch folder, removed at the end), as #533 did
  (`setup.log`): `lfs535k2a` = `machine create … apple-container` (10.9 s; container CLI 1.6.1);
  `lfs535k2m` = `machine create … microsandbox` (31.8 s; msb 0.7.11); `lfs535k2s` = `machine connect lfs535k2s
  127.0.0.1 --port 60520 --identity <key> --known-hosts <file>` (16.3 s) on a throwaway Lima vz VM `lfs535k2s`
  (Ubuntu 24.04.5, kernel 6.8.0-142, 2 CPUs, 4 GiB, 20 GiB ext4 root; `scripts/lfs535k2s.yaml`): root key login
  with a generated ed25519 key, `ssh_deletekeys: false`, `user.max_user_namespaces = 0`, no PostgreSQL, no Docker,
  all guest ports unforwarded (the product's own `ssh -L` reaches the worker). Its host key was taken with
  `ssh-keyscan`. #533's `lfc-ssh` recipe, minus its extra Store disk.
- **PostgreSQL** (owner rule D12): one throwaway `postgres:17-bookworm` container `lfs535k2-pg` on the Mac,
  `0.0.0.0:55542`, generated password (scratch file only; no receipt holds it: `grep` of this folder finds none),
  `log_connections=on`. One database per platform, cell and URL form, because a cell's Store data stays in the server:
  loopback `postgres://postgres@127.0.0.1:55542/<p>{cell}` (rewritten by `reach`) with `<p>` = `a` (apple-container),
  `m` (microsandbox), `s` (ssh refusal); "remote" `postgres://postgres@192.168.2.123:55542/<p>{cell}` with `<p>` =
  `ar`, `mr`, `sr`. The remote URL is the same server through the Mac's LAN address (no second host exists, so it
  stands in for a remote server). Server summary: `pg-server.log` is not kept; `pg-server-summary.txt` (no ERROR or
  FATAL line; Docker's proxy shows every peer as 192.168.65.1).
- **Runner:** one cell per command, `scripts/cells.sh` (115 s limit, `perl alarm`, as #533); two queues at once on
  different machines (`scripts/q1.sh`, `q2.sh`), each cell holding a machine lock and a cell-number lock (the shared
  home names every cell's Store `c<nn>`), so never two cells on one machine or one Store name. Other CLI steps:
  `scripts/run.sh` (one `layerfs` call per invocation, UTC start, load1, wall ms, exit; password and paths scrubbed).
  One attempt per call; a changed second attempt only after a diagnosis, all attempts kept.
- **Disk** (`tools/cleanup.py check`): 259.38 GB free at start, 186.06 GB mid-run (other lanes), 213.61 GB at the end.
  **Load (1 min):** 4.47 at start, 5.76 at the end; 6–15 during the cells (in each receipt).
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`. All Stores Disposable.

## Verdicts

| Item | Platform / Store | Result | First error / note |
| --- | --- | --- | --- |
| 9 machine power | apple-container `lfs535k2a` | **PASS** | `power-lfs535k2a.log` |
| 9 machine power | microsandbox `lfs535k2m` | **PASS** | `power-lfs535k2m.log` (msb list: stopped, then running) |
| 9 machine power | ssh `lfs535k2s` | **PASS** | `power-lfs535k2s.log` (no worker on the host, no forward, then both back) |
| 9 → 1 cells after start | the three platforms | **36/36 PASS** | item 1's run below *is* the after-start run (not run twice) |
| 1 sqlite | apple-container | **12/12 PASS** | pushed native → machine and pulled back in every cell |
| 1 sqlite | microsandbox | **12/12 PASS** | idem |
| 1 sqlite | ssh | **12/12 PASS** | idem |
| 2 PostgreSQL loopback | apple-container | **12/12 PASS** | URL rewritten by `reach` |
| 2 PostgreSQL remote | apple-container | **12/12 PASS** | |
| 2 PostgreSQL loopback | microsandbox | **12/12 PASS** | |
| 2 PostgreSQL remote | microsandbox | **0/12 FAIL** | `sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c01's server at 192.168.2.123: no connection to the server was made …` — **product defect D1** below |
| 2 PostgreSQL remote | ssh | **12/12 PASS** | |
| 2 PostgreSQL loopback | ssh | expected refusal, recorded once | `ssh-pg-loop-C01.json`: `Unsupported the ssh machine lfs535k2s cannot reach the PostgreSQL Store's server: an ssh machine cannot reach the caller's loopback` |
| 3 many Stores (VM part) | microsandbox | **PASS** (attempt 3) | attempts 1–2 failed: a script defect, then **product defect D2**; see "Item 3" |
| 6 R1 (trace of a worker-routed Store call) | apple-container | **FAIL as specified / substance holds** | `LAYERFS_TRACE` is read by nothing in this build (no trace file is written); substitute observation: exactly one connection to the worker per `file.read`/`file.stat` |
| 6 R2 (`workspace.exec` without a worker hop) | apple-container | **FAIL (expected, recorded spec difference)** | each exec is one connection to the guest worker `:7070`; the Sandbox daemon listens on guest loopback only |
| 7 binding parity | apple-container, sqlite, C01 C05 C09 C12 | **PASS** (12/12 identical) with one CLI deviation | the CLI cannot `store push` after a CLI `branch fork`: **product defect D3**; the CLI driver forks after the push |

Every PostgreSQL cell's Commit was checked in its own database (`pg-commits-in-own-database.txt`: 44 timed Commit
ids found, the four C03 cells are UpToDate with no head), so no PostgreSQL PASS was credited by a stale service (D2).
Same-name PostgreSQL cells on one worker (apple-container `c<nn>` loopback then remote) were 102–180 s apart, longer
than the worker's 60 s idle close.

## Product defects found

**D1. microsandbox machines cannot reach a PostgreSQL server on a private (RFC 1918) address.**
`connect/crates/layerfs-machine/src/platforms/microsandbox/create.rs:40-46` creates the VM with `--net public,host`:
`public` is the internet, `host` is the host alias (`host.microsandbox.internal` → 172.16.0.5). A server on the LAN
(192.168.2.123) matches msb's `private` profile, which is not granted, so msb refuses the connection
(`msb-pg-remote-diagnosis.log`: from the machine's guest, `/dev/tcp/192.168.2.123/55542` → `Connection refused`;
the host alias → open). Control: a throwaway msb sandbox made by hand with `--net public,private,host` (same image)
opens 192.168.2.123:55542. The reach check reports it correctly (`Unavailable … no connection to the server was
made`), so nothing is half-created; but the §7.3 table enables microsandbox PostgreSQL Sandboxes, and a remote server
on a private network (the common company case) can never be used. msb cannot widen a policy after create, so every
existing microsandbox machine would need re-creating after a fix.

**D2. A machine's worker keeps a PostgreSQL Store's old service open after `store.disconnect`, keyed by the Store's
name: a Store reconnected under the same name to another database within 60 s reads the old database.**
`api/crates/layerfs-control/src/store/call.rs:87-91` keys the open service by `<home>/store/<name>/postgresql`, a
path that does not depend on the URL; `store/open.rs:63-70` reuses an entry for the same path without comparing its
URL; `store/connect.rs:287` (disconnect) closes the entry only in the caller's process; the worker closes it only
after `IDLE` = 60 s unused (`open.rs:17`, reaper `open.rs:84-89`). Reproduced with one CLI process per call
(`item3/diag-stale-name.log`): `e1` → database `i3pg9`, fork `lane`, Sandbox on `lfs535k2m`, mount, close,
disconnect; `e1` → `i3pg10`, fork `lane2`, Sandbox, `workspace mount e1 lane2` → `NoSuchBranch: Branch lane2 does
not exist`; `workspace mount e1 lane` → the Branch id is found in `i3pg9` and the daemon (on `i3pg10`) refuses
`bind Snapshot … Branch 110a4b… missing`. `pg_stat_activity` still showed two sessions on `i3pg9` from the worker
after the close and disconnect. A Commit in that window would be bound to the wrong database's Branch id. Item 3
attempt 2 hit it (its PostgreSQL Store names were reused 30 s after attempt 1).

**D3. A `layerfs` CLI process exits with its Stores open, leaving a sqlite write-ahead log; `store push` then refuses.**
`api/crates/layerfs-cli/src/run.rs:77-118` returns from the call without closing the Stores `store/open.rs`'s static
`OPEN` cache holds (statics are not dropped at exit). After `layerfs branch fork w2/p lane main`, `store.sqlite-wal`
is 12,392 bytes; `layerfs store push w2 lfs535k2a` then refuses `Failed: store push, snapshot: … has an unfinished
write-ahead log; open the Store once on its machine` (`store/transfer/steps.rs:103-112`), and the advice does not help
(a CLI `file read` leaves it unchanged; `cli-wal-after-fork.log`, `-2.log`). Through the CLI alone a native sqlite
Store cannot be pushed after any CLI write. The Node and Python clients are not affected within one process (the push
closes the Store in the same process first).

**Observation (not counted as a defect):** `workspace mount <sandbox> main` answers `NoSuchBranch: Branch main does
not exist` although `main` exists (`file read … main …` works in the same run; `item4/roundtrips-attempt1.log`).
Mounting a forked Branch works; the message names the wrong cause.

## Item 9: machine power

Per machine (`scripts/power.sh`): a sqlite Store `pw<x>` created and pushed; `machine stop`; `machine list`;
`machine status`; `store status pw<x>`; `store get`; `machine list` and `machine status` again; `machine start`;
`machine status`; `store status`.

| Machine | stop (ms) | list / status after | `store status` while stopped | start (ms) | after start |
| --- | ---: | --- | --- | ---: | --- |
| `lfs535k2a` | 941 | stopped / stopped (`down_reason: the machine is stopped`), twice | `state: down`, "machine lfs535k2a is not running; start it with `layerfs machine start lfs535k2a`" | 1,263 | running, Store `idle`, file present |
| `lfs535k2m` | 302 | stopped / stopped, twice; `msb list`: `stopped` | idem (`lfs535k2m`) | 1,002 | running; `msb list`: `running` |
| `lfs535k2s` | 259 | stopped / stopped (`its worker forward is not running`), twice; on the host no `layerfs worker`, on the Mac no `ssh -L` | idem (`lfs535k2s`) | 1,560 | running; worker pid on the host and the `ssh -L … 127.0.0.1:7070` forward back |

No call started a machine (D11). The ssh external check's `pgrep` lines also match the checking shell itself; the
lines naming `/layerfs/bundle/layerfs worker` and `/usr/bin/ssh -N … -L` are the evidence. apple-container had no
external check (it ran first); its product answers agree before and after. The 36 sqlite cells of item 1 ran right
after these starts (07:23–07:33 UTC), so they are item 9's "12 cells after start" on each platform.

## Item 3: many Stores on one VM (microsandbox)

`item3/many-stores.mjs` (Node, frozen package): two sqlite Stores created, Inited, forked and pushed to `lfs535k2m`;
two PostgreSQL Stores connected (both through the loopback URL, rewritten by `reach`: the remote URL cannot reach
microsandbox, D1); four Sandboxes created at once; then all four at once: mount `lane`, exec (write a file, `sleep 2`),
Commit, unmount; then `file.read` of each new file from the caller (the Mac).

| Attempt | Result | Cause |
| --- | --- | --- |
| 1 (`microsandbox-many-stores-attempt1-script-defect.json`) | FAIL | script defect: mounted `main` (`NoSuchBranch`) |
| 2 (`cleanup-attempt2.log`; receipt overwritten by attempt 3, steps in this file's D2 section) | FAIL | sqlite mounts OK; both PostgreSQL mounts `NoSuchBranch: Branch lane does not exist` — D2 (Store names `i3p1`/`i3p2` reused 30 s after attempt 1, new databases); two sqlite Workspaces left mounted by the script, unmounted by hand |
| 3 (`microsandbox-many-stores.json`, Store names `j3*`, databases `i3pg11`/`i3pg12`) | **PASS** | four Commits (`Committed`), the four execs overlapped (latest start 07:34:26.486, earliest end 07:34:28.444), concurrent phase 2,342 ms, each file read back from the Mac |

Diagnosis runs kept: `item3/diag-two-pg.log` (two PostgreSQL Stores in one process read their own Branches,
sequentially and at once), `item3/diag-mount.log` (fresh names: mounts one at a time and at once both work),
`item3/diag-stale-name.log` (D2 reproduced).

Note on attempt 2's receipt: it was written to `item3/microsandbox-many-stores.json` and then replaced by attempt 3's
(same path). Its steps, read before the rerun: Stores and Sandboxes all OK; `i3s1.mount`, `i3s2.mount` OK;
`i3p1.mount`, `i3p2.mount` refused `NoSuchBranch` (Branch lane does not exist); closes of the sqlite Sandboxes refused
`MountsActive`, pulls and removes `InUse` (the script did not unmount on failure; fixed in attempt 3).

## Item 6: roundtrips (apple-container)

How tracing is configured in this build (`item4/trace-config.txt`): no Rust, JavaScript or Python source reads
`LAYERFS_TRACE`; the frozen CLI, `layerfs.node` and `_native.abi3.so` hold no such string; `common/trace.rs`'s
`take_scope`/`run` have no caller (536 DECISIONS step D, "nothing sets the trace scope"). Each measured call ran with
`LAYERFS_TRACE=<file>` and no file was created. So neither R1 nor R2 can be shown by a trace at `fe6f85013b`.

Substitute observation (`item4/roundtrips.sh`, `roundtrips.log`, decoded in `roundtrips-decoded.txt`): the guest's
`/proc/net/snmp` TCP counters and `/proc/net/tcp` read by `container exec` just before and after each CLI call.

| Call | inbound connections (dPassiveOpens − dActiveOpens) | new socket | LAYERFS_TRACE file |
| --- | ---: | --- | --- |
| `file read trc3/p main /hello.txt` (sqlite pushed to `lfs535k2a`) | 1 | `192.168.64.14:7070 ← 192.168.64.1` | not created |
| `file stat …` | 1 | `…:7070 ← 192.168.64.1` | not created |
| `workspace exec <ws> 'echo exec on the VM; uname -n'` | 1 | `…:7070 ← 192.168.64.1` (the worker) | not created |
| `workspace exec <ws> 'echo again'` | 1 | `…:7070 ← 192.168.64.1` | not created |

R1 in substance: one worker connection per worker-routed Store call. R2: `workspace.exec` is one hop to the worker
(the Sandbox daemon listens on `127.0.0.1:42557` inside the guest only) — the recorded spec difference (536
DECISIONS: exec stays on the worker). Attempts 1 and 2 of the script (`roundtrips-attempt1.log`, `-attempt2.log`):
R1 rows identical; the Sandbox part failed on the script (mounting `main`; then forking before the push, which D3
refuses).

## Item 7: binding parity (apple-container, sqlite)

Node = item 1's receipts; CLI and Python = `parity/cell.py` (same bodies, exported from the runner's own
`workloads.mjs` by `parity/export.mjs`; same steps and verdict rule; Stores `cli<nn>`/`py<nn>`). The CLI driver forks
after the push (D3); everything else is in Node's order. `parity/compare.txt`:

| Cell | Node | CLI | Python | timed Commit | verifier |
| --- | --- | --- | --- | --- | --- |
| C01 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |
| C05 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |
| C09 | PASS | PASS | PASS | UpToDate ×3 (setup Committed ×3) | `VERIFY OK` ×3 |
| C12 | PASS | PASS | PASS | Committed ×3 | `VERIFY OK` ×3 |

Same body sha256, Sandbox kernel (`Linux 6.18.35`) and uid (`501:20`) for all three bindings.

## Every cell

Times are diagnostic (debug builds, one sample, shared host). `push`/`pull` are the cell's `store.push`/`store.pull`.

| Receipt | Verdict | timed exec ms | Commit ms (kind) | push ms | pull ms | load1 | started (UTC) | first error |
| --- | --- | ---: | --- | ---: | ---: | ---: | --- | --- |
| apple-container-pg-loop-C01 | PASS | 1025.504 | 910.006 (Committed) | - | - | 15.0 | 07:25:53 |  |
| apple-container-pg-loop-C02 | PASS | 1300.69 | 740.974 (Committed) | - | - | 14.6 | 07:26:00 |  |
| apple-container-pg-loop-C03 | PASS | 1207.99 | 156.691 (UpToDate) | - | - | 13.6 | 07:26:08 |  |
| apple-container-pg-loop-C04 | PASS | 932.678 | 865.498 (Committed) | - | - | 14.3 | 07:26:13 |  |
| apple-container-pg-loop-C05 | PASS | 1185.047 | 951.491 (Committed) | - | - | 13.9 | 07:26:19 |  |
| apple-container-pg-loop-C06 | PASS | 1440.505 | 6660.878 (Committed) | - | - | 13.2 | 07:26:25 |  |
| apple-container-pg-loop-C07 | PASS | 2224.848 | 12623.292 (Committed) | - | - | 12.9 | 07:26:44 |  |
| apple-container-pg-loop-C08 | PASS | 1097.982 | 6463.941 (Committed) | - | - | 11.4 | 07:27:05 |  |
| apple-container-pg-loop-C09 | PASS | 173.949 | 54.067 (UpToDate) | - | - | 12.2 | 07:27:16 |  |
| apple-container-pg-loop-C10 | PASS | 1582.356 | 412.838 (Committed) | - | - | 11.9 | 07:27:27 |  |
| apple-container-pg-loop-C11 | PASS | 300.843 | 1527.137 (UpToDate) | - | - | 10.3 | 07:27:42 |  |
| apple-container-pg-loop-C12 | PASS | 364.432 | 463.817 (Committed) | - | - | 10.3 | 07:27:56 |  |
| apple-container-pg-remote-C01 | PASS | 655.64 | 718.645 (Committed) | - | - | 10.3 | 07:28:00 |  |
| apple-container-pg-remote-C02 | PASS | 1238.988 | 750.594 (Committed) | - | - | 9.4 | 07:28:06 |  |
| apple-container-pg-remote-C03 | PASS | 1326.233 | 103.051 (UpToDate) | - | - | 9.2 | 07:28:15 |  |
| apple-container-pg-remote-C04 | PASS | 1319.589 | 1242.276 (Committed) | - | - | 8.7 | 07:28:20 |  |
| apple-container-pg-remote-C05 | PASS | 1239.095 | 1132.587 (Committed) | - | - | 8.4 | 07:28:26 |  |
| apple-container-pg-remote-C06 | PASS | 2025.308 | 8415.004 (Committed) | - | - | 8.1 | 07:28:32 |  |
| apple-container-pg-remote-C07 | PASS | 4289.306 | 22350.378 (Committed) | - | - | 8.3 | 07:28:47 |  |
| apple-container-pg-remote-C08 | PASS | 2088.598 | 12389.351 (Committed) | - | - | 10.2 | 07:29:22 |  |
| apple-container-pg-remote-C09 | PASS | 259.143 | 16.503 (UpToDate) | - | - | 9.7 | 07:29:44 |  |
| apple-container-pg-remote-C10 | PASS | 2947.214 | 560.194 (Committed) | - | - | 14.4 | 07:30:06 |  |
| apple-container-pg-remote-C11 | PASS | 506.774 | 2975.301 (UpToDate) | - | - | 14.5 | 07:30:35 |  |
| apple-container-pg-remote-C12 | PASS | 697.18 | 898.943 (Committed) | - | - | 14.6 | 07:31:00 |  |
| apple-container-sqlite-C01 | PASS | 813.038 | 584.959 (Committed) | 165.627 | 250.9 | 9.4 | 07:23:35 |  |
| apple-container-sqlite-C02 | PASS | 2055.549 | 593.416 (Committed) | 112.544 | 170.468 | 16.4 | 07:23:47 |  |
| apple-container-sqlite-C03 | PASS | 1877.81 | 28.48 (UpToDate) | 93.794 | 98.061 | 16.0 | 07:23:55 |  |
| apple-container-sqlite-C04 | PASS | 1611.088 | 669.029 (Committed) | 116.65 | 115.266 | 16.0 | 07:24:00 |  |
| apple-container-sqlite-C05 | PASS | 1407.257 | 667.475 (Committed) | 87.314 | 118.743 | 16.4 | 07:24:05 |  |
| apple-container-sqlite-C06 | PASS | 1316.611 | 7902.792 (Committed) | 115.577 | 161.487 | 15.6 | 07:24:14 |  |
| apple-container-sqlite-C07 | PASS | 3385.423 | 16545.928 (Committed) | 147.35 | 72.789 | 20.3 | 07:24:40 |  |
| apple-container-sqlite-C08 | PASS | 1503.172 | 7575.163 (Committed) | 90.93 | 62.932 | 18.2 | 07:25:06 |  |
| apple-container-sqlite-C09 | PASS | 103.662 | 3.101 (UpToDate) | 89.916 | 60.92 | 18.3 | 07:25:18 |  |
| apple-container-sqlite-C10 | PASS | 1174.692 | 157.217 (Committed) | 136.217 | 62.844 | 17.4 | 07:25:28 |  |
| apple-container-sqlite-C11 | PASS | 237.925 | 1442.799 (UpToDate) | 89.741 | 62.986 | 17.2 | 07:25:40 |  |
| apple-container-sqlite-C12 | PASS | 377.211 | 243.24 (Committed) | 84.012 | 107.24 | 15.0 | 07:25:51 |  |
| microsandbox-pg-loop-C01 | PASS | 687.378 | 1642.073 (Committed) | - | - | 17.2 | 07:25:37 |  |
| microsandbox-pg-loop-C02 | PASS | 1200.235 | 1464.422 (Committed) | - | - | 16.1 | 07:25:45 |  |
| microsandbox-pg-loop-C03 | PASS | 1646.479 | 90.955 (UpToDate) | - | - | 15.0 | 07:25:53 |  |
| microsandbox-pg-loop-C04 | PASS | 1118.316 | 920.225 (Committed) | - | - | 14.6 | 07:25:59 |  |
| microsandbox-pg-loop-C05 | PASS | 1098.04 | 949.992 (Committed) | - | - | 14.0 | 07:26:05 |  |
| microsandbox-pg-loop-C06 | PASS | 1113.602 | 6405.808 (Committed) | - | - | 14.3 | 07:26:11 |  |
| microsandbox-pg-loop-C07 | PASS | 2700.451 | 12952.783 (Committed) | - | - | 13.2 | 07:26:22 |  |
| microsandbox-pg-loop-C08 | PASS | 1096.039 | 6368.863 (Committed) | - | - | 12.9 | 07:26:44 |  |
| microsandbox-pg-loop-C09 | PASS | 252.051 | 6.743 (UpToDate) | - | - | 11.9 | 07:26:56 |  |
| microsandbox-pg-loop-C10 | PASS | 1228.537 | 194.014 (Committed) | - | - | 12.5 | 07:27:07 |  |
| microsandbox-pg-loop-C11 | PASS | 274.074 | 1513.06 (UpToDate) | - | - | 11.9 | 07:27:21 |  |
| microsandbox-pg-loop-C12 | PASS | 389.48 | 558.44 (Committed) | - | - | 11.3 | 07:27:33 |  |
| microsandbox-pg-remote-C01 | FAIL | - | - | - | - | 10.7 | 07:27:38 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c01's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C02 | FAIL | - | - | - | - | 10.7 | 07:27:39 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c02's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C03 | FAIL | - | - | - | - | 10.7 | 07:27:40 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c03's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C04 | FAIL | - | - | - | - | 10.3 | 07:27:42 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c04's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C05 | FAIL | - | - | - | - | 10.3 | 07:27:42 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c05's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C06 | FAIL | - | - | - | - | 10.3 | 07:27:43 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c06's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C07 | FAIL | - | - | - | - | 10.3 | 07:27:44 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c07's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C08 | FAIL | - | - | - | - | 11.6 | 07:27:46 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c08's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C09 | FAIL | - | - | - | - | 11.6 | 07:27:46 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c09's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C10 | FAIL | - | - | - | - | 11.6 | 07:27:47 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c10's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C11 | FAIL | - | - | - | - | 10.3 | 07:27:56 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c11's server at 192.168.2.123: no connection to the server was m |
| microsandbox-pg-remote-C12 | FAIL | - | - | - | - | 10.3 | 07:28:00 | sandbox.create: Unavailable machine lfs535k2m cannot reach PostgreSQL Store c12's server at 192.168.2.123: no connection to the server was m |
| microsandbox-sqlite-C01 | PASS | 669.032 | 611.625 (Committed) | 69.605 | 770.561 | 9.6 | 07:23:22 |  |
| microsandbox-sqlite-C02 | PASS | 2471.189 | 761.685 (Committed) | 155.835 | 159.675 | 9.4 | 07:23:35 |  |
| microsandbox-sqlite-C03 | PASS | 2617.122 | 87.222 (UpToDate) | 103.92 | 87.798 | 14.7 | 07:23:43 |  |
| microsandbox-sqlite-C04 | PASS | 1503.578 | 728.53 (Committed) | 77.874 | 120.651 | 16.4 | 07:23:50 |  |
| microsandbox-sqlite-C05 | PASS | 1955.406 | 839.174 (Committed) | 80.009 | 139.504 | 16.0 | 07:23:55 |  |
| microsandbox-sqlite-C06 | PASS | 1335.495 | 6883.807 (Committed) | 61.83 | 78.331 | 16.4 | 07:24:02 |  |
| microsandbox-sqlite-C07 | PASS | 2843.927 | 16771.326 (Committed) | 98.204 | 88.347 | 15.6 | 07:24:14 |  |
| microsandbox-sqlite-C08 | PASS | 1802.696 | 8433.378 (Committed) | 102.839 | 79.487 | 20.3 | 07:24:40 |  |
| microsandbox-sqlite-C09 | PASS | 249.152 | 47.047 (UpToDate) | 84.437 | 78.949 | 19.5 | 07:24:55 |  |
| microsandbox-sqlite-C10 | PASS | 1701.724 | 244.797 (Committed) | 65.551 | 87.994 | 18.2 | 07:25:08 |  |
| microsandbox-sqlite-C11 | PASS | 298.703 | 1471.227 (UpToDate) | 78.952 | 58.066 | 18.0 | 07:25:23 |  |
| microsandbox-sqlite-C12 | PASS | 411.76 | 284.802 (Committed) | 57.344 | 108.147 | 18.0 | 07:25:35 |  |
| parity/apple-container-sqlite-C01-cli | PASS | 658.468 | 562.075 (Committed) | 92.767 | 169.318 | 8.0 | 07:37:58 |  |
| parity/apple-container-sqlite-C01-python | PASS | 580.012 | 529.177 (Committed) | 90.035 | 153.765 | 7.5 | 07:38:17 |  |
| parity/apple-container-sqlite-C05-cli | PASS | 950.352 | 592.84 (Committed) | 96.373 | 115.922 | 7.8 | 07:38:03 |  |
| parity/apple-container-sqlite-C05-python | PASS | 888.956 | 577.742 (Committed) | 81.543 | 105.163 | 7.1 | 07:38:21 |  |
| parity/apple-container-sqlite-C09-cli | PASS | 114.615 | 12.063 (UpToDate) | 93.489 | 72.356 | 7.8 | 07:38:07 |  |
| parity/apple-container-sqlite-C09-python | PASS | 104.714 | 3.08 (UpToDate) | 88.717 | 58.558 | 7.1 | 07:38:25 |  |
| parity/apple-container-sqlite-C12-cli | PASS | 347.946 | 248.648 (Committed) | 100.725 | 116.567 | 7.7 | 07:38:15 |  |
| parity/apple-container-sqlite-C12-python | PASS | 333.021 | 245.301 (Committed) | 89.563 | 104.177 | 6.5 | 07:38:33 |  |
| ssh-pg-loop-C01 | FAIL | - | - | - | - | 7.4 | 07:35:21 | sandbox.create: Unsupported the ssh machine lfs535k2s cannot reach the PostgreSQL Store's server: an ssh machine cannot reach the caller's l |
| ssh-pg-remote-C01 | PASS | 1118.663 | 1111.859 (Committed) | - | - | 14.0 | 07:31:11 |  |
| ssh-pg-remote-C02 | PASS | 1212.389 | 800.575 (Committed) | - | - | 12.3 | 07:31:35 |  |
| ssh-pg-remote-C03 | PASS | 1069.335 | 102.959 (UpToDate) | - | - | 11.5 | 07:31:45 |  |
| ssh-pg-remote-C04 | PASS | 900.39 | 900.801 (Committed) | - | - | 11.0 | 07:31:50 |  |
| ssh-pg-remote-C05 | PASS | 1058.568 | 907.521 (Committed) | - | - | 10.3 | 07:31:56 |  |
| ssh-pg-remote-C06 | PASS | 1086.481 | 6984.041 (Committed) | - | - | 10.1 | 07:32:02 |  |
| ssh-pg-remote-C07 | PASS | 2768.451 | 13683.074 (Committed) | - | - | 10.5 | 07:32:15 |  |
| ssh-pg-remote-C08 | PASS | 1061.932 | 6464.823 (Committed) | - | - | 10.0 | 07:32:39 |  |
| ssh-pg-remote-C09 | PASS | 156.079 | 5.062 (UpToDate) | - | - | 10.0 | 07:32:51 |  |
| ssh-pg-remote-C10 | PASS | 1354.17 | 214.29 (Committed) | - | - | 10.2 | 07:33:03 |  |
| ssh-pg-remote-C11 | PASS | 261.853 | 1480.027 (UpToDate) | - | - | 10.2 | 07:33:18 |  |
| ssh-pg-remote-C12 | PASS | 369.954 | 520.988 (Committed) | - | - | 9.3 | 07:33:31 |  |
| ssh-sqlite-C01 | PASS | 723.376 | 555.319 (Committed) | 73.041 | 187.245 | 9.4 | 07:28:06 |  |
| ssh-sqlite-C02 | PASS | 1828.866 | 758.615 (Committed) | 74.842 | 189.04 | 8.7 | 07:28:19 |  |
| ssh-sqlite-C03 | PASS | 1423.758 | 26.123 (UpToDate) | 81.737 | 72.351 | 8.4 | 07:28:27 |  |
| ssh-sqlite-C04 | PASS | 2501.053 | 1226.544 (Committed) | 87.604 | 125.341 | 8.1 | 07:28:31 |  |
| ssh-sqlite-C05 | PASS | 1635.154 | 807.907 (Committed) | 78.713 | 154.18 | 8.5 | 07:28:39 |  |
| ssh-sqlite-C06 | PASS | 1531.225 | 11260.466 (Committed) | 97.693 | 107.976 | 8.3 | 07:28:47 |  |
| ssh-sqlite-C07 | PASS | 4684.626 | 27549.705 (Committed) | 186.288 | 319.611 | 10.2 | 07:29:22 |  |
| ssh-sqlite-C08 | PASS | 2226.155 | 12881.869 (Committed) | 371.401 | 110.272 | 14.4 | 07:30:06 |  |
| ssh-sqlite-C09 | PASS | 316.775 | 4.442 (UpToDate) | 113.514 | 147.891 | 14.5 | 07:30:27 |  |
| ssh-sqlite-C10 | PASS | 2313.005 | 266.986 (Committed) | 117.696 | 98.666 | 14.5 | 07:30:48 |  |
| ssh-sqlite-C11 | PASS | 221.795 | 1490.497 (UpToDate) | 67.214 | 64.664 | 13.5 | 07:31:23 |  |
| ssh-sqlite-C12 | PASS | 299.821 | 236.517 (Committed) | 60.497 | 109.652 | 11.5 | 07:31:43 |  |

`ssh-pg-loop-C01` is the expected refusal (not a failure of the item).

## Resources

Created and removed: Lima VM `lfs535k2s` (`limactl delete -f`), machines `lfs535k2a`, `lfs535k2m` (`machine remove`;
afterwards `container list -a`, `container volume list`, `msb list`, `msb volume list` show nothing of this home) and
`lfs535k2s` (`machine disconnect`, then the VM deleted), the throwaway msb sandbox `lfs535k2-netprobe`, PostgreSQL
container `lfs535k2-pg` (`docker rm -fv`, with all its databases), the shared home, the generated password and ssh
key. Left: nothing but this folder and scratch scripts (no secrets). `cleanup.log` holds the commands.
