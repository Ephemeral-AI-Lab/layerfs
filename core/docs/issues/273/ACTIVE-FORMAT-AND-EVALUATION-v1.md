# #273 active Workspace format and public evaluation, version 1

> Prospective checkpoint 0 contract. This document freezes the intended format
> and cases before product source or candidate samples. It records design bounds,
> not implementation or measured results. The parent is #271; its original #248
> 4,097-WRITE gate remains a distinct, unchanged selection. The source baseline
> is `48b51e874a41b3e1e6c6661e145316df8b408f07`.

> **Later source audit / owner direction (2026-09-28):** The active route's
> resident Nodes and dirty identities grow through Budget admission; the
> 256-node/128-successor-dirty wording below is not enforced there as stated.
> #264 owns the main lane's namespace/resource work and integrates after this
> side lane completes. The [phase 4.5 proposal](PHASE4.5-IMPLEMENTATION-SPEC.md)
> defines prospective v2 hot publication before checkpoint 5. This notice
> changes no historical page grammar, receipt, registered case or limit.

> **Preimplementation correction, 2026-09-27:** The direct physical slot
> choice in checkpoint commit `d9f8ef615` would make each safe shared-tail
> copy repoint every live extent on that page, including unrelated files.
> The corrected v1 record carries a stable logical pack-page identity and
> the pooled index has one physical locator for the active tail. No v1
> product page or candidate measurement existed before this correction;
> the original commit and issue note retain their historical wording.
> The later storage slice also fixes the previously omitted packed-slot length
> in the 56-byte extent value below. No public v1 page exists yet; the earlier
> commits remain unchanged.
> The pooled inode/namespace slice subsequently fixes its typed values below
> before those source files are committed; this is still a preselection v1
> contract and there are no mounted-format receipts to reinterpret.
> A later pre-mounted correction removes the namespace key's length byte.
> Length-first ordering would make an indexed namespace scan incompatible
> with the existing byte-ordered canonical listing and directory cookie.
> No mounted v1 page or candidate sample has been selected; the earlier
> source commit and this correction remain separately visible.
> Before the mounted switch, the large-WRITE review found that a 128-byte
> packed slot cannot represent the public 128 KiB FUSE WRITE without splitting
> one syscall into a large unbounded index batch. The v1 extent grammar below
> now reserves a Payload kind and `L` inverse key for the existing `p-*`
> allocator. No mounted v1 page or candidate sample exists; this amendment
> precedes the corresponding product code.

> **Implemented v2 amendment, 2026-09-28:** Phase 4.5.1 replaces the private
> ordered index for **new attachments** with the v2 grammar in the
> [phase 4.5 proposal](PHASE4.5-IMPLEMENTATION-SPEC.md) §4.2: index magic
> `LFSAIDX2` at version 2, fixed fences with tagged child targets, and the
> fixed `LFSAHOT2` 64-slot directory. The v1 pack record, the `LFSAPAK1`
> version-1 pack page, every C1/C2/Bridge format, leaf key/value meanings and
> the 416-byte inode and 16-byte namespace values are unchanged. No running
> attachment changes format, no old incarnation is adopted or migrated, and no
> v1 receipt is relabelled. The v1 page/name grammar below therefore describes
> the retired current-attachment index only; it remains the historical record
> for older receipts and for pack pages, which v2 keeps. This notice is
> prospective format authority, not a measurement or a speed claim.

## Boundary and identity

One attached Workspace incarnation owns one active tuple: `(incarnation,
generation, revision, namespace root, inode root, dirty root, extent roots,
pack watermark)`. It persists across Exec calls. Publication changes the tuple
once under the existing Workspace writer gate, then performs the existing
checked projection notification. A capture pins the entire tuple and closes
its pack tail; successor edits use new page versions. G1/G2 readers use their
captured tuple until their pins end. Commit lowers only the final indexed view
of dirty inodes through the current SaveFile descriptor/replacement stream;
C1 `apply_edits` and `construct_stream` retain their current roles.

The existing verified `<WorkspaceConfig.root>/private-backing/<workspace-id>/`
directory owns all new files. The names are `a-pack-v1-<page-id>-<epoch>` and
`a-index-v1-<page-id>-<epoch>`, with fixed-width lowercase hexadecimal IDs.
They are disjoint from `p-*`, `m-page-*`, and `m-ledger-*`. Each name identifies
one **allocated 4,096-byte file**, created exclusively relative to the verified
directory handle with no symlink following. There is no preallocated segment,
file directory, or index root per tiny file. The existing large-payload format
remains readable for legacy roots, old captures, and writes larger than the
tiny-slot limit. No old file is rewritten into v1 on attach. A partial prior
incarnation is quarantined, never silently adopted or migrated. This temporary
backing adds no crash-durability or `fsync` contract.

## Page and record bytes

All integers are big endian. Every page is exactly 4,096 bytes; bytes 0..128
are its header and bytes 128..4096 its body. Header offsets are: magic `0..8`
(`LFSAPAK1` or `LFSAIDX1`), version `8..10` (1), page kind `10..12`,
Workspace incarnation `12..44`, page ID `44..52`, reuse epoch `52..60`,
birth generation `60..68`, last revision `68..76`, used body bytes `76..78`,
record count `78..80`, SHA-256 `80..112`, and zero `112..128`. The digest is
SHA-256 of the entire page with bytes `80..112` zeroed. Read validates the
header, digest, file identity, expected kind and zero tail before exposing a
record. Page ID never aliases another live page; epoch increases before reuse.

A tiny packed record has a 48-byte header followed by 1..128 data bytes:
inode serial `0..8`, generation `8..16`, revision `16..24`, logical offset
`24..32`, stable logical pack-page ID `32..40`, length `40..42`, ordinal
`42..44`, flags `44..46` (zero in v1), and zero `46..48`. The page digest
authenticates the complete record. An extent names `(logical pack-page ID,
ordinal, inode, generation, revision, logical range)`; every read resolves
that logical page through the captured index locator and checks those fields
against the record. One locator update publishes a new physical tail copy
without rewriting extents in other files. A split/overlap can leave several
extent references to one slot. A disk-backed inverse-reference index names
them for compaction. Records are contiguous from byte 128, with no separate
directory: 80 one-byte records or 22 maximum-size records fit in 3,968 body
bytes. The 4,096 one-byte checkpoint therefore needs 52 occupied pack pages
before pinned copies and the inactive tail.

Index pages use the same header and hash, with kinds for branch, leaf, and
pooled small-inode records. A leaf body holds contiguous sorted records:
`key_length:u16, value_length:u16, key, value`; a branch body holds sorted
`key_length:u16, key, child_page_id:u64, child_epoch:u64` records, including
its rightmost child as the final record. Each key is that child's maximum.
Key prefixes are `N|parent:u64|name` for a
namespace binding/tombstone, `I|serial:u64` for current inode attributes,
`D|generation:u64|serial:u64` for the dirty frontier,
`E|serial:u64|start:u64` for one extent,
`P|logical_pack_page:u64` for one physical `(page ID, epoch)` locator, and
`R|logical_pack_page:u64|ordinal:u16|serial:u64|start:u64` for one inverse
slot reference. `L|payload_id:u64|serial:u64|start:u64` is the inverse
reference for one large payload extent. Duplicate keys and unsorted,
overlapping extents are invalid.
The `I` value is 416 bytes: a 160-byte active inode header plus four 64-byte
inline slots. Header offsets are revision `0..8`, generation `8..16`, length
`16..24`, kind `24` (`0` file, `1` directory, `2` symlink), fresh flag `25`,
storage `26` (`0` empty, `1` inline, `2` ordered `E` records), inline count
`27`, portable mode `28..32`, mtime seconds `32..40`, mtime nanoseconds
`40..44`, selected regular-file link count `44..48` (zero is valid for an
open-unlinked inode), base content root `48..80`, metadata root `80..112`
and zero `112..160`. Each occupied inline slot is an 8-byte logical start
followed by the 56-byte extent value; unused slots are zero. Short files
therefore share pooled inode pages; longer sequences use ordered `E` leaves.
This checkpoint-2 amendment assigns the previously zero `44..48` field
before any mounted active page exists. Link, unlink and replacement update it
in the same index publication as their name records; lookup after forgotten
resident references reads this count from the selected inode. The active
files are temporary to one Workspace incarnation and never migrate across
attachments, so no earlier stage-1 storage-only page is adopted or relabeled.
An extent value is
exactly 56 bytes: end `0..8`, kind `8` (`0` Base, `1` Zero, `2` Packed,
`3` Payload),
zero `9..16`, source offset `16..24`, logical pack page `24..32`, ordinal
`32..34`, full packed-slot length `34..36`, source generation `36..44`, source
revision `44..52`, and zero `52..56`. Unused source fields are zero. The
Payload kind uses the existing verified `p-*` large-payload allocator: its
`source_offset` selects bytes within that payload, `logical_pack_page` holds
the nonzero payload ID, `ordinal` and `slot_length` are zero,
`source_generation` holds the payload's declared byte length, and
`source_revision` is zero. Every selected Payload range must fit that length.
The active Workspace retains a Host-memory-charged ownership descriptor for
each live or pinned payload ID; an `L` inverse reference counts each selected
extent. A payload becomes reclaimable only after its last current inverse
reference and every frozen/read pin that could name it have ended. Physical
`p-*` block refunds still follow the existing PayloadHost custody and actual
unlink, never a logical extent deletion. A failed transfer or cleanup remains
owned and charged. The 128-byte packed slot stays the small-write fast path;
one larger public WRITE publishes one Payload extent and one Workspace
revision without a per-WRITE `RootOwner`.
The `P` locator value is the physical page ID and epoch (16 bytes); `R` and
`L` inverse-reference values are exactly `[1]`. A namespace `N` value is 16 bytes:
serial `0..8`, kind `8`, tombstone `9`, zero `10..16`; current portable
attributes reside in the corresponding `I` value. A dirty `D` value is
exactly `[1]`. A leaf holds ordered,
nonoverlapping `(start, end, Base | Zero | Packed | Payload)` intervals.
The namespace key's remaining bytes are the 1..255-byte name, so keys for
one parent sort in the same byte order as canonical `Inspect::List` and the
directory cookie; an exact key is unambiguous even when one name prefixes
another.
The current root and at most one right-edge leaf/spine per hot file may be
cached; every cache page, staged candidate and handle is charged to the
Workspace's Host memory budget. The page cache is capped at 64 index and 8
pack pages, with eviction before admission; there is no resident map of all
files or extents. The mutable active tuple uses page versions without creating
an immutable `RootOwner` or traversing ownership edges per WRITE. A cached
right edge is a fast path, not a different format or acknowledgement rule.

## Safe publication and physical accounting

The active pack tail has an acknowledged page and an inactive candidate page.
Each tiny WRITE constructs a complete candidate from the acknowledged page,
adds or replaces its slot, writes and re-reads the inactive page, then stages
the affected index pages and one tail locator change. A failed or short write
never selects the candidate;
it cannot damage an earlier acknowledged slot. Candidate index pages are also
new versions. Once all pages validate, the Workspace gate publishes bytes,
length, attributes, dirty membership and revision as one tuple update. Old
pages can be reused only when no active, captured, handle or uncertain owner
pins them. A postpublication notifier failure retains the typed coherence
failure and the published receipt. An uncertain reply never triggers replay.

Before creating a candidate, reserve its worst-case whole 4 KiB page count in
the shared Host disk budget. Transfer the exact `st_blocks * 512` observed
for each created file to that Workspace's charged allocation; charge any
excess or failed partial allocation and stop admission if the reservation was
insufficient. Keep a candidate's identity, completed bytes, actual blocks and
failure phase until successful cleanup. A failed unlink, identity check or
uncertain outcome leaves the page owned, charged and quarantined. Refund
exactly the blocks observed before a successful physical unlink, after all
handles close; a logical tombstone alone refunds nothing. One page file makes
release and its block refund exact without hole punching. Clean close drains
pins and candidates, accounts every page, removes `a-*` files, and only then
allows `Directory::close` to remove the empty private directory.

Every physical page records its birth capture sequence. On replacement, it
is pinned exactly by captures made while it was current. Uncaptured old
versions can be released in the publishing operation; captured ones enter an
indexed retired-page queue and release after their last pin. A mixed sealed
pack page may be compacted with other pages only after all surviving slots
and their inverse references are relabelled to a new logical pack page and
the new indexed view is published. Compaction counts slots
moved, references changed, 4 KiB reads/writes, and actual released blocks.
Trigger when dead bytes exceed half a sealed page or quota admission needs
reclaimable space; cap each mutation's relocation at one page and continue
charged work at capture/Commit/close as applicable. No unbounded debt may be
left after a complete measured command. A frozen generation never changes.

The first implementation supports at most 128 successor dirty inodes (the
existing reconcile refusal), 256 resident nodes, 128 handles, and 32 retained
roots. It must refuse at a bound before mutating; it cannot claim unlimited
files or generations. A name's deletion is a versioned tombstone. Open-unlinked
handles and captured roots keep their data and physical charges until release;
directory handles retain their selected namespace view.

## Prospective cost and space gates

Let `F` be indexed files, `D` dirty identities, `E_f` extents of file `f`,
`K` extents overlapped by a write, `L` live tiny slots, `J` allocated pack
pages, `I` allocated index pages, `P_pin` pages retained only by captures,
`G` captured generations, and `Q_fetch` actual pack-page fetches. The target
one-byte monotone right-edge path is amortized `O(1)` index/slot operations
conditional on a cached right edge and no frozen-page copy. General WRITE is
`O(log_B F + log_B D + log_B E_f + K + input bytes + copied pages)`; capture
is a bounded tuple pin and tail seal. Commit targets
`O(D + changed names + dirty extents + final changed runs + replacement bytes
+ Q_fetch)` plus actual freeze, construction and cleanup. Logical unlink is
indexed; physical reclamation pays for pages and references moved or released.
Report operation counts as well as durations; a failed count bound is a
failure even when a timer happens to pass.

Physical active charge is the actual `st_blocks * 512` sum of every pack and
index page, including inactive tail, candidates, dead-slot slack and pins,
plus legacy payload/metadata and ownership charges. The target after bounded
compaction is `J = O(ceil(L / slots_per_page) + P_pin + G + 1)`; the `+1` is
the inactive candidate. For the one-file, one-generation 4,096 separated
one-byte WRITE checkpoint, **all** charged private backing has a prospective
design budget of at most 3 MiB. This is a target against the observed 17.88
MiB old-format checkpoint, not a measurement. Separate evidence must state
physical allocated bytes for 128 one-byte files, 4,097 repeated overwrites,
and up to 32 retained one-edit generations. Those cases have no inherited
3 MiB pass threshold; report their live, pinned, dead and relocated pages.

## Registered public evaluation

The old head is one closed, independently verified 10 MiB `data.bin` of `A`.
Every row gets an independent writable `shutil.copyfile` Store/history clone.
One SDK Mount, one generic Exec, one explicit Commit, full independent
old/new-head byte oracle, and clean close are mandatory. The C writer opens
one fd and makes one ordinary mounted `write` or `pwrite` per byte. Byte `i`
is `B + (i % 24)`. Case IDs are `issue273-{append,dispersed,repeated}-
{100,512,4097}-10m-v1`, run in that table's row-major order, with the frozen
control arm before the candidate arm for each case. Take one sample per case
per source arm. Append uses `O_APPEND`; dispersed
uses offset `(104729 + i*2654435761) % 10485760`; repeated uses offset
`5242880`. Expected lengths and every expected byte derive independently
from this schedule. Keep the old 8,194-byte #248 separated-offset 4,097 gate
as its own row and unchanged 25 s limit and FAIL history.

Add `issue273-clean-commit-v1` (no mutations after a retained old generation)
and `issue273-one-edit-commit-v1` (one new byte after a retained generation
containing 4,097 unrelated journal records). Their preparation and cleanup
remain visible, and neither may scan the unrelated journal. The retained
state's preparation has its own recorded command wall and is included in the
complete lifecycle wall; only the separate Commit timer isolates the quick
Commit question. The mandatory case registry is:

| Selection | Work | Complete command limit |
| --- | --- | ---: |
| `issue273-append-{100,512}-10m-v1` | 100 or 512 appended bytes | 15 s each |
| `issue273-append-4097-10m-v1` | 4,097 appended bytes | 25 s |
| `issue273-dispersed-{100,512}-10m-v1` | 100 or 512 separated `pwrite`s | 15 s each |
| `issue273-dispersed-4097-10m-v1` | 4,097 separated `pwrite`s | 25 s |
| `issue273-repeated-{100,512}-10m-v1` | 100 or 512 same-offset `pwrite`s | 15 s each |
| `issue273-repeated-4097-10m-v1` | 4,097 same-offset `pwrite`s | 25 s |
| `issue273-clean-commit-v1` | Capture and Commit with zero new edits | 15 s |
| `issue273-one-edit-commit-v1` | One edit after an unrelated retained 4,097-record generation | 15 s |
| Original #248 `gate` | 4,097 writes to the separate 8,194-byte file | 25 s, unchanged |

The external correctness/space registry also includes
`issue273-many-file-128-v1` (128 one-byte files in one Workspace),
`issue273-multi-exec-v1` (three 100-write Execs before one Commit),
`issue273-g1-g2-v1` (three consecutive edited generations with an old reader
held across successors), `issue273-retained-32-v1` (one edit and pin in each of
32 generations), and `issue273-mutations-v1` (truncate/hole, rename/unlink,
alias and open-unlinked handle, quota refusal, failed/uncertain backing and
clean-close cases). Each test execution is capped at 60 s and reports the
full byte oracle, physical allocated blocks and cleanup; a missing capability
is a failed or unrun cell, never a silently omitted one.

Record actual FUSE callbacks, Service
calls, Exec/Commit/freezing/cleanup walls, pack/index reads and writes,
`Q_fetch`, live/dead/pinned pages, physical `st_blocks`, Host quota charge,
memory/cgroup domains, and any interference. A complete performance command
has a 15 s limit. The three 4,097 matrix cases and the separate #248 gate
are prospectively named 25 s exceptions; every other case remains 15 s.
The separate verifier has a 9 s limit. An explicitly causal diagnostic
may use 60 s but cannot replace a gate row. Every test execution is capped at
60 s. One construction worker remains the default except namespace Init.

Before each timed arm, invalidate and check whole Store/history clone source
residency on macOS with the existing Darwin mmap/mincore cold mechanism,
recording file identities, page counts and the launch gap; apply the same
procedure to control and candidate. Do not prime mounted data, use recent
write pages, or pool warm and cold rows. If either arm cannot prove the
declared state, both numeric speed rows are `INELIGIBLE`, even if their
functional oracle passes. Image/runtime cache and competing-work evidence
must also match before a general 2x claim. The one-file 3 MiB target and Big O
counts are separate from latency eligibility. Retain every FAIL,
INCOMPLETE, INELIGIBLE and NOT_RUN receipt in a fresh path, with exact source,
product, compilation, dependency, image, harness and workload identities.
Never repeat an unchanged arm to replace a number.
`NOT_RUN` names the registered case, source arm, measured preparation or
attempt wall when one exists, exact failed prerequisite or time bound, and
the missing verifier scope. A row that starts and misses its bound is `FAIL`,
not `NOT_RUN`; a cache mismatch is `INELIGIBLE`. The candidate cannot close
#273 until the mandatory cells and all correctness, custody, and resource
gates have accountable results.
