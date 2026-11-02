# Provisioned upstream and bounded page-cache checkpoint

> Status: component checkpoint committed at `4090cb9a2d5fa1cd899d63c7a44537ff3f23e3cd`, first parent `86f766750029ab7f4224b587e0f20a7779813e89`.
> S0/S7–S13 remain incomplete. Functional evidence only; no performance sample.

This continues the [S7–S13 plan](IMPLEMENTATION-PLAN-S7-S13-20261007.md) through
actual Daemon upstream composition, operation-owned Workspace demand, Content
page-cache correction and a native TCP option. The primary checkout, two separately
owned notes, reference source and unrelated containers/worktrees remain preserved.
API-core, Sandbox and FUSE remain excluded; mounted Exec/Commit is not claimed.

## Product route and original custody

[Upstream](../../architecture/47-daemon-upstream.md) composes original SDK Attachment/
Calls, initialized OwnerClient and public Workspace/Content ports. It verifies
oriented peers, runtime/catalog/provider/Workspace, complete coherent BranchSnapshot,
root serial, exact StoragePolicy and separately trusted actual
`Handles::profile().persistence`. Filesystem profile never supplies durability.
Binding and Policy each have one original request; actual checked BaseView demand
precedes one local Open. Only known Opened binds. Global initialization/acquisition
belongs to the host; bind adds no whole-tree scan/copy/materialization or database.

Refusal retains original Attachment, received Messages, first provider failure or
boxed attempted Completion. Unknown Open delivery permits no replacement Open,
guessed Close or namespace-absence conclusion. Each operation owns a fresh
RemoteObjects failure scope. Failed demand preserves original PortFailure/credit
and refuses another miss without exchange; a fresh operation can use healthy Calls.
Fenced Calls stay terminal.

[Workspace scopes](../../architecture/48-shared-cache-operation-scopes.md) share
the original checked base/install cell and serial owner, with one CanonicalCache
inside the provisioned authority context. Retained plans keep their immutable
root/client; known install publishes checked metadata using the original binding
client, without promoting a failed operation provider. Cross-context sharing and
dynamic revocation are unqualified. Actual cache/hash/copy/network work remains.

[Content](../../architecture/03-files.md) decodes and checks a missed page/summary
before eviction/insertion, invokes the existing allowance, and moves checked
canonical bytes instead of cloning. Default64-page retention is at most524288bytes,
excluding map/candidate/decoded/provider/deferred/OS state. Invalid pages leave the
old cache intact; hits still check requested summary. Draft/reference/detached/
resolved/emitted growth and sparse byte work remain K1; the
[K0/K1 design](K0-K1-BACKED-EDIT-DESIGN-20261007.md) is still a proposal.

[Native construction](../../architecture/23-native-bridge.md) attempts
`set_nodelay(true)` after successful KK authentication, before direction cloning/
record allocation. Original I/O failure propagates; no retry/reconnect/timeout/
record-format change. The syscall is separate from ChannelWork wire counts.
The public regression reads both actual options and verifies duplex accounting.

## Actual host and independent Linux proof

The host uses actual Handles, Project Init, Storage, History, Runtime, Sessions and
Supervisor, captures persistence before consuming Handles and native metadata
before import, then deletes source before binding/access. Production Linux Upstream
uses saved-object calls through authenticated framing and one initialized Overlay
Owner. There is no scripted host or legacy adapter in this proof.

The independent oracle covers all13 original paths, exact directory sets and all7
regular-file names with complete bytes/EOF. Two names are hardlinks:6 distinct
regular-file inodes. It includes `.git/index`, ignored state, dependency/cache/
output, portable permissions/mtime, hardlink serial/reference agreement and raw
symlink target. Normative symlink mode0777 is applied before import; raw macOS0755
and exact native timestamps are retained separately. The repository mount contains
proof artifacts, so source removal is tied to the inspected public path, not a
container-isolation claim.

The first missing-object reply/envelope/bytes remain retained. Terminal second read
makes no exchange, verified using the next Calls correlation; fresh operations
succeed. Ordinary create/write/append/truncate release exact publication reply
owners and verify bytes/signed portable timestamp.

Read-only observers see empty live maintenance before one known Close and logical
`CleanupState::Gone` from automatic terminal reclamation. No physical deletion,
allocation/shrink, reclamation work/latency or global GC is measured. Consumer fence
verifies no partial replies and zero consumer receive live-message/byte credit.
Host workers join and Sessions fence succeeds with empty Save slots; AttachmentFence
fields are not inspected individually. Zero host-domain credit/graceful host native
close is unqualified.

[Receipt18](checks/r4-upstream-20261007/18-real-host-docker-proof.json) passes both
profiles with131 host deliveries each and79.05s combined wrapper time. After three
external fixture lint corrections, [final affected proof32](checks/r4-upstream-20261007/32-real-host-docker-final-proof.json)
passes both profiles with131 deliveries each and78.94s combined wrapper time. Workload and40s inner/45s host/120s outer
limits are unchanged. These are functional observations, with no latency/resource
sample, packet attribution, distribution or qualified speed claim.

## Checks and preserved failures

[Append-only receipts](checks/r4-upstream-20261007/) retain commands, stdout/stderr,
wall stops and [original diagnoses](checks/r4-upstream-20261007/FAILURES.md).
[Final formatted source](checks/r4-upstream-20261007/final-formatted-source.json) and
[actual Linux binary](checks/r4-upstream-20261007/linux-consumer-final-binary.json)
pin functional inputs. Cache state is uncontrolled; no sealed measurement arm.

| Scope | Actual coverage |
| --- | --- |
| Host Workspace/Daemon | All-target build and52 distinct bodies PASS, including4 scope/3 upstream bodies. Device-full fixture unrun; real-host proof separate |
| Host Bridge/SDK | Final all-target build and68 bodies PASS, including actual socket options; macOS Store bodies execute on host |
| Content | All-target build and23 affected cache/localized/no-op/reference bodies PASS on host and Linux |
| Linux ARM64 | Pinned-image all-target build and79 distinct portable Bridge/SDK/Workspace/Daemon bodies PASS. macOS Store targets run0, unclaimed; affected upstream3 PASS after lint correction |
| Static | Host/Linux warning-denying all-target Clippy,685-file product guard and41 tooling self-tests PASS; format/link/whitespace receipts separate |

Every test has explicit60/120s stops; Docker adds110s or proof40s stops with1s grace.
No timeout occurred. Original import/authoring/nonblocking-socket/symlink-oracle and
three fixture Clippy failures remain. Initial fixture cleanup lost consumer logs;
that gap is explicit, and every later proof retains artifacts. NODELAY selection
is source-led, without measured causality. Unused fuser patch warning is expected
while FUSE is excluded. Offline lock updates add existing first-party edges only;
all third-party versions/sources/checksums are unchanged.

## Exact production accounting and continuation

Production LOC: 162382 -> 162906 (delta +524).
Core96965 ->97489 (+524); reference65417 ->65417 (+0).
This is additive composition/cache correction, with no relocation, duplicate
product or reference retirement. [Exact final comparison](checks/r4-upstream-20261007/final-production-loc-source.json)
uses first parent86f766750029ab7f4224b587e0f20a7779813e89 and staged `core/crates`
`821aafb81824801e4f0744d24168744a0ab10533`: `git archive TREE crates core/crates`,
then unchanged `tools/production_loc.py --root ARCHIVE --json`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Runtime SQL/excluded predecessor product count; tests/inline tests/examples/docs/
tools/harness/third-party do not. Confirm final staged root tree against commit.

Complete topology/provenance, crash/disconnect/unknown recovery, huge-root/dense>
4GiB and E/Q service/resource/numerical gates remain open. All27 E1 samples stay
NOT_RUN/qualification NOT_EVALUATED. All8 incumbent Init speed/strict allocation
FAILs and6 approved history pairs retain original identities/routes, without
resampling/waiver. Next ready work is lazy Plan, indexed Content backing in existing
Overlay, sparse/captured normalization and native F0/F1. Pinned fuser INTERRUPT is
an exact public-interface gap; timestamp-only exception is not extended.
S0/S7–S13 stay unchecked and the active full goal continues.

## Post-commit confirmation

[Verification](checks/r4-upstream-20261007/38-committed-verification.json) confirms
root tree `e86ff6f6d91a2707fe59bdf4d7187a9514678ed3`, exact staged source and
recomputed committed counts. Separate [S7 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6025181756)
and [S9 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6025188711)
record the boundary; [comment-body receipts](checks/r4-upstream-20261007/39-tracker-receipts.json)
confirm the issue checklist is unchanged. These later receipts will be retained
by the next checkpoint without altering this committed evidence identity.
