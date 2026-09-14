# Host operation adapter verification ledger

The adapter delegates filesystem operations through the existing authenticated
`BackingConnection`. The host returns a known operation result inside a successful
transport frame. Unknown or cancelled mutations retain their exact session request
and admitted ownership until that same request is resolved. This component does
not yet replace the daemon entrypoint/control/cache-reconciliation lifecycle.

| Evidence | Check identity and configuration | Result / validity |
| --- | --- | --- |
| `host-client-attempt01.json` / `.log` | Two `host_client::tests`, macOS arm64 Rust 1.96.0, live feature; exact source hashes in receipt | PASS 2/2. Lost reply applies once; known NoSpace and Unknown remain distinct; a new call resolves the exact pending request first. Cancellation retains the pre-admitted lookup owner in Pending; recovered abandoned response rolls back the exact reference. Successful submitted entry retains its kernel reference; a partially emitted page releases only its unemitted suffix. |
| `wire-codec-attempt01.json` / `.log` | Two `host_wire::tests`; live feature, same host/toolchain | PASS 2/2. Bounded 1 MiB requests and namespace validation, explicit mutation sequence, response identity, known errors and Unknown encoding. Subsequent request-digest helper is separately checked in root-owned evidence. |
| `legacy-kernel-{refs,base,page}-attempt01.json` / `.log` | Initial three legacy regression selectors | NOT_RUN, invalid evidence: zero tests matched because their real module is `immutable_acquisition_tests`. Exit zero is not recorded as a pass. |
| `legacy-kernel-{refs,base,page}-attempt02.json` / `.log` | Exact corrected existing `kernel_references_preserve_unlinked_contents_until_last_forget`, `kernel_reference_keeps_old_base_contents_after_unlink_and_root_change`, `kernel_page_rollback_overflow_and_detach_balance_pins` | PASS 1/1 each. Required narrow invalidation: `KernelReferences` now dispatches between the legacy owner and host client; its Drop/submitted/partial-page branches changed. Other legacy tests were not rerun. |
| `host-client-linux-check-attempt01.json` / `.log` | `cargo check -p layerfs-fuse --features live --target aarch64-unknown-linux-musl` | BLOCKED build: installed Rust target lacked `aarch64-linux-musl-gcc` for blake3's C source. No Linux adapter compile pass. |
| `host-client-linux-check-attempt02.json` / `.log` | Same command with existing Zig CC/AR configured directly | FAIL toolchain setup: cc-rs also supplied Rust's target triple, which Zig expects to be normalized. Original build output retained. No source or feature relaxation. |
| `host-client-linux-build-attempt03.json` / `.log` | `cargo zigbuild -p layerfs-fuse --features live --lib --target aarch64-unknown-linux-musl`, existing installed cargo-zigbuild and Zig 0.16.0 | PASS library build in 9.69 s. The existing wrapper supplies the correct compiler/linker triple. Linux FUSE async dispatch and owned read/write reply paths compile. No Docker or benchmark workload ran. |

`FilesystemPort` now has asynchronous defaults for metadata/readlink and handle
release/directory pin operations. Existing asynchronous FUSE dispatch sites await
them so the remote client never nests `Runtime::block_on` inside a callback future.
Mount flags, permission checks, reply shape, existing error handling and inode
translation were retained.

Still pending: the real HostClient-to-HostOperations Store-fixture TCP test (added,
not yet run at this ledger entry), control-service/notifier SDK reconciliation,
actual daemon/runtime switchover and supported-surface V1 acceptance. A passing
Linux build is not a mounted-kernel correctness result.

The subsequent actual-network check is now PASS:
`host-authenticated-tcp-attempt01.json` / `.log` runs
`authenticated_client_and_sdk_share_one_host_root_and_owned_snapshots` against
the real capability-authenticated BackingServer and HostOperations handler.
The unchanged compiled binary was reused from the cleanup component's exact
source receipt; its SHA-256 is recorded. Client writes and SDK edits share the
same HostOverlay, retained canonical snapshot bytes remain readable, aliases
reuse inode identity, open-unlinked bytes survive until unpin, real backing fsync
completes, and partial kernel-page references are balanced by detach.

Later lifecycle/SDK `OperationGate`, immutable-read-lease RPCs and shared
LiveControl extraction change relevant adapter dependencies. The targeted
cancel/detach, network and Linux-build results require an affected refresh once
those changes stabilize; the known-request acknowledgment test can be retained
unless its call/replay implementation changes. SDK cache mechanism observations
are separately recorded in `sdk-cache-mechanism.md`; they do not resolve V1.

The resumed Linux SDK/control build is recorded separately in
`host-client-linux-build-attempt04.json` / `.log`: PASS in 5.92 s with the
current `--features proxy` invocation shown verbatim in that receipt. It refreshes
the changed SDK actor, shared control handler and callback gate dependencies.
Root subsequently built the actual Linux FUSE helper and daemon with explicit
host-authority startup selection; root owns that binary seal and mounted checks.

The new host SDK coordinator is described in `sdk-host-coordinator.md`. At this
entry its three Store-fixture tests and exact installed-generation snapshot test
are compiled-source candidates, not yet test passes. Source freezing/building is
shared with the first coherent Workspace/Store capacity candidate; no benchmarks
are running. Existing unaffected passes remain retained above.

Host SDK coordinator first execution is now PASS 4/4 in 0.10 s:
`sdk-host-attempt01.json` / `.log`. The new exact installed-generation snapshot
check is PASS 1/1 in 0.02 s: `sdk-installed-snapshot-attempt01.json` / `.log`.
Both use the same already-built workspace binary
`0869ef5dcda8f36013d124129753dd125bd1d3a25101f13d51c9477cebcfb4a7`,
with source identities from
`../phase4-candidate/capacity-compile-attempt03-source.json`.
No successful test was rebuilt or rerun to obtain those results.

The four coordinator checks cover unknown BEGIN cancellation and lost CANCEL,
reader quota before BEGIN, exact input and pinned target identity across rename,
owned lease bytes despite later writes, lost APPLY without duplicate mutation,
and verified-detach retirement of a lease never delivered to the daemon while
an independent Snapshot remains readable. The shared build's original adapter
compile failures remain in the capacity evidence; they are not test failures or
passes. The new daemon absent-scope CANCEL case still awaits its focused check.

A source audit then found the missing existing `CanonicalPath` entry guard. The
new exact `invalid_sdk_path_is_rejected_before_control_or_target_pin` reproducer
FAILed in `sdk-path-attempt01`: `/file` reached BEGIN once. The original failure
and source seal are retained. The repair reuses the existing canonical validator
before any pin/reservation/control scope. A targeted rerun of this failed check
and one valid retry case is required; the three other valid-path state-machine
passes above retain their unchanged ownership/cancellation/admission invariants.

Root's first mounted actual-helper run retained all SDK mapping, descriptors,
namespace and owned C1/C2 assertions but failed at normal unmount (`target busy`).
That original run is **FAIL**, in `mounted-host-attempt01.{json,log}`. The client
retained its preopened mount descriptor across prepare_shutdown. The fix takes
that root slot before unmount, lets in-flight syncfs retain its own Arc until
completion, rejects later root attachment, and restores explicit local SHUTDOWN
handling. Native `sdk-shutdown-attempt01` is PASS1/1; the actual corrected mounted
check is still required and is owned by root.

The new absent-scope cancellation check is PASS1/1 in `sdk-cancel-attempt01`.
Only the affected prior protection/cancellation regression was refreshed:
`sdk-protection-attempt02` PASS1/1. The shifted-terabyte-suffix/release retry proof
is unaffected by cancellation settlement and is retained. These three checks use
the Fuse test binary recorded by `sdk-shutdown-compile-attempt01` with no source
drift; no timing samples or #122 workload were run.

The SDK path repair is verified: `sdk-path-attempt02` PASS1/1 (0.02s), and the
representative valid retry `sdk-valid-retry-attempt02` PASS1/1 (0.04s). Current
native workspace binary is
`cbabe2def5568d913f95c2b4f21506d490dd138f8c206894f946b8cd36971b06`;
`sdk-path-compile-attempt02-source.json` seals the source with no build drift.
Only the existing canonical path guard and its narrow comment were added. The
other three valid-path SDK state-machine passes and exact installed-generation
pass remain valid for their unchanged dependencies and invariants. The original
invalid-path FAIL is retained.

Owner's bounded-stop relay now limits remaining execution to the observed
shutdown repair's actual mounted rerun, the already successful spill fixture,
and the focused maintenance oracle rerun, followed by repository publication and
handoff. Broader production migration, capacity integration, V1 work and final
benchmarks are pending; no terminal feature or benchmark PASS is claimed.

Corrected Linux helper build `sdk-shutdown-linux-attempt02` is PASS (5.67s),
source before/after identical. Artifact SHA-256:
`29f56dcd24aad94149510d8f0474a2d9895b96ea7904465404aa39172a33e513`.
This is the helper for root's required actual mounted shutdown rerun. The
maintenance oracle check also passed independently in
`../phase4-candidate/maintenance-attempt02`; its two unaffected passes were
retained. No further SDK/FUSE production work is authorized under the owner's
bounded-stop relay except repair of that scoped mounted shutdown check.
