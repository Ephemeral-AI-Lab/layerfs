# Lima and Apple container machines, live (2026-11-07)

> **Status:** Live receipt. Owner request 2026-11-07, "get lima and apple
> container work". Source `fb02aa0666` (`layerfs_v2_0_0`, PR #608) on
> branch `machines-live`; no product change was needed.

## Setup

| | |
| --- | --- |
| Host | macOS 26.4.1, arm64, about 38 GB RAM |
| Tools | `limactl` 2.2.1 (vz); Apple `container` CLI 1.6.1 |
| Bundle | sealed from the branch (`tools/linux_bundle.py`, debug profile), hash `d1c6615df2ab…4c94571`, passed as `LAYERFS_BUNDLE`: worker 201 MB, daemon 197 MB, helper 32 MB, `sandbox-image.tar` 2,747,719,680 bytes |
| CLI | `target/release/layerfs` and `layerfs-daemon` built from the same tree |
| Home | one private home with both machines; Store profile `disposable` |
| Machines | `lm`: `machine create lm lima --cpus 4 --memory 6G --disk 30G` (VM `lfs-ba8a6d34-lm`); `ac`: `machine create ac apple-container --cpus 4 --memory 6G --disk 30G` (container `lfs-ba8a6d34-ac`, volume `lfs-ba8a6d34-ac-data`) |
| Downloads | none this run. Lima's cache already held Ubuntu 24.04's arm64 cloud image (616,522,752 bytes, cached 2026-11-06); `debian:trixie-slim` (30.2 MB) was already in the container image store |

Each step is one command under a wall timeout (`scripts/timeout.py`), its
command line, output and exit in one receipt file. Scripts: `scripts/`.

The receipt files named below (`L01`, `l-smoke/08a`, `T-sandbox_machines-lm.txt`
and so on) are raw run output, so they are not committed (documentation
policy). They are retained on the test host at
`~/.cache/layerfs-evidence/project-113/fb02aa0666/machines-live/`, with this
file and the scripts.

## Results

| Step | Lima `lm` (forwarded) | Apple container `ac` (worker) |
| --- | --- | --- |
| `machine create` | PASS `L01` | PASS `A01` |
| `machine start`, `status` | PASS `L02`, `L03` | PASS `A02`, `A03` |
| store create (Disposable), push, project init through the worker | PASS `l-smoke/01`–`03` | PASS `a2-smoke/01`–`03` |
| sandbox create (`backend: namespaces`) | PASS `l-smoke/04` | PASS `a2-smoke/04` |
| fork, mount, exec, commit, unmount, file read | PASS `l-smoke/05`–`11` | PASS `a2-smoke/05`–`11` |
| plain close | PASS `l-smoke/14a`, `14b` | PASS `a2-smoke/14a`, `14b` |
| caller-named id: `sleep 600 & sleep 600` in its cgroup, cancelled from another process (`cancelled` SIGTERM; exec `kind: "cancelled"`, 143) | PASS `l-smoke/08a`, `08b`, `08f` | PASS `a2-smoke/08a`, `08b`, `08f` |
| no exec cgroup and no `sleep 600` left | PASS `l-smoke/08g`, `08h` | PASS `a2-smoke/08g`, `08h` |
| duplicate id `Exists` | PASS `l-smoke/08c` | PASS `a2-smoke/08c` |
| unmount and close `Busy` (`detail.execs`) | PASS `l-smoke/08d`, `08e` | PASS `a2-smoke/08d`, `08e` |
| second cancel `ended` | PASS `l-smoke/08i` | PASS `a2-smoke/08i` |
| generated id from `--progress`, cancelled with grace 0 | PASS `l-smoke/08j`–`08n` | PASS `a2-smoke/08j`–`08n` |
| `unmount --force` with a command running | PASS `l-smoke/12a`–`12f` | PASS `a2-smoke/12a`–`12f` |
| `close --force` with a command running | PASS `l-smoke/13a`–`13f` | PASS `a2-smoke/13a`–`13f` |
| stale forwarded record (owner CLI killed): cancel ends it in the worker | PASS `l-gap3/10*` | n/a: the worker keeps the record |
| stale: unmount ends it first | PASS `l-gap3/20*` | n/a |
| stale: close ends it first (then `MountsActive`, a Workspace mounted) | PASS `l-gap3/30*` | n/a |
| stale: `close --force` ends it first, then closes | PASS `l-gap3/70`–`77` | n/a |
| stale: exec of the same id ends it first, then runs | PASS `l-gap3/41*` | n/a |
| worker unreachable (VM stopped by `limactl stop`): cancel `unknown`; unmount, unmount `--force`, close, close `--force` refused `Unknown`; same id `Exists`; record byte-identical; log `kept: true` | PASS `l-gap3/50*`–`58` | n/a |
| VM back (`machine start`): the kept record's cancel answers `ended` and removes it | PASS `l-gap3/60`–`64` | n/a |
| `sandbox_machines.rs` VM leg (`LAYERFS_TEST_MACHINES`) | PASS `T-sandbox_machines-lm.txt` | PASS `T-sandbox_machines-ac.txt` |
| power cycle: `machine stop`, `status` stopped, `machine start`, the test leg again | PASS `P1`–`P6`, `T-sandbox_machines-after-restart.txt` | PASS same |
| PostgreSQL leg | NOT_RUN: no server of this run's own; the one listening on 5432 belongs to another session | NOT_RUN: same |
| final `machine stop` | PASS `Z3` | PASS `Z4` |

`a-smoke-try1/` is a first Apple container run whose guest checks failed
in the script (a shell function passed to the timeout wrapper); its
product steps passed. `a2-smoke/` is the run after the script fix.

## Notes

- `sandbox close --force` answers `unknown` listing every mounted
  Workspace; that is by design (O31).
- After the Lima VM restarted, `sandbox get` showed the Sandboxes as
  `running` with a `down_reason` (their forward had ended) and
  `workspace status` was `Unavailable`; `sandbox close --force` answered
  `was_gone: true`. That matches the documented record state.
- Lima's root disk shows Lima's own 100 GiB (sparse); the data disk is the
  30 GiB the create asked for.
