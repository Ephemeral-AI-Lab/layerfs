# L3 results: lima, firecracker and the Linux rows at `35ed671b6c`

Lane L3 of the second (final) #535/#536 terminal pass, 2026-11-03
08:48–09:13 UTC, on this Mac (arm64, 14 CPUs, Docker Desktop, Lima 2.2.1).
One product commit, `35ed671b6c`; Disposable Stores only. Durable: NOT_RUN —
disabled by owner until explicit reauthorization. All binaries are debug
builds, so every time in a receipt is diagnostic; the verdict is PASS/FAIL.
One sample per cell. `summary.md` (made by `summarize.py`) tabulates every
cell receipt with its times and 1-minute load; `traces.txt` (made by
`traces.py`) counts each Mac cell's `LAYERFS_TRACE` lines.

## Verdicts

| Item | Platform / row | Store | Result | First error |
| --- | --- | --- | --- | --- |
| 1 sqlite cells | lima `lfs535l3l` | sqlite, pushed out and pulled back | **PASS 12/12** | – |
| 1 sqlite cells | firecracker `lfs535l3f` | sqlite, pushed out and pulled back | **PASS 12/12** | – |
| 1 sqlite cells | Linux `native` (in `lfs535l3x`) | sqlite | **PASS 12/12** | – |
| 1 sqlite cells | Linux `docker` (in `lfs535l3x`) | sqlite | **FAIL 0/12** | `store.push`: `Unavailable` "machine docker is not running; start it with `layerfs machine start docker`": `machine start docker` itself fails (product defect D-3) |
| 2 PostgreSQL cells | lima | loopback `127.0.0.1:55553` (rewritten to 192.168.5.2) | **PASS 12/12** | – |
| 2 PostgreSQL cells | lima | remote `192.168.2.123:55553` | **PASS 12/12** | – |
| 2 PostgreSQL cells | firecracker | loopback (rewritten to 192.168.5.2) | **PASS 12/12** | – |
| 2 PostgreSQL cells | firecracker | remote | **PASS 12/12** | – |
| 2 PostgreSQL cells | Linux `native` | remote | **PASS 12/12** | – |
| 2 PostgreSQL cells | Linux `docker` | remote | **FAIL 0/12** | `sandbox.create`: `Unavailable` "machine docker is not running; start it with `layerfs machine start docker`" (D-3) |
| 4 (Linux part) platforms vs `SUPPORT.json` | Linux | – | **PASS** | 7/7 platforms, 0 differences in every static field |
| 4 (Linux part) create refusals word for word | Linux | – | **PASS 3/3** | – |
| 9 machine power | lima | – | **PASS** | – |
| 9 machine power | firecracker | – | **PASS** | – |
| 5 Linux rerun of `b772e77f97`'s two tests | Linux container | – | **PASS 2/2** | – |
| 5 `sandbox_deploy` (Linux branch) | Linux container | – | **PASS 1/1** | – |
| 5 `machine_providers` (incl. Linux docker argv) | Linux container | – | **PASS 13/13** | – |
| `machine list`/`get` vs `machine status docker` | Linux | – | **PASS** (agree: `stopped` before start, after the failed start, after stop) | the `running` side could not be reached (D-3) |

Totals: 120 cells owned (lima 36, firecracker 36, Linux native 24, Linux
docker 24): **96 PASS, 24 FAIL** (all Linux docker, all D-3).

The Linux rows use only the remote URL form: inside the Linux VM a loopback
URL names the VM itself, which has no server (owner rule D12: no PostgreSQL
in any VM). There is no second host, so the Mac's LAN address
`192.168.2.123:55553` stands in for a remote server.

## Fixed since `fe6f85013b`: checks this lane reached

| Fix | Verdict | Evidence |
| --- | --- | --- |
| Linux docker: worker runs `--network host` and dials its Sandboxes' published loopback ports (K3 D-1) | **FAIL** | The worker container does run `--network host` (`docker-home.log`: NetworkMode `host`, `LAYERFS_WORKER_HOST_NETWORK=1`, `LAYERFS_WORKER_LISTEN=127.0.0.1:44845`), and from inside it a container port published on 127.0.0.1 answers (`docker-diag2.log`): the dial route is right. But `machine start docker` fails after 60 s, so no Linux docker cell gets a worker: 0/24 again (D-3). |
| `machine create` of a platform not on this host refused in the table's words, before the tool probe (K3 D-2) | **PASS** | `item4.log`, `item4-compare.txt`: lima, apple-container, firecracker each `Unsupported` with exactly `support.rs:76-78`'s text, `detail: null`. |
| A missing PostgreSQL database is `Unavailable` "the server refused the session …" | **PASS** | `missing-db.log`: `store connect` of `…/nosuchdb` → `Unavailable` "the server refused the session: the database does not exist, or the user or password is wrong"; the server logged `FATAL: database "nosuchdb" does not exist`. Home unchanged (`store list` empty). |
| `workspace.mount(sandbox, "main")` refused `InvalidInput` (D14) | **PASS** | `mount-main.log` (firecracker Sandbox): `InvalidInput` "main is the project's published line, not a Branch, and is not mounted: …"; a side Branch mounts and unmounts. |
| `machine list`/`get` show the built-in docker's live state | **PASS** (Linux, stopped side) | `docker-home.log`, `docker-home-teardown.log`: list, get and status all `stopped` before `machine start`, after the failed start (status: "the machine runs but its worker does not answer") and after `machine stop`. Not `ready` as at `fe6f85013b`. The running side is untested (D-3). Mac home: list showed docker `stopped` (`setup.log`). |
| `LAYERFS_TRACE` worker-connection lines (item 6 R1) | observed, not this lane's item | Every Mac cell wrote well-formed lines (0 malformed, `traces.txt`): lima/firecracker sqlite cells `store.call` 3–4, `store.file` 8, `sandbox.host` 6–7; PostgreSQL cells `sandbox.host` only (Store calls in process). Linux cells: none (native has no worker; docker never got one). |

Fixes not reachable in this lane's scope (other lanes): CLI closes its
Stores, `store.status`/`list` side effects, `project.init` progress,
`InUse` `detail.related`, push/pull recovery and worker retire.

## Product defect found (not changed here)

**D-3. Linux `docker`: `machine start docker` always fails; the readiness
check looks for the worker on port 7070, but a host-network worker listens on
a free host port.** `8b1474bd97` (the D-1 fix) makes the Linux worker listen
on `127.0.0.1:<free port>` (`connect/crates/layerfs-machine/src/platforms/docker/power.rs:63-66`)
and passes that port to `worker::ready` (`power.rs:113-114`). `ready`
first requires the guest to be listening on the fixed `WORKER_PORT` 7070
(`connect/crates/layerfs-machine/src/setup/worker.rs:141`:
`facts::listening(&out.stdout, WORKER_PORT)`, reading the container's
`/proc/net/tcp`), and only then tries the TCP connect (`worker.rs:143`). With
`--network host` nothing listens on 7070, so `guest_ready` stays false and
`ready` times out after `READY` = 60 s (`worker.rs:17`, `146-155`):
`Failed` "the worker of docker did not answer at 127.0.0.1:44845 within 60
s; its log ends: " (`docker-home.log`). Shown in `docker-diag.log` and
`docker-diag2.log`: the worker logs "listening on 127.0.0.1:44845",
`/proc/net/tcp` in its container has `0100007F:AF2D` (127.0.0.1:44845) in
LISTEN and no `:1B9E` (7070), and a connect to 127.0.0.1:44845 succeeds. The
empty log tail is a second, smaller fault on this route: `log_tail`
(`worker.rs:161-171`) reads `/layerfs/state/worker.log`, which the docker
worker never writes (its log goes to `docker logs`). The record keeps no
endpoint, so every later call says the machine is not running. The unit test
`machine_providers::docker_on_macos_and_linux_starts_one_worker_container_without_the_home`
checks the `docker run` argv only (passes on Linux), so it cannot see this.
The dial half of the fix is right: from inside the host-network worker a
container port published on 127.0.0.1 answers (`docker-diag2.log`). Fix
direction for the lead: check `listening(…, port)` with the port the worker
was told to listen on (the free port on Linux, 7070 otherwise).

## Item 9, machine power (`lima-power.log`, `fc-power.log`)

On each machine a throwaway sqlite Store `pw` was created and pushed there,
then `machine stop`; afterwards `machine list` (twice), `machine status` and
`store status pw` all answered and left it stopped: list and status
`"state":"stopped"` ("the machine is stopped"), `store status` `"state":"down"`,
"machine … is not running; start it with `layerfs machine start …`".
Independent checks: `limactl list` showed the lima VM `Stopped`; on the host
`lfs535l3h` (still running: a separate lima machine) 0 `firecracker`
processes and all 1,100 huge pages free. `machine start` (lima 12.3 s,
firecracker 7.8 s) brought each back (`running`, `store status pw` idle,
lima VM `Running`, 1 firecracker process, 86 huge pages in use); `pw` was
pulled back and removed. **Item 1's 12 sqlite cells ran right after this
stop/start on each machine**, so they also cover "the 12 cells pass after it".

## Item 4, Linux part (`item4.log`, `item4-compare.txt`, `compare4.py`)

In `lfs535l3x` with the L3 Linux build's `layerfs`, before Docker was
installed. `machine platforms` answered `host: linux` with the same seven
platforms as `SUPPORT.json`'s Linux rows and no difference in any static
field. Live fields: native and ssh ready; docker `missing: ["tool:docker"]`;
microsandbox `missing: ["tool:msb"]`; apple-container, lima, firecracker
`missing: ["host:macos", …]`. The three creates are refused `Unsupported`
with the table's text word for word (the row's `postgresql.reason`).

## Item 5 (`linux-tests.log`)

Tests built at `35ed671b6c` in the worktree's Linux target
(`tools/worktree.py cargo --platform linux -- test … --no-run`), each run
once in a fresh container of the build image with an ext4-backed `/tmp`
volume (`lt-exe.sh`, as K3), under a 120 s wall limit:
`named_sandboxes_on_native` ok (0.12 s), `a_sandbox_with_no_machine_runs_on_its_store_s`
ok (0.02 s), `sandbox_deploy` 1/1 ok (`the_daemon_and_the_linux_bundle_come_from_the_versioned_cache`,
whose Linux branch expects "lima runs on macOS hosts only"), `machine_providers`
13/13 ok. Kept in the log: one first invocation that named the test
`sandbox_family::named_sandboxes_on_native`, which matches nothing inside the
test binary (0 run, not a verdict).

## Attempts kept and why

- `linux-vm-apt-attempt1.log`: `apt-get install docker.io` failed when the
  host's proxy (192.168.5.2:15236) closed the connection mid-download (as
  K3's first attempt). The second run resumed the partial downloads and
  succeeded (`linux-vm-apt.log`). Not a product operation.
- No cell was rerun. The Linux docker row ran once, in one shared home whose
  docker machine was started by hand first (#535 D11, `dockerhome.sh`); the
  start failed (D-3) and the 24 cells were still run once each to record the
  verdict.

## Builds, machines and resources

- Frozen macOS build `k-macos` (product `35ed671b6c`) and the sealed bundle
  `764e4977…` with the Firecracker kit, read only; `LAYERFS_BUNDLE` named
  its `linux-aarch64` folder.
- **L3 Linux build** for the Linux rows: `tools/worktree.py build --platform
  linux --profile debug --jobs 4 --features layerfs-node/postgresql` at
  `35ed671b6c` (`build-linux-6eaf9dbb9ae94d778db06fc51534178e`, source sha256
  `d53a4e1b…`): `layerfs`
  `9b0f5db5efac30de0a496e75b8576aa73226b8f34e01c22ce53a2c29a8299877`,
  `layerfs.node`
  `a922f69c2a3b17153eff484795f13342fbb98d86d2af14651bf9ce1d4cbdb412`
  (item 4 and the docker home's `machine` calls). Its Node package files are
  identical to `k-macos`'s. The Linux cells used the sealed bundle's
  `layerfs-daemon`/`layerfs` copied into the VM at the default bundle path
  (hashes as in `bundle.json`; `firecracker/` left out) and Node v24.21.0
  (`0f8949d1…`, the binary K3 used).
- Shared Mac home (`$TMPDIR/lfs535l3-home.*`, 0700) holding `lfs535l3l`
  (lima, 2 CPUs, 4 GiB), `lfs535l3h` (lima `--nested`, 4 CPUs, 4 GiB, 1,100
  huge pages set by hand as #533) and `lfs535l3f` (firecracker on
  `lfs535l3h`, `--memory 2G`). `lfs535l3l` was stopped before `lfs535l3h`
  was created (at most two lane VMs at once: one machine VM plus the Linux VM).
  Observation: `machine create` records `disk` 20 GiB while `limactl list`
  shows the VMs' disk as 100 GiB.
- Linux VM `lfs535l3x`: Lima vz, `template:ubuntu-24.04` (24.04.5, kernel
  6.8.0-142), 4 CPUs, 4 GiB, 40 GiB, the staging folder mounted writable;
  Docker Engine 29.1.3 from apt `docker.io` (installed while the native
  PostgreSQL cells ran); `user.max_user_namespaces=0` before the native row
  (`sysctl-bundle.log`); the bundle's Sandbox image loaded as
  `sha256:1499b864…` (`docker-load.log`).
- PostgreSQL: `lfs535l3-pg` (`postgres:17-bookworm`, `0.0.0.0:55553`,
  generated password kept only in the scratch area, `log_connections=on`),
  one database per cell and Store kind (`lc01`…, `rlc01`…, `fc01`…,
  `rfc01`…, `rnc01`…, `rdc01`…). All 964 connections reached the server
  from `192.168.65.1` (Docker Desktop's proxy); the one FATAL is the
  deliberate `nosuchdb` check (`pg-server.log`).
- Cells overlapped at most two at a time (one on the Mac, one in the Linux
  VM); never two in one home and machine. The missing-database check ran in
  its own throwaway home during the lima loopback row.
- Disk (`tools/cleanup.py check`): **230.16 GB free at start, 219.33 GB at
  the end** (low point 173.68 GB during the run, other lanes active).
  1-minute load: 3.63 at start, 7.16 at the end (up to ~15 during the run).

## Cleanup (`cleanup.log`)

Removed: `lfs535l3f`, `lfs535l3h`, `lfs535l3l` (`machine remove`; `limactl
list` and `limactl disk list` show none of them), the Linux VM `lfs535l3x`
(`limactl delete -f`), its staging folder, `lfs535l3-pg` (`docker rm -fv`),
the Mac home, the Linux docker home with its worker container and volume.
Left on purpose: the L3 Linux build bundle
`<registration>/artifacts/build-linux-6eaf9dbb9ae94d778db06fc51534178e`
(565 MB, the Linux addon/CLI whose hashes are above) and the registration's
Linux target volume `lfs-dev-layerfs-issue535-verify-417967c19d8d-target`
(holds the item 5 test binaries); both are releasable to the cleanup script.
Not touched: `lfc-*`, `layerfs-postgres`, other lanes' resources, the
shared bundle.

## Files

Cell receipts `<platform>-<store>-<Cnn>.json` (`lfs533-cell/1`); `trace/`
(one `LAYERFS_TRACE` file per Mac cell); `progress/` (one line per cell as it
ran); `setup.log`, `lima.log`, `fc.log`, `lima-power.log`, `fc-power.log`,
`item4*.{log,txt}`, `linux-tests.log`, `docker-*.log`, `missing-db.log`,
`mount-main.log`, `pg-server.log`, `linux-vm-apt*.log`, `sysctl-bundle.log`,
`cleanup.log`; the runners `run.sh`, `run2.sh`, `cell.sh`, `seq.sh`,
`power.sh`, `fcproc.sh`, `mountmain.mjs` (Mac), `cellx.sh`, `seqx.sh`,
`dockerhome.sh`, `item4.sh`, `diag2.sh`, `compare4.py` (Linux VM),
`lt-exe.sh`, `lt2.sh` (item 5), `summarize.py` → `summary.md`, `traces.py`
→ `traces.txt`.
