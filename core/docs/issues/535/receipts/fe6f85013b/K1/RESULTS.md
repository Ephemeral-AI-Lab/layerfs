# K1 (host, macOS) results — #535/#536 terminal pass on `fe6f85013b`

Lane K1: `native` and `docker` on this Mac only (no VMs). Product: the frozen
macOS builds `k-macos/` (`SHA256SUMS` checked OK: `bin/layerfs` 5259d573…,
`bin/layerfs-daemon` 89cd70cd…, `node/layerfs.node` be6c7e5d…,
`_native.abi3.so` 84b5e3e6…) and the sealed Linux bundle `a1f0d886…`
(`LAYERFS_BUNDLE`, read only). Docker Desktop 29.5.2 (8 CPUs, 8 GiB guest).
Disposable Stores only; Durable `NOT_RUN — disabled by owner until explicit
reauthorization`. One attempt per operation; a second attempt only after a
diagnosis, both receipts kept.

| | start | end |
| --- | --- | --- |
| UTC | 2026-11-03T07:17:03Z | 2026-11-03T07:59:56Z |
| host free (cleanup.py check) | 259.38 GB | 248.79 GB |
| 1-minute load | 4.42 | 9.72 |

Item numbers are the lead's K1 brief (1–10); the spec item each covers is named.

| # | Item (spec) | Verdict | Receipts |
| --- | --- | --- | --- |
| 1 | 12 cells × {native sqlite, native pg-loop, docker sqlite, docker pg-loop, docker pg-remote} (#535 phase 8 items 1, 2) | **PASS 60/60** | `<machine>-<store>-Cnn.json`, `series.sh`; `docker-sqlite-C01.attempt1-machine-stopped.json` |
| 2 | many Stores on native: 2 sqlite + 2 PostgreSQL, 4 Sandboxes at once, each Commit read back (#535 phase 8 item 3, native part) | **PASS** (attempt 3; 1–2 were script faults) | `item02-many-stores-native*.json`, `many_stores.mjs` |
| 3 | `machine platforms` vs `connect/SUPPORT.json` macOS rows; observed results vs rows (#535 phase 8 item 4, macOS part) | **PASS** | `item03-*.json/.log` |
| 4 | contracts (#535 phase 8 item 5) | **PASS** | `item04-contracts.log` |
| 5 | binding parity CLI / Node / Python on native, C01 C05 C09 C12 (#535 phase 8 item 7, native part) | **PASS** (identical) | `item05-binding-parity.log`, `parity-native-*.json`, `parity_cell.py`, `parity_compare.py`, `workloads-dump.mjs`, `workloads.json` |
| 6 | 2.0 → layout 3 conversion, native + docker (docker stands in for "one VM") (#536 §12 item 2) | **PASS** (refusal naming both; conversion; 24/24 cells after) | `item06-conversion/` |
| 7 | intents: push, pull, create SIGKILLed at each phase (#536 §12 item 3) | **FAIL** (3 findings; every phase hit) | `item07-intents/`, `intent_kill.py`, `intents.sh`, `intents_clear.sh`, `intents_summary.py` |
| 8 | locks (#536 §12 item 4) | **PASS** | `item08-locks.json`, `locks.py` |
| 9 | layout and size (#536 §12 items 5, 6) | **PASS** (notes) | `item09-layout.log`, `item09-final-home.log` |
| 10 | #536 §10b contract (HANDOFF-2 §6 item 3) | **FAIL** (75 PASS, 8 FAIL in attempt 3, plus supplements; 1 row NOT_RUN) | `item10-*`, `contract_10b.py` |

## 1. Cells (60)

All 60 PASS (`python3 cells_table.py .` regenerates the table from the receipts):
native sqlite 12/12, native pg-loop 12/12, docker sqlite 12/12, docker
pg-loop 12/12, docker pg-remote 12/12.

- Runner: `node <k-macos>/node/test/verify/cells/cell.mjs <Cnn> <machine> <receipt>`
  under `perl -e 'alarm 115'`, one cell per invocation (`series.sh`), with
  `LFS533_DAEMON`/`LFS533_LAYERFS` = the frozen macOS daemon/CLI,
  `LFS533_PRODUCT=fe6f85013b`. native cells: a fresh home per cell (the
  runner's default). docker cells: one shared home (`LFS533_HOME`) holding
  `docker`, so each sqlite cell pushes its Store native → docker and pulls it
  back, then removes it; `LFS533_IMAGE=sha256:afa1ef9c…` (the bundle's
  `saved_from` image, as #533 ran docker).
- PostgreSQL: lane container `lfs535k1-pg` (`postgres:17-bookworm`, `0.0.0.0:55541`),
  one database per cell; password only in the scratch area, given as
  `LFS535_PG_PASSWORD=env:…` (no receipt holds it; checked by grep).
  pg-loop URL `postgres://postgres@127.0.0.1:55541/{n}c<nn>` (native: `nc<nn>`,
  docker: `c<nn>`; docker's `reach` rewrites it); pg-remote URL
  `postgres://postgres@192.168.2.123:55541/rc<nn>` (the Mac's LAN address,
  dialled as given; there is no second host, so this stands in for a remote
  server). Receipts' `store.host` shows each.
- Declared setup: `layerfs machine start docker` in the docker home before
  its cells (#535 D11: nothing starts a machine on demand). The first docker
  C01 ran before that and was refused, as D11 expects, `Unavailable store push,
  check: machine docker is not running; start it with layerfs machine start
  docker` (kept as `docker-sqlite-C01.attempt1-machine-stopped.json`).
- At most two cells at once, always in different homes or machines (native
  and docker series ran side by side; receipts carry timestamps). Timing is
  diagnostic.
- `series.sh` logged `rc=0` for every cell until fixed during item 6 (the
  `$(date)` substitution reset `$?`); the receipts' `ok` is the verdict.

## 2. Many Stores on native

`many_stores.mjs`, one home: `s1`, `s2` (sqlite on native), `p1`, `p2`
(PostgreSQL, loopback URL, databases m5/m6); the four `sandbox.create` calls
start within 0.1 ms of each other and all four Sandboxes are alive together
from 1557 ms until the first close at 1960 ms; the four Commits overlap
(1486–1927 ms); each `file.read` from the caller returns the token its
Sandbox wrote (all four `matches: true`, Commits `Committed`). PASS.
Attempt 1 mounted `main` (refused `NoSuchBranch`: `main` is the published
base; a forked Branch is mounted, as the runner does) and attempt 2 read a
relative path (refused: paths are absolute); both script faults, receipts kept.

## 3. Support rows (macOS)

`layerfs machine platforms` (host `macos`) and `connect/SUPPORT.json`'s macOS
rows: the same seven platforms in the same order, every SUPPORT.json field
equal; the live answer adds `ready`/`missing`/`version` only (microsandbox
`missing: tool:msb` because `~/.microsandbox/bin` was not on that command's
PATH; unused here). Every one of the 68 runner receipts (60 + 8 parity)
matches its row: native `backend native`, `route nfs`, sqlite in process (no
push); docker `backend docker`, `route fuse`, sqlite through the worker (pushed);
PostgreSQL Sandboxes allowed on both. SUPPORT.json refuses no Store
combination on macOS native/docker, so no refusal text applies in this lane.

## 4. Contracts

- no `get_text("kind") == Some("postgresql")` (and no `"postgresql"` literal) under `api/crates/layerfs-api/src`;
- `LAYERFS_KERNEL`: 8 hits, all Markdown under `core/docs/issues/533` and `535`
  (the spec naming its removal, phase-log rows "LAYERFS_KERNEL gone", #533's
  list of old env names) — historical docs only, no source/tool/test hit;
- `DaemonSetup` (`engine/crates/layerfs-bridge/src/daemon_setup.rs:30`) has
  `route: KernelRoute` (line 38) and `store: DaemonStore` (line 50);
- `python3 -B tools/check_product_boundary.py` PASS; its layer order refuses
  any engine → connect edge and no engine manifest names a connect crate.

## 5. Binding parity (native)

C01, C05, C09, C12 through the CLI binary and Python (`parity_cell.py`, the
runner's steps, the runner's exact workload bytes from `workloads-dump.mjs`)
against the Node runner's `native-sqlite-Cnn.json`: identical verdict, step
sequence, exec and body exit codes, Commit kinds (C09 setup Committed, timed
UpToDate; others Committed), verifier output (`VERIFY OK`), identity and body
hash. PASS.

## 6. Conversion (2.0 → layout 3)

- 2.0 build from `58b8d1742b` in the throwaway worktree
  `layerfs-issue535-k1base` (registered with `tools/worktree.py`): macOS CLI
  (`ec120175…`) and daemon (`8950a7de…`), and — because a 2.0 `docker`
  Sandbox needs a 2.0 worker — a 2.0 Linux bundle (`tools/linux_bundle.py
  daemon/worker/helper/seal --dest <registration>/bundle`, hash `9f2a3dae…`,
  never the shared bundle). `item06-conversion/base-build.log`.
- 2.0 homes (`make-2.0-homes.log`): H6 with a native Sandbox and a docker
  Sandbox (Store moved to docker with 2.0's `store move`), each mounted,
  written, Committed and closed; H6R with `sr6` (native) and `sdr6` (docker)
  left running. The native daemon was recorded as a symlink, repointed to the
  fe6f daemon at the "upgrade" (an in-place install).
- fe6f CLI on H6R: `store list`, `sandbox list`, `machine list` refused
  `InUse` naming **both** (`sandbox sdr6 runs on machine docker … close it with
  the build that started it (layerfs sandbox close sdr6); sandbox sr6 runs on
  machine native …`), `detail.related` filled; `doctor` reports both as
  errors; layout stays 2. After closing both with the 2.0 CLI the first fe6f
  call converted it (layout 3; `log/migration-2.json` lists the removed 2.0
  folders). PASS.
- H6 (all closed): the first fe6f call converted it (layout 3, nothing
  refused), the 2.0 Commits read back on both machines; the docker worker
  still ran the 2.0 bundle and calls to it were refused `Unavailable` naming
  `machine stop` then `machine start` (#536 worker hello row); after those
  two calls the worker (new bundle) had deleted the guest's layout 2 folders
  (`store/d6/project/p/sandboxes` gone, `.layout-3` written). Then C01–C12
  on native and on docker in H6: **24/24 PASS** (`cells/`, `cells/TABLE.md`).
  A first cells attempt ran the native and docker series in the same home at
  once, so their Store names collided (lane fault; `cells-attempt1-collided/`,
  set aside); the second ran them one after the other.
- The base worktree, its registration, Cargo homes, Linux volumes and the 2.0
  bundle were removed at the end (`cleanup.log`).

## 7. Intents — FAIL

Every point was hit (`item07-intents/SUMMARY.txt`): push and pull at `check`,
`snapshot`, `send` (first event), mid-`send`, `verify`, `install`, `record`
(the progress event), inside the record window (a thread polling the record
killed on `"phase":"record"`), `retire`; `sandbox.create` at `claim` and
`start` on native and on docker. The Python binding calls the progress
handler synchronously, so the kill lands at that event, before the phase is
written into `op` (so a kill at event X leaves `op.phase` = the previous phase).

What held: each kill left the `op` doctor names (certain → warning "holds
nothing"; record window → error "unresolved … `store status` shows it; `store
remove` clears it"); a mid-send/verify/install pull left `.incoming` on native
and doctor named it with `store pull` as the fix; `sandbox.get`, `doctor` (and
`store get`, `sandbox list`) changed no file (hashes); the clearing calls
worked: a repeated push/pull replaced each stale op (10/10), `store remove`
cleared both unresolved ops (after push/pull were refused `Unresolved` naming
it), `sandbox close` cleared all four killed creates (`was: gone`); no daemon
or container was left by a killed create.

Findings:
1. **`store.status` is not read-only**: it deletes stale lease files
   (`store/<s>/open/<pid>` of the killed process) and appends
   `log/leases.jsonl` (`api/crates/layerfs-control/src/store/status.rs:16-18`
   calls `lease::holders`, which removes; doctor even names `store status` as
   the fix). §12 item 3 requires it to report "without changing anything".
   The `op` itself is never changed by any read. `store.list` does the same
   (`store/list.rs:20-22`). Isolated in `iso-read-isolation.log` and
   `store-status-isolation.log`.
2. **A crash between install and record is certain, not uncertain**: killed
   at the `record` progress event, the target already holds the installed
   `store.sqlite`, but the record keeps `op` phase `install`, `uncertain:
   false`, and the Store on the source. Doctor says "holds nothing … the next
   operation replaces it", yet the next push/pull is refused `Exists` ("machine
   docker already holds a Store file named pu-record"; pull: native). The
   uncertain flag is set in memory (`store/transfer/phases.rs:267`) and only
   written by `enter(record)` (phases.rs:268), after install's effect (phases.rs:258-262).
   No documented call clears it except `store remove`, which leaves the
   target's copy.
3. **A crash at retire leaves both copies with no finding**: the record names
   the target, the source copy stays; the next opposite transfer is refused
   `Exists`; doctor reports nothing (a stray `store.sqlite` on native for a
   Store recorded on docker is not checked; a worker copy cannot be, as documented).

Also seen: a killed `claim` leaves `sandbox/<n>/.record.json.<pid>.1.tmp` (removed by `sandbox close`).

## 8. Locks — PASS

Two processes released by one start file: native creates of two projects'
Sandboxes overlap fully (0.052 s each vs 0.108 s solo); docker creates overlap
fully (3.43 s each vs 2.64 s solo, neither near solo + other); two processes
pushing Store `l5` (24 MB) to docker at the same instant: one pushed, the
other refused `Busy` ("Store l5 is busy: store.push (op-…, phase snapshot)
runs in process 48306").

## 9. Layout and size — PASS (notes)

On all eight homes and the four docker worker roots: no Sandbox record
outside `sandbox/<name>/`, no `store/*/project/*/sandboxes/`. Final home
(H6, the converted one): `doctor` "ok: nothing to report"
(`item09-final-home.log`). `api/crates/layerfs-api/src` at fe6f85013b:
`calls/` (check, dispatch, keys, mod, route, scope, table, text) and `lib.rs`
only. Notes: (a) the many-stores homes held stale leases left by the exited
Node process (doctor warnings), and every exited CLI process leaves its
leases too (the open-Store map `store/open.rs:28` is a process static, never
dropped at exit); (b) **worker roots keep `store/<s>/record.json` and
`store/<s>/project/<p>/record.json` after a pull or remove** — `retire`
deletes only `store.sqlite`, `-wal`, `-shm` (`store/transfer/steps.rs:241-253`)
and `store remove` only removes on the record's machine — so a later Store of
the same name pushed there refuses `project.init` with `Exists` (`store/identity.rs:351`;
hit in item 10's b/q).

## 10. #536 §10b — FAIL

`contract_10b.py` attempt 3 (`item10-contract-10b.json`: 75 PASS, 8 FAIL)
plus `item10-supplement.log`. Attempts 1 (argument-name clash) and 2 (busy
test invalid: the pusher was refused `InUse` by the test process's own lease
on that Store) were script faults; logs/receipt kept.

Held: lists carry `state` (`machine.list` also `supports`; `store.list`
items carry `op` while busy); with docker's worker stopped by `machine stop
docker`, `machine.list/get/status`, `machine.platforms`, `store.list/get/status`,
`project.list/get`, `sandbox.list` and `doctor` all answered and the worker
stayed stopped; down row: `store.status` `down` with `down_reason`, Store
calls, `sandbox.create`, `project.init`, `project.remove` (supplement B),
`store.pull`, `store.remove` refused `Unavailable` naming `layerfs machine
start` (D11), none started it; in_use row (another process's lease): reads
and Store calls answer, `sandbox.create` allowed, push/remove `InUse` with
`related` naming the lease pid; busy row (a live push paused in `send`):
`store.status` `busy` + `op`, reads answer, `sandbox.create`, `project.init`,
`project.remove`, push, pull, remove refused `Busy` naming the op; unresolved
row (supplement A attempt 2): reads answer with `op`, `sandbox.create`,
`project.init`, `project.remove`, push, pull refused `Unresolved` naming
`store status` and `store remove`, `store remove` clears; push/pull refused
`InUse` with `related` while a Sandbox record exists, including one whose
process was killed (`state: gone`); progress for push and pull is
`{phase, done, total}` with byte counts in `send` and nulls elsewhere;
project.init into a docker-held sqlite Store sends `{phase: send, done, total}`
bytes; Workspace handles (`sa:w1` and `sa:<path>`) carry `ws`/`name`/`path`
and every `workspace.*` call took both forms; `InUse` `related` from
`store.push`, `store.pull`, `store.remove`, `machine.stop`, `store.disconnect`
(with a Sandbox); `store.remove` on PostgreSQL refused `InvalidInput` naming
`store disconnect`; final doctor clean.

FAIL:
1. `project.init` progress on `native` sqlite and PostgreSQL is `{step:
   "files"|"tree", entries, files, bytes}`, not `{phase, done, total}` (no
   `send` byte counts); only Init through a worker uses the §10b shape.
2. `project.remove` `InUse` ("Sandbox sa of a/p is running") has `detail: null`, no `related`.
3. `store.disconnect` `InUse` from another process's lease ("Store g is open
   in process [41394]") has `detail: null`, no `related`.
4. busy row: `file.list` refused `Busy` while the push runs (§10b: "answer
   from the source until the record step"; the 2.0.0 docs say Busy).
5. unresolved row: `file.list`, `branch.list` refused `Unresolved` (§10b:
   "answer where the placement is"; the docs say Unresolved).
6. (attempt 3's "down: project.remove b/q" check was invalid — b/q had been
   refused `Exists` by the worker leftover in §9 note (b); supplement B shows
   the row holds.)

NOT_RUN: `related` on `InUse` from `machine.remove` (docker is built in:
refused `InvalidInput` "docker is built in and never removed"; no VM in this lane).

Other defect seen: after a CLI `branch fork` the Store's WAL stays non-empty
(12392 bytes) and `store push` is refused `Failed … has an unfinished
write-ahead log; open the Store once on its machine`
(`store/transfer/steps.rs:105-113`); a CLI read does not clear it
(supplement C). Probable cause: the CLI process exits without closing its
open Store (process-static map `store/open.rs:28`). The 2.0 build behaves the
same (`make-2.0-homes.log`).

## Resources

Created and removed (`cleanup.log`): PostgreSQL container `lfs535k1-pg`
(`docker rm -fv`); homes (8) and their docker worker containers and data
volumes `lfs-{a58aa71d,f3013024,72f0a70c,3b57211a}-docker[-data]`; the
throwaway worktree `layerfs-issue535-k1base` (`git worktree remove`), its
registration `~/.cache/layerfs-worktrees/layerfs-issue535-k1base-fb1807d8525e`
(Cargo homes, logs, the 2.0 bundle) and its Linux volumes
`lfs-dev-layerfs-issue535-k1base-fb1807d8525e-{cargo,target}`. Native cell
homes were removed by the runner. Left: the untagged Docker image
`sha256:50e8f040…` (3.79 GB) that the product loaded from the sealed bundle's
`sandbox-image.tar` on `machine start docker` in a home with no image set
(#535 P5, ~90 s); it is the same image for every lane using this bundle and
other lanes' homes may record it as `loaded_image`, so it is left for the lead.
