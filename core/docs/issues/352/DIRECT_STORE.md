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
  (`Global\layerfs-writer-lane-<hash>` — one lane per directory across
  login sessions); an abandoned mutex is taken by the
  next waiter, so a dead writer does not hold the lane.
- The kernel guard allows local NTFS volumes only: it opens the Store
  directory's own handle, reads its final path (a spelling that lands on
  `\\?\UNC\` — a share behind a junction or mapped drive — is refused) and
  the filesystem name with `GetVolumeInformationByHandleW`; anything else
  fails `Unsupported` before the Store opens.
- SQLite is bundled (rusqlite) on Windows targets; Disposable/WAL/OFF is the
  only profile, unchanged.
- Acquisition identity is the real volume serial and file index
  (`GetFileInformationByHandle`), in audited FFI modules listed in
  engine/AGENTS.md.
- The SDK addon serves fs, branch, project and operator calls in-process
  exactly as on Linux (on Windows, `project.list_sandboxes` and the sandbox
  family also refuse `Unsupported` — the daemon owns them). Workspace, exec
  and daemon calls refuse `Unsupported`
  on Windows: FUSE, the daemon and Workspaces remain Linux-only, and the dsh
  macOS and WSL2 exceptions are unchanged.

Recorded limitations of this route: the writer lane lives in the
`Global\` namespace, so one login session's scheduled task and another
session's interactive process share one lane (round-3 A2 F4 closed the
`Local\` per-session split recorded earlier; a mutant needs no
create-global privilege); the discovery mutexes remain `Local\` — they
coordinate one session's discovery only and are no integrity boundary.
The named mutexes can in principle be squatted by another process of the
machine before this product creates them (the directory ACL is not the
capability there, as Linux flock's is); the discovery turn waits bounded
(60s `Busy`) and the writer lane's wait is the readiness wait of the lock
model, as Linux's blocking flock is. Windows Init carries names as
lossless WTF-8 end to end (an unpaired surrogate included, byte-exact
round trip through the codec and the audited identity probes) and
synthesizes the mode from the read-only attribute, and has no hard-link
membership. **The WTF-8 name grammar is owner-ratified 2026-10-31
("可以合并") as the long-term content grammar**; until the byte-carrying
merge/compare path lands as follow-up, a merge or compare that must build
a String path over a name outside UTF-8 is refused (`InvalidRecord`) at
every entrance — the recorded-name path and the walked path alike
(round-6 D1 F2 fenced `join`, whose collapsed key silently dropped the
second changed entry of one directory)
rather than decided on a collapsed path, and SDK listings render such
names empty; a JS write naming one lands as an U+FFFD sibling file
rather than being refused (the JS string boundary cannot carry an
unpaired surrogate — the same byte-carrying follow-up). The
backing-inside-source check walks the audited
volume-serial + file-index identity of every ancestor (a subst drive or
junction into the source is recognised and refused, as on Linux), and
overlay allocation is the entry's real on-disk footprint. Last-owner
overlay-file deletion applies the same audited identity on Windows as on
Linux (volume serial + file index, sole link, checked on both the path's
entry and the retained handle), opening the path itself, never a
substituted reparse point. Homes, discovery records, the install volume
directory and Store files carry owner-only protected DACLs (the audited
ACL module): what this route creates is restricted to its owner, and an
existing entry whose DACL grants Everyone, Users or Authenticated Users
is refused — the parity of unix's 0700/0600 discipline; the installed
Store file answers its own entry check at every open, its directory is
restricted when the route creates it (a caller-pre-created directory is
accepted as the caller shaped it — hence the sidecar note below), and a
link planted at the file's spelling is refused, never followed. A home that
lost its owner (restored from backup, moved between machines) answers
`Unauthorized`; an administrator recovers it from an **elevated** console
(runbook verified end to end, round 6): first
`takeown /f <home> /r /d y` — and the same recursively on the volume
tree, when the volume sits outside the home; taking ownership rewrites
the files' OWNER RIGHTS ace into an inherit-only form, so every file
must be re-granted explicitly — then
`icacls <home> /inheritance:r /grant:r '<user>:(OI)(CI)F'` and the same
grant on the volume tree, and finally a plain
`icacls <file> /grant:r '<user>:F'` on the installed Store file and on
`<home>/<project>/project.record` (an `(OI)(CI)` grant gives a *file*
nothing) — before the route accepts it again.

Operational notes from the 2026-10-31 production-readiness round: the
audited Win32 calls spell paths verbatim (`\\?\`), so trees deeper than
MAX_PATH initialize — but the host process's own plain-spelled
operations still need the host's long-path relaxation (a
`longPathAware` manifest, or an OS build that no longer requires it —
round-3 A1 measured plain 308-character paths working on one such
host); keep sources and homes within ~260 characters on hosts whose
policy is not confirmed.
Windows lacks the 64 MiB disk-headroom pre-admission macOS and Linux run
(a filling disk surfaces mid-transaction as the typed `Capacity`
refusal instead); SQLite's `-wal`/`-shm` sidecars inherit the
directory's DACL, and every open tightens the ones it finds back to
owner-only (a crash's leftovers wait for the next open); a writer-lane
squatter parks every Store call for as long as it holds the mutex (the
readiness wait has no bound, exactly as Linux's blocking flock) — round
5 added two neighbours of that local-adversary availability class: a
pre-planted lane object carrying a foreign DACL makes the next open
fail (currently typed `Malformed`, recorded for a later typing pass),
and any local user can enumerate lane names in
`\BaseNamedObjects` (a digest of the directory that reveals write
activity, not the path); a caller-pre-created wide directory also lets
another user pre-place `-wal`/`-shm` the per-open tightening cannot
re-own (create the volume owner-only, as the route's own layout does);
the lane's name is the canonicalized spelling, so a subst-drive spelling
that does not canonicalize to the real path can hold a second lane for
one directory — bounded by SQLite's own WAL write lock, which refuses
the overlapping writer rather than corrupting (round-6 D1 F3);
a killed process leaves one
overlay directory under `%TEMP%\layerfs-store-*` until the OS cleans
it (schema-only and never blocking the next open — accumulation is the
machine's TEMP policy); the overlay crate's two Win32 path probes
(entry identity, free space) take raw wide spellings of TEMP-local
paths — the product-source boundary keeps layerfs-overlay free of
first-party dependencies, so the shared verbatim helper is not imported
there and a TEMP tree beyond MAX_PATH fails those probes cleanly; and
a `fs.rm`/`fs.cp` over a subtree holding a name outside UTF-8
refuses with a message naming that entry (the SDK cannot spell such a
name until the byte-carrying path lands — the same follow-up as the
merge/compare fence above, which surfaces at the SDK as code `Failed`
with the cause in the message, not as the content-layer error name).
Upstream-family records pending an owner decision (round 7): the Linux
FUSE invalidation keys `Stale::Entry` on the display spelling, so a
non-UTF-8 name rebound by a merge misses its kernel dentry invalidation
(untouched by this branch); the 256-component path limit is enforced on
the workspace/rename route but not the SDK fs/init routes (live-accepted
at 257-301 components); and deep single-chain operations grow
near-quadratically (a d=250 `mkdir` measures ~1.8 s) — the engine guide's
reject-quadratic rule makes that a decision, not a footnote.

**Obtaining the addon**: `npm run build:native` in `api/` builds on
Linux, macOS and Windows (cargo `+1.85.1 --release --offline --locked -p
layerfs-node`, artifact staged to `engine/crates/layerfs-node/layerfs.node`;
Node ≥ 20 is the runtime floor); `LAYERFS_NATIVE` names another copy.

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
