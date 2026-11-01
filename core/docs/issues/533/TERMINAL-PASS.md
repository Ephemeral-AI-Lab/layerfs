# #533 step 19: terminal pass on this Mac

> **Status:** Dated checkpoint, 2026-11-02; not release evidence or a product
> contract. Delegated by the owner (asleep); nothing is pushed or merged.

Run on branch `issue533-v2` at product commit `11551a8d4f` (the twelve-cell
runner commit; the product source is `6775d06789`'s). Host: this Mac (arm64,
14 CPUs, 36 GiB, macOS Darwin 25.4.0), Docker 29.5.2. All binaries are
**debug** builds (`cargo +1.85.1`, `target/debug`), so every time below is
diagnostic only: it is not a measurement and is not compared with #529's
release numbers. One sample per cell; the 1-minute load is in each receipt.
Store profile: Disposable only. Durable: NOT_RUN — disabled by owner until
explicit reauthorization.

Receipts: [`receipts/11551a8d4f/`](receipts/11551a8d4f/), one JSON per cell
(`lfs533-cell/1`), written by `api/node/test/verify/cells/cell.mjs`.

**Rerun at product commit `a487409fe5`** (owner instruction, 2026-11-02:
build the `docker` and PostgreSQL Sandbox paths before step 19). The rows
below carry the rerun's verdict where it changed; the first run's rows are
kept in the section after the table. Receipts:
[`receipts/a487409fe5/`](receipts/a487409fe5/).

## Result

| Step-19 item (README §6, step 19) | Verdict | Evidence |
| --- | --- | --- |
| The 12 #516 cells on `native` (macOS) | **PASS 12/12**, at `11551a8d4f` and again at `a487409fe5` | `macos-native-C01…C12` below |
| The 12 cells on `docker` (macOS) | **PASS 12/12** at `a487409fe5` | The Store moved to `docker`, the Sandbox started by the docker worker beside itself (private test bundle at `LAYERFS_BUNDLE`, see below). At `11551a8d4f` it was refused `Unsupported` (`receipts/11551a8d4f/macos-docker-C01.json`). |
| The 12 cells on `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | **NOT_RUN** | At `11551a8d4f` there was no sealed Linux bundle (O16). At `a487409fe5` a private test bundle exists (below), and `limactl` and `container` are installed, but the VM runs were not part of the owner's instruction, and creating a VM downloads and boots an OS image for longer than the 2-minute command rule allows. `msb` is not installed. No ssh host. |
| Two Lima machines of different sizes, two Stores each, at once | **NOT_RUN** | Same: no sealed Linux bundle. |
| Init on `native`, moves to `docker` and one VM, and back | **PARTLY**: Init on native and the move to `docker` PASS; the VM and the move back NOT_RUN | At `a487409fe5` every docker cell and `cli-docker-sqlite.txt` move their Store to `docker` through the real docker worker. A VM: NOT_RUN (see the VM row). Back to `native`: not run. |
| Backup to and restore from MinIO, `backup list` | **PASS** | `tests/minio_service.sh start lfs533-minio`, then `cargo test -p layerfs-api --features machine --test backup_family` 1/1 live (create, list states, restore, a worker Store); `cargo test -p layerfs-remote-storage --test minio -- --include-ignored` 9/9 (after the step-17 `from_env` move). MinIO stopped and removed after. |
| Backup to and restore from R2 | **NOT_RUN** | No R2 account or credentials on this host. |
| Init into PostgreSQL | **PASS** | Throwaway `postgres:17-bookworm` (`lfs533-pg-main`): `layerfs store connect pg postgres://postgres@127.0.0.1:…/lfs3 --password env:LFSPW` made the `layerfs` schema; `layerfs project init pg/cli <folder> --progress` in 227 ms (debug) with one progress line; `file list` and `store status` (reachable, server 17.11) answered; no password in any home file. `cargo test … --test postgres_connect` 1/1 on two fresh databases. Needs a build with `--features postgresql` (off by default, as in 1.1.0). |
| `docker` Sandboxes on the PostgreSQL Store | **PASS** at `a487409fe5` | `cli-pg-docker.txt`: connect, Init, a Sandbox on `docker` whose daemon dials the server as `host.docker.internal`, fork, mount, exec, Commit into PostgreSQL, read back, close; no password in the home. Also `api-sandbox_machines.txt`. At `11551a8d4f` it was refused. |
| `native` Sandboxes on the PostgreSQL Store (O3) | **PASS** at `a487409fe5` | `cli-pg-native.txt`, the same steps on `native` with a daemon built with `postgresql`. |
| A Mac with Docker stopped: `native` still works | **PASS (simulated)** | Docker was not stopped (other sessions use it). C01 on `native` ran with no `docker` on `PATH` and `DOCKER_HOST` at a missing socket: PASS (`macos-native-nodocker-C01`). The VM kinds' half is NOT_RUN (no bundle). |
| Machines: none running while idle; one started by the first call; an agent creating and removing a machine | **PARTLY** | `machine list` in a fresh home: only `native` and `docker`, no worker running. The idle stop passed in `worker_idle` (step 13, in-process worker). At `a487409fe5` the docker worker was started by the first call that needed it (`store move … docker`). Starting a VM on the first call and an agent creating and removing one: NOT_RUN. |
| The same calls from the binary, Node and Python, compared | **PASS** | At this commit: `node --test api/node/test/calls.mjs test/shell.mjs` 5/5 (all 57 calls refused identically by the command and Node; a live Store read both ways, identical JSON) and `python3 api/python/test/test_calls.py` 4/4 (all 57 calls and the live reads identical to the command; `layerfs.aio`). |
| Linux host (`native`, `docker`) | **NOT_RUN** | No Linux or ssh host is available to this pass. |
| Windows | **NOT_RUN** | Not run (O5). |

## The rerun at `a487409fe5`

Host as above; Docker 29.5.2; one sample per cell; debug builds, so times
are diagnostic only. The guest binaries come from the launcher's sealed
Linux build `build-linux-0c84a1ae4ed4` (head `a487409fe5`, features
`layerfs-daemon/postgresql`), copied with a hand-written `bundle.json` into
a private folder named by `LAYERFS_BUNDLE`; nothing was placed in the user
cache. The docker machine's image was the local Sandbox image
`sha256:afa1ef9c…f38e`. Each docker cell removed its own worker container
and data volume. The Node package was run with the addon rebuilt at the
commit. A first `macos-docker-C01` attempt failed at `store.move` ("the
docker machine's worker container needs an image") because the package
loaded a stale addon from before `a487409fe5`; a second attempt was killed
by macOS because the new addon was copied over the loaded file. Both were
harness setup faults, not product outcomes; the receipt path was reused by
the run below, so their receipts are not kept.

| Receipt | Verdict | Mount | Exec | Commit (kind) | Unmount | Load |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| macos-docker-C01 | PASS | 14.4 | 682.2 | 552.1 (Committed) | 9.4 | 4.7 |
| macos-docker-C02 | PASS | 18.1 | 1246.8 | 553.8 (Committed) | 11.2 | 4.1 |
| macos-docker-C03 | PASS | 13.4 | 929.4 | 26.4 (UpToDate) | 8.0 | 4.3 |
| macos-docker-C04 | PASS | 14.6 | 975.5 | 604.0 (Committed) | 9.1 | 4.1 |
| macos-docker-C05 | PASS | 14.5 | 987.0 | 596.2 (Committed) | 9.9 | 3.9 |
| macos-docker-C06 | PASS | 15.9 | 1074.5 | 5180.9 (Committed) | 19.3 | 3.7 |
| macos-docker-C07 | PASS | 18.8 | 2084.9 | 10337.1 (Committed) | 25.5 | 4.1 |
| macos-docker-C08 | PASS | 16.9 | 1102.3 | 5192.1 (Committed) | 16.8 | 3.6 |
| macos-docker-C09 | PASS | 21.9 | 147.2 | 7.4 (UpToDate) | 10.8 | 4.1 |
| macos-docker-C10 | PASS | 21.6 | 1135.6 | 171.3 (Committed) | 21.0 | 4.5 |
| macos-docker-C11 | PASS | 21.4 | 259.7 | 1460.6 (UpToDate) | 18.0 | 4.5 |
| macos-docker-C12 | PASS | 14.4 | 381.1 | 247.3 (Committed) | 9.2 | 4.4 |
| macos-native-C01 | PASS | 25.6 | 1534.8 | 674.3 (Committed) | 47.8 | 3.8 |
| macos-native-C02 | PASS | 26.4 | 7526.2 | 672.6 (Committed) | 49.1 | 3.8 |
| macos-native-C03 | PASS | 26.1 | 2656.3 | 136.1 (UpToDate) | 25.5 | 3.1 |
| macos-native-C04 | PASS | 26.6 | 5503.6 | 703.1 (Committed) | 49.8 | 2.9 |
| macos-native-C05 | PASS | 26.8 | 5521.4 | 707.4 (Committed) | 44.4 | 3.3 |
| macos-native-C06 | PASS | 25.9 | 1762.9 | 5551.5 (Committed) | 26.6 | 3.0 |
| macos-native-C07 | PASS | 26.1 | 3471.3 | 10937.9 (Committed) | 32.7 | 3.1 |
| macos-native-C08 | PASS | 28.6 | 1768.4 | 5528.5 (Committed) | 30.4 | 2.9 |
| macos-native-C09 | PASS | 28.9 | 97.3 | 117.9 (UpToDate) | 26.4 | 2.7 |
| macos-native-C10 | PASS | 29.7 | 1851.5 | 317.1 (Committed) | 23.1 | 2.9 |
| macos-native-C11 | PASS | 25.5 | 654.5 | 1588.3 (UpToDate) | 27.3 | 3.0 |
| macos-native-C12 | PASS | 28.7 | 763.4 | 351.3 (Committed) | 31.8 | 2.9 |

Each cell ran as one command (`$S` is the private bundle's folder):

```bash
cd api/node && LFS533_DAEMON=$PWD/../../target/debug/layerfs-daemon \
  LFS533_IMAGE=sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e \
  LAYERFS_BUNDLE=$S/bundle perl -e 'alarm 100; exec @ARGV' \
  node test/verify/cells/cell.mjs C01 docker ../../core/docs/issues/533/receipts/a487409fe5/macos-docker-C01.json
```

Beside the cells, at the same commit:

- `api-sandbox_machines.txt`: `cargo test -p layerfs-api --features
  machine,postgresql --test sandbox_machines`, 3/3 live (PostgreSQL
  Sandboxes on `native` and `docker`, a sqlite Store's Sandbox on the
  docker machine, the refusals).
- `cli-pg-native.txt`, `cli-pg-docker.txt`, `cli-docker-sqlite.txt`: the
  same paths through the `layerfs` command, one fresh home and database
  each.

Afterwards no `lfs-*` container or worker volume and no test home remained;
the PostgreSQL container was stopped (`--rm`).

## The twelve cells on `native` at `11551a8d4f`

Times in ms (debug build), from each receipt's timed phase. Commit kinds match
#529's `mac` row: C03, C09 and C11 are `UpToDate`.

| Receipt | Verdict | Mount | Exec | Commit (kind) | Unmount | Load |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| macos-native-C01 | PASS | 25.0 | 1437.8 | 681.8 (Committed) | 47.5 | 2.9 |
| macos-native-C02 | PASS | 33.0 | 7418.9 | 661.0 (Committed) | 47.0 | 2.9 |
| macos-native-C03 | PASS | 26.0 | 2768.9 | 98.9 (UpToDate) | 23.5 | 2.9 |
| macos-native-C04 | PASS | 26.1 | 4946.4 | 692.7 (Committed) | 42.5 | 2.9 |
| macos-native-C05 | PASS | 31.2 | 5086.4 | 695.1 (Committed) | 50.9 | 4.2 |
| macos-native-C06 | PASS | 26.2 | 1756.9 | 5633.2 (Committed) | 26.3 | 4.1 |
| macos-native-C07 | PASS | 32.7 | 3501.0 | 11060.9 (Committed) | 28.5 | 3.8 |
| macos-native-C08 | PASS | 31.1 | 1788.5 | 5616.3 (Committed) | 31.4 | 4.0 |
| macos-native-C09 | PASS | 25.5 | 93.0 | 114.4 (UpToDate) | 30.6 | 3.8 |
| macos-native-C10 | PASS | 33.0 | 1895.3 | 314.6 (Committed) | 26.9 | 3.6 |
| macos-native-C11 | PASS | 31.7 | 670.9 | 1578.5 (UpToDate) | 26.4 | 3.8 |
| macos-native-C12 | PASS | 25.5 | 769.5 | 357.8 (Committed) | 27.5 | 3.8 |
| macos-native-nodocker-C01 | PASS | 26.2 | 1482.3 | 679.9 (Committed) | 53.6 | 5.0 |
| macos-docker-C01 | FAIL (refused at `sandbox.create`) | – | – | – | – | 3.3 |

Each cell ran as one command in a fresh home:

```bash
cd api/node && LFS533_DAEMON=$PWD/../../target/debug/layerfs-daemon \
  perl -e 'alarm 100; exec @ARGV' node test/verify/cells/cell.mjs C01 native ../../core/docs/issues/533/receipts/11551a8d4f/macos-native-C01.json
```

After the cells: no NFS mount and no `layerfs-daemon` process remained.

## What this pass does not show

- Any VM kind or ssh: none ran. The docker machine's worker ran at
  `a487409fe5` from a private test bundle; no installer places a bundle in
  the user cache yet (O16).
- Anything on Linux: `remote_store` and the namespaces backend are Linux-only
  and did not build or run here.
- Release-build timings: every number above is from a debug build.
