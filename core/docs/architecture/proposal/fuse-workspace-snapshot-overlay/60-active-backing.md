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
tiny file gets its own root page. `backing/active/index.rs` stages copy-on-write index pages
and publishes one current root. A capture pins a root/revision in constant
index-state work and advances the generation; a read view pins the same
current root without advancing it. Retired index and pack pages are held by
pins whose revisions fall between each page's birth and retirement revisions.
The 32 capture limit and 128 possible directory-handle pins are charged in
the index owner. Captures require explicit release; a dropped read view
releases its pin and retains a stop/error state if cleanup fails. The currently implemented generic index
does **not** yet supply the specialized hot-right-edge update or a streaming
extent cursor for writes larger than a tiny slot. Its bounded 128-row scan and
predecessor lookup cover all extent starts within one 128-byte tiny write,
including replacement of 128 one-byte extents.

`backing/active/pack.rs` writes tiny records into a Workspace-shared logical
tail. A slot uses a stable logical page ID and ordinal. A candidate physical
tail page is complete and validated before the caller can publish the pooled
`P` locator for that logical page. The old physical tail remains owned until
the caller proves it is unpinned and calls release. The current writer creates
a new page file for each candidate. `backing/active/extents.rs` stores final
nonoverlapping Base/Zero/Packed intervals and inverse `R` references. A tiny
splice updates the overlapping extents and their inverse references together.
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
