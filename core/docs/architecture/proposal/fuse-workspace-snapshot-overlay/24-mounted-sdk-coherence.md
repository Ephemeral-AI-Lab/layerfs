# Mounted SDK mutation and checked projection coherence

> **Status: implemented and verified on a real read-only Linux kernel projection.**
> Implementation parent: `cb9d5a8249602e77a454672de290f6358e04c23b`.
> Product input seal: `f0b6f5bf3e7fd0a84a85f5a4ad1446c0ae1bf17d6994c21114e57abb5c7f4d48`.
> Kernel WRITE/SETATTR, full R4, remote SDK edit/Commit control and Pair 1 remain open.

## Public binding and publication order

LocalEdit Workspaces can now own the existing read-only Linux projection while
local SDK mutations remain visible through checked invalidation. MountLease binds
one `ProjectionInvalidation` callback: `Arc<Fn(MutationReceipt, Instant) ->
io::Result<()> + Send + Sync>`. Workspace imports no fuser type. The production
adapter captures only its session notifier and root serial, maps the changed
serial to the kernel inode and calls `inval_inode(ino, 0, 0)`.

`begin_projection_reply(deadline)` returns an opaque non-cloneable permit. At most
two exist, matching the existing event loops. Lookup, getattr/handle attributes,
access, open, read, readlink, opendir and readdir acquire it before selecting a
result and retain it through the final reply attempt, including readdir's add/ok
sequence. Forget, flush, release and releasedir stay available for cleanup.
Statfs returns its unchanged constant profile. Local SDK ReadReply pins are
independent and may still survive later mutations.

The shared mutation path checks binding health before preparation and again under
the final state lock. Any old projection reply or pending completion causes Busy
before publication. Otherwise it publishes inode bytes/length/mtime, maintained
frontier and the exact pending receipt atomically. The preparation helper then
returns, releasing its piece vectors, backing window, metadata writer reservation
and state lock. Only afterward does it invoke the invalidator. The outer operation
remains active through completion, so unmount cannot overtake notification.

Fresh projection permits remain admissible while notification runs: they select
the published new view and can complete kernel reads that invalidation waits for.
The pending slot excludes the next mutation, not reads or explicit Commit. Capture
and known-own reconciliation preserve visible bytes/attributes, so they need no
additional data notification. Completion checks the retained pending receipt,
not the current generation/revision; an intervening Commit may advance those
counters without invalidating this completion identity.

The compatibility cost is explicit: an outstanding mounted observation can make
an SDK mutation return Busy. There is no event queue, worker, notification retry,
rollback, automatic Commit or unbounded sequence log. Empty write remains its
validated no-op. RangeEdit, resize, handle write and truncating local open all use
the common publication/completion path.

## Known publication failures and teardown

WorkspaceStatus adds `projection_replies` and optional CoherenceStatus: Unbound,
Ready, Pending { receipt, published_handle }, or Failed(CoherenceFailure).
LocalEdit reservation is Unbound until installation and refuses publication/reply
admission. ReadOnly reservation retains its immutable pre-bind behavior and can
also bind the callback once. Successful detach clears this state.

`WorkspaceError::Coherence` is an inline failure containing the exact published
MutationReceipt, optional published handle, original I/O kind/raw errno and
`notifier_returned_ok`. A failed notification or deadline observation after it
does not erase accepted state or report a pre-publication failure. It retains the
failed binding and excludes later mutations until checked detach; fresh replies
and cleanup can still progress. No post-publication allocation is required.

A truncating open publishes its READY handle at the existing atomic inode cut.
Its failure therefore exposes that valid handle in `published_handle`; the caller
can inspect/release it. Hiding the handle or rolling back only its reservation
would violate the prior open contract. The actual native failure proof verifies
this behavior, then detaches and explicitly commits the retained empty file.

Unmount stops admission, drains ordinary operations, projection handles, reply
permits and in-flight completion, then detaches, joins and checks mount absence.
A retained **completed** failure does not prevent teardown. MountLease::finish
removes the callback/control owner under State and drops it outside the lock.
Dirty data remains owned. Kernel mount/unmount and notification syscalls have
deadline observation points; they are not claimed to be preemptible.

## Kernel profile and initialization

The mount remains RO, nosuid, nodev, default_permissions, exec and noatime. Writable
open flags and access(mask&2) still return EROFS, and WRITE/SETATTR remain refused.
LocalEdit access must not silently make kernel access writable. No writeback,
shared-writable mapping, direct-I/O-write or append-fd-position profile is added.

Binding occurs after Session::new and before callback workers. Inspection of the
installed fuser 0.18.0 source established that Session::new itself performs its
INIT handshake and synchronous reply attempt before returning. Its notifier is
therefore not bound before that handshake. fuser still logs reply-send failures
internally rather than returning a checked kernel-acceptance receipt; actual
mounted lookup/read in these tests supplies runtime readiness evidence.

An uncached inode's ENOENT is normalized to success by fuser's notification path;
other errors remain errors. Upstream Linux v6.12 source also dispatches inode
notifications without an initialized-state gate and returns ENOENT for an absent
inode. This source inference is separate from the actual 6.12.76-linuxkit proofs.
[Linux notification dispatch](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dev.c#L1990),
[inode invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L517).
Installed fuser session.rs SHA-256 is
1311a037e94d8c874b0df47e1a268ded85a7c57872235e730791ece253b6005f.

## Fixed resource account

The new optional boxed-state pointer raises the existing internal per-Workspace
control reservation from 8,192 to 8,200 bytes on the selected 64-bit layout, and
the minimum admission calculation changes with it. The mount control is allocated
on demand and charged before allocation as
`size_of::<ProjectionState>() + 2 * size_of::<ProjectionReplyPermit>() + 64`.
Each permit is 16 bytes on that layout. The callback is separately charged using
its concrete capture layout plus the aligned two-usize Arc header. This handles
over-aligned captures without guessing their padding; separately owned adapter
session resources retain their own account.

The production callback owns only Notifier plus the root serial; its channel
ownership shares the existing FUSE device descriptor. Pending/failure state is
inline in the charged control. There is no additional FD, thread, growing queue,
content cache or new public budget knob. Metadata working buffers end before
notification; the existing 640 KiB reservation, 8 MiB consumer budget and backing
root/page/window bounds remain. These are accounted-allocation rules and source
layout arithmetic, not RSS/cgroup or performance measurements.

## Actual verification and retained corrections

Eight selections pass on the product seal above. Visibility, read_race, exec,
native_save, notify_failure and truncate_failure use actual Linux mounts.
Completion and deadline test the public projection-completion binding with the
real service/backing but no kernel mount; they are not counted as mounted proofs.

The visibility case keeps the same application FDs open across writes, cross-page
edits, shrink/reextend, mtime changes and repeated Commits. Both hard-link aliases
see exact bytes, including discarded tail bytes returning as zeros. Kernel write
open remains EROFS. The held-read case stops an actual native backing read while
its FUSE observation permit exists, requires SDK Busy without publication, then
finishes the old reply and verifies the acknowledged new version through the same
FD. It also checks the two-permit bound. ELF and replacement shell scripts execute
through the real projection.

The actual-save case confirms the service's SQLite RESERVED owner before/after
SIGSTOP, performs a live SDK write and observes it through FUSE before resume,
then verifies frozen G and the later Commit. The completion API case allows fresh
reply admission and Commit while a notification callback is held; its original
receipt completes correctly after the generation changes. The real-clock deadline
case preserves published bytes and records notifier-returned-Ok before a late
deadline failure. It makes no kernel notification-latency claim.

Failure cases use **native seccomp denial of notifier writev**. A dedicated SDK
caller thread, created after the FUSE workers, installs a filter matching native
architecture, SYS_writev and the unique observed /dev/fuse descriptor. Only that
syscall returns EIO; an ordinary socket writev still succeeds. Filter count rises
only on that caller, which exits before teardown; existing threads remain
unchanged. No OwnedFd is closed or modified externally. The real notification
send fails, but it never enters the kernel invalidation handler, which is the
precise scope of this proof. [Kernel seccomp API](https://docs.kernel.org/userspace-api/seccomp_filter.html).
Both cases retain the known receipt, refuse a later mutation, preserve live FUSE
reply progress, unmount normally and explicitly Commit retained state afterward.

Two initial test corrections remain visible. The first Linux build rejected two
discarded synchronization-lock bindings in the external callback gate; named
bindings fixed those compile errors. The first visibility selection then failed
its subtype-only mountinfo oracle: the actual runtime reports `fuse layerfs`, not
`fuse.layerfs`. The corrected oracle accepts the actual FUSE type/source pair and
its subtype-qualified spelling. Original FAIL, mountinfo, original callers and
checked removal of the owned failed runtime remain separate evidence. No product
source, capability, timeout or selected byte oracle changed for either correction.


## Results, checks and reproduction

| Selection | Route | Result | Complete functional wall seconds |
| --- | --- | --- | --- |
| visibility-02 | actual Linux mount | PASS | 0.990084792 |
| read_race-01 | actual Linux mount | PASS | 0.840328542 |
| exec-01 | actual Linux mount | PASS | 0.845710542 |
| native_save-01 | actual Linux mount | PASS | 1.668624750 |
| notify_failure-01 | actual Linux mount | PASS | 0.830716292 |
| truncate_failure-01 | actual Linux mount | PASS | 0.807625250 |
| completion-01 | native completion API | PASS | 0.799641459 |
| deadline-01 | native completion API | PASS | 1.028711417 |
| visibility-01 | original mountinfo oracle | FAIL | 3.477539417 |

All 84 earlier native Workspace selections also pass on this source. The old
RangeEdit test's former blanket LocalEdit mount refusal is replaced by an explicit
Unbound reservation/publication/refusal/finish check; it no longer assumes the
now-implemented binding is Unsupported. Original historical receipts keep their
old assertion identity. Actual read-only daemon mount/authenticated Status also
passes. The append-only index therefore contains **93 PASS and one original FAIL**:
92 native Workspace cases (six with the new real mounts, two completion API
subsets and 84 earlier cases), plus the actual daemon mount regression.

All complete commands remain below 60 seconds. RangeEdit frontier is
28.885 seconds (exact receipt retained), and the daemon mount is 24.702849917 seconds.
These are functional budget observations. No same-worktree build overlaps a
selection; other-worktree activity remains declared. No performance, cache-state,
RSS/cgroup or full writable Pair 1 qualification is made.

Locked Rust 1.85.1 host whole-core tests pass 653, zero failures/three ignored.
Linux Workspace tests pass ten ordinary tests with 92 native cases ignored by
default; all 92 were selected through their actual drivers. Whole-core host/Linux
Clippy with all targets and denied warnings, examples/binaries builds, fmt,
the 241-file boundary guard and its six self-tests pass. Original Linux compile
failure and corrected caller checks have separate logs. No CI/preflight ran.

Exact commands run from the implementation worktree root:

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

Linux uses the existing Rust tool image with a read-only registry, this worktree
at `/work` and target `/work/core/target-linux`. Its test command selects
`-p layerfs-workspace`; whole-core build and Clippy use the same locked/offline
arguments as host. Construction workers remain 1. Root ARMv8 config SHA remains
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.

Reproduce each declared case once with a fresh output and the immutable identities
in [coherence-inputs-02.json](evidence/mounted-sdk-coherence/coherence-inputs-02.json):

```sh
python3 core/crates/layerfs-workspace/tests/coherence_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs> \
  --test-binary <coherence_binary-from-inputs> \
  --lock-observer <recorded-store-lock.dylib> \
  --case visibility --output <fresh-owned-output>
```

The caller sees only its immutable binary directory and owned private volume;
Store/catalog stay at the host service. Legacy regression driver exposure retains
its previous separate declaration. [Raw receipts and identities](evidence/mounted-sdk-coherence/functional-index.json)
preserve every outcome, original/corrected callers, runtime observations and
checked external cleanup. No issue is closed.

## Next dependency

Kernel mutation needs an explicit origin/reply contract: it must not exclude its
own publication with an observation permit. Projection append must check the
kernel-supplied offset against the live EOF; a count-only WRITE reply cannot
repair an accepted wrong fd position. Kernel WRITE/SETATTR post-reply processing,
clean mapping/direct-I/O profile and actual mounted write/append/truncate proofs
remain required. Notification return success is a checked send result, not a
universal claim about arbitrary pinned/dirty mappings. Authenticated remote SDK
edit/Commit controls, namespace/new-inode/larger-input work, declared npm and
matched R6 remain open.

## Exact production LOC

First parent `cb9d5a8249602e77a454672de290f6358e04c23b`; counted staged tree
`e5035eeb64785bb5981cadd5dc351b19f580ede0`. Production LOC: **107575 → 107860
(delta +285)**. Reference 65417 → 65417 (+0); core 42158 → 42443 (+285),
including Workspace 9249 → 9492 (+243) and FUSE 788 → 830 (+42).
This adds the bounded completion/reply binding and its actual projection adapter;
no reference relocation or retirement is claimed.

Method: `git archive <revision> crates core/crates`, then the same
`python3 tools/production_loc.py --root <archive> --json` on both snapshots;
counter blob `b5b9617d08204977176302311e0b2c72a811b420`. Nonblank,
noncomment production Rust/runtime SQL, excluding inline/external tests, fixtures,
examples, docs, tooling, manifests and generated output.
[Machine-readable comparison](evidence/mounted-sdk-coherence/production-loc.json).
Final receipt/doc additions do not alter the counted production tree.

Committed implementation: `d770f5d10bf160b57a90b212102e7960148dba18`.
[Exact changed product/test/manifest paths](evidence/mounted-sdk-coherence/committed-files.json)
are obtained from its first-parent Git comparison; the historical source seal,
raw receipts and production LOC comparison above retain their original identities.
