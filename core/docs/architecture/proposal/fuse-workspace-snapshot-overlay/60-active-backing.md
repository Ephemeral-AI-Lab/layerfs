# #273 active backing storage and captured Commit

> **Status:** Source description; no latency, RSS/cgroup or release claim.
> Source pin: frozen functional candidate `4ae36ad3a`, last product change
> `a2359620a`, building on the phase-4.5.1 format source `91c9c4938`.
> The [phase-4.5 log](../../../issues/273/PHASE4.5-LOG.md) and
> [frozen handoff](../../../issues/273/HANDOFF-PHASE45-FROZEN.md) record actual
> proof scope and retained failures. The [implementation specification](../../../issues/273/PHASE4.5-IMPLEMENTATION-SPEC.md)
> governs new private v2 attachments; the [v1 record](../../../issues/273/ACTIVE-FORMAT-AND-EVALUATION-v1.md)
> retains historical index receipts and the unchanged v1 pack grammar.
> Checkpoint 5 remains **NOT_RUN**, pending the owner's frozen-candidate review.

`backing/active/page.rs` encodes versioned 4 KiB pack/index pages with the
Workspace incarnation, page ID and epoch, generation/revision, used length,
record count and whole-page SHA-256. Decode checks the expected physical
identity and zero tail before returning body bytes. A new attachment selects
private index **v2**: index magic `LFSAIDX2` at version 2 for leaves and
branches, a fixed 64-slot `LFSAHOT2` directory, and unchanged `LFSAPAK1`
version-1 pack records. `PageStore::create_verified` returns the page it wrote
and verified in place, so a selector decodes exactly the bytes a later reader
sees. `backing/active/keyed.rs` encodes sorted leaf records unchanged
(`key_len:u16, value_len:u16, key, value`) and v2 branch records as
`fence_len:u16, fence, target`; the largest leaf cell is 788 bytes and the
largest branch cell 291. A fence is the child's exclusive upper key and only
the final child may inherit its parent's bound with a zero-length fence, so a
replaced leaf leaves every ancestor record byte-identical. A 17-byte target is
`tag:u8, first:u64, epoch:u64`: tag 0 names a physical page ID and epoch, tag 1
names one directory slot and reuse epoch, and any other tag is rejected.
`backing/active/hot_directory.rs` encodes the selected 64-slot table
(`slot_count:u16, active_count:u16, reserved:u32` plus 32-byte entries of reuse
epoch, physical page reference, level and kind), rejects non-zero vacant or
reserved bytes, requires level 0 exactly for leaves and levels 1..7 exactly for
branches, and validates occupancy before exposing any entry. `backing/active/records.rs` encodes 416-byte inode values
with four optional inline extents and 16-byte namespace bindings/tombstones,
plus fixed inode, namespace and generation-dirty keys. The same pooled index
holds these records beside extent, inverse-reference and locator records; no
tiny file gets its own root page. The `N|parent|name` key omits a length
prefix so names sort in the same byte order as canonical directory listings
and existing continuation cookies.

`backing/active/hot_cursor.rs` retains at most eight inode/generation
frontiers and 64 authenticated decoded index nodes. The resident reservation
cap is 1 MiB, including descriptors, directory and shared pack allowances;
old/candidate copies and operation scratch remain charged separately against
the configured Host Budget. An inode cursor retains I/D membership, its EOF
or Base/Zero source frontier, lower/exclusive fences and ancestor
(slot, reuse epoch, physical content version) bindings. P/R bindings belong
to the shared live tail. Generic publication, including reconcile, refreshes
their affected ancestor bindings even when it has no tiny-WRITE seed. Another file changing a shared leaf does not
invalidate the binding merely by replacing that leaf; every ancestor version
must still match and existing I/D/P keys must remain present. Insertions must
stay inside the bound partition. Frozen views resolve their own directory
and physical pages, never a live cached target.

`backing/active/hot_path.rs` chooses eligible 1..128-byte EOF or advancing
Base/Zero edits before physical allocation. It retains exact gaps, suffixes
and source offsets. Each changed leaf receives one merged candidate; a
replacement through the same HotRef leaves its ancestors unchanged. Shared
P/R rollover is an insertion through the existing bindings, rather than a
new cold root seek for every logical pack page. Overflow propagates through
recorded parent slots. Balanced leaf/branch groups keep at least
1,196/1,693 body bytes in the normal overflow case. The advancing group
retains the old slot/epoch where possible, and closed siblings are Cold
unless another current cursor or Hot descendant needs them. Ordinary EOF
and inherited edits reserve at most 6/7 active files, split events at most
98, and the restricted merged admission/normalization route at most 227.
These are candidate bounds, not total physical occupancy or RSS bounds.

`backing/active/splice.rs` preflights the admission path union and current
users under the fixed slot/byte bounds. Quota, slot or optional-cache
admission refusal keeps the generic v2 route in the same publication.
Normalization converts only unused closure, stopping at shared ancestors;
admission, representation changes and keyed mutation share one reached-node
traversal and physical staging. Changing Hot to Cold keeps the child physical
page and its original birth interval. Each slot has a charged incarnation
high-water reuse epoch; a reused slot increments it and older frozen
directories continue naming their old pair. Epoch or height overflow refuses.
Arbitrary overlap, Payload and other general mutations retain the indexed
splice and its actual affected-work cost.

`backing/active/index.rs` installs one root/height/directory selection and
revision. `hot_directory.rs` validates all four prefix-reserved bytes; its
charged copy includes the actual Rust table/Arc storage. `pages.rs` carries
physical birth facts in custody, so publication does not reread old pages to
recover them. `retirement.rs` routes retired index/directory/pack owners to
the latest selecting revision in their [birth, retire) interval. Unpinned
owners receive one exact unlink/refund attempt before the triggering WRITE
returns. The final pin release visits only its cohort, reassigning remaining
selectors or releasing; failed release remains explicit charged custody.
Ordinary WRITE does not sweep unrelated retired owners. Growing payload and
legacy maintenance remain separate work and counters.

The filesystem wrappers reuse validated selected inode/dirty facts and the
inode returned by publication. `active_file.rs` updates one resident Node
through `node_index`, replacing the all-Node scan. There is one revision per
ordinary WRITE, followed by the existing checked projection notification.
Notification failure retains its published receipt; candidate failure does
not damage acknowledged bytes and never retries through another algorithm.

A capture pins the complete root/directory/pack selection, advances live
generation and seals the shared tail. It does not drain hot closure.
`commit/active_reconcile.rs` prepares frozen G1 extents and saved facts off
the Workspace state gate. Under the gate it rechecks the live inode revision;
matching G1 applies its prepared deletion patch, while intervening G2 keeps
its extents. Prepared rows, cumulative update maps, ordered vectors and
compaction clones are precharged before allocation. C5 transfers its charged
map into the backing publisher, which moves keys/values into the sorted
candidate; it does not retain successive complete patch clones. Touched
logical-ID sets, returned owner vectors and added P deletions keep their
reservations through publication. Installation
still pays its actual affected-index work under the gate. Upload retains
charged O(E_f) final extent scratch; it is not a streaming or constant-RAM
Commit claim. Unrelated cursors survive and affected files can readmit after
saved-base installation without a backing reset or process barrier.
Fresh open-unlinked files retain saved private facts without declaring an
unbound new canonical inode. Reconcile keeps their unintroduced identity,
and subsequent saves may use an installed content base independently of
that namespace-introduction flag.

`backing/active/pack.rs` writes tiny records into a Workspace-shared logical
tail. A slot uses a stable logical page ID and ordinal. A candidate physical
tail page is complete and validated before the caller can publish the pooled
`P` locator for that logical page. The old physical tail remains owned until
the caller proves it is unpinned and calls release. The current writer creates
a new page file for each candidate. `backing/active/extents.rs` stores final
nonoverlapping Base/Zero/Packed intervals and inverse `R` references. A tiny
splice updates the overlapping extents and their inverse references together.
Length changes use that same range splice: shrink removes selected extents
beyond EOF, while extension publishes Zero intervals, so a later growth cannot
expose bytes cut by an earlier truncate. An old pinned view keeps its old
intervals and packed bytes. A size-only change retains the selected portable
mtime; data WRITE stamps the current time. A metadata-only first touch of an
inherited file materializes one inline Base extent so active reads keep the
canonical bytes without allocating ordered `E` records.
The extent codec now also reserves Payload kind `3` and an `L` inverse key
for an owned large `p-*` payload. The codec validates its declared byte range;
the active wrapper publishes a larger payload, its inode and dirty key in one
index revision, and retains a Host-charged payload owner for current and
frozen reads. The range reader carries selected payload owners outside the
active lock. Full orphan-payload reclamation and public
failure proof remain open.
When a large overwrite removes every selected slot on the current packed
tail, its locator and physical page stay owned for the next tiny append;
sealed fully dead pages can be released in the same publication. The external
cross-page overwrite test checks that tail transition.
The active wrapper in `backing/active/generation.rs` publishes pack,
extent, locator, inode attributes and dirty membership through one index root
change for `write_tiny_file`. Its lower-level `write_tiny` still accepts
caller-supplied records for backing tests. The public Workspace selects
`write_tiny_file` for payloads at most 128 bytes, `write_payload_file` for
larger owned payloads, and `set_attributes_file` for length and portable
attributes. A separate `publish_records` call publishes namespace,
inode and dirty changes without a pack append; range scans expose bounded
current and frozen index pages for the later namespace and Commit readers.
The range reader selects one current or frozen inode and extent view, copies
authenticated packed bytes and Zero ranges, then issues canonical Base reads
after releasing the active lock. Its Base-span scratch is Host-memory charged.
`backing/active/reclaim.rs` removes a sealed logical page only when
its inverse references are all gone; captured index generations defer that
physical release until their readers finish. Mixed live/dead-page compaction
remains open. No reader may infer that an unselected candidate is current.

`backing/active/pages.rs` uses the existing verified private directory,
aligned 4 KiB I/O and shared Host quota. It reserves a whole candidate page,
observes `st_blocks * 512`, charges actual blocks and retains uncertain or
partial pages for explicit cleanup. It checks file identity on reads and
unlink, refunds only after physical unlink, and reports outstanding pages,
reservations, pins and incomplete candidates. The page registry's metadata
and I/O scratch are charged to the Host memory budget. These files are
temporary Workspace backing; no sync or crash recovery is added.

Local-edit attachment constructs one active owner for the Workspace
incarnation after acquiring the verified private directory. Failed attachment
closes that owner before the arena and directory; clean close drains it before
metadata and payload cleanup. Repeating the active owner's already-complete
close is safe when a later cleanup phase failed and the Workspace retries.

## Checkpoint 2 public selection

An attached LocalEdit Workspace selects one pinned active view for lookup and
directory listing. A directory handle keeps its index revision and a charged
copy of its path and parent across later edits. `filesystem/active_create.rs`,
`active_remove.rs`, `active_rename.rs`,
`active_file.rs` and `active_attributes.rs` publish namespace bindings,
current inode attributes and dirty membership as one index revision under the
Workspace state gate. Regular WRITE selects one final extent view and reports
one accepted byte count; read uses that view for Base, Packed, Payload and Zero
intervals. Symlink targets use an owned Payload extent in the same name
publication.

The inode header's selected regular-file link count occupies bytes `44..48`
under the checkpoint-2 amendment. Link, unlink and destination replacement
change it with their namespace rows, so lookup remains correct after the
resident Node is forgotten. A charged immutable origin-path map in each view
retains canonical lookup paths for inherited directories moved locally. It is
scoped to the Workspace incarnation and pinned with directory views; the
active index remains the single namespace publication authority. A handle
opened before an inherited directory move keeps the original canonical path;
a later handle selects the moved directory's charged origin map.

The former mutable `RootOwner` publication functions in create, remove,
rename, file write and directory attribute paths have been deleted. Old
keyed-root decoders and canonical/frozen-root readers remain for captured
views and legacy lowering. `stage` and `commit` consume the captured active
tuple described below; neither can acknowledge an empty legacy root as active
data. A dirty Workspace refuses clean close until Commit installs its
successor. If backing
cleanup fails after index publication, the public method completes any checked
notification and returns `Published` with the receipt, retained handle if any,
and cleanup cause. A caller can distinguish accepted data from a
prepublication refusal without replaying the mutation.

The external `active_backing` test covers format corruption/identity,
multi-page pooled index lookup, a captured old locator beside a successor
locator, tiny slots from two inodes in one logical tail, Host quota refusal,
`st_blocks` accounting, pinned-page refusal, and exact close cleanup on an
owned Linux ext4 volume. The successor test checks G1/G2 pack bytes; a
test-thread seccomp fault makes the candidate `pwrite64` fail after its 4 KiB
allocation and confirms the acknowledged tail is intact and the failed file
stays charged until cleanup. The separated 4,096-write storage check exercises
the provisional 3 MiB bound without using FUSE or a timed gate; the repeated
4,097-write storage check exercises dead sealed-page release. A 128-file case
checks two shared pack pages and a 32-generation case checks physical pins and
refunds. A typed record case places 128 inode, namespace and dirty identities
in the shared index. A backing test checks same-revision inode, extent and
dirty publication, a retained inode version after capture, and a complete
128-byte replacement of 128 one-byte extents. Another backs atomic namespace
publication and its captured dirty state; an indexed read test covers
inherited Base, packed, hole and frozen bytes. A same-generation read view
keeps old namespace and packed bytes across successor mutations without
capturing a generation. These are backing tests. Public Workspace tests on a
Linux ext4 backing volume cover inherited lookup/list, create and WRITE,
editor replacement with both old and new open handles, symlink target
retention, forgotten hard-link lookups, append, truncate/zero extension,
portable attributes, quota refusal, typed notifier failure and pinned
directory handles. A privileged Docker FUSE test covers actual mounted
create, append, hard link, cross-directory rename, truncate, symlink and
removals with exact revision accounting. A second mounted test forces an
entry-notifier EIO on `/dev/fuse` after active create and checks the retained
receipt and handle. Mixed-page compaction and the #273 benchmark registry
remain open.

## Checkpoint 3 active capture and lowering

`overlay/snapshot.rs` pins one active index revision, generation, namespace,
dirty frontier and sealed pack tail under the Workspace state gate. Its
submission retains the selected Branch context and counters. The active index
advances its generation and revision at that same capture; later mutations
publish into G2 while a G1 Stage reads the pinned G1 root. The old rooted
capture remains available for legacy frozen-root callers. Active snapshots
are released explicitly after the known Commit outcome is installed; a
retained or failed submission keeps its pin and charge.

`commit/active.rs` scans the captured `D` records and their final `I`, `E`
and `N` values. It sends one existing `SaveFile` request per dirty regular
file, with ordered Base, replacement and Zero descriptors. Packed and Payload
bytes are read through the pinned active view, so the upload does not replay
old journal records. The existing Service chooses C1 `apply_edits` for an
existing base and `construct_stream` for a fresh file. Portable metadata and
symlink targets follow the existing Service operations. Prepared directory
and identity rows use the existing C5 wire format. Their resident encoded
body, dirty set and extent/name scratch are charged to the Host memory budget.
This implementation still scans retained `N` rows for a directory it changed;
its name cost has not yet been proved proportional only to this generation's
name changes.

The active reconciliation path removes captured dirty keys, installs saved
content and metadata roots as the next base for touched inodes, clears their
fresh status, and advances the Branch context under the state gate. It keeps
G2 extents and namespace records intact, including bytes and names that were
published while G1 Stage ran. Repeated G1 content extents and retained
namespace rows remain physically owned until the retirement described below.

External release tests on an owned Linux ext4 volume proved G1 staged bytes
beside G2 live bytes, two Commit outcomes, a fresh named file followed by
clean close, and a directory/file/link/symlink Commit followed by rename and
unlink in a second generation. The existing Stage semantics case also held
the native Save call while a G2 edit published, then checked the frozen
candidate and the live successor separately. A privileged FUSE test wrote and appended
through the mounted path, unmounted, committed, and independently read the
new canonical content root. Those are functional observations at the
checkpoint-3 source. They do not establish the public 3 × 3 timing matrix
or the unchanged #248 gate.

## Checkpoint 4 retirement and compaction

Every published extent replacement removes its old `R` or `L` inverse
reference. Paged inverse scans determine whether a pack page or large Payload
still has a current owner; a 128-entry scan limit cannot mistake the first
full page for the whole set. The owner records its birth and retirement
revision. The index releases a physical pack page or large Payload only after
all captured and read views whose revisions could name it are gone. Cleanup
uses the PageStore and PayloadHost unlink paths, retaining the actual
`st_blocks * 512` Host charge if unlink or identity verification fails. A
failed cleanup stops further admission but a verified read pin can still read
the already published bytes.

A mutation that kills more than half of a sealed pack page's body relocates
its surviving slots into a new pooled page. It rewrites the selected `E` and
`R` records and `P` locator in one candidate index publication, then retires
the old physical page. Ordinary mutations relocate at most one source page;
Commit processes all remaining touched source pages and pools their survivors
across destination pages. When shared Host quota headroom drops below the
conservative page reserve for its update batch, Commit also scans sealed
locators and pools partially dead pages from earlier generations only when
a pair's surviving records fit one destination. It leaves an isolated
partially dead page in place so a low-quota Commit does not spend another
page without a refund. A source page with at most half dead body otherwise
remains charged slack. An
admission that lacks temporary copy-on-write space refuses before publication.
The final ordered SaveFile upload caches one charged
pack page at a time, validating the slot identity and selected subrange. The
pooled index applies a sorted update batch to each reached leaf once; a clean
Commit advances the revision without scanning old journal entries.

Public Linux Service diagnostics in the checkpoint-4 evidence record show
128 one-byte files, 4,097 repeated one-byte overwrites, 32 retained
generations, mixed sealed-page relocation, a large `p-*` Payload refund,
and 4,096 separated writes with full byte oracles and exact clean-close
refunds. The separated-write functional row observed 1,224,704 B of private
allocation before Commit and 24,576 B afterward, below the prospective
3 MiB checkpoint target. These runs do not enforce a cold cache and provide
no eligible latency comparison. The public matrix, separate Commit controls,
and #248 performance gate require the checkpoint-5 receipt set.

### #273 C5 capacity observer (post-013 diagnostic source)

`LFS_CAPACITY_DIAGNOSTIC=1` enables **refusal-only**, bounded production
stderr records. `LFS_CAPACITY_REFUSAL v=1` identifies the actual failed
`Budget::reserve` (including a `Charge::resize` caller) with attempted bytes,
atomic observed used and the **8 MiB configured limit**, or the shared Host
`metadata_reserve` / `payload_acquire` check with allocated, reserved, request
and configured disk quota. The static Rust caller identifies the failing
request, not necessarily its owning high-level operation. A `Capacity` from
integer overflow, index height, allocator `try_reserve`, or some other site
has **no** domain record; missing logs are *not* a zero or a physical proof.
No refusal line is printed on accepted work; the opt-in records do not change
admission. The parser in external tests requires a complete line with
`used+request>limit` or `allocated+reserved+request>limit`.

An active C5 error also emits a separately tagged **post-unwind** `LFS_C5_FAILURE_CONTEXT`:
known canonical outcome, installed revision, Budget used, shared Host charge,
active page count and pins. The latter values are sampled *after* scratch
may have been dropped or candidate rollback completed; do not substitute
these for the refusal-instant charge. If the canonical result is known but
`installed_revision=None`, the operation cannot be retried blindly, and
neither G1 refund nor G2 byte continuity is inferred from Docker cleanup.
Telemetry has no file-format, C1/C2, owner-graph, admission or limit change.

For a selected C5 patch with `D` deleted extent/inverse keys and `K` dirty
rows, preparation retains charged `O(K+D)` rows/deletions concurrently with
selected G1 and mutable G2. Reconcile then retains `O(D)` charged map and
ordered patch scratch; compaction can retain `O(P)` logicals/partial/paired
sets under pressure (P = scanned sealed pack locators), while the index
candidate still owns new 4 KiB versions and the old selected pages until
pins end. The heuristic `(updates.len()+32)*4096` selects the pressure branch,
**not** a physical reservation. On a successful publication, dead G1 owners
are only unlinked/refunded once their final selector/pins end. This observer
alone neither proves an avoidable C5 allocation nor accounts for the rising
no-key WRITE representation pages at 100/512/4,097 and 8,192; those require
separate phase-local closure, carry, slot, eviction and pin classification
before any algorithm or limit change.

### #273 C5 owner overlap, pressure scan, WRITE page-cause observer (RCA identity)

Opt-in `LFS_CAPACITY_DIAGNOSTIC=1` now emits causal summaries in addition to
refusal-only lines. The `LFS_C5_CHARGE` prepared line measures row and deletion
charges retained alongside the selected G1 and mutable G2; it also records
Budget used before preparation, after preparation and after the charged update
map. The index-prepare line measures the already-held update-map charge,
ordered-vector scratch charge and Budget used before/after compaction. These
are **simultaneous charged owners**, not necessarily allocator live bytes;
Budget also covers other Workspace/C1/C2/metadata owners. Do not sum whole
Budget checkpoints as independent allocations. A failed `Charge::resize` line
reports the requested *increment*; `LFS_INDEX_SCRATCH` records its old and
full target charge, update count and key/value lengths. Dropping a charge or
using a deferred uncharged patch to pass a tier would not satisfy the memory
contract. Post-unwind physical/used numbers remain separate from peak.

`LFS_C5_COMPACTION` distinguishes the **quota headroom estimate** from
pressure-branch P locator entries and scan calls, `pack.records` reads on the
pressure pass, subsequent source-page reads, partial/paired counts, and
attempted/successful **4 KiB pack-page creations** in the compaction plan.
These are attempts through product `PageStore`, not a global filesystem
allocation total; C5 index/metadata physical requests, live selected backing
and physical `st_blocks` require their own oracles. The pressure pass may
read unrelated P locators even when it creates zero physical pack pages. Its
read counters are neither SaveFile pack loads nor attempted physical pages.

`LFS_INDEX_PAGE_CAUSE` emits one bounded summary for each *prepared* index
mutation: selected revision/generation, selected/new height, capture/frozen
counts, directory occupancy and new index pages, with eight **disjoint**
creation-source buckets. `changed_leaf` and `changed_parent` have actual
updates in the recursive subtree; `direct_leaf` and `direct_parent` arise
from direct hot propagation; `admission_no_key` creates a generic no-key
node from a cold target admitted to a hot slot; `normalization_no_key` starts
at a selected hot node marked for normalization/eviction; `connection_no_key`
is the residual no-key recursive connection; `height_pages` arise from root
height growth. Each successfully emitted leaf/branch is counted once; the existing
`index_page_writes` total **also includes hot-directory pages**, recorded
separately as `directory_pages` in the causal line. A base Commit can prepare
one candidate in the successor generation while its G1 capture is still held;
the external WRITE observer excludes this pinned candidate before matching
the subsequent contiguous, individually acknowledged WRITE revisions. Hot
slot creation and normalizations may overlap these creation causes and are
separate event counters. `connection_no_key` is **not** a proven removable
page: changed child target, fence, epoch and selected pin obligations remain.
The record is emitted *before* publication and thus includes candidates that
could subsequently fail; an external observer must pair revision/byte
acknowledgements and Commit outcome, never infer that `prepared=accepted`.
The observer formats at most one line per mutation, stores only fixed
counters per mutation, and changes no page encoding, Budget/hot limits,
acknowledgement ordering, compaction decision, worker or cache policy. Its
wall includes observer overhead and is not a matched speed sample.

### #273 map-to-vector charge transfer and generic split witness (implementation)

C5's patch retains an ordinary sorted `BTreeMap` with one `128 + key.len()
+ value.len()` Budget allowance per original entry. Its ordered index input
is a `Vec<(Vec<u8>, Option<Vec<u8>>)>`. Prior to this change, `into_iter()`
exhausted and deallocated the map **nodes**, but the map's entire charge
persisted while the ordered tuples and index scratch were also charged. At
the 8,192 diagnostic, the redundant old-node allowance was
`16,388 * 128 = 2,097,664` bytes. This amendment transfers ownership of
**only those freed nodes' allowance**; no selected page, revision, canonical
root, G1/G2 payload or pin is rewritten.

Before moving anything, C5 reserves the requested `len * size_of::<Update>()`
ordered-Vec charge, uses fallible `try_reserve_exact(len)` and adjusts that
charge to the Vec's **actual retained capacity**; an allocator over-allocation
that cannot be charged fails before draining the map. Then `extend` consumes
every BTreeMap entry. Its key and optional value Vec buffers move unchanged;
their actual *capacities*, with checked arithmetic, replace the old map
node+logical-byte allowance only **after** the nodes are gone. The previously
precharged ordered Vec retains the tuple headers. The index's separately
charged scratch still covers its mutation/recursive temporary allowance.
A failed precharge, allocation or resize aborts the compaction candidate,
leaves the index unpublished and returns `Capacity`; C5 still owns any known
canonical result. No after-ACK work, spill, new worker, pin reset, quota
change or fsync is involved. Success may retire G1 only after final selectors
end; surviving G2, metadata, old readers and unknown owners remain charged.
The `map_transferred` refusal diagnostic reports original map charge, actual
key/value/ordered capacities and the post-transfer Budget checkpoint.

`Mutation::emit` additionally records **generic split events** (`nodes.len()>1`)
and the total new index pages emitted by those split groups, independent of
its disjoint page-creation causes. Direct-route carries are counted at the
existing two `Counter::Carry` sites. The per-revision diagnostic is v2;
v1 receipts keep their original parser and source identities. A split-group
page can also be changed-key or normalization-only, so the new split counts
must **not** be summed into the page-creation total.

**No normalized page is skipped by this change.** In the existing v2 grammar
`Mutation::change` returns the previous target when there is neither an
update nor heating, or when its reconstructed branch children equal the old
children. For an emitted no-key branch it has observed `merged != original`:
at least one authenticated child target/fence relation changed. Substituting
the old page without an alternate target/epoch mapping would leave the new
selection's parent referring to the wrong target (or to a cleared/reused hot
slot), even though an old frozen selection must still resolve its old page.
`emit` validates each new child kind/slot epoch and encodes the new fences;
the old snapshot retains its own selected root and directory, and retirement
waits for selecting revisions. This proves **why simply omitting those pages
is invalid under the current grammar**, not equivalence of a proposed
replacement algorithm. No normalized-page removal is considered safe until
a separately specified target/fence/epoch/old-pin equivalence and public
failure-custody oracle exist. The generic split count and pinned controls
are necessary observations, not such a replacement proof.
