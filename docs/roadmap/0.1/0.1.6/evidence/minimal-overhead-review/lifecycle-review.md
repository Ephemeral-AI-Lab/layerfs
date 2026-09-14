# Fresh Workspace per tool call: lifecycle and supported-surface review

Owner-requested lifecycle clarification: the proposal's
[continued-operation section](README.md#continued-operation-during-and-after-commit)
now gives ASCII C1/C2 timelines, live-descriptor versus captured-reader ownership,
lock/attempt separation, exact coverage, failure/retry and reclamation checks.
Those guarantees remain prerequisites of #124; #130 is deferred until the earlier
seven steps and verified completion of #124/#125. This addition is documentation,
not a new implementation or verification result.

This is a read-only source review, not a benchmark or V1 qualification. No product
file, fixture, test, measurement or GitHub state was changed. Other agents were
editing `host_overlay.rs`, `overlay_index.rs` and `overlay_payload.rs`; those edits
were preserved. Review initially read HEAD `ca46e2793f1a8c41260e97f3b04d10edaf2612fc`.
HEAD subsequently advanced externally to
`ff7098929eda07d91fbfc2d1072b286231203325` with ordinary reader-cache charging,
while candidate-capacity, snapshot-candidate and Store scratch/spill changes were
active. The table explicitly distinguishes the initial and updated source reads.
The96MiB construction draft was still present at the updated read; it is not a
qualified memory requirement or a promise about later edits.
The v0.1.5 tag resolves to `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`.

## Recommendation

Keep **a fresh Workspace identity per tool call**. Reuse the already existing
Store handle, bound container, authenticated daemon owner and process-owned
LiveRuntime. Create a new branch pin, overlay root, WorkspaceId, transport replay
session and FUSE mount ownership scope for each call. Select the existing daemon
mount/exec route. It already mounts in the daemon process: `handle_mount` calls
`layerfs_fuse::mount_remote` directly, with a map keyed by WorkspaceId and checks
for duplicate roots, authorization and admission. A new helper process, Docker
CLI invocation or container boot is not necessary on this route.

The recommended overlay optimization is a **lazy immutable delta with a bounded,
evictable memory tier and admitted spill**, preserving the current root and
snapshot ownership APIs. Defer payload arenas, unused metadata backing and
maximum outstanding-work reservation until their first actual demand. Avoid
creating a new architecture for service pooling: existing daemon binding and
shared runtime already supply process reuse.

Do not substitute one mutable Workspace with a reset root between calls. That
would conflate branch leases, stale replay requests, inode identity, open files,
mappings and independently held readers. Separate Workspaces can share physical
services and immutable canonical pages; they must not share mutable roots or
reuse old node/session identities as new ownership.

## What v0.1.5 actually measured

Read using `git show v0.1.5:release-notes/0.1.5/benchmark-closeout.md` and its
`benchmark-performance.csv`. The closeout is a release transcription of #120,
not a new release benchmark. Its ordinary samples use seed1/repetition1 with the
published cache/custody contract; ratios to v0.1.3 are historical context with an
undeclared reference cache profile, not paired speedups.

The exact clean1 receipt is
`benchmark-results/host-store/issue120/performance/workspace_change_locality/workspace-clean-commit-1-compact-v2/perf.jsonl`.
Producing binary was `c55daf13e372331a5ab6dbd465ece351a55923831c45864325ac46b1508fa295`
on source `1ff1f2dddeb60493953311de316fa5bec4634a1a`.

| Exact clean1 timer component | Time |
| --- | ---: |
| Begin (`create`) | 6.730750ms |
| Commit | 1.501458ms |
| Visibility | 0.096459ms |
| End | 2.166500ms |
| `pure_call_sum_ns` | **10.495167ms** |
| Attach lifecycle, included in Begin | 5.625375ms |
| Mount-ready, included in attach | 5.406791ms |
| Docker CLI calls in attach | **0** |

The Begin receipt also reports ten snapshot DB calls/rows, 4,968 DB bytes,
ten cached rows/5,928 bytes, and zero Store-wide scans. This is already an example
of cheap fresh Workspace identity with physical service reuse. The closeout's
clean10/100/500 sums are11.87/15.47/17.80ms; they are not isolated Commit timings.

Preparation670.521167ms, command wall581.574375ms and cleanup386.553750ms are
separate sample clocks, not additions to any individual SDK method. Warm fixture
preparation and container/executor setup sit outside `pure_call_sum_ns`. Those
aggregate envelopes do **not** identify a standalone cold attach cost. A current
Snapshot-Isolated fresh Workspace time, fresh-process startup time, CPU/RSS
increment, and cross-platform cold mount time are **unavailable** in this review.
No speedup or new numerical gate is inferred.

## Source-read fixed costs and their meaning

At the initial source read, `Budget::open` reserves the complete declared capacities and eagerly
creates two Index file pairs plus two payload arenas. Each file uses a private
create-new/unlink lifetime. Index files grow as pages are written; these
multi-GiB quota reservations are not multi-GiB physical allocations.

| Default standing admission | Source-derived value | Meaning |
| --- | ---: | --- |
| Initial Budget fixed memory before cache correction | **5,846,016B (5.575195MiB)** | Historical source read; omitted the ordinary SnapshotReader cache |
| Updated fixed memory at ff709892 | **14,234,624B (13.575195MiB)** | Includes the required8MiB SnapshotReader cache allowance; admission, not measured RSS |
| Host overlay disk | **7,784,693,760B (7.250061GiB)** | 4GiB metadata +1GiB payload index +2GiB payload/retention +64KiB move headroom +256MiB scratch |
| Host descriptor slots | **8** | Full Workspace cap reserved immediately; six backing FDs opened by the factory |
| HostClient cleanup memory | **1MiB** | 256 pre-admitted rollback slots; lives in daemon domain for remote mode, host domain for local mode |
| Prior canonical construction draft | **96MiB,128 FD slots,+1.25GiB scratch allowance** | Acquired by current candidate construction; not an empty-Workspace Begin allocation; final qualification remains open |

The default128GiB aggregate disk cap permits17 full default overlay disk
reservations before operation slack/construction. After the correct8MiB cache
charge at ff709892, the128MiB aggregate memory budget permits only9 base
reservations, so memory is now the tighter standing limit. These are source-derived
admission counts, not measured concurrency or resident footprints. Dynamic
admission can improve these limits without raising or lying about capacity. It must reserve the next allocation and recovery margin
before acknowledgment, keep active/retired owners charged, and allow a valid
operation to wait under bounded admission or return its existing resource error.

The fixed memory formula reserves two index traversal/root domains, up to8,192
payload owner slots, request/replay slots, replay bytes and reclamation buffers.
Several vectors are eagerly capacity-reserved as well. The ordinary reader-cache
charging fix is correct and must stay: a replacement shared bounded demand cache
must charge actual retained cache capacity/entries once in the correct Store/trust
domain, not simply subtract8MiB again. Cross-Workspace sharing must not hide a
missing/corrupt object in another Store or drop required retained object ownership.
Reducing a semaphore
preclaim alone is not proof of lower RSS; changing queue allocation alone is not
proof of safe capacity. Both actual ownership and admission must stay bounded.

`LiveRuntime::shared()` already amortizes its two runtime workers, bounded blocking
workers, kernel worker admission, immutable-read cache and transfer semaphores.
`BackingServer::start` adds an ephemeral listener and up to three authenticated
role connections per remote Workspace. Reusing the daemon avoids process startup,
but does not eliminate those per-mount handshakes or mount/unmount operations.
Current public `create_workspace_session` still constructs the legacy Workspace
and projection path; HostRuntime has component/runtime tests and is not yet the
fully migrated public lifecycle. Its source-derived costs cannot be presented as
measured current public Begin regressions.

## Per-call costs that remain real

- **Fresh empty call:** branch pin/expected-root identity, private ownership scope,
  authentication/session, mount registration and verified End remain. Lazy
  metadata can represent immutable base plus a fixed root without opening payload
  arenas. No canonical construction reserve should be needed solely to Begin.
- **Read-only workload:** canonical lazy lookup avoids tree discovery, but kernel
  lookup/readdirplus references and open-unlinked lifetime still need bounded
  ownership. Current HostClient serializes all its data RPC exchanges behind one
  connection mutex; READ, ATTR and LOOKUP therefore pay framing, queueing and a
  host round trip. A cache can avoid authenticated immutable rereads; it cannot
  return newer live data to a captured reader or cache mutable attributes without
  a coherence contract. Parallel connections or multiplexing would be an explicit
  transport change, not a safe one-line removal of that mutex.
- **No-op Commit:** after a valid captured boundary proves no changed effect, avoid
  the full candidate/construction-capacity path and use the existing canonical
  predecessor. Preserve authoritative V4/UpToDate publication resolution and
  coverage; do not infer success from a stale head. The Commit reviewer owns this
  narrow construction optimization.
- **Mounted 'clean' state:** current host generation equals covered generation
  does not prove absence of dirty mapped bytes. `prepare_kernel_open` currently
  ignores the writable flag. There is no current proof that host-only generation
  can bypass V1 even after an apparently read-only command. An explicitly proven
  absence of writable exposure could support a narrower optimization, but that
  proof must be implemented and ordered before open acknowledgment; it is not
  assumed here or proposed as full V1 resolution.
- **End/Discard:** terminate/resolve the call's actual processes, resolve installed
  SDK/Commit uncertainty, close the mount-root descriptor, unmount and verify
  helper/control retirement, then retire kernel refs and host owners. Only then
  release the branch lease/private ownership. End may register bounded deferred
  reclamation where its existing contract permits, but unfinished physical work
  remains charged and reported. Moving an O(graph) cleanup walk into the next
  Begin is not a latency win. Current retained-session pruning is bounded by
  policy but scans/sorts retained records; its amortization is a separate small
  lifecycle concern, not permission to retain unbounded operation history.

## Frozen-spec boundaries for a memory tier or sparse overlay

These constraints come from `overlay-snapshot-spec.md` §§Page and payload backing,
Resource reservation and mutation installation, Snapshot acquisition, Budgets and
complexity, V1, and End/Discard; the architecture design carries matching gates.

| Proposal | Disposition and exact condition |
| --- | --- |
| Lazy immutable base + sparse changed records | Compatible. Base lookup must stay lazy/authenticated; inode aliases, bindings, removals and change coverage remain indexed. No full canonical tree copy/scan. |
| Bounded immutable pages in RAM, spill when needed | Compatible in principle. Captured pages must remain evictable, with stable identity and fallible spill/relocation preserving old readers. Charge cache, metadata and spill/recovery memory before publication. |
| Edit a published page in place when only current root owns it | **Conflicts with frozen draft:** it explicitly requires immutable published pages even when only current owns them. An exclusive-page fast path needs the specification/ownership proof revised openly; it cannot be slipped underneath this implementation as an optimization. |
| Convert the entire RAM graph to disk during AcquireSnapshot | **Forbidden:** acquisition may not enumerate/serialize dirty records, move all cache pages, flush payload, or walk all ownership. O(1) capture includes registration of an already-owned root. |
| Pin all snapshot pages in RAM until Commit finishes | **Forbidden:** snapshot-owned cached pages must be evictable, and repeated post-capture edits cannot grow unbounded resident memory. |
| Spill all current pages on the first edit after capture | **Forbidden:** the first later mutation must retain the affected-path bound, without a full clone, cache conversion or reachability walk. |
| Reuse a disk slot or buffer after dropping only its cache entry | **Unsafe:** root graph ownership, external payload tokens, replay results, SDK leases, read plans and physical readers may still retain it. |
| Release capacity at logical End while delayed readers/cleanup remain | **Unsafe:** reservations follow actual owners, including retired files/snapshots and uncertain publication/replay state. |

A viable memory tier needs an indexed page/extent ownership scheme independent of
resident byte buffers. If a snapshot holds raw transitive `Arc<PageBytes>` graphs,
those pages cannot simply be evicted. If it holds only IDs, lookup/ownership cannot
require an unbounded RAM ID map. Existing stable IDs, disk catalogs, root tickets
and physical-reader leases are useful; a bounded resident directory may cover a
small pre-spill mode, but its transition must itself be bounded/admitted and must
not be deferred to acquisition. No-op/read-only fast paths must not force the
full sparse backing to appear just because a snapshot owner is acquired.

## Independent blockers and self-review

1. **V1 remains open.** The retained probe/investigation demonstrates a real
   writable-mapping visibility gap. Host ownership of acknowledged buffers and
   cheaper metadata storage do not close it. No unchanged probe was rerun, no
   kernel-specific requirement was invented, and no platform impossibility is
   claimed. Public full-surface qualification still requires the concrete supported
   mechanism and exact boundary/error proof.
2. **Public lifecycle integration remains required.** Reusing services is already
   available; the HostRuntime authority must replace the ordinary public path with
   correct SDK/End/Discard/replay handling. The review does not recommend deleting
   old consumers before those replacements exist.
3. **Sparse spill is not just an allocator swap.** Index/payload ownership, stable
   IDs, aliases/cookies, root installation compare, retention and uncertain I/O
   must keep their existing component contracts. The other agents' active capacity
   changes were not tested or overwritten here. The now-published step1 capacity
   receipt records8,193 one-byte files passing, with payload index physical
   allocation71,593,984B (about2 pages/file) and arena33,611,776B. This supersedes
   older16-page estimates; those totals are a narrow retained scale result, not
   the100k read-only or million-file qualification.
4. **Same Workspace reuse was rejected on self-review.** It fails the requested
   fresh-Workspace-per-call identity requirement. The final recommendation reuses
   only physical services and immutable context.
5. **Timing claim corrected on self-review.** The10.495167ms baseline includes
   Begin/attach and End; it is not a1.5ms complete Workspace cycle or a Commit-only
   measurement. Cold service setup remains separate/unavailable.

The lowest-risk implementation sequence is therefore existing daemon binding +
fresh identity, lazy/demand-admitted immutable overlay backing, bounded evictable
cache with incremental spill, then the narrow proven no-op construction path.
None of these independent optimizations requires a global freeze, remount of a
live Workspace, implicit user fsync, mmap removal or an operation-history log.
They are not substitutes for V1 or the final full benchmark campaign.

## Read-side canonical promotion: concrete next design seam

Current `HostOverlay::resolve_name` does substantial persistent work for a plain
cold read. It looks up the canonical directory, calls `acquire_inode`, allocates
a live NodeId, constructs a Base range tree for a nonempty regular file, stores
the inode record and canonical→live map, then stores a binding. The shared binding
hook also creates directory-cookie state. Canonical symlink acquisition constructs
an owned byte tree and can use the payload arena. `lookup`, kernel lookup and
readdir acquisition all reach these paths. A100k-base read-only traversal can thus
populate100k persistent overlay records/range roots without a content mutation.
The current implementation cannot honestly promise zero temporary disk for reads.

There is already a useful read-only precedent: `read_snapshot_path` carries
`CapturedNode::Base` and reads canonical bytes from the captured reader without
importing that file into current state. Reuse that canonical access machinery for
inode-addressed views; do not invent another decoder or canonical encoder.

**Compact scoped canonical identities can avoid this promotion**, with the
following explicit implementation/proof obligations:

1. **Validated reversible IDs, not truncation/hash.** `tree/compact.rs::InodeSerial`
   accepts exactly1..i64::MAX; its32-byte key has a validated zero prefix and a
   big-endian serial suffix. Use this only when the namespace's explicit compact
   profile/scope qualifies. Canonical serial identity is `(scope,serial)`, never
   serial alone across scopes. The root directory's serial is allocated and is
   not contractually1, while FUSE ROOT is1. A possible checked encoding reserves
  1 for the namespace root, maps other canonical serials into2..2^63, and puts
   new live IDs into a disjoint domain. Its root exception must also reject the
   otherwise duplicate root encoding. This is a proposed encoding, not implemented
   or verified here.
2. **Separate live ordinal allocation.** Today `Mutation::allocate_inode` and
   store_inode_record maintain a single raw-NodeId high-water mark, and candidate
   canonical serial assignment adds raw NodeId to a reserved serial range. A
   high-bit domain cannot simply be plugged into those functions: it can cause
   collision, overflow or huge serial reservations. Give new live nodes a bounded
   checked local ordinal, preserve their encoded live ID across Commit, and bind
   C1/C2 canonical correspondence independently. Do not let reading a high canonical
   serial advance the live allocator.
3. **Consult sparse overrides before immutable base.** Lookup/getattr/read for a
   virtual base ID should first resolve captured/current overlay overrides, then
   use authenticated point lookup against that root's immutable namespace/table.
   Plain reads need a transient base file view, not a persistent `Ranges` tree or
   origin allocation. First actual content/metadata/namespace mutation promotes
   that same identity atomically. Cold hardlink aliases derive the same ID and
   must immediately see an already promoted inode. New links, rename, SDK pins
   and replay results continue carrying that stable ID.
4. **Deletion masks must prevent resurrection.** Removing the last live link and
   later dropping its last open/kernel owner cannot make virtual-ID fallback
   expose the unchanged canonical base inode again. Keep the bounded sparse
   current-state deletion/override marker needed by the immutable base. Snapshots
   before deletion retain their own root and still resolve the old file. Handle
   open-unlinked content before reclamation; never reinterpret a reused ID as
   a new file.
5. **Directory parent and cookie ownership are separate.** Canonical inode records
   do not directly give current directory parent; rename must update `..` without
   rewriting all descendant paths. Capture the required parent relationship in
   bounded active-directory state or sparse overlays with authenticated lookup
   provenance. Current directory_cookies seeds bindings and ordered cookie rows
   for every enumerated name. A metadata read fast path alone does not remove
   that persistent growth. A revised cursor/cookie scheme must preserve resumed
   cookies, concurrent insertion/deletion semantics, numeric-cookie bounds and
   kernel reference ownership; it cannot keep an unbounded name→cookie RAM map.
6. **Active references can still be large.** A kernel may retain nlookup references
   for many entries observed by a traversal. A100k scan with100k still-active
   kernel owners legitimately needs O(active owners) ownership metadata. Bound
   resident memory and spill that metadata when necessary; do not fail valid
   active references just to advertise O(1) memory or zero disk. Once FORGET/open
   owners release, readonly cache/reference rows should retire without leaving
   full inode/range/binding copies of every file ever observed. Moving these refs
   outside the content overlay does not remove atomicity: entry lookup+retention
   must still linearize against unlink/root installation. A separate uncoupled
   ref map can let unlink discard the live orphan before lookup acknowledges it;
   retain/revalidate the complete source root or an equivalent proved transition.
7. **Legacy fallback remains supported.** Arbitrary256-bit noncompact inode IDs
   have no lossless direct u64 encoding. Keep the existing bounded-RAM/disk-indexed
   identity mapping fallback for those namespaces unless a separately proved
   deterministic indexed rank mapping is implemented. A cache-only mapping that
   allocates a different NodeId after eviction violates stable inode identity.
   Even a zero-prefixed legacy key is not authority to treat the namespace as
   compact. Report optimized compact and legacy costs separately; do not remove
   legacy coverage or promise zero promotion for that fallback.

The smallest relevant proof set for this seam is: first read/stat/readdir of a
large compact base with content/range promotion counters; repeated scans with
active references retained then forgotten; a high canonical serial and non1 root;
first create before/after those reads; two cold hardlink aliases with a write via
one; rename/unlink/open-unlinked followed by repeated lookup; snapshots straddling
promotion; SDK retry by a pinned identity after path replacement; and arbitrary
legacy keys with the same low64 bits. Verify source authentication and missing or
corrupt canonical-node errors in both paths. None requires replaying the unchanged
V1 probe; neither this optimization nor these component checks close V1.
