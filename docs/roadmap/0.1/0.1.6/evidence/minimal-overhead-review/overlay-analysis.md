# Live overlay representation: minimal-overhead review

Read-only analysis, 2026-09-14. No product edit, build, benchmark, stress test, or new PASS was produced for this review. The owner requires a fresh Workspace identity for an agent tool call; reusing a Workspace is not the proposed optimization. The numerical models below are record/page accounting, not timing predictions or a qualification of 100,000 changed files. Concurrent agents are editing the current implementation; source identities at review completion appear at the end.

## Recommendation

Preserve the installed `Arc<OverlayRoot>`, exact leased-source comparison, owned snapshot reader, and existing canonical construction pipeline. Replace the expensive representation underneath those interfaces. In order of confidence and importance:

1. Make an empty or small Workspace use **lazy private backing and a small, charged share of a host-wide page/owner cache**. Opening a Workspace must not preallocate maximum root/release queues, reserve its entire possible disk quota from the host, or open six backing files before it has bytes to spill. A new Workspace still has a new ID and independent root/publication context.
2. Pack records by bytes into immutable pages. Remove the seven-entry limit for short records. Store a singleton/compact file range directly with its inode; do not create a B-tree to hold one range. Preserve a weighted range tree only after the bounded inline representation is full. Pack the already identified 106-byte singleton canonical correspondence directly into its parent record; do not retain three independent pages per singleton file.
3. Reduce the inventory, not just its packing. Put the latest effective change sequence in its authoritative inode/binding row, and use subtree sequence summaries for changed-key discovery. Put the numeric directory cookie in the binding row. Keep the required cookie-order lookup. Consider an explicit singleton alias representation, with carefully defined promotion for real hardlinks and undiscovered canonical aliases.
4. Keep the existing Payload API and interval/reclamation implementation, but add compact tagged whole-source/tiny-source representations. A new one-byte ordinary write does not inherently require six indexed records and a private 4-KiB allocation. Stage this separately from the safe packing changes; failure-safe promotion and ordinary-spool accounting must be proved.
5. Batch all changes to an affected packed leaf/ownership page in one private mutation. Buffer mutable allocation/reference metadata. Dense pages alone can otherwise make the number of per-child reference syscalls worse.

The smallest credible first implementation is items 1–2 plus the cookie merge and atomic update batching. The change-summary replacement is a concrete, reviewable frozen-spec amendment, not an implementation detail to slip in silently. Tiny payload/alias specialization is a further focused change, not a premise for promising a 20-MiB total footprint.

## What the governing documents require, and what they merely selected

The rule requires one host authority, host-owned acknowledged bytes, immutable captured state, O(1) snapshot registration independent of file count, incremental mutation paths, bounded residency and queues, retained owners that survive eviction, and physical reclamation. The resulting current state must survive Commit without a freeze, remount, global checkpoint installation, or first-post-snapshot whole-map copy. A 64-MiB logical source with one retained byte must eventually retain at most the intersecting 4-KiB payload block plus explicitly charged metadata/read/recovery overhead.

The spec currently selects immutable published B-tree pages **even when only the live root owns them** (§2, page backing). Therefore an exclusive-page in-place fast path would require an explicit spec change. It must not be smuggled in under a refcount optimization. A RAM-resident immutable page with an evictable body is compatible with the host-buffer allowance; the rule does not require an ordinary mutation to perform a disk write or fsync.

The spec and V2 resolution explicitly select separate change-by-key and change-by-sequence indexes. Replacing them with primary-row sequence summaries requires updating those documents and the cursor/complexity contract. The external filesystem semantics can remain exact. Likewise the V2 document's suggestion of keeping existing `Node`/`PieceTree` value types is not proof that today's independently paged range nodes are necessary: the current implementation already introduced fixed typed records and a different range backing.

Unresolved generic writable-mapping V1 remains unresolved by every metadata optimization here. Host-owned ordinary acknowledgments are a separable mechanism, not a substitute for full kernel visibility.

## v0.1.5: useful reference, not a new speed claim

Read with `git show v0.1.5:<path>`, using tag commit `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`:

- `release-notes/0.1.5/benchmark-closeout.md` is a report of the #120 receipts, not a new run. Its measured candidate was `c55daf13…` at `1ff1f2ddd…`; the tag adds documentation/version changes. It preserves 56 fresh PASS, 125 WARN and one unrepaired performance FAIL, plus reused and optional rows. It must not be called an all-gates-pass baseline.
- Its SDK edit-plus-Commit examples include `overwrite-head-4k-on-100mib-ops-1` at 7.88 ms and the 500-MiB counterpart at 8.78 ms. These are historical whole selected calls, **not** Workspace-create latency or overlay-write latency. Historical v0.1.3 ratios use an undeclared cache profile and do not support a paired speedup claim for this design.
- `crates/layerfs-workspace-core/src/file_edit.rs` has a 64-byte `CompactSplice`, 72-byte `CompactSpoolSplice`, one-word logical charge for a whole spool range, and a shared treap for general edits. A single equal-length edit did not require three disk pages. The compact form does not erase all supporting `Node`, map, path, payload or retained-reader memory; its reported charge is not total RSS.
- `LiveWorkspace` used resident maps/sets and `Node.paths`, while `BackingOwner` also held exported facts/dirty state and retained segments. Those structures are unsuitable as an unbounded million-file snapshot representation. The old `FrozenWorkspaceChanges` borrowed those maps under the old lifecycle; that borrowing/freeze cannot be restored.
- `HostSpool::default()` starts with empty maps and no segment descriptor. The first append creates an unlinked shared segment (normally 1 MiB), and multiple logical files share it. Old segment-level retention, per-segment descriptors, and RAM registries must not be restored wholesale because the new bounded partial-retention and eviction contracts are stronger.

The correct target is to recover the old **small-input cost shape**—small fixed context, compact changed records, shared raw backing, no unnecessary disk work—while adding the new ownership/snapshot guarantees. The extra host acknowledgment dependency and generic kernel-coherence work still exist; page packing cannot promise that all old timings are attainable.

## Current costs traced to actual source

### Startup: charge, capacity reservation, allocation and I/O are different

`overlay_budget::components` at reviewed baseline `3f04d4146` computed a default fixed resident reservation of 5,846,016 B (5.575 MiB) before any edit. During this review, commit `ff7098929` correctly added the previously uncharged 8-MiB SnapshotReader demand-cache allowance: **the current fixed reservation is 14,234,624 B (13.575 MiB)**. This accounting correction must not be reverted or described as an actual new 8-MiB allocation; a minimal design must charge real cache growth and retained capacity correctly. It reserves 7,784,693,760 B (7.250 GiB) of possible temporary disk capacity and eight descriptors from aggregate admission. Those are reservations, not statements that that much physical disk or RAM was allocated. At the old baseline, the 128-MiB aggregate memory cap admitted only 22 such idle fixed reservations; at `ff7098929` it admits only **nine**. The unchanged 128-GiB disk reservation permits only 17. Operation/construction reservations lower those counts further. A canonical reader cache can be charged on growth or safely shared by immutable Store identity, but its allowance cannot simply disappear from accounting to make D0 look smaller.

`Budget::open` immediately creates the Payload private index, two payload arenas, and the main metadata index: **six empty files**, each opened then unlinked. Each Index pre-reserves 16,384 retired-root slots; Payload pre-reserves 8,192 release IDs, reader slots and writer slots. On the ordinary 64-bit layout assumption (`Option<u64>` 16 B, `u64` 8 B), the two root queues plus payload release queue alone reserve 589,824 B (576 KiB) of vector storage. This is a source-derived layout model, not an allocator/RSS measurement. It excludes Arc/control structures, allocation rounding and bounded traversal buffers.

`HostOverlay::new` resolves and imports only the immutable namespace root. It does **not** import a 100,000-entry namespace eagerly. It still writes the root inode and canonical-to-live mapping into the main Index. For a fully cleaned minimal one-leaf root this is a 4-KiB body allocation plus allocation-block rounding in the two distant catalog regions; a typical 4-KiB-block accounting model gives another 8 KiB for those regions. Initial transient pages and actual filesystem allocation must be measured separately. The reverse catalog is placed at `max_pages * 128`, so apparent sparse file length is especially misleading for a small Workspace.

### One logical range is currently a miniature database

`overlay_ranges::Ranges::node` constructs an independent Index root and performs:

1. `set(META, 64-byte Summary)`;
2. `set(PROVENANCE, 49-byte Piece metadata)`;
3. optional left child link;
4. optional right child link;
5. `set_owned(BACKING, locator/bytes, payload token)`.

A singleton payload range performs three path-copy leaf writes to leave **one 4-KiB leaf**. Its final three encoded entries use only 195 B: 64+49+8 value bytes, three one-byte keys, three 23-byte entry envelopes, and a two-byte node prefix. A base range has a 40-byte locator instead of eight; a tiny Inline range also leaves most of the page unused. The stored 64-byte subtree Summary is derivable for a singleton; height/count/priority/left length are representation bookkeeping, not unavoidable per-file state.

Every data page additionally uses two 64-byte ownership-header slots and two 64-byte reverse-location slots: 256 B/page before filesystem rounding/highwater residue. Thus 100,000 singleton range leaves alone account for 435,200,000 B (415.04 MiB) of packed page/catalog capacity. Three physical pieces for one splice can triple that contribution. No snapshot rule requires this layout.

### Low fanout and recursive ownership work

The main Index caps every leaf at seven records and every branch at eight children, despite a 3,968-byte body and keys often only 9–33 bytes. Its generic entry envelope is 23 B plus key/value bytes; two optional eight-byte references are always encoded. A 17-byte-key, 16-byte-value payload coverage row uses 56 B but receives at least one seventh of a page. This is very low density unrelated to the key's actual size.

Each copied page writes a full 3,968-byte body. It then retains every child/linked/external reference. Each internal reference increment/decrement reads the current header, and `put_header` reads the header again before writing a 64-byte alternate slot. Allocation also writes its ownership header and reverse location. External Payload retains cause **another COW update in the private Payload Index**. Root releases, ref releases and physical evacuation add further work. A page-write counter alone is not a syscall, byte-I/O or physical-allocation counter.

For an illustrative write to an already dirty, empty imported file, let `Ho` and `Hp` be actual root-to-leaf path lengths in the overlay and payload catalogs. The initial singleton range contributes three leaf-body writes; six payload setup rows and its metadata-page retain contribute at least `7*Hp` copied bodies; inode installation plus latest-key/sequence replacement contributes about `4*Ho`. This lower-path model is `3 + 7*Hp + 4*Ho` body writes before splits, source reads, ownership headers, obsolete-root reclamation and namespace creation. At `Ho=Hp=7` it is 80 bodies / 317,440 B for a one-byte replacement. This is **not a measured average**, and actual paths, coalescing or new-file setup can differ. It explains why fewer syscalls and fewer independent transactions matter beyond a disk-footprint change.

Dense packing without cached/batched ownership updates could increase leaf-child retain count from seven to dozens. The fix must include buffer/transaction batching, not simply change `MAX_KEYS`.

### Payload: six records plus 4-KiB padding for a fresh tiny source

`write_classified` creates SOURCE, TOKEN, LOCATION, PHYSICAL, COVER and COVER_REVERSE records. All keys are 17 B. The values are respectively 24, 40, 24, 24, 16 and 16 B, totaling **384 encoded bytes per fresh source including current envelopes**. Then a metadata leaf independently retains its token. The disk-token versus resident-handle admission repair was committed during this review as `ca46e2793`; it fixes the accidental resident-token ceiling and missing/starved catalog reclamation, but does not eliminate this representation cost.

The allocator currently starts each `write_from` source at `aligned(len)`, using 4-KiB alignment/padding. Distinct one-byte new-file writes therefore reserve 4096 B each rather than sharing the same physical block. Large-source interval sharing is useful and proven by earlier component tests; it is not a reason to force every fresh tiny source into the general six-row form.

COVER_REVERSE originally supported predecessor lookup through reversed keys. The Index now has an exact `floor` operation. A single forward cover index can supply predecessor and successor queries; removing its reverse copy is plausible, but every interval insertion/removal, normalization and recovery path must migrate together. PHYSICAL remains needed for bounded tail evacuation unless the allocator supplies another exact bounded reverse lookup. SOURCE/TOKEN/LOCATION can be combined for the common whole-source case; arbitrary partial retention still requires equivalent information.

### Namespace and change duplication

For one new single-link file with a 16-byte name, excluding the parent/root's amortized constant rows and kernel pins, current logical state has nine main-index records:

| Row | Encoded bytes in current generic format |
|---|---:|
| Inode, key 9 / value 128 | 160 |
| Binding, key 25 / value 8 | 56 |
| Reverse binding, key 33 / empty value | 56 |
| Cookie name, key 25 / value 8 | 56 |
| Cookie order, key 17 / value 24 | 64 |
| Inode latest-change and sequence-change | 41 + 41 |
| Binding latest-change and sequence-change | 57 + 57 |
| **Total** | **588** |

The inode's 128-byte representation includes 64 B for optional canonical identity and base root even when a new inode has neither. A compact typed encoder can omit absent fields. This is a moderate byte saving, not the principal explanation for hundreds of megabytes.

`Description::build` creates offset and origin trees; `Description::persist` adds a header tree. A one-descriptor nonzero file leaves three separate pages before the outer correspondence maps. The construction agent's direct 106-byte singleton value fits the existing 128-byte inline limit and removes these three pages without altering lineage semantics. This should be an immediate first implementation change. For fragmented descriptions, globally packed typed `(inode,offset)` and `(inode,origin,start)` records, or a bounded inline description followed by packed overflow trees, avoid per-file empty/singleton trees. Preserve exact predecessor lookup; a full scan of the old description is not an acceptable substitute.

## Reducing the inventory while preserving behavior

### Change discovery through primary-tree summaries

Store `latest_effect_seq` in the authoritative inode/binding row and deletion marker. Store `max_effect_seq` per child/subtree. A captured cursor traverses only subtrees with `max_effect_seq > covered_sequence`, yields keys in **primary key order**, and filters leaf records against the same captured cutoff. Its resume token is the immutable primary key, not a sequence-index key. The owned captured root prevents a later edit from changing the cursor's results. The candidate already deduplicates nodes/bindings and does not semantically require sequence ordering.

This is output-sensitive traversal: at most the union of the ancestor paths leading to qualifying leaves plus those leaves. For one post-C1 edit it follows the new path instead of scanning all C1 files. A subtree maximum taken from a newer **live** root would be wrong; it must be part of the immutable captured root graph and updated during private preparation. Atomic rename updates both keys and all summaries before root installation. Checked sequence arithmetic and exact leased-root comparison remain unchanged.

Do not clear every live row's sequence after Commit. Old sequence fields are fixed-size current metadata, not historical operations. Once coverage advances, maxima allow those subtrees to be skipped. This removes the current need to delete two change-index entries for every covered live key.

Deletion is the main counterexample: physically erasing the primary row can erase its only deletion effect. Retain a compact deletion marker until its effect is covered, or prove that the surviving parent binding and canonical reference cascade completely subsume that particular inode marker. Base-backed binding removals must remain as masks while the live base is old, even after coverage. A create/remove during retained C1 cannot be assumed irrelevant to C2. The already tested absent-parent cleanup proof remains valid for child effects only when that parent is absent; preserve the surviving-parent removal.

`commit_maintenance` currently uses `take_while(seq <= covered)` and inspects the first sequence-sorted row. Those assumptions must be removed. For removable deletion markers, use a charged disk retirement queue or subtree `min_retirable_seq`/count summaries. A plain maximum cannot find old removable tombstones efficiently when newer keys occur in every subtree. Do not turn each maintenance pass into a full primary scan. No queue entry may refer to a reusable inode ID without an exact generation identity.

This changes selected index/cursor/complexity contracts; document the amendment before implementation. It preserves C1/C2 contents and atomicity if the above conditions are met. Adding one sequence per row while retaining both old change indexes would miss the main benefit.

### Directory cookies and aliases

Put the cookie next to `(parent,name)->inode/mask`. The primary name lookup then supplies the cookie, so COOKIE_NAME is redundant. Retain `(parent,cookie)->name` for numeric resume and the per-directory highwater/import state. Rename/removal must update both directions atomically. No resurrection of a removed cookie and no unbounded archive of removed names is needed. Imported cold bindings still get exact independent cookies for hardlink names.

A singleton alias can reside with its inode as an explicit `None | One(parent,name) | Many(reverse_root)` representation. On adding the second known link, privately create two reverse entries and atomically switch to Many. The namespace binding is still authoritative; the alias representation supplies inverse lookup. A long name is bounded by the existing 255-byte limit; packed variable-size rows must handle that without one overflow page per ordinary short-name inode. Deleting an alias or demoting Many must be bounded and preserve open-unlinked state.

**Do not equate one imported name with one canonical link.** A lazily discovered base inode may have undiscovered aliases. A canonical link count greater than one must use an `IncompleteCanonical/Many` state retaining canonical lookup semantics; it cannot be treated as a complete singleton or force eager namespace enumeration. If the current SDK invalidation/parent-walk consumers cannot use that incomplete state safely, retain the compact reverse index initially. Hardlink discovery and live/canonical identity are more important than eliminating one short row. Deterministically truncating a canonical ID into NodeId is not an alternative to exact canonical-to-live mapping.

## A concrete RAM-first spill transition

Using an ordinary persistent `Arc<BTreeMap>` or treap whose Arc graph permanently owns every node in RAM is not sufficient. Snapshot registration must retain logical graph ownership without pinning every body. Neither Commit nor the first threshold crossing may serialize the whole graph.

Use the existing PageId/Root abstraction, exact header receipts, source-root install and bounded reclaim machinery with **two locations for a page body**: charged resident buffer or disk slot. Ref/location entries are allocator state, separate from immutable logical bodies. A bounded shared resident directory maps only resident PageIds to bodies/ref state; evicted IDs use the fixed disk catalog. This is not an unbounded PageId map hidden behind a disk value. Reuse IDs only after all graph references and physical readers are gone. Workspace identity/generation is part of the owning service, so no lookup can cross a fresh Workspace's namespace.

The transition is incremental from the first page:

1. Before a mutation, reserve its new buffers, owner/release tickets and worst-case spill/recovery working set from aggregate admission. Create a private immutable candidate using existing PageIds for unchanged children. Dirty allocator/ref state is also charged and bounded.
2. Publish by exact root comparison. A resident page already has a host-readable owned body and registered child/token ownership, so snapshot acquisition only retains the root. There is no disk flush or cache enumeration at acquisition.
3. Under pressure, select a bounded number of resident page/allocator entries, write their bodies and exact current ownership/location metadata to lazily created backing, then atomically switch each location. Retain the resident source until the spill receipt is resolved and all readers of that source finish. The logical root/PageId does not change; spill is not another mutation or Commit.
4. A disk parent may refer to a still-resident child: PageId resolution checks the bounded resident directory before the disk catalog. A resident child cannot leave that directory until its own disk entry is valid. There is no requirement to recursively flush children or drain the whole cache to evict a parent.
5. Dirty refcount/catalog pages follow the same bounded cache discipline. Their fixed slots are reusable; growing a sparse catalog address range does not populate every potential slot. A header update uses its retained exact receipt after ambiguous I/O rather than recomputing a decrement.
6. Failure keeps the old body/location and its resource charge; a failed spill does not make the snapshot unreadable or roll back an acknowledged mutation. If no legal eviction/recovery reservation is available, later admission must backpressure or fail before install under the existing resource contract. Root-install synchronization never performs spill/read/reclaim I/O.

The shared cache must not mean a shared logical Workspace. New IDs/root contexts remain independent. One pool bounds actual resident pages, request/reader scratch and tickets across many short Workspaces. Per-Workspace hard limits remain limits, while aggregate allocation is charged when resources are admitted. Do not reserve all of a 7.25-GiB maximum up front for a zero-edit instance. Reserve actual disk growth plus non-consumable recovery headroom atomically; quota exhaustion must remain failure-safe. Do not promise every Workspace can simultaneously reach its maximum.

Reserve each root's future release slot when its owned root is created, so making queues lazy does not introduce allocation or failure in `Drop`. A shared-cache mutex may protect directory/credit/locator transitions only: it must not be held across backing I/O, external-token callbacks or another Index traversal. The present metadata-Index → Payload → private-Index callback order is acyclic; a cache added around both indexes must not turn it into a recursive lock cycle. Cache resident-byte credit and actual page ownership are distinct: taking an eviction victim must retain its ownership and source bytes until its exact destination receipt is resolved.

Reclamation still follows actual references. Do not use a single retained epoch to pin all writes since snapshot capture. If a current-only intermediate page has never spilled, last-reference reclaim can release its buffer and enqueue its bounded children without any filesystem I/O. If it spilled, retain the existing bounded physical release mechanism. Snapshot roots do not hold eviction pins; physical read pins are short and separately capped. A reader of one file should lease its exact immutable file/range input where possible, not retain an entire Workspace merely to save a lookup.

The proposed cache/eviction state machine is a design, **not an already verified implementation**. Its hardest tests are snapshot acquisition during page eviction, all-cache-resident to mixed-resident transition under a tiny budget, last-ref versus spill completion, source I/O uncertainty, and a held snapshot with repeated overwrite/reclaim. These are indispensable before a performance claim.

## Payload specialization without weakening ownership

Start with the same `Payload::write_from`, token retain/release, subrange and read interfaces. Keep spool-vs-SDK classification and immutable logical origin/coordinates. Do not change the canonical encoding pipeline or impose implicit fsync.

**Tiny tag:** an immutable payload of at most the existing bounded small-inline size can live in a packed token record. It needs no arena LOCATION/PHYSICAL or COVER pair. Its ordinary-spool charge is still charged exactly once while owned; SDK-inline remains a distinct source class. It is not free memory and is not silently counted against the wrong public limit. A retained tiny source can keep its bounded whole record while subrange tokens exist: even 64 bytes is below the required 4-KiB indivisible payload ceiling. Source-owner count and token reference count are different. Dropping the initial whole token must not destroy the byte-bearing source while subranges still refer to it. Exact same-token references do not multiply physical byte charge. Packed metadata containing raw bytes must be included in physical/resident accounting and explicit fsync semantics.

**Whole arena-source tag:** one record can encode the initial source/token, length/highwater, class, location, refcount and whole-coverage state; one physical reverse row supports evacuation. The six general rows are not necessary while exactly one whole-token interval describes coverage. Promote before the first partial slice/relocation, append/join, or other operation that creates a second independently owned token interval. A full-range duplicate also requires promotion unless it reuses the same token with the correct reference increment. Reserve the expanded records, construct them privately, then atomically change the allocator root. Preserve the old token ID and its reference count. The initial COVER count is **one token interval**, not the number of metadata pages retaining that token. Add the new subrange with at most two boundary splits against the initial whole interval. This promotion is O(1) initial metadata work, not one operation per 4-KiB block of a 64-MiB source. All subsequent fragmented ownership uses the existing bounded interval algorithm. Failure leaves the compact record readable and the old refs unchanged; physical readers retain their old location until completion. This trigger was expanded during the final adversarial review; it is a proposal correction, not an observed product regression.

Do not promise promotion stays constant for an already fragmented source: work follows actual overlap fragmentation and remains under its admitted record/path bounds. A whole token's last reference can require deferred release across many coverage intervals, exactly as today. Repeated transient slices must normalize and reclaim; otherwise specialization just delays an operation history leak.

Small arena replacements may share blocks/segments, as v0.1.5 did, but each source cannot independently count or free the same physical block. Either use the tiny token form or implement a shared physical-block/segment allocation record with exact liveness and fixed recovery headroom. Mixing SDK-inline and ordinary-spool accounting in one source without per-class ownership is incorrect. Merely changing `aligned(len)` to `len` is unsafe. Retaining an entire 1-MiB old segment for one byte is also incorrect. This physical allocator improvement should not delay the less risky D0/D1 metadata work.

## Accounting models

### D0/D1/D10 with an unchanged 100,000-file base

Define D0 as open a fresh Workspace and snapshot it without touching child inodes. D1/D10 modify one/ten distinct files; the base contains 100,000 files. Lookup depths/canonical reads are separate; a base file edited in the middle may need a compact splice, not just one physical range. These are proposed microcases, not executed tests.

| Domain | Current cost shape | Proposed first implementation shape |
|---|---|---|
| D0 | Root imported only; current fixed 13.575-MiB reservation (5.575 MiB before the reader-accounting correction), preallocated queues, six empty files, root index writes | Constant fresh context and small tickets; immutable base reference; no mandatory overlay/payload files or namespace materialization |
| D1 | The same fixed cost, one or several independent range pages, six raw-source rows, repeated COW/header I/O | A bounded compact inode/range record in a packed resident page, one owned replacement; no spill solely because the first edit occurred |
| D10 | Fixed cost plus records/pages for ten affected/imported inodes and their traversed bindings | A handful of packed resident pages/owners, charged by actual use; spill one bounded unit only if the shared cache is pressured |
| All | Canonical base size is not overlay residency; changed/touched namespace costs still exist | No O(100,000) initialization, snapshot walk, fact export or threshold migration |

A resident-page design is not an assertion of zero total RSS: Store reader, TCP/kernel adapter, thread stacks, replay, canonical cache and construction have separate costs. In D1/D10 with sufficient admission and no explicit fsync, **no private metadata disk write is intrinsically required**; the host already owns immutable readable buffers. This is the important cost-shape target. Fixed numeric latency targets require compatible existing benchmark evidence; none is invented here.

For an additional closed-form page illustration, use fresh files with one-byte whole contents and 16-byte names, rather than base-file splices. Ignoring history and assuming ideal seven-entry/eight-child packing, current D1 has approximately four main pages (including its range) and one payload-catalog page, plus 4 KiB raw data; catalog region rounding makes about 40 KiB. Current D10 has approximately 27 main pages and 12 payload-catalog pages plus 40 KiB raw data, about 212 KiB with two catalog regions rounded separately. These are optimistic representation models, not upper bounds or measurements; allocation highwater, tree split distribution, old roots, import/replay/kernel records and failure residue add costs. The fixed admission reservation applies separately to all three cases.

### 100,000 new single-range files: full assumptions

Use 100,000 new single-link regular files, one parent directory, 16-byte names, one ordinary one-byte whole-file write each, no kernel pins, no unresolved attempt and no extra held old root. Preserve current cookies/reverse bindings. The parent/root contributes O(1) rows. No source fixture is generated. Current admission limits, performance or cleanup may prevent this example from running; this is not a capacity claim.

For current packed page accounting, each leaf holds at most seven entries. Compute internal levels by repeatedly taking `ceil(children/8)` until one root. Multiply total pages by 4,352 B (4,096 body slot + 256 catalog capacity), then separately account allocation rounding, highwater holes and retained versions.

| Current steady inventory | Rows/pages | Model contribution |
|---|---:|---:|
| Main inode/binding/cookie/change rows | ~900,003 rows | 639,491,584 B |
| Independent singleton range roots | 100,000 pages | 435,200,000 B |
| General payload catalogs | 600,000 rows | 426,334,976 B |
| One-byte arena sources padded to 4 KiB | 100,000 blocks | 409,600,000 B |
| **Subtotal before canonical descriptions** | | **1,910,626,560 B (~1.779 GiB)** |

At Commit, current singleton descriptions add three pages/file (1,305,600,000 B) plus the outer correspondence forward/reverse/pending maps. Their three rows/file add approximately 213,178,368 B under the same low-fanout model, or **1,518,778,368 B** combined. Canonical construction/admission scratch and Store bytes are additional. Some raw ownership can later be substituted/reclaimed; do not add all categories as permanently simultaneous without stating the actual retained owners. Conversely, do not count raw cleanup before it completes. The direct 106-byte singleton representation removes the three-page term; it does not make its outer maps or lifetime free.

Independent existing evidence now supports the distinction between a leak and structural cost: `../step1-capacity/README.md` records the initial 8,193-file attempt failing at file 6,841 because the Payload catalog did not recycle, then an 8,193-file component PASS after `ca46e2793`. Its observed final Payload index allocation is **71,593,984 B**, or about **8,738 B/file**, with arena allocation **33,611,776 B**. That is substantially above this table's ideal seven-entry packing lower model and approximately two index pages/file; it does not measure the whole overlay footprint. The earlier observed backlog growth was fixed and is not used here to justify permanent structural overhead. The run did not Commit and is not a million-file or benchmark qualification. This review did not rerun it.

For a **first dense-page implementation**, keep all nine main rows and all six payload rows, replace the independent range leaf with a conservative 64-byte packed singleton contribution, and retain generic envelopes. Raw encoded metadata is `(588 + 64 + 384) * 100,000 = 103.6 MB`. With assumed 65–80% leaf fill, 2.5% internal-tree overhead and the current 6.25% page catalog ratio, that is approximately **134.5–165.5 MiB of metadata**. This is not a 20-MiB design. Real fanout, keys, separators and old/new reservations must replace those assumptions in qualification.

After removing COOKIE_NAME (56 B/file), adding its cookie to the binding (8 B), eliminating dedicated inode/binding change rows (196 B), and adding one sequence to each authoritative row (16 B), raw encoded metadata becomes `(424 + 384) * 100,000 = 80.8 MB`. Under the same assumptions, **104.9–129.1 MiB**. This retains the reverse binding and all six payload rows. It omits no required payload bytes: current separate raw allocation is still another **390.625 MiB** for these one-byte writes until tiny/shared-block allocation is implemented. Correspondence, snapshots, scratch and highwater remain additional. A 65–80% fill model is a declared assumption, not a new resource gate.

Further reductions—omit absent canonical/base fields, singleton alias, compact payload tags, singleton correspondence—must be costed from their **final encoded formats**. Replacing 384 B/source with an assumed 40 B while forgetting refcount, source lifetime, class, reverse lookup, catalog checksums or free-space records is not defensible. Tiny payloads could reside in metadata rather than 390.625 MiB of private raw blocks, but those bytes and their retained versions remain charged. The recommendation is to establish D0/D1/D10 first, then publish the exact new census and matched 100,000-file accounting rather than promise an unsupported final total.

## Self-review and required counterexamples

| Tempting shortcut | Why it fails / concrete required repair |
|---|---|
| Clone a small mutable map at Commit, then spill later | Acquisition cost depends on changed files; threshold migration or first write copies the map. Use immutable evictable pages from the beginning. |
| Pin a persistent Arc tree forever and call it spillable | All page bodies remain resident through root ownership. Separate PageId ownership from evictable body location. |
| Retain all post-capture pages under one epoch | Long C1 pins unrelated overwritten intermediates; reference-based reclaim must continue. |
| Increase page fanout alone | More linked/token refs per copied page can increase uncached ownership syscalls. Batch and cache ownership updates as well. |
| Sequence maximum without deletion markers | Unlink may erase the only effect. Retain/prove each deletion and preserve base-backed masks. |
| Sequence maximum used to find all old tombstones | New keys may prevent pruning everywhere. Use a retirable summary/queue and remove the old sequence-order assumptions. |
| Inline alias after observing just one base name | Hidden canonical hardlinks break identity/invalidation. Use explicit incomplete/many state or retain reverse indexing. |
| Whole-source promotion sets COVER count to token refs | Metadata-page sharing then overcounts interval owners; initial whole token contributes one interval, independent of refs. |
| Tiny bytes treated as SDK Inline | Ordinary-spool quota changes silently. Preserve source class and retained-owner accounting. |
| Free catalog slot means physical space freed | Header highwater, fragmentation and failed truncation remain allocated/charged. Keep truthful physical evidence. |
| Report old v0.1.5 numbers as target PASS | Different implementation, visibility, capture and custody; collect only the applicable compatible campaign later. |

The minimum meaningful implementation sequence is: freeze representation amendments; implement lazy ownership/body cache and compact inode/range/correspondence encoding; prove D0/D1/D10 and spill/snapshot races; batch allocator/index work; remove redundant changed/cookie inventory with deletion/hardlink tests; then specialize payload records with partial-retention and failure proofs. Run the required final benchmark campaign only after public supported-surface correctness and source custody are complete. This report neither executes #122 nor authorizes deleting required semantics to reach v0.1.5 latency.

## Source identity

The reviewed baseline HEAD is `3f04d4146`; current worktree files include other agents' uncommitted changes. HEAD advanced during the read-only review to `ff7098929`, after `ca46e2793` fixed token/cascade reclamation and `ff7098929` added reader-cache admission. Both changes are retained. The important still-observed structure is the seven-entry Index, one-page range node, six-row fresh payload source, eager six-file Resources factory, and dual change/cookie inventory described above. Completion-time SHA-256 values follow; this report owns no product file and must not be used to overwrite later work.


Capture UTC: 2026-09-14T02:54:57.232318+00:00; HEAD `ff7098929eda07d91fbfc2d1072b286231203325`.

| Source | SHA-256 |
|---|---|
| `crates/layerfs-workspace/src/overlay.rs` | `f86e574501a6f08b570db74daf477c5db1692ba7617eca3a0a280ed786a485e3` |
| `crates/layerfs-workspace/src/overlay_index.rs` | `eaf9a14a67bb189a3b293e2336f6637c4a4d7717def2dd7d1a05de472bbe1a79` |
| `crates/layerfs-workspace/src/overlay_ranges.rs` | `386873a0a109e20396e228113393283ac10fb663a13e1e6eaeb4c7b2fa85e76a` |
| `crates/layerfs-workspace/src/overlay_payload.rs` | `ac2bac76cc486e99be6d87a072288e5e89bb50f9c6435ad05a05682e9df2d5f5` |
| `crates/layerfs-workspace/src/overlay_budget.rs` | `7cee989550a603fdf33bd94712314b074b0ffc9626c35ae02d42f9f0d1256ef9` |
| `crates/layerfs-workspace/src/host_overlay.rs` | `f59c3801bafea8498ca92337dddb887f36711d969bc5433980ab5081c9fe5b74` |
| `crates/layerfs-workspace/src/host_directories.rs` | `fcf5d0793154a085d770a28128170f16ea3f6e74a8b5fecc94700dd63797fb9a` |
| `crates/layerfs-workspace/src/correspondence.rs` | `15e1d267950819bc5742e2520b5f604697339e72eae04f3d378ddb2936e8fe95` |
| `crates/layerfs-workspace/src/snapshot_candidate.rs` | `837173451ebcc2fe1498c134e32d533df8837549c7d72fca7f58cc66b5a8a34e` |
| `crates/layerfs-workspace-core/src/limits.rs` | `ddd5784415c80ba20e1ae22d8e9f84d2ff7100ca521653c89400dc51e4364888` |
| `docs/roadmap/0.1/0.1.6/overlay-snapshot-rule.md` | `03d93fc5929ba2b6e99c013e4898a454ddf908c81089c41bb87f9d24d54f543e` |
| `docs/roadmap/0.1/0.1.6/overlay-snapshot-spec.md` | `1e5913da8454cf0aae597f0d5cf6908ab8ebe8e86e212f966e424df799b356da` |
| `docs/roadmap/0.1/0.1.6/overlay-snapshot-contract-resolution.md` | `af3fe9e4776110eada28fac0be4648f2208122fcf731d977e15c86c630670530` |
