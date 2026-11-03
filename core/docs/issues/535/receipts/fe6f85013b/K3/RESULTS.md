# K3 results: lima, firecracker and the Linux rows at `fe6f85013b`

Lane K3 of the #535/#536 terminal pass, 2026-11-03 07:17–07:46 UTC, on this
Mac (arm64, 14 CPUs, macOS 26.4.1, Docker Desktop 29.5.2, Lima 2.2.1). One
product commit, `fe6f85013b`; Disposable Stores only. Durable: NOT_RUN —
disabled by owner until explicit reauthorization. All binaries are debug
builds, so every time in a receipt is diagnostic; the verdict is PASS/FAIL.
One sample per cell. `summary.md` (made by `summarize.py`) tabulates every
receipt with its times and 1-minute load.

## Verdicts

| Item | Platform / row | Store | Result | First error |
| --- | --- | --- | --- | --- |
| 1 sqlite cells | lima `lfs535k3l` | sqlite, pushed out and pulled back | **PASS 12/12** | – |
| 1 sqlite cells | firecracker `lfs535k3f` | sqlite, pushed out and pulled back | **PASS 12/12** | – |
| 1 sqlite cells | Linux `native` (in `lfs535k3x`) | sqlite | **PASS 12/12** | – |
| 1 sqlite cells | Linux `docker` (in `lfs535k3x`) | sqlite | **FAIL 0/12** | `sandbox.create`: `Failed sandbox create stopped at Connect: … Connection refused` (product defect D-1 below) |
| 2 PostgreSQL cells | lima | loopback `127.0.0.1:55543` (rewritten to 192.168.5.2) | **PASS 12/12** | – (attempt 1, a harness fault, kept: see below) |
| 2 PostgreSQL cells | lima | remote `192.168.2.123:55543` | **PASS 12/12** | – |
| 2 PostgreSQL cells | firecracker | loopback (rewritten to 192.168.5.2) | **PASS 12/12** | – |
| 2 PostgreSQL cells | firecracker | remote | **PASS 12/12** | – |
| 2 PostgreSQL cells | Linux `native` | remote | **PASS 12/12** | – |
| 2 PostgreSQL cells | Linux `docker` | remote | **FAIL 0/12** | `sandbox.create`: the same Connect refusal (D-1) |
| 4 (Linux part) platforms vs `SUPPORT.json` | Linux | – | **PASS** | 7/7 platforms, 0 differences in every static field |
| 4 (Linux part) create refusals word for word | Linux | – | **FAIL** | refused `Unsupported` "lima cannot be created on this host", not the table's "lima runs on macOS hosts only" (D-2 below) |
| 9 machine power | lima | – | **PASS** | – |
| 9 machine power | firecracker | – | **PASS** | – |
| 5 Linux rerun of `b772e77f97`'s two tests | Linux container (Docker Desktop) | – | **PASS 2/2** | – |

**Lima's committed route (`31619094f4`, 192.168.5.2) is proven as committed
here for the first time**: the 12 loopback PostgreSQL cells passed on lima
with the frozen build, no local lift or diagnostic edit (G7 needed one).

The Linux rows use only the remote URL form: inside the Linux VM a loopback
URL names the VM itself, which has no server (owner rule D12: no PostgreSQL
in any VM). There is no second host, so the Mac's LAN address
`192.168.2.123:55543` stands in for a remote server.

Totals of the final attempts: 120 cells owned (lima 36, firecracker 36,
Linux native 24, Linux docker 24): **96 PASS, 24 FAIL** (all Linux docker).

## Product defects found (not changed here)

**D-1. Linux `docker`: the docker machine's worker cannot reach its
Sandboxes' daemons.** Every Linux docker Sandbox (sqlite and PostgreSQL)
goes through the docker machine's worker container (G8,
`api/crates/layerfs-control/src/sandbox/deploy.rs:149-170`), which is told
to dial the Sandbox's published port at `daemonHost` =
`host.docker.internal` (`deploy.rs:165-169`). On a Linux Engine the worker
container resolves that name to the bridge gateway through
`--add-host host.docker.internal:host-gateway`
(`connect/crates/layerfs-machine/src/platforms/docker/power.rs:67-72`), but
the Sandbox container publishes its daemon port on `HostIp 127.0.0.1` only
(`connect/crates/layerfs-sandbox/src/backend/docker/create.rs:70`), so the
dial is refused. Docker Desktop forwards `host.docker.internal` to the
host's loopback, which is why macOS docker works. Shown directly in
`docker-diag.log`: the worker's own 127.0.0.1-published port answers from
the VM host and is refused from inside the worker at
`host.docker.internal` (172.17.0.1). #533 step 19's Linux docker row passed
at `6837bcfce2`, before G8 routed every sqlite docker Sandbox (and #535 5a
every PostgreSQL one) through the worker. Fix options for the lead: publish
on the gateway (or `0.0.0.0` with a firewall rule), run the worker with
`--network host` on Linux, or have the worker dial the Sandbox container's
bridge address.

**D-2. `machine create` on Linux does not give the support table's text.**
`machine create <n> lima|apple-container|firecracker` on Linux is refused
`Unsupported` with `"<platform> cannot be created on this host"` and
`detail.missing: ["host:macos", "tool:…"]`
(`api/crates/layerfs-control/src/machine/create.rs:68-73`: the probe's
missing list is checked before `support.rs`'s `host_refusal`). The table
(`connect/crates/layerfs-machine/src/support.rs:76-78`, `SUPPORT.json`)
says "lima runs on macOS hosts only", "apple-container runs on macOS hosts
only", "firecracker runs on macOS hosts only, in a nested lima machine".
The refusal code and meaning agree; the text is not the table's, word for
word. `api/crates/layerfs-api/tests/sandbox_deploy.rs:101` asserts the
current text.

Smaller observations: on Linux, `machine list`/`machine update` show the
docker machine `"state":"ready"` while `machine status docker` says
`"stopped"` ("the machine is stopped") until `machine start docker`
(`docker-home.log`). A PostgreSQL connect to a database that does not exist
is reported as `Persistence(Refused { status: "Closed" })`, not "database
… does not exist" (lima attempt 1; the server logged the FATAL).

## Item 9, machine power (`lima-power.log`, `fc-power.log`)

On each machine a throwaway sqlite Store `pw` was created and pushed there,
then `machine stop`; afterwards `machine list` (twice), `machine status` and
`store status pw` all answered and left it stopped: list and status
`"state":"stopped"`, `store status` `"state":"down"`, "machine … is not
running; start it with `layerfs machine start …`". Independent checks:
`limactl list` showed the lima VM `Stopped`; no `firecracker` process on
the host `lfs535k3h` (which stays running: it is a separate lima machine).
`machine start` (lima 11.9 s, firecracker 8.3 s) brought each back
(`running`, `store status pw` idle, firecracker process back, 84 huge pages
in use); `pw` was pulled back and removed. **Item 1's 12 sqlite cells ran
right after this stop/start on each machine**, so they also cover "the 12
cells pass after it".

## Item 4, Linux part (`item4.log`, `item4-compare.txt`)

In `lfs535k3x` with the K3 Linux build's `layerfs` (the sealed bundle's
`layerfs` is the guest worker build without the machine calls: every
machine call answered "not built yet (#533 step 6)",
`item4-attempt1-bundle-cli.log`). `machine platforms` answered `host:
linux` with the same seven platforms as `SUPPORT.json`'s Linux rows and no
difference in `how`, `backend`, `kernel`, `sqlite`, `postgresql`,
`options`, `description`. Live fields: native and ssh ready; docker
`missing: ["tool:docker"]` (run before Docker was installed); microsandbox
`missing: ["tool:msb"]`; apple-container, lima, firecracker `missing:
["host:macos", …]`. The three creates: D-2 above.

## Item 5 (`linux-tests.log`)

`layerfs-api --features machine`, tests built at `fe6f85013b` in the
worktree's Linux target (`tools/worktree.py cargo --platform linux … --no-run`):
`sandbox_family::named_sandboxes_on_native` and
`sandbox_machines::a_sandbox_with_no_machine_runs_on_its_store_s` each
**ok** (0.11 s and 0.00 s; under 2 s wall each) in a fresh container of the
build image with an ext4-backed `/tmp` volume (`lt-exe.sh`, the verify
lane's runner). Kept in the log: two first invocations that never ran a test
(a zsh word-splitting fault in my driver), and one launcher run where
`sandbox_machines` passed and `sandbox_family` failed at `project.init`
because the launcher container's `/tmp` is overlayfs, which the direct
Store refuses by design ("does not carry SQLite locks between kernels").

## Attempts kept and why

- `lima-pg-loop-*-attempt1-wrongdb` (12 FAIL, `store.connect`): my runner
  built the database name `lcc01` (prefix `lc` + `{cell}` = `c01`) instead
  of `lc01`; the server logged `database "lcc01" does not exist`. Harness
  fault; rerun with the right prefix: 12/12 PASS.
- `linux-docker-*-attempt1-not-started` (24 FAIL): each cell made a fresh
  home and never started its docker machine; refused `Unavailable` "machine
  docker is not running; start it with `layerfs machine start docker`",
  which is #535 D11 (no call starts a machine). The #533 runner predates
  D11. Attempt 2 used one shared home whose docker machine was started by
  hand (`dockerhome.sh`, `docker-home.log`), cells with `LFS533_HOME`.
- `linux-docker-sqlite-*-attempt2` (12 FAIL): D-1.
- `linux-docker-pg-remote-*-attempt2` (12 FAIL): C01–C09 found the
  attempt-1 databases (renamed while this run started: my race) already
  holding project `p`, C10 hit the rename, C11–C12 hit D-1. Harness fault
  for C01–C10, rerun on fresh databases as `-attempt3` (10 FAIL, D-1). So
  the Linux docker PostgreSQL row's final evidence is attempt 3 C01–C10
  plus attempt 2 C11–C12: 12 FAIL, all D-1.

## Builds, machines and resources

- Frozen macOS build `k-macos` (CLI `5259d573…`, Node addon `be6c7e5d…`)
  and the sealed bundle `a1f0d886…` with the Firecracker kit, read only;
  `LAYERFS_BUNDLE` named it.
- **K3 Linux build** for the Linux rows: `tools/worktree.py build
  --platform linux --profile debug --jobs 4 --features
  layerfs-node/postgresql` at `fe6f85013b` (`build-linux-59fd4d40f265…`,
  source sha256 `af44808a…`): `layerfs.node`
  `c56e8d19f9a2f85ad9a0f4740c1e4c7c3a6eb67f64a74d52c086369a19af176b`,
  `layerfs` `78879f778fdb256a9e93dbf3dcd7c02c043cbedfaab625489987a2ea676a330e`
  (used for item 4 and the docker home's `machine` calls). Its Node package
  files (`src/`, the cell runner) are identical to `k-macos`'s. The Linux
  cells used the sealed bundle's `layerfs-daemon`/`layerfs` (copied into the
  VM at the default bundle path, hashes as in `bundle.json`; `firecracker/`
  left out) and Node v24.21.0 (`0f8949d1…`) taken from the local image
  `layerfs-352-runtime:node24-https-apt-20261024`.
- Shared Mac home (`$TMPDIR/lfs535k3-home.*`, 0700) holding `lfs535k3l`
  (lima, 2 CPUs, 4 GiB), `lfs535k3h` (lima `--nested`, 4 CPUs, 4 GiB, 1,100
  huge pages set by hand as #533) and `lfs535k3f` (firecracker on
  `lfs535k3h`, `--memory 2G`). `lfs535k3l` was stopped before `lfs535k3h`
  started (at most two lane VMs at once).
- Linux VM `lfs535k3x`: Lima vz, `template:ubuntu-24.04` (24.04.5, kernel
  6.8.0-142), 4 CPUs, 4 GiB, 40 GiB; Docker Engine 29.1.3 from apt
  `docker.io` (the first `apt-get install` failed when the mirror closed the
  connection; the second succeeded, `linux-vm-apt.log`);
  `user.max_user_namespaces=0` before the native row (`sysctl.log`); the
  bundle's Sandbox image loaded as `sha256:1499b864…` (config `1d91885d…`).
- PostgreSQL: `lfs535k3-pg` (`postgres:17-bookworm`, `0.0.0.0:55543`,
  generated password kept only in the scratch area, `log_connections=on`),
  one database per cell and Store kind (`lc01`…, `rlc01`…, `fc01`…,
  `rfc01`…, `rnc01`…, `rdc01`…, `rd3c01`…). Every connection reached the
  server from `192.168.65.1` (Docker Desktop's proxy), `pg-server.log`.
- Cells overlapped at most two at a time (one on the Mac, one in the Linux
  VM); never two in one home and machine.
- Disk (`tools/cleanup.py check`): **259.38 GB free at start, 249.52 GB at
  the end**. 1-minute load: 4.42 at start, 6.57 at the end (11–21 during the
  Mac cells; other lanes were running).

## Cleanup (`cleanup.log`)

Removed: `lfs535k3f`, `lfs535k3h`, `lfs535k3l` (`machine remove`; `limactl
list` and `limactl disk list` show none), the Linux VM `lfs535k3x`
(`limactl delete -f`), `lfs535k3-pg` (`docker rm -fv`; nothing listens on
55543), the Mac home, the Linux docker home with its worker container and
volume. Left on purpose: the K3 Linux build bundle
`<registration>/artifacts/build-linux-59fd4d40f2654632b96b2cf296f1a141`
(567 MB, the Linux addon whose hashes are above) and the registration's
Linux target volume `lfs-dev-layerfs-issue535-verify-417967c19d8d-target`
recreated by that build; both are releasable to the cleanup script.
Not touched: `lfc-*`, `layerfs-postgres`, the other lanes' resources, the
shared bundle.

## Files

Cell receipts `<platform>-<store>-<Cnn>[-attemptN…].json`
(`lfs533-cell/1`); `progress/` (one line per cell as it ran); `setup.log`,
`lima.log`, `fc.log`, `lima-power.log`, `fc-power.log`, `item4*.{log,txt}`,
`linux-tests.log`, `docker-*.log`, `pg-server.log`, `cleanup.log`; the
runners `run.sh`, `cell.sh`, `seq.sh` (Mac), `cellx.sh`, `seqx.sh`,
`dockerhome.sh`, `item4.sh` (Linux VM), `lt.sh`, `lt2.sh`, `lt-exe.sh`
(item 5), `summarize.py` → `summary.md`.
