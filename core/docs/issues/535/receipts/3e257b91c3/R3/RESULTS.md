# R3 results: Linux built-in docker rerun at `54161c9e2d`

Targeted rerun lane R3 of #535/#536, 2026-11-03 09:21–09:39 UTC, on this Mac
(arm64, 14 CPUs, Docker Desktop, Lima 2.2.1). It reruns only the checkpoints
that L3 defect D-3 (fixed in `3e257b91c3`) left unreached at `35ed671b6c`:
the Linux built-in docker machine's start, its list/get/status in both
states, the 24 Linux docker cells, the docker `LAYERFS_TRACE` line and the
worker container's `--network host`. Everything else of L3
(`../../35ed671b6c/L3/`) is unaffected evidence and is not rerun or relabelled.

Product commit `54161c9e2d` (production tree identical to `3e257b91c3`).
Disposable Stores only. Durable: NOT_RUN — disabled by owner until explicit
reauthorization. Debug builds; times are diagnostic, the verdict is PASS/FAIL.
One sample per cell; no cell rerun.

## Verdicts

| Item | Row | Result | First error |
| --- | --- | --- | --- |
| `machine start docker` (Linux Engine) | shared home, first start | **PASS** | – (0.67 s, `state: running`) |
| `machine start docker` after `machine stop` | same home | **PASS** | – (0.55 s) |
| `machine list` / `get` / `status docker` agree, stopped | before the first start, after `stop`, at teardown | **PASS 3/3** | – (all `stopped`; status "the machine is stopped") |
| `machine list` / `get` / `status docker` agree, running | after each start, before teardown | **PASS 3/3** | – (all `running`; status `down_reason: null`) |
| 1 sqlite cells | Linux `docker`, pushed native → docker and pulled back | **PASS 12/12** | – |
| 2 PostgreSQL cells | Linux `docker`, remote `192.168.2.123:55563` | **PASS 12/12** | – |
| `LAYERFS_TRACE`: one docker Store call writes one `worker.connection` line | `branch.list` on a Store pushed to docker | **PASS** | – (1 line, `op: store.call`, all six keys) |
| worker container runs `--network host` | `docker inspect` of `lfs-a2d62c12-docker` | **PASS** | – (`NetworkMode=host`) |

Totals: 24 cells, **24 PASS, 0 FAIL**. With L3's unaffected rows this closes
the Linux docker gap (L3: 0/24).

The PostgreSQL row uses only the remote URL form: inside the Linux VM a
loopback URL names the VM itself, which has no server (owner rule D12). There
is no second host, so the Mac's LAN address stands in for a remote server.

## L3 D-3 verdict: FIXED

`machine start docker` on a Linux Docker Engine answered `running` in 0.67 s
(`docker-home.log`), and again in 0.55 s after a `machine stop`
(`docker-stopstart.log`); at `35ed671b6c` it failed after 60 s. The worker was
told `LAYERFS_WORKER_LISTEN=127.0.0.1:45041` and logged "listening on
127.0.0.1:45041"; the readiness check now waits on that port
(`connect/crates/layerfs-machine/src/platforms/docker/power.rs:114-116`,
`setup/worker.rs:147`). After the restart the record's endpoint followed the
new free port (`127.0.0.1:38705` in every trace line), so the record keeps the
right endpoint across a stop/start.

Not exercised: the second, smaller D-3 fault (empty log tail on a docker
readiness failure, `worker.rs` `log_tail` reads `/layerfs/state/worker.log`;
the commit says it is not fixed). No readiness failure occurred, so it was not
reached.

## Fix checks reached here (from the k2 list)

| Fix | Verdict | Evidence |
| --- | --- | --- |
| Linux docker: worker runs `--network host` and dials its Sandboxes' published loopback ports (K3 D-1) | **PASS** | `docker-home.log`: `NetworkMode=host`, `LAYERFS_WORKER_HOST_NETWORK=1`; 24/24 cells mounted, ran and committed through Sandboxes on docker (`sandbox.host` trace lines in every cell). |
| `machine list`/`get` show the built-in docker's live state | **PASS** (both states, Linux) | `docker-home.log`, `docker-stopstart.log`, `docker-home-teardown.log`: the three always agree. |
| `LAYERFS_TRACE` worker-connection lines (item 6 R1) | **PASS** | `trace-check.log`; every cell's `trace/*.jsonl`: sqlite cells `store.file` 8, `sandbox.host` 6–7, `store.call` 3–4 (as L3's Mac lima/firecracker cells); PostgreSQL cells `sandbox.host` only (Store calls in process). R2 stays the recorded spec difference (exec goes through the worker). |

## Worker container argv (`docker-home.log`)

`docker inspect` of the worker, reconstructed as a `docker run` command (the
product's exact argv is not recorded by Docker; image-defined `ENV` entries
such as `NODE_VERSION` appear in `Config.Env` too; the
`LAYERFS_WORKER_SECRET` value is a file path, redacted by key name anyway):

```
docker run -d --name lfs-a2d62c12-docker --network host
  -v /var/run/docker.sock:/var/run/docker.sock
  -v lfs-a2d62c12-docker-data:/layerfs/data
  -v /root/.cache/layerfs/bundle/2.0.0/linux-aarch64:/layerfs/bundle   (ro)
  -e LAYERFS_WORKER_LISTEN=127.0.0.1:45041 -e LAYERFS_WORKER_SECRET=… -e LAYERFS_WORKER_HOST_NETWORK=1 …
  --entrypoint sh sha256:1499b864… -c '…wait for /layerfs/state/secret…; exec /layerfs/bundle/layerfs worker'
```

No `-p` publication: the host-network worker listens on the host loopback
directly.

## Builds and environment

- **R3 Linux build** at `54161c9e2d`: `tools/worktree.py build --platform linux
  --profile debug --jobs 4 --features layerfs-node/postgresql`
  (`build-linux-c4b95c1647044a038b878e1183db48fd`, source sha256
  `ae0480af…`, toolchain 1.85.1): `layerfs`
  `aeb528b7f51960266261f597144e6b1d6564ec242b8163c6613129e153ff4801`
  (the docker home's `machine` calls), `layerfs.node`
  `a3ac8ee0eee70432f1a524be3c9dcc4943dfe51dc06700f3035b7481d5a5e1c7`
  (the cells and `call.mjs`). Its Node package `src/` and `test/` are
  identical to `k-macos/node`'s. L3's `35ed671b6c` build was not used.
- Sealed bundle `185b0240…` (source `54161c9e2d`), read only; its
  `layerfs`, `layerfs-daemon`, `layerfs-sandbox-helper`, `sandbox-image.tar`
  and `bundle.json` copied into the VM at the default bundle path
  (`firecracker/` left out); hashes as in `bundle.json` (`layerfs`
  `93be8f81…`, `layerfs-daemon` `97f73b15…`). The cells' `import-env` and
  the worker use these. Sandbox image loaded as `sha256:1499b864…`
  (`docker-load.log`, the same id L3 loaded).
- Node v24.21.0 (`0f8949d1…`, the binary K3 and L3 used).
- Linux VM `lfs535r3x`: Lima vz, `template:ubuntu-24.04` (kernel
  6.8.0-142), 4 CPUs, 4 GiB, 40 GiB, the staging folder mounted writable
  (`linux-vm-create.log`); Docker Engine 29.1.3 from apt `docker.io`. The
  only lane VM; at most one at a time. `user.max_user_namespaces` left at the
  default (L3 set it to 0 for its native row only, which is not rerun here).
- One shared home `/root/lfs535r3-dockerhome` (0700) whose docker machine was
  started by hand (#535 D11, `dockerhome.sh`); the 24 cells ran in it one at a
  time with `LFS533_HOME` (sqlite 09:34:00–09:35:25, PostgreSQL
  09:35:44–09:37:30, then the trace check), never two at once.
- PostgreSQL: `lfs535r3-pg` (`postgres:17-bookworm`, `0.0.0.0:55563`,
  generated password kept only in the scratch area, `log_connections=on`),
  databases `rdc01`…`rdc12`. All 183 TCP connections came from
  `192.168.65.1` (Docker Desktop's proxy); no FATAL or ERROR
  (`pg-server.log`).
- Disk (`tools/cleanup.py check`): **225.44 GB free at start, 224.24 GB at
  the end**. 1-minute load: 5.64 at start, 3.52 at the end (Mac); inside the
  VM 0.24–1.06 during the cells.

## Attempts kept and why

Not product operations; no product call was repeated.
- `linux-vm-apt-attempt1.log`: `apt-get install docker.io` before any
  `apt-get update` (no candidate: the lists were not yet fetched).
- `linux-vm-apt-attempt2.log`: `apt-get update` refused the lists lock held
  by cloud-init's own `apt-get update` (the VM was still provisioning), then
  no candidate again.
- `linux-vm-apt-attempt3.log`: after cloud-init's apt finished, the update
  worked but the host proxy (192.168.5.2:15236) closed the connection
  mid-download of `containerd` (as L3's and K3's first attempts). The fourth
  run resumed and succeeded (`linux-vm-apt.log`).
- `linux-vm-create.log`: `limactl start` returned 0.
- `docker-load.log`: `docker images -q` lists no untagged image on this
  Engine, so the first `inspect` had no argument; the id was then taken from
  the `docker load` line and inspected.

## Cleanup (`cleanup.log`)

Removed: the VM `lfs535r3x` (`limactl delete -f`; `limactl list`/`disk
list` show no `lfs535r3*`), inside it before that the worker container
`lfs-a2d62c12-docker`, its volume `lfs-a2d62c12-docker-data` and the home
(`docker-home-teardown.log`: `docker ps -a` and `docker volume ls` empty);
the PostgreSQL container `lfs535r3-pg` (`docker rm -fv`); the scratch staging
folder. Left on purpose: the R3 Linux build bundle
`<registration>/artifacts/build-linux-c4b95c1647044a038b878e1183db48fd`
(the binaries whose hashes are above), releasable to the cleanup script. Not
touched: `lfc-*`, `layerfs-postgres`, other lanes' resources, the shared
bundle.

## Files

Cell receipts `linux-docker-<sqlite|pg-remote>-<Cnn>.json`
(`lfs533-cell/1`, `product: 54161c9e2d`); `trace/` (one `LAYERFS_TRACE`
file per cell, plus `linux-docker-tracecheck.jsonl`); `progress/` (one line
per cell as it ran); `summary.md` (by `summarize.py`); `docker-home.log`,
`docker-stopstart.log`, `docker-home-teardown.log`, `trace-check.log`,
`docker-load.log`, `pg-server.log`, `linux-vm-*.log`, `cleanup.log`; the
runners `cellx.sh`, `seqx.sh`, `dockerhome.sh`, `tracecheck.sh`, `call.mjs`
(adapted from L3's).
