# Existing-file Linux WRITE and its reply origin

> **Status: ordinary mounted WRITE implemented and verified; not full R4 or Pair 1 qualification.**
> Implementation parent: `d770f5d10bf160b57a90b212102e7960148dba18`.
> Product input seal: `c6fd9f87480c1429114167ff706ce1288872823fd027c86a75b87d8a06af8a83`.
> This round selects ordinary WRITE/pwrite and O_APPEND, including fcntl flag changes.
> Size SETATTR/truncating open is the next independent operation.

## One shared operation and a bounded reply permit

`layerfs_fuse::mount_writable` explicitly selects a LocalEdit Workspace and the
Linux direct-I/O profile. The existing `mount` remains the cached read-only
projection, including the daemon's existing `--mount-readonly` command. A
ReadOnly Workspace is refused before writable mount creation. This round adds
no daemon writable startup/control route and makes no host-SDK/container
management claim.

`Workspace::begin_projection_write(deadline)` returns one non-cloneable
ProjectionWritePermit. It owns the exclusive projected-write flag and one of the
existing two reply slots until Drop. Admission requires a Ready invalidation
binding and zero outstanding replies. Its single-attempt `write_file(handle,
offset, &OwnedPayload, append, deadline)` retains the earlier admission deadline;
a refused attempt cannot be replayed through the same permit. Drop releases only
the slot/flag, with no I/O or implicit Commit.

The adapter validates identity, flags, handle and the 128 KiB ingress ceiling,
then owns the callback bytes through the existing private payload Source path.
The borrowed FUSE slice is fully installed before visible publication and reply.
Both projected and native writes use the same file-ownership, range splice,
zero-gap, inode timestamp, dirty frontier, capture and Commit implementation.
Projection scope cannot be substituted for a native Local handle or vice versa.
Release, READY rights and identity are revalidated at the existing boundaries.

One fresh observation can use the other slot during preparation. Final
publication refuses while that observation still owns its reply. After
publication, preparation buffers, metadata writer and State guards end before
the checked invalidator runs. Fresh reads remain available during notification;
only a second mutator is excluded. The write permit remains alive through the
WRITE reply-send attempt, including errors. fuser does not expose checked reply
delivery or a later kernel-completion acknowledgement; no invented acknowledgement,
retry, sleep, queue or helper worker supplies one.

Projection append uses the current incoming WRITE O_APPEND flag: fcntl can change
it after OPEN. Native SDK handles continue using their stored semantic option.
Projected append requires the incoming offset to equal exact live EOF under the
same metadata writer/stamp, before candidate construction. A count-only WRITE
reply cannot correct an accepted syscall's wrong fd position. Empty projected
append also validates EOF, then returns a no-op without candidate/custody/mtime
publication. A stale incoming append offset is EINVAL; bounded contention is
EBUSY and neither authorizes an automatic retry.

## Kernel profile and completion

Writable mounts retain nosuid, nodev, default_permissions, exec and noatime.
Every regular open, including read-only/exec opens, returns FOPEN_DIRECT_IO.
WRITEBACK_CACHE, ASYNC_DIO, PARALLEL_DIRECT_WRITES, DIRECT_IO_ALLOW_MMAP, DAX and
KEEP_CACHE are not selected. There are still two dispatch loops, one background
request, TTL-zero lookup/getattr, no userspace content cache and no prefetch.
Shared mappings are refused by the kernel's direct-I/O mmap path. O_SYNC/O_DSYNC
and application O_DIRECT are refused explicitly; fsync/fsyncdir remain unsupported.
No durability claim follows. Other not-yet-implemented mutations return
EOPNOTSUPP on this profile; the original RO profile retains EROFS.

Upstream Linux v6.12 only invalidates cached data **before** synchronous direct
WRITE. A private fault or filemap splice can refill old bytes before userspace
publication. The selected completion therefore invalidates inode attributes/data
after publication and before WRITE reply. Reverse invalidation does not acquire
the originating direct-write inode lock, but it can wait for another FUSE READ's
folio. Keeping the second callback slot and releasing metadata working resources
before notification is required. The notification syscall has deadline observation
points, not a proven hard cancellation deadline. Linux discards the page
invalidation helper's error result; checked send success is not a guarantee about
arbitrary pinned/dirty mappings.

Sources: [direct WRITE and its pre-invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L1570),
[write completion](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L1664),
[private mmap and splice paths](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2542),
[reverse invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L517).
This source reasoning is distinct from the actual Linux runtime receipts below.

A failed notification retains the already published receipt and blocks later
mutations. Matching-inode/alias flush reports that exact retained coherence
failure; unrelated inode flush remains successful. Release still cleans up.
Checked detach retires the failed binding, leaving dirty data available for
explicit Commit. No new failure history or backing-sync path is added.

## Explicit qualification limits

Two observed protocol/kernel limits prevent universal writable qualification:

* RWF_APPEND/RWF_NOAPPEND can change a kiocb's append intent without changing
  file->f_flags. This FUSE WRITE protocol sends the latter. The adapter cannot
  claim to detect or enforceably reject these flags. In particular, an SDK growth
  between kernel offset selection and a non-O_APPEND descriptor's RWF_APPEND
  WRITE can make a positional-looking request overwrite the former EOF.
  [kiocb flags](https://github.com/torvalds/linux/blob/v6.12/include/linux/fs.h#L3510),
  [FUSE transmitted flags](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L1072).
* SDK invalidation marks attributes invalid but does not install the new kernel
  i_size. Private faults and filemap splice/sendfile can consult the old size
  before GETATTR; an SDK extension can therefore produce premature EOF/fault
  rejection. A late kernel WRITE/SETATTR update can also install an older size
  after SDK completion. Ordinary direct read/pread and default fstat refresh paths
  have separate safe behavior; tests of them do not qualify cached-size consumers.
  An intervening fstat would hide this limit and is not used to claim it away.
  [filemap splice](https://github.com/torvalds/linux/blob/v6.12/mm/filemap.c#L2890),
  [filemap fault](https://github.com/torvalds/linux/blob/v6.12/mm/filemap.c#L3301).

Fixed-length clean private mappings/sendfile and sequential executable opens
have their own declared tests. Private COW pages owned by an application and bytes
already spliced into a pipe remain earlier observations. Concurrent SDK-size
mapping/splice semantics, RWF append variants and arbitrary pinned-page coherence
remain unqualified. No dependency patch, fallback or extra producer masks them.

Size SETATTR has a different NOWRITE/post-reply invalidation sequence. It is not
implemented by calling this WRITE notifier from its callback. Its exact origin,
reply attributes, truncating open and mounted shrink/extend proofs come next.

## Resource account and verification

The fixed projection reservation is now `size_of::<ProjectionState>() +
size_of::<ProjectionWritePermit>() + size_of::<ProjectionReplyPermit>() + 64`.
On the selected 64-bit layout the permits are 40 and 16 bytes: at most one writer
plus one observer, or two smaller observers. This adds 24 bytes for permit storage
and at most eight for the flag in the boxed state, charged by actual size_of.
No slot, window, FD, worker, callback allocation or growing metadata structure was
added. Existing payload/metadata disk accounting, one-consumer working limits and
single construction producer remain. Resource values are accounted allocations,
not RSS/cgroup or performance measurements.

R4 truncate/extend, wider daemon
controls, namespace/new-inode/larger-input dependencies, the declared npm target
and matched R6 remain open. No issue is closed.

## Actual results and retained scope

Eleven new selections pass: nine real Linux mounts and two public native
projection API subsets. Twenty current-source native write/coherence regressions
and the real daemon mount/authenticated Status route also pass: **32 PASS, zero
functional failures**. No case was rerun to obtain a different functional result.

| New selection | Route | Complete command seconds |
| --- | --- | ---: |
| positional | actual Linux mount | 5.236311625 |
| append | actual Linux mount | 0.853526375 |
| read_race | actual Linux mount | 0.831318291 |
| mappings | actual Linux mount | 0.872798375 |
| exec | actual Linux mount | 0.874264084 |
| native_save | actual Linux mount | 2.067468250 |
| ingress | actual Linux mount | 0.880905416 |
| quota | actual Linux mount | 0.757066792 |
| backing_failure | actual Linux mount | 0.769511583 |
| origin | native API subset | 0.820112042 |
| completion_failure | native API subset | 0.830540458 |

All selection commands remain below the 60-second functional budget. The longest
native write-frontier regression is 27.017444958 seconds; mounted daemon/Status is
24.906925166 seconds. These are functional budget observations, not performance
measurements. No same-worktree build overlaps a selection. Each driver retains
its actual interference snapshot and immutable binary/source identity; setup uses
an independent byte copy of the existing closed Store and fresh live C5 fixture
authority. There is no cold-cache or hard RSS/cgroup claim.

The positional mount keeps aliases/FDs across zero-gap extension and A/B/A
incremental Commits, verifies exact saved bytes and one logical Commit each, and
checks timestamp visibility. Actual concurrent append preserves both bytes and
fd positions. fcntl clears/sets O_APPEND after OPEN; both directions use the current
WRITE intent. The shell appends to an existing file. The held native READ on a
separate inode produces EBUSY before WRITE ingress allocation, keeps the old
version, finishes the read, then accepts a new explicit write call.

Private clean mappings and sendfile see a later fixed-length kernel overwrite
without an intervening fstat; MAP_SHARED is actually refused. O_SYNC/O_DSYNC/
application O_DIRECT, fsync and not-yet-supported truncating open fail explicitly.
ELF true/false/true replacements execute through direct-I/O opens. Actual service
save is observed at SQLite RESERVED ownership, stopped with that ownership
confirmed, and the mounted LIVE write/read completes before resume. The first
saved root retains G; the next explicit Commit saves the successor.

The 128 KiB ingress ownership proof changes the caller's original input buffer
after syscall success, reads and saves the original accepted bytes, and observes
no canonical ReadFile to copy up its 64 MiB immutable base. This kernel splits the
unaligned user buffer into two owned payloads: payload allocation is 143360 bytes,
including headers/alignment; total allocated backing is 163840 bytes with 20480
metadata bytes and 851968 reserved progress bytes. This is a functional account
observation, not a file-residency measurement. The test oracle uses the actual
request count and each segment's header/alignment equation, not an assumed
one-syscall/one-FUSE-request relation.

Quota refusal preserves bytes/size/mtime and cleanly detaches. A real file-size
limit causes private payload allocation failure after an earlier accepted write;
that prefix remains visible. The failed record stays charged (failed_payloads=1),
normal detach succeeds, and clean-close refuses dirty state. The public origin
subset verifies one-attempt permits, old-reply publication refusal, deadline and
append-offset checks, SDK exclusion through the retained reply boundary and the
two-slot ceiling. The public completion subset retains known publication and
reports the same error on alias flush, leaves other-inode flush/release usable,
then detaches and explicitly commits. It is not claimed as a new real notifier
errno proof; the existing real seccomp-notifier failure routes separately pass.

The runtime layout observation is observer=16, writer=40, binding_charge=280 bytes.
The existing fixed working allowance is unchanged. Locked Rust 1.85.1 host whole
core tests pass 653, zero failures/three ignored. Linux FUSE+Workspace ordinary
tests pass 10, zero failures/103 native tests ignored by default; the 11 new tests
and 20 specifically affected regressions above were run through their real
registered routes. Other older native selections were not rerun this round.
Whole-core host/Linux Clippy with all targets/-D warnings, examples/binaries,
fmt, the 241-file boundary guard and six guard self-tests pass. The corrected
pre-run ingress-allocation oracle was rebuilt and Clippy-checked separately;
no product algorithm or limit changed and no failed native result was replaced.

The existing route-seal method yields c6fd9f87…; an earlier inventory additionally
computed compact-JSON(path,SHA256) manifest seal 77acc23f…. Both methods and their
exact correspondence are retained, not conflated. The initial source archive
contains identical production files and the earlier pre-run ingress oracle;
[kernel-write-harness-01](evidence/mounted-write/kernel-write-harness-01/kernel_write.rs)
contains the exact executed caller. Reconstruct that caller by overlaying it at
`core/crates/layerfs-fuse/tests/kernel_write.rs`. All runtime binary hashes and
final test/helper hashes are pinned in
[kernel-write-inputs-02.json](evidence/mounted-write/kernel-write-inputs-02.json).
[All raw receipts, explicit NOT_RUN rows and limits](evidence/mounted-write/functional-index.json)
remain distinct from source reasoning and from full R4/R5/R6 acceptance.

Reproduction uses the same locked commands as round24, selecting Linux
`-p layerfs-fuse -p layerfs-workspace` for ordinary tests. Run each declared
selection once at a fresh output:

```sh
python3 core/crates/layerfs-fuse/tests/kernel_write_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs-02> \
  --test-binary <kernel_write_binary-from-inputs-02> \
  --lock-observer <recorded-store-lock.dylib> \
  --case positional --output <fresh-owned-output>
```

Construction workers remain 1. Target directories remain in this worktree,
registry mount is read-only, ARM build config remains hash
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.
No CI/preflight, third-party modification, benchmark campaign or issue closure
was performed.

## Exact production LOC

First parent `d770f5d10bf160b57a90b212102e7960148dba18`; counted staged tree
`293be53bdefe53ee09a3f94ec80bc1d475c14ef2`. Production LOC: **107860 → 108145
(delta +285)**. Reference 65417 → 65417 (+0); core 42443 → 42728 (+285),
including Workspace 9492 → 9686 (+194) and FUSE 830 → 921 (+91).
Growth adds the real projected-write origin/adaptation; no reference retirement,
relocation or measured simplification is claimed.

Method: `git archive <revision> crates core/crates`, then identical
`python3 tools/production_loc.py --root <archive> --json`; counter blob
`b5b9617d08204977176302311e0b2c72a811b420`. Count nonblank, noncomment
production Rust/runtime SQL, excluding inline/external tests, fixtures, examples,
docs, tooling, manifests and generated output.
[Machine-readable comparison](evidence/mounted-write/production-loc.json).
Final receipt/doc additions do not change the counted production tree.

Committed implementation: `4d5443c1239722c2ed57f0428ad2c32bdbb3d941`.
[Exact changed product/test/manifest paths](evidence/mounted-write/committed-files.json)
are obtained from its first-parent Git comparison; the historical source seal,
raw receipts and production LOC comparison above retain their original identities.
