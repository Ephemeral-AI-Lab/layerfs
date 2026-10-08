# R2 native lookup ownership component

> **Status:** Verified component on parent78374ced6. Full R2 Goal remains ACTIVE.
> No native Fuse mount, permissions, Ready or terminal drain acceptance claimed.

Implemented Overlay schema17 and the existing Workspace/Daemon route for native
mount/source/lookup/read custody. One final owner transaction decides current
lookup/getattr, increments a positive lookup count and retains its independent
read. The original outcome/candidate survives a failed completion without guessed
adoption. Source custody independently protects the parent/target across cold
fact rounds, FORGET and unlink. Native references retire automatically through a
64-row indexed cursor after explicit logical revocation and source/read drain.
See [architecture](../../../../architecture/73-native-read-custody.md).

| Selection | Exact evidence | Result and limit |
| --- | --- | --- |
| Initial external-test build |04/05;09analysis | FAIL: wrong diagnostic accessor; corrected to public statement array |
| Initial source plan |07/08;09analysis | FAIL: CHECK IN introduced constant ephemeral VM; contiguous BETWEEN preserves domain and hot-source invariant |
| Owner schema regression |14/15;17/18 | Original FAIL16vs17 retained; corrected assertion PASS |
| Completion ownership |17/18;21/22 | Original immediate-zero race retained; bounded publisher-release observation PASS with exact credit assertions |
| Host final selected functional coverage |[host summary](34-host-summary.json);14/15,17/18,21/22 |68tests PASS; each explicit100s wall limit; no timeout |
| Linux final selected functional coverage |[Linux summary](34-linux-summary.json);[records](27-linux-tests.json) |69tests PASS; normal tests100s; actual daemon application256804750ns within9s |
| Locked builds/examples and warning-denying Clippy |19/20/22;26/27/28 | Overlay, Workspace and Daemon all-targets PASS on host/Linux |
| Formatting |30/33;38/41 | Original one test-assertion wrapping FAIL retained; repaired PASS |
| Boundary and tooling |39/40/41 |772production Rust/SQL files;48tool tests PASS |

Native tests cover32repeated lookup references sharing one group, exact partial
FORGET and underflow, implicit root custody, u64::MAX request keys, foreign/stale
tokens, independent source/read fencing, semantic failure/rollback custody,
hard-link identity and unlink between fact rounds. The130-inode retirement case
checks actual indexed plans and DatabaseWork, then complete namespace/resource
reclamation. Daemon native_jobs uses real explicitly Disposable installed Store
facts, fair Owner futures and retained completion custody. No test bypasses the
public product by importing private implementation.

The [final source identity](23-final-source-identity.json), exact binary hashes,
commands, image, logs and bounds are retained. [Post-check identity delta](42-final-identity-delta.json)
contains only rustfmt wrapping in a test, with unchanged product bytes. Early
Cargo per-package runtime commands compiled narrower feature graphs; their wall
is not pure execution time. Final runtime selections execute exact prebuilt
binaries. Natural caches, functional component scope: no latency, cold-cache,
throughput, RSS or storage-efficiency qualification. No SDK benchmark was run.

Global Store runs explicitly Disposable/WAL/OFF; Overlay MEMORY/OFF/EXCLUSIVE.
Durable: NOT_RUN — disabled by owner until explicit reauthorization. Construction
workers1 on Linux; no measured Commit/capture operation here. Linux runtime uses
the pinned ARM64 image and repository target-feature configuration, shared build
cache only, fresh test fixtures outside the repository bind. Native application
is the actual startup/control regression with no FUSE Ready claim. Build/test
work was serialized by the nonblocking worktree lock. Container
f50ccc88c0d6f8a3c1502ce00bb7ee5921b64dc80f99977d1029523a1bb2fc48
was removed successfully in [cleanup29](29-linux-cleanup.json).

Production LOC:171783 ->172718 (delta+935). Core106366→107301; active63492→64427;
excluded predecessors37431, excluded integration5443 and root reference65417
unchanged. Exact parent/final staged archives, pinned counter and classification
are in [final count](44-final-production-loc.json) and [per-file classification](45-final-per-file-production-loc.json).
This adds required ownership semantics, with no retirement or reclassification.

Remaining R2: native file and directory handles/cookies; actual replacement Fuse
activation with fixed K shared dispatch, receive admission and async engine/Store
ports; profile/mount/session facts; Attach/Locate/Ready; Sandbox permissions;
reversible Busy, complete normal drain and the owning native proofs. The present
revoke fence must be extended for native opens/directories before those owners
are wired. This checkpoint neither completes nor pauses the full R2 Goal.
