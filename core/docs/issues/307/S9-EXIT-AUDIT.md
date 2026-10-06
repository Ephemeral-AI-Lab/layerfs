# S9 runtime and complete-root audit

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Milestone state: CHECKPOINT, incomplete. S9 remains unchecked.

The new [bound history handler](../../architecture/37-runtime-history-receipts.md)
is actual product code under SDK runtime/handlers, over the initialized real
macOS Store/catalog. It adds checked root serial binding, stage-after-SaveFinish,
exact-token conditional transition/discard and bounded retained receipts. It
preserves authenticated peer/Workspace/Branch/catalog/runtime authority and the
captured expectations. Unknowns retain custody; no Branch refresh, resend,
automatic conflict discard or guessed install is added.

| S9 requirement | Current implementation/evidence | Remaining |
| --- | --- | --- |
| Authenticated policy/object/serial/Save boundary | Existing native KK VerifiedPeer binding, owning semantic object admission, local policy/length/serial windows and interleaved Saves | Typed bounded fair service now exists; logical framing/codecs and actual authenticated transport delivery remain |
| History adapters | stage_saved/commit_saved/discard_saved plus history_receipts; exact outcome/token/error retention | Typed fair queued history/retained receipt access now exists; logical transport and complete integrated history reads remain |
| Save/topology lifetime | Stack-borrowed initialized Storage owners, same-Save reads, no whole-Save provider checkout; finished Save required for stage | Local attachment/cancellation/result fences now exist; transport/restart and aggregate flow/capacity proofs remain |
| Semantic/context authority | Root grammar/profile/scope and supplied authority checked; Store reference closure retained | Full contextual topology/provenance qualification and untrusted transport admission |
| Unknown history | Typed unknown retains slot, commit/discard cannot replay or release it | P10 completion-fenced resolver protocol remains explicit; no unknown fault proof added |
| Complete initial roots | Content/base proofs include aliases/symlinks/full-state shape; new native import proofs cover paths/link targets | Native Init now preserves symlink targets and all ignored/dependency/cache/output/.git paths, and removes4GiB refusal; backed scan/job/namespace collections and hard-link identity remain incomplete |
| Sandbox/API-core/client assembly | Current package boundaries preserved; no excluded package activated from a scaffold | Actual owning Sandbox/API-core and SDK client integration still required |

Nine real-host runtime tests pass, including three new history bodies: UpToDate,
wrong-role/authority/cross-capability admission and exact explicit discard, and
same-Branch Committed winner versus deciding-transaction HeadMoved with retained
original stage/token. Both candidates are saved through real initialized owners;
loser release refuses until its exact known discard. There is no automatic
re-stage, operation retry or Branch refresh. Binding's root serial is authenticated
at bind. Test fixture native accept/read/write waits are bounded and panic-safe.
These small fixtures qualify handler behavior, not full-root readiness or a
load-bearing runtime.

[Covering host output](checks/s9-history/layerfs-sdk-covering.log), [no-run builds](checks/s9-history/)
and [failures](checks/s9-history/FAILURES.md) remain append-only. Rust1.85.1 locked,
repository ARM64 profile, explicit120-second test ceiling and one construction
worker apply. Linux compile succeeds but the owning provider tests are macOS cfg;
no Linux global provider/history execution is claimed. Four-package Clippy/fmt,
boundary568 and26 tool tests pass at the relevant product source. [Identity](checks/s9-history/identity.json)
records exact source and binary inputs; uncontrolled caches have no speed/RSS eligibility.

Production LOC uses unchanged tools/production_loc.py and exact first-parent/staged
archives; the commit/handoff record core/reference/combined totals and signed delta.
Reference65417 and excluded predecessors remain counted and preserved.

Next independent work is authenticated logical demand/Save/history framing and
fair delivery with exact disconnect/restart fences, then faithful backed initial
acquisition in Project/import and owning Sandbox/API-core integration. S7 complete
resource/cost acceptance and S8 native integration also remain required. S10
incremental Commit and P3/P6/P7/P13/P14 are outside this batch and explicitly open.

## Fair service and native-import checkpoint (2026-10-06)

The real [typed SDK service](../../architecture/38-authenticated-runtime-service.md)
borrows the initialized authenticated registry, with fixed connection/job windows,
Workspace/class rotation, same-Save ordering and demand/control byte reserves.
Caller results retain credit and receipt ownership. Disconnect cancels only queued
unattempted bodies; completed original outcomes/Saves survive. Another connection's
Save head progresses after cancellation. Exact original-binding reconnect does not
refresh a Branch or repeat Finish/history. Fresh service epochs and retained-result
leases prevent stale capability reuse. These are local synchronous dispatch fences;
logical wire transport/client, actual socket fences and process-restart custody remain.

New real-host bodies cover fairness despite multiple connections, same-Save pending
reads before Finish, retained-result release refusal, disconnect/cancellation/successor
promotion, completed Finish retention, queued authority revocation and service-owner
rewrap fencing. A queued history body also covers Stage/Commit order, exact original
token/outcome across reconnect and duplicate transition refusal. Prior nine handler
proofs retain their identity and outcomes; no unknown-history resolver is invented.

Project [native import](../../architecture/39-native-import-links.md) now includes
symbolic links without following their targets, preserving broken/external/cyclic/
opaque-byte targets with existing canonical0777 link mode and exact mtime. It removes
the inherited4GiB scan/role check while preserving canonical/platform limits. A saved
root test covers ignored bytes, dependencies, caches, outputs, empty directories and
.git/index, then deletes the native source before canonical-only readback. Original
portable-metadata failure from macOS's umask-dependent link bits is retained with its
source diagnosis; canonical grammar is unchanged. Greater-than4GiB native streaming
proof remains NOT_RUN. Scan/job/frontier/child/namespace collections and native regular
hard-link identity still need P12 correction, so this is not full-root bounded acceptance.

[Append-only receipts](checks/s7-startup-s9-service/) retain all compiler/lint/test
failures and corrections, locked builds, explicit120s tests, source fingerprints,
ARM64 flags and one construction producer. Host provider tests are macOS; Docker
verifies portable engine/import paths rather than claiming Linux global provider
support. S9 stays unchecked. P3/P6/P7/P13/P14 remain later Commit prerequisites.

Final production-source covering host checks run once with39 selected no-run
binaries hashed before/after:48 Overlay,12 Daemon,32 Workspace,15 SDK and66
Project (9 integration/helper plus57 example/harness checks) pass. The earlier
14-body SDK checkpoint remains diagnostic rather than being relabeled as the
new15-body result. Exact production hashes remain unchanged throughout; minor
private macOS error cfg and Copy fixture repairs are included in final lint/build.

Docker's final3 native acquisition/scaling checks also pass with3 binaries
unchanged across execution; global-provider tests remain macOS-only. The six
changed/dependent packages pass locked Linux all-target no-run build and warning-
denying Clippy. No owning-platform/global-provider substitution was introduced.

Follow-up service accounting includes every submission attempt/refusal by class,
including authority/stale/slot failure before credit acquisition. A queued authority
refusal remains a dispatched original adapter outcome rather than being relabeled
as unattempted admission. This adds no provider call, retry or flow cap.

## Authenticated logical delivery checkpoint (2026-10-06)

[Architecture40](../../architecture/40-runtime-wire-ownership.md) describes real
versioned header/fragment and operation/reply/error codecs, opaque remote Save
identity, independent native client directions and bounded input/output workers.
The host authorizes fixed facts before body allocation, and the client waits for
its exact header grant. Existing Sessions and fair Service retain provider/Save
ownership; no whole Save lock is transferred to socket threads. Shared count/byte
credits include partial and caller-held input/results. First demand/control service
slots survive ordinary Save pressure. Known terminal Accept/Finish/Abort headers
refuse before body allocation. Original binding inspection never refreshes Branch.

Native real Store delivery proves binding/policy/serial/Begin/Accept/same-Save demand/
Finish/length/Stage/UpToDate/Release. Exact deciding conflict stage/context and
original/cleanup/persistence causes survive typed wire encoding. Receipt access
refusal is distinct from the original Finish/history outcome. Original partial
input and stopped output return after explicit worker fences; no replay, guessed
cleanup, implicit Bash timeout/Commit/unmount or unknown-history resolver is added.

Host37 and Docker19 functional bodies cover the final custody source; seven selected
executables per platform are pinned before/after. Existing earlier checks retain
scope, not performance eligibility. Complete-root native scan/hard-link/backing,
full contextual authority/topology, consumer/application assembly and process-
restart custody are still unfinished. S9 remains CHECKPOINT/unchecked; S10–S13 and
P3/P6/P7/P13/P14 remain later Commit prerequisites. The accepted fuser correction
and Docker verification are unchanged and do not require QEMU or a release wait.

## Native regular identity checkpoint (2026-10-06)

[Architecture41](../../architecture/41-native-regular-aliases.md) adds grouping of
the existing native job vector by device/inode, borrowed unique-file construction,
pre/post descriptor/path mode/ctime checks and shared logical inode bindings.
Separate equal-byte files remain separate; links outside the root do not count
inside it. Path-count inode reservations are consumed without recycling alias gaps.
No custom alias map or per-group resident vector is introduced. Memory and both
real host profiles qualify .git/index plus ignored/cache alias, outside-root link
and equal-byte copy, followed by deletion of the native source before readback.

Host66 functional bodies and Docker3 selected acquisition/scaling bodies pass.
Host11 and Linux11 compiled executables have unchanged pre/post hashes;3 Linux
executables were actually invoked. Both-platform Clippy, fmt and603-file boundary
checks cover source.39 unchanged tool tests are reused at their original identity.
All tests have explicit<=120s stops; Docker inner110s+1s fence. Original E0609 test
field-name failure and diagnosis remain in checks/s9-native-aliases. No timeout or
cold speed/RSS/rate proof. The original O(N) scan/input collections remain visible
and require backing; new grouping isO(N log N)+actual unique payload bytes, not
bounded complete-root acceptance. Greater-than4GiB native proof remains NOT_RUN.
S9 remains unchecked; remaining application/consumer/restart/context and backing
work is active, with P3/P6/P7/P13/P14 still S10 prerequisites.

## Native consumer ports checkpoint (2026-10-06)

[Architecture42](../../architecture/42-native-consumer-ports.md) adds SDK client
`Calls`, one bounded exchange owner over an existing authenticated connection, and
`RemoteObjects`/`RemoteLengths`/`RemoteSerials` implementing the public
AuthenticatedObjects, FileLengths and InodeSerials ports. Each exchange checks its
original grant/reply correlation, class, operation, object/Save identity, demand
cardinality/order, serial count and attempted receipt phase. Transport/protocol
failure is retained and terminal for that owner; no reconnect, refresh or replay
is added. Independent CloseHandle stays outside the call mutex; partial drain is
nonblocking and never claims an in-progress call joined. Only typed owning
MissingObject/ObjectMissing becomes content absence. Canonical hash failure is
`FrameError::IdentityMismatch`. The mutex spans one adapter unit, not a Save/Commit.

Host40 functional bodies (Bridge14, SDK runtime19, wire7) and Docker21 portable
bodies (Bridge14, wire7) pass; owning global-provider cases are macOS cfg and are
not claimed on Linux. Seven compiled executables per platform have unchanged
pre/post hashes. Both-platform all-target Clippy with warnings denied, fmt and the
606-file boundary guard cover the source. Every receipt carries the same888-input
source map; its cohort digest is recorded in [identity](checks/s9-consumer-ports/identity.json).
All tests have explicit<=120s stops; Docker inner110s+1s fence. No timeout occurred.
The original E0433 fixture import failure and the command-order mistake that
launched a functional command after a failed no-run (zero bodies executed) remain
in [failures](checks/s9-consumer-ports/FAILURES.md). Unchanged tool self-tests are
reused at their original identity. Caches are uncontrolled: no cold speed, RSS or
sustained-rate claim.

Remaining S9 exits are unchanged in kind: backed initial acquisition (scan/job/
frontier/child/inode/directory collections stay resident), owning application/
daemon runtime assembly and supervision of these ports, contextual authority/
topology/provenance, exact disconnect/process-restart custody including the P10
completion-fenced unknown-history resolver obligation, and public-API/real-Store
qualification of interleaved Saves, fairness and repeated bound use. Greater-than
4GiB native proof remains NOT_RUN. S9 remains CHECKPOINT/unchecked; S8 is a later
batch; S10–S13 and P3/P6/P7/P13/P14 remain later Commit prerequisites.

## Backed initial acquisition checkpoint (2026-10-06)

[Architecture43](../../architecture/43-backed-initial-acquisition.md) moves every
input-sized Project/import collection (scan entries, directory frontier, wide
directory ordering, native identities, jobs, aliases, per-entry roots, inode rows
and directory bindings) into operation-owned scratch and streams it into the public
`build_directory`/`empty_directory`/`build_table` constructors. Initial acquisition
no longer calls the resident whole-namespace `build_filesystem`. The selected
backing is Content's existing public `OrderingBacking`/`FileBacking` ordering runs:
append-only, written once then read, byte-charged, unsynchronized and released with
a checked result before the tree Save finishes. A first SQLite-in-Project attempt
was refused by the unchanged product boundary guard and withdrawn; no engine
dependency, new dependency, public backing port or format change was introduced.
Public `init`/`InitRequest`/`Initialized` are unchanged; `NamespaceWork` adds
`sort_capacity_bytes` and `backing_bytes`. Order, serial reservation, alias binding,
descriptor/path stability checks and one-attempt failure are preserved.

Host: 12 Project bodies pass, including a public test requiring the published root
to equal the `build_filesystem` root for a wide, nested, aliased tree, a 17 000-child
directory ordered through merged runs, and a refused source that leaves no scratch
or history. All-target Clippy with warnings denied, fmt, the 609-file boundary guard
and 39 guard self-tests pass. Docker linux/arm64: build and Clippy pass; nine bodies
pass; the unmodified, ungated `init_sqlite` test FAILED with `BackendUnavailable`
from the macOS-only global Store before reaching Init, and the scaling body it
blocked passed in one separate run. That failure and the withdrawn SQLite receipts
are retained in [failures](checks/s9-backed-acquisition/FAILURES.md); the 894-input
source cohort is in [identity](checks/s9-backed-acquisition/identity.json). Every
test had an explicit <=120 s stop (Docker inner 110 s + 1 s); none was reached.
Caches are uncontrolled. Init time and storage effect are NOT_RUN: no speed, RSS,
page-cache or cold claim. Resident sorter state is fixed chunks and buffers plus run
handles growing with the logarithm of the stream; Save-internal state is not bounded
by this change.

This closes the resident-collection defect of initial acquisition, not S9.
Remaining S9 exits: owning application/daemon runtime assembly and supervision of
the consumer ports, contextual authority/topology/provenance, exact disconnect/
process-restart custody including the P10 completion-fenced unknown-history
resolver obligation, and public-API/real-Store qualification of interleaved Saves,
fairness and repeated bound use. Greater-than-4GiB native proof remains NOT_RUN.
S9 remains CHECKPOINT/unchecked; S8 is a later batch; S10–S13 and
P3/P6/P7/P13/P14 remain later Commit prerequisites.

## Review and SQLite correction plan (2026-10-06)

[The remaining implementation plan](IMPLEMENTATION-PLAN-S7-S9-20261006.md)
records committed consumer `90d7a2c5b` and run-backed acquisition `97a02fffc` as
checkpoints. It preserves their functional outcomes and limits. The acquisition
checkpoint's Docker all-target command remains FAILED; the later scaling pass
does not replace it. Acquisition binary pre/post seals were not recorded.

Planning/tooling source `5900de7331202e8ead57b4c6c7c77941e8c56bbb`, tree `4a11ab6a242f27218a095bf5d2aeffe1d202df58`, clarifies that direct
Project→rusqlite violates placement while provider-owned SQLite backing through
a domain port is permitted. The dependency graph is unchanged. The first next
implementation is the capability/Store-open/schema-compatibility design, followed
by the actual indexed provider and Project ordinary path. There is no implemented
new port, restored SQLite backend or permission to use the rejected per-Init
prototype unchanged. Keep streamed canonical construction and identity proofs.

Two public regressions failed once each after an isolated Cargo no-run: an empty
source with its scratch parent inside the source fails ENOENT; a failed Content
FileBacking append leaves4096 disk bytes while charging0 held bytes. Both and the
initial manual-link harness compile failure are preserved in [review receipts](checks/s7-s9-review-20261006/review.json).
The probe uses its own lock/test profile and supplies source-level diagnostics,
not sealed product qualification. Original cleanup cause/residual custody also
requires correction. No product Rust source was changed in this planning step.

Runtime supervisor/attachment, contextual authority, exact restart/disconnect
custody, explicit P10 scope and real Store/huge-root/>4GiB/resource qualification
remain open. S9 is incomplete/unchecked; S8 and later Commit prerequisites stay
deferred. [S9 tracker receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6010923803) now records both previously unposted
checkpoints, their exact production LOC and these corrections. The plan/guard
change itself has core92680/reference65417/combined158097 unchanged, delta0.

## Acquisition custody corrections and A1 design (2026-10-06)

Source `8bd03d76243987c365a06453d83c45015f72d4a5`, tree
`c0f5436141425f1a7873aff843dfc6e8e7a7b0e3`, first parent `ee4a64722`, delivers plan
package A4. Production LOC core 92680 → 92797 (+117: Content +82, Project +35),
reference 65417 → 65417 (+0), combined 158097 → 158214 (+117);
[committed LOC receipt](checks/s9-acquisition-custody/committed-loc.json).

- A failed partial `FileBacking` append keeps the bytes that reached the file
  charged; a path returns only its own listed bytes; a failed checked release
  records its host cause and is not replayed by a destructor.
- A scratch parent that is the source or inside it is refused as
  `ScratchInsideSource` before anything is created. The review probe expected
  success with the scratch excluded; refusal was chosen because creating the
  scratch there changes the source directory.
- `ProjectError::Cleanup` carries the deciding host error and the retained
  directory, run count and bytes.
- The macOS-only Store oracle is platform-gated; other platforms prove the
  `BackendUnavailable` refusal. The Linux Project all-target command, FAILED at
  the previous checkpoint, passes at this source. The earlier failure stays FAILED.

Host: Content 289 and Project bodies pass; whole-workspace build and Clippy pass.
Docker Linux: Content 289 bodies and the Project all-target command pass, Clippy
passes. One new-test defect failed once and is retained in the
[failure ledger](checks/s9-acquisition-custody/FAILURES.md) with the evidence
limits: the failed-release Project case is an induced race and prints a skip
under uid 0 in Docker; binaries carry a post-run seal only. Caches uncontrolled;
no speed, RSS or cold claim. [Identity](checks/s9-acquisition-custody/identity.json).

[A1](A1-ACQUISITION-CONTRACT.md) is the written acquisition capability, table,
compatibility and cleanup design. It is a proposal: no port, table or provider
exists. Two Store-format decisions in its section 9 are the owner's and gate A2.
A2, A3, R1–R4, E1–E4 and Q1 are not started. S9 remains CHECKPOINT/unchecked.
