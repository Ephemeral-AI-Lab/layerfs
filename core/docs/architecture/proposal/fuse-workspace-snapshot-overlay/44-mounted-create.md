# Mounted regular-file creation

> **Status: implemented and functionally verified for the declared mounted CREATE scope.**
> Implementation parent: `7eb46ef0766eae0d6ccfb894d07919a39c69ef38`.
> Exact implementation commit: `5ed91aaca38dc54e145753a6844b12e6baf30abd`; [confirmation](evidence/mounted-create/commit-confirmed.json).
> Product input seal: `eaa740c5c19b9e3fc858f65cbf45b6f6bcd9233b00973481ae847cc1ef6a776c`.

This round projects the native create/open operation through the existing mounted
mutation permit. It also enables native create while mounted with the established
parent/entry notification path. No new Service operation or backing format is
introduced. The native capacity failure recorded in [43](43-native-create.md) remains
open; this round does not relabel its diagnostic as gate evidence.

## Selected behavior

Kernel CREATE owns one Projection lookup reference and one ready Projection handle.
It retains its single-use mutation permit through the reply. Native mounted create
owns Local references/handles and reports published-handle custody if notification
fails. Existing-name open/truncate retains its exact accepted attributes and uses
the existing mutation path with the proper projection origin.

The adapter validates the Linux regular-file type bit and supported open flags,
applies umask once through the existing kernel profile, and uses the current direct-I/O
reply. Kernel creation does not send reverse entry invalidation while the kernel
holds the parent lock. SDK creation uses the existing checked parent-attribute and
entry callback. Unsupported capability and failure paths remain explicit.

## Actual proof and qualifications

Six mounted-create selectors passed on their first runtime attempts. Three focused
regressions also passed: native create refusal with an unbound projection lease,
existing SDK truncation notification failure, and mkdir notification failure.

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| kernel | PASS | 3.518174791 | 3.656271708 |
| existing | PASS | 1.106816667 | 1.229072000 |
| visibility | PASS | 1.297940500 | 1.423901708 |
| permit | PASS | 0.993867959 | 1.081370208 |
| notification_failure | PASS | 1.003265792 | 1.117482208 |
| successor | PASS | 1.251101667 | 1.336821208 |
| native-refusals | PASS | 1.195405750 | 1.290967666 |
| coherence-truncate | PASS | 1.034462292 | 1.138419708 |
| mkdir-notification | PASS | 0.976367291 | 1.080075833 |

The kernel selection creates exactly three files: empty, mode0400 and mode000.
Actual descriptors verify umask027, requested access, append, grow/shrink via
ftruncate, and exact results after one explicit Commit. This runtime/process is
UID0; initial-descriptor behavior is observed, but unprivileged later-open mode
enforcement is not claimed. Existing positive-name O_CREAT can enter OPEN, so the
existing-name CREATE/truncate branch is separately exercised through the public
projection permit and labelled accordingly. Wrong kinds, O_EXCL, read-only mounts
and unsupported synchronous flags retain their declared refusals.

Visibility verifies a previously missing name, SDK creation, kernel parent attrs,
and an old directory handle retaining its captured view while a new listing sees
the new files. Permit tests preserve the legitimate second read/reply slot, reject
a second mutation permit, consume an invalid attempt, and prove no reverse CREATE
notification with a filter targeting the exact FUSE descriptor. Successful reply
ownership is checked by exact handle release and lookup forget; the unobservable
fuser kernel reply-send result is not fault-injected or claimed.

The entry-notification failure uses an actual thread-local seccomp EIO. The name
and exact ready Local handle survive the failed notification and checked unmount/
remount. After repair, the same handle is used and then released; absence of a
remaining node proves the withheld Local lookup was released. Failed coherence
blocks subsequent mutations before repair. The six new tests share an ENOENT-only
procfs observation helper with the two old notification callers: disappearing
baseline tasks may be skipped, every surviving baseline thread retains its filter
count, and the live faulting thread must show exactly one additional filter.
No sleep, deadline extension, dependency or product test hook was added.

The successor selection keeps an actual created kernel descriptor open across G
capture, a partial G0tail->D1tail update, Stage/CommitStaged completion and the next
explicit Commit. A second D1-born file is created only after Stage is ready. Exact
canonical bytes, metadata roots, the next EditFile/metadata bases, and F containing
only that later-born file are checked. It makes two explicit workload Commits.

All selections reuse independent writable byte copies of the closed master and
fresh live C5 authority. Each uses the original10-second operation deadline,
60-second complete verification budget, CPU quota2 and one construction worker.
No fixture was regenerated and no passing runtime sample was repeated. Eight
receipts include the explicit native clean-close marker; the older coherence
harness records only its narrower external-runtime cleanup qualification. All
owned runtime cleanup succeeded. The [functional index](evidence/mounted-create/functional-index.json.gz),
[source review](evidence/mounted-create/source-review.json.gz) and
[archive manifest](evidence/mounted-create/archive-manifest.json) preserve commands,
caller candidates, exact identities, source changes and every log.

Rust1.85.1 locked/offline verification passed: host691 tests/3 ignored, Linux689
tests/149 ignored, both all-target Clippy runs with warnings denied, fmt, host
binaries/examples, both platform library/binary checks, the254-file product
boundary guard and six guard self-tests. The [check index](evidence/mounted-create/checks-index.json.gz)
links exact command records. Builds/checks/samples ran serially with Cargo jobs2.
CI and the retired aggregate preflight did not run. These are functional results,
not cold-cache, performance, durability or release qualification.

## Resources and production source

The [resource review](evidence/mounted-create/resource-review.json.gz) and
[Linux compiler layouts](evidence/mounted-create/layout/layout-01.json.gz) compare23
fully qualified types against the exact Round43 binary. All matched sizes and
alignments are unchanged, including Creation16 bytes and MutationOrigin1 byte,
State272, Node4320, Handle72, ProjectionState160, mutation permit40, reply permit16,
OpenReservation56 and CoherenceFailure96. Enum storage accommodates the new origin
without increasing those type sizes. These are compiler layouts, not stack-peak,
allocator, RSS or cgroup measurements.

The existing two reply slots, one mutation permit, one remote slot,128 handles,
256 resident nodes,137 candidate pages,26 reconciliation slots,128-KiB I/O and
640-KiB writer charges remain unchanged. Record formats, the128-row/name prepared
bounds,32768-byte metadata envelope and8-MiB replacement limit remain unchanged.
There is no new retained map, queue, worker or automatic resource growth.

Production LOC: **112364 ->112470 (delta +106)**; core46947->47053 (+106),
reference65417->65417 (+0). The exact first-parent/final-staged comparison uses
unchanged tools/production_loc.py, blob b5b9617d08204977176302311e0b2c72a811b420,
with identical product-only scope and exclusions. The
[per-file/folder/crate inventory](evidence/mounted-create/source-loc.json) retains
all254 core and193 reference production files. External-test deduplication is not
counted as a product source reduction.

The Round43 native capacity failure remains open. Fresh symlink construction and
admission, larger full-input admission, the unchanged prepared DSH tree uploaded
in full before one explicit Commit, subsequent incremental Commits, hard memory
qualification and matched R6/#207 remain subsequent work.

Additional retained client/fixture output is indexed in the
[archive supplement](evidence/mounted-create/archive-supplement-01.json).
Original receipts and result classifications are unchanged.
