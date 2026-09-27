# Issue 271: Workspace-scoped active head and packed journal

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.

This is the detailed architecture candidate behind the [active-head
proposal](PROPOSAL-ACTIVE-HEAD.md). The [causal ledger](CAUSAL-DIAGNOSTIC-LEDGER.md)
pins its evidence, including the first `INCOMPLETE` cause row, the corrected
cause-complete row, and the unchanged 25 s public gate FAIL. Neither format
nor speedup is implemented. Source-derived counts and the raw durations below
are not cache-qualified comparative latency evidence.
Implementation and proof are tracked in [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273),
a true sub-issue of [#271](https://github.com/Ephemeral-AI-Lab/layerfs/issues/271).

## 1. The actual replacement boundary

Today one mounted WRITE acquires a private payload, copies the affected extent
path and keyed inode/root path, charges their owner edges, publishes a new
immutable `RootOwner`, and completes checked invalidation before replying.
The next acquisition may reclaim that old root. The path is in
[`filesystem/write.rs`](../../../crates/layerfs-workspace/src/filesystem/write.rs),
[`backing/binary_plus_tree/extent/splice.rs`](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs),
[`backing/ownership.rs`](../../../crates/layerfs-workspace/src/backing/ownership.rs)
and [`backing/metadata_reclaim.rs`](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs).
Commit's [`capture_submission`](../../../crates/layerfs-workspace/src/overlay/snapshot.rs)
pins a `RootOwner`; its dirty walk and file lowering read that frozen tree.
Replacing only the extent splice would leave keyed inode publication and
old-root reclaim on the WRITE path. The new active head therefore owns current
inode attributes, dirty membership **and** file extents until capture.

```text
BEFORE, each WRITE                         PROPOSED, current generation
------------------                         ----------------------------
pwrite                                      pwrite
  -> private 4 KiB payload file               -> authenticated packed slot
  -> copy extent-tree path                     -> update active extent index
  -> copy keyed inode/root path                -> update active inode record
  -> charge ownership edges                   -> publish one revision
  -> publish immutable root                   -> checked notify, reply
  -> checked notify, reply
next WRITE reclaims eligible old root      capture pins index + pack watermark
Commit opens each private payload          Commit walks packed records in order
```

## 2. One active generation belongs to one Workspace

The active head survives successive Exec commands. Files share its packed
journal and allocator, while each file has its own extent-index root. A
disk-backed inode map avoids keeping one page cache entry or open descriptor
per file. A dirty-inode index makes capture visit changed files, not every
file in the Workspace. All windows, index cache pages, journal tail bytes and
allocator state are charged to that Workspace under the Host budget.

```text
            Workspace (id, incarnation; Host-shared quota)
      +-------------------------------------------------------+
      | frozen namespace/root G0: Arc<RootOwner>             |
      | active generation A: (generation, revision)          |
      | namespace delta + dirty frontier + identity state   |
      |                                                       |
      |   inode map on disk: inode -> HotInode                 |
      |       HotInode = (length, attributes, revision,       |
      |                   dirty, inline extents | index root) |
      |            |                                          |
      |            v                                          |
      |   ordered extent B+tree:                              |
      |       branch = (upper_bound, child_page)              |
      |       leaf   = ([start,end), Base | Zero | Packed)     |
      |                                           |           |
      |   one Workspace packed journal <---------+           |
      |       4 KiB page -> many authenticated slots         |
      |       slot = (inode, revision, offset, length, data,  |
      |               authentication)                         |
      |   dirty-inode index                                   |
      |   bounded charged page cache + pack tail             |
      +-------------------------------------------------------+
```

### Physical placement and removal

The existing [`WorkspaceConfig.root`](../../../crates/layerfs-workspace/src/types.rs)
is a Host path. Attachment gives each Workspace one private directory at
`<root>/private-backing/<workspace-id>/`, separate from its mounted
`<root>/workspace/<workspace-id>/` path. Today that private directory holds
`p-*` payload segments, `m-page-*` metadata pages and `m-ledger-*` ownership
files. The proposed files belong **inside the same Workspace-owned private
directory**, not in the mounted file tree, the Store, or a directory per file
or Exec:

```text
<root>/private-backing/
  <workspace-id>/                 existing private directory
    p-* / m-page-* / m-ledger-*    existing formats retained where needed
    a-pack-*                      proposed shared packed-page segments
    a-index-*                     proposed pooled inode/namespace/extent pages
```

`a-pack-*` and `a-index-*` are illustrative names, **not a frozen format**.
Their versioned headers, page identities and authenticated slot validation
must bind the Workspace incarnation, page reuse epoch, inode,
generation/revision and logical range so a reused slot cannot alias old
bytes. Follow the existing
[`Directory`](../../../crates/layerfs-workspace/src/backing/directory.rs)
and [`segments`](../../../crates/layerfs-workspace/src/backing/segments.rs)
ownership, no-symlink and aligned-I/O checks. The current large-payload
`segments::allocate` accepts only a new segment and is not, unchanged, a
growing packed-file allocator. The active format needs a proved page-growth
and cleanup rule. A partial or uncertain rewrite of a shared tail must not
damage earlier acknowledged slots: use page copy/dual buffering or prove an
equivalent publication protocol. Allocate 4 KiB pages only as needed;
preallocating a large mostly empty segment per Workspace or file would
defeat quick-edit space efficiency. Attribute actual allocated blocks,
reservations and bounded resident windows to this Workspace, with admission
against the **Host-wide** disk and memory budgets. The current product has
no independent per-Workspace disk quota.

Unlink first removes a name from the current namespace. An open handle or
frozen generation may still own the content. A pack page is refundable only
when **all** its slots are unreferenced; a partly live page needs charged
compaction that copies surviving slots, publishes their new locations and
waits for old pins before releasing its blocks. Direct `(page, slot)` extent
references make relocation update every referring extent; stable slot IDs
instead add an indexed lookup. Freeze that choice in the format contract.
Index pages obey the same pin rule. A quota refund follows **physical** block
release, not a logical tombstone: the existing segment API can unlink a whole
file but cannot punch a page-sized hole or shrink a retained file. A new
physical release method or segment-level compaction must prove exact refunds.
`close_clean` must release active/frozen pins, reclaim or retain failed files
with exact accounting, and remove pack/index files **before**
[`Directory::close`](../../../crates/layerfs-workspace/src/backing/directory.rs)
removes the empty private directory. A failed or uncertain unlink retains
custody for inspection and cannot report a clean close. These files are
temporary Workspace backing; the design adds no `fsync` durability promise.

For three one-byte writes of `X` at offsets 0, 2 and 4 into the original
8,194-byte `A` file, one file's ordered leaf view is:

```text
logical range  source
[0,1)          Packed(page P0, slot 0) -> X
[1,2)          Base(offset 1)         -> A
[2,3)          Packed(page P0, slot 1) -> X
[3,4)          Base(offset 3)         -> A
[4,5)          Packed(page P0, slot 2) -> X
[5,8194)       Base(offset 5)         -> remaining original bytes
```

The inode map pools short inline extent lists and root descriptors on shared
pages; a separate 4 KiB index-root page for every one-write file would lose
the many-file space advantage. The slot width, authentication grammar and
index record width need a versioned
format specification. A 64- or 128-byte slot is an **illustrative design
choice**, not an existing encoding. A first slot reserves a physical pack
page; later files may use its remaining slots. Per-slot authentication or an
immutable authenticated page, with a generation boundary, must keep old
slots verifiable when a tail page is extended, closed or pinned. A frozen
generation closes its current tail before the next generation appends.

## 3. Operation flow and publication invariant

```text
WRITE:
  FUSE callback -> validate handle/offset -> reserve quota and page windows
      -> append authenticated record -> change affected extent intervals
      -> update HotInode attributes/dirty/revision
      -> publish one consistent revision -> checked invalidation -> reply

READ:
  select (generation, inode revision, index root, pack watermark)
      -> seek extent in O(log_B E_file)
      -> Base: read captured canonical source
         Zero: fill bytes
         Packed: authenticate referenced slot and return bytes

CAPTURE / COMMIT:
  seal pack tail and pin (root(s), watermark, dirty-inode index)
      -> advance to a new active generation with page copying on first touch
      -> lower only captured dirty files via ordered extent cursors
      -> existing bounded SaveFile descriptor/replacement stream
      -> reconcile result while old G1/G2 readers retain their exact bytes
      -> reclaim and refund unpinned pages inside accountable work
```

No WRITE replies before its new bytes, length and attributes are visible in
one revision. An index or pack failure **before publication** rolls back or
quarantines the candidate, with exact quota state. A checked invalidation
failure **after publication** retains the published bytes and the existing
typed coherence-failure receipt; it does not roll the revision back.
Uncertain outcomes are never blindly replayed. Aliases, open-unlinked
handles, concurrent readers, append, truncate, sparse holes and zero-fill
must resolve against the same selected revision. Regular file handles select
the live inode serial, while directory handles retain their opened namespace
view. A read pins its selected index root and pack watermark without holding
a metadata writer gate across remote reads. A capture atomically pins one
Workspace tuple: namespace delta, inode map, dirty frontier, pack watermark,
generation and revision. It cannot combine file bytes and names from
different revisions or turn inherited G1/G2 bytes into a mutable alias.
The current Workspace backing
provides no `fsync`/`fdatasync` durability claim; the new format cannot imply
one. A versioned format must read old roots explicitly and reject unsupported
required capabilities rather than silently falling back.

For the increasing-offset one-file shape, a bounded right-edge index path
could make appends amortized constant work. Arbitrary overlapping writes
still search the tree and rewrite `K` affected extents. An index root and
pack watermark can be pinned without a full extent rebuild **only if**
every published index page and pack slot is already complete. Closing a pack
tail, any frozen-page copy, dirty-file lowering, compaction and cleanup must
be charged to their actual measured phases; they cannot be moved into setup.
The current capture also owns fresh, declared, unbound and carried identities;
the active dirty index cannot replace those namespace and handle obligations.

The word **edit** covers different public filesystem routes. POSIX `pwrite`
writes at an explicit offset without advancing the file descriptor's current
position; the diagnostic makes 4,097 separate one-byte calls at even offsets.
An in-place `pwrite` overwrites an existing range: append its replacement bytes, then
splice `K` overlapping extents in that file's active index, retaining older
pack slots for pinned generations. This costs `O(log_B E_f + K)` rather than
the monotone right-edge target. Append selects live EOF and can take the
right-edge path. Truncate updates the indexed length and zero/hole rules and
must release or retain old slots correctly. Many editors save by writing a
temporary file and renaming it over the old name; those sequential writes can
use the hot path, but the final rename is a namespace/identity transaction.
An open handle to the replaced inode and a frozen generation still read its
old bytes. The retired SDK range-edit family is not a substitute for any of
these public FUSE operations.

| Mutation | Plausible benefit from an active Workspace head | Remaining cost/risk |
| --- | --- | --- |
| Append or increasing-offset WRITE | Avoid per-WRITE immutable root creation and the next WRITE's old-root reclaim; pack tiny bytes. | One FUSE callback per syscall, charged pack/index publication and checked invalidation remain. |
| Dispersed in-place edit | Avoid the same root publication/reclaim cycle. | Cold inode/extent index seeks and `K` affected extents; no right-edge `O(1)` claim. |
| Repeated overwrite | Avoid repeated immutable root churn even though the final file has one changed run. | Old packed slots become dead; bounded, charged compaction is decisive for space and speed. |
| Unlink/delete | Publish a namespace tombstone in the same active generation without copying the whole current root, if the namespace delta is implemented. | Physical release of unpinned pack/index pages and exact quota refunds still cost work; pinned generations and open handles retain bytes. |

### Preserve localized edits; keep the workload public

There are two different existing edit mechanisms. A WRITE currently uses the
Workspace's immutable extent splice in
[`backing/binary_plus_tree/extent/splice.rs`](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs),
then publishes a copied keyed root. That physical per-WRITE mechanism is the
work to retire. Its logical Base/Zero/replacement interval semantics stay.
At Commit, [`commit/save.rs`](../../../crates/layerfs-workspace/src/commit/save.rs)
lowers the frozen final file and sends `SaveFile` with ordered descriptors and
a replacement stream through
[`commit/upload.rs`](../../../crates/layerfs-workspace/src/commit/upload.rs)
and [`commit/source.rs`](../../../crates/layerfs-workspace/src/commit/source.rs).
For an existing base root, the Service uses C1
[`apply_edits`](../../../crates/layerfs-server/src/service/save/content.rs);
fresh content uses `construct_stream`. The proposed index must normalize its
**final** Base/Zero/Packed extents into that same validated wire format:
Packed maps to a Local descriptor whose replacement bytes come from the
authenticated slot. Pack offsets never enter the Bridge wire. Both descriptor
and replacement cursors must read the same pinned final view. Historical
journal records cannot be replayed as C1 edits: thousands of repeated writes
to one byte yield one final changed run, not thousands of ordered edits.
The Service/C1 localized construction remains unchanged.

A one-byte overwrite of the 10 MiB base keeps untouched prefix and suffix
as Base intervals and sends one replacement byte; Workspace does not
materialize the whole base file. Larger files may require more Base
descriptors under the extent-size limit, and C1 still pays for affected
content chunks and metadata. Prove locality with old-payload-read,
replacement-byte and descriptor counts as base size grows; do not infer it
from one fast wall time.

The benchmark's [`write-separated.c`](../../../benchmark/fs-bench-pro/writers/write-separated.c)
is an application program, invoked by
[`write_patterns.py`](../../../benchmark/fs-bench-pro/write_patterns.py)
inside a generic Exec. The daemon runs the shell with the mounted Workspace
as its current directory in
[`execution.rs`](../../../crates/layerfs-daemon/src/execution.rs).
The program opens `data.bin` once and issues one ordinary `write` or `pwrite`
per requested byte; those calls enter the normal
[`FUSE write callback`](../../../crates/layerfs-fuse/src/adapter.rs).
It does not call Workspace, C1 or C2 APIs. The runner prepares an independent
clone and checks the public operation count and an external result oracle.
Progress printing is inside Exec time. This establishes an authentic
application-level mutation route, not a qualified latency comparison: the
retained diagnostics had uncontrolled cache and remain `INELIGIBLE` for speed.
The current Python selection is fixed at 100 writes; its 512/4,097
extensions require a new prospective contract and receipts.

## 4. Scaling dimensions and current product limit

| Dimension | Design and cost requirement | Risk to prove |
| --- | --- | --- |
| Many files in one Workspace | One Workspace pack combines tiny records from different inode serials. The disk inode map pools short extent lists/root descriptors on shared pages; the bounded cache evicts cold roots. Commit enumerates the dirty-inode index. | A pack or index-root page per file wastes at least one 4 KiB page per small file. A resident map or FD per file grows with file count. The current successor reconciliation also has a 128-dirty-inode ceiling; any wider claim requires an explicit bound/ruling. |
| Write, truncate and delete | Content edits change extent intervals and HotInode attributes. Unlink/rename change the same generation's namespace delta and identity state. A tombstone may publish removal without copying the entire current root. | The content index alone does not speed delete. Open-unlinked handles and G1/G2 must retain data. Physical pack/index reclamation and refunds cost at least the references released; bounded cleanup cannot be hidden after the measured operation. |
| Several Exec commands in one Workspace | Later Execs continue the same active generation until explicit capture/Commit; no rebuild at each command boundary. | Frequent Commits create more frozen generations and can shift cost into capture, traversal and reclaim. A mostly empty pack tail may be sealed once per generation. Measure a separate multi-Exec/multi-Commit workload. |
| Several Workspaces under one Host | One active head, pack namespace, quota ledger and short mutation lock per Workspace; Host budgets remain shared. | A global pack/index lock or uncharged per-Workspace cache makes one Workspace block or exhaust another. Current `MetadataHost` has one Host-wide writer gate and remote admission is shared. |
| Several active Workspaces in one sandbox daemon | **Not supported by the current control lifecycle.** It has one `selected` Workspace and one mount slot; `WorkspaceHost.max_count` is an attachment limit, not concurrent mounts. | A separate control/lifecycle change is required before claiming multi-Workspace sandbox throughput. |

The current single-selection boundary is in
[`layerfs-daemon/lifecycle.rs`](../../../crates/layerfs-daemon/src/lifecycle.rs)
and [`layerfs-daemon/control.rs`](../../../crates/layerfs-daemon/src/control.rs).
The Host-wide metadata gate and remote admission are in
[`backing/metadata.rs`](../../../crates/layerfs-workspace/src/backing/metadata.rs)
and [`runtime/host.rs`](../../../crates/layerfs-workspace/src/runtime/host.rs).
The proposed storage scope should allow future Workspace isolation, but it
does not itself add simultaneous daemon mounts or parallel upstream Commits.
The one-construction-worker rule remains in force.
The current successor reconciliation's 128-dirty-inode refusal is in
[`commit/reconcile.rs`](../../../crates/layerfs-workspace/src/commit/reconcile.rs);
the current metadata host also caps retained roots. These are product limits,
not a licence to omit large-file-count or frequent-generation cases.

## 5. Complexity: one-file shape and general Workspace

Definitions: `W` cumulative accepted tiny WRITEs, `F` indexed files, `N`
active namespace records, `E_f` extents of file `f`, `E = sum E_f`, `D`
dirty identities, `N_D` changed namespace records at capture, `E_D`
extents in dirty files, `R_D` their final changed
runs, `S_D` their replacement bytes, `P_D` private Local payload files,
`Q_fetch` **actual** packed-page fetches during ordered streaming, `K_i`
extents overlapped by WRITE `i`, `x_i` its input bytes, `C_i` page-copy work
caused by a frozen generation, `B` index fanout, and `Delta` cumulative old
ownership-edge/reclamation work. Big O abstracts fixed 4 KiB pages; the
observed 4,097-WRITE curve is not a global complexity proof.

| Work | Current immutable publication | Proposed active generation |
| --- | --- | --- |
| One WRITE | Tree paths `O(log_B E_f + log_B M)` plus charged/reclaimed edges, where `M` is keyed metadata size. | General indexed lookup and overlap update target `O(log_B F + log_B D + log_B E_f + K_i + x_i + C_i)`. A cached, already-dirty hot inode with an increasing-offset right edge and no frozen-page copy targets amortized `O(1 + x_i)` work. Every acknowledged tiny WRITE may still issue a full 4 KiB physical page write. |
| `W` increasing-offset WRITEs to one hot file | Structural `O(W log_B W + Delta)`; measured ownership cost is large but no global `O(W^2)` proof exists. | Target `O(W)` journal/index updates, with occasional bounded page splits. FUSE still handles `Theta(W)` callbacks. |
| Read `y` bytes crossing `T` extents | Indexed seek, then extent/output work. | `O(log_B F + log_B E_f + T + y + q)` where `q` counts actual packed-page fetches; remote Base reads retain their own cost. |
| Logical unlink/delete | Copies affected namespace/keyed paths and later releases the file's owned references. | Indexed tombstone target `O(log_B N + log_B F)` plus affected identity/link work. Physical reclaim is separately at least proportional to pages/slots released and survivors relocated. |
| Capture | Pins an immutable `RootOwner`. | Target `O(1)` root/watermark/dirty-frontier pin and one bounded tail seal, **only** if published pages are complete. Later first-touch copies pay `C_i` in the later mutation; no whole-index rebuild. |
| Commit one captured generation | `O(D + N_D + E_D + R_D + S_D + P_D)`; one Local private-file open/read per relevant payload. | `O(D + N_D + E_D + R_D + S_D + Q_fetch)` plus actual freeze, construction, compaction and cleanup. It traverses changed identities, not every Workspace file; frequent Commits sum this cost for every generation. |
| Physical reclamation | Releases unowned private payload and metadata files with charged edge walks. | With an indexed victim list, target `O(U + V + A + R)` logical/page work for `U` physical pages released, `V` live slots moved, `A` locator/extent references updated and `R` ownership references retired, plus actual bytes copied and I/O. No whole-journal scan or unmeasured cleanup. |
| Charged backing | For this separated tiny-write shape, `O(4096W + E + pinned deltas)` bytes. | Exactly account for full pack/index pages and legacy large payloads. Without compaction, dead records can make pack space `Theta(W)` even when the current file has one live changed byte. A conditional live-space target appears below. |
| Resident memory | Per-payload registry records grow with retained payloads; I/O windows are bounded. | Target bounded charged page cache and pack tail plus live handles; the inode map and extent indexes live on disk. No uncharged `O(F)` root cache. |

For physical space, let `b = 4096` bytes/page, `h` pack header bytes,
`r` slot bytes, `c = floor((b-h)/r)` slots/page, `J` allocated pack pages,
`I` allocated index pages, `L` live active slots, `G` frozen generations
with sealed tails, and `P_pin` unique **pack** pages retained only by older
views. The active-page charge is **`b(J+I)`**, plus existing large-payload,
ownership and bounded resident charges. At least `ceil(L/c)` pack pages are
needed for live active slots; actual `J` includes dead slots, partially
filled tails and pages retained by pins. The proposed live index has
`O(E + F + N + D)` records, plus `O(L)` locator entries if stable slot IDs
are selected. Physical `I` also includes page rounding, branches,
free-list/ownership records and uniquely pinned copies. A separate 4 KiB
root page for each tiny file would make the `F` term `4096F`, defeating the
many-file target.

Without compaction, one long generation can retain roughly `ceil(W/c)`
pack pages even if repeated overwrites leave `L = 1`. With a sealed tail
per generation, one retained one-edit generation can cost one 4 KiB page:
`G` such generations can occupy `4096G` pack bytes, not `rG`. The desired
**conditional** bound after proved compaction is `J = O(ceil(L/c) + P_pin +
G + C)`, where `C` is a fixed bound on pages simultaneously copied during
compaction; it requires a measured fragmentation trigger and physical release
mechanism. Across several Workspaces, sum each Workspace's pages and allow
at least one partial tail per active Workspace under the shared Host budget.
Neither this bound nor the `O(1)` hot WRITE target has been implemented.
The slot-location choice can add work to these time bounds: direct
`(page, slot)` extents avoid an extra read lookup but can make compaction
update many extent references; stable slot IDs need a disk-backed locator
lookup and update, potentially `O(log_B L)` on a cold path. The hot-path
`O(1)` target requires a bounded cached or append-ordered locator route if
that indirection is selected. Count locator fetches and updates before
claiming either bound.

The one-file monotone target does **not** imply constant time for a large
multi-file working set. With cold index roots, each WRITE pays disk lookup;
repeated capture may pay new page copies. Across many generations, total
Commit work is the sum of each generation's dirty-file traversal. The
observed four upstream Service calls apply only to the one-file fixture:
SaveFile is issued per dirty file. Packing distinct records into one page
reduces allocated space, **not automatically** page-fetch count or physical
write traffic. Interleaved files can make an ordered Commit fetch the same
page repeatedly; `Q_fetch` is actual fetches and may approach `R_D` unless
streaming order, bounded reuse or an explicitly charged spool coalesces them.
Each acknowledged tiny WRITE may still rewrite a full 4 KiB pack page.
Sharing limits duplication, but pinned generations can retain unique pages
and pack records until cleanup. Append-only dead slots can grow with total
writes even when the live view shrinks; fragmentation can leave one live tiny
record in a 4 KiB page. A compaction rule must bound and pay for that work
before claiming a live-space bound.

## 6. Space calculation and quick-Commit cost

The corrected [causal receipt](evidence/causal-v2/run/receipt.json) reports,
at accepted WRITE 4,096, **16,777,216 bytes** charged for 4,096 distinct
4 KiB payloads and **1,974,272 bytes** other allocated backing: **18,751,488
bytes (17.88 MiB)** total, excluding its separate reservation. Merely
changing the active index while retaining private payload files cannot save
more than the 1,974,272-byte non-payload portion, or 10.5%, even if that
portion vanished and no new index/journal bytes were added.

Illustration only: reserve 256 bytes of each 4 KiB pack page for its header.
Then a 64-byte slot fits 60 times/page: 4,096 records need 69 pages, or
**276 KiB**. A 128-byte slot fits 30 times/page: 137 pages, or **548 KiB**.
The existing 8,194 final extents at 32 bytes each have **about 256 KiB raw record
bytes** before new index headers, branches, ownership, dirty-inode entries
and pinned generations. Set **at most 3 MiB allocated backing** for this
single-generation fixture as a *design budget*, not an after measurement;
meeting it would reduce the observed 17.88 MiB by at least 83%. The format
specification must fix record widths, page headers, charging and slack before
claiming an exact bound. That budget leaves 2,796 KiB with 64-byte slots, or
2,524 KiB with 128-byte slots, for every other allocated structure. It is a
**one-file** target; many short files need pooled inode/index pages and their
own prospective space budget.

Raw diagnostic Exec was **26.497 s**, Commit **0.449 s**, complete command
**31.391 s**. The acquisition-metadata-maintenance and publication-core
children were **10.912 s + 14.853 s = 25.765 s** through WRITE 4,096.
These are within-run attribution, not cache-qualified comparisons. For any
candidate, the honest accounting identity is:

```text
candidate complete time = existing complete time
                        - measured old work actually eliminated
                        + new pack/index work inside Exec
                        + actual capture/Commit work
                        + actual charged reclamation/cleanup work
```

The previously discussed **2 s** was an arbitrary sensitivity input for the
*sum* of new work. It was not measured, was not an index estimate and is **not
a proposed fixed cost per Commit**. A clean or one-edit Commit must not scan
the entire journal, rebuild every file index or compact unrelated pack pages.
The design target is a bounded root/watermark pin plus tail sealing when
capture occurs; a captured dirty file still pays its actual ordered lowering
and SaveFile work. Reclamation pays for references actually released, under
a bounded charged policy, in the operation or complete-command phase that
requires it. It cannot be silently deferred past measurement or allowed to
grow without bound. Frequent quick Commits need their own measured case;
an `O(1)` pin does not make their Service/lifecycle or per-generation page
cost disappear.

Ignoring unknown new work, the unchanged 25 s complete-command bound needs
more than **6.391 s** removed from this raw diagnostic, and 2x raw Exec needs
more than **13.248 s** removed. New work raises those requirements. The
earlier 40–60% pool-removal range was likewise a scenario assumption, not a
forecast. No value in this section is a predicted performance PASS; future
comparisons require cache state declared and enforced equally in both arms
and one sealed public sample per case per arm. A byte-copy clone alone is
not a cold-cache contract.

## 7. Prospective public workload matrix

The existing 100-WRITE [three-pattern specification](../../../../docs/roadmap/0.1/0.1.7/issue261-three-pattern-100-spec.md)
uses one 10 MiB old `data.bin` and one fd per selection. The static
[`write-separated.c`](../../../benchmark/fs-bench-pro/writers/write-separated.c)
already accepts counts through 4,097. A **new**, prospectively committed
benchmark contract must extend its runner, expected manifests, source seals,
cache policy, oracle and budgets before any candidate sample; historical
100-WRITE receipts keep their original identities. The proposed matrix is:

| Public write pattern | 100 | 512 | 4,097 | Mechanism it stresses |
| --- | ---: | ---: | ---: | --- |
| Append (`write` on `O_APPEND`) | one row | one row | one row | Hot right-edge index, growing length and partially filled pack tail. |
| Dispersed one-byte overwrite (`pwrite` at the existing deterministic 10 MiB permutation offsets) | one row | one row | one row | Scattered index paths, many separated extents and pack-page fetch order. |
| Repeated one-byte overwrite (`pwrite` at offset 5 MiB) | one row | one row | one row | Dead journal slots, compaction/refund and old-generation retention despite one final changed run. |

All three tiers use the same closed, verified 10 MiB master, independent
writable byte-copy clones, one public Mount -> one generic Exec -> one
explicit Commit, actual callback counts, full old/new-head and byte oracle,
and clean close. A speed comparison needs one sample per case **per source
arm**, matched control and candidate identities except for the declared
product treatment, and a prospectively fixed arm order and completeness
rule. Cache state must be declared and enforced identically; a clone does
not establish cold cache. The historical uncontrolled-cache 100-WRITE
receipts are not numerical speed controls. The existing
8,194-byte **separated-offset** #248 4,097 gate is a *fourth, distinct*
workload and remains in the campaign; the three-pattern matrix cannot
replace or relabel its FAIL. The dispersed schedule selects distinct,
nonadjacent positions at all three listed counts, while the repeated case
ends with one changed byte. These source-derived expectations belong in the
new independent oracle, not in a product test hook.

Register separate **clean Commit** and **one-edit Commit** controls so a
constant freeze or compaction tax is observable. Include a prior retained
generation with substantial unrelated journal state in the one-edit control
to expose an accidental whole-journal scan; its preparation and cache state
must be declared and charged to their own operation and complete-command
boundaries. A small Commit must not walk unrelated files or all prior
journal records. Freeze both controls' fixtures, cache contracts, limits and
verifiers with the matrix. Keep ordinary 15 s complete commands; any new
25 s exception must be named prospectively and does not inherit the #248
exception or the 60 s diagnostic limit. Verification remains separate and
under 10 s, with one construction worker and append-only
PASS/FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN reporting. A cell that cannot fit
is reported `NOT_RUN` with measured wall and reason; do not enlarge its
timeout or warm selected data to make it pass.

## 8. Source seams, tests and decisions before implementation

The provisional minimum is one focused family in the existing
[`layerfs-workspace`](../../../crates/layerfs-workspace/src/lib.rs) crate;
create each file when its responsibility becomes real, rather than merging
unused scaffolding:

```text
core/crates/layerfs-workspace/
  src/backing/active/           NEW; private to one Workspace incarnation
    mod.rs                      declarations/delegation only
    generation.rs               revision, atomic publication, capture, pins
    pack.rs                     authenticated tiny slots and bounded readers
    pages.rs                    pooled physical pages, cache, quota charges
    keyed.rs                    inode, namespace and dirty indexes
    extents.rs                  ordered per-file intervals and right edge
    reclaim.rs                  pin-aware compaction, release and refunds
  src/runtime/                  existing state, attachment, lifecycle
  src/overlay/snapshot.rs       existing capture boundary
  src/filesystem/               existing public mutation/read paths
  src/commit/                   existing final-view SaveFile lowering
  tests/                        external public behavior proofs
core/benchmark/fs-bench-pro/    prospective workload and quick-Commit cases
```

The existing immutable extent codec/cursor and keyed reader remain useful
for frozen or old roots; the path-copying
[`extent/splice.rs`](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs)
and [`keyed/update.rs`](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/keyed/update.rs)
cannot become mutable active writers unchanged. Reuse current budgets,
verified directory handles and aligned I/O primitives where their contracts
fit, and keep the Bridge `SaveFile`, Service and C1 edit interfaces. A new
crate or broad FUSE adapter rewrite is not part of the plan; the adapter may
need only a small call-site change after the charged tiny-write staging seam
is decided.

Integration reaches [`runtime/state.rs`](../../../crates/layerfs-workspace/src/runtime/state.rs)
and [`overlay/snapshot.rs`](../../../crates/layerfs-workspace/src/overlay/snapshot.rs)
for version capture; [`filesystem/write.rs`](../../../crates/layerfs-workspace/src/filesystem/write.rs),
[`filesystem/read.rs`](../../../crates/layerfs-workspace/src/filesystem/read.rs),
[`filesystem/namespace_view.rs`](../../../crates/layerfs-workspace/src/filesystem/namespace_view.rs)
and resize/rename/remove/open paths for visible inode semantics; and
[`commit/lower.rs`](../../../crates/layerfs-workspace/src/commit/lower.rs),
[`commit/source.rs`](../../../crates/layerfs-workspace/src/commit/source.rs),
[`commit/upload.rs`](../../../crates/layerfs-workspace/src/commit/upload.rs)
and [`commit/reconcile.rs`](../../../crates/layerfs-workspace/src/commit/reconcile.rs)
for a frozen ordered stream. [`runtime/lifecycle.rs`](../../../crates/layerfs-workspace/src/runtime/lifecycle.rs)
must reject outstanding pack/index pins on clean close and refund them
exactly. Legacy immutable pages remain readable under an
explicit version rule. Keep product-only source below the Core file ceilings
and behavior tests external to `src/`.

Planning only: the current committed baseline is **59,621 Core** and
**65,417 reference** production LOC (**125,038 combined**). A complete first
switch may add roughly **2,500–4,500** active-backing lines and
**1,000–2,000** integration lines, then retire **200–700** old hot-path lines:
about **+3,000–6,000 net Core production LOC**, or **62,600–65,600 Core**.
Old `RootOwner`, ledger, immutable tree and private payload readers cannot
be counted as deletions at first switch because frozen/old roots, result
roots and large payloads may still use them. This range is not a staged LOC
comparison; every implementation commit must count its exact first-parent
and staged product source with `tools/production_loc.py`. The existing
`metadata.rs`, `ownership.rs`, `rename.rs`, extent `splice.rs` and FUSE
`adapter.rs` are near the 999-physical-line ceiling; put new behavior in
focused files, with `mod.rs` under its 200-line declaration limit.

Before product code, decide and write the record/index bytes, per-slot versus
per-page authentication, tail rewrite safety, page/segment growth and physical
release, direct slot reference versus stable locator, Host quota
charge/refund rule, failed-write progress and quarantine, snapshot pinning,
bounded cache eviction, pack-page compaction trigger, old-format access,
and whether concurrent active Workspaces require a separate daemon-control
project. Existing limits include
256 resident nodes and 128 handles in
[`runtime/state.rs`](../../../crates/layerfs-workspace/src/runtime/state.rs),
32 retained roots in
[`backing/metadata.rs`](../../../crates/layerfs-workspace/src/backing/metadata.rs),
and the 128-dirty-successor-inode refusal in Commit reconciliation. Define
the target supported file and generation counts rather than assuming these
limits vanish. Proof must include many files, in-place edits, append,
temp-file rename saves and deletion; several Execs before one Commit;
several Exec/Commit generations; read/truncate/holes, aliases and
open-unlinked handles; G1/G2 and failed/unknown outcomes; quota exhaustion;
exact close and cleanup. Existing ignored captured-generation cases do not
count as this proof. Use new external tests through the public SDK/FUSE route
and an independent old/new-head byte oracle. Register new benchmark
selections and gates before sampling; the original 25 s gate and
incomplete/cache-ineligible historical rows retain their status.

## 9. Implementation rollout and stop/go gates

These are review checkpoints for one replacement architecture, not claims
that a pack-only or index-only build improves Exec. Keep the current public
writer until a complete Workspace generation can take over. Do not add a
per-WRITE dual writer or a benchmark-only route.

| Checkpoint | Work and owning boundary | Evidence required to advance |
| --- | --- | --- |
| 0. Freeze contracts | Specify versioned `a-pack-*`/`a-index-*` page bytes under the existing Workspace private directory; slot/page authentication, safe tail publication, lazy growth, direct versus indirect locators, exact physical release, Host-quota refunds, frozen-page lifetime and supported file/generation counts. Freeze the §5 complexity targets and public workload/cache contract before samples. | Reviewed format and ownership rules, with a complete case registry, fixed budgets, arm order and `NOT_RUN` rules. No presumed per-Workspace disk quota or unchanged reuse of the large-payload allocator. |
| 1. Build Workspace storage | Implement one Workspace-owned pack, pooled disk inode/namespace/extent indexes, dirty frontier and bounded charged page cache in `backing/active/`; use verified directory and aligned-I/O primitives where valid. | External checks for authenticated append/read, overlap/split, partial tail failure, exact `st_blocks` charges, quota refusal, abandoned candidates and cleanup. No preallocated large empty segment, resident map, pack tail or root page per file. |
| 2. Integrate one active view | Select the new representation once per Workspace incarnation, covering WRITE/read, inode attributes, dirty membership and create/truncate/unlink/rename in one revisioned tuple. Preserve regular and directory handle semantics and checked invalidation. | Public mounted-view semantics and byte oracle for append, dispersed/repeated edits, temp-file rename, delete, aliases, holes, open-unlinked handles and failed/unknown outcomes. No per-WRITE immutable keyed-root publication on this route. |
| 3. Freeze and lower | Atomically pin namespace, inode/index roots, dirty frontier and pack watermark in `overlay/snapshot.rs`; preserve successor writes and reconciliation. Adapt `commit/{lower,upload,source,reconcile}.rs` to emit the existing final ordered `SaveFile` descriptors and replacement bytes. | G1/G2 and concurrent read/Commit proof; descriptor/replacement and saved-root parity for existing-base edits; C1 `apply_edits` and fresh-file `construct_stream` stay unchanged. Clean and one-edit Commit do not rebuild every index or scan prior journal history. |
| 4. Bound lifetime work | Complete pin-aware slot/page reclamation, compaction, actual block release, Host-quota refunds and clean-close accounting across several Execs, Commits, files and generations. | Track `J`, `I`, `L`, pinned pages, page slack, bytes relocated and allocated blocks; repeated overwrite and `G` retained one-edit generations exercise the §5 space bound. No refund before physical release; failed cleanup remains owned. The ≤3 MiB target applies only to the one-file 4,096-write fixture. |
| 5. Freeze and measure | Seal the complete candidate and run declared public control/candidate cells once per arm with symmetric enforced cache state, one construction worker, separate verification and append-only receipts. Attribute Exec, Commit, freeze, pack/index I/O and reclamation, plus callback and Service counts. | Correctness, custody, cleanup and resource gates pass; every registered cell has a result, including failures and `NOT_RUN`. Count right-edge updates, cold seeks, page copies, actual fetches and physical bytes to test the §5 bounds. The #248 25 s gate stays 25 s; any 2× claim needs its own cache-qualified matched evidence. |

Checkpoint 2 switches the whole mutable Workspace view, not an individual
file, Exec or syscall: namespace, identities, attributes, extents, dirty
frontier and pack watermark must share one revision. Checkpoints 3 and 4
may be developed alongside it, but the new route is not ready for default
selection or release until capture, Commit, retention and cleanup all pass.
Old frozen roots remain readable until their pins end. A format, workload or
cache-contract change after checkpoint 5 starts a new source or scenario
identity; earlier FAIL and `INELIGIBLE` evidence remains as recorded.
