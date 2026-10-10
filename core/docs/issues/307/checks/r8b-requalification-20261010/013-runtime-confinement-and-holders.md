# Actual runtime confinement and held-mount Unmount: exploratory observations

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-10-10 at `388503a77` (product tree `9a76077239b4`, unchanged). All
runs used the retained release runtime and daemon binaries of registration v2
(SHA-256 `a553bc6a…9843`, `068df979…6d2e`) over the owned full-fixture volume
`layerfs-r8-full-proof-20261010-2cd69f68db34`, in the pinned image, command
identity 501:20. They are exploratory: one run each, no timing claim, no
admission. The judged tools run once more at the final registered identity.

## What was run

| Receipt | Tool | Outcome |
| --- | --- | --- |
| [008](008-confinement-exploratory/) | `run_confinement.py`, first version | Tool FAIL from a harness defect: the probe listed its own transient listing descriptor as inherited. Product cells as in 011 |
| [010 shared](010-namespace-holder-shared/), [010 private](010-namespace-holder-private/) with [tools](009-namespace-holder-tools/) | Unjudged diagnostic: a descendant keeps its working directory in the mount, in the daemon's mount namespace or in a private user and mount namespace | Completed; see below |
| [011](011-confinement-exploratory-2/) | `core/benchmark/r8-tools/run_confinement.py`, descriptors asked by path | PASS: 84 protected cells refused, 21 recorded cells |
| [012 descriptor](012-mount-holder-exploratory-descriptor/), [012 mapping](012-mount-holder-exploratory-mapping/) | `core/benchmark/r8-tools/run_mount_holders.py` | PASS: sibling Unmounted and Gone while held; held Unmount is the reversible `Busy` |

## Confinement matrix (011)

One unprivileged command started in Workspace A of a daemon with two
Workspaces. Every cell is a real system call.

- Refused, 84 of 84 protected cells: open for read and write and directory
  listing of the Store file, its `-wal` and `-shm`, the Overlay database and
  directory and `daemon.setup`, by direct path and through `/proc/1/root`;
  `/proc/1/{cwd,exe,root}`; `/proc/1/{fd,fdinfo,map_files}` and every
  `/proc/1/task/*/fd`; a one-byte read of
  `/proc/1/{environ,maps,mem,smaps,pagemap,auxv,stack,io}`; `/dev/fuse` open
  and a `mknod` twin; `umount2` plain and detached, `umount`, `umount -l`
  (`fusermount` and `fusermount3` are absent from the image); `kill(1, 0)`,
  `ptrace` attach and `process_vm_readv` (`EPERM`); a `tmpfs` mount over the
  Workspace; `umount2` of the sibling.
- Identity: uid 501, gid 20, `CapInh/Prm/Eff/Amb` all zero, `NoNewPrivs` 1,
  `Seccomp` 2; descriptors 0, 1, 2 only.
- Root observer: Store and Overlay directories 0700 root, files 0600 root,
  `/dev/fuse` 0600 root; container `CapAdd [CAP_SYS_ADMIN]`,
  `no-new-privileges=true`, `apparmor=unconfined`, not privileged.
- Both Workspaces and the controller's own Status answered after the probe.

Recorded, not judged (no product declaration exists to judge against):

| Observation | Value |
| --- | --- |
| `unshare(CLONE_NEWUSER \| CLONE_NEWNS)` by the unprivileged command | **allowed**. Inside it the protected paths stay `EACCES`, `umount2` of the Workspace copy is `EINVAL`, and the Workspace mount is present in the private mount table |
| Sibling Workspace B from a command started in A | listable (same uid, same mount namespace) |
| Connect to the control listener `127.0.0.1:30421` | connects; closed without authentication |
| `/sys/fs/fuse/connections` | listable and empty: no abort control exists in the container |
| `lstat` of `/layerfs-store/global` and `/layerfs-local` | allowed (mode only); everything beneath is `EACCES` |

## Normal Unmount against a held reference

| Holder (descendant of an ordinary command) | Sibling B while A is held | Normal Unmount of A |
| --- | --- | --- |
| working directory, daemon's namespace (010 shared) | not exercised in that run | `ControlRefusal { code: Busy, phase: "unmount:kernel" }`, "no terminal effect"; mount still present |
| open descriptor (012) | Unmounted, Gone | same `Busy`; mount still present |
| shared read-only mapping, descriptor closed (012) | Unmounted, Gone | same `Busy`; mount still present |
| working directory in a **private user and mount namespace** (010 private) | not exercised in that run | after 5.0 s: `Retained(TeardownCustody { stage: Join, detached: true, … loops_exited: 0 … })`. The daemon's mount row is gone, the holder's copy keeps the connection alive, and the mount and fence threads of that namespace end only when the holder exits |

The last row is the case the mount propagation contract names (specification
section 10.3: a namespace that copies a mount must leave normal unmount
truthful, or a successful detach must dispose every copy). Neither happened:
the kernel's plain `umount2` in the daemon's namespace saw no user of its own
mount, detached it, and the connection could not drain because the copy was
still referenced. The Workspace ended in retained teardown custody, which has
no exit short of stopping the container, caused by an ordinary unprivileged
command. With the current Sandbox topology Force is not available either
(no abort control is bound).

This is a failure of FP-22-FS at the actual topology on exploratory evidence.
It is not repaired in this run: the specification forbids choosing a
propagation or namespace policy by assumption, and each candidate is a
topology decision.

| Candidate (inference from kernel behaviour, none verified here) | What it would give | Limit |
| --- | --- | --- |
| Mount each Workspace under a shared parent mount, so a less privileged copy is a slave and the plain `umount2` busy check covers it | truthful `Busy` for a copy that keeps the default propagation | a command that makes its copy private (the default of `unshare --mount`) is outside the check again |
| Deny user-namespace creation to commands in Sandbox setup (a seccomp profile for the container; the default profile allows it because the container holds `CAP_SYS_ADMIN`) | no copy can be made | a new shipped profile and a statement that such tools are unsupported in a Workspace |
| Bind the abort control in the Sandbox and end a detached-but-undrained connection | every copy dies with the connection | needs fusectl in the container and a rule for when a normal Unmount may abort |

It is carried as a new `PENDING OWNER` question.
