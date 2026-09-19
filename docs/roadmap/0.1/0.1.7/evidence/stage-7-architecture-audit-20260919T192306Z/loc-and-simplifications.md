# Stage 7 C1/C2 audit: production LOC and implemented simplifications

> **Status:** Research; informative and not a product contract.

Read-only audit, 2026-09-20. No source edits, builds, tests, benchmarks, staging, commits, or issue updates. Stage 6's active work was left alone. Current-code findings refer to the frozen snapshot `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z`, snapshot manifest SHA-256 `c2f6034c534853919833c9f59758a17e74ea6331f41835b4267adf883bb74ff8`. Committed baseline is `66bce8378b5e9ecb1135b1636f8ee2ffac46ccf5`; v0.1.6 is `dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79`. Paths below are repository-relative unless stated. Source line references were checked against the frozen snapshot; active Stage 6 edits may move them later.

## 1. Exact production LOC

Same counter for all trees: frozen `tools/production_loc.py`, SHA-256 `c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`. Counts nonblank/non-comment first-party Rust implementation under product src and shipped runtime SQL. Excludes external tests/tooling/docs/manifests/examples/benchmarks, strips legacy inline test-only items and transitively excludes test-only source modules. This is source LOC, not physical file length or git diff statistics.

| Production scope | v0.1.6 tag | Committed HEAD | Frozen working snapshot |
| --- | ---: | ---: | ---: |
| Reference `crates/` (whole old product) | 65,417 | 65,417 | 65,417 |
| Replacement C1 `layerfs-content` | absent | 12,303 | 12,512 |
| Replacement C2 `layerfs-storage` | absent | 6,453 | 6,819 |
| Replacement telemetry | absent | 763 | 763 |
| Replacement C1+C2 only | absent | 18,756 | 19,331 |
| Replacement including telemetry | absent | 19,519 | 20,094 |
| Combined first-party product | **65,417** | **84,936** | **85,511** |

The snapshot includes **575 additional production LOC** beyond committed HEAD: C1 +209, C2 +366, telemetry 0. Reference source counts are unchanged.

A package-footprint comparison gives:

| Broad package comparison | v0.1.6 | Frozen replacement | Difference |
| --- | ---: | ---: | ---: |
| old content crate → C1 | 15,701 | 12,512 | −3,189 (20.31% smaller) |
| old layerstack-store crate → C2 | 18,376 | 6,819 | −11,557 (62.89% smaller) |
| those two old packages → C1+C2 | 34,077 | 19,331 | **−14,746 (43.27% smaller)** |
| those two old packages → C1+C2+telemetry | 34,077 | 20,094 | −13,983 (41.03% smaller) |

**These are package-footprint differences, not proven equivalent-functionality LOC savings.** Old content includes file/directory/filesystem diff, reconciliation and Apple ACL responsibilities not in the replacement scope. Old layerstack-store includes branch/layerstack/workspace/history, staging, queries, schema migrations and telemetry, while C2 is the physical storage component. Even its large `objects.rs` interleaves construction/runtime adapters, physical admission, synchronization and private candidate handling; there is no sound automatic file-level boundary for a matched C2 subtotal. C1 also acquired explicit resource/telemetry/reference-reducer responsibilities. A function-by-function ownership map would be needed for a true same-scope savings figure; it is **not established** by these totals.

The honest repository-wide answer today is **no LOC has been saved through retirement**: coexistence increased production LOC by **20,094 (+30.72%)** over v0.1.6. That is migration duplication, not an algorithmic regression. The new component implementation is much smaller than the two broad old packages, with the scope caveats above. Do not state that LayerFS as a whole is 43% smaller.

Reproduction used `git archive` of exact Git objects into `/tmp/stage7-loc-data/{v016,head}`, then the same frozen counter's `scan(root)` for all three snapshots. Raw summary JSON is in `/tmp/stage7-loc-data/{v016,head,working}.json`; exact per-file production and physical counts are in `/tmp/stage7-loc-data/{v016,head,working}-files.json`. Equivalent shell commands (on fresh scratch directories):

```sh
git archive dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79 crates | tar -x -C /tmp/loc-v016
git archive 66bce8378b5e9ecb1135b1636f8ee2ffac46ccf5 crates core/crates | tar -x -C /tmp/loc-head
python3 /var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z/tools/production_loc.py --root /tmp/loc-v016 --json
python3 /var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z/tools/production_loc.py --root /tmp/loc-head --json
python3 /var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z/tools/production_loc.py --root /var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z --json
```

## 2. What has actually been simplified

The numeric results below are **historical deterministic-work/allocator receipts between successive v0.1.7 revisions**, not all comparisons against v0.1.6, not a fresh test of the dirty snapshot, and not elapsed-time speedup claims. Inspection confirms these mechanisms still exist in the frozen source. Receipt paths use `docs/roadmap/0.1/0.1.7/evidence/` as their root:

- `P1` = `phase1-execution-20260918T090000Z/rounds/`
- `P2` = `phase2-execution-20260918T150435Z/rounds/`
- Each numbered item has a `receipt.md`; these retain qualifications, controls and identities.

| Implemented change | Before → after and evidence | Current source |
| --- | --- | --- |
| One read connection/decode workspace per provider operation | Reopening each wave → shared `ReadSession`; pipeline readback **3 connection opens → 1**. Visibility ceiling still refreshed per wave; oversized demands refused before opening. P1 `p1-2/receipt.md`. | `core/crates/layerfs-storage/src/cas/provider.rs:118`, `cas/read.rs:143`, `cas/read.rs:180` |
| Read a tree branch's children in larger bounded batches | Width 32 → 256 (with scratch-based narrowing); D25/D26 **7 waves → 5**, same objects and bytes; scratch 349,820 → 709,388 B. P1 `p1-1/receipt.md`. | `core/crates/layerfs-content/src/filesystem/sorted/page.rs:37` |
| Batch child materialization | Child point reads → grouped demands; nominated fixture **170 provider waves → 107**, inode waves **68 → 5**, same 303 demanded objects / 134 inode pages / canonical root. P1 `p1-3/receipt.md`. | `core/crates/layerfs-content/src/filesystem/sorted/page.rs:499` |
| Share validation lookups and remembered inode records | Repeated per-record tree descents across validation walks → `lookup_many` + validation-scoped memo; directory-update **42 validation waves/pages → 2**, hardlink move **6 → 1**, logical demands unchanged. P1 `p1-4/receipt.md`. | `core/crates/layerfs-content/src/filesystem/validate.rs:433`, `:479` |
| Preserve unaffected ordering scan positions | Every spill resets every scan → reset only replaced tiers; forced-64 workload **59,007 run rows read → 27,777**, identical rows written/merges/peaks. P1 `p1-5/receipt.md`. | `core/crates/layerfs-content/src/filesystem/references/runs.rs:135` |
| Reuse already-validated boundary nodes during joins | Discard decoded node then read it again → carry it in `JoinSide`; interior multi-level fixture **24 edit node loads → 22**, strict non-root validation retained. P1 `p1-6/receipt.md` §6 corrects the earlier decline and records landing at `360431d10`. | `core/crates/layerfs-content/src/file/edit/tree.rs` (`JoinSide`, `take`) |
| Avoid predecessor navigation for a pure deletion | Unconditional rightmost-payload walk → skip when replacement length is zero; edit-owned node loads **8 → 7** in deletion receipt, provider-demand count unchanged on that fixture. P1 `p1-9/receipt.md`. | `core/crates/layerfs-content/src/file/edit/apply.rs:301` |
| Use one ordered cursor across retained ranges | Restarting range traversal → retained `RangeCursor`; split/assembly fixture **16 nodes read → 13**, identical canonical result. P1 `p1-8/receipt.md`. | `core/crates/layerfs-content/src/file/mapping/read.rs:361` |
| Share mapping pages between comparison and edit construction | Compare and construction repeat page demands → edit-scoped memo; **9 nodes read → 7**, edit load count **10 → 8**, same output. Comparison remains separate to preserve zero-emission on equal edits. P1 `p1-7/receipt.md`. | `core/crates/layerfs-content/src/file/edit/tree.rs:164`, `file/edit/apply.rs:64` |
| Carry the ordering state already read | `touched_serials` then re-find each state → return serial/state pairs; **27,777 rows read → 25,809**. Public API and bound changed: `ordering_bytes / 8` → `/16`; do not hide this compatibility tradeoff. P1 `p1-10/receipt.md`. | `core/crates/layerfs-content/src/filesystem/references/reduce.rs:177`, `filesystem/input.rs:91` |
| Keep a page-width running total | Re-sum all entry widths per append, O(k²) filling → O(k) filling with O(1) append bookkeeping; exact partition preserved, +8 B per live page. Structural proof, no direct CPU counter. P1 `p1-12/receipt.md`. | `core/crates/layerfs-content/src/filesystem/sorted/merge.rs:71`, `sorted/page.rs:106` |
| Assemble whole-file edits in the final canonical buffer | Payload buffer → value buffer → canonical buffer copies reduced to a pre-sized destination moved into output; allocator peak delta **262,328 → 131,826 B**, same root and 65,559 canonical bytes. Applies to edits; construction still has intermediate assembly. P1 `p1-14/receipt.md`. | `core/crates/layerfs-content/src/file/edit/apply.rs:97`, `file/content.rs:126`; construction caveat `file/content.rs:81` |
| Keep open-pack assembled length | Re-sum every existing group for each fit check → O(1) fit check with running total; retained-byte accounting also O(1) per lane. Same pack bytes and boundaries in 12 historical stores. No measured CPU counter. P2 `p2-8/receipt.md`. | `core/crates/layerfs-storage/src/pack/placement.rs:25`, `pack/layout.rs:204` |
| Authenticate reconstructed bytes once | Resolver hashes, then wave hashes again → resolver returns verified ID; wave compares IDs. One full hash pass removed per requested record; tamper rejection retained. No runtime hash counter. Membership's own rehash remains. P2 `p2-6/receipt.md`. | `core/crates/layerfs-storage/src/cas/read.rs:103` |
| Remove redundant ordering-run copy | Copy newest input run before consolidation → adopt it; merge writes only fresh output. Forced-64 **124 → 123 runs**, **25,760 → 25,632 rows written**, **25,809 → 25,681 rows read**. P2 `p2-7/receipt.md`. | `core/crates/layerfs-content/src/filesystem/references/runs.rs:397` |
| Decompress an ordinary group once per read operation | Decompress per record → bounded operation-owned decoded group cache; pipeline fixtures **2 group decodes → 1**. Ceiling checked before a cached answer. P2 `p2-4/receipt.md`. | `core/crates/layerfs-storage/src/encoding/decode.rs:32`, `cas/read.rs:143` |
| Batch referenced-object presence and reuse pool reader | Per-offered-object presence check → wave-level seed; **4,096 logical presence query sets → 8 waves** on nominated reference fixture. One row costs more: D22 **0 → 1**. Pooled delta trials reuse owner's reader. A logical query set may contain several SQL pages. P2 `p2-5/receipt.md`. | `core/crates/layerfs-storage/src/cas/dependencies.rs:50`, `cas/save.rs:44`, `cas/pool_lane.rs:93` |
| Insert object-location rows in SQL batches | One execute per row → SQLite-limit-bounded multi-row insert; **8,191 statements → 72**, **1,023 → 9**, unchanged commits. This restores a mechanism already present in v0.1.6; it is not a new advantage over v0.1.6. P2 `p2-2/receipt.md`. | `core/crates/layerfs-storage/src/sqlite/write.rs:137` |

Two structural migration simplifications are separately visible directly against the old source:

1. **A narrow authenticated batch boundary replaces overlapping read adapters.** v0.1.6 had `ObjectSource` plus `CoreReader` adapting to `ObjectRead` (`crates/layerfs-layerstack-store/src/objects.rs:923` and `:950`). C1 now asks only `AuthenticatedObjects` for canonical bytes (`core/crates/layerfs-content/src/object/access.rs:33`); C2 implements the bridge in `cas/provider.rs`. This removes concrete storage requirements from C1, though the output is owned `Vec<Vec<u8>>`, not a zero-copy borrowed interface.
2. **Finalized owned output and placement-before-assembly.** `FinalizedObject` moves canonical bytes to `FinalizedConsumer` (`core/crates/layerfs-content/src/object/output.rs:92` and later trait definition); C2's bounded `flush_batch` borrows from those owned objects (`cas/save.rs:69`) instead of copying each object. New pack placement decides fit before assembly (`pack/placement.rs:73`), whereas old `objects.rs:2388` explicitly clones the open groups and assembles a candidate. These are source-visible ownership/assembly improvements; no new timing claim is made. Current C2 still builds identity maps and performs duplicate validation because callers can submit duplicates; do not claim all uniqueness work has been removed.

## 3. Things that must not appear in a list of landed wins

- **P1-6 deleting the non-root validation:** that unsafe form was declined. The receipt has an appended correction: a safe `JoinSide` variant DID land, retaining the check and removing the repeated load. Count the safe variant above, never removal of validation.
- **P1-13 fanout-4 ordering merge:** incomplete/reverted, subsequently owner-closed as not delivered/not-big-impact. **P1-15 restart binary search:** also owner-closed as not delivered. Neither is an implemented simplification.
- **P2-1 32 MiB SQLite cache/spill-OFF profile:** measured and declined. **P2-3 EXCLUSIVE locking:** measured and declined because it breaks the multi-store behavior and moved no gated work.
- **32× fewer SQL lookups from 4,096 vs 128 demand width:** explicitly retracted. C1 demand width and SQLite's 128-ID paging are different; old and replacement SQL lookup paging are both 128.
- **One SQL query for all referenced-object checks:** historical counter counts query sets, not every SQL page; use “one paged presence lookup per wave.”
- **No duplicate hashing anywhere:** false. P2-6 removes the read wave's second hash; membership still independently authenticates its reuse path.
- **Zero-copy whole-file construction:** false; the single-buffer improvement is on whole-file edits, not general construction.
- **No pack rewrites:** false. Both trees still rewrite the whole pack BLOB on append.
- **More construction workers:** not implemented and prohibited by the one-worker rule except namespace initialization. A removed thread/channel is a structural simplification, not an automatically favorable throughput result.
- **Shrunken benchmark wall time:** harness preparation reuse/verification improvements are tooling savings, not C1/C2 production simplifications.
- **All optimizations made code smaller:** false; caching, reusable cursors, explicit bounds and checked batching can add production code while removing runtime work.

## 4. Architecture-flexibility implications

Most items changed implementation behind the existing provider/output boundary, which is encouraging for the requested independently optimized-worktree scenario. They show concrete algorithm changes without requiring SQL/pack access in C1. They do **not** prove the Stage 7 substitution matrix: this audit did not compile an unchanged consumer against C1-only, C2-only and paired replacements or test cross-revision stores.

One concrete counterexample matters: **P1-10 deliberately changed a public return type and a resource ceiling.** `touched_serials` returns `(u64, PendingState)` rather than serials; the maximum touched serial count halves for an unchanged tight `ordering_bytes` budget. An integration that reaches into this lower-level reducer can need source edits or behave differently after a seemingly internal optimization. Minimal review remedy: identify the public integration-facing subset, preserve or version its semantics, and explicitly classify lower-level evolution. More public Rust symbols are not automatically a stable supported integration contract.

The read-session and decoded-cache changes preserve per-wave visibility, showing why compatibility must cover call lifetime, ownership and success/error semantics, not signatures alone. Likewise retained canonical partitions/IDs and byte-for-byte physical results in some optimization receipts are useful historical evidence, but an optimized candidate still needs pinned compatibility checks. Older architecture study tables are time-pinned research: `core/docs/architecture/11-optimization-study.md` still describes one-row inserts and calls P2 mechanisms unimplemented at its earlier source pin; current code and later receipts supersede that for this audit.
