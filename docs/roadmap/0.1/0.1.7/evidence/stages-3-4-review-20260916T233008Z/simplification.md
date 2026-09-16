# Simplification audit — Stages 3–4 (issue #168 / #169) implementation

**Status banner:** Independent reviewer artifact. This is the answer to *question 3*
("what can be removed, merged or simplified without weakening the contract") of
`stages-3-4-reviewer-handoff.md`. It is **not** a correctness verdict and **not**
an acceptance result. No product source was modified. No build, test, clippy or cargo
command was run (the parent agent held the verification suite); every claim below is
from static reading of the pinned tree plus read-only `git` inspection.

## 0. Reviewed identity

| Item | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| Branch | `main` |
| HEAD | `91c3a0741fff64e8161d5c1b6e759f347ffbf757` ("docs: state the Stages 3-4 implementation-complete status") |
| Working tree | clean except this untracked evidence directory (`git status --porcelain` reports only `?? docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/`) |
| Candidate components | `core/crates/layerfs-content` (C1), `core/crates/layerfs-storage` (C2), `core/crates/layerfs-telemetry` |
| Reference for provenance | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` → `crates/layerfs-layerstack-store`, `crates/layerfs-content` |
| Reference implementation base | Stage 2 `c38961f2f` / `5e8b8cbc2`, used only for provenance questions |
| Tools used | read, grep, `git log/show/ls-tree/grep`, python3 (read-only scans) |

**Stale-input warning (affects provenance, not the code).** The implementer's report
states its own HEAD as `dfd54fd8e` (stages-3-4-report.md line 121). The reviewed
HEAD is five commits later (`c99192b16`, `fba18606e`, `65f2a402d`,
`275c81ab7`, `91c3a0741`). Report section 1 (lines 84–91) still describes the
superseded design — `ExtentBuilder` "wrapped by `EditFrontier`", "stored-node
**split/concat reuse** is not implemented" — but `EditFrontier` exists nowhere in
the reviewed tree (only stale `core/target` object files match it), while report
section 3 line 261 lists split/concat reuse as covered by `edit_localized`. The two
sections contradict each other; the understating one is stale. Nothing in this audit
depends on the report's text.

---

## 1. Ranked list (highest value first)

| # | Finding | Class | Est. benefit (ESTIMATE) | Contract risk |
| --- | --- | --- | --- | --- |
| 1 | Per-object full canonical copy in every save wave (`cas/save.rs:50`) | duplicated payload owner | 1 full canonical copy removed per inserted object | none |
| 2 | C1 localized-edit node round trip: encode→hash→**decode→re-encode→hash** (`edit/tree.rs:225,106`) | repeated encode/hash of drafts; **worse than v0.1.6** | -1 encode -1 hash per emitted mapping page | none |
| 3 | Clone-per-group fit test in placement + dead `layout::fits` (`pack/placement.rs:84`, `pack/layout.rs:210`) | repeated full-vector copy, O(n^2) | ~15 group copies + ~0.7 MiB memcpy per full pack | none |
| 4 | Dead-code sweep: 25 items with no src/test/example caller (section 3) | dead API/config/constants | about -200 lines | none |
| 5 | `emit_file_state` implemented three times (`edit/tree.rs:629` identical to `mapping/build.rs:327`) | duplicated responsibility | -17 lines | none |
| 6 | Pooled-leaf read: one SQL point query per row + one cached-value clone per row (`pool/read.rs:266,73`) | point loop behind a batch API, duplicated owner | up to 100 queries to 3; up to 100 copies to 1 per leaf | none |
| 7 | Editing scaffolding: `assemble_final`/`assemble_inner` wrapper, per-iteration `FileState` rebuild, empty `if` (`edit/apply.rs:117,134,152,283`) | wrapper with no responsibility, dead state | about -25 lines | none |
| 8 | Pooled selection: three identical FULL fallbacks, discarded `policy` parameter (`cas/owner.rs:489,502,512,177`) | dead config path, copy-paste | about -20 lines | none |
| 9 | Value-group build: body cloned + length recomputed + body retained only for `.len()` (`pool/value_group.rs:48,56`; `cas/owner.rs:612`) | double payload owner | about -15 KiB copy and -15 KiB retained per group | none |
| 10 | Small merges: `tree::coalesce` to `concat::coalesce_adjacent`; `rebuild_leaf` duplicate store; redundant `entry[12..16]` conjunct; `plan_lane` double record framing; `Availability` two sets to one; demand linear scan | rule duplication | about -25 lines, -1 record copy | none |

**Cross-cutting provenance result (required check).** Every symbol in the dead-code
sweep (section 3) is **absent from v0.1.6** (`git grep <name> 44cf748 -- crates/`
returns 0 files for all of them) and absent from Stage 2. They are therefore **new Core
additions**, not inherited reference surface that must be preserved. Findings 1-3 were
traced against the reference explicitly below: **finding 2 is a regression against
v0.1.6**; findings 1 and 3 are pre-existing patterns this batch did **not** introduce
(finding 3 is already an improvement on the reference).

---

## 2. Material findings in detail

### F1 — Every save wave clones the whole canonical object it is about to store

| | |
| --- | --- |
| **Location** | `core/crates/layerfs-storage/src/cas/save.rs:50` (`let object = objects[index].clone();`), consumed at `cas/save.rs:68` into `cas/owner.rs:324` |
| **Current behaviour and work cost** | `flush_batch` clones the `FinalizedObject` for **every** wave member. `FinalizedObject` owns `canonical: Vec<u8>` (`object/output.rs:61-67`), so each clone is a heap allocation plus a full copy of the canonical object: 32 KiB per chunk object, up to `CANONICAL_LIMIT` = 16 MiB for a singleton (`policy.rs:53`), up to 1 MiB for a default whole-file object. A 512-object wave of chunk objects copies about 16 MiB per wave, and `offer` never takes ownership of those bytes. |
| **Smallest change** | Take a borrow: `pub fn offer(&mut self, object: &FinalizedObject, advisory: &[ObjectId], availability: &mut Availability)` (`cas/owner.rs:324`) and drop the clone (`let object = &objects[index];` at `cas/save.rs:50`). `offer` only reads `id()`, `role()`, `canonical()`, `canonical_len()` (`cas/owner.rs:333, 364-369, 385, 398-404`). |
| **Callers affected** | `cas/save.rs:68` only. `offer` is crate-internal: `cas/mod.rs:9` declares `mod owner;` privately and re-exports only `OutcomeCounters, PoolCounters`. |
| **Invariant preserved** | Availability validation, delta selection, exact CAS comparison, placement and accounting are untouched. The bytes are still copied once, into the framed group record at `cas/owner.rs:363`. |
| **Est. benefit (ESTIMATE)** | One canonical-size allocation and memcpy removed per newly inserted object per wave; -1 line net. |
| **Verification case** | `cas_reuse`, `cas_roundtrip`, `core_pipeline`, `delta_payload`, `metadata_pool` must stay green with unchanged roots and counters. No allocation-count oracle exists; the dead `FinalizedObject::canonical_capacity` (`object/output.rs:117`) suggests one was intended. |

```text
before                                   after
for index in 0..objects.len()            for index in 0..objects.len()
  object = objects[index].clone()  -->     object = &objects[index]
  |  (alloc + memcpy canonical)            |  (borrow, no alloc)
  owner.offer(object, ...)                 owner.offer(object, ...)
```

### F2 — C1 localized edits encode, hash, decode, re-encode and re-hash the same page

| | |
| --- | --- |
| **Location** | `layerfs-content/src/file/mapping/build.rs:357` (`emit_node`: `encode_node`), `object/output.rs:74` (`FinalizedObject::new`: `ObjectId::for_bytes`), `file/edit/tree.rs:225` (`DeferredSink::accept`: `decode_node_with_context`), `file/edit/tree.rs:106-107` (`hold_node`: `encode_node` + `ObjectId::for_bytes`), `file/edit/tree.rs:185` (`commit_node`: decode again) |
| **Current behaviour and work cost** | On the stored-tree edit path (`edit/apply.rs:257` wraps `DeferredSink` around the RHS `ExtentBuilder`) each emitted mapping page is encoded and hashed by the builder, **decoded** by the sink purely to recover `logical_len/extent_count/level`, then **encoded and hashed a second time** by `hold_node`, then **decoded a second time** by `commit_node`. The sink already holds the identity and bytes in `object.id()` / `object.canonical()` and discards both. Pages are at most `MAX_NODE_OBJECT_BYTES` = 8 KiB (`mapping/types.rs:19`); the cost is one extra encode + one extra BLAKE3 + one extra decode per page an edit creates. A large replacement rebuilds about `replacement_len / 2 MiB` leaf pages (128 extents x 16 KiB chunks), so a 1 GiB replacement pays about 512 redundant encode+hash (+decode) cycles. |
| **Provenance — a regression against v0.1.6** | The reference does it once. `crates/layerfs-content/src/file/rope/edit.rs` at `44cf748`, `emit_node`: `let canonical = encode_node(&node)?; let id = store.put(&canonical)?;` then builds the summary from the **in-memory node** — no decode, no re-encode. `DeferredFileObjects::put_sealed_node(&mut self, canonical: &[u8])` likewise takes already-encoded bytes. |
| **Smallest change** | Add a canonical-bytes entry point to `EditObjects`, e.g. `pub fn hold_canonical(&mut self, id: ObjectId, canonical: Vec<u8>, node: &ExtentNode) -> ContentResult<NodeSummary>`; keep the single decode in the sink for the summary and pass `object.id()` plus the moved bytes instead of re-deriving them. Removing the remaining decode as well needs the builder to hand over the summary (larger, optional). |
| **Callers affected** | `file/edit/tree.rs:217-231` (`DeferredSink`), `tree.rs:105` (`hold_node`, also called from `emit_leaf`/`emit_branch` at `tree.rs:565,602`), `edit/apply.rs:257`. `hold_node` stays for the tree-local callers. |
| **Invariant preserved** | The identity-collision check `if prior != &canonical { IdentityMismatch }` (`tree.rs:123-126`) is kept; the deferred charge/limit path (`tree.rs:108-122`) and reachability-only emission are untouched. |
| **Est. benefit (ESTIMATE)** | -1 encode -1 hash per emitted page (-1 decode optional); about +3/-6 lines. |
| **Verification case** | `edit_localized` (5 cases), `edit_reference` (v0.1.6 oracle root/partition equality), `edit_batch::every_object_the_edit_emits_is_reachable_from_its_root`; assert `EditCounters.nodes_created` (`tree.rs:131`) and `nodes_read` unchanged. |

### F3 — Placement clones the whole open pack's groups to test one append

| | |
| --- | --- |
| **Location** | `layerfs-storage/src/pack/placement.rs:81-90` (the `fits_open` block), open-pack state at `placement.rs:16-19`; the unused exact predicate at `layout.rs:210` |
| **Current behaviour and work cost** | For every group placed into a lane's open pack: `let mut candidate = open.groups.clone(); candidate.push(group.clone()); assembled_length(lane, &candidate)`. Both clones copy owned byte buffers (`EncodedGroup.bytes`) and `assembled_length` then walks the candidate again, so the cost is O(n^2) in groups per pack with n about `PACK_LIMIT / GROUP_TARGET` = 256 KiB / 48 KiB = 5 for the ordinary/native lanes (`cas/owner.rs:27,349-354`, `pack/layout.rs:111`): about 15 group copies and about 0.7 MiB memcpy per fully filled pack (ESTIMATE). Whole-file, pooled-metadata and singleton lanes seal after one record and are unaffected. `layout::fits` is the canonical form of exactly this test and has **no caller anywhere**; the check was inlined instead. |
| **Provenance — pre-existing, and already improved** | v0.1.6 `crates/layerfs-layerstack-store/src/objects.rs:2390` (`append_open_pack`) has the same `entry.groups.clone()` pattern and goes further, assembling the whole candidate pack (`pack::assemble_version`) before the fit test. Core already reduced that to a length-only measurement. The remaining clone is **not** something this batch introduced; removing it is new work, not a re-credit. |
| **Smallest change** | Track the assembled length in `OpenPack` (`placement.rs:16`) as groups are pushed and test `open.assembled_len + directory(lane) + group.body_size(lane)? <= lane.pack_limit()`; alternatively keep `assembled_length` on the existing vector and add only the incoming group's contribution. Then delete the dead `layout::fits`. |
| **Callers affected** | `cas/owner.rs:718` (`seal_group`) and `cas/owner.rs:592` (value groups). `retained_bytes` (`placement.rs:61`) keeps using `assembled_length`. |
| **Invariant preserved** | Identical accept/new-pack decision; `assemble()` remains the single authority for the bytes written and for the pack-length check (`pack/assemble.rs:179-186`). |
| **Est. benefit (ESTIMATE)** | About 15 group clones and 0.7 MiB memcpy per filled pack removed; -4 lines (the dead predicate) net. |
| **Verification case** | `pack_locator`, `physical_formats`, `cas_roundtrip`, `core_pipeline`; assert unchanged `OutcomeCounters.packs_created` and group/record ordinals for a fixed workload. |

### F4 — `emit_file_state` exists three times, two of them byte-identical

| | |
| --- | --- |
| **Location** | `layerfs-content/src/file/edit/tree.rs:628-645` (caller `tree.rs:159`), `layerfs-content/src/file/mapping/build.rs:326-343` (same code, exported at `mapping/mod.rs:10`), `layerfs-content/src/file/edit/finish.rs:40-54` (a wrapper adding `EmittedRoot`, legitimate) |
| **Current behaviour and work cost** | `tree::emit_file_state(consumer, mapping: NodeSummary)` and `mapping::emit_file_state(consumer, mapping_root: NodeSummary)` are the same 17 lines with a renamed parameter (build `FileState`, wrap in `FinalizedObject::new(FileState, encode_file_state(state)?)`, `with_references(vec![mapping.id])`, accept, return id). No line saving from keeping both, and a real hazard: a file-state grammar change must be made twice, and `tree.rs` reaches for `crate::file::mapping::profile_id()` inline where `build.rs` uses `profile_id()`. |
| **Smallest change** | Delete `tree.rs:628-645` and have `EditObjects::finish` (`tree.rs:157-161`) call `crate::file::mapping::emit_file_state(self.consumer, mapping)`, already in scope via `tree.rs:18-21`. |
| **Callers affected** | `tree.rs:159` only. |
| **Invariant preserved** | Byte-identical state object implies identical root identity; roles and references unchanged. |
| **Est. benefit (ESTIMATE)** | -17 physical lines, one grammar authority. |
| **Verification case** | `edit_localized`, `edit_reference`, `file_complete` (root equality with a fresh construction). |

### F5 — Pooled-leaf read: one point query and one clone per row

| | |
| --- | --- |
| **Location** | `layerfs-storage/src/encoding/pool/read.rs:266` (`pool::group_for(connection, row.ordinal)` inside the per-row loop of `leaf_canonical`), `pool/read.rs:72-74` (`Ok(values.clone())` on the cache hit), `sqlite/pool.rs:74-120` (`group_for` is one `query_row` with `ORDER BY first_ordinal DESC LIMIT 1`) |
| **Current behaviour and work cost** | A pooled leaf has at most `MAXIMUM_LEAF_ROWS` = 100 rows. `leaf_canonical` issues **one point query per row** to find the covering group, then `group_values` **clones the whole cached group** (up to `VALUES_PER_GROUP` = 165 x 73 B, about 12 KiB) on every call to return a single value (`read.rs:269-273`). Typical leaves reference 1-3 distinct groups, so about 100 queries and about 100 clones (about 1.2 MiB copied) replace 1-3 of each. `group_for` is a pure function of the ordinal for a fixed catalogue, and appending later groups cannot change the row covering an existing ordinal, so caching within a leaf read is safe. |
| **Smallest change** | In `leaf_canonical`, remember the last `(first_ordinal, ValueGroupRow)` (or a small map keyed by `first_ordinal`) and reuse it while `row.ordinal` falls inside it; add `group_values_into(&mut self, ..., out: &mut Vec<[u8; INODE_VALUE_BYTES]>)` so the cached path appends instead of cloning. `pool::catalogue(connection, from)` (`sqlite/pool.rs:123`) already exists if a bulk form is preferred. |
| **Callers affected** | `pool/read.rs:250` (`leaf_canonical`), reached from `delta/read.rs:105-125` (`resolve_charged`, `InodeLeaf` branch). `PoolIndex::find` (`pool/index.rs:198-201`) already avoids re-reading a group per candidate — the pattern to mirror. |
| **Invariant preserved** | Every group is still authenticated against its catalogue digest before any ordinal is trusted (`value_group::authenticate`, `read.rs:88`); the visibility ceiling still applies (`read.rs:75-80`); no trial decode or fallback is introduced. |
| **Est. benefit (ESTIMATE)** | Up to 100 SQL statements to at most 3 per leaf; about 1.2 MiB copying removed per leaf read; about +6/-6 lines. |
| **Verification case** | `metadata_pool`, `metadata_window`, `metadata_pool_index`, `metadata_chain`, `metadata_fingerprint_collision`; identical reconstructed bytes and unchanged `ChainCounters`. |

### F6 — Editing scaffolding that does no work

| | |
| --- | --- |
| **Location** | (a) `file/edit/apply.rs:116-125` and `:127-134` (`assemble_final` wraps `assemble_inner`, which immediately does `let _ = assemble;` at `:134`); (b) `apply.rs:152-155` (an `if` whose body is only a comment); (c) `apply.rs:214, 283-289, 291, 302` (a `FileState` rebuilt on every edit iteration whose only surviving consumer is its `logical_len`, which equals `summary.bytes`); (d) `apply.rs:324-331` (`let last = children.last()...; let _ = last;` where `summaries.last()` two lines later repeats the check). |
| **Current behaviour and work cost** | (a) two function heads, one `#[allow(clippy::too_many_arguments)]` and a scope handle that is discarded; (b) a no-op branch; (c) a five-field struct literal plus a `profile_id()` lookup per edit whose value is overwritten before it is read (the loop always runs at least once because the empty stream returns at `apply.rs:53`); (d) a redundant binding and a duplicated error path. |
| **Smallest change** | (a) delete `assemble_inner` and move the body into `assemble_final`'s closure as `scope.run(|_| { ... })`; (b) match `Segment::Replace { index, len, .. }`; (c) delete the in-loop `FileState` literal, take `let (_, mut summary) = read_state(...)` at `apply.rs:214` and read `summary.bytes` at `:291, :302`; (d) drop the `last` binding and keep `summaries.last().ok_or(...)`. |
| **Callers affected** | `apply.rs:84` (`assemble_final`); the rest is internal to `replace_chunked`. `read_state` (`tree.rs:611`) keeps its signature, so no public API changes. |
| **Invariant preserved** | Same arithmetic, same error values (`LengthMismatch`, `InvalidRange`, `InvalidRecord("empty branch")`), same segment order; "the discarded base range is deliberately not read" still holds (the comment moves above the arm). |
| **Est. benefit (ESTIMATE)** | About -25 physical lines; one `profile_id()` and one struct build per edit removed. |
| **Verification case** | `edit_transitions`, `edit_batch`, `edit_bounds`, `edit_model`, `edit_localized`, `edit_timing`. |

### F7 — Pooled selection: three copies of one fallback, and a discarded parameter

| | |
| --- | --- |
| **Location** | `cas/owner.rs:489-498`, `:502-511`, `:512-521` (three identical ten-line `EncodedRecord { lane: Ordinary, record: full, ..., base: None }` returns), `cas/owner.rs:177` (`let _ = policy;`) with the parameter declared at `cas/owner.rs:155`, and `cas/owner.rs:626-636` (rebuilds with `skip/take` the chunking `write_value_groups` already computed at `:576`) |
| **Current behaviour and work cost** | Three textually identical fallback constructions that must be kept in sync; a `StoragePolicy` parameter accepted and explicitly dropped (the accepted policy is already reflected in the `StorageCapacities` passed alongside, and `Store::begin_save` passes both at `cas/store.rs:185-190`); an O(groups x fresh) `skip/take` walk where `fresh.chunks(VALUES_PER_GROUP)` is the exact partition used to build the groups. |
| **Smallest change** | Hoist the fallback into one tail (`let trial: Option<(ObjectId, Vec<u8>)> = ...` then one final `match~/`return`). Delete the `policy` parameter from `MutationOwner::acquire` and its argument at `cas/store.rs:187`. Replace the `skip/take` loop with `for (_, chunk) in fresh.chunks(VALUES_PER_GROUP).zip(&built)`. |
| **Callers affected** | `select_pooled` only from `select_record` (`owner.rs:385-387`); `acquire` only from `Store::begin_save`. `MutationOwner` is crate-internal (`cas/mod.rs:9,14`). |
| **Invariant preserved** | Same policy outcome in every branch (no base / no program / program not smaller all store FULL); `full_leaves`, `delta_leaves` and `trials` unchanged; ordinals still assigned in first-encounter order. |
| **Est. benefit (ESTIMATE)** | About -20 lines; one dead parameter and one redundant iteration removed. |
| **Verification case** | `metadata_pool`, `metadata_window` (full-leaf vs delta-leaf counters), `metadata_pool_index`; `policy_capacity` for the acquire path. |

### F8 — Value-group build keeps two copies of every body

| | |
| --- | --- |
| **Location** | `encoding/pool/value_group.rs:46-64` (`body` then `let mut encoded = body.clone()` at `:48`, `decoded_length: framed_length(&records)?` at `:56`), `BuiltGroup.body` (`value_group.rs:21`) read only at `cas/owner.rs:612` |
| **Current behaviour and work cost** | The framed body (about 16 KiB) is allocated, hashed for the catalogue digest, then **cloned once more** into `encoded`; on the raw-codec path both copies survive into `BuiltGroup`. `decoded_length` is recomputed by re-walking `records` although `frame_group_bounded` returns exactly that length (`pack/assemble.rs:40-74`), so `body.len()` is the same number. |
| **Smallest change** | Build `EncodedGroup.bytes` directly from `compress_group_body`'s `Option` (moving `body` in when it is `None`), take `decoded_length: body.len()` before the move, and replace the `body` field with its length for the transaction accounting. |
| **Callers affected** | `cas/owner.rs:581-590` (only constructor), `owner.rs:597-613` (only `body` reader). |
| **Invariant preserved** | The digest is still taken over the **uncompressed** framed body before compression, which is what the read path authenticates (`pool/read.rs:88`); the retained-compression rule of `compress_group_body` (`codec.rs:613-627`) is unchanged. |
| **Est. benefit (ESTIMATE)** | One 16 KiB clone and one 16 KiB retained copy removed per written group; -2 lines net. |
| **Verification case** | `metadata_pool`, `metadata_window` (group digests and readback), `physical_formats`. |

### F9 — Small merges (each independently verifiable)

| Item | Location | Change | Est. (ESTIMATE) |
| --- | --- | --- | --- |
| One coalescing rule, not two | `file/edit/tree.rs:259-284` vs `file/edit/concat.rs:12-29` | `tree::coalesce` re-implements the "same payload + contiguous source offset" rule that `coalesce_adjacent` already owns; delegate per element | -10 lines, one authority for the canonical partition rule |
| Duplicate store in `rebuild_leaf` | `object/inode_leaf.rs:377` vs `:392` | `subtree_bytes` is assigned the identical expression twice; `:392` and `let _ = prefix;` (`:393`) are dead | -3 lines |
| Redundant directory-flag conjunct | `pack/layout.rs:326` | `entry[12..16] != [0,0,0,0] && entry[13..16] != [0,0,0]` — the second clause implies the first, so the check is exactly "reserved bytes 13..16 are zero"; removing the *implied* conjunct weakens nothing and makes the rule readable | -1 line |
| Double record framing | `encoding/full.rs:183` and `:192` | `record::encode(WholeFile, ...)` and `record::encode(Singleton, ...)` produce **byte-identical** records (`record.rs:63-78` and `record_width` at `record.rs:38-46` share one layout), so encode once and reuse; only the assembled contribution differs | -1 full record copy (up to 1 MiB) on the oversized path, -2 lines |
| Two sets for one fact | `cas/dependencies.rs:20-42` | `present: BTreeMap<ObjectId,()>` and `inserted: BTreeSet<ObjectId>` are only ever queried together via `known()`; one `BTreeSet` serves both | -6 lines, one structure for up to 512 ids per wave |
| Demand lookup by index | `file/mapping/read.rs:109-114` | `distinct.iter().position(...)` per demand (up to 128 demands x 32 distinct) plus a "demand index" error path; store the index in `Demand` when pushing (`:65-81`) | -4 lines, removes a linear scan and an error branch |
| Pooled leaf decoded twice per store | `cas/owner.rs:441` then `:483` (`pooled_body` re-decodes at `inode_leaf.rs:301`) | `select_pooled` decodes the leaf, then `pooled_body` decodes (and re-encodes for canonical-form validation) the same bytes again; pass the decoded `InodeLeaf` | -1 full leaf decode plus re-encode per pooled leaf |
| Fresh `PoolReader` per base | `cas/owner.rs:681` | A new reader with a cold pack cache is built inside `pool_base` while the owner already holds `self.pool_reader` (`owner.rs:119`) | -1 allocation and a cold pack cache on the pooled-delta path |
| Three dead `StorageCapacities` fields | `policy.rs:241,245,247`, set at `:290,292,295` | `whole_file_envelope`, `chunk_canonical_limit`, `chunk_frame_limit` are written and never read in `src/`; the effective chunk limits come from the private constants in `encoding/codec.rs:94-96`. Delete the mirrors or read them instead of the constants — keeping both is a trap. | -6 lines |

---

## 3. Dead-code inventory (all verified absent from v0.1.6)

Method: every `pub`/`pub(crate)` item defined under `core/crates/*/src` was
matched against all `core/**/*.rs` with **comments stripped** (prose in doc comments
otherwise produces false "uses"), then each candidate was confirmed with a literal
grep for its call syntax.

| Location | Item | Callers (src/tests/examples) | Est. lines (ESTIMATE) | Note |
| --- | --- | --- | --- | --- |
| `content/src/file/view.rs:113-171` | `FileView::walk_extents` + private `descend` | 0/0/0 | 59 | A second extent-tree traversal duplicating `file/mapping/read.rs:166-214`; never called |
| `content/src/file/edit/input.rs:386-389` | `Plan::base_end` | 0/0/0 | 4 | Nothing reads the base cursor after the plan ends |
| `content/src/object/id.rs:31-45` | `ObjectId::for_reader` | 0/0/0 | 15 | Streaming hash path never used; `for_bytes` is the only one |
| `content/src/object/output.rs:116-119` | `FinalizedObject::canonical_capacity` | 0/0/0 | 4 | Capacity accounting nothing consumes |
| `content/src/object/predecessor.rs:91-95` | `AdvisoryPredecessors::to_ids` | 0/0/0 | 5 | `ids()` is the used accessor |
| `content/src/object/inode_leaf.rs:282-285` | `leaf_layout` | 0/0/0 | 4 | Exported at `object/mod.rs:22`; despite its doc it fully decodes *and re-encodes* the leaf to return a row count |
| `content/src/policy.rs:26-27` | `MINIMUM_WHOLE_FILE_BYTES` | 0/0/0 | 2 | Superseded by `Representation` dispatch |
| `storage/src/encoding/codec.rs:629-632` | `group_body_parameters` | 0/0/0 | 4 | Exposes constants `compress_group` already owns |
| `storage/src/encoding/delta/read.rs:70-75` | `Resolver::resolve` | 0/0/0 | 6 | Only `resolve_dependency`/`resolve_at` are used |
| `storage/src/encoding/delta/read.rs:208-212` | `Resolver::note_edge` | 0/0/0 | 5 | `edges`/`max_depth` are set directly in `resolve_charged` |
| `storage/src/encoding/delta/record.rs:19-20,237-240` | `COMPACT_DROP` + `compact_framing_is_dropped` | 0/0/0 | 6 | Self-referential pair; `WHOLE_FILE_COMPACT_DROP` is used directly elsewhere |
| `storage/src/encoding/pool/delta.rs:14-15` | `MATCH_BUDGET_BYTES` | 0/0/0 | 2 | Duplicate of `policy::METADATA_MATCH_BUDGET_BYTES` (`policy.rs:112`), which is what `owner.rs:500` uses |
| `storage/src/encoding/pool/leaf.rs:46-52` | `is_pooled` | 0/0/0 | 7 | Record interpretation is driven by the locator role |
| `storage/src/encoding/pool/leaf.rs:119-130` | `body_width` | 0/0/0 | 12 | Parallel to `physical_length`; unused |
| `storage/src/encoding/pool/read.rs:283-286` | `group_identity` | 0/0/0 | 4 | Thin alias of `ObjectId::for_bytes` |
| `storage/src/encoding/pool/read.rs:288-294` | `value_ordinal` | 0/0/0 | 7 | Byte-for-byte duplicate of the live private `PoolIndex::fingerprint` (`pool/index.rs:249-254`); its doc names a caller that does not exist |
| `storage/src/pack/layout.rs:99-105` | `records_per_group` | 0/2/0 | 7 | Only `tests/physical_formats.rs:100-101` asserts constants against literals; the enforced rule lives in `build_group` |
| `storage/src/pack/layout.rs:115-118` | `directory_is_starts_only` | 0/0/0 | 4 | — |
| `storage/src/pack/layout.rs:209-212` | `fits` | 0/0/0 | 4 | Canonical form of the predicate inlined at `placement.rs:86` (see F3) |
| `storage/src/pack/placement.rs:55-58` | `open_pack_id` | 0/0/0 | 4 | — |
| `storage/src/policy.rs:67,69` | `WHOLE_FILE_RAW_LIMIT`, `WHOLE_FILE_FRAME_LIMIT` | 0/0/0 | 4 | Default-cutoff duplicates of derived capacities |
| `storage/src/policy.rs:87,89,93` | `POOLED_LEAF_ROWS_LIMIT`, `POOLED_LEAF_PREFIX`, `POOLED_PHYSICAL_ROW` | 0/0/0 | 6 | C1 owns the real values (`inode_leaf.rs:24-32`); these mirrors are unfenced |
| `storage/src/policy.rs:117-118` | `METADATA_INDEX_BYTES` | 0/0/0 | 2 | The 32 MiB index-byte budget is declared and never enforced or compared (see 4.1) |
| `storage/src/policy.rs:323-332` | `check_canonical_limit` | 0/0/0 | 9 | Public validator nothing calls; canonical limits are enforced at `full.rs:104-110` and in C1 |
| `storage/src/encoding/full.rs:37-43` | `EncodedRecord::width` | 0/0/0 | 6 | Callers use `record.record.len()` |
| `storage/src/cas/owner.rs:422-425` + `cas/store.rs:441-447` | `depth_cache_entries` pair | 0/0/0 | 11 | The two functions reference each other and nothing else; no external caller at all |
| `content/src/file/edit/apply.rs:152-155` | comment-only `if` | — | 3 | See F6(b) |

**Total dead-code estimate: about 200 physical lines across 26 sites (ESTIMATE).**
None of it is trust-boundary validation, CAS comparison, candidate quality,
backpressure, atomicity, visibility or cleanup: no listed deletion performs a check,
holds a queue bound, or orders a write. The two most subtle items are
`records_per_group` (a lane property whose only "proof" is a test restating the
constant; the behaviour is enforced in `build_group` and stays) and its neighbour
`body_limit`, which **is** live at `layout.rs:340,347` and must stay.

---

## 4. Incidental observations (found while tracing; outside the simplification scope)

Recorded because the handoff asks for construction/save/read/failure tracing and
because they affect other review questions. All are **source-derived**, not executed:
no build or test was permitted in this session.

1. **`Candidates::live_bytes` reports a constant.** `encoding/delta/candidates.rs:104-106`
   is `std::mem::size_of_val(&self.slots) + std::mem::size_of_val(&self.references)`.
   Both fields are `Box<[T]>`, so the inferred `T` is the box (a fat pointer),
   not the slice: the function returns 32, not the roughly 128 KiB that the const
   assertion at `candidates.rs:35-44` bounds and `INDEX_BYTES` declares. It flows
   to `MutationOwner::candidate_index_bytes` (`owner.rs:418`) and
   `SaveOperation::candidate_index_bytes` (`store.rs:434`), which tests and
   examples read for the memory ledger. The sibling index maintains its total
   explicitly (`pool/index.rs:52-54`). Smallest fix:
   `size_of_val(&*self.slots)` or `self.slots.len() * size_of::<Option<Entry>>()`.
   No test asserts `live_bytes() <= INDEX_BYTES`, so nothing catches it.
2. **`SaveOutcome.chain` is the last chain read, not the save's work.** Every
   `Resolver::resolve_charged` begins with `*self.counters = ChainCounters::default()`
   (`delta/read.rs:103`). `MutationOwner` shares one `ChainCounters` across the
   whole save (`owner.rs:113`, passed at `owner.rs:310,395`), so each resolution
   overwrites the previous tally and `finish` copies the survivor (`owner.rs:805`).
   Counting is correct *within* one selection (`select.rs:248-266` reads it right
   after `acquire`), but the reported `chain` field describes only the final chain.
3. **`PoolReader.decoded_work` is reset per chain only on the read path.**
   `begin_chain` (`pool/read.rs:48`) is called only from `leaf_canonical`
   (`pool/read.rs:258`), yet the owner reuses one reader for a whole save
   (`owner.rs:119`) through `sync_pool_index` and `PoolIndex::find`. On the write
   path `METADATA_DECODED_WORK_LIMIT` (32 MiB, `policy.rs:102`) is therefore a
   per-*save* budget. A cold-start sync of the retained window is about 795 groups x
   27.5 KiB, about 21.9 MiB by `METADATA_INDEX_VALUES` / `VALUES_PER_GROUP`, so the
   budget can be exhausted and fail a save with `Integrity("metadata decoded work")`.
   Source arithmetic only; I could not execute it.
4. **The C1 frontier cap has no test, and the reference pruned instead of failing.**
   `EDIT_DEFERRED_LIMIT` (`tree.rs:31`) appears in no test or example
   (`grep -rn EDIT_DEFERRED_LIMIT crates/*/tests crates/*/examples` returns nothing),
   so `BoundedCapacityExceeded("edit.deferred_nodes")` is unexercised. The same
   number exists in v0.1.6 (`FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES`), but there a
   4 MiB prune threshold (`DEFERRED_FILE_PRUNE_BYTES`, `prune_to`) kept the set far
   below it and replacement pages were sealed through `put_sealed_node` as they were
   produced. Core correctly removed the prune pass (an unreachable node is simply
   never emitted, `tree.rs:163-170`) but routes RHS pages into the deferred set until
   `commit`, so a replacement whose own mapping exceeds about 8 MiB now fails where
   the reference completed. Treat as a **supported-envelope / missing-evidence** item,
   not a simplification: do not restore the prune pass to "fix" it.
5. **Report text is stale relative to the reviewed HEAD** (see section 0), which matters
   if the parent's verdict quotes report section 1.

---

## 5. Looks removable but is NOT

| Item | Why it must stay |
| --- | --- |
| `cas/membership.rs:30-47` `reuse_or_collide` + `stored_canonical` (reads and byte-compares the stored object on every hit) | This **is** the exact CAS comparison, explicitly protected by the hard invariants. |
| `cas/dependencies.rs:49-72` `Availability::validate` and its second pass | Trust-boundary check for direct logical references; it is what makes a stored record complete. Merging its two containers (F9) is fine; deleting the re-check is not. |
| `encoding/delta/candidates.rs` (`signature`, slot/reference tables, overlap >= 2) and `delta/select.rs:295-331` (`acquisition`/`eligible`) | Candidate quality: the candidate universe, first-eligible rule and one-trial rule are frozen. Do not batch the point lookups here either — at most `MAXIMUM_ADVISORY_PREDECESSORS` = 4 ids, and the ordered refusal counters depend on the sequential walk. |
| `cas/batch.rs` (`PendingBatch` count *and* byte bounds) and `cas/owner.rs:773-790` (`maybe_commit`) | Backpressure and the bounded-transaction split. Removing either changes the memory contract. |
| `cas/owner.rs:796-853` (`finish_inner` rollback/commit/watermark) and `sqlite/schema.rs:225-256` (`retained_pack_ceiling` / `advance_retained_pack_ceiling`) | Atomicity and visibility: the watermark advances with the packs or not at all. The empty-transaction rollback branch is load-bearing. |
| `cas/finish.rs:12-25`, `owner.rs:856-889` (`abandon`/`mark_terminal`/`quarantine`, one cleanup attempt) and `sqlite/cleanup.rs` | Cleanup and the no-retry/no-guess rule; `Drop~ calling `abandon` once is deliberate. |
| The near-duplicate `compress`/`compress_prefix` and `decompress`/`decompress_prefix`/`decompress_group` bodies in `encoding/codec.rs` | They look like copy-paste (a narrow shared "configure context" helper is defensible), but the differences are load-bearing: `refPrefix` must be cleared on success *and* failure, and `decompress_group` deliberately omits `d_windowLogMax`. Do not merge these unsafe blocks for line count. |
| `content/src/file/content.rs:194-208` (`construct_stream` cutoff probe buffer) | Looks like a whole-input collector; it is the frozen bounded threshold probe (`read_at_most`, at most `cutoff` bytes) that the small-to-large route requires. |
| `content/src/file/edit/apply.rs:136-143` (one allocation for the whole final object) | Mandated: "Small to small: construct the final whole object, targeting one final allocation". |
| `content/src/file/edit/input.rs:184-222` (`Replacements`) and `PlanReader` (`apply.rs:360-447`) | `Replacements` is the only shipped concrete `EditSource` and the external tests' input path; `PlanReader` is the streaming adapter the large-file routes need. Neither holds a second edit list (`Plan` is a cursor). |
| `#[derive(Clone)]` on `FinalizedObject` (`object/output.rs:60`) | Looks removable once F1 lands, but external tests clone it. Remove the *internal* clone, not the derive. |
| `content/src/object/inode_leaf.rs:269-272` (decode re-encodes to prove canonical form) and `InodeLeaf::validate` | Canonical-form and trust validation. F9's "decode once per store" only removes a *duplicate* decode. |
| `cas/store.rs:169-179` (`pool_index_entries`/`pool_index_bytes`), `store.rs:450-461` (`abort`), `store.rs:510-512` (`take_failure`), `pool/index.rs:257` (`value_of`) | Callers are tests/examples only, but these are the declared public and measurement surface named by the verification contract (`examples/measure_edits.rs`, `measure_pooled.rs`) and the memory ledger. Not dead. |
| `content/src/file/edit/tree.rs:167-202` (`commit`/`commit_node` reachability walk) | This **replaced** v0.1.6's deferred prune pass (`DeferredFileObjects::prune_to`/`collect_state`/`collect_mapping`, `objects.retain`). It is the already-made simplification: do not re-propose a prune pass or credit it to this batch again. |

---

## 6. What I could NOT verify

- **No execution at all.** Per the assignment, no cargo, rustc, build, test or clippy
  was run (the parent held the suite). Every work-cost number is a **source-derived
  ESTIMATE**, never a measurement. I produced no allocation counts, timings or byte
  counters.
- **The `size_of_val` defect (4.1) is not executed.** It rests on Rust semantics plus
  the field types at the call site; a one-line assertion that
  `candidates.live_bytes() <= candidates::INDEX_BYTES` would settle it. That test does
  not exist (`INDEX_BYTES` appears only inside `candidates.rs`).
- **The `PoolReader.decoded_work` accumulation (4.3)** is arithmetic over declared
  constants; I did not build the roughly 795-group catalogue needed to demonstrate it.
- **The frontier-cap consequence (4.4)** is source-derived; no test exists at or near
  `EDIT_DEFERRED_LIMIT`, so I cannot state the exact replacement size at which
  `edit.deferred_nodes` first fires, nor whether an owner-approved case covers it.
- **The telemetry crate was not audited** beyond noting that `timer/report.rs:74,80,92`,
  `scope.rs:61` and the `write_text`/`write_json` formatters have no in-src caller.
  They are the declared composition/formatting API of a Stage 2 component and are out
  of this batch's scope.
- **I did not re-derive production LOC** (no counter run, no build); the report's LOC
  numbers are cited from the implementer's text only.
- **Non-Rust surface**: `benchmark/`, `tools/` and the retained evidence receipts were
  not audited for simplification opportunities.
