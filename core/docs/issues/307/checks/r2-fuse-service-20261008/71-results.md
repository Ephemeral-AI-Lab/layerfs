# R2 native request service checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The replacement Fuse is now an active core member with one fixed daemon-wide
worker pool,16 admitted requests and2 borrowed receive slots per mount, bounded
input charges, fair runnable turns and event-driven parked work. Original failed
continuations, errors and worker joins remain owned. The daemon implements Fuse
ports over the real Owner admission/completion futures and actually admitted
Store readers. Workspace's existing native decisions and byte composition are
reused. The initial Linux adapter wires lookup/stat/open/opendir/read/readlink/
release/forget and records the selected profile separately from offered flags.

Full R2 remains ACTIVE and incomplete. Remaining work includes READDIR/RELEASEDIR
and directory-handle GETATTR, complete inline/refusal/opcode accounting, first-party
mount/plain detach, session notifications/serving/join custody, daemon assembly,
Bridge/SDK Attach/Locate/Ready, Sandbox permissions and full native Busy/drain proofs.
The application still does not expose native Ready. R3–R5 remain separate unfinished
work. No first-mount demo, kernel permissions or mounted acceptance is claimed here.

## Verification at the selected source

[Source identity52](52-source-identity.json) pins the final product/selected test
sources and compile inputs; [binary identities62](62-binary-pins.json) records
the final reader-port and Linux dispatcher/attribute binaries. Locked builds
precede all runtime selections. Each test command has an explicit100second wall
ceiling and bounded internal observations. No selected runtime timed out.

| Check | Actual outcome and scope |
| --- | --- |
| Host dispatcher | [8 PASS](33-weak-notification-tests.txt): fixed workers, bounded receive/admission pressure, terminal wakeups, stale queue/waker rejection, wake-during-poll coalescing, original failure/panic custody and absence of strong notification cycles. Dispatcher implementation is unchanged at source52. |
| Host direct service composition | [2 PASS](54-retained-input-tests.txt): three real requests park for the only Store reader while another continuation runs; reader release resumes them; open-file data survives FORGET; a128KiB mixed inherited/local window matches its independent expected bytes; exact original input/error and completion/source disposal. |
| Linux dispatcher and attributes | [10 PASS](63-linux-pinned-results.json) at pinned final binaries:8 dispatcher plus2 checked inode/attribute tests. Initial callback-family results are retained separately in [the earlier run](linux-callback-results.json). |
| Linux direct service composition | [2 PASS](linux-final-port-port-tests.txt) at the unchanged final binary recorded in62; same original-source, real reader and128KiB mixed-window cases. |
| Warning-denying Clippy | [Host](55-retained-input-clippy.txt) and [Linux](linux-final-port-clippy.txt) PASS for Fuse/Daemon all targets. |
| Format/product boundary/tooling | [Format](66-final-format.txt) PASS; [801 production files](67-final-boundary.txt) PASS; [49 tooling tests](42-tooling-tests.txt) PASS, including the new daemon→Fuse/forbidden reverse-edge and unsafe boundaries. |
| Pinned fuser provenance | [PASS](linux-final-port-fuser-integrity.txt),104 manifest/lock/config inputs checked and authorized patch verified. No additional third-party source edit accompanies this checkpoint. |

These are component functional checks with natural caches. Raw command times are
diagnostics, not speed/storage, cold-state or resident-memory qualification. Linux
reuses image `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`
and worktree-local Cargo targets. Functional Store fixtures live outside the
repository bind and explicitly use Disposable/WAL/OFF; Overlay is
MEMORY/OFF/EXCLUSIVE. `LAYERFS_CONSTRUCTION_WORKERS=1` is set for Store fixtures.
Durable is NOT_RUN — disabled by owner until explicit reauthorization.

[Review and failure analysis](61-review-and-failures.md) retains every original
compile/test failure and the source-level copy/input-custody findings. The failed
strong-waker lifetime test was fixed by weak notifications, not by dropping
original request custody. No unchanged performance treatment was resampled.
The final binary run supplies an explicit covering identity after development
builds; its passing outcome does not relabel earlier failed or narrower receipts.
The full staged whitespace check reports trailing blank lines in retained raw
test/stdout transcripts. Those original bytes are preserved. The separately scoped
source/documentation/tooling check passes; the full transcript check is not
reported as clean.

All4 owned Linux check containers were removed. [Preserved-state receipt70](70-preserved-state.json)
confirms the4 unrelated containers, protected notes and15 predecessor files remain
intact. No remote action, command supervisor, automatic operation replay, global
Store profile change or early reference retirement was performed.

Production LOC: **173827 -> 176088 (delta +2261)**. Core108410→110671; active
65536→67797; root reference65417 unchanged. The1447-line Fuse predecessor relocation
reclassifies excluded predecessors37431→38878 and excluded integration5443→3996,
with no deletion. This growth implements the request service and native read
composition; relocation is not an algorithmic simplification.
[Exact parent/staged accounting](68-exact-production-loc.json) and
[per-file classification](69-per-file-production-loc.json) use the pinned counter
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb` over first-party
Rust/runtime SQL, excluding tests/docs/tools/third-party/generated source. Final
staging adds documentation/receipts only; the counted product trees are checked
again before commit and against the resulting commit.

Implementation ownership and lifecycle limits are documented in
[architecture75](../../../../architecture/75-native-request-service.md). The next
work remains completion of the same R2 native filesystem, not a new assignment
or a pause for another approval.
