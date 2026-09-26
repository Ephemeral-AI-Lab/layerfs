# #245 joint tree and resource research for #248 and #256

> **Status:** Research; informative and not a product contract.
>
> Source pin: f74dbe77da12fa533587be8a578375bce3f19373 (2026-09-26, after #252 and PR #255). Three parallel read-only source reviews informed this report. No benchmark was run. All capacities below are algebraic estimates from current formats, not observed admissions or latency claims.

[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) scales final changed runs within one file. [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) scales changed files and names within one Workspace generation. They share private page allocation, ownership, quota, root pinning and reclamation. Their trees select children differently. The source still has a 4 GiB logical-file maximum; these issues do not currently specify a larger file format.

## 1. Count the actual tree roles

    Workspace private backing (one Arena / RootOwner ownership substrate)
      ├── length-indexed file extent tree
      │     branch: child PageRef + subtree byte length
      │     leaf: Base / Local / Zero extent records
      └── key-indexed metadata tree
            branch: maximum key + child PageRef
            leaf: D dirty, I inode, N directory, R result cells
                  and per-directory E binding / T tombstone roots

    Commit consumes a frozen Workspace root
      ├── SaveFile -> C1 canonical file mapping tree
      └── prepared namespace update -> C1 canonical filesystem structures
            -> one Stage/Branch-head publication

The first two are the **two private Workspace tree algorithms**. The latter C1 structures are separate content-addressed canonical objects. Directory bindings and dirty identities are instances of the keyed private index, not additional B+ implementations. See [the file tree](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs), [the keyed index](../../../crates/layerfs-workspace/src/backing/metadata_index.rs), [private page formats](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs), and [C1 filesystem input](../../../crates/layerfs-content/src/filesystem/input.rs).

### Reuse decision

Reuse the existing [Arena and RootOwner](../../../crates/layerfs-workspace/src/backing/metadata.rs), [PageRef codecs](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs), [ownership ledger](../../../crates/layerfs-workspace/src/backing/ownership.rs), quota and custody. Repair the two tree algorithms separately. The keyed index splits variable-size cells by maximum key and occupied bytes; the file tree splices byte intervals, carries subtree lengths and coalesces extents. Their descent, balancing and cursor positions are not the same code.

A new generic private B+ engine was considered. A small structural abstraction is estimated to add roughly 150–300 kernel LOC plus 60–120 adapter LOC and net 100–240 LOC after replacing limited duplication. A universal engine including C1 would need different page identities and canonical format rules plus a separate compatibility proof; its code size is not estimated here. Neither is the shortest safe implementation now. Extract a tiny helper only if the completed fixes reveal identical level/path code in both trees.

### SRP extraction plan

SRP supports independent data-structure modules. The proposed destination is **a staged relocation of existing code plus focused new behavior**, not a new generic B+ engine or a requirement to move the whole backing tree in one commit:

    backing/
      page_store/    Arena, RootOwner, PageRef, ledger, quota and reclamation
      payload/       aligned payload segments, charged ID index and cleanup
      binary_plus_tree/         independent private B+ data-structure component
        mod.rs        thin dispatch/reexports; shared invariants only
        extent/       length-indexed format, splice/balance and byte cursor
        keyed/        keyed format, update/delete and ordered key cursor
    commit/          SaveFile and prepared namespace orchestration stay here

The B+ tree lives explicitly in binary_plus_tree/: extent/ and keyed/ are two specializations over the same page_store/. Its mod.rs may host a checked level/path helper if both implementations use it, but there is no universal node layout or generic mutation engine to extract from current code. Keep old reexports while moving one responsibility at a time. Each mod.rs remains a thin declaration/delegation file below 200 physical lines; each implementation file remains below 1,000. The following **destination physical-line envelopes include relocated code** and are not net additions or a mandate to create every listed file:

| Proposed destination file | Estimated physical lines | Responsibility |
| --- | ---: | --- |
| page_store/mod.rs | <100 | Thin API and declarations. |
| page_store/host.rs | 300–450 | Shared budget and arena registry. |
| page_store/arena.rs | 350–550 | Physical page/slot allocation. |
| page_store/root.rs | 350–550 | COW root ownership and publication. |
| page_store/ledger.rs | 400–550 | Owner records and grouped ref deltas. |
| page_store/reservation.rs | 300–400 | Candidate/completion and progressive builder reserves. |
| page_store/reclaim.rs | 500–750 | Page/root retirement and custody progress. |
| payload/mod.rs | <100 | Thin payload interface. |
| payload/records.rs | 350–500 | Acquisition records and charged state. |
| payload/acquire.rs | 350–500 | Bounded input and segment acquisition. |
| payload/segments.rs | 350–420 | Existing aligned physical I/O. |
| payload/index.rs | 220–300 | Charged ID lookup and reclaim eligibility. |
| payload/reclaim.rs | 280–350 | Routine and pressure cleanup. |
| payload/directory.rs | 100–200 | Incarnation-scoped directory ownership. |
| binary_plus_tree/mod.rs | <100 | Thin tree-family reexports and any actually shared level check. |
| binary_plus_tree/extent/mod.rs | <100 | Thin length-indexed tree surface. |
| binary_plus_tree/extent/format.rs | 180–280 | PieceRecord/ChildRef codec and checks. |
| binary_plus_tree/extent/splice.rs | 450–650 | Level-aware path copy and balancing. |
| binary_plus_tree/extent/cursor.rs | 370–520 | Byte-position traversal. |
| binary_plus_tree/keyed/mod.rs | <100 | Thin key-indexed tree surface. |
| binary_plus_tree/keyed/format.rs | 150–260 | Cell/fence codec and checks. |
| binary_plus_tree/keyed/update.rs | 350–520 | Find, insert and split. |
| binary_plus_tree/keyed/delete.rs | 200–350 | Delete and sibling rebalance. |
| binary_plus_tree/keyed/cursor.rs | 150–240 | Ordered key traversal. |
| binary_plus_tree/keyed/build.rs | 180–330 | Multilevel ordered builder. |

The source files for page ownership alone already contain more than 2,300 physical lines; these destination ranges allow that existing code plus new batching work. The separate §6 forecast estimates *net new production behavior*. Relocation itself is reported as migration, not an algorithmic LOC reduction.

## 2. Physical format and capacity model

| Current constant | Value | Consequence |
| --- | ---: | --- |
| Private page / header | 4,096 / 128 B | 3,968 B encoded page body. |
| File leaf / branch | 124 extents / 248 child refs | Extent record is 32 B; child ref is 16 B. |
| Keyed leaf | 128 cells maximum, byte-filled | Cell costs 4 B framing + key + value. |
| Ownership ledger | 62 owner slots per 4 KiB ledger file | Custody slots and metadata pages both need owner records. |
| One distinct 1-byte payload | 8 KiB | 4 KiB header + 4 KiB aligned data. |
| First dirty mutation reserve | 345 × 4 KiB = 1.348 MiB | Candidate 137 pages plus completion escrow 208 pages. |
| Current shared page slots | 65,536 | This count can bind before a 1 GiB disk quota. |
| File length / extent length | 4 GiB / 2^24−1 B | A contiguous 4 GiB Base/Zero range needs 257 extent records. |

Sources: [page formats](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs), [ledger/slots](../../../crates/layerfs-workspace/src/backing/ownership.rs), [payload segments](../../../crates/layerfs-workspace/src/backing/segments.rs), [reservations](../../../crates/layerfs-workspace/src/backing/metadata.rs), and [SaveFile contract](../../../crates/layerfs-bridge/src/contract/request.rs).

Let A be unique retained metadata page files across all pinned roots, J allocated ledger files at slot high water, and b_i the lengths of distinct retained payloads. The tracked backing blocks are:

    D = 4 KiB × (A + J)
        + Σ_i [4 KiB × ceil(b_i / 1 MiB)
               + 4 KiB × ceil(b_i / 4 KiB)]
        + outstanding reservations and uncertain allocations.

This excludes ext4 inode/directory/journal overhead, canonical Store objects, and OS file-backed cache. Sharing a page across generations counts it once; path-copied pages and frozen readers keep distinct pages and payloads charged until released. A logical Base extent reads canonical bytes and a Zero extent allocates no private payload.

### Resource-budget illustration

These are optimistic **all-metadata** bounds holding the first-mutation reserve, assuming prompt reclamation and no payloads or other Workspaces.

| Private disk quota | Current data pages + ledgers | Quota-only hypothetical after slot, ledger-table and retained-memory caps scale |
| --- | ---: | ---: |
| 1 MiB | First mutation cannot reserve 1.348 MiB | Same until reservation policy changes. |
| 16 MiB | ~3,691 pages + 60 ledgers | Same; quota binds first. |
| 1 GiB | 65,536 pages + 1,058 ledgers, ~260 MiB used | ~257,643 pages + 4,156 ledgers, ~1,023 MiB used. |

The 8-byte PageRef already has a 32-bit slot and reuse epoch. A supported 1 GiB quota can use more than 65,536 slots without widening that codec, but the decoder, allocator, 1,058-ledger identity table, reconciliation reservation and 128 KiB retained-metadata sub-budget must scale together. The 32-root and 11-arena ceilings are simultaneous-resource limits: audit them against the selected concurrency workload rather than deleting them merely because they are counts. Slot epoch exhaustion needs retirement and charged fresh-slot allocation, rather than a lifetime edit-count refusal. A quota beyond roughly 16 TiB of 4 KiB slots would require a wider reference or an explicit supported byte-budget contract.

## 3. File extent tree: how much can it represent?

For E densely packed extent records, the optimistic live tree has L = ceil(E/124) leaves, then ceil(L/248) first branches and further 248-way levels until one root. Raw tree-page storage is 4 KiB per page. Occupancy is not guaranteed; these rows exclude ledgers, payloads, the keyed inode tree, pinned generations and reservations.

| Raw file-tree page budget | Pages | Packed extents | Isolated one-byte changed runs with Base gaps |
| --- | ---: | ---: | ---: |
| 1 MiB | 256 | ~31,372 | ~15,685 |
| 16 MiB | 4,096 | ~505,672 | ~252,835 |
| 256 MiB | 65,536 | ~8,093,356 | ~4,046,677 |
| 1 GiB | 262,144 | ~32,374,540 | ~16,187,269 |

The 1 GiB tree-only row is impossible under the current 65,536 global slot ceiling; the 256 MiB row unrealistically gives all slots to one tree. Actual one-byte Local changes pay 8 KiB each and therefore run out of backing much sooner.

| Final file shape | Packed tree | Tree-only ledger | Distinct payload bytes |
| --- | ---: | ---: | ---: |
| 4,097 isolated one-byte changes | 8,195 extents; 67 leaves + 1 branch = 68 pages, **272 KiB** | ≥2 pages, 8 KiB | 4,097 × 8 KiB = **32.008 MiB** |
| 65,536 isolated one-byte changes | 131,073 extents; 1,058 leaves + 5 branches + root = 1,064 pages, **4.156 MiB** | ≥18 pages, 72 KiB | 65,536 × 8 KiB = **512 MiB** |
| 4 GiB all-Zero logical file (or all-Base after the exact-end fix) | 257 records; 3 leaves + 1 branch = **16 KiB** | ≥1 page | No private Local payload |

Full custody ledgers are larger than the tree-only ledger column because every distinct payload owns a custody slot. Including those slots and the first-mutation reserve, a favorable 4,097-change shape costs roughly **33.9 MiB** of tracked private backing before transient COW pages or ext4 metadata. At 16 MiB, the same model admits roughly 1,843 such writes; at 1 GiB, the current slot ceiling binds near 64,488, while quota alone would allow roughly 128,799. None is a production admission or performance measurement.

The file tree is already a length-indexed B+ style rope. Its structural proof remains open: [pack](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs) may create a singleton branch at 249 leaves, and height-two narrow splices may mix child levels. The [piece parser](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs) also appears to reject a source span ending at exactly 2^32 although the declared file maximum is 4 GiB. These are source-derived risks, not reproduced failures. #248 needs level-aware local splice/balance, a resumable ordered cursor, and exact-boundary tests.

## 4. Namespace keyed tree: many small files

A keyed leaf cell costs 4 B framing plus key and value. With a 16-byte or 32-byte name, an E binding costs 37 or 53 B; a T tombstone costs 22 or 38 B. An E leaf packs at most 107 or 74 such bindings, subject also to the 128-cell cap. The global D dirty cell costs 22 B and the I inode cell 173 B. The following static model assumes F new empty files under one existing directory, one dirty parent, dense pages, and no old pinned generation. It excludes ledgers, COW transients, service objects and payloads.

| F empty files | Name bytes | E pages | Global D/I/N pages | Raw live pages | In-flight saved-result R pages | Prepared row bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 129 | 16 / 32 | 3 / 3 | 9 | 12 / 12 = 0.047 MiB | 5 | 14,039 / 16,103 |
| 257 | 16 / 32 | 4 / 5 | 16 | 20 / 21 = 0.078 / 0.082 MiB | 8 | 27,735 / 31,847 |
| 1,025 | 16 / 32 | 11 / 15 | 57 | 68 / 72 = 0.266 / 0.281 MiB | 26 | 109,911 / 126,311 |
| 65,536 | 16 / 32 | 619 / 898 | 3,521 | 4,140 / 4,419 = 16.172 / 17.262 MiB | 1,575 = 6.152 MiB | 7,012,588 / 8,061,164 |

The raw live-page column excludes the R result tree that Commit writes while saving each changed file. At 65,536 files that second tree adds about 6.152 MiB before ledgers, reservations, G1/G2 pins or canonical Store output. The global page count includes the dirty parent marker; omitting it undercounts the exact 65,536 boundary by one page.

A 1 MiB **E-tree-only** allocation could densely hold roughly 27,071 16-byte names or 18,648 32-byte names; with one D and I record per new empty file, the same raw page budget holds roughly 4,004 or 3,762. A 1 MiB *whole private-backing quota* admits no first dirty mutation today because its upfront reservation is 1.348 MiB. Many directories with one file each consume at least one E root page per directory and therefore differ sharply from one wide directory. If every new file has a distinct one-byte payload, add 8 KiB per file plus custody ledger capacity: 1,025 such payloads already add about 8.0 MiB.

The live keyed tree already has [find/update/next](../../../crates/layerfs-workspace/src/backing/metadata_index.rs), with maximum-key branch fences. Ordinary create uses a path-local keyed insertion. Current [directory deletion/rebind](../../../crates/layerfs-workspace/src/overlay/directories.rs) enumerates up to 128 names and rebuilds an E or T tree, so repeated updates can be quadratic in directory width. #256 needs path-local delete/rebalance, a persistent key cursor and a multilevel ordered builder. That builder must seal and transfer finished pages progressively: reconciliation currently reserves only 64 pages and 26 slot credits, and a RootOwner refuses its 129th temporary page. Those limits cannot build the roughly 3,521-page global tree in the 65,536-file example even with free quota. Its 128 dirty identities/names, 32 KiB prepared input, 256 active nodes, shallow keyed-tree level and 256-cell ordered builder are separate ceilings. The receiver and C1 also materialize rows. Service currently calls C1 with no ordering backing, so the reducer's first spill after 4,096 distinct pending serials fails; passing the existing FileBacking fixes that refusal but not the proportional receiver/C1 vectors and sets. Existing-base alias/cycle validation has a separate 4,096-visited-binding work limit that can block a wide directory rebind; it does not necessarily block fresh regular-file additions. The old prepared codec also uses u16 total counts, which refuse exactly 65,536 rows even if 128 and 32 KiB checks are removed. A 65,536-file one-directory input is about 6.69–7.69 MiB of prepared rows before frame overhead, but still needs a wide-count streamed format, bounded receiving state and charged ordering work.

## 5. Before and target complexity

Use R = final changed runs in one file, F = final changed inode identities, K = final changed names, H = private tree height, L = distinct leaf pages reached by one ordered cursor, S = final replacement bytes, and N = retained payload records. Fixed page fanout is a constant in asymptotic notation but its actual I/O remains in the evidence.

| Operation | Current source at f74d | Target shared #248/#256 work |
| --- | --- | --- |
| One narrow file write | Touched-path COW pages, but routine payload maintenance scans N records twice; copied pages update every child reference through separate ledger I/O. | Touched file/index paths plus actual input bytes; resource-charged keyed payload lookup and work-triggered reclaim; group owner deltas by ledger page before atomic root publication. |
| N retained tiny file writes | O(N²) cumulative cleanup record checks; 8 KiB payload file per one-byte acquisition. | No scan of all previous writes per callback; total work follows submitted bytes, touched paths, and owned physical I/O. |
| Local payload read for R Local extents | Payload ID lookup scans the record Vec; many distinct Local extents can cost O(RN) comparisons. | Charged keyed ID lookup, expected O(1) or worst-case O(log N), plus actual payload reads. |
| File sequential cursor / Commit | Ancestors can be reread per leaf and a new cursor created per transport pull; lower_file holds metadata writer across full frozen walk. | Persistent bounded cursor: O(H+L) index-page reads per pass and O(final records) CPU, O(R+S) required output work, no long mutation-gate hold. |
| One namespace create | Keyed insertion already path-local, but linear active-node/fresh scans and one ReserveInodes service round trip per new file add cost. | Keyed active-node index, charged active pins and serial-reservation windows; unchanged atomic namespace publication. |
| One namespace delete/rebind | E/T full-directory rebuild, O(K) materialization and potentially O(K²) cumulative work across K updates. | Keyed path-local delete/rebalance, O(H) touched pages per name plus bounded sibling work. |
| Namespace Commit | Repeated root-to-leaf successors, O(F+K) resident Vecs, sort, 128/32 KiB refusal, shallow builder. | Ordered D and merged E/T cursors, O(H+L) index-page reads per cursor; fixed transport frames, charged spool and one canonical result/head. |
| C1 canonical construction | File split/concat per final run; namespace receiver/C1 materialize input and touched serials. Cost and canonical identity risk vary by workload. | Reuse current exact-root file builder if counts pass; otherwise bounded frontier with identity proof. Stream/index namespace input and reuse C1 FileBacking for external ordering. |

Lower bounds remain: R final runs require Ω(R) descriptor validation, S replacement bytes require Ω(S) transfer, and F changed files with K changed names require Ω(F+K) input/validation. The target forbids an extra full-history or whole-frontier pass per small operation. A bounded resident tree/stream frontier does not make OS page cache free or make time independent of input size.

## 6. Implementation slices and file/LOC forecast

These are **planning estimates**, not the exact first-parent production LOC count required by AGENTS.md for a future commit. “Physical” includes comments and relocated code; “net production LOC” excludes relocated code, tests and docs. Large files must be split by responsibility before the Core 999-physical-line ceiling; lib.rs/mod.rs stay under 200. The tables below name **current source files** to estimate where engineers will work; the SRP destination map in §1 shows how responsibility-based moves can split those files. A move is counted once in the eventual committed tree, not once at its source and again at its destination.

### Shared private backing, implemented once

| File | Current → likely physical lines | Responsibility |
| --- | ---: | --- |
| backing/metadata.rs | 882 → ~630 | Host/Arena/RootOwner state and per-Arena writer gate. |
| backing/metadata_reservation.rs (new) | 0 → ~330 | Candidate, completion and scalable reconciliation reserves; much moved. |
| backing/ownership.rs | 940 → ~720 | Slots, custody, page edges. |
| backing/ownership_ledger.rs (new) | 0 → ~470 | Ledger codec, grouped reference deltas and progress custody; moved plus new. |
| backing/payload.rs | 620 → ~580 | Payload acquisition and handles. |
| backing/payload_index.rs (new) | 0 → ~260 | Charged ID index and cleanup eligibility. |
| backing/reclaim.rs / metadata_reclaim.rs | 246/567 → ~320/~640 | Work-driven payload/root retirement and quota recovery. |
| backing/metadata_pages.rs / segments.rs / budget.rs / mod.rs | 535/355/56/15 → ~550/~370/~65/~18 | Existing codecs, I/O, charged allocation and wiring. |

Estimated shared growth: **~700–850 physical lines, ~500–700 net production LOC**, excluding tests/docs. A packed-page format is an optional later change if public-route syscall counts demand it, adding roughly 400–700 physical lines. The shared work must remove the 65,536 page-slot/1,058-ledger admission, scale the 128-temporary-page candidate and reconciliation's fixed 64-page/26-slot reserve through progressive quota-charged output, and address the 128 KiB retained-metadata sub-budget. The 32-root and 11-arena limits are simultaneous-resource limits to audit against actual concurrency; they need not be raised merely to support many files in one Workspace. Page capacity and bounded I/O windows are legitimate; aggregate cardinality ceilings need resource-based admission.

### #248 file-specific work

| File | Approximate new/touched implementation LOC | Main work |
| --- | ---: | --- |
| backing/metadata_pieces.rs | ~260–520 touched; move splice/pack | Keep format/Fold, expose focused splice owner. |
| backing/metadata_pieces/splice.rs (new) | ~350–650 including relocated code | Level-aware path-copy splice and local balance. |
| backing/metadata_cursor.rs | ~70–210 | Resumable branch stack. |
| backing/metadata_pages.rs | ~10–60 | Exact 4 GiB boundary/height checks. |
| commit/lower.rs | ~25–100 | Short frozen-root reads, no long writer hold. |
| commit/upload.rs / source.rs | ~35–120 / ~40–135 | Retain descriptor/replacement cursor across pulls. |
| filesystem/write.rs | ~15–75 | Integrate revised splice and preserve atomic publication. |
| C1 file/edit/apply.rs plus focused new builder | **Conditional**, ~250–500 touched if counters require it | Bounded canonical frontier with exact-root proof. |

The file work moves substantial existing splice code, so gross reviewed lines across source and destination double-count some relocation. A provisional mandatory **net** range is roughly **−100 to +700 production LOC**, plus conditional **+150–350** if a new C1 builder is required. The final exact first-parent count must wait for the implementation. The work is substantial despite that small possible net number.

### #256 namespace-specific work

| File | Likely net production LOC | Main work |
| --- | ---: | --- |
| backing/metadata_index.rs | +70 | Keyed delete/cursor hooks. |
| backing/metadata_delete.rs (new) | +250 | Point deletion, sibling rebalance, root collapse. |
| backing/metadata_key_cursor.rs (new) | +160 | Persistent ordered key cursor. |
| backing/metadata_build.rs | +160 | Multilevel bounded-frontier builder. |
| overlay/directories.rs | −90 | Remove whole-directory rebuild helpers. |
| filesystem/create.rs / remove.rs / rename.rs | +55 / +30 / +45 | Point mutation and serial-window integration. |
| filesystem/rename/plan.rs (new) | ~0 net | Relocate 150–220 lines from rename.rs to respect the physical-line cap. |
| runtime/node_cache.rs (new) / state.rs / host.rs | +250 / +80 / +25 | Serial index, pin-aware eviction, charged actual-length paths. |
| commit/directories.rs / lower.rs / save.rs / reconcile.rs | +100 / +50 / +50 / +85 | Frozen cursor lowering and scalable successor ownership. |
| bridge/contract/prepared_stream.rs (new) / protocol/prepared_stream.rs (new) / request.rs | +200 / +200 / +60 | Versioned ordered frame grammar, exact totals and admission. |
| server/save/namespace_spool.rs (new) / filesystem.rs / catalog.rs | +350 / +120 / +80 | Validated receive spool, C1 handoff and atomic stage/commit. |
| content/filesystem/input.rs / validate.rs / validate/cycles.rs (new) / update.rs / references/reduce.rs | +120 / +200 / ~0 net moved / +180 / +160 | Stream/indexed-spool input and bounded validation/reduction; move roughly 250–350 lines from the current 912-physical-line validate.rs into focused cycles.rs. |

Likely namespace net source growth is **about 3,000 production LOC**, with a **1,400–5,000** range until the C1 input contract is prototyped. Roughly 300–500 lines may be relocated out of large files; that relocation is not net LOC. These estimates are not a mandate to add abstractions. Use existing keyed pages, C1 sorted builder and C1 FileBacking before adding code.

## 7. Sequence, proof and measurement

1. **Freeze current-source contracts.** Keep #252 SaveFile and PR #255 cleanup. Define one shared resource/ownership policy. Pin exact source, formats, quotas, and the public SDK/FUSE workloads. Do not count an old EditFile source diagram as current code.
2. **Shared physical fixes.** Charge page slots, ledger identity capacity, payload IDs and root ownership by actual resources. Make keyed payload lookup and routine reclamation scale. Group ledger writes only with explicit progress/unknown-outcome custody; preserve old pinned roots. Localize the metadata writer gate per Arena for future #249 multi-Workspace support.
3. **Two tree algorithms.** Repair file height transitions, 4 GiB edge and resumable cursor; add keyed directory deletion, key cursor and multilevel builder. Test page occupancy, root collapse, stale PageRef epochs, G1/G2 pinned-reader and low-quota failure paths with external tests.
4. **Commit streams.** Keep SaveFile for each dirty content root. Make file lowering/transfers nonblocking to G2. Stream prepared namespace rows from G1 through fixed frames, validate/spool them, feed C1 bounded input and existing charged FileBacking, then publish exactly one filesystem root/head. Preserve canonical roots for formerly accepted inputs and precise expected-head/unknown-outcome handling.
5. **Public gates.** #248: one SDK Exec with 4,097 isolated writes, Commit and independent old/new-head oracle; focused >65,535-run and 249/250-leaf tests. #256: 129, 257 and 1,025 changed-file/name cases via SDK Exec/FUSE with full-tree oracle, plus focused wide-directory and serial-boundary tests. Combined: many tiny files and one fragmented file in the same generation, one Commit, old/new heads and G2 write preservation. No benchmark driver calls internal mutation methods.
6. **Benchmarks only at frozen identities.** Early count-driven diagnostics may trace page and ledger I/O, FUSE writes, RPCs, Store objects, page visits and memory, labelled as diagnostics. Preserve one sample per registered case/arm, declared cache state, complete-command budgets and append-only receipts. A timed-out or cache-ineligible row is not a speed PASS. Existing native 4,097 receipts are not public API performance samples.

### Open implementation decisions

- The real fraction of wall time due to per-edge ledger I/O, payload acquisition, FUSE/kernel overhead and per-file serial-allocation RPCs is unmeasured. Use count-driven public-route diagnostics before a packed-page format or C1 file-builder rewrite.
- The 4 GiB logical-file bound, path/name grammar, inode serial width and fixed operation deadlines remain. “Resource-based only” for **aggregate file/run counts** does not itself remove these format and API bounds; removing the 4 GiB maximum is a separate cross-component decision.
- A first dirty mutation reserves 1.348 MiB today. If a 1 MiB private quota is intended to support an edit, the upfront reserve design must change even if the final live tree is tiny.
- Shared global writer-gate isolation for #249 is a future concurrency proof; #248/#256 must at least avoid holding that gate over their long frozen walks.
