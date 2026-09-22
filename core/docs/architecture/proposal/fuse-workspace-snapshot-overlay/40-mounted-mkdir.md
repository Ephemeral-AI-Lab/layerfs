# Mounted mkdir and checked namespace notification

> **Status: implemented and functionally verified for the selected mounted scope.**
> Implementation parent: `e95c90d757d246d66ac96d2b4fa7e1d6fcbb11ca`.
> Exact implementation commit: `3b34e3002b4a7abfd205b68bc94de9a4af22b572`; [confirmation](evidence/mounted-mkdir/commit-confirmed.json).
> Product input seal: `01706a3acbaf1381755aa0ff602d08b92fa8521f168199951c3dc56573074cbd`.

This round projects the native mkdir operation from [39](39-native-mkdir.md) into
Linux FUSE and allows coherent SDK directory creation on the same mounted
Workspace. Reservation, namespace publication, capacity accounting, backed lookup,
stable directory views and Commit remain owned by Workspace. No new management
opcode, canonical builder, worker, queue or dependency is added.

## Kernel and SDK origins

`ProjectionMutationPermit::mkdir` is a single-use operation under the original
admission deadline. The shared creation implementation checks the appropriate
Local or projected admission before work and at publication, then acquires one
lookup reference in that same scope. The adapter translates the parent identity,
calls the permit, converts the returned attributes and sends one TTL-zero entry
reply. It keeps the permit through the reply attempt and releases the reference
if attribute conversion fails. Fuser's reply API does not expose delivery success;
a send attempt is not evidence of kernel acceptance.

INIT does not negotiate `FUSE_DONT_MASK`. Linux therefore supplies mode after
applying the process umask, with permission/sticky bits only. The adapter passes
that mode unchanged and native umask zero; direct SDK callers still supply and
apply their own umask. No file-type bit is invented or silently stripped.
The selected mapping follows [Linux FUSE mkdir](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L847)
and [VFS mode preparation](https://github.com/torvalds/linux/blob/v6.12/fs/namei.c#L3047).

Projected mkdir does not synchronously notify the kernel about its own entry.
The VFS caller holds the parent inode lock through that FUSE request, while reverse
entry invalidation takes the same lock. Successful creation already installs the
entry and invalidates the parent through the kernel's normal completion path.
See [creation completion](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L736)
and [reverse entry invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L1274).

## SDK completion and failure custody

The existing invalidation callback receives the unchanged applied MutationReceipt
and an optional borrowed `(parent serial, name)` target. File mutations pass None.
SDK mkdir publishes its existing Pending receipt atomically with the child, parent
metadata, binding and dirty accounting. It then releases State, backing windows
and the metadata writer before callback delivery. The active operation guard stays
owned until completion. The validated caller name remains borrowed only for this
synchronous call; no name queue or retained name allocation is introduced.

For an entry target the adapter checks `inval_inode(parent, -1, 0)` for parent
attributes only, checks the original deadline, then checks `inval_entry(parent,
name)`. Existing completion checks the receipt and final deadline before Ready.
The parent call is explicit because entry invalidation can encounter an absent
cached entry. The published fuser 0.18.0 implementation supports both calls and
normalizes cache-absence ENOENT. Existing file invalidation remains unchanged.

If either notification fails or completion is late, the namespace change and
applied receipt remain valid and the binding stays Failed. The API releases only
mkdir's one Local reference whose successful return was withheld; concurrent
lookup references remain owned by their callers. Further mutations are refused.
The selected recovery is the existing checked Unmount followed by Mount, retaining
the dirty namespace. Neither allocation nor mkdir is replayed, and no rollback or
in-place notification retry is invented.

Existing reply exclusion prevents an older lookup result from being installed
after a namespace publication. Reads admitted after that cut select the new view,
including while notification is Pending or Failed. Plain readdir handles retain
their deliberately older pinned views; readdirplus remains unsupported.

## Selected proof and limits

Four new selectors are registered: `kernel`, `visibility`, `permit`, and
`notification_failure`. They cover real mkdir syscalls and mode/umask/refusals;
SDK/kernel observation and stable directory iteration; reply/permit exclusion;
and checked partial notification failure with remount recovery and no leaked
return reference. The failure case targets the exact owned `/dev/fuse` descriptor
and `writev` with four iovecs, allowing the two-iovec parent notification while
refusing only the entry notification. The separate projected-permit case denies
all writev calls to that exact descriptor to detect any unexpected notification.
This uses the published notifier and an external seccomp fixture, with no product
hook. TTL-zero attributes mean the parent-attribute observation does not independently
count its notification syscall; the failure proof uses source ordering and the
actual entry-notification error.

The old native refusal caller is updated narrowly: an unbound projection lease
still refuses before reservation; a bound mount is now supported by this round.
That one changed native selector is selected as a regression. Historical Round39
receipts and caller snapshots retain their original universal-mounted-refusal
scope and are not rewritten.

Each selection uses an independent byte copy of the closed canonical fixture and
fresh live C5 authority, fixed source/binary/caller identities, a 60-second complete
command budget, one construction worker and verified Docker two-CPU quota.

## Actual checks and preserved attempts

The [functional index](evidence/mounted-mkdir/functional-index.json.gz) and
[archive manifest](evidence/mounted-mkdir/archive-manifest.json) retain every
attempt, command, receipt, ownership journal, caller snapshot and log. Times below
are functional command observations, with no cold-cache or performance claim.

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| kernel-01 | FAIL: caller selected read-only mount | 3.354586667 | 3.461560125 |
| kernel-02 | FAIL: wrong long-name errno oracle | 1.013469750 | 1.111856792 |
| kernel-03 | PASS | 1.134860833 | 1.232658625 |
| visibility-01 | PASS | 1.266828250 | 1.404767458 |
| permit-01 | PASS | 1.024162041 | 1.127418292 |
| notification_failure-01 | PASS | 1.033862375 | 1.134407459 |
| native-refusals-01 | PASS | 1.155242542 | 1.278204541 |

The product seal never changed during these attempts. The first caller correction
uses `mount_writable` at all five mount/remount sites and verifies the actual mount
options contain rw and exclude ro. The second matches the established invalid-name
profile: shared child_path rejects a 256-byte component with InvalidInput and FUSE
maps it to EINVAL. The normative contract permits that mapping; this is not a
POSIX ENAMETOOLONG or kernel NAME_MAX-preflight qualification. Each correction has
a new caller/source freeze and immutable executable. The initial independent
caller review missed both errors; the follow-up review checked their source basis.
Failed attempts have normal external cleanup but native clean-close NOT_RUN after
panic. All five final selectors have both selected assertions and native clean-close
PASS, normal process exits and removed owned containers/volumes. No passing selector
was repeated and no fixture was regenerated.

The [check index](evidence/mounted-mkdir/checks-index.json.gz) records locked/offline
Rust 1.85.1 whole-core host tests **680 passed / 3 ignored**, Linux tests
**678 passed / 136 ignored**, host/Linux all-target warning-denying Clippy, host
binary/example build, formatting, the 251-file product-boundary guard and six
guard self-tests. The mounted/native cases above explicitly execute five ignored
tests. Caller-only corrections rebuilt all test executables with the same workspace
feature graph; Clippy passed after the mount correction and fmt passed after the
errno correction. Full product tests were not repeated for unchanged product source.
No CI or retired aggregate preflight ran.

## Resources, source size and next dependency

Linux aarch64 compiler DWARF confirms unchanged sizes: MutationReceipt64,
CoherenceFailure96, CoherenceStatus96, ProjectionState160, mutation permit40,
reply permit16, Handle72 and Cell288 bytes. The exact before/after executables and
raw layout output are in [layout evidence](evidence/mounted-mkdir/layout/layout-01.json.gz).
The post-change layout artifact predates caller-only corrections but has the same
product seal. This is object layout, not an allocated-heap/RSS bound. The borrowed
name adds no retained storage; notifier captures and persistent backing layouts
are unchanged. End-state resource observations are not peak-memory measurements.

Production LOC: **111632 -> 111756 (delta +124)**; core **46215 -> 46339 (+124)**,
reference **65417 -> 65417 (+0)**. Scope and counter are unchanged: exact parent and
final staged product snapshots through `tools/production_loc.py`, blob
`b5b9617d08204977176302311e0b2c72a811b420`; exclude tests, docs and tooling.
[Current source inventory](evidence/mounted-mkdir/source-loc.json) gives all
file/folder/crate counts. No relocation, legacy retirement or counting-scope change.

Next is the shared fresh-file construction prerequisite before native file create.
Workspace needs both saved content and metadata roots for its existing R record
before Stage/Commit and captured-G reconciliation. Reuse the Service save owner
and C1 new-inode support; keep canonical construction outside Workspace. File and
symlink creation, the full preinstalled DSH upload followed by one explicit Commit,
wider namespace operations, hard memory qualification and matched R6/#207 remain open.
