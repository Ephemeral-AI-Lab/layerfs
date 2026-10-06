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
