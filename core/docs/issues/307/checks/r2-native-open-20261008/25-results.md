# R2 native regular-file ownership component

> **Status:** Verified component on parentcc0b9a06a. Full R2 Goal remains ACTIVE.

Schema18 adds an indexed native_file association to the existing exact OpenFile
owner. The ordinary file-owner implementation remains authoritative; its acquire
and close internals are shared. Native Open uses the existing Workspace evaluator
and immutable fact rounds, then acquires the file and independent processing read
inside one deciding transaction. Original open candidates survive later failure
without guessed adoption. The mount, route, serial and encoded owner are checked
together on handle access; separate opens have separate nonrecycled owners.

After the last FORGET and unlink, a native file handle can retain a processing
source. That source survives later descriptor release, and getattr still reports
the retained inode with nlink0. Exact close atomically removes the native mapping
through an indexed foreign-key cascade. Native mount revocation refuses existing
file owners in addition to processing sources/reads. Directory ownership and real
kernel/drain wiring remain unfinished. See [architecture](../../../../architecture/73-native-read-custody.md).

| Selection | Evidence | Outcome |
| --- | --- | --- |
| Initial build |02/03 and[diagnosis](04-failure-analysis.md) | FAIL: locally bound closure lifetime too narrow for SourceRows callback; inline delegates preserve one semantic implementation |
| Corrected locked all-target build |05/06 | PASS; tests/examples built before runtime |
| Host functional component suite |07/08/[09](09-host-results.json),[summary23](23-host-summary.json) |71tests PASS,100s explicit per-binary wall ceiling |
| Linux functional component suite |13/15/[16](16-linux-tests.json),[summary24](24-linux-summary.json) |72tests PASS; normal tests100s; real application257505375ns within9s |
| Warning-denying Clippy |10/11,15/16 | PASS host and Linux, all targets in Overlay/Workspace/Daemon |
| Formatting, source guard, tooling |19–22 | PASS;773product Rust/SQL files and48tool tests |

Tests include full-width native request keys, multiple opens of one inode,
read-only enforcement, wrong serial/foreign mount/stale handle rejection, both
native and ordinary close paths, independent processing after final handle
release, and removed/nonregular refusal without ownership changes. The real
Daemon/installed-Store test retains original Completion credit, verifies exactly
one BEGIN/COMMIT for open, validates native source acquisition after FORGET and
confirms automatic final namespace cleanup. No timeout occurred. Original failure
receipts remain unchanged; no failed operation or unchanged measurement was replayed.

[Source hashes](12-source-identity.json), binary hashes, exact commands/image,
runtime limits and raw SQL receipts remain in this directory. No product source
changed after the final build/check identity. All runtime selections execute
prebuilt binaries and fresh fixtures. Natural caches and functional component
scope only: no speed, cold-cache, RSS or storage-efficiency qualification. Global
Store fixtures explicitly select Disposable/WAL/OFF; Overlay MEMORY/OFF/EXCLUSIVE.
Durable: NOT_RUN — disabled by owner until explicit reauthorization. The Linux
application case is startup/control regression, with native FUSE Ready=false.
The worktree lock serialized build/test execution; the Docker image and repository
ARM64 inputs are unchanged. The acknowledged owned container
13a3febc8ee231aa76c4217822e03388dc660b590dc30d5250c512363ea4973a
was removed in [cleanup18](18-linux-cleanup.json).

Production LOC:172718 ->172929 (delta+211). Core107301→107512; active64427→64638;
excluded predecessors37431, excluded integration5443 and root reference65417
unchanged. [Exact parent/staged count](26-exact-production-loc.json) and
[per-file classification](27-per-file-production-loc.json) use the pinned
production counter, excluding test/docs/tool/third-party source. No relocation,
retirement or reclassification occurred.

Remaining R2 is directory handles/cookies, actual replacement Fuse activation,
fixed K shared dispatch and deferred provider/Owner service, mount/profile/session
facts, Attach/Locate/Ready, Sandbox permission proof, reversible Busy and complete
normal drain/native acceptance. This file records a component checkpoint; it does
not complete or pause the user-requested full R2 Goal.
