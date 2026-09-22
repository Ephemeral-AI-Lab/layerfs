# Mounted symbolic-link creation

> **Status: implemented and functionally verified in the declared mounted scope.**
> Implementation parent: `9060c26bcc3e905e031415da20cec54352b94192`.
> Frozen product input seal: `e5589871b9e32f54fbccd458fccdc187302b99dc1f7873f51f65a039721df963`.

This round projects the existing native symbolic-link operation through the
single-use mutation permit and FUSE SYMLINK callback. It reuses atomic child
publication, target payload custody, mode0777 and explicit Commit from
[47](47-native-symlink.md), and the checked parent/name notification from mounted
mkdir. Kernel creation returns one Projection lookup reference and no handle.
The permit remains held through the entry reply attempt; no reverse notification
is sent while the kernel owns the parent lock. A pre-reply attribute conversion
failure releases its withheld reference; entry send has no observable result and
cannot justify guessed reference rollback.

A native SDK call while mounted publishes one Local reference and Pending receipt
with no handle. Checked notification failure preserves the name, target and Failed
state while releasing exactly the unreturned Local reference. Recovery uses checked
Unmount/Mount; no replay, rollback or in-place owner repair is inferred.

Native targets retain their0..4096 non-NUL byte grammar, including empty and opaque
bytes. The Linux syscall obtains its target through getname(flags0), so empty and
4096-byte targets fail before FUSE; kernel-created targets are1..4095 bytes.
[Linux pathname source](https://raw.githubusercontent.com/torvalds/linux/v6.12/fs/namei.c).
FUSE Readlink has a PAGE_SIZE-1 output bound. This adapter declares the Linux
PATH_MAX-1 bound and returns ENAMETOOLONG for4096 bytes on both canonical and local
targets; native Readlink still returns the complete4096 bytes. It never truncates.
[Linux FUSE source](https://raw.githubusercontent.com/torvalds/linux/v6.12/fs/fuse/dir.c).
The SDK-created empty target is qualified by an actual mounted readback, separately
from syscall creation's empty-target rejection.

Seven registered selections passed on their first attempts, with14 checks including
native clean close. Four new selectors cover kernel behavior, visibility/successor
generations, projection permit ownership and notification failure/recovery. Focused
native symlink-refusal, mounted mkdir-notification and mounted CREATE-kernel
regressions cover the changed shared paths.

| Selection | Driver seconds | Complete command seconds |
| --- | ---: | ---: |
| mounted_create-kernel-01 | 1.376319583 | 1.490104417 |
| mounted_mkdir-notification_failure-01 | 1.049001959 | 1.189204459 |
| mounted_symlink-kernel-01 | 4.311424875 | 4.434658583 |
| mounted_symlink-notification_failure-01 | 1.129166709 | 1.228893709 |
| mounted_symlink-permit-01 | 1.064463458 | 1.194311958 |
| mounted_symlink-visibility_successor-01 | 2.173594541 | 2.272391042 |
| symlink-refusals-01 | 1.433157041 | 1.558602583 |

The kernel selection observes relative/absolute/opaque and4095-byte targets,
mode0777 despite umask077, EEXIST/ENOTDIR/EROFS, and the syscall-only empty/4096
preflight failures. The SDK-created4096-byte target has exact native and C1 reads
before/after Commit; both kernel reads return ENAMETOOLONG. The visibility selector
first reads the SDK-created empty target through the kernel while its G constructor
is held, with no prior target read or additional RPC. Kernel D1 creation occurs
only after StageReady; CommitStaged then one explicit Commit preserves its exact
self target and the older open directory view.

The permit case verifies single use, held-mutation exclusion, exact Projection
reference cleanup and no notifier under thread-local writev denial. Notification
failure denies entry notification alone, verifies retained name/target/Failed
receipt with no handle and no hidden Local reference, then checked remount and
one explicit Commit. The real fault thread must remain live with exactly one added
seccomp filter; vanished baseline thread entries use the existing qualified helper.
The native mounted-Unsupported oracle is replaced only by an unbound-lease Busy
oracle. UID0 tests do not claim unprivileged DAC enforcement or entry-reply-send
failure coverage.

All selections retain their60-second complete budget and10-second callback limit,
run in an actual two-CPU Linux runtime with one construction worker, and use an
independent byte copy of the closed fixture with fresh live C5 authority. Native
clean close and normal external cleanup pass in all seven; no forced cleanup or
rerun occurred. These elapsed times are functional receipts, not cache/performance
claims. See the [functional index](evidence/mounted-symlink/functional-index.json.gz),
[caller review](evidence/mounted-symlink/caller-review.json.gz) and
[archive manifest](evidence/mounted-symlink/archive-manifest.json).

Locked/offline Rust1.85.1 whole-core checks pass: host702 tests/3 ignored,
Linux700 tests/160 ignored, both all-target Clippy runs with warnings denied, fmt,
host binaries/examples, host/Linux product checks,255-file boundary guard and
six self-tests. The four new ignored Linux tests were actually exercised by the
selectors above. Builds/checks/samples remained serial with Cargo jobs2 and
Docker CPUs2. [Exact commands](evidence/mounted-symlink/checks-index.json.gz).

No persistent field, queue, scratch limit, page allowance, dependency or worker
count changes. Native47 target/payload/page/reclaim bounds still apply. All13
[compiler layouts](evidence/mounted-symlink/layout/layout-01.json.gz) retain their
size and alignment between the exact native47 and mounted48 binaries, including
Creation24, MutationOrigin1, mutation/reply permits40/16, State280 and Node4320.
These type layouts do not establish heap/RSS/cgroup or measured stack peaks.

Production LOC: **112883 ->112949 (delta +66)**; reference65417 ->65417 (+0),
core47466 ->47532 (+66), across255 core product files. The unchanged counter
`b5b9617d08204977176302311e0b2c72a811b420` compares exact first-parent/final-staged
archives, counting product Rust/runtime SQL and excluding tests including inline
test code, docs, fixtures and tooling. The [source inventory](evidence/mounted-symlink/source-loc.json)
pins the frozen product seal; commit/tree confirmation follows in the continuation.
This adds the projection path without relocation or reference retirement.

Round43's file-capacity FAIL remains open. The full prepared DSH tree, one complete
upload Commit, subsequent incremental Commits and matched R6 remain unqualified.
