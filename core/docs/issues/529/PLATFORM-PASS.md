# #529 platform pass: `sandbox.create({ platform })` on every platform

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-11-01 on branch `claude/issue529-sandbox-platform-559e87`, after the
#527 pass (`97d408cc51`) and the #529 spec (`71f825ad22`). This is the terminal
pass of [#529](README.md) §8: the twelve #516 cells on nine platforms, each Sandbox
made by `sandbox.create({ platform })`, plus the default, refusal, mixed-project and
old-record checks. Nothing is pushed or merged; that waits for the owner.

- **Product commit of the receipts:** `6f242b72bf` for the full pass. Three later
  product commits follow the parent's rulings (`91f31613ba`, `4e8999872b`,
  `78b63e86eb`); the checks they affect were rerun at `78b63e86eb` (see "Rerun
  after the parent's rulings"). `33733d0645` is move-only (below).
- **Receipts:** `receipts/6f242b72bf/` beside this file, 111 JSON files in the
  #527 `lfs527-cell/1` format; the rerun's 7 are in `receipts/4e8999872b/` and
  `receipts/78b63e86eb/`. Each names its row, cell, platform, verdict, cause,
  timings, load, identities, lock and teardown. C01 receipts also carry `extras`
  (the default and refusal checks).
- **Delegation:** the owner asked for no questions; the parent session held the
  authority. Owner decisions P1–P6 of the spec apply. Choices left open (§9) and
  made during the pass are listed under "Decisions taken during the pass".

## Result

All 108 cells PASS on nine platforms. The mixed project, the default, the
refusals and the old records PASS. After the parent's rulings, the affected
checks were rerun at `78b63e86eb` and PASS (see "Rerun after the parent's
rulings"); one rerun receipt failed first and led to `78b63e86eb`.

| Platform (row) | C01 | C02 | C03 | C04 | C05 | C06 | C07 | C08 | C09 | C10 | C11 | C12 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `docker`, macOS (macos-docker) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `docker`, Linux (linux-docker) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `mac` (macos-native) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `linux` (linux-namespaces) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `apple-container` (vm-apple-container) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `microsandbox` (vm-microsandbox) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `lima` (vm-lima, vz) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `firecracker` (nested in Lima vz; pass/fail only) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `ssh` (vm-ssh) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| `docker`, Windows (WSL2 worker) | NOT_RUN — no Windows host | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |

Every cell's create answer and timed mount answer named the row's platform; the
runner fails a cell otherwise.

| Check (§8) | Receipt | Outcome |
| --- | --- | --- |
| Mixed project (Linux home) | `linux-mixed-check-…` | PASS (below) |
| Default (each home, `platform` left out) | each row's C01, plus `linux-native-C01-…` | PASS on ten homes |
| Refusals | each row's C01, plus `linux-native-C01-…` | PASS: 107 answers, all as expected; nothing made |
| Old records | `linux-oldrecords-check-…` | PASS (below) |
| Windows | — | NOT_RUN (no Windows host) |
| Connect | harness simulations | see "Connect" |

### Default and refusals

C01 on each home first makes one Sandbox with no `platform` and checks the answer
names the home's §4 default. It then asks for every platform the home does not
allow (expecting `Unsupported`) and for `windows`, `native`, `namespaces` and
`podman` (expecting `InvalidInput`). The project's Sandbox listing is read before
and after the refusals and must be equal: nothing was made.

`linux-native` is the Linux home with `LAYERFS_SANDBOX=native`, run for its C01
only, because §4 gives it its own default. Its other eleven cells are the #527
row and not part of the nine platforms.

| Home (row) | Default made | `Unsupported` for | `InvalidInput` | Wrong answers | Listing unchanged | Verdict |
| --- | --- | --- | --- | ---: | --- | --- |
| macos-docker | `docker` (expected `docker`) | `mac`, `linux`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| linux-docker | `docker` (expected `docker`) | `mac`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| macos-native | `mac` (expected `mac`) | `docker`, `linux`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| linux-namespaces | `linux` (expected `linux`) | `mac`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| linux-native | `linux` (expected `linux`) | `mac`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| vm-apple-container | `apple-container` (expected `apple-container`) | `docker`, `mac`, `linux`, `microsandbox`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| vm-microsandbox | `microsandbox` (expected `microsandbox`) | `docker`, `mac`, `linux`, `apple-container`, `lima`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| vm-lima | `lima` (expected `lima`) | `docker`, `mac`, `linux`, `apple-container`, `microsandbox`, `firecracker`, `ssh` | 4/4 | 0 | yes | PASS |
| vm-firecracker | `firecracker` (expected `firecracker`) | `docker`, `mac`, `linux`, `apple-container`, `microsandbox`, `lima`, `ssh` | 4/4 | 0 | yes | PASS |
| vm-ssh | `ssh` (expected `ssh`) | `docker`, `mac`, `linux`, `apple-container`, `microsandbox`, `lima`, `firecracker` | 4/4 | 0 | yes | PASS |

The answers read, for example, "Unsupported: platform mac is not available on this
macOS local (worker) home, which allows docker" and "InvalidInput: platform is one
of docker, mac, linux, apple-container, microsandbox, lima, firecracker, ssh, not
windows". Each refusal took under 0.4 ms (no call left the client).

### Mixed project

`linux-mixed` (`mixed.mjs`, one agent process as root in `lfs529-linux`) made a
`docker` and a `linux` Sandbox in one project and mounted Branch `lane` in both.

- **One Store file.** With both mounted, the runner took the (dev, inode) of
  `LAYERFS_STORE_PATH`, the Docker volume's
  `/var/lib/docker/volumes/<volume>/_data/global/store.sqlite` (dev 64769, inode
  797004), and scanned every `/proc/<pid>/fd` for that file. Three processes held
  it: the agent process; `layerfs-daemon` in the Docker container's cgroup
  (`docker-a546f755….scope`), which sees it at `/layerfs-store/global/store.sqlite`
  through the volume mount `docker inspect` reports; and the linux Sandbox's daemon
  in cgroup `/layerfs/6cad0d9d…`, at the host path. One file, three openers, one
  kernel.
- **Commits cross.** A file written and committed from `docker` was read by a fresh
  `linux` mount; a file committed from `linux` was read by a fresh `docker` mount.
- **Listing.** Both were listed, `docker` with image `sha256:afa1ef9c…` and `linux`
  with `sha256:1d91885d…`.
- **Closing one leaves the other.** After `docker` closed, the listing showed
  `linux` alone, and `linux` still mounted, ran a command and closed.

### Old records

`linux-oldrecords` made two Sandboxes with the pre-#529 SDK (the #527 bundle at
`97d408cc51`): a Docker one, and a namespaces one with `LAYERFS_SANDBOX=namespaces`.
Their records carry no `platform`. With the #529 bundle:
- `runtime.sandbox.records` showed both running, `platform: null`;
- `project.list_sandboxes` listed them as `docker` and `linux` (the read-time
  default);
- each was mounted, ran `printf ok` (exit 0, `ok`), was unmounted and closed;
- the listing ended empty.

## Rerun after the parent's rulings

The parent ruled on four points after the pass (decisions 5, 6, 13 and 14). Three
product commits implement them, and the rerun covers the checks they affect:
the mixed project and the old records, which the parent named, plus C01 on one
platform per changed path. C01 is the cell that also runs the default and
refusal extras. Every create now writes the owner marker on `mac`, `linux` and
the VM platforms, and `linux` and the VM platforms now take their default image
from the archive. The other 103 cells were not rerun (AGENTS.md §3.4): their
code paths are unchanged apart from these two create steps, which the rerun's
C01 cells cover.

| Receipt | Product | Verdict | What it shows |
| --- | --- | --- | --- |
| `macos-native-C01-…` (`4e8999872b/`) | `4e8999872b` | PASS | `mac` create writes the marker; default and refusals unchanged. It counts for `78b63e86eb`, whose change is Linux-only code. |
| `linux-mixed-check-…T005517…` (`4e8999872b/`) | `4e8999872b` | **FAIL**, kept | `linux` given Docker's digest was refused, but as `Failed` at the provider's Validate step, after a Sandbox number was claimed. Fixed by `78b63e86eb` (decision 15). |
| `linux-mixed-check-…` (`78b63e86eb/`) | `78b63e86eb` | PASS | below |
| `linux-oldrecords-check-…T005806…` (`78b63e86eb/`) | `78b63e86eb` | PASS, weak | Its no-`LAYERFS_LOCAL_DIR` step sent no name prefix, so the record was refused by the name check, not the marker. Kept; superseded by the next. |
| `linux-oldrecords-check-…T005836…` (`78b63e86eb/`) | `78b63e86eb` | PASS | below |
| `linux-namespaces-C01-…` (`78b63e86eb/`) | `78b63e86eb` | PASS | The home's `LAYERFS_IMAGE` is Docker's `sha256:afa1ef9c…`; the Sandbox ran `sha256:1d91885d…`, taken from `LAYERFS_IMAGE_ARCHIVE`. |
| `vm-lima-C01-…` (`78b63e86eb/`) | `78b63e86eb` | PASS | A VM platform's create, with its image from the bundle's archive. |

**Mixed project (rerun).** As in the first pass, with three changes:
- **Image defaults.** `linux` was made with no `image`. It listed with
  `sha256:1d91885d…` (from the archive) beside `docker` with `sha256:afa1ef9c…`.
  After the first `linux` closed, a second was made with an explicit
  `image: sha256:1d91885d…` and listed with it.
- **Docker's digest for `linux`** (`image: sha256:afa1ef9c…`) is refused
  `InvalidInput` ("the image archive's config digest is … not the requested …"),
  with the listing empty before and after.
- **Store identity** (`checks.store.identity`). The Store path is (dev 64769,
  ino 570866). The `docker` Sandbox's daemon is pid 3663, with the same
  (64769, 570866) at `/layerfs-store/global/store.sqlite`. The `linux` Sandbox's
  daemon is pid 3668, with the same (64769, 570866) at the volume's host path.
  `same: true`.

**Old records (rerun).** The pre-#529 records reopen exactly as in the first
pass. One step was added first: a reclaim from a client with no
`LAYERFS_LOCAL_DIR`, keeping the home's name prefix. It was refused
`InvalidInput`: "Sandbox 2's local record names …, whose owner marker cannot be
read …; set LAYERFS_LOCAL_DIR (deploy.localDir) to the directory it was made
under". Both Sandboxes stayed running. The rest of the check, like the first
pass, runs with the home's `LAYERFS_LOCAL_DIR` (`/var/lib/lfs529/lfs527-local-…`)
set, as a pre-#529 home had it. The namespaces record reopens through that
setting, not through the marker.

**Machine.** `lfs529-linux` was made again for the rerun, the same as in the
first pass (Lima 2.2.1 vz, Ubuntu 24.04, Docker 29.9.2 with the containerd store,
Node v22.23.4, user namespaces 0). The image archives were made again from
Docker Desktop's `sha256:afa1ef9c…`; the uncompressed one has the same name
(`1d91885d…`) and size (2,747,699,200 B). `lfs529-ssh` and `lfs529-vzkvm` were
not needed.

## Receipt identities

| Receipts | Product commit | Linux bundle (manifest sha256) | macOS bundle (manifest sha256) | VM bundle | Runner (`run.mjs`) |
| --- | --- | --- | --- | --- | --- |
| `receipts/6f242b72bf/` (111) | 6f242b72bf | build-linux-b5f1d388… (`ba1c640b…`), 99 receipts | build-macos-77750910… (`cf18e512…`), 12 receipts | cells-b5f1d388 (the Linux bundle plus `images/1d91885d….tar`, identity `ba1c640b…`) | `028293bd` ×111 |
| `receipts/4e8999872b/` (2) | 4e8999872b | build-linux-9cc0c9a1… (`e1410fd9…`), 1 receipt | build-macos-509dacd2… (`f7045a44…`), 1 receipt | — | `028293bd` ×2 |
| `receipts/78b63e86eb/` (5) | 78b63e86eb | build-linux-70433254… (`45dfb128…`), 5 receipts | — | cells-70433254 (the Linux bundle plus `images/1d91885d….tar`, identity `45dfb128…`) | `028293bd` ×5 |

- **Runner files** (all 111): `run.mjs` `028293bd`, `cell.mjs` `7164549b`,
  `workloads.mjs` `e22d812d`, `mixed.mjs` `57fc441b`, `make.mjs` `d6014896`,
  `oldrec.mjs` `6aa3cf37`.
- **Rerun runner files:** as above except `mixed.mjs` `fc82d2df` (all 7) and
  `oldrec.mjs` `8c2740bb` (the first four) or `f05f7be2` (the last three, after
  the prefix fix in this commit).
- **Old bundle:** build-linux-4ee01388… (`8fd20df6…`, `97d408cc51`), used only to
  make the old records.
- **Sandbox images.** `docker` uses `sha256:afa1ef9c…` (the index digest, arm64,
  node 24). `linux` and the VM platforms use the same image as an uncompressed
  docker-archive named by its config digest `sha256:1d91885d…`.
- **Why the receipts count at `33733d0645`.** That commit moves
  `api/runtime/src/place/` back to `src/platform/` and changes only import paths,
  comments, test paths and docs (Production LOC unchanged, 123966). The runner
  loads the runtime from the sealed bundle, which at `6f242b72bf` has `place/`; the
  move changes no behaviour, so the pass was not rerun (AGENTS.md §3.4).

## Rows running at the same time

The cells ran in batches of up to nine commands at once, one cell per row in each
batch, so rows on different machines overlapped. The runner's host-wide lock lets
only one timed sample run at a time; setup, warm-up and verification overlapped.
`linux-mixed` and `linux-oldrecords` ran after `linux-namespaces` finished, beside
`linux-docker`. No cell failed, so nothing was rerun.

| Row | Project setup | Budget left for the cell | Cells run (UTC, 2026-10-31/11-01) |
| --- | --- | --- | --- |
| macos-docker | 1.0–1.3 s | 94 s | 23:32:15–23:59:59 |
| linux-docker | 0.3–0.4 s | 89 s | 23:44:31–00:12:47 |
| macos-native | 0.0–1.1 s | 94–95 s | 23:44:19–00:12:46 |
| linux-namespaces | 0.0–0.1 s | 90 s | 23:32:42–00:03:07 |
| vm-apple-container | 7.0–22.7 s | 62–78 s | 23:34:06–00:06:14 |
| vm-microsandbox | 6.0–57.4 s | 27–79 s | 23:35:42–00:08:56 |
| vm-lima | 21.3–32.2 s | 53–64 s | 23:38:06–00:11:27 |
| vm-firecracker | 29.1–36.2 s | 48–56 s | 23:38:39–00:12:24 |
| vm-ssh | 12.1–15.4 s | 69–73 s | 23:32:51–00:03:31 |

vm-microsandbox C01's 57.4 s setup was the first pull of `node:22-trixie-slim`,
which the new machine did not have; its other cells took 6.0–8.2 s.

## Decisions taken during the pass

Reversible choices, made at the recommended option and recorded here.

1. **`status().platforms[].missing` (§9).** Each entry is `{ id, allowed, ready,
   missing }` for all eight platforms. A platform the home does not allow is
   `{ allowed: false, ready: false, missing: [] }`. An allowed one lists the names
   `status.missing` already uses, plus `tool:<cli>` for a missing command
   (`tool:docker` when the engine does not answer; `tool:container`, `tool:msb`,
   `tool:limactl`, `tool:ssh` for the VM providers) and, for `linux`, the Linux
   host needs `host:root`, `host:/dev/fuse` and `host:cgroup2`. `ready` is
   `allowed` with nothing missing.
2. **Old records: read-time default (§9, preferred).** Records are not rewritten.
   Rust returns the record's `platform` (null when absent) and `runtime`; the agent
   derives a missing platform: Docker → `docker`; native → `mac` on a macOS
   in-process home, `linux` elsewhere; namespaces → the provider on a `vm:` home,
   `linux` elsewhere.
3. **Store identity proof (§9).** (dev, inode) of `LAYERFS_STORE_PATH` matched
   against the fds of every process, with each holder's cgroup tied to its
   Sandbox's container id, and `docker inspect`'s mounts recorded.
4. **Every later call follows the record's runtime.** Reopen, close, reclaim and
   idle close dispatch on the record's `runtime` only; the deployment's backend no
   longer decides. A record naming a runtime this host never reopens is refused
   `InvalidInput` and left in place (neither used nor deleted).
5. **A local record reopens without `LAYERFS_LOCAL_DIR` only when it owns its
   directory (parent, 2026-11-01; `4e8999872b`).** With `localDir` set, the
   record's directory must be exactly where that setting puts the Sandbox, as
   before. With none, all three must hold:
   - (a) the directory is its own real path (no symlink in it);
   - (b) its last component is the name this home gives that Sandbox;
   - (c) its `owner` marker, written once at create (mode 0600, create-new, not
     followed if a symlink), equals the real path of the record's own directory
     `LAYERFS_HOME/<project>/sandboxes/<n>`, the directory the record was read
     from.

   Anything else is refused `InvalidInput` before any effect, naming
   `LAYERFS_LOCAL_DIR` as the fix, and left in place. A record made before #529
   has no marker, so a client with no `LAYERFS_LOCAL_DIR` is refused on it; a
   pre-#529 home set `LAYERFS_LOCAL_DIR`, and the old-records check keeps it
   set. The parent's first wording (the record's `directory` must resolve to
   `LAYERFS_HOME/<project>/sandboxes/<n>`) would have refused every native
   record, because a native record's `directory` is its working folder under
   `LAYERFS_LOCAL_DIR`; the marker replaces it, as the parent approved. The
   first pass ran under the earlier rule (parent directory, absolute, no `..`).
6. **`image` defaults per platform (parent, 2026-11-01; `91f31613ba`).** When a
   call leaves `image` out, `docker` uses `LAYERFS_IMAGE`; `linux` and the VM
   platforms use the image `LAYERFS_IMAGE_ARCHIVE` holds (its config digest)
   when that is set. An explicit `image` still wins. With no `image`, an
   archive naming more than one image is refused before any effect. The
   settings `localDir` and `imageArchive` of `@layerfs/runtime` are approved;
   the `linux` platform no longer needs `LAYERFS_IMAGE`. The first pass gave
   `linux` its image per call (`sandbox.create({ platform: "linux", image })`);
   the rerun's mixed check makes it with no image, then once with an explicit
   one.
7. **`platform: null` is `InvalidInput`.** Only an absent `platform` (undefined)
   means the default.
8. **The deployment's `sandboxHost` field is gone.** A worker's Sandboxes run on its
   Linux host, the addon's on this one; the agent passes that host when it builds a
   platform's deployment.
9. **Sweep.** With `localSandboxes`, the sweep also reclaims projects whose records
   hold a stopped non-docker Sandbox; a failure is reported in `failed`.
10. **`src/platform/` kept (owner, 2026-11-01).** The spec's §6.6 rename to
    `src/place/` was made (`b3efa9bad9`) and moved back (`33733d0645`) on the
    owner's instruction. The spec records the reversal.
11. **macOS target seed.** `tools/worktree.py seed --platform macos` was refused
    because an exited container (`lucid_sammet`, another session's) binds the main
    checkout's target. It was left alone; the #527 macOS target was cloned with
    `cp -c` instead.
12. **Batches.** Cells ran nine at a time across machines (above), as #527's row
    helpers did.
13. **`native` on a `vm:` home maps to that home's provider (parent,
    2026-11-01).** §4 allows only the provider there, so `LAYERFS_SANDBOX=native`
    on a `vm:` home defaults to the provider, like `namespaces`
    (`platform.mjs` covers it).
14. **The Store identity receipt holds all three (dev, ino) values and the pids
    (parent, 2026-11-01).** `mixed.mjs` records the Store path's (dev, ino) and,
    for every holder, its pid, (dev, ino) and fd target, under
    `checks.store.identity`, with `same` computed from them.
15. **A given image the archive lacks is `InvalidInput` before any effect
    (`78b63e86eb`).** Found by the rerun: Docker's digest given to `linux` was
    refused `Failed` at the provider's Validate step, after a number was
    claimed. The deployment now checks the archive names the image, the same
    read the no-image default does.

## Findings not fixed in this pass

1. **One `LAYERFS_IMAGE` alone still cannot serve both platforms of a Linux
   home; fixed by default (decision 6).** Docker with the containerd store
   names the image by its index digest (`sha256:afa1ef9c…`); the namespaces
   backend wants the config digest (`sha256:1d91885d…`) and an uncompressed
   archive. With `LAYERFS_IMAGE_ARCHIVE` set, `linux` now takes the archive's
   own digest, so no per-call image is needed. Passing the Docker digest to
   `linux` is refused `InvalidInput` before any effect (decision 15; the
   rerun's mixed check records it). A follow-up
   could let the namespaces backend accept the index digest.
2. **`native_discovery::a_dropped_guard_…` needs `cc`**, which `lfs529-linux` does
   not have. The other five tests pass; the code it covers is unchanged.
3. **`discovery.rs` is at 985 lines** (ceiling 999).
4. **The #527 open findings stand:** `kernel_guard` on macOS checks the OS, not the
   deployment; `layerfs-store-service` does not build for Windows; `vm:ssh` leaves
   `/layerfs/{home,store}` on its host (the runner removes them); and the others in
   [#527's list](../527/PLATFORM-PASS.md#findings-not-fixed-in-this-pass).
5. **Windows is untested** (no host). `docker` through the WSL2 worker should
   behave as macOS's worker home; the agent treats any worker without a `vm` field
   as a `docker`-only home.
6. **A refused record fails the call that reads it.** `project.list_sandboxes`,
   reclaim and the sweep read every record, so one record they refuse (damaged,
   another `LAYERFS_LOCAL_DIR`, or with no `LAYERFS_LOCAL_DIR` one without its
   owner marker) refuses the whole call for that project; it is not left out.
   This was so before #529; `agent.md` said "left out of listing and reclaim"
   and now says what happens.
7. **With no `LAYERFS_LOCAL_DIR`, a stopped local record whose directory is
   gone is not reclaimed.** Its directory cannot be resolved, so decision 5
   refuses it; a client with `LAYERFS_LOCAL_DIR` set reclaims it as before.

## Commits

Production LOC uses `tools/production_loc.py` on the first parent and the staged
tree. The scope is core, engine and api; the reference is absent (retired R9c; last
counted 65,417 at tree 498dd1917812). Over #529: **123811 → 124059 (delta +248)**.

| Commit | Subject | Production LOC |
| --- | --- | --- |
| `71f825ad22` | docs: #529 Sandbox platform spec, sandbox.create({ platform }) | 123811 -> 123811 (delta 0) |
| `b3efa9bad9` | runtime: src/platform/ becomes src/place/ (#529 §6.6, move only) | 123811 -> 123811 (delta 0) |
| `db4b8e4dc8` | sandbox, agent, runtime: a Sandbox names its platform (#529) | 123811 -> 123966 (delta +155) |
| `6f242b72bf` | test: the cell runner makes each Sandbox with its platform (#529 §8) | 123966 -> 123966 (delta 0) |
| `33733d0645` | runtime: src/place/ is src/platform/ again (owner 2026-11-01, move only) | 123966 -> 123966 (delta 0) |
| `71ecd9fc6a` | docs: #529 platform pass, sandbox.create({ platform }) on every platform | 123966 -> 123966 (delta 0) |
| `2fe92d2473` | docs: #529 platform pass, Connect and cleanup | 123966 -> 123966 (delta 0) |
| `91f31613ba` | sandbox, agent: a linux or VM Sandbox's default image is its archive's (#529) | 123966 -> 123993 (delta +27) |
| `4e8999872b` | node: with no LAYERFS_LOCAL_DIR a local record must own its directory (#529) | 123993 -> 124051 (delta +58) |
| `78b63e86eb` | node: a given image the archive lacks is InvalidInput before any effect (#529) | 124051 -> 124059 (delta +8) |
| this commit | docs, test: #529 rerun after the parent's rulings | 124059 -> 124059 (delta 0) |

`db4b8e4dc8` subtotals: core 41405, engine 77696 (+57), api 4865 (+98). Tests
changed or deleted there (listed in its message): in `native_local.rs`,
`a_native_record_is_not_reopened_by_a_namespaces_deployment`,
`a_native_record_is_used_only_with_its_deployment_s_absolute_local_dir` and
`close_refused_as_mixed` were replaced; in `native_deployment.mjs`, the two
`sandboxHost` assertions were deleted with the field.

Checks run before the pass: Clippy `-D warnings` on macOS and Linux
(`layerfs-node`), `cargo fmt`, `check_product_boundary` PASS; `native_local` 10/10,
`native_admission` 2/2, `native_connection_wait` 1/1, `native_discovery` 5/6
(finding 2), as root in `lfs529-linux`; JS `platform.mjs`, `calls` 42,
`packaging` 7 modes, `trace` 9, `runtime.mjs` and `vm_placement` (every mode),
CLI 78/78. `runtime.mjs` and `vm_placement.mjs` were run again, all modes PASS,
after `33733d0645`.

After the rulings: subtotals at `78b63e86eb` are core 41405, engine 77787
(+91 since `db4b8e4dc8`), api 4867 (+2). Tests added: `namespaces_rootfs`'s
`an_archive_names_its_one_image_by_its_config_digest` (with `holds` in
`78b63e86eb`) and `native_local`'s
`with_no_local_dir_a_record_s_directory_must_be_real_and_owned_by_it`. Tests
changed: `native_deployment.mjs` expectations (a namespaces deployment has no
image unless given), `platform.mjs` cases (VM, linux, `native` on a `vm:`
home), and two `native_local` tests that now write the owner marker their
records stand for. None deleted. Checks: Clippy `-D warnings` on Linux
(`layerfs-sandbox`, `layerfs-node`) and macOS (`layerfs-node`), `cargo fmt`,
`check_product_boundary` PASS; `namespaces_rootfs` 14/14 and `native_local`
11/11 on Linux (the latter in the Node runtime image, since `lfs529-linux` was
gone then); JS `platform`, `native_deployment`, `calls` 42, `runtime.mjs`
(surface, sandboxes, backend, docker-default), `vm_placement` (settings, flow,
bundle), CLI 78/78.

## Connect

**PASS in the harness simulations; live NOT_RUN** (Connect's LayerFS link is not
moved to this unmerged branch).

The changes are in `~/Ephemeral-AI-Lab/layerfs-connect`, uncommitted, on top of
the Connect UI polish session's uncommitted work. That checkout is on
`feat/connect-ui-polish` (`988d525`), not `main`. Nothing was switched, stashed,
discarded or committed there. The parent session was told and had not answered when
the edits began; editing in place is reversible, so the recommended option was taken.

- **Badge.** Every Sandbox row names its platform after the Sandbox's name (mark
  and name; the mark alone below 24rem). A host on an older LayerFS names no
  platform, and the badge reads it from the Sandbox's place and runtime.
- **New Sandbox.** An open project row offers a menu built from
  `HostStatus.platforms` (the host passes `runtime.status().platforms` through):
  - ready platforms are choices;
  - allowed but not ready ones are disabled and say what they need;
  - not allowed ones are left out;
  - a one-platform home adds the line about per-project places.

  The choice calls a new host route, `POST /api/projects/:name/sandboxes
  { platform }`, which runs `sandbox.create({ platform })`.
- **Host.** Each `SandboxInfo` carries `platform` when LayerFS names it.
- **Harness.** Ten cases, three of them new: `mac-native`, `linux-mixed` and
  `platforms` (every badge, and a platform that needs setup). Each was checked at
  360 px in dark mode and at 1100 px in light mode:
  - one badge per Sandbox row, none outside the page;
  - the menu only where the project's place answers;
  - no horizontal overflow from these changes.

  Screenshots were checked for `linux-mixed`, `platforms` and `mac-native`.
- **Checks.** `tsc` (web and node) is clean, and `host/test/server.test.js` passes
  11/11 (with the new route). `host/test/compute.test.js` cannot load in this
  checkout: Connect's `connect-main` LayerFS link has no `node_modules`, an
  environment problem that does not depend on this change. Its new `sandboxTag`
  assertions passed when run on their own.
- **Found, not fixed (Connect polish).**
  - At 360 px the "Other places" setup list overflows.
  - At 360 px a Sandbox row's state chip overlaps the `n / 16` column. The badge
    does not cause it: the column widths are the same with the badge hidden.
  - The Later line still lists native Sandboxes as not in LayerFS.

## Cleanup

Removed:
- **Per-cell resources:** every per-cell VM, microVM, Sandbox, container, volume and
  Store, by each cell's teardown (`teardownOk` in all 111 receipts). Checked after
  the pass:
  - the Lima VMs held no containers, volumes, Stores or Firecracker VMs;
  - huge pages were back to 1100;
  - Apple Container, Microsandbox and Docker Desktop held nothing named `lfs52*`.
- **Lima VMs:** `lfs529-linux`, `lfs529-ssh` and `lfs529-vzkvm`, with their disks
  (`/var/lib/lfs529-nlt*` went with `lfs529-linux`). Also Lima's Ubuntu image
  download (`by-url-sha256/002fbe46…`).
- **Pulled images:** `node:22-trixie-slim` from Apple Container and Microsandbox.
- **Apple Container:** its service is stopped (the pass had started it).
- **Caches under `~/.cache/layerfs-test`:** the VM bundle copy, the image archives,
  the SSH test keys, the share folder, the runner copy and the receipt folder
  (committed here first). The folder is empty, as it was before the pass.
- **Scratch:** the environment scripts.

Kept:
- **This worktree**, its sealed bundles and its private Cargo targets, warm for the
  owner's review (409 MB under `~/.cache/layerfs-worktrees`, plus the target
  volumes in Docker). Once the branch is merged or dropped, release them with
  `tools/worktree.py cleanup-scope --release-caches`.
- **`~/.lima/_config` and `_disks`**, which existed before the pass.

Host free space: 208.6 GB before the cells, 219.9 GB after cleanup.

**Rerun.** Removed the same way: `lfs529-linux` with its disk; the `vm-lima`
cell's own VM (by its teardown, `teardownOk`); the image archives, VM bundle copy,
share folder, runner copies and lock under `~/.cache/layerfs-test` (empty again);
Lima's Ubuntu download; the scratch environment scripts. Kept with the worktree:
the rerun's sealed bundles (build-linux-9cc0c9a1…, build-linux-70433254…,
build-macos-509dacd2…) beside the first pass's. Host free space: 219.6 GB before
the rerun, 208.6 GB at its peak, 219.5 GB after.

## Appendix: per-row identities and timings

Timings are diagnostic: one sample per cell, never rerun for speed. vm-firecracker
timings describe the nesting (two hypervisors) and are not comparable with other
rows. "lock wait" is the time a cell waited for the host-wide sampling lock.

### macos-docker

Where: Linux 6.12.76-linuxkit aarch64, 8 vCPUs, 7.8 GiB, cpu.max max 100000; platform `docker`; image sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 2.4–6.6; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 257.7 | 301.5 | 10.3 | 102.3 (Committed) | 8.5 | 0.0 |
| C02 | PASS | 804.7 | 851.2 | 10.0 | 102.0 (Committed) | 8.6 | 1.0 |
| C03 | PASS | 486.2 | 524.9 | 10.5 | 20.5 (UpToDate) | 7.0 | 1.0 |
| C04 | PASS | 689.6 | 730.8 | 11.2 | 120.1 (Committed) | 8.8 | 1.0 |
| C05 | PASS | 742.0 | 786.4 | 11.2 | 130.9 (Committed) | 12.3 | 1.0 |
| C06 | PASS | 130.9 | 162.8 | 8.5 | 238.3 (Committed) | 10.0 | 1.0 |
| C07 | PASS | 378.6 | 425.4 | 12.6 | 657.9 (Committed) | 24.6 | 1.0 |
| C08 | PASS | 210.3 | 254.4 | 12.7 | 353.6 (Committed) | 17.6 | 0.0 |
| C09 | PASS | 57.8 | 99.7 | 14.3 | 3.5 (UpToDate) | 11.0 | 1.0 |
| C10 | PASS | 210.5 | 253.5 | 14.6 | 58.5 (Committed) | 20.3 | 0.0 |
| C11 | PASS | 130.6 | 184.6 | 14.5 | 171.0 (UpToDate) | 17.5 | 0.0 |
| C12 | PASS | 181.4 | 226.1 | 12.4 | 71.4 (Committed) | 8.3 | 1.0 |

### linux-docker

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max max 100000; platform `docker`; image sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 0.0–6.8; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 260.3 | 274.2 | 3.6 | 94.7 (Committed) | 3.1 | 1.0 |
| C02 | PASS | 797.4 | 811.3 | 4.7 | 97.5 (Committed) | 2.8 | 0.0 |
| C03 | PASS | 454.4 | 467.0 | 4.1 | 8.1 (UpToDate) | 1.7 | 0.0 |
| C04 | PASS | 684.9 | 697.1 | 3.4 | 106.0 (Committed) | 2.4 | 1.0 |
| C05 | PASS | 718.8 | 732.6 | 3.6 | 104.8 (Committed) | 2.5 | 0.0 |
| C06 | PASS | 149.0 | 163.4 | 3.7 | 316.6 (Committed) | 10.0 | 1.0 |
| C07 | PASS | 330.6 | 343.2 | 3.5 | 620.0 (Committed) | 18.5 | 0.0 |
| C08 | PASS | 182.2 | 197.1 | 3.8 | 326.8 (Committed) | 10.3 | 0.0 |
| C09 | PASS | 52.2 | 66.9 | 7.2 | 0.7 (UpToDate) | 4.2 | 0.0 |
| C10 | PASS | 207.5 | 221.3 | 6.3 | 55.6 (Committed) | 13.5 | 1.0 |
| C11 | PASS | 107.8 | 123.5 | 6.8 | 146.1 (UpToDate) | 9.9 | 1.0 |
| C12 | PASS | 171.0 | 183.4 | 3.6 | 45.4 (Committed) | 1.7 | 1.0 |

### macos-native

Where: Darwin 25.4.0 arm64, 14 vCPUs, 36.0 GiB, cpu.max None; platform `mac`; image None; source 6f242b72bf; bundle build-macos-7775091064c346f9a9c5a0b1b2a588bc; 1-min load 3.5–6.9; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 661.4 | 679.8 | 22.4 | 191.4 (Committed) | 40.6 | 0.0 |
| C02 | PASS | 6306.6 | 6322.7 | 29.2 | 206.8 (Committed) | 43.7 | 0.0 |
| C03 | PASS | 1104.5 | 1122.0 | 22.4 | 81.5 (UpToDate) | 26.8 | 1.0 |
| C04 | PASS | 4524.5 | 4545.8 | 29.5 | 203.7 (Committed) | 36.2 | 0.0 |
| C05 | PASS | 4674.6 | 4691.5 | 26.2 | 207.3 (Committed) | 36.6 | 0.0 |
| C06 | PASS | 187.4 | 203.9 | 27.2 | 432.5 (Committed) | 22.8 | 0.0 |
| C07 | PASS | 371.9 | 387.0 | 22.3 | 754.7 (Committed) | 24.6 | 1.0 |
| C08 | PASS | 198.1 | 212.7 | 22.8 | 428.5 (Committed) | 24.1 | 0.0 |
| C09 | PASS | 44.1 | 62.0 | 21.7 | 95.6 (UpToDate) | 29.1 | 0.0 |
| C10 | PASS | 225.9 | 241.6 | 22.0 | 171.0 (Committed) | 25.1 | 0.0 |
| C11 | PASS | 142.6 | 158.4 | 22.2 | 259.3 (UpToDate) | 24.1 | 0.0 |
| C12 | PASS | 373.2 | 387.9 | 21.8 | 133.3 (Committed) | 29.1 | 0.0 |

### linux-namespaces

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; platform `linux`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 0.0–5.6; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 243.3 | 258.9 | 3.0 | 93.6 (Committed) | 2.3 | 1.0 |
| C02 | PASS | 789.3 | 806.7 | 2.7 | 100.1 (Committed) | 2.7 | 0.0 |
| C03 | PASS | 422.5 | 440.9 | 2.5 | 6.5 (UpToDate) | 1.2 | 0.0 |
| C04 | PASS | 731.6 | 743.1 | 2.8 | 103.4 (Committed) | 2.3 | 1.0 |
| C05 | PASS | 571.3 | 588.8 | 2.7 | 74.0 (Committed) | 1.4 | 0.0 |
| C06 | PASS | 142.1 | 153.7 | 2.9 | 311.6 (Committed) | 9.4 | 1.0 |
| C07 | PASS | 331.3 | 341.1 | 2.8 | 617.7 (Committed) | 17.9 | 1.0 |
| C08 | PASS | 160.0 | 173.2 | 3.0 | 317.0 (Committed) | 10.3 | 1.0 |
| C09 | PASS | 54.6 | 72.4 | 6.1 | 0.5 (UpToDate) | 4.0 | 1.0 |
| C10 | PASS | 192.1 | 203.9 | 5.5 | 51.4 (Committed) | 13.3 | 1.0 |
| C11 | PASS | 94.2 | 105.3 | 5.3 | 140.3 (UpToDate) | 9.8 | 0.0 |
| C12 | PASS | 171.7 | 184.8 | 2.8 | 44.1 (Committed) | 1.8 | 0.0 |

### vm-apple-container

Where: Linux 6.18.35 aarch64, 5 vCPUs, 4.0 GiB, cpu.max None; platform `apple-container`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 3.3–7.7; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 249.8 | 281.0 | 6.1 | 93.6 (Committed) | 5.4 | 0.0 |
| C02 | PASS | 662.4 | 687.7 | 5.3 | 95.1 (Committed) | 10.0 | 2.0 |
| C03 | PASS | 398.7 | 420.2 | 5.3 | 9.2 (UpToDate) | 5.4 | 1.0 |
| C04 | PASS | 446.5 | 465.0 | 4.1 | 71.8 (Committed) | 3.4 | 2.0 |
| C05 | PASS | 543.9 | 580.5 | 5.9 | 103.5 (Committed) | 5.3 | 2.0 |
| C06 | PASS | 131.2 | 153.9 | 6.5 | 304.9 (Committed) | 11.1 | 1.0 |
| C07 | PASS | 295.0 | 320.9 | 6.1 | 597.5 (Committed) | 16.7 | 1.0 |
| C08 | PASS | 143.7 | 174.4 | 7.9 | 320.4 (Committed) | 15.6 | 2.0 |
| C09 | PASS | 40.1 | 55.8 | 8.4 | 2.0 (UpToDate) | 6.6 | 1.0 |
| C10 | PASS | 170.1 | 183.4 | 9.7 | 46.3 (Committed) | 13.0 | 1.0 |
| C11 | PASS | 82.6 | 99.4 | 7.8 | 133.9 (UpToDate) | 11.0 | 1.0 |
| C12 | PASS | 162.3 | 196.8 | 6.0 | 49.9 (Committed) | 8.3 | 0.0 |

### vm-microsandbox

Where: Linux 6.12.113 aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; platform `microsandbox`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 3.3–7.7; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 219.6 | 294.0 | 4.8 | 91.7 (Committed) | 6.2 | 1.0 |
| C02 | PASS | 643.9 | 700.9 | 6.2 | 95.4 (Committed) | 8.8 | 1.0 |
| C03 | PASS | 344.4 | 390.6 | 3.2 | 5.8 (UpToDate) | 5.4 | 0.0 |
| C04 | PASS | 561.0 | 614.8 | 5.6 | 100.6 (Committed) | 13.0 | 1.0 |
| C05 | PASS | 578.7 | 668.6 | 5.3 | 101.1 (Committed) | 8.3 | 0.0 |
| C06 | PASS | 132.8 | 169.5 | 4.8 | 305.1 (Committed) | 21.6 | 0.0 |
| C07 | PASS | 351.5 | 408.4 | 5.6 | 612.3 (Committed) | 15.4 | 1.0 |
| C08 | PASS | 128.1 | 179.5 | 5.5 | 306.4 (Committed) | 30.2 | 0.0 |
| C09 | PASS | 33.3 | 107.4 | 9.1 | 1.5 (UpToDate) | 7.3 | 0.0 |
| C10 | PASS | 208.5 | 265.6 | 9.7 | 48.2 (Committed) | 14.6 | 0.0 |
| C11 | PASS | 70.5 | 115.6 | 7.6 | 132.3 (UpToDate) | 10.2 | 1.0 |
| C12 | PASS | 163.4 | 214.1 | 5.0 | 43.5 (Committed) | 8.2 | 0.0 |

### vm-lima

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; platform `lima`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 3.4–8.4; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 253.3 | 263.6 | 5.3 | 98.3 (Committed) | 5.8 | 0.0 |
| C02 | PASS | 653.8 | 672.1 | 4.2 | 70.8 (Committed) | 4.7 | 0.0 |
| C03 | PASS | 407.3 | 422.5 | 7.7 | 18.9 (UpToDate) | 5.0 | 0.0 |
| C04 | PASS | 625.5 | 639.1 | 5.4 | 102.9 (Committed) | 6.2 | 0.0 |
| C05 | PASS | 737.0 | 753.6 | 6.3 | 103.4 (Committed) | 5.8 | 0.0 |
| C06 | PASS | 144.8 | 157.9 | 6.5 | 318.4 (Committed) | 14.7 | 2.0 |
| C07 | PASS | 319.4 | 332.0 | 5.8 | 616.7 (Committed) | 21.8 | 1.0 |
| C08 | PASS | 159.5 | 179.1 | 8.8 | 312.7 (Committed) | 17.7 | 3.0 |
| C09 | PASS | 50.9 | 64.2 | 11.6 | 1.7 (UpToDate) | 9.6 | 0.0 |
| C10 | PASS | 195.6 | 211.2 | 10.9 | 55.1 (Committed) | 19.8 | 1.0 |
| C11 | PASS | 86.8 | 99.4 | 8.1 | 136.9 (UpToDate) | 14.5 | 1.0 |
| C12 | PASS | 161.5 | 173.5 | 5.5 | 47.1 (Committed) | 5.3 | 0.0 |

### vm-firecracker

Where: Linux 6.18.56+ aarch64, 4 vCPUs, 1.9 GiB, cpu.max None; platform `firecracker`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 3.5–7.9; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 474.1 | 520.6 | 12.1 | 92.6 (Committed) | 10.6 | 1.0 |
| C02 | PASS | 1338.4 | 1404.4 | 13.5 | 115.1 (Committed) | 24.4 | 1.0 |
| C03 | PASS | 1003.5 | 1071.3 | 8.0 | 19.9 (UpToDate) | 5.9 | 1.0 |
| C04 | PASS | 1569.2 | 1616.6 | 10.5 | 116.3 (Committed) | 6.2 | 1.0 |
| C05 | PASS | 1560.7 | 1599.2 | 8.3 | 124.9 (Committed) | 15.2 | 1.0 |
| C06 | PASS | 286.9 | 336.8 | 14.8 | 391.3 (Committed) | 29.7 | 1.0 |
| C07 | PASS | 652.3 | 704.5 | 17.7 | 778.5 (Committed) | 37.0 | 1.0 |
| C08 | PASS | 346.1 | 410.8 | 13.7 | 400.3 (Committed) | 26.2 | 1.0 |
| C09 | PASS | 81.4 | 123.7 | 16.0 | 2.9 (UpToDate) | 6.1 | 0.0 |
| C10 | PASS | 431.6 | 481.8 | 18.4 | 97.6 (Committed) | 17.3 | 1.0 |
| C11 | PASS | 249.9 | 288.9 | 22.9 | 148.2 (UpToDate) | 10.4 | 0.0 |
| C12 | PASS | 367.6 | 397.8 | 9.0 | 56.6 (Committed) | 4.5 | 0.0 |

### vm-ssh

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; platform `ssh`; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 6f242b72bf; bundle build-linux-b5f1d388c41e43d9bd4391c819773ace; 1-min load 2.7–5.9; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 247.2 | 261.1 | 4.1 | 95.6 (Committed) | 3.4 | 0.0 |
| C02 | PASS | 780.6 | 797.9 | 3.9 | 96.7 (Committed) | 3.5 | 1.0 |
| C03 | PASS | 456.9 | 470.7 | 3.8 | 8.0 (UpToDate) | 2.5 | 1.0 |
| C04 | PASS | 641.5 | 652.1 | 4.4 | 101.9 (Committed) | 3.3 | 1.0 |
| C05 | PASS | 507.5 | 520.0 | 2.4 | 73.4 (Committed) | 2.2 | 0.0 |
| C06 | PASS | 150.4 | 167.7 | 4.1 | 318.2 (Committed) | 10.4 | 1.0 |
| C07 | PASS | 311.1 | 328.0 | 4.7 | 599.5 (Committed) | 16.1 | 0.0 |
| C08 | PASS | 152.0 | 162.3 | 5.1 | 313.0 (Committed) | 10.2 | 3.0 |
| C09 | PASS | 52.5 | 65.6 | 6.5 | 2.0 (UpToDate) | 4.7 | 1.0 |
| C10 | PASS | 183.0 | 197.2 | 6.6 | 52.4 (Committed) | 12.2 | 0.0 |
| C11 | PASS | 86.0 | 103.4 | 6.3 | 135.3 (UpToDate) | 13.6 | 0.0 |
| C12 | PASS | 162.4 | 176.1 | 3.8 | 46.1 (Committed) | 3.2 | 1.0 |

