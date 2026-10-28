# #352 — Direct Store access from the SDK (option A)

> **Status:** Current, owner decision 2026-10-18. For Store calls (fs, branch,
> project, operator), the SDK opens the global Store **in-process**, without a
> Sandbox daemon in the path. This supersedes, for those calls, the 2026-10-07
> direction that the host is control-only. Root AGENTS.md records it (the
> LayerFS mental model's target paragraph, revised 2026-10-18). Workspace
> calls (mount, exec, Workspace commit, status, unmount) stay with the daemon.
> Implementation: [DIRECT_STORE_IMPL.md](DIRECT_STORE_IMPL.md), tracked as fix
> F-020.
> **One route, Linux only (owner, 2026-10-18):** Store calls run in-process
> only; the daemon stops serving them, and an unsupported setup fails
> `Unsupported`. There is no daemon route to choose or fall back to.
> **Windows-native route (owner, 2026-10-25):** the Store engine and this
> in-process Store face also run natively on Windows; see §6.

## 1. Architecture

```
Node process (SDK, native addon)
  ├─ Store calls: fs.*, branch.*, project.*, operator.*
  │     └─ in-process: Content + Storage + History + Persistence
  │           └─ opens the global Store file directly (Disposable/WAL/OFF)
  └─ Workspace calls: sandbox.mount / exec / commit / status / unmount
        └─ control channel ──▶ daemon in a Sandbox ──▶ the same Store file
```

- The SDK becomes one more direct opener of the Store, beside the daemons. It
  reuses the same Content, Storage, History and Persistence crates and their
  public ports; no second encoder, no second publication path (AGENTS.md
  "shared construction boundaries").
- An fs write is one attempt: construct, save, publish to the Branch head with
  the same merge-at-publish rule as F-008, in one writer transaction under the
  Store's short write lane. No retry, no resend.
- Reads use a read-only session; they never wait for a writer (F-018's rule).

## 2. The safety condition (non-negotiable)

All openers of one Store must share **one Linux kernel** and one local
filesystem holding the Store, because SQLite's WAL locks and the daemons' writer
coordination (the parent-directory inode lane, core/AGENTS.md) only coordinate
within one kernel.

| Where the SDK process runs | Direct mode |
| --- | --- |
| A Linux container mounting the Store's Docker volume (same VM kernel as the daemons) | **Allowed.** This is how the #352 runner runs from now on |
| A Linux host with the Store on a local filesystem, daemons on the same host | Allowed |
| macOS host, Store on a Docker volume inside Docker Desktop's VM | **Refused explicitly** at open (`Unsupported`), before any effect. File sharing across the VM boundary does not carry SQLite locks |
| Any network or shared filesystem (NFS, virtiofs, bind mounts from macOS) | Refused |

The check is made when the Store is opened, from facts the process can verify
(platform, filesystem type of the Store's directory, the same device as the
daemons' volume). An unsupported setup fails with its reason; there is no daemon route
for Store calls to fall back to.

## 3. What changes for the replay

1. **The runner moves into a Linux container** (`layerfs-issue352-runner`),
   built from the runtime image (Node 24), with the Store volume, `~/lfs-history`
   (plan, oracles, metrics, snapshots) and the repository checkout mounted, and
   the Docker socket for creating Sandboxes. Its image id is recorded like any
   binary.
2. **Store calls go in-process;** Sandboxes are created only where the plan needs
   Workspaces (apply, check, hold, install, build, tests). The service Sandbox is
   no longer needed for Store calls.
3. **New baseline.** This is a new route and a new SDK binary: the speed baseline
   is re-measured on it (ranges 0–4, a fork of the range-4 snapshot), beside the
   earlier ones, never mixed. Old measurements keep their labels.
4. **Kind 1 under REBUILD.md:** the Store format does not change; existing
   snapshots and the `p2-main` line continue.

## 4. Proof before use

- **Locking test (must pass first):** the in-process SDK and two daemons write to
  one Store concurrently from separate containers on the shared volume; every
  publish is either applied once or refused with its exact outcome; `PRAGMA
  integrity_check` passes; the final heads equal a serial oracle. Barrier-ordered,
  bounded, under 120 s.
- **Refusal test:** opening from an unsupported setup (macOS host path, a
  non-local filesystem) fails `Unsupported` before any write.
- **Equivalence:** the same fs, branch and project calls give identical results and
  Store contents in-process and through the daemon, on the same snapshot.
- **Old vs new on the same snapshots** (SPEC §6): fs.read, fs.write, list, stat,
  merge, fork, publish p50/p95 and SQL per call; in-process is expected to remove
  the control round trip.

## 5. Not changed

Disposable/WAL/OFF only; page size 4 KiB; one attempt per operation; FUSE and
Workspaces stay in the daemon; the daemon keeps opening the Store directly; no
`layerfs-server`, no host/server adapter in the data path (the SDK is a direct
opener, not a proxy).

## 6. Windows-native route (owner direction 2026-10-25)

The one-kernel safety condition is met natively on Windows: every opener of a
Windows Store shares one Windows kernel and one local NTFS volume. No WSL2 and
no daemon are involved. Implemented on branch `windows-native-store`:

- The cross-process writer lane is a named kernel mutex
  (`Local\layerfs-writer-lane-<hash>`); an abandoned mutex is taken by the
  next waiter, so a dead writer does not hold the lane.
- The kernel guard allows local NTFS volumes only, read with
  `GetVolumeInformationW`; other filesystems and non-disk prefixes fail
  `Unsupported` before the Store opens.
- SQLite is bundled (rusqlite) on Windows targets; Disposable/WAL/OFF is the
  only profile, unchanged.
- Acquisition identity is the real volume serial and file index
  (`GetFileInformationByHandle`), in audited FFI modules listed in
  core/AGENTS.md.
- The SDK addon serves fs, branch, project and operator calls in-process
  exactly as on Linux. Workspace, exec and daemon calls refuse `Unsupported`
  on Windows: FUSE, the daemon and Workspaces remain Linux-only, and the dsh
  macOS and WSL2 exceptions are unchanged.

Recorded limitations of this route: the Windows lane and discovery
mutexes live in the `Local\` namespace, so a session-0 service and a user
session would each hold its own lane (Linux's directory flock crosses
sessions); the product opens its Store within one login session today, and
cross-session coordination would need the `Global\` namespace with a
privilege evaluation (K3 review F2); Windows Init carries names as
lossless WTF-8 (an unpaired surrogate included, byte-exact round trip),
synthesizes the mode from the read-only attribute, has no hard-link
membership, and its backing-inside-source check resolves the placement's
spelling (a subst drive or unresolvable alias is not recognised); overlay
allocation is the logical length. Last-owner overlay-file deletion applies
the same audited identity on Windows as on Linux (volume serial + file
index, sole link, checked on both the path's entry and the retained
handle).

When the owner merges this branch, the observation period runs by a
fixed checklist (each step reads a mechanism that already exists, none
needs a new decision):

1. **Merge lands** — the observation job records `merged` with the merge
   commit; the tech-tree gate `pr_merged_and_stable` flips on that fact.
2. **Stability window** — the owner bounds it (the ruling names no
   length); its evidence is the absence of new findings on this PR plus
   the standing suites staying green on `main`.
3. **WinFsp independent period starts contract-first** (SSO pattern):
   the mount face's contracts are written and reviewed before any
   implementation, under the `winfsp-independent-period` tree node whose
   precondition is exactly step 1 plus step 2.

Proof receipts: the cross-process lane verifier
(`layerfs-persistence` `tests/windows_store_native.rs`, two re-exec'd child
processes, bounded waits); `cargo check --locked` of the five touched crates
on Windows and in the `rust:1.85.1-bookworm` container; the lifecycle smoke
(init → fork → one-Commit write → read back → list_commits → close) with no
WSL2 and no daemon.
