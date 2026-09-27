# #273 active backing storage foundation

> Source pin: this revision follows typed-format contract `ba6ceeb30` and
> describes the source committed with this revision. No mounted
> Workspace selects it yet, and it has no latency or release claim. The
> prospective [format and evaluation contract](../../../issues/273/ACTIVE-FORMAT-AND-EVALUATION-v1.md)
> states the target behavior and remaining proof gates.

`backing/active/page.rs` encodes versioned 4 KiB pack/index pages with the
Workspace incarnation, page ID and epoch, generation/revision, used length,
record count and whole-page SHA-256. Decode checks the expected physical
identity and zero tail before returning body bytes. `backing/active/keyed.rs`
encodes sorted leaf and branch records; the final branch child also carries
its maximum key. `backing/active/records.rs` encodes 416-byte inode values
with four optional inline extents and 16-byte namespace bindings/tombstones,
plus fixed inode, namespace and generation-dirty keys. The same pooled index
holds these records beside extent, inverse-reference and locator records; no
tiny file gets its own root page. The `N|parent|name` key omits a length
prefix so names sort in the same byte order as canonical directory listings
and existing continuation cookies. `backing/active/index.rs` stages
copy-on-write index pages and publishes one current root. During a large
update it releases intermediate candidate pages as soon as a later staged
version replaces them; only the final candidate pages and original replaced
pages remain owned for publication. Its scratch grows under the Host memory
budget rather than a fixed update-count ceiling. A capture pins a root/revision
in constant index-state work and advances the generation; a read view pins
the same current root without advancing it. Retired index and pack pages are held by
pins whose revisions fall between each page's birth and retirement revisions.
The 32 capture limit and 128 possible directory-handle pins are charged in
the index owner. Captures require explicit release; a dropped read view
releases its pin and retains a stop/error state if cleanup fails. The generic
index does **not** yet supply the specialized hot-right-edge update. A
replacement walks affected extents in bounded 128-row index pages; its
working overlap set grows only within the Host memory budget. A 300-extent
overwrite tests an update exceeding the former 512-key batch ceiling.

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
active lock. Full orphan-payload reclamation, mounted selection and public
failure proof remain open.
When a large overwrite removes every selected slot on the current packed
tail, its locator and physical page stay owned for the next tiny append;
sealed fully dead pages can be released in the same publication. The external
cross-page overwrite test checks that tail transition.
The current active wrapper in `backing/active/generation.rs` publishes pack,
extent, locator, inode attributes and dirty membership through one index root
change for `write_tiny_file`. Its lower-level `write_tiny` still accepts
caller-supplied records for backing tests. Neither method is selected by the
public Workspace yet. A separate `publish_records` call publishes namespace,
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

Local-edit attachment now constructs one active owner for the Workspace
incarnation after acquiring the verified private directory. Failed attachment
closes that owner before the arena and directory; clean close drains it before
metadata and payload cleanup. Repeating the active owner's already-complete
close is safe when a later cleanup phase failed and the Workspace retries.
The mounted mutation and read methods still use the old root, so this
lifecycle ownership alone does not select the active view.

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
capturing a generation. These are backing tests. Public
FUSE WRITE and namespace operations, final-view SaveFile lowering, mixed-page
compaction and the #273 benchmark registry are still unproven and remain on
the old production route.
