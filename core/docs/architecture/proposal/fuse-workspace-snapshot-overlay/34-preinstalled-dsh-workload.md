# Pinned preinstalled DSH directory workload

> **Status: prepared input and offline application startup verified; mounted upload/Commit/R6 not yet run.**
> Owner direction2026-09-22 replaces the live network-dependent npm installation selection.
> Product continuation checkpoint: `0d220870175abb6e3f162cb164dba7c49bc98f9d`.

The selected application is `npx @deepseek-ai/dsh web`. Acquire/install once outside
workload timing, then upload the complete prepared directory through the real mounted
Workspace. Network acquisition never enters that workload. This is an installation
filesystem-write workload; it does not claim that executing npm installation, its
lifecycle scripts or network resolution through LayerFS was verified.

Pinned application`@deepseek-ai/dsh@0.1.5-rc.2`, Node24.21.0, npm11.19.0,
Linux arm64/glibc2.36, Node ABI137/N-API10. Official immutable runtime digest:
`node@sha256:64af3819f9275802414d7cdc38c27e9d82bd564dec4d4da87d008255d36c63b4`;
platform manifest`sha256:91882e0e5959240d4413fc42c180022bbdd09c5491e00e75faa6c100d8d7751b`.
MacOS native dependencies cannot substitute for this Linux input.

| Prepared property | Value |
| --- | ---: |
| Regular files | 25,433 |
| Regular-file bytes | 223,614,913 |
| Directories excluding root | 3,299 |
| Relative symlinks | 10 |
| Installed dependencies | 520 |
| Largest direct-child count | 342 |
| Largest regular file | 18,259,144 bytes |
| Archive entries | 28,742 |
| Archive bytes | 274,698,240 |

The largest file is the Linux arm64 libvips shared library. Keep every installed
package/asset, including bundled unused-platform assets; no hand pruning. Preserve
regular executable/mode bits and relative symlink targets. Source shared-filesystem
symlinks report0755 while Linux canonical links may expose0777; compare link type
and target, not a nonportable symlink access mode. The lockfile SHA256 is
`20f956d5695bd5227faf533c90be6af52219da6034304af30a93241f9831c9f8`.

Prepared master: `core/target/pair1-evidence/preinstalled-dsh-01/app`.
Archive: `core/target/pair1-evidence/preinstalled-dsh-01/prepared-app.tar`, SHA256
`694eb0421c021a162c6a87f0d19208f477c0b4c064f89c787c6f0058c3d4d0b7`.
The full file manifest is retained alongside it, SHA256
`a6a447a5ef5fe01d8d28e3e1a382ae293c54f503adf60d22f1c950e19b5368cb`.
These ignored artifacts are owned by this worktree; do not reinstall for each case,
use another worktree's targets, or mutate the master. Archive verification checks
all entries. Acquisition/read/hash reuse is setup only and supplies no cold claim.

The single npm installation took156.252230334seconds of setup, with lifecycle
scripts enabled, optional dependencies included and Docker limited to two CPUs.
It is not a60-second functional selection. Offline CLI help and real Web startup
passed with Docker network disabled, fresh external DSH_HOME, the actual local
303 token/cookie exchange, HTTP200/27,724-byte HTML containing__DSH_BOOT__, and
normal SIGTERM exit. A second launch used the master read-only and compared the
full content/mode/mtime/link manifest before/after; the master remained unchanged.
No provider credentials were supplied and no inference/external-service proof is
claimed. Private temporary auth URLs/runtime files remain outside committed evidence
and the upload archive.

Two setup/verifier failures remain retained locally: Docker rejected a bare
platform manifest reference before the immutable repository digest was selected;
the first HTTP verifier lacked browser cookie handling and received401. Corrected
verification passed with unchanged package bytes and without reinstalling.
[Preparation and scope](evidence/preinstalled-dsh/prepared-workload.json),
[fixture summary](evidence/preinstalled-dsh/fixture-summary.json),
[pristine-master check](evidence/preinstalled-dsh/pristine-master-check.json),
[lockfile](evidence/preinstalled-dsh/package-lock.json).

Before the workload can run, implement actual mounted mkdir/create/write/symlink
and required portable metadata behavior, then one explicit Commit covering this
whole upload. No prepared namespace injection, hidden smaller Commits or full
resident namespace mirror. Reconcile the current shared128-record/dirty-inode and
8MiB replacement bounds in their owning algorithms. New-file construction and
existing-file editing must use their actual appropriate public paths; a large
new file does not by itself justify expanding every EditFile limit.

Later explicit incremental Commits preserve the complete installed base and later
edits. Application launch must exercise the actual mounted JavaScript/native
.node/.so files with isolated runtime state. Full shell/namespace behavior remains
separately qualified. R6 needs its own matched cache/timer/source/binary/harness
contract and exact v0.1.6 comparator; no measured performance or RSS/cgroup claim
comes from this preparation record.
