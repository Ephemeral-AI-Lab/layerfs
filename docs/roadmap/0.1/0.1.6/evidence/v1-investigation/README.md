# V1 follow-up: dirty-page retrieval is available; snapshot retention is not proved

Status: component investigation, **V1 OPEN**. This directory corrects one factual
claim in the predecessor's analysis. It preserves the original probe and does not
alter the governing semantics or claim product/phase/benchmark acceptance.

## Finding and retained evidence

`FUSE_NOTIFY_RETRIEVE` can obtain dirty cached mapping bytes without `syncfs`,
`fsync`, or a FUSE WRITE request. It can also address an open-unlinked inode by
node ID. Therefore the predecessor's statements that no kernel-buffer retrieval
facility exists and that forcing writeback is the only way to read these bytes
are incorrect. Its original observation that mapped stores do not spontaneously
arrive at the daemon remains valid. Keep
[the original source and observations](../v1-probe/README.md) as recorded evidence;
read their exclusivity/impossibility conclusions with this correction.

The changed-mechanism experiment demonstrates why retrieval alone still does not
meet V1: a page retained by a successful notification remains mutable. When a
later mapped store completes before the daemon reads the queued reply, that reply
contains the later value. This refutes notification-return as a snapshot cut.

| Identity | Observation | Result and scope |
| --- | --- | --- |
| V1-H1: dirty retrieval | Mapped `A`; retrieve returned 4,096 bytes including `A`, with zero FUSE WRITE callbacks | PASS for byte visibility only |
| V1-H2: retained-page isolation | Notify returned while value was `A`; mapped `B` then completed; daemon read reply containing `B`, with zero WRITE callbacks | PASS for the negative oracle: page-reference retention is not content isolation; snapshot-mechanism hypothesis REFUTED |
| V1-H3: open-unlinked retrieval | Mapped `C`; unlinked pathname; node-ID retrieval still returned `C`, with zero WRITE callbacks | PASS for node-ID visibility only |
| Full supported-surface snapshot acceptance | No host/kernel atomic owned snapshot mechanism established | OPEN; not a passed test |

The test scheduler temporarily holds only its own daemon's `read(2)` for H2. The
mapped writer completes during that interval. This is an adversarial observation
schedule, not a proposed product pause or acquisition mechanism. All observations
precede cleanup; `munmap` may initiate writeback during cleanup.

Raw output: [run-01.log](run-01.log). Exact command/image/limits:
[run-01.json](run-01.json). Source: [retrieve_probe.c](retrieve_probe.c), which
includes the original server instead of copying it. SHA-256 custody:
[SHA256SUMS](SHA256SUMS). Exit status is 0 and no probe container remains.

The original H1-style no-notification experiment was not rerun. This run adds the
previously untested notification and its ownership schedule. Existing product
checks were not repeated.

## Kernel contracts checked

The deployed kernel identifies itself as `6.12.76-linuxkit`; the probe ran as
Linux aarch64 with GCC 12.2.0 in image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
It negotiated protocol minor 31, flags `0x21`, and `max_write=1048576`, retaining
the predecessor's cached write-through capability selection. This is not a claim
that the LinuxKit build is byte-identical to upstream Linux. The upstream stable
v6.12.76 source is pinned at
`39b686f8d57d7506af7789e915fe7fd103b0fe57`; the additional v6.12 review is pinned at
`adc218676eef25575469234709c2d87185ca223a`.

### Retrieval and queued page ownership

The documented protocol has supported retrieval since version 7.15. It returns
present pages, stops at the first absent page, and preserves dirty status, so a
later WRITE callback is still possible.
[libfuse API documentation](https://libfuse.github.io/doxygen/fuse__lowlevel_8h.html)

In upstream v6.12.76, `fuse_notify_retrieve` looks up the inode by node ID.
`fuse_retrieve` caps each request by negotiated transfer/page limits, obtains
pages using `find_get_page`, queues `FUSE_NOTIFY_REPLY`, and releases those page
references only when the request ends. It does not clone or write-protect them.
`fuse_copy_pages` copies them while delivering the queued request. The probe
confirms the resulting mutable-reference behavior on the deployed kernel.
[Pinned kernel retrieval/copy implementation](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/fs/fuse/dev.c#L1603-L1727)

Reading all cached pages before returning acquisition would still enumerate and
copy payload proportional to the cached state. Keeping queued references and
reading them later leaks later stores (H2). A sequential multi-inode collection
also lacks a common cut: read old `A`; writer changes `A`, then `B`; read new `B`.
The collected old-`A`/new-`B` pair never coexisted. This is a reasoning consequence,
not a claimed additional experimental pass; separate application operations need
not be atomic for that schedule to refute the collected view.

The newer libfuse API also names `fuse_lowlevel_notify_increment_epoch`. Its
documented purpose is cache revalidation against a connection epoch, not retention
of old page contents. No snapshot handle or COW ownership is returned. It does
not establish interception of CPU stores through already writable mappings.
[Epoch API documentation](https://libfuse.github.io/doxygen/fuse__lowlevel_8h.html)

### Cached/direct-I/O mmap

`fuse_file_mmap` installs `filemap_fault`/`fuse_page_mkwrite`; mapping itself does
not send a daemon request. `FUSE_DIRECT_IO_ALLOW_MMAP` permits shared mappings by
entering cached inode I/O mode, so it does not create a host-owned byte path.
`fuse_page_mkwrite` waits for prior page writeback, but provides no userspace
epoch/COW callback. Holding a page reference does not change that behavior.
[Pinned kernel mapping implementation](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/fs/fuse/file.c#L2387-L2483)

### userfaultfd and page-table scanning

Synchronous userfaultfd protection accepts anonymous, shmem, and hugetlb memory,
not an ordinary cached FUSE mapping. The source explicitly permits the different
WP-async mode on other memory types.
[Pinned eligibility predicate](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/include/linux/userfaultfd_k.h#L200-L231)

WP-async records that a page was written; the kernel resolves the write-protection
fault automatically and sends no fault message. It cannot retain overwritten
bytes for a snapshot.
[Kernel userfaultfd documentation](https://docs.kernel.org/admin-guide/mm/userfaultfd.html#write-protect-notifications)

`PAGEMAP_SCAN` does offer a range scan plus write protection, but its implementation
walks page tables and applies protection per page, with TLB flushing. It is not a
constant-work filesystem snapshot registration API.
[Pinned scan implementation](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/fs/proc/task_mmu.c#L2327-L2418)

Changing to shmem plus synchronous userfaultfd would additionally require
controlling every application's writable alias and new mapping, including
cross-process aliases. Protecting one daemon mapping cannot protect another
process's writable PTE. No such complete adapter exists in the current code, and
requiring cooperating/preloaded applications would narrow the supported surface.
No syscall probe was repeated merely to reconfirm an explicit eligibility branch.

### Passthrough, reflink, and DAX

Passthrough registers a backing file and redirects mmap to that file. Its v6.12
implementation retains kernel file ownership and requires `CAP_SYS_ADMIN`. It has
no snapshot operation or per-inode backing-offset parameter that makes many
logical files ordinary slices of one bounded arena.
[Pinned passthrough implementation](https://github.com/torvalds/linux/blob/adc218676eef25575469234709c2d87185ca223a/fs/fuse/passthrough.c#L116-L130)
[Backing-file API and accounting](https://docs.kernel.org/filesystems/fuse/fuse-passthrough.html)

This does not prove every backing filesystem incapable of snapshots. It does
show that a concrete additional backing snapshot mechanism is necessary.
For the examined Btrfs reflink mechanism, preparation flushes the source mapping
and waits for ordered extents; remapping also takes inode/mmap locks. It therefore
does not provide the missing bounded acquisition over dirty mappings.
[Pinned Btrfs reflink preparation](https://github.com/torvalds/linux/blob/adc218676eef25575469234709c2d87185ca223a/fs/btrfs/reflink.c#L724-L840)

FUSE DAX maps a shared memory window through DAX fault operations. Mapping removal
performs writeback and invalidation; there is no retained snapshot-root operation
in the examined implementation.
[Pinned DAX implementation](https://github.com/torvalds/linux/blob/adc218676eef25575469234709c2d87185ca223a/fs/fuse/dax.c#L738-L859)
The virtio-fs setup obtains a device shared-memory cache region; it is not a switch
that the current `/dev/fuse` Docker session can enable by changing an OPEN flag.
[Pinned virtio-fs DAX setup](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/fs/fuse/virtio_fs.c#L987-L1047)
Even host-readable shared memory still needs acquisition-time ownership that
prevents subsequent stores from overwriting snapshot data. Visibility alone is
not enough.

## Narrow blocker and useful next work

The checked stock APIs do not establish a compliant V1 mechanism. This is a
bounded finding about these APIs and this deployed surface, not a proof that all
future kernel/hypervisor designs are impossible. A semantic waiver is neither
assumed nor requested by this investigation.

The missing platform capability must establish one acquisition epoch covering
kernel-mapped bytes and host-installed state, retain pre-epoch bytes against all
writable aliases, route later mutations into live ownership, and expose retained
reads without scanning/copying all mapped state or pausing ordinary operations for
construction. Errors, eviction, truncate/unlink, and writeback failure reporting
must remain explicit. Simply pinning pages or periodically collecting dirty bits
does not implement those requirements.

If no further repository-level mechanism supplies that capability, the precise
external intervention is access to a supported kernel/adapter or VM facility
with that contract (or explicit authorization and a target environment for
developing one). Access to ordinary `CAP_SYS_ADMIN` or enabling
`FUSE_NOTIFY_RETRIEVE` alone is insufficient; this probe already has both. No
kernel replacement, Docker Desktop reconfiguration, or semantic relaxation was
performed.

V1 prevents full-surface acquisition qualification, final phase-6 candidate
sealing, and phase-7 execution. It does **not** prevent host overlay indexes,
leased-source installation, payload ownership/reclamation, replay and budgets,
snapshot-owned readers over host-installed state, acknowledged ordinary mutation
handoff, canonical correspondence, publication receipts, or the non-Commit
consumer audit. Those tasks remain authorized and independently executable.

No benchmark or #122 case was executed. The existing measurement lock was acquired
nonblocking for the short probe and released normally; no product/Cargo build,
issue comment, commit, release, tag, or platform modification was performed by
this subtask.
