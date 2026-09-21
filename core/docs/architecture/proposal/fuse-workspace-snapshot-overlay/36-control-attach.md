# Authenticated Attach and current daemon ownership

> **Status: implemented and verified in the declared actual daemon/Linux scope; target v0.1.7, not released.**
> Implementation parent: `3e92fe277a0379fd85f158b858085af3ecb2e2d5`.

The daemon accepts one new managed name and nonzero incarnation through
WorkspaceAttach, profile 3/opcode 13, independent grant bit 16. Valid grant masks
are 0..31. Request bytes, maximum name length, empty authenticated input and
1..5000 ms budget match existing lifecycle controls (124-byte maximum request).
The startup AttachOptions supplies the immutable Store, base, access and UID/GID
profile. Requests cannot supply native paths, arbitrary Store authority, writable
access or budgets. Existing headless and --mount-readonly launch grammar remains.

Attach requires the current owner to be absent or successfully closed and
unmounted. A closed Workspace normally has stopping=true; its closed state permits
replacement. An open, merely unmounted, stopping-but-not-closed, mounted or unresolved
owner refuses replacement. There is no implicit Unmount, CloseClean or Mount.

Daemon lifecycle assembly now owns one shared current selector/capability and
optional native MountHandle. Both authenticated control and signal shutdown use it.
WorkspaceHost remains the sole registry and failed-resource owner. Before entering
native Attach, the requested selector replaces the old closed capability in the
slot; the old capability is held locally during the attempt. Success installs the
returned Workspace. Failure restores the old closed capability only after an exact
successful NotFound observation. Failed inspection preserves the attempted selector.
Existing allocation charges on both capabilities remain live during that interval.
The startup Workspace local clone is dropped after readiness, so it cannot survive
all later replacements as an additional owner.

Attach has its own response tag 15, WorkspaceAttachWire, at most 100 bytes:
Completed or Retained(Code). Retained includes unresolved custody and does not claim
that a physical resource necessarily remains. All defined Code classifications are
permitted only by this Attach outcome; existing Mount/Unmount/CloseClean validators
are unchanged. A Service failure with unknown=true maps to Code::Unknown even if
its primary code differs. Original native causes remain in WorkspaceHost. Confirmed
absence returns an ordinary Failure and restores the previous target. Missing,
malformed, wrong-operation or wrong-identity mutation terminals remain Unknown and
never authorize replay; Status is a new observation, not a replacement receipt.

Healthy and closed healthy Status retain their existing wire shape. A selected
failed/unresolved attachment returns WorkspaceAttachmentWire, tag 16, at most
103 bytes: Attaching, or Failed(original cause, optional cleanup cause, Running or
retained mount-directory/metadata-arena/backing-directory flags). Only the matching
Status request accepts this response. Status never changes or disposes selection.
If exact inspection proves NotFound, it reports that failure without inventing a
healthy status. CloseClean can dispose that proven-absent selector, or explicitly
call native failed-attachment cleanup. Successful failed-owner cleanup clears the
selection; no fictitious closed Workspace is created. Mount/Unmount require an
actual selected Workspace capability. Requests for a former selector are denied.

One immediate lifecycle try_lock serializes control ownership changes. Its mutex
is outside Workspace semantic locks and the registry; it may span native lifecycle
I/O but does not serialize ordinary FUSE callbacks. The one authenticated session,
Q=0 policy, pre-input authorization, complete END_INPUT, expiry recheck and stopping
checks remain. Exact target checking follows complete input and is repeated in the
held owner slot before mutation. Mutations retain the original deadline and 100 ms
terminal reserve; kernel syscalls and scheduling are observed, not preempted.
Service rejects the new control before Store admission. No additional transport,
queue, worker, replay cache or semantic registry is introduced.

Signal shutdown joins control, then unmounts and closes the current owner. Incomplete
cleanup remains selected until another explicit signal. Initial CLI Attach failure
also preserves a pending selector when ownership remains or inspection fails;
exact NotFound or invalid-selector refusal can release the host. The unserved
listener is closed and cleanup uses the original startup deadline; a later signal
admits a new attempt and completion returns the original startup error.

## Declared verification

Locked whole-core host/Linux tests, bins/examples, all-target warning-denying
Clippy, fmt and the product boundary/self-tests cover the frozen source. External
Bridge tests exercise request/codec bounds, all defined cause classifications,
strict response correlation and unknown terminals. Actual daemon/Linux selections
cover new-target attach/read/lifecycle, shutdown of the new target, independent
authority, open-owner refusal, lost terminal, malformed input/headroom, native
collision, retained failure/explicit cleanup and failed initial CLI attachment.
Each selection reuses independent bytes from the closed read-only fixture, pins
binaries/images/product/caller identities and uses a fresh output with the existing
60-second complete-command budget. Runs and builds are serial with Cargo jobs 2,
Docker quota 2 CPUs and construction workers 1. Failures stay recorded.

These are functional ownership proofs. They make no warm/cold throughput,
RSS/cgroup, durability, writable-history restart, full R4, DSH mounted upload,
incremental Commit or R6 qualification claim. Those later operations remain open.


## Source and fixed ownership layout

Frozen product input seal:
`67a94d08f5455010f84423f65c0714543461fd25b6020d2f38e85d916db9e7d2`.
Production LOC: **109066 → 109490 (delta +424)**; core 43649 → 44073 (+424),
reference 65417 unchanged. Daemon adds 212, Bridge 210 and Service 2 production
lines. The unchanged counter and full per-file/recursive-folder inventory are in
[31](31-source-map-and-loc.md) and [the inventory](evidence/control-attach/source-loc.json).
There is no reference retirement, relocation, third-party change or dependency addition.

Locked Rust 1.85.1 Linux AArch64 DWARF records the shared lifecycle Arc allocation
as 312 bytes, compared with the previous mount-owner Arc's 128 bytes: fixed-object
delta **+184 bytes**. Lifecycle is 296 bytes, Slot 176, Selected 72, AttachOptions
104 and control Target 16. String headers are included; heap storage for the
profile/current names is separate. Both name lengths are bounded by 63 bytes;
actual heap capacities were not dynamically measured, and length is not reported
as capacity. Transient ownership includes the prior closed selection, one attempt's
options and bounded wire identities. Existing Workspace charges remain live.
No worker, queue or mount-FD capacity increases. These object layouts exclude
allocator overhead, whole-process heap, RSS and cgroup peaks.

## Core verification and retained unsuccessful checks

The frozen source has 667 unique passing host tests and 3 ignored tests. Host
verification completed in two parts: the initial whole-workspace invocation
passed Bridge/content (316 tests), then stopped at the daemon's obsolete test
assertion that grant mask 16 was invalid. The corrected mask is 32. The remaining
workspace crates passed 351 tests/3 ignored; the already-passing Bridge/content
crates were excluded from that continuation. The original invocation remains FAIL.
Linux whole-workspace tests passed 665 tests/126 ignored. The ignored native
selections are independent of the authenticated-control selections below.

Host/Linux whole-workspace bins/examples and warning-denying all-target Clippy
passed. Formatting, the 243-file product boundary guard and all six guard
self-tests passed. An earlier all-target host check is retained as preliminary;
a subsequent failed build reported one missing Some() at the startup constructor
call after its signature changed. That call was corrected before final checks.
No historical/preliminary check is relabeled as final-source qualification.

All actual control proofs are serial, two-CPU Linux runs with independent fixture
byte copies, original 60-second complete-command budgets and immutable archived
host/Linux executables. Caller dependencies include the actual imported modules,
including the opaque lost-result proxy, native frame parser and isolation helper.
A caller-only correction receives another input manifest and fresh attempt path;
passing selections are not repeated. The following test-oracle failures remain:

- `attach_refusals-01`: the overlength managed name is correctly rejected as
  Capacity by the existing bounded decoder; the caller expected InvalidInput.
  Correcting that one expected classification leaves product bytes unchanged.
  The failed attempt's forced teardown is not normal-cleanup qualification.
- `attach_startup_failure-01`: a paused Service encounters the existing five-second
  handshake cap before the ten-second Attach deadline, allowing normal native
  failed-attachment cleanup. The test incorrectly expected retained ownership.
  Its first observed diagnostic was not flushed before the next read failed;
  that missing original line is not reconstructed as raw evidence.
- `attach_startup_failure-02`: successful handshake/HELLO followed by a completely
  silent hold of the actual 142-byte terminal hits the separate five-second
  I/O-progress cap. The raw proxy reports client EOF after 5.213387916 seconds and
  the saved daemon stderr reports Service Io. The absolute Attach deadline was
  not reached, so retention was again an incorrect test expectation.

The corrected startup schedule sends only actual bytes from that same terminal
at intervals below the existing progress cap and withholds a suffix until caller
EOF. It changes neither the absolute ten-second startup deadline nor either
five-second transport cap, synthesizes no keepalive/frame, and records exact
forwarded offsets/timestamps and the joined upload worker. It is a functional
absolute-deadline check, with no performance claim.


## Actual selection results

All 15 registered selections pass; no registered selection remains NOT_RUN.
The three unsuccessful attempts remain FAIL with their original receipts. Every
passing row records actual Docker NanoCpus=2,000,000,000, matching product/binary/
caller identities, unchanged original and cloned read-only databases, and normal
cleanup. The retained prior-round stopped diagnostic container remains unrelated
to these new successful cleanup receipts.

| Selection/attempt | Status | Driver seconds | Complete external seconds |
| --- | --- | ---: | ---: |
| attach_success-01 | PASS | 4.193235750 | 4.294964292 |
| attach_signal-01 | PASS | 1.739712625 | 1.872946458 |
| attach_authority-01 | PASS | 5.527812458 | 5.651957708 |
| attach_open-01 | PASS | 2.155351625 | 2.303787292 |
| attach_stopping-01 | PASS | 1.642195208 | 1.769404292 |
| attach_unresolved-01 | PASS | 2.032994000 | 2.169423375 |
| attach_loss-01 | PASS | 1.818325750 | 1.918911833 |
| attach_refusals-01 | FAIL | 1.270369208 | 1.387920875 |
| attach_refusals-02 | PASS | 2.395105833 | 2.521239125 |
| attach_expiry-01 | PASS | 10.819770209 | 10.921374583 |
| attach_collision-01 | PASS | 2.092481334 | 2.190233334 |
| attach_failed-01 | PASS | 2.902916375 | 3.040947375 |
| attach_startup_failure-01 | FAIL | 5.848147417 | 5.974785458 |
| attach_startup_failure-02 | FAIL | 6.149282208 | 6.262857958 |
| attach_startup_failure-03 | PASS | 11.202889042 | 11.324524750 |
| mount_session_failure-01 | PASS | 1.745618166 | 1.867655958 |
| close_cleanup_failure-01 | PASS | 1.415265750 | 1.506773375 |
| shutdown_retained-01 | PASS | 11.396309083 | 11.533927333 |

The successful startup case received real server frames of 48, 38 and 142 bytes.
It forwarded the last frame's original length header and four 16-byte ciphertext
prefixes at 2.005224333, 4.007649333, 6.012832208 and 8.014668875 seconds, leaving
78 bytes withheld until caller EOF at 10.054826000 seconds. The upload worker
joined. The daemon retained its failed owner, exposed no ready control listener,
cleaned on explicit SIGTERM, and exited with the original Service Io error.

The remote failed-Attach case retains an empty native owner after its original
request deadline, exposes Failed Status with the native original/cleanup causes,
refuses Mount/replacement, and completes explicit CloseClean without Service work.
It then attaches a fresh incarnation and reads actual mounted bytes. Nonempty
retained mount/backing/arena custody is covered by the separate native round35;
this round does not relabel that proof as an actual remote failure selection.

Remaining qualification: remote nonempty retained Attach resources, deterministic
signal overlap inside Attach I/O, writable startup/Commit, full R4, namespace and
larger-input work, complete preinstalled DSH upload and matched R6. No full Pair1
completion or issue closure follows from these functional selections.


[Original input manifest](evidence/control-attach/inputs-01.json),
[refusal caller correction](evidence/control-attach/inputs-02.json),
[first terminal-delay caller](evidence/control-attach/inputs-03.json),
[final startup caller](evidence/control-attach/inputs-04.json),
[host check commands](evidence/control-attach/checks/commands-01.json),
[Linux check commands](evidence/control-attach/checks/commands-02.json),
[compiler layout](evidence/control-attach/checks/layout.json),
[independent review record](evidence/control-attach/source-review.json), and
[lossless raw-log index](evidence/control-attach/raw-log-manifest.json)
retain exact identities, commands, qualification and unsuccessful attempts.
Original uncompressed artifacts remain in this worktree's
`core/target/pair1-evidence/control-attach`; fixture databases and executable
archives stay outside Git.


All four caller input manifests have recoverable exact source bytes: 14 unique
caller/helper snapshots cover all 44 manifest links, with no unavailable version.
Three prior caller variants were recovered after the runs by reversing recorded
caller edits and accepted only when their SHA256 matched the original manifest.
This is source recovery, not rewriting a receipt or reproducing a measurement.
[Snapshot linkage](evidence/control-attach/caller-snapshot-linkage.json) records
creation/recovery methods; [the archive index](evidence/control-attach/caller-snapshot-archive.json)
links lossless compressed bytes. Restore them to their recorded repository paths
in an isolated caller checkout when reproducing a historical attempt.

[Complete attempt index](evidence/control-attach/functional-index.json) includes every
command, driver/external wall, caller variant, cleanup result and retained failure.
