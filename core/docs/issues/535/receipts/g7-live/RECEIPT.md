# #535 G7 live receipt: PostgreSQL Store Sandboxes on lima, apple-container, microsandbox and firecracker reach a server in Docker Desktop

- **When:** 2026-11-03 06:20–06:39 UTC (14:20–14:39 +08)
- **Host:** macOS 26.4.1 (25E253), arm64, Apple M3 Max; load1 about 4–7 during the run
- **Tools:** limactl 2.2.1 · container CLI 1.6.1 · msb 0.7.11 (`~/.microsandbox/bin/msb`, put on PATH for the
  CLI by `run.sh`; without it `machine platforms` reports microsandbox `missing: ["tool:msb"]`) · Docker Desktop
  29.5.2 · PostgreSQL 17 (`postgres:17-bookworm`, image `639ab7ceb90e`) · Firecracker v1.17.0 (kit below)
- **Disk:** 141.26 GB free at start, 129.98 GB at the end of the run (`tools/cleanup.py check`, threshold 100 GB)
- **Product:** worktree `layerfs-issue535-verify`, branch `issue535-live`, at `b772e77f97` plus
  `6d35b90711` (tools only: the bundle's guest worker built with `--features postgresql`, see lima attempt 1).
  **Every PostgreSQL row was lifted locally for the check**: an uncommitted edit of
  `connect/crates/layerfs-machine/src/support.rs` `postgresql_refusal` answering `None` for the platforms under test
  (`local-lift-1.diff.txt`; `local-lift-fc.diff.txt` adds firecracker). Never committed; the tree is clean.
- **macOS CLIs** (`cargo +1.85.1 --locked --offline build -p layerfs-cli --features machine,postgresql`, debug,
  via `tools/worktree.py cargo`), kept under the worktree registration's `builds/lfs535-cli/`:

  | Binary | sha256 | Source |
  | --- | --- | --- |
  | `layerfs-lifted` | `5001a074…0da186` | `b772e77f97` + lift of lima, apple-container, microsandbox |
  | `layerfs-lifted-diag` | `7457db1f…bf9500` | + lift of firecracker, + two **local diagnostic** edits (`local-lift-diag.diff.txt`): lima `HOST_ALIAS` = `192.168.5.2`; microsandbox `reach` = `Loopback::Lookup` of `msb exec <vm> -- getent hosts host.microsandbox.internal` |
  | `layerfs-lifted-fc` | `2e123e73…41143b` | `b772e77f97` + lift of all four VM rows, no diagnostic edit |

- **Linux bundle** (`tools/linux_bundle.py daemon|worker|helper|seal --dest <registration>/builds/lfs535-bundle`,
  debug, `LAYERFS_BUNDLE` pointed at it): first seal `caf648cc…225505` (worker without PostgreSQL), resealed
  `8fa4c9a445f856c66134b2bfc768287101c02524ed39cbfd11237ffd9247329e` after `6d35b90711` (`bundle.json`; source
  sha256 `d8fcc294…` = the committed tree, the lift reverted while it built). Image `sha256:1d91885d…`.
  Firecracker kit (lane FC-KIT, `…/layerfs-issue535-c1-f90330a1552e/builds/firecracker-kit`) cloned with `cp -c`
  into the bundle's `firecracker/` after the seal (411 MB on disk; sha256 of all four files matched KIT.md).
- **Server:** throwaway container `lfs535-pg`, published on `0.0.0.0:55535` and `127.0.0.1:55536`, a generated
  password (`--password env:LFSPW`, in no file here; `grep` of every file in this folder and of the home: none),
  `log_connections=on`. Stores in one fresh private home (`$TMPDIR/lfs535-home.*`, 0700):
  - `pg` = `postgres://postgres@127.0.0.1:55535/lfs535` (server reachable on the bridge);
  - `pglo` = `postgres://postgres@127.0.0.1:55536/lfs535lo` (server on the Mac's loopback only).
  
  Each was Inited from one Mac folder (`readme.txt`, `sub/list.txt`, `sub/deeper/leaf.txt`, symlink
  `link-to-list -> sub/list.txt`): `setup.log`.
- **Runner:** `run.sh` (one `layerfs` call per invocation, UTC start, load1, wall ms, exit code, password and home
  paths scrubbed). No call was retried; every changed second attempt follows a recorded diagnosis.

## Verdicts

| Platform | `pg` (127.0.0.1:55535 → rewritten) | `pglo` (loopback-only server) | With the committed `reach` |
| --- | --- | --- | --- |
| lima | **PASS** only with the diagnostic alias `192.168.5.2` | PASS (diagnostic alias) | **FAIL**: Sandbox daemon cannot resolve `host.lima.internal` |
| apple-container | **PASS** (`192.168.64.1`, read from `container inspect`) | refused at the reach check (expected) | as committed |
| microsandbox | **PASS** (`host.microsandbox.internal`) | PASS | as committed |
| firecracker | **PASS** (`192.168.5.2`, NAT on the Lima host) | PASS | as committed |

PASS = `sandbox create` (reach check from the guest worker) → `workspace mount` → exec reading Mac files
(`readme.txt`, `sub/deeper/leaf.txt`, the symlink) and writing one → `workspace commit` → `workspace unmount` →
`file read` of the new file **from the Mac** (in-process Store call on the host server) → `sandbox close`.

## lima — FAIL as committed; PASS with the address instead of the alias (`lima.log`)

- `machine create lfs535l lima` 30.2 s.
- **Attempt 1** (bundle `caf648cc…`): `sandbox create pg/app sl lfs535l` refused in 52 ms:
  `Unavailable: the worker's machine cannot reach PostgreSQL Store pg's server at host.lima.internal: this LayerFS
  build has no PostgreSQL engine; build the binding with --features postgresql`. Cause: `tools/linux_bundle.py`
  built the guest `layerfs` with `--no-default-features` only, while the worker runs the reach check
  (`api/crates/layerfs-control/src/sandbox/route.rs:376` `reach_check` → `store/postgres.rs:117` `read_binding`,
  `#[cfg(not(feature = "postgresql"))]` → `UNBUILT`). The guest binary held the UNBUILT text and no PostgreSQL
  client. Fixed in `6d35b90711`; bundle resealed `8fa4c9a4…`; `machine stop` / `machine start` (20.9 s) loaded it
  (guest `/layerfs/bundle/bundle.json` hash `8fa4c9a4…`).
- **Attempt 2**: `sandbox create` failed after 5.6 s: `Failed: native sandbox stopped at Listener … local daemon exited
  before its listener line; its daemon's last log lines: after Overlay startup: Store open: physical Store I/O: pack
  persistence: Refused { status: "Closed" }`. The reach check **passed** (server log: three authenticated
  connections at 06:27:25 from the worker). Diagnosis: the namespaces helper pivots the daemon into the Sandbox
  image root (`connect/crates/layerfs-sandbox/src/bin/layerfs-sandbox-helper/init.rs`, `mounts.rs:100`); that root
  has no `/etc/hosts` and a `resolv.conf` naming 1.1.1.1/1.0.0.1, so `host.lima.internal` (only in the Lima
  guest's `/etc/hosts`) does not resolve there:
  ```
  guest root:            getent hosts host.lima.internal -> 192.168.5.2
  chroot <image root>:   getent hosts host.lima.internal -> rc=2
  ```
  The PostgreSQL connect error becomes `BackendError::Closed`
  (`engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs:60`). So the reach check
  (worker, guest root) and the daemon (Sandbox root) resolve in different contexts: a passing check does not prove
  the daemon reaches the server.
- **Attempt 3** (`layerfs-lifted-diag`, alias replaced by `192.168.5.2`, local only): create 2.4 s, mount 0.34 s,
  exec 0.07 s (read `hello from the mac`, `deep`, `sub/list.txt`), commit 0.17 s, unmount 0.05 s, Mac `file read
  pg/app wl /from-lima.txt` → `written in lima lfs535l`, close 0.24 s. **PASS.**
- Loopback-only `pglo` (diag CLI): create 1.4 s … read-back `lo-lima` **PASS** (Lima's host agent dials the Mac's
  127.0.0.1, as the G7 probe found).
- Fix options for the lead (not changed here): `platforms/lima/adapter.rs:14` `HOST_ALIAS` → `"192.168.5.2"`
  (Lima user-mode networking's fixed host address; firecracker's adapter already uses it), or give namespaces
  Sandbox roots the guest's `/etc/hosts` (read-only bind, as Docker does), which covers every alias.

## apple-container — PASS (`ac.log`)

`layerfs-lifted`, no diagnostic edit. `machine create lfs535a apple-container` 9.6 s; `sandbox create pg/app sa`
4.4 s (reach check at `192.168.64.1:55535`, the `container inspect` gateway); mount 0.28 s; exec 0.17 s; commit
0.32 s; unmount 0.08 s; Mac `file read pg/app wa /from-ac.txt` → `written in apple-container lfs535a`; close
0.29 s. **PASS.**

Loopback-only `pglo`: `sandbox create pglo/app sa2 lfs535a` refused in 77 ms, exit 125, nothing created:
```
{"code":"Unavailable","message":"the worker's machine cannot reach PostgreSQL Store pglo's server at 192.168.64.1: reading schema layerfs: pack persistence: Refused { status: \"Closed\" }","detail":null}
```
Judgement: it names the address it tried, but does not tell a user what to do. It does not name the machine
(`the worker's machine`, not `lfs535a`), it leaks an internal error (`pack persistence: Refused { status: "Closed"
}`) instead of "connection refused", and it does not say that apple-container reaches the Mac only on the vmnet
bridge, so the server must listen on `0.0.0.0` (or `192.168.64.1`) and `pg_hba` must admit `192.168.64.0/24`.
Related: attempt 1 on lima shows the same wrapper (`route.rs:126-136`) calls a missing build feature "cannot reach
… server".

## microsandbox — PASS with the committed alias (`msb.log`)

- `machine create lfs535m microsandbox` 24.4 s (created `--net public,host` by `83715db285`).
- With `layerfs-lifted-diag` (alias looked up in the guest: `172.16.0.5`): create 6.8 s … Mac read-back
  `written in microsandbox lfs535m` **PASS**.
- **Control with the committed alias** (`layerfs-lifted`, URL rewritten to `host.microsandbox.internal`): create
  0.6 s, mount 0.22 s, exec 0.09 s, commit 0.17 s, unmount 0.04 s, Mac read-back `written in microsandbox lfs535m,
  commit alias`, close 0.22 s. **PASS.** Unlike lima, the alias resolves inside the Sandbox root: msb answers DNS
  for it whatever server the root's `resolv.conf` names (`chroot <image root> getent hosts
  host.microsandbox.internal` → `172.16.0.5`, rc 0). The diagnostic lookup is not needed.
- Loopback-only `pglo` (`layerfs-lifted`): create 0.86 s … read-back `lo-msb` **PASS**.

## firecracker — PASS (`fc.log`)

`layerfs-lifted-fc` (all four rows lifted, no diagnostic edit), bundle `8fa4c9a4…` + kit.
- `machine create lfs535h lima --nested --cpus 4` 34.4 s (instance `lfs-1dfaa5f6-lfs535h`; `/dev/kvm` present; the
  kit copied to `/layerfs/data/firecracker` with `.sealed`).
- **Manual, as #533:** `echo 1100 > /proc/sys/vm/nr_hugepages` on the host (1,100 free).
- `machine create lfs535f firecracker --host lfs535h --memory 2GiB` refused `InvalidInput` (`memory` is a size such
  as 512M or 8G), then `--memory 2G` 45.0 s. On the host afterwards: `net.ipv4.ip_forward = 1`, `-A POSTROUTING -s
  172.30.0.0/16 ! -d 172.30.0.0/16 -j MASQUERADE`, the microVM's config `"mem_size_mib":2048`,
  `"huge_pages":"2M"`, 76 huge pages free.
- `sandbox create pg/app sf lfs535f` 14.0 s (reach check at `192.168.5.2:55535`); mount 0.27 s; exec 0.26 s;
  commit 0.31 s; unmount 0.06 s; Mac read-back `written in firecracker lfs535f`; close 0.16 s. **PASS.**
- Loopback-only `pglo`: create 1.1 s … read-back `lo-fc` **PASS** (the Mac's 127.0.0.1 is reached through
  192.168.5.2 after the NAT).

## `machine.platforms` in the lifted builds (`setup.log`)

With `layerfs-lifted`: lima, apple-container, microsandbox `postgresql: { store_calls: "in_process", sandbox: true,
reason: null }`, `ready: true` (microsandbox `ready: false, missing: ["tool:msb"]` until msb was on PATH);
firecracker `sandbox: false, reason: "firecracker NAT route not verified (G7)"`. With `layerfs-lifted-fc`, the
firecracker machine's `supports` read `postgresql_sandbox: true, reason: null` (`fc.log`).

## Peer addresses the server saw (`pg-server.log`)

Docker Desktop's port proxy sets the peer, so the server cannot tell platforms (or the Mac) apart:
every connection through the `0.0.0.0:55535` publish arrived from **`192.168.65.1`** (62, database `lfs535`, from
the Mac and all four platforms), every one through the `127.0.0.1:55536` publish from **`172.17.0.1`** (39,
database `lfs535lo`). Each was admitted by `pg_hba.conf:128` of the image's default file, method scram-sha-256.
The remaining 4 + 3 "connection received" lines without authorization are the SSLRequest probes below. No FATAL
or ERROR line.

## Owner's server reach (protocol level) (`owner-reach.log`)

The lead recreated the owner's `layerfs-postgres` published on `0.0.0.0:5432`. One PostgreSQL SSLRequest
(`00 00 00 08 04 d2 16 2f`, `sslreq.sh`, no StartupMessage, no authentication, nothing created) from each guest's
root; one-byte answers:

| Guest | Target | Answer |
| --- | --- | --- |
| lima (`lfs535h`, the nested Firecracker host; same template and networking as `lfs535l`) | `host.lima.internal:5432` and `192.168.5.2:5432` | `N` |
| firecracker microVM `lfs535f` (ssh from its host) | `192.168.5.2:5432` | `N` |
| apple-container `lfs535a` | `192.168.64.1:5432` (its `ipv4Gateway`) | `N` |
| microsandbox `lfs535m` | `host.microsandbox.internal:5432` (`172.16.0.5`) | `N` |

Controls in the same runs: `:55535` answered `N` everywhere; `:55536` (loopback-only) answered `N` from lima,
firecracker and microsandbox and **Connection refused** from apple-container. `lfs535l` had already been removed,
so the Lima check ran from `lfs535h`; `lfs535a` and `lfs535m` were recreated by `machine create` only for this check
(9.4 s, 24.5 s) and removed after.

## Cleanup (`cleanup.log`)

Removed through `layerfs machine remove`: `lfs535l`, `lfs535a`, `lfs535m` (06:32), `lfs535f`, `lfs535h` (06:36),
and the recreated `lfs535a`, `lfs535m` (06:37). Afterwards `limactl list` showed only `lfc-linux`, `lfc-ssh`,
`lfc-vzkvm` (untouched), `limactl disk list` only `lfcssh-store`; `container list --all`, `container volume list`,
`msb list`, `msb volume list` empty. `docker rm -fv lfs535-pg` removed the container and its anonymous volume
`fd1c0c8b…`; nothing listens on 55535/55536. The home and the source folder were deleted (no password in the home
before deletion). Kept on purpose: the sealed bundle (with the kit clone) at
`<registration>/builds/lfs535-bundle/` and the three CLIs at `<registration>/builds/lfs535-cli/`.
Not touched: `layerfs-postgres` (only SSLRequests sent), `lfc-*`, the other containers, the shared bundle.
