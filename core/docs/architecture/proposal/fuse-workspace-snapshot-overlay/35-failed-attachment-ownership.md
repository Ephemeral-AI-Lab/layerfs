# Failed native Attach ownership and explicit cleanup

> **Status: implemented and verified native prerequisite to remote Attach; not released.**
> Implementation parent: `0d220870175abb6e3f162cb164dba7c49bc98f9d`.

The existing Attach failure path could retain an unobservable registry entry with
state=None and lose its local mount-leaf ownership flag. It also allocated a
metadata arena before a fallible BranchContext allocation; that later failure
could leave the arena registered after the entry was removed. Remote Attach must
not expose those ambiguous outcomes.

WorkspaceHost keeps the same registry. Its entry now distinguishes Attaching,
Attached(the existing Inner) and Failed(original cause, latest cleanup cause,
retained resources). The public `attachment(id, incarnation)` observes this exact
entry; Attached returns an existing Workspace handle, not a second semantic state.
Missing/stale identities return NotFound. Invalid identifiers/zero incarnations
are refused. A failed observation never acts as a successful Attach receipt.

`cleanup_failed_attach(id, incarnation, deadline)` admits one explicit cleanup
attempt. Attaching/healthy entries and a concurrent cleanup refuse Busy. A failed
entry keeps the count slot and accounting while cleanup owns its resources outside
the registry lock; observations report Running. Failed cleanup republishes the
remaining exact owners and latest cleanup error while keeping the original cause.
Only complete resource release removes the exact failed entry. No automatic
Attach replay, new identity, guessed deletion, recovery claim or renewed deadline.

The ordinary failed Attach still performs its initial cleanup through this same
resource owner and original absolute deadline. Fallible BranchContext preparation
now precedes backing/mount/arena acquisition. Any acquired empty arena remains
owned through later publication failure and is explicitly closed before backing
release. Backing uses the existing checked Directory owner; the native mount leaf
records creation/ambiguity and device/inode identity without acquiring the
Linux-only O_DIRECT backing profile, preserving portable read-only Attach.

Admission uses try_lock. All filesystem/backing I/O happens outside the registry
and Workspace semantic-state locks. Final custody restitution uses the existing
short non-I/O registry mutex so a refusal cannot discard resources. Absolute
deadlines are observed before native work and before successful final publication;
syscalls, thread scheduling and final lock scheduling are not preempted. No queue,
helper worker or internal retry is added. A poisoned registry keeps custody but
public inspection/disposition returns Io; this is not a recovery capability.

Managed native directories are exclusively owned by the execution host. A static
replacement before cleanup fails identity verification and remains untouched.
Pathname metadata-check/removal is not an atomic defense against a concurrent
same-owner/root actor renaming managed directories between syscalls; that external
namespace interference is outside this profile. Supported filesystem mutations go
through Workspace/FUSE and do not rename native mount/backing leaves. Tests and
claims distinguish static substitution from that unqualified race.

Workspace owns this lifecycle prerequisite in its private runtime group; Bridge,
Service, FUSE and daemon control contracts are unchanged. Inline entry state is
charged by the existing registry capacity formula. A fixed reservation additionally
covers a retained HistoryFailure and its possible StageWire box. Observation copies
belong to callers. Exact compiler layout, per-file/folder LOC, checks and actual
native selections are recorded below on the frozen implementation.

Declared verification covers public exact identity/active/healthy/expired states;
actual failed native attachment with retained backing and an unowned mount-leaf
sentinel; static directory substitution; concurrent cleanup and observation while
actual removal is held; a syscall crossing the original cleanup deadline; and an
owned mount leaf retained after its creation crosses the original Attach deadline.
Fixtures reuse the closed RO Store/history through actual authenticated delivery;
no writable C5 restart, new bootstrap or product fault hook. Every actual functional
selection has a fresh output and unchanged60-second complete-command budget.

This is a native ownership operation. Remote Attach, daemon target replacement,
writable startup/edit/Commit controls, full R4, namespace/larger-input work, the
prepared DSH upload and matched R6 remain separate. No issue closure, performance,
RSS/cgroup or durability claim follows.

## Frozen source and actual checks

Product input seal`6c3fff5bf08b7273eed005500579351c319b1f5a01f83a0359bfb77738b7a75f`.
Locked Rust1.85.1 whole-core tests passed host664/0failed/3ignored and
Linux662/0failed/126ignored. The six new ignored native selections run separately
below. Whole-core host/Linux bins/examples and warning-denying all-target Clippy
passed, together with fmt, the242-file boundary guard and six guard self-tests.
Preliminary library/focused checks are retained without relabeling them as the
final source. The corrected external caller was rebuilt with whole-workspace
feature unification and checked with owning-crate all-target Linux Clippy; no
already-passing native selection was repeated.

All eight requested functional selections now pass: six native ownership selections
and two actual mounted/control regressions. Every passing row verifies Docker's
actual two-CPU quota and normal cleanup. All passing complete external invocations
fit60seconds; the original mount-substitution attempt is retained as INCOMPLETE.
No build overlaps these selections. Source/binary/input/caller identities and
other-worktree interference snapshots remain distinct from any resource or
performance claim.

| Selection | Status | Driver seconds | Complete external seconds |
| --- | --- | ---: | ---: |
| retained01 | PASS | 2.918102250 | 3.027971500 |
| substitution01 | PASS | 0.957203417 | 1.072752250 |
| concurrent_cleanup01 | PASS | 1.000728125 | 1.114199542 |
| cleanup_deadline01 | PASS | 1.995122166 | 2.104613083 |
| mount_substitution01 | INCOMPLETE | unavailable | 60.007986833 |
| mount_substitution02 | PASS | 4.006215833 | 4.120762916 |
| branch_capacity01 | PASS | 1.099338042 | 1.196608417 |
| mounted Mount success regression | PASS | 3.460156250 | 3.601844500 |
| mounted CloseClean cleanup-failure regression | PASS | 1.406122417 | 1.509756875 |

The first failed mount-substitution caller installed a statx notification filter,
then inspected its own/proc task status before transferring the listener. Native
Attach had not begun. Kernel diagnosis shows the thread blocked in seccomp on that
self-inspection. The external60second supervisor ended the caller before inner
receipt/stdout/stderr publication; no substitute inner receipt was invented. The
original command journal, stack/syscall/fd diagnosis, fixture copies and stopped
container/volume are retained. The fix transfers the listener first and lets the
unfiltered observer inspect the thread. Product bytes and other binaries are
unchanged. The corrected case uses input manifest02; the first four passing cases
retain manifest01. No timeout was enlarged or failed row promoted.

The native cases establish actual failed resource acquisition, original-cause
retention, expired/stale/duplicate refusals, static backing and mount-leaf identity
substitution, and observer/cleanup admission while actual kernel removal is held.
After a removal crosses its original deadline, already released resources remain
released while the empty failed owner/count slot stays until explicit completion.
Restoring the exact owned directory permits cleanup; unrelated replacements and
unowned sentinels remain untouched. Each case later attaches with a fresh incarnation,
reads data.bin through authenticated native delivery and clean-closes.

The BranchContext capacity case forwards the actual native reply, then changes
only its Vec spare capacity in an external adapter. Across12 fresh-incarnation
Capacity failures the observer's accounted bytes remain exactly1,987,356; no
attempt leaves a registry entry or acquired directories. A later normal LocalEdit
Attach/close succeeds, demonstrating the11-arena ceiling was not consumed. This is
an adapter allocation contract proof, not an oversized wire input, RSS measurement
or full writable-history workflow. The closed RO Store/history fixture is reused
as independent bytes; no restarted write authority or new bootstrap is claimed.

[Raw attempt index](evidence/attachment-owner/functional-index.json),
[original input identities](evidence/attachment-owner/inputs-01.json),
[corrected caller identity](evidence/attachment-owner/inputs-02.json),
[check commands](evidence/attachment-owner/checks/commands.json),
[original timeout diagnosis](evidence/attachment-owner/runs/mount_substitution-01/external-timeout-diagnostic.json).
Raw logs are stored as lossless gzip; original uncompressed artifacts remain in
this worktree's core/target/pair1-evidence/attachment-owner. No live npm installation,
DSH mounted upload, remote Attach, R6 or issue closure is claimed.

## Resource and source-size evidence

Rust1.85.1 Linux AArch64 compiler DWARF records Entry80 →304bytes (+224),
EntryState240bytes and AttachResources48bytes. Registry capacity continues to charge
capacity×sizeof(Entry), including old+replacement capacity during growth. Each live
entry additionally reserves HistoryFailure176 + StageWire344 =520bytes for the
possible retained boxes. Relative to the old formula, the requested-byte increase
is224×registry capacity +520×live entries. Existing scratch/window/FD/worker/queue
bounds remain unchanged. These values exclude allocator overhead and do not state
heap/RSS/cgroup peaks. [Raw layout](evidence/attachment-owner/checks/layout.json).

Production LOC: **108781 →109066 (delta +285)**; reference65417 →65417 (+0),
core43364 →43649 (+285). Workspace9783 →10068 (+285); other crates unchanged.
Runtime1268 →1532 (+264), root exported types group501 (+21). The new private
runtime/attachment.rs has256 production LOC/266 physical lines. All production
files remain within999 physical lines and lib.rs/mod.rs within200.
[31](31-source-map-and-loc.md) and the[full inventory](evidence/attachment-owner/source-loc.json)
list every current file and recursive folder with P/L/file counts. This growth adds
checked ownership/inspection/cleanup; it is not legacy retirement or relocation.
Exact first-parent/staged/committed comparison uses the unchanged production counter
and is recorded with the commit; documentation/tests/tools are excluded.
