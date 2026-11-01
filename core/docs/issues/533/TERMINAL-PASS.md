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

## Result

| Step-19 item (README §6, step 19) | Verdict | Evidence |
| --- | --- | --- |
| The 12 #516 cells on `native` (macOS) | **PASS 12/12** | `macos-native-C01…C12` below |
| The 12 cells on `docker` (macOS) | **FAIL as built**: refused | `macos-docker-C01`: `sandbox.create` answers `Unsupported` (step 12 did not build Sandboxes on the docker machine; DECISIONS). C02–C12 not run: the same refusal. |
| The 12 cells on `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | **NOT_RUN** | No sealed Linux bundle (O16): `machine create dev lima` and `store move shop docker` both answer `Unsupported … no sealed Linux bundle at ~/.cache/layerfs/bundle/1.1.0`. No bundle builder exists (O16 says a build places it), and a cold Linux build of the guest binaries exceeds the 2-minute command rule (5 in an extreme case). `microsandbox` is also missing its tool (`machine list`: `missing: tool:msb`). |
| Two Lima machines of different sizes, two Stores each, at once | **NOT_RUN** | Same: no sealed Linux bundle. |
| Init on `native`, moves to `docker` and one VM, and back | **PARTLY**: Init on native PASS (every cell); live moves NOT_RUN | `store move shop docker` refused (no bundle, phase `check`). The move protocol itself (snapshot, send, verify heads, install, record) passed earlier against an in-process worker (`store_family`, step 8). |
| Backup to and restore from MinIO, `backup list` | **PASS** | `tests/minio_service.sh start lfs533-minio`, then `cargo test -p layerfs-api --features machine --test backup_family` 1/1 live (create, list states, restore, a worker Store); `cargo test -p layerfs-remote-storage --test minio -- --include-ignored` 9/9 (after the step-17 `from_env` move). MinIO stopped and removed after. |
| Backup to and restore from R2 | **NOT_RUN** | No R2 account or credentials on this host. |
| Init into PostgreSQL | **PASS** | Throwaway `postgres:17-bookworm` (`lfs533-pg-main`): `layerfs store connect pg postgres://postgres@127.0.0.1:…/lfs3 --password env:LFSPW` made the `layerfs` schema; `layerfs project init pg/cli <folder> --progress` in 227 ms (debug) with one progress line; `file list` and `store status` (reachable, server 17.11) answered; no password in any home file. `cargo test … --test postgres_connect` 1/1 on two fresh databases. Needs a build with `--features postgresql` (off by default, as in 1.1.0). |
| `docker` Sandboxes on the PostgreSQL Store | **FAIL as built**: refused | `sandbox create pg/cli t docker` answers `Unsupported: a Sandbox of a PostgreSQL Store is not built in 2.0 yet` (O3 needs the docker machine's Sandbox path). |
| A Mac with Docker stopped: `native` still works | **PASS (simulated)** | Docker was not stopped (other sessions use it). C01 on `native` ran with no `docker` on `PATH` and `DOCKER_HOST` at a missing socket: PASS (`macos-native-nodocker-C01`). The VM kinds' half is NOT_RUN (no bundle). |
| Machines: none running while idle; one started by the first call; an agent creating and removing a machine | **PARTLY** | `machine list` in a fresh home: only `native` and `docker`, no worker running. The idle stop passed in `worker_idle` (step 13, in-process worker). Starting a VM on the first call and an agent creating and removing one: NOT_RUN (no bundle). |
| The same calls from the binary, Node and Python, compared | **PASS** | At this commit: `node --test api/node/test/calls.mjs test/shell.mjs` 5/5 (all 57 calls refused identically by the command and Node; a live Store read both ways, identical JSON) and `python3 api/python/test/test_calls.py` 4/4 (all 57 calls and the live reads identical to the command; `layerfs.aio`). |
| Linux host (`native`, `docker`) | **NOT_RUN** | No Linux or ssh host is available to this pass. |
| Windows | **NOT_RUN** | Not run (O5). |

## The twelve cells on `native`

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

- Any VM kind, ssh, or the docker machine's worker container on macOS: none
  ran, for want of a sealed Linux bundle. The machine adapters (step 5) have
  unit tests only; their live behaviour in 2.0 is unverified.
- Sandboxes on the docker machine and on PostgreSQL Stores: refused by design
  in this branch (DECISIONS, step 12).
- Anything on Linux: `remote_store` and the namespaces backend are Linux-only
  and did not build or run here.
- Release-build timings: every number above is from a debug build.
