# Authenticated Workspace Commit and writable daemon entry

> **Status: implemented and verified in the declared functional scope; target v0.1.7, not released.**
> Implementation parent: `74e6fbd2d23cb7519ea63c275f42283ce3c79d17`.
> Implementation commit: `8e01d28a1c8b7708f5d319990c1440ae682f8b44`; [exact staged-tree confirmation](evidence/control-commit/commit-confirmed.json).

This round exposes ordinary native Workspace Commit. The identity-only request
uses profile 3/opcode 14 and independent daemon grant bit 32; valid masks are
0..63. It accepts the existing managed-name/nonzero-incarnation bounds, zero
Store/generation/result-data fields and complete empty authenticated input.
The wire maximum is the existing native MAX_OPERATION_MS=600000; it is a protocol
ceiling, not a measurement or test timeout. Declared functional Commit requests
use 10000 ms and each complete selection keeps its existing 60-second budget.
The daemon reserves the existing 100 ms terminal headroom from the original
absolute request deadline. Service refuses this daemon control before Store
admission. There is no caller-supplied filesystem candidate, auto Commit or replay.

The required startup path is `--mount-writable ID INCARNATION STORE branch:ID UID GID`.
It selects the existing LocalEdit Workspace capability and requires a positive
explicit LAYERFS_WORKSPACE_DISK_BUDGET_BYTES, the existing explicit max count,
and at least one currently unexpired configured Commit grant. The read-only and
headless grammars remain available. A grant may expire later under the existing
authorization policy; startup neither creates nor extends authority. Both initial
mount and authenticated remount select mount_writable for LocalEdit and the existing
read-only constructor for ReadOnly. Kernel writes keep the existing direct-I/O,
existing-file, append and size-SETATTR qualifications; this does not add namespace
creation, writable mappings, writeback or synchronization promises.

One native Workspace remains the semantic owner. Daemon control invokes its existing
`commit(deadline)` through the shared lifecycle slot; Workspace still owns capture G,
live successor G+1, preparation, one composite C5 Commit and local reconciliation.
The lifecycle mutex excludes another control operation/owner replacement but does
not serialize ordinary FUSE callbacks. Authentication, exact identity, complete
input, expiry, stopping and original deadline are checked before entry. Q=0 and the
existing single control session remain unchanged.

## Exact results and observations

Response tag 17 carries WorkspaceCommitWire, maximum 1590 bytes. Completed mirrors
the native CommitReport: generation, optional stage token, exact CommitOutcomeWire
and installed revision. Failed preserves generation, phase, native disposition,
shared Failure (including unknown/cleanup/HistoryFailure), known versus observed
stage records, known versus observed C5 outcomes and optional installed revision.
KnownBeforeCommit, Unknown and KnownCommitLocalFailure are distinct. A cause's
unknown flag is not equated with the disposition: a malformed observed response can
be Unknown even when its cause is definite InvalidInput. Stage records are
observations only; they are not remotely usable StageSelector capabilities.

Native Stage failures after capture but before CommitAttempt reservation are
represented as failed preparation, preserving the captured generation and cause.
A bare native error can result when failure bookkeeping itself is unavailable.
Before/after native generation/submission observations distinguish an unchanged
admission refusal from changed or unobservable custody; the latter ordinary
failure is marked unknown rather than inventing a typed phase or successful abort.
A conversion validation failure after native entry also remains unknown. No loss
or failed observation authorizes replay, guessed rollback or a regenerated receipt.

Writable Status uses separate tag 18, maximum 325 bytes. It carries one native
healthy observation plus generation, revision, dirty-inode count and the complete
bounded SubmissionStatus/CommitStatus fields. The existing read-only healthy and
failed-attachment Status forms remain unchanged. Only matching Status accepts
tag 18 and only matching Commit accepts tag 17, with exact Workspace/incarnation
and zero result data. Status is a current observation, never a replacement for a
missing Commit terminal or authority to adopt a known/observed record.

Existing control field codecs move into their own native protocol file; Commit/
writable observation codec has its own focused file. C5 stage/Commit validation,
Commit outcome encoding and contextual Failure encoding are shared with the
existing history protocol, whose profile-dependent wrappers remain unchanged.
No new dependency, synthetic request profile or parallel socket protocol is added.

## Startup and signal custody

The daemon establishes its control owner before exposing writable callbacks.
A fresh lifecycle slot is held through control startup and initial mount. Early
authenticated control requests can refuse Busy; none can replace the initial owner
while mounting. A mount failure installs any returned owner before releasing the
slot. Failed-startup cleanup retains its available control endpoint on dirty or
incomplete cleanup, uses the original deadline, and returns the original startup
error after eventual explicit cleanup. Local startup Workspace clones are dropped
before long-lived failure loops so later replacements do not keep stale charges.

Signal cleanup first tries the lifecycle slot, checked unmount and native clean
closure while control stays available. Dirty, retained, active or otherwise
refused cleanup leaves the endpoint available for explicit Commit, Status or
further cleanup. An active Commit holds the slot, so a signal observes Busy and
does not interrupt that operation. Successful closure ends control admission while
still holding the slot, then releases it and joins control. A replacement Attach
cannot enter between clean closure and stop-admission. Both read-only and writable
profiles use this order. There is no implicit Commit, forced product cleanup or
internal repeated attempt; another signal is a separate explicit cleanup attempt.

## Declared functional verification

Use the existing closed 64 MiB canonical content Store as independent byte copies;
create a fresh history catalog and live C5 producer through the public bootstrap
for each selection. Reopened read-only history cannot supply write continuity.
Do not regenerate the large canonical input. The owner-selected preinstalled DSH
fixture remains separate and is not replaced by this smaller primitive proof.

Seven selections are registered: repeated mounted writes/explicit Commits;
dirty signal followed by still-callable Commit; one lost control terminal without
replay; live G+1 during an externally observed actual C2 save; independent Commit
authority; read-only/profile/input/deadline refusals; and typed known denial from
an actual service grant. That last case retains its failed submission and records
any external forced test teardown separately from normal native cleanup.
The lost control terminal proves outer delivery uncertainty, not retained native
upstream Commit uncertainty. Existing affected read-only signal/lifecycle routes
are selected separately after source freeze. Builds and functional runs remain
serial, Cargo jobs 2, Docker quota 2 CPUs and construction workers 1.

This round does not add remote Stage/CommitStaged/Discard or known-own reconciliation
controls, full R4 qualification, namespace/new-inode/large-input support, full DSH
upload, npm execution, matched R6, a durability promise or issue closure. Required
later operations and every unsuccessful or unrun selection remain explicit.


## Frozen source, checks and resource observations

Product input seal:
`25774a7949ce2628e98a928589d3061a9fe485ce86fd300f615e156787a2c5bb`.
Locked Rust 1.85.1 whole-core tests passed host **672/0 failed/3 ignored** and Linux
**670/0 failed/126 ignored**. Whole-core host/Linux bins/examples and warning-denying
all-target Clippy passed, together with fmt, the 247-file boundary guard and all
six guard self-tests. Independent review then strengthened the configuration
negative caller's otherwise-valid connection environment; the changed configuration
suite passed all three tests on both hosts. Product bytes were unchanged. The
initial all-target host check is preliminary, before the final startup-clone drop.

Review closed three product hazards before source freeze: a bare post-capture error
could look like definite refusal; writable startup could have no currently usable
Commit grant; and writable callbacks could become visible before control startup.
The reviewed correction preserves custody and uncertainty in each route. External
caller review also closed missing-environment false positives and independent
cleanup failure handling before the actual selections.

Production LOC: **109490 → 110285 (delta +795)**; core 44073 → 44868 (+795),
reference 65417 unchanged. Bridge adds 533, daemon 260 and Service 2 production
lines. This includes relocation of existing control codecs/C5 validators, separately
identified from the new operation; there is no legacy retirement or dependency
addition. [31](31-source-map-and-loc.md) and the
[full inventory](evidence/control-commit/source-loc.json) use the unchanged production
counter and record every production file and recursive folder. Core contains 247
production files / 54058 physical lines; entry/source ceilings continue to pass.

Locked Linux AArch64 DWARF records fixed sizes: Response 112 bytes (unchanged),
shared lifecycle Arc 312 bytes (unchanged), WorkspaceCommitWire 240 bytes,
WorkspaceCommitFailureWire 1032 bytes and WorkspaceWritableStatusWire 312 bytes.
These describe the named payload/object layouts. Owned name/identity vectors and
optional contextual-failure boxes are additional allocations; allocator overhead,
whole-process heap, RSS and cgroup peaks were not measured. Worker, queue and FD
capacities do not increase. [Layout evidence](evidence/control-commit/checks/layout.json).

## Actual selections and qualification

Thirteen registered selections passed their functional assertions. **Twelve** have
normal native cleanup; the known service-denial negative selection has
**EXTERNAL_ONLY** teardown and does not qualify native clean closure or reusable
cleanup-PASS evidence. No registered selection remains NOT_RUN. The original
live-successor failure is retained, making fourteen total attempts. No passing
selection was repeated. Every attempt keeps the same 60-second complete-command
budget; actual runtime quota, frozen product/binary/helper identities and external
wall are recorded. These are functional proof timings, not performance samples.

| Selection/attempt | Assertion | Driver seconds | Complete external seconds | Cleanup |
| --- | --- | ---: | ---: | --- |
| commit_repeated-01 | PASS | 16.217928042 | 16.367851500 | PASS |
| commit_dirty_signal-01 | PASS | 1.744727250 | 1.855664167 | PASS |
| commit_control_loss-01 | PASS | 2.086897000 | 2.200573834 | PASS |
| commit_live_successor-01 | FAIL | 4.935268709 | 5.057519833 | FAIL / forced |
| commit_live_successor-02 | PASS | 5.553135334 | 5.691651084 | PASS |
| commit_authority-01 | PASS | 10.810749542 | 10.944387625 | PASS |
| commit_refusals-01 | PASS | 1.828453375 | 1.986606375 | PASS |
| commit_denied-01 | PASS | 1.249967792 | 1.388640875 | EXTERNAL_ONLY |
| held_init-01 | PASS | 11.368204834 | 11.486666459 | PASS |
| signal-01 | PASS | 6.302546875 | 6.429963459 | PASS |
| shutdown_retained-01 | PASS | 11.561328292 | 11.694744459 | PASS |
| attach_signal-01 | PASS | 1.936510042 | 2.027879875 | PASS |
| mount_session_failure-01 | PASS | 1.949983083 | 2.087430500 | PASS |
| attach_startup_failure-01 | PASS | 11.174222083 | 11.308157958 | PASS |

The repeated A→B→A sequence installed revisions 4, 7 and 10, with independent C5
head/root/parent checks, complete declared 8192-byte content checks and matching
A content roots. These root facts do not imply identical Commit identities.

The first live-successor attempt observed and reconfirmed the actual C2 RESERVED
lock, and SIGTERM correctly returned shutdown Busy. A subsequent fresh pathname/
FD open returned EBUSY before any LIVE write. That failure and its forced cleanup
remain source-pinned under input01. The corrected input02 opens data/alias
file descriptors before Commit and performs only pwrite/pread/fstat on those held
descriptors during the stopped-service interval. Its input stays 4 MiB and its
request/whole-command budgets remain unchanged. Captured G1 installed revision 37
while G2 kept one dirty inode; the next explicit Commit installed revision 39.
Content verification covers the declared 128 KiB prefix plus exact C5 outcomes and
alias identity; it does not claim exhaustive 4 MiB readback.

This proves the declared S-11 established-handle successor write/read subset.
The [necessary-remote-read allowance](02-overlay-snapshot.md)
and [bounded admission rule](04-implementation-and-verification.md) distinguish
local progress from remote misses. Current lookup always enters remote admission,
so fresh pathname opening during a save remains a compatibility limitation; a
proven cached binding could avoid some such requests in a later shared-owner
improvement. This proof does not qualify general pathname-open/readdir/base-read
progress or complete R4, and does not relabel the original attempt as PASS.

The known service-denial case returns typed KnownBeforeCommit with retained native
submission. CloseClean refuses that state. Its external container teardown is an
explicit negative-test disposition, not product recovery, discard, cleanup success
or permission to drop unknown state in an application.

All executed caller variants and imported repository helpers were snapshotted
before their first sample; no post-run source recovery was needed. Twenty unique
artifacts include the external observer binary, which remains in ignored owned
storage; nineteen source/build-receipt artifacts have lossless compressed copies
in Git. The 64 MiB canonical master is independently byte-copied without regeneration;
fresh live C5 bootstrap is explicit setup in each selection. The original fixture
receipt and master remain unchanged. There is no restarted writable-history claim.

[Complete attempt index](evidence/control-commit/functional-index.json),
[initial inputs](evidence/control-commit/inputs-01.json),
[held-FD caller inputs](evidence/control-commit/inputs-02.json),
[all core check commands](evidence/control-commit/checks/commands-01.json),
[review record](evidence/control-commit/source-review.json),
[fixture reuse](evidence/control-commit/fixture-reuse-01.json),
[caller archive](evidence/control-commit/caller-snapshot-archive.json), and
[raw-log index](evidence/control-commit/raw-log-manifest.json) preserve exact commands,
source variants, cleanup classification and the failed attempt. Original raw logs,
working databases and immutable executables remain under this worktree's ignored
`core/target/pair1-evidence/control-commit` and `binary-archive` directories.

Next prerequisites concern namespace/new-inode preparation and the maintained
Workspace namespace index before actual mkdir/create/upload. Remote failed-submission
disposition, native upstream Unknown, remaining R4 schedules, the complete prepared
DSH directory upload and matched R6 remain open. No Pair1 completion or issue closure.
