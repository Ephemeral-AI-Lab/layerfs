# #273 active backing storage foundation

> Source pin: the implementation committed with this note, based on first
> parent `ad76e6740`. This describes the storage foundation only. No mounted
> Workspace selects it yet, and it has no latency or release claim. The
> prospective [format and evaluation contract](../../../issues/273/ACTIVE-FORMAT-AND-EVALUATION-v1.md)
> states the target behavior and remaining proof gates.

`backing/active/page.rs` encodes versioned 4 KiB pack/index pages with the
Workspace incarnation, page ID and epoch, generation/revision, used length,
record count and whole-page SHA-256. Decode checks the expected physical
identity and zero tail before returning body bytes. `backing/active/keyed.rs`
encodes sorted leaf and branch records; the final branch child also carries
its maximum key. `backing/active/index.rs` stages copy-on-write index pages
and publishes one current root. A capture pins a root/generation in constant
index-state work, while old page versions created before that capture stay
charged until its explicit release. The currently implemented generic index
does **not** yet supply the specialized hot-right-edge update or a bounded
streaming extent cursor.

`backing/active/pack.rs` writes tiny records into a Workspace-shared logical
tail. A slot uses a stable logical page ID and ordinal. A candidate physical
tail page is complete and validated before the caller can publish the pooled
`P` locator for that logical page. The old physical tail remains owned until
the caller proves it is unpinned and calls release. The current writer creates
a new page file for each candidate; reuse and compaction are still to be
implemented. No reader may infer that an unselected candidate is current.

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
owned Linux ext4 volume. This is a backing test. Public FUSE WRITE,
namespace operations, final-view SaveFile lowering, frozen pack lifetime,
partial-tail failure, compaction and the #273 benchmark registry are still
unproven and remain on the old production route.
