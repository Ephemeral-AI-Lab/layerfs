# #273 active backing storage and captured Commit

> **Status:** Source description; no latency, RSS/cgroup or release claim.
> This revision builds on the phase-4.5.1 product source `91c9c4938` and
> implements the bounded hot publication and lifetime changes described below.
> The [phase-4.5 log](../../../issues/273/PHASE4.5-LOG.md) pins their actual
> product/proof identities. The [implementation specification](../../../issues/273/PHASE4.5-IMPLEMENTATION-SPEC.md)
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
reconcile/compaction clones are precharged before allocation. Installation
still pays its actual affected-index work under the gate. Upload retains
charged O(E_f) final extent scratch; it is not a streaming or constant-RAM
Commit claim. Unrelated cursors survive and affected files can readmit after
saved-base installation without a backing reset or process barrier.

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
