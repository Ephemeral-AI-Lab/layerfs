# #529 Sandbox platform: `sandbox.create({ platform })`

> **Status:** Spec; target LayerFS 2.0.0; not a released contract.
> Written 2026-11-01 by the owner's direction, on top of #527 (branch
> `claude/issue527-platform-pass-e05aa3`, PR #528, `b02242fd59`). Owner
> decisions in §2 are settled; choices the spec leaves to implementation are
> in §9 and are decided by the parent session, not the owner.

## 1. Summary

A Sandbox names the platform it runs on. `sandbox.create` takes an optional
`platform`, records it with the Sandbox, and every later call on that Sandbox
follows the record. Listing and the CLI report it. Every platform keeps the
Docker Sandbox contract: numbered Sandboxes, up to 16 Workspaces, idle close
after 2 minutes, `list_workspaces`, `close`, the same refusals.

`LayerFS.init` and where a home keeps its Stores (`LAYERFS_PLACEMENT`) do not
change in this issue. A project's Store stays where its home puts it, so a
Sandbox may use only the platforms that can open that Store (§4).

## 2. Owner decisions (2026-11-01)

| # | Decision |
| --- | --- |
| P1 | The option is `platform`, not `backend`. |
| P2 | `LayerFS.init` keeps its current signature and behaviour; per-project placement is a later issue. |
| P3 | Only the Sandbox binds to a platform. |
| P4 | The platform names are `docker`, `mac`, `linux`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh`. |
| P5 | There is no `windows` platform: a Windows Sandbox is `docker` through the WSL2 worker. |
| P6 | `linux` is a native Linux Sandbox in its own namespaces (today's `namespaces` backend). |

## 3. The platforms

| `platform` | A Sandbox is | Host systems | Where its project's Store must be | Runtime underneath (record field `runtime`) |
| --- | --- | --- | --- | --- |
| `docker` | a Docker container | macOS, Windows, Linux | the project's Docker volume | `docker` |
| `mac` | a native macOS process, served over loopback NFS | macOS | this Mac's local apfs or hfs disk | `native` |
| `linux` | a native Linux process in its own namespaces and cgroup | Linux (root, cgroup v2, `/dev/fuse`) | the Linux host's local ext4, xfs or btrfs disk | `namespaces` |
| `apple-container` | namespaces inside the project's VM | macOS | that VM's disk | `namespaces` |
| `microsandbox` | namespaces inside the project's VM | macOS, Linux | that VM's disk | `namespaces` |
| `lima` | namespaces inside the project's Lima vz VM | macOS | that VM's disk | `namespaces` |
| `firecracker` | namespaces inside the project's microVM, nested in Lima | macOS | that VM's disk | `namespaces` |
| `ssh` | namespaces on the project's remote Linux host | macOS, Linux | that host's disk | `namespaces` |

`platform` is what a caller chooses and reads. `runtime` stays in the record
and in `runtime.sandbox.records` as the lower-level fact (#527 step 12).

## 4. Which platforms a home allows

A Sandbox can open its project's Store only from the machine or VM that holds
it (one kernel, AGENTS.md). With `LAYERFS_PLACEMENT` unchanged:

| Home | Allowed | Default when `platform` is left out |
| --- | --- | --- |
| macOS `local` (Docker Desktop) | `docker` | `docker` |
| macOS `local` with `LAYERFS_SANDBOX=native` (Store on apfs/hfs) | `mac` | `mac` |
| Windows `local` (WSL2 worker) | `docker` | `docker` |
| Linux `local` | `docker`, `linux` | `docker`, or `linux` when `LAYERFS_SANDBOX=namespaces` |
| `vm:<provider>` | `<provider>` | `<provider>` |

- **Refusals, before anything is made:** a known platform the home does not
  allow is `Unsupported`, naming the home and the platforms it allows; any other
  value (including `windows`, `native`, `namespaces`) is `InvalidInput`, listing
  the eight names.
- **Linux mixes platforms in one project.** On a Linux home the SDK already opens
  the Store in-process at `LAYERFS_STORE_PATH` on a local filesystem that the
  daemons share. A `docker` Sandbox mounts the project's volume; a `linux`
  Sandbox's daemon opens the same Store file on the host. Both see the same
  Branches and Commits.
- **`LAYERFS_SANDBOX` becomes the home's default only.** Its values stay
  `docker | native | namespaces` (and the former pair, for the one release #527
  promised). `native` on Linux (no namespaces) keeps working as a home default
  and records platform `linux` with runtime `native`; it is not a value of
  `platform`.

## 5. API changes

```diff
- sandbox.create({ image }?)             -> { sandbox, image }
+ sandbox.create({ image?, platform? }?) -> { sandbox, image, platform }

- project.list_sandboxes(pageSize?, pagePos?) -> [{ sandbox, name, image, workspaces }]
+ project.list_sandboxes(pageSize?, pagePos?) -> [{ sandbox, name, image, platform, workspaces }]

- workspace.mount(sandbox, branch) -> { sandbox, path, image }
+ workspace.mount(sandbox, branch) -> { sandbox, path, image, platform }

+ runtime.status() ... platforms: [{ id, allowed, ready, missing }]
```

- `image` is `null` for `mac` and native `linux` Sandboxes; an `image` given for
  them stays ignored, as today.
- `project.list_sandboxes` lists every platform's running Sandboxes, not only
  the client's default.
- `runtime.status().platforms` has all eight entries for the home: `allowed`
  (§4), `ready` (its settings and tools are present) and `missing` (the same
  names `status.missing` uses, per platform). Connect builds its New Sandbox
  menu from it.
- CLI: `layerfs sandbox create [--image <text>] [--platform <name>]`; `layerfs
  project list_sandboxes` prints the platform.
- Unchanged: every other method, Sandbox numbering, the 16-Workspace limit, idle
  close, the error codes and `LayerFS.init`.

## 6. Implementation changes

1. **Agent (`api/agent`).** `sandbox.create` validates `platform` and sends it in
   the call. Workspace and Sandbox calls (`mount`, `exec`, `commit`, `unmount`,
   `close`, `list_workspaces`) stop sending the client's own backend as the
   deciding fact (`client.js` reads `LAYERFS_SANDBOX` once today and sends it with
   every call); the Sandbox's record decides.
2. **Records.** A Sandbox record stores `platform` beside `runtime`. A record
   from before this change, with no `platform`, reads as the platform its
   `runtime` and the home imply.
3. **Rust (`engine/crates/layerfs-sandbox`, `layerfs-node`).** Create takes the
   platform; reopen, close, reclaim and idle close dispatch by the record's
   runtime for every runtime, with one reopen path for all three (finding F5 in
   #527). Refusals happen before any effect.
4. **Linux mixing.** A `linux` Sandbox on a Linux home opens the same Store file
   as the home's `docker` Sandboxes (§4). Verify the path the daemon sees is the
   volume's own file, not a copy.
5. **Runtime (`api/runtime`).** `status().platforms`; listing, reclaim, sweep and
   leftovers over every platform's records.
6. **Naming.** In `@layerfs/runtime`, `src/platform/` means where a home lives
   (`docker/`, `vm/`), which now clashes with the Sandbox's `platform`. Rename it
   to `src/place/`, as one move-only commit. The Rust `platform/{docker,native,
   namespaces}` already matches the new meaning and stays.
   **Superseded (owner, 2026-11-01): keep `src/platform/`.** The rename was made
   (`b3efa9bad9`) and moved back by a later move-only commit.
7. **Docs.** `api/docs/agent.md`, `runtime.md`, the CLI help and the
   `.agents/skills/layerfs-js-sdk` skill. Rewrite the "There is no placement
   (owner 2026-10-23)" paragraph: a Sandbox now names its platform; the home
   still decides where Stores live.

## 7. Connect UI (`~/Ephemeral-AI-Lab/layerfs-connect`)

After the LayerFS side passes:

- Each Sandbox row shows a platform badge, in the same row layout as Docker.
- **New Sandbox** offers a platform menu built from
  `runtime.status().platforms`: allowed and ready ones are choices; allowed but
  not ready ones show what is missing; not allowed ones are left out. A home with
  one platform shows it alone, with one line saying that other platforms come
  with per-project places.
- The Compute page keeps its current design; its Projects table shows each
  Sandbox's platform.

Rules: Connect is edited only in its own repository on `main` (no worktree, no
LayerFS edit for it). That checkout holds uncommitted work from the Connect UI
polish session: build on it, discard nothing, commit nothing. Verify in the
harness simulations (`.game-harness`, every case, narrow and dark). Connect's
pinned LayerFS checkout is not moved to the unmerged branch, so a live check is
NOT_RUN unless the branch is merged.

## 8. Terminal pass

Reuse the #527 cell runner and bundles. One cell per command, at most 120 s.

| Check | Platforms / homes | Expected |
| --- | --- | --- |
| C01–C12 through `sandbox.create({ platform })` | `docker` (macOS), `docker` (Linux), `mac`, `linux`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | 108 PASS |
| Mixed project | Linux home: a `docker` and a `linux` Sandbox in one project, both mounting one branch; a Commit from each is seen by the other; listing shows both with their platforms; closing one leaves the other | PASS |
| Default | each home with `platform` left out makes its §4 default | PASS |
| Refusals | every disallowed (home, platform) pair of §4 is `Unsupported`; `windows`, `native`, `namespaces` and an unknown name are `InvalidInput`; nothing made in any | PASS |
| Old records | a Sandbox made before the change (no `platform` in its record) lists, mounts and closes | PASS |
| Windows | `docker` through WSL2 | NOT_RUN (no Windows host) |
| Connect | harness simulations of the badge and the menu | PASS; live NOT_RUN unless merged |

Record everything in `core/docs/issues/529/PLATFORM-PASS.md` in the #527
format: result table, receipt identities, decisions, findings not fixed,
commits with Production LOC, cleanup.

## 9. Left to implementation (decided by the parent session)

- The exact shape of `runtime.status().platforms[].missing`.
- Whether a record migration is a read-time default (preferred) or a rewrite.
- How the Linux `docker` + `linux` Store sharing is proven equal (file identity
  check in the receipt).

## 10. Related

- #527 and PR #528: the consolidation this builds on.
- #516: the twelve cells. #512: the VM routes.
- A later issue: per-project place in `LayerFS.init` (P2), which is what lets
  one macOS project mix `docker`, `mac` and a VM.
