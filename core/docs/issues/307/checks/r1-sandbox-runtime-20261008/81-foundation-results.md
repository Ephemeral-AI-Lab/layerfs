# R1c ordinary Engine runtime foundation results

> **Status:** Implemented foundation checkpoint; R1 remains IN_PROGRESS.
> Parent `cd541ae927ab2728b734f49a78c6f8e8d3498a4d`; final
> [source/harness identities](71-final-source-identity.json). No release or full R1 acceptance.

Active `layerfs-sandbox` now has real standard Engine1.54 Unix command creation,
consumed launch, stdin/output/runtime owner splitting, exact stream delivery and
coherent structural status. It has no old API-core/Server/data-runtime dependency,
source include, reconnect/replay, command classifier, lifetime output/runtime cap,
filesystem registration or daemon launcher. Root reference is unchanged. The old
Sandbox source/test bytes moved intact to excluded `layerfs-sandbox-legacy`; only
its manifest package name changed. This is relocation, not retirement/simplification.

| Selected evidence | Outcome and scope |
| --- | --- |
| Locked all-target host/Linux Sandbox builds | PASS [67](67-start-selector-build.txt), [75](75-final-linux-build.txt); source hashes checked in Linux builder; tests/examples compile before execution |
| Warning-denying all-target host/Linux Sandbox Clippy | PASS [70](70-final-clippy.txt), [73](73-linux-clippy.txt) |
| Formatting / product guard / tooling tests | PASS [61](61-final-format-check.txt), [54](54-final-boundary.txt):746 Rust/SQL files, [57](57-tooling-tests.txt):47 tests |
| External public Unix protocol/custody cases | PASS9 macOS [45](45-reviewed-protocol-tests.txt), PASS9 Linux [77](77-linux-protocol-tests.txt); affected final lost-Start selector proof PASS1 [68](68-start-selector-tests.txt), exact artifact [69](69-start-selector-test-identity.json) |
| Actual macOS-controller/Linux Engine ordinary execution | PASS [53](53-reviewed-native-proof.txt): UID501/GID20, Groups20, CapEff/CapAmb0, NoNewPrivs1; stdin/stdout2097152B each, stderr17B, exact Exec/Container IDs, original pre-start null exit and actual exit37 |
| Explicit owned fixture lifecycle cleanup | PASS [58](58-owned-runtime-cleanup.json): original acknowledged owned container stopped/removed once after commands completed. No per-command cancellation or native drain claim |
| Predecessor preservation | PASS [63](63-predecessor-preservation.json):six original non-manifest files, exact bytes unchanged, no execution/retirement |
| External cancellation client/access | NOT_QUALIFIED [38](38-runtime-client-probe.json):client absent; [39](39-containerd-reachability-probe.json):VM binary bind unavailable, no acknowledged create; exact current runtime facts [59](59-runtime-components.json) and source-supported next route [78](78-cancellation-prerequisites.md) |

Protocol cases cover giant ignored Arguments, escaped/duplicate deciding keys,
null/overflow/wrong type, mismatching actual IDs, truncated creation/Start/header/
maximal u32 frame, upgrade read-ahead/media, strict chunk framing/trailers/profile,
original HTTP diagnostic and positive sink progress with pending bytes. They do
not replace overload/slow-sink/resident-memory/long-lived descendant qualification.
Metadata depth/header/profile limits are explicit; no claim of unrestricted JSON,
universal HTTP support or numerical memory bound. Runtime process exit, pipe EOF,
output disposition, descendants and filesystem owners remain distinct.

Actual proof has a9s complete functional stop; ordinary product streams have no
automatic runtime timer. Host/Linux external tests have explicit100s stops; no
test reached a wall ceiling. These are functional evidence with natural caches,
not performance samples, cold eligibility or a speed/storage gate. No Store is
opened in runtime-only proofs. Global Store policy remains explicit
Disposable/WAL/OFF only; Durable NOT_RUN disabled by owner. Construction workers1
and ARM64 flags unchanged. Existing nix0.31.3 poll capability is the sole external
edge; no new package version, third-party modification or fuser build occurs.

All earlier failures are retained: visibility/type/name errors [04](04-initial-build.txt),
header status-output signature [08](08-ownership-build.txt), Clippy [11](11-initial-clippy.txt),
fast-peer/deadline and incomplete giant fixture [15](15-protocol-tests.txt), missing
socket import [18](18-poll-build.txt), inherited nonblocking peer [24](24-affected-tests.txt),
missing diagnostic export [41](41-final-custody-build.txt), large value errors [46](46-reviewed-clippy.txt)
and fixture prefix lint [49](49-final-host-clippy.txt).19/22/23 mistakenly used the
old artifact after the graph changed and are invalid for the poll source; they
remain exact raw diagnostics. Repairs preserve bounds/workload/policy and use
actual source/artifact selection, not replay or relaxed treatment gates. The
previous actual stream PASS [33](33-native-streams-proof.txt) remains;53 reran
because strict media/custody and actual identity qualification changed.

R1 still requires real Sandbox lifecycle/SDK facade/daemon authentication and
install-to-ControlReady, borrowed shared volume with separate Overlay, protected
config/Store/proc/device access and no key env/argv, qualified process-specific
external-runtime cancellation, and exact failure/delete custody. R2–R9 native
mount/mutations/live Commit/sustained drain/qualification/conditional retirement
remain unchanged. The next goal turn continues those requirements, never treats
this narrower actual foundation as full completion.

Combined Sandbox/Daemon/SDK/Bridge all-target compile with the unified existing
nix feature set is PASS [82](82-combined-feature-build.txt); this is compile scope,
not another test execution/whole-core CI gate. Prior standalone family evidence
is reused only where operation behavior is unchanged.

Production LOC:167277→169065 (delta+1788), core101860→103648,
active58986→60774; reference65417 unchanged. Excluded predecessors
36325→37431 and excluded integration6549→5443 reflect the unchanged1106-line
Sandbox relocation, not growth/deletion/simplification. The [exact comparison](84-exact-production-loc.json)
uses the pinned counter and exact parent/staged snapshots, excluding tests/docs/tools.

Full staged whitespace check is FAILED(exit2) for exact raw test stdout only
[85](85-full-whitespace.txt). Trailing failure-line spaces/blank lines are retained.
[Scoped source/docs check](86-source-doc-whitespace.txt) is PASS with this
campaign's raw.txt receipts excluded; no full-check PASS is claimed.
