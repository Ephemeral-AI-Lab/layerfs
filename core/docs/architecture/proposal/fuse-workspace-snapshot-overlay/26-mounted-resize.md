# Linux size SETATTR and truncating OPEN

> **Status: ordinary mounted size SETATTR and truncating OPEN implemented and verified; full Pair 1 remains open.**
> Implementation parent: `4d5443c1239722c2ed57f0428ad2c32bdbb3d941`.
> Product input seal: `4eb85c4a14ff66c575314b43ca46073e1e8bf94739205fffbb39b9fab9425031`.
> One size operation follows mounted WRITE. Full R4, wider controls and Pair 1
> remain subject to the qualifications retained in round25.

## Selected size-only contract

The writable projection accepts SETATTR only when size is supplied and every
mode, uid, gid, time or BSD-attribute field is absent. It permits an optional
file handle. A supplied handle must remain a READY writable Projection regular
handle for the exact inode; handleless requests use existing inode/owner/mode
checks. Append intent does not prohibit resizing a writable file description.
Unsupported metadata fields are refused before publication rather than ignored.

The now-two-operation guard is named ProjectionMutationPermit, acquired with
`Workspace::begin_projection_mutation(deadline)`. WRITE keeps the same method
and current append flag. Its size method is `set_len(serial, length,
Option<HandleId>, deadline) -> NodeAttributes`. Both consume the same single
attempt flag and retain the admission deadline. The rename replaces a proposal
API, not a released compatibility promise; there is no redundant alias/facade.

Size shares the native SetLen algorithm: shrinking removes the logical tail,
extending creates Zero ranges without payload allocation, and same-length
requests update metadata/frontier normally. Existing bounds, exact original
identity, current-generation stamp and failure-safe metadata ownership remain.
The method returns complete exact attributes from the publication cut; it does
not publish and then perform a fallible lookup to discover its own result.

A distinct projection-size origin skips the userspace invalidator and leaves the
binding Ready while retaining the exclusive mutation/reply slot. Fresh observation
admission, old-reply publication refusal and SDK exclusion through the reply-send
attempt remain unchanged. The FUSE adapter prechecks unchanged representation
fields, returns the exact published attributes with TTL zero, and retains the
permit through its single reply attempt. There is no synthetic kernel-finished
acknowledgement or callback after reply-send.

## Kernel completion and truncating OPEN

ATOMIC_O_TRUNC remains disabled. In upstream Linux v6.12, ordinary existing-file
OPEN precedes `handle_truncate`. FUSE strips O_TRUNC from OPEN when atomic
truncation is disabled, while leaving O_SYNC, O_DSYNC and application O_DIRECT
visible. The adapter's existing unsupported-flag check can therefore refuse those
combined opens before any truncate. Workspace OPEN does not duplicate truncation.

For ordinary mode<=0777 files without writeback, path truncate supplies size and
no handle; ftruncate supplies size and its handle; open(O_TRUNC) supplies a separate
size-zero SETATTR without a handle. FUSE suppresses automatic kernel-maintained
timestamp input in this profile. The existing semantic operation supplies and
saves the changed mtime. If OPEN succeeds but SETATTR fails, the application open
fails and kernel fput eventually sends RELEASE; the earlier semantic handle must
remain valid until that cleanup rather than being discarded prematurely.

Linux's size path keeps NOWRITE during the userspace SETATTR request. After a
successful reply it installs attributes/size, releases NOWRITE, then truncates and
invalidates cached pages when the size differs. The source warns that laundering
pages before removing NOWRITE can deadlock. This operation therefore uses that
kernel-owned completion, without running WRITE's synchronous notifier before its
reply. A notifier immediately after reply-send would still not establish a
kernel-completion fence.

Primary source basis (not a mounted proof):
[OPEN then truncate](https://github.com/torvalds/linux/blob/v6.12/fs/namei.c#L3760),
[FUSE OPEN flag conversion](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L25),
[truncate/ftruncate attributes](https://github.com/torvalds/linux/blob/v6.12/fs/open.c#L39),
[FUSE ATTR_OPEN handling](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L1944),
[SETATTR completion/NOWRITE](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L2029).
The installed fuser 0.18.0 adapter exposes the Option fields directly but not the
raw validity bits/lock-owner. No distinction between handleless path truncate and
open truncation is needed for the selected size semantics.

## Retained qualification boundaries

This size operation does not resolve round25's RWF_APPEND/RWF_NOAPPEND protocol
observability or concurrent SDK-size cached-i_size mapping/splice limitations.
Ordinary direct reads/default fstat, kernel size completion, fixed-length clean
mapping tests and sequential exec have separate scopes. No universal writable
mmap/cache contract or matched performance result follows.

The daemon's startup/control surface remains its existing read-only mount and
authenticated Status. Workspace/FUSE libraries supply this operation through the
actual mounted integration tests; host-SDK/container writable management requires
its own next transport-owned controls. Namespace/new-inode/larger-input operations,
declared npm and R6 remain open. No issue is closed or older failed receipt
relabeled.

## Verification and accounting

The permit remains 40 bytes and its demand-allocated mount control remains 280
bytes on the selected 64-bit layout; its reservation and two-slot ceiling are
unchanged. The private publication value grows from 96 to 152 bytes by adding a 56-byte
NodeAttributes value. Existing scalar scratch already covers these fixed values;
conservatively retaining the previous 596624-byte transient allowance and adding
this 56-byte growth gives 596680 bytes, 58680 below the unchanged 655360-byte working
reservation. No new dynamic allocation, piece vector, window, FD, worker or
metadata format is introduced. These are source/layout and accounted-allocation
statements, not RSS/cgroup or performance measurements.

Nine new cases pass, eight on actual Linux mounts and one native public-origin
subset. The path/fd sequence verifies shrink/reextend zeros through the same
aliases/FDs and exact successive saved roots. O_TRUNC publishes empty state before
the application's first write; shell `: > existing` also succeeds and commits.
Same-size ftruncate increments the semantic revision and saves its mtime without
an EditFile/content upload. Unsupported metadata calls return EOPNOTSUPP, and
O_TRUNC combined with O_SYNC/O_DSYNC/application O_DIRECT fails before content
changes. Read-only-fd ftruncate fails without publication.

At 1 MiB private quota, truncating OPEN returns ENOSPC, its projection handle drains,
original attrs/data remain, and normal detach/clean-close succeeds. A real 2048-byte
file-size limit instead makes metadata creation fail EIO: old bytes/attrs/revision
remain, while one failed metadata root retains 1409024 reserved bytes and stopped
admission after detach. Zero allocated bytes do not erase that reserved failed
owner; no clean-close success is claimed for it.

Exactly 8 MiB of extension zeros is accepted and committed; another byte is refused.
The proof reads every saved byte in bounded chunks and observes zero payload
records. Actual C2 save is stopped only after confirmed SQLite RESERVED ownership;
mounted shrink/reextend and zero-tail read complete before service resume. G's
saved root retains its old tail and the next Commit saves only the successor view.
The native-origin subset binds a counting EIO invalidator and verifies that size
never invokes it, checks optional-handle identity/scope/rights, original deadline,
single attempt and old-reply/SDK exclusion, then detaches and commits exact bytes.
It is explicitly not a kernel timing proof.

The completed results and reproducible identities follow.

## Completed results and reproduction

All nine new selections and 28 affected regressions pass: **37 PASS, zero
functional failures**. The regressions are all 11 mounted-write/origin selections,
all 9 native SDK resize selections and all 8 mounted SDK-coherence/completion
selections. This yields 23 actual mounted selections and 14 native API subsets;
API subsets are not promoted to kernel proofs. No functional selection was rerun
for a better result.

| New selection | Route | Complete command seconds |
| --- | --- | ---: |
| semantics | actual Linux mount | 4.501556916 |
| open_trunc | actual Linux mount | 0.951611042 |
| same_length | actual Linux mount | 0.876091042 |
| refused | actual Linux mount | 0.812157041 |
| failed_open | actual Linux mount | 0.788103750 |
| metadata_failure | actual Linux mount | 0.787630708 |
| envelope | actual Linux mount | 3.776827125 |
| native_save | actual Linux mount | 1.708332792 |
| origin | native API subset | 0.854569917 |

The maximum complete functional command is 7.058380875 seconds (native resize
frontier). Every selection fits the 60-second functional budget. These are budget
observations, not performance measurements. Source stayed frozen during builds
and proofs, no same-worktree build overlapped a selection, and each raw receipt
retains its actual interference snapshot, immutable artifacts and fresh output.
Closed Store fixtures use independent byte-copy reuse and fresh live C5 history
authority; there is no cold-cache assertion or restarted write authority.

Locked Rust 1.85.1 host whole-core tests pass 653 with zero failures/three ignored.
Linux FUSE+Workspace ordinary tests pass 10 with 112 native cases ignored by default;
the 37 explicitly selected routes above ran through their registered drivers.
Whole-core host/Linux Clippy with all targets/-D warnings, examples/binaries,
fmt, boundary guard (241 production files) and all six guard self-tests pass.
No build/check failure occurred. Requirement-ID metadata in the Python drivers
was aligned before the first selection, retaining both source archives. The
unchanged daemon Status route was not rerun: its prior actual proof stays pinned
to its own source identity. No CI/preflight or performance campaign ran.

[All receipts and explicit remaining rows](evidence/mounted-resize/functional-index.json),
[compiled/executed input hashes](evidence/mounted-resize/kernel-resize-inputs-01.json)
and [kernel/implementation source review](evidence/mounted-resize/kernel-resize-review.json)
separate source reasoning, actual mounted outcomes, failure/ownership proofs and
unqualified behavior. The source archive 02 contains the final registered drivers;
its production seal equals the compilation seal from archive 01.

Exact commands from the implementation worktree root:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Linux uses the recorded Rust 1.85.1 tool image, read-only Cargo registry, this
worktree mounted at /work and target /work/core/target-linux. Its test command adds
`-p layerfs-fuse -p layerfs-workspace`; Clippy/build use the same whole-core
arguments above. ARM build configuration remains SHA256
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.

Run each declared selection once with a fresh output and the immutable paths from
inputs 01:

```sh
python3 core/crates/layerfs-fuse/tests/kernel_resize_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs-01> \
  --test-binary <kernel_resize_binary-from-inputs-01> \
  --lock-observer <recorded-store-lock.dylib> \
  --case semantics --output <fresh-owned-output>
```

The next read/lifecycle control is authenticated Unmount of the daemon's existing
target, with exact incarnation/grant checks and retained native mount ownership.
It will be a separate operation/commit using the existing transport owners.
Writable management, namespace/new-inode/larger-input work, declared npm and R6
remain open; the implemented local mounted pipeline is not their replacement.

## Exact production LOC and changed files

First parent `4d5443c1239722c2ed57f0428ad2c32bdbb3d941`; counted staged tree
`b025631d99c69daa09e9e2f1ec0316c01736b6ce`. Production LOC: **108145 → 108284
(delta +139)**. Reference 65417 → 65417 (+0); core 42728 → 42867 (+139),
including Workspace 9686 → 9783 (+97) and FUSE 921 → 963 (+42).
The change adds projected size validation/exact results and kernel adaptation,
sharing the existing resize algorithm; no reference relocation/retirement occurs.
[Exact changed product/test paths](evidence/mounted-resize/changed-files.json).

Method: `git archive <revision> crates core/crates`, then identical
`python3 tools/production_loc.py --root <archive> --json`; counter blob
`b5b9617d08204977176302311e0b2c72a811b420`. Nonblank/noncomment production
Rust/runtime SQL, excluding inline/external tests, fixtures, examples, docs, tools,
manifests and generated output.
[Machine-readable comparison](evidence/mounted-resize/production-loc.json).
Final receipt/doc additions leave the counted production tree unchanged.
