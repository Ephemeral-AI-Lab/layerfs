# L1 (host, macOS) results — #535/#536 final terminal pass on `35ed671b6c`

Lane L1: `native` and the built-in `docker` on this Mac (no VMs), redoing
first-pass lane K1 (`../../fe6f85013b/K1/`) on the new commit and checking
each fix since `fe6f85013b` this lane reaches. Product: the frozen macOS
builds `k-macos/` (`SHA256SUMS` checked OK: `bin/layerfs`, `bin/layerfs-daemon`,
`node/layerfs.node`, `python/src/layerfs/_native.abi3.so`; `PRODUCT`
35ed671b6c34…) and the sealed Linux bundle `764e4977…` (`LAYERFS_BUNDLE`,
read only). Docker Desktop 29.5.2. Disposable Stores only; Durable
`NOT_RUN — disabled by owner until explicit reauthorization`. One attempt per
operation; a second attempt only after a diagnosis, both receipts kept.

| | start | end |
| --- | --- | --- |
| UTC | 2026-11-03T08:47:45Z | 2026-11-03T09:14:59Z |
| host free (cleanup.py check) | 230.29 GB | 228.58 GB |
| 1-minute load | 3.77 | 8.33 |

| # | Item (spec) | Verdict | Receipts |
| --- | --- | --- | --- |
| 1 | 12 cells × {native sqlite, native pg-loop, docker sqlite (pushed, pulled back), docker pg-loop, docker pg-remote} (#535 phase 8 items 1, 2) | **PASS 60/60** | `<machine>-<store>-Cnn.json`, `CELLS.md`, `series.sh`, `docker-pg-series.sh`, `logs/series-*.log` |
| 2 | many Stores on native: 2 sqlite + 2 PostgreSQL, 4 Sandboxes at once, each Commit read back (#535 phase 8 item 3, native part) | **PASS** | `item02-many-stores-native.json`, `item02-many-stores-doctor.log`, `many_stores.mjs` |
| 3 | `machine platforms` vs `connect/SUPPORT.json` macOS rows; observed results vs rows (#535 phase 8 item 4, macOS part) | **PASS** | `item03-machine-platforms.json`, `item03-platforms.log`, `platforms.py` |
| 4 | contracts (#535 phase 8 item 5) | **PASS** | `item04-contracts.log` |
| 5 | binding parity CLI / Node / Python on native, C01 C05 C09 C12 (#535 phase 8 item 7, native part) | **PASS** (identical) | `item05-binding-parity.log`, `parity-native-*.json`, `parity_cell.py`, `parity_compare.py`, `parity_series.sh`, `workloads-dump.mjs`, `workloads.json` |
| 6 | 2.0 → layout 3 conversion, native + docker (docker stands in for "one VM") (#536 §12 item 2) | **PASS** (refusals naming the running Sandbox, native and docker; conversion; 24/24 cells after) | `item06-conversion/` |
| 7 | intents: push, pull, create SIGKILLed at each phase (#536 §12 item 3), now with the fixed recovery | **PASS** (every phase hit; reads change nothing; every clearing call works) | `item07-intents/`, `intent_kill.py`, `intents.sh`, `intents_clear.sh`, `intents_summary.py` |
| 8 | locks (#536 §12 item 4) | **PASS** | `item08-locks.json`, `item08-locks.log`, `locks.py` |
| 9 | layout and size (#536 §12 items 5, 6) | **PASS** | `item09-layout.log`, `layout.sh` |
| 10 | #536 §10b contract (HANDOFF-2 §6 item 3) | **101 PASS, 3 FAIL** — the 3 are the recorded spec differences (busy-row and unresolved-row reads); 1 row NOT_RUN | `item10-contract-10b.json/.log`, `contract_10b.py`; attempt 1 `item10-contract-10b.attempt1-script-fault.*` |
| F | fixes since `fe6f85013b` reachable on macOS | **11 PASS, 0 FAIL**, 2 not reachable here (table below) | `fixes.log`, `fixes.sh`, `fix-machine-list-state.log`, and the items named |

## Fixes since fe6f85013b

| Fix | Verdict | Evidence |
| --- | --- | --- |
| `store.status`/`store.list` change nothing (no lock file, no lease pruning, no `log/leases.jsonl` line) | **PASS** | `fixes.log` F-e: a killed holder's stale lease `store/r1/open/<pid>` stays; hashes of every home file and folder equal before/after CLI and Python `store status`/`store list`; no `log/leases.jsonl` created. Item 7: all 22 kills' reads (`store status/get/list`, `sandbox get/list`, `doctor`) changed no home or worker file (`READS UNCHANGED`) |
| `project.init` progress `{phase, done, total}` on every route | **PASS** | item 10 checks 5–13: native sqlite and PostgreSQL emit exactly `{phase,done,total}`, phases `check, install, record`, nulls, no `send`; Init into a docker-held Store adds `send` with byte counts ending `done == total` |
| `InUse` `detail.related` for `project.remove`, `store.disconnect`, `backup.restore` | **PASS** | item 10: `project.remove` (Sandbox), `store.disconnect` (Sandbox; another process's lease), `backup.restore` (Sandbox; another process's lease) all name `{kind, name}` (lane MinIO bucket) |
| push/pull crash between install and record is `Unresolved`; a new push or pull (either direction) clears it, deleting the copy the record does not name | **PASS** | item 7: kills at the `record` event leave `op` phase `install`, `uncertain: true`; in the record window phase `record`, uncertain; Store calls refused `Unresolved` naming `store status` and the clearing push/pull; doctor error names the same call. Same direction (`pu-record-window` by push, `pl-record-window` by pull) clears and runs; opposite direction (`pu-record` by `store pull`, `pl-record` by `store push`) clears (op dropped, the unnamed copy deleted, the Store answers) and then runs as usual, i.e. is refused `InvalidInput` "already on native" / "is on machine docker" (exit 125). Item 10 unresolved row: the same via the Python package |
| retire window: usable Store, `last_transfer.retired: false`, doctor warning; next push/pull deletes the old copy | **PASS** | item 7 `pu-retire`, `pl-retire`: record on the new machine, `op` certain at `retire`, `retired: false`, `file list` answers, doctor warns naming the next call; the next opposite call answered and left `retired: true` and one copy |
| retiring from a worker removes the Store's whole folder there | **PASS** | item 7 clear log: after every pull `worker=[(no folder)]`; item 10 check 2 |
| `workspace.mount(sandbox, "main")` refused `InvalidInput` | **PASS** | `fixes.log` F-c, native and docker: "main is the project's published line, not a Branch, and is not mounted …" |
| CLI `store push` after a CLI `branch fork` | **PASS** | `fixes.log` F-a: `wal_bytes: 0` after the CLI fork; push and pull answered |
| `machine list`/`get` docker state = `machine status docker` | **PASS** | `fix-machine-list-state.log`: `stopped` in all three before `machine start docker`, `running` in all three after |
| a missing PostgreSQL database's message | **PASS** | `fixes.log` F-d: `store connect` refused `Unavailable` "the server refused the session: the database does not exist, or the user or password is wrong"; nothing recorded |
| `LAYERFS_TRACE` worker-connection lines for docker Store calls (item 6 R1) | **PASS** | `fixes.log` F-b: each CLI Store call on a docker-held Store appends exactly one `{at, event:"worker.connection", op:"store.call", worker, sent:true, ms}` line (1, 1, 1, then 3 for three calls); three Python calls in one process, 3 lines; a native Store call, 0 lines. R2 (recorded difference): a docker `workspace.exec` is one `op:"sandbox.host"` worker connection |
| Linux docker `--network host` (K3 D-1) | not reachable | Linux only (lane L3/K3) |
| `machine create` of a platform not on this host refused in the table's words (K3 D-2) | not reachable | every SUPPORT.json platform is on macOS (`item03-platforms.log`), so nothing is refused for that reason here |

## 1. Cells (60)

All 60 PASS (`CELLS.md`, regenerated by `python3 cells_table.py .`; it also
lists the 8 parity receipts): native sqlite 12/12, native pg-loop 12/12,
docker sqlite 12/12, docker pg-loop 12/12, docker pg-remote 12/12.

- Runner: `node <k-macos>/node/test/verify/cells/cell.mjs <Cnn> <machine> <receipt>`
  under `perl -e 'alarm 115'`, one cell per invocation (`series.sh`), with
  `LFS533_DAEMON`/`LFS533_LAYERFS` = the frozen macOS daemon/CLI,
  `LFS533_PRODUCT=35ed671b6c`. native: a fresh home per cell. docker: one
  shared home (`LFS533_HOME`, `<HD>`) holding `docker`; each sqlite cell pushes
  its Store native → docker and pulls it back, then removes it.
  `LFS533_IMAGE` was **not** set (K1 set the `saved_from` image): the cells used
  the image the product loaded from the bundle's `sandbox-image.tar` on the
  first `machine start docker` (`sha256:50e8f040…`, already present in
  Docker Desktop from the first pass, so no new image was created).
- Declared setup: `layerfs machine start docker` in `<HD>` before its cells
  (#535 D11), 08:49:15Z → 08:49:44Z (`logs/docker-start.raw`).
- PostgreSQL: lane container `lfs535l1-pg` (`postgres:17-bookworm`,
  `0.0.0.0:55551`), one database per cell; password only in the scratch area,
  given as `LFS535_PG_PASSWORD=env:LAYERFS_PG_L1` (no receipt holds it; checked
  by grep). pg-loop URL `postgres://postgres@127.0.0.1:55551/{n}c<nn>` (native
  `nc<nn>`, docker `c<nn>`; docker's `reach` rewrites it); pg-remote URL
  `postgres://postgres@192.168.2.123:55551/rc<nn>` (the Mac's LAN address,
  dialled as given; there is no second host, so this stands in for a remote
  server).
- At most two cells at once, never two in the same home and machine (native
  series and docker series side by side; parity cells overlapped docker
  pg-remote; receipts carry timestamps). Timing is diagnostic.

## 2. Many Stores on native — PASS

`many_stores.mjs`, one home: `s1`, `s2` (sqlite), `p1`, `p2` (PostgreSQL,
loopback URL, databases m1/m2); the four `sandbox.create` calls start at the
same instant (1147.5 ms) and all four Sandboxes are alive together from
1463 ms until the first close at 1981 ms; the four Commits overlap
(1345–1941 ms); each `file.read` from the caller returns the token its
Sandbox wrote (`matches: true`, Commits `Committed`). Note (as K1 note a):
the exited Node process leaves one stale lease per Store
(`item02-many-stores-doctor.log`, doctor warnings); the CLI now closes its
Stores, the Node package does not at process exit.

## 3. Support rows (macOS) — PASS

`machine platforms` (host `macos`) and SUPPORT.json's macOS rows: the same
seven platforms in the same order, every SUPPORT.json field equal; the live
answer adds `ready`/`missing`/`version` only (all ready, microsandbox `msb
0.7.11` with `~/.microsandbox/bin` on PATH). All 68 runner receipts (60 + 8
parity) match their rows: native `backend native`, `route nfs`, sqlite in
process (no push); docker `backend docker`, `route fuse`, sqlite through the
worker (pushed); PostgreSQL Sandboxes allowed on both. No macOS row refuses
a combination, so no refusal text applies in this lane.

## 4. Contracts — PASS

No `get_text("kind") == Some("postgresql")` and no `"postgresql"` literal
under `api/crates/layerfs-api/src`; `LAYERFS_KERNEL` only in the #533/#535
spec and decision Markdown and the first-pass receipts quoting them;
`DaemonSetup.route: KernelRoute` (daemon_setup.rs:38), `.store: DaemonStore`
(:50); `check_product_boundary.py` PASS; no engine manifest names a
connect/api crate.

## 5. Binding parity (native) — PASS

C01, C05, C09, C12 through the CLI and Python (`parity_cell.py`, the
runner's steps and exact workload bytes; `workloads.json` byte-identical to
the first pass's) against the Node runner's `native-sqlite-Cnn.json`:
identical verdict, step sequence, exit codes, Commit kinds (C09 setup
Committed, timed UpToDate; others Committed), `VERIFY OK`, identity and body
hash.

## 6. Conversion (2.0 → layout 3) — PASS

- 2.0 build from `58b8d1742b` in the throwaway worktree
  `layerfs-issue535-l1base` (registered with `tools/worktree.py`): macOS CLI
  `c71698de…` and daemon `d04f9150…`, and a 2.0 Linux bundle in the
  registration (`216b8aa6…`; its three binaries hash as K1's 2.0 bundle).
  `item06-conversion/base-build.log` (attempt 1's dependency seed was refused;
  see there).
- 2.0 homes: H6 with a native Sandbox (`n6`) and a docker Sandbox (`d7`), each
  mounted, written, Committed, read back and closed; H6R with `sr6` (native)
  and `sdr7` (docker) left running; H6N with `sr8` (native) left running.
  Two script faults on the way, logs kept: attempt 1 lacked 2.0's `machine
  update docker --image` (`make-2.0-homes.attempt1-no-docker-image.log`);
  attempt 2's `store move` was refused "unfinished write-ahead log" after a
  2.0 CLI `branch fork` (the 2.0 CLI's unclosed Store, K2 D3, fixed in 35ed;
  `…attempt2-wal-after-cli-fork.log`); attempt 3 moved right after Init and
  forked on docker (`make-2.0-docker.log`). In H6R the native `sr6` had
  ended before the upgrade (its process belonged to an earlier command's
  session; 2.0's close answered `was_gone: true`), so H6N repeats the native
  case with the 2.0 CLI in its own session (`h6n.log`).
- In-place upgrade: the native daemon is recorded as a symlink, repointed to
  the 35ed daemon. 35ed CLI on H6R: `store list`, `sandbox list`, `machine
  list` refused `InUse` naming `sdr7` (`… runs on machine docker, whose
  worker this build does not ask; close it with the build that started it`),
  `detail.related` filled; doctor error; layout stays 2. On H6N: refused
  naming `sr8` (`runs on machine native; …`), doctor error, layout 2. After
  the 2.0 close the next 35ed call converted each (layout 3,
  `log/migration-2.json` lists the removed 2.0 folders; H6R's lists the
  gone `r6/p#1` as reclaimed). Doctor then warned that docker's worker is of
  another build (H6R) / clean (H6N).
- H6 (all closed): the first 35ed call converted it; the 2.0 Commit reads
  back on native; on docker the 2.0 worker was refused `Unavailable` naming
  `machine stop` then `machine start` (#536 worker hello row); after those
  two calls the Commit reads back and the worker root holds only
  `.layout-3` and `store/d7/…` (no `sandboxes` folder). `convert.log`.
- Then C01–C12 on native and on docker in H6, one series after the other:
  **24/24 PASS** (`cells/`, `cells/TABLE.md`). docker cells there use the
  image 2.0 recorded (`afa1ef9c…`). A first launch of this series did not run
  (its background shell ended with the launching command; no receipt, no
  Store made) and was relaunched with nohup.
- The base worktree, its registration, Cargo homes, Linux volumes and the
  2.0 bundle are removed at the end (`cleanup.log`).

## 7. Intents — PASS

Every point was hit (`item07-intents/SUMMARY.txt`): push and pull at
`check`, `snapshot`, `send`, mid-`send`, `verify`, `install`, `record` (the
progress event), the record window, `retire`; `sandbox.create` at `claim`
and `start` on native and docker (home `<HI>`, its own docker worker). A
kill at progress event X lands before X is written into `op`, so `op.phase`
names the previous phase (as K1 found).

- Certain kills (check … install event): `op` certain, doctor warning
  "holds nothing; the next operation … replaces it"; mid-send/verify/install
  left `.incoming` (push: on the worker; pull: on native, doctor names it);
  the same push/pull again replaced each (12/12: `pu-*`, `pl-*`), leaving one
  copy and no worker folder after a pull.
- Install–record kills (`record` event, record window): `op` uncertain
  (`install` resp. `record`), `Unresolved` with the clearing call named,
  doctor error naming `store push <s> docker` / `store pull <s>`; cleared as
  in the fixes table (4/4).
- Retire kills: `op` certain at `retire`, Store usable on the new machine,
  `retired: false`, doctor warning naming the next call; the next opposite
  call deleted the old copy and ran (2/2).
- Creates: `op` certain (`claim`/`start`), doctor warning, no daemon or
  container left; `sandbox close` cleared all four (`was: gone`). A killed
  `claim` leaves `sandbox/<n>/.record.json.<pid>.1.tmp` (removed by the close).
- Reads: no `store status/get/list`, `sandbox get/list` or `doctor` changed
  any home or worker file (22/22 `READS UNCHANGED`; 22 kills, each hit: 9 push, 9 pull, 4 create). Final doctor "ok:
  nothing to report".
- The clearing script's first run was stopped by this lane's 115 s alarm on
  the whole script just after `pl-send`'s pull answered; run 2 continued from
  `pl-send-mid`, repeating no call (`clear.log`).

## 8. Locks — PASS

Two processes released by one start file: native creates of two projects'
Sandboxes overlap fully (0.045/0.044 s vs 0.045 s solo); docker creates
overlap fully (3.58/3.57 s vs 2.41 s solo, neither near solo + other); two
processes pushing Store `l5` to docker at once: one pushed, the other refused
`Busy` ("Store l5 is busy: store.push (op-…, phase snapshot) runs in process …").

## 9. Layout and size — PASS

On all seven homes (`<HD>` cells, `<HI>` intents/locks, `<H10>`/`<H10B>`
10b, `<H6>`/`<H6R>`/`<H6N>` converted) and the running docker worker roots:
no Sandbox record outside `sandbox/<name>/`, no `store/*/project/*/sandboxes/`,
doctor "ok: nothing to report" everywhere except H6R, whose docker worker
still ran the 2.0 bundle (doctor's warning; that 2.0 worker kept layout 2's
`store/dr7/project/p/sandboxes`); after the documented `machine stop docker`,
`machine start docker` the new worker had removed it and H6R's doctor was
clean too (`item09-layout.log`). Final home H6 (converted, after its 24
cells): doctor clean. `api/crates/layerfs-api/src` at 35ed671b6c: `calls/`
(check, dispatch, keys, mod, route, scope, table, text) and `lib.rs` only.
The "commit table shows the deleted 1.1 paths" half of §12 item 6 is the
lead's TERMINAL-PASS table, not run here.

## 10. #536 §10b — 101 PASS, 3 FAIL (all recorded spec differences)

`contract_10b.py` attempt 2 (fresh home, fresh PostgreSQL database
`k10g2`, lane MinIO bucket). Attempt 1 (42 PASS, 0 FAIL until then) crashed
on a script fault (the lease holder for Store `k` read a Branch the script
never forked); receipt and log kept, its home cleaned
(`item10-attempt1-home-cleanup.log`).

Held: lists carry `state` (`machine.list` also `supports`); Workspace handles
(`sa:w1`, `sa:<path>`) on every `workspace.*` call; `InUse` `related` from
`store.push`, `store.remove`, `project.remove`, `store.pull`, `machine.stop`,
`store.disconnect` (Sandbox and lease), `backup.restore` (Sandbox and lease),
lease holders in the in_use row; progress for push/pull/Init (per route);
the worker keeps no folder for a pulled Store; in_use row (reads answer,
`sandbox.create` allowed, push/remove `InUse` naming the lease); busy row
(`store.status` `busy` + `op`, reads answer, `sandbox.create`,
`project.init`, `project.remove`, push, pull, remove refused `Busy` naming the
op, the paused push then completed); no list/get/status call, `machine
platforms` or doctor starts a stopped docker; down row (`store.status` `down`
with `down_reason`; Store calls, `sandbox.create`, `project.init`,
`project.remove`, `store.pull`, `store.remove` refused `Unavailable` naming
`layerfs machine start`, none started it); idle after `machine start`;
unresolved row (status/list `unresolved` with `op`, reads answer,
`sandbox.create`/`project.init`/`project.remove` refused `Unresolved` naming
`store status u` and `store push u docker`, doctor error, the new push clears
it and runs, native keeps no copy); final doctor clean.

FAIL (recorded spec differences, §10b vs the 2.0.0 docs; not new defects):
busy row `file.list` refused `Busy`; unresolved row `file.list` and
`branch.list` refused `Unresolved`.

NOT_RUN: `related` on `InUse` from `machine.remove` (docker is built in:
refused `InvalidInput` "docker is built in and never removed"; no VM here).

## Product defects

None new in this lane. Notes: (a) the Node package leaves its Store leases
when its process exits (doctor warnings, cleared by the next push/pull/remove)
— as K1 note a, now only for Node/Python processes since the CLI closes its
Stores; (b) a push/pull of the opposite direction that clears an unresolved
intent then refuses as usual (exit 125 after the clearing), which matches the
docs ("first deletes the copy …, then runs") but reads as a failure to a
caller.

## Resources

Created and removed (`cleanup.log`, 09:14:46Z): PostgreSQL container
`lfs535l1-pg` and MinIO container `lfs535l1-minio` (from the local
`cgr.dev/chainguard/minio` image; never `layerfs-minio`), with `-v`; seven
homes and the six docker worker containers and data volumes
`lfs-{8fe9d479,afd96eb1,9de5072c,f41276f4,2f3f2250,52c7944a}-docker[-data]`;
the throwaway worktree `layerfs-issue535-l1base` (`git worktree remove`), its
registration `~/.cache/layerfs-worktrees/layerfs-issue535-l1base-860c8face50e`
(Cargo home, logs, the 2.0 bundle) and its Linux volumes
`lfs-dev-layerfs-issue535-l1base-860c8face50e-{cargo,target}`; four exited
containers of this registration's failed first build steps (removed during
the work). Native cell homes and parity homes were removed by their runners.
No Docker image was created: `machine start docker` loaded the bundle's
`sandbox-image.tar` as the already present `sha256:50e8f040…`, and the 2.0
homes used `sha256:afa1ef9c…` (present). Left: nothing of this lane's except
the receipts; the untagged `50e8f040…` image is the first pass's, left for
the lead.
