# Issue 271: Workspace-scoped active head and packed journal

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.

This is the detailed architecture candidate behind the [active-head
proposal](PROPOSAL-ACTIVE-HEAD.md). The [causal ledger](CAUSAL-DIAGNOSTIC-LEDGER.md)
pins its evidence, including the first `INCOMPLETE` cause row, the corrected
cause-complete row, and the unchanged 25 s public gate FAIL. Neither format
nor speedup is implemented. Source-derived counts and the raw durations below
are not cache-qualified comparative latency evidence.

## 1. The actual replacement boundary

Today one mounted WRITE acquires a private payload, copies the affected extent
path and keyed inode/root path, charges their owner edges, publishes a new
immutable `RootOwner`, and completes checked invalidation before replying.
The next acquisition may reclaim that old root. The path is in
[`filesystem/write.rs`](../../../crates/layerfs-workspace/src/filesystem/write.rs),
[`backing/metadata_pieces.rs`](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs),
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
                   Workspace (id, incarnation, quota)
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
page; later files may use its remaining slots. Per-slot authentication and a
generation boundary must keep old slots verifiable when a tail page is
extended, closed or pinned. A frozen generation closes its current tail before
the next generation appends.

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

Definitions: `W` total accepted WRITEs, `F` files, `E_f` extents of file `f`,
`E_D` the total extents in files dirty at capture, `R_D` their changed runs,
`S_D` their replacement bytes, `P_D` private Local payload files, `Q_fetch`
actual packed-page fetches during ordered streaming, `D` dirty identities,
`K_i` extents overlapped by WRITE `i`, `B` index fanout, `Delta` total
ownership-edge and reclamation work, and `r` packed bytes per tiny record.
Big O below abstracts fixed 4 KiB page sizes and does not turn
the observed 4,097-WRITE curve into a global theorem.

| Work | Current immutable publication | Proposed active generation |
| --- | --- | --- |
| One narrow WRITE | Tree paths `O(log_B E_f + log_B M)` plus charged/reclaimed edges, where `M` is keyed metadata size. | Pack append plus disk inode, dirty-index and extent lookup `O(log_B F + log_B D + log_B E_f + K_i)` in general. A cached hot inode already dirty in this generation and an increasing-offset right edge target amortized `O(1)` page updates. |
| `W` increasing-offset WRITEs to one hot file | Structural `O(W log_B W + Delta)`; measured ownership cost is large but no global `O(W^2)` proof exists. | Target `O(W)` journal/index updates, with occasional bounded page splits. FUSE still handles `Theta(W)` callbacks. |
| Logical unlink/delete | Copies affected namespace/keyed paths and later releases the file's owned references. | Target indexed namespace tombstone/update, then charged reclamation of pages/slots no generation or open handle retains. Logical publication can be small; total cleanup remains proportional to what becomes unreferenced. |
| Commit one captured generation | `O(E_D + R_D + S_D + P_D)` plus namespace changes; one Local private-file open/read per relevant payload. | `O(E_D + R_D + S_D + Q_fetch)` plus the actual freeze, construction, compaction and cleanup work. It must traverse dirty files, not every Workspace file. Across many Commits, sum these costs for every captured generation. |
| Charged backing | For this tiny shape, `O(4096W + sum E_f + pinned deltas)` bytes. | Without compaction, `O(rW + sum E_f + pinned unique pages)` for cumulative writes. A live-space target needs charged bounded compaction of dead slots and page-level fragmentation. |
| Resident memory | Per-payload registry records grow with retained payloads; I/O windows are bounded. | Target bounded charged page cache and pack tail plus live handles; the inode map and extent indexes live on disk. No uncharged `O(F)` root cache. |

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

## 6. Space calculation and time sensitivity

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
These are within-run attribution, not cache-qualified comparisons. Define
`f` as the fraction of that pool removed and `C_new` as **all** new pack,
index, authentication, freeze, compaction and cleanup time. Then a planning
sensitivity is:

```text
T_new_complete ~= 31.391 s - f * 25.765 s + C_new
```

`C_new` has not been measured. The previously discussed **2 s** was an
illustrative value for this whole term, **not** an estimate of index time;
40–60% pool removal was likewise a sensitivity assumption, not a forecast.
For example, removing 50% of the pool gives 18.508 s with `C_new=0`,
20.508 s with `C_new=2 s`, and 23.508 s with `C_new=5 s`. With zero new
cost, the unchanged 25 s complete-command gate needs more than 24.8% pool
reduction and 2x raw Exec needs more than 51.4%. Any positive new cost raises
those thresholds. No value in this section is a predicted performance PASS;
future comparisons require declared equal cache state and one sealed public
sample per arm.

## 7. Source seams, tests and decisions before implementation

The new format would live in focused modules under
[`backing/`](../../../crates/layerfs-workspace/src/backing/mod.rs): an active
generation/transaction owner, packed journal, and disk inode/extent index.
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

Before product code, decide and write the record/index bytes, per-slot versus
per-page authentication, physical quota charge/refund rule, failed-write
progress and quarantine, snapshot pinning, bounded cache eviction, pack-page
compaction trigger, old-format access, and whether concurrent active
Workspaces require a separate daemon-control project. Existing limits include
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
