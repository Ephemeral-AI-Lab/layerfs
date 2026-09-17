# W6 evidence — latent traps, dead code and design deviations

## What the packet changes

### W6.1 — an accepted metadata depth is now a usable one

`cas/owner.rs::pool_base` charged the encoded chain budget a worst-case
`(depth + 2) × METADATA_RECORD_LIMIT` (8 192 B per record) against a 139 281-byte
limit, so any accepted depth above 15 could never admit a chain even though the
policy, the SQL `CHECK` and the persisted row all accepted 0..50. Both budgets are
now charged what a read actually pays: the chain's canonical sum from the depth
walk, and the base chain's encoded bytes **as the reader measured them**
(`PoolReader::chain_encoded_bytes`) plus the dependent's own record width. Two
further defects fell out of the same case:

* `DepthCache::cost_of` walked at most `MAXIMUM_DELTA_MAX_DEPTH` **records**, but a
  chain of depth *d* has *d + 1* records, so the deepest chain a policy accepts
  could not be used as a base at all: the save failed with
  `Integrity("stored dependency chain depth")` instead of selecting FULL.
* the depth was therefore also unusable *at* the cap, not only above 15.

No policy, schema or persisted value changed: the range stays 0..50 and the fix is
in the charge, which is the option the packet names first.

**Oracle.** `metadata_pool.rs::a_deep_metadata_depth_admits_and_reads_a_fifty_link_chain`
builds a 50-link chain one save at a time (depth 50 is usable), reads the deepest
leaf and every leaf back, then checks the boundary: a further link is *not*
acquired (`trials == 0`) and is not charged as refused work (`work_exceeded == 0`)
— depth is a limit, not a budget.

**Controls.** `w6-fails-without-fix.log`: with the worst-case charge restored the
case fails at round 17 (`work_exceeded: 1`, i.e. depth 16 — the reported defect);
with the walk bound reverted it fails with
`Integrity("stored dependency chain depth")`. The log also keeps a first attempt
whose patch did not apply; it is labelled as proving nothing and is not cited.

### W6.2 — constants and dead dispatch

* `select` no longer contains the dead `InodeLeaf` early return: a pooled leaf has
  its own grammar, lane and reader, and reaching `select` with one is now an
  explicit `Integrity("pooled metadata leaf selection")` instead of an encode path
  that only appeared to work.
* `StorageCapacities::delta_depth_for_role` maps `InodeLeaf` to the persisted
  **metadata** depth; mapping every non-chunk role to the whole-file depth was a
  latent trap for any caller that routed a pooled leaf through selection.
* The 23-byte constants are renamed for what they are —
  `WHOLE_FILE_CANONICAL_OVERHEAD`, the 13-byte bytes-role envelope **plus** the
  10-byte whole-file value header. The arithmetic is unchanged and correct: a
  whole-file object is `raw + 10` canonical bytes inside a 13-byte envelope, and a
  chunk is `raw + 21` because its value carries only the eight-byte magic. The
  review's reading that the limit is "10 bytes generous" is not what the code
  computes; the evidence is the decoder (`decode_bytes_object`) and the frozen
  lengths asserted by `object_identity` and `policy_capacity`.
* The unread `StorageCapacities` fields `chunk_canonical_limit`,
  `chunk_frame_limit` and `whole_file_envelope` are deleted; the frozen codec
  profile (`CodecProfile::native()` / `whole_file`) is what the codec consults and
  where those limits live.
* Dead code removed (all absent from v0.1.6 and Stage 2, each verified to have no
  caller before deletion): `FileView::walk_extents` and its `descend` (59 lines, a
  second extent traversal), `ObjectId::for_reader`,
  `FinalizedObject::canonical_capacity`, `inode_leaf::leaf_layout`,
  `AdvisoryPredecessors::to_ids`, `MINIMUM_WHOLE_FILE_BYTES`,
  `WHOLE_CANONICAL_OVERHEAD` (the duplicate of the policy constant),
  `check_canonical_limit`, `pool::delta::MATCH_BUDGET_BYTES`,
  `pool::read::{group_identity, value_ordinal}`, `Resolver::note_edge`,
  `LanePlacement::open_pack_id`, `pool::leaf::{is_pooled, body_width}`, and the
  `depth_cache_entries` pair on `MutationOwner` and `SaveOperation`.

### W6.3 / W6.4 — the two format/design deviations, recorded (owner decision open)

`physical-encoding-and-packing.md` now annotates both rows of its format table:

* **v6**: the candidate writes pooled **value groups** in v6 and pooled **leaf**
  records in the **v1 Ordinary** lane, distinguished by `objects.object_role`. The
  table's "Active pooled physical inode metadata" describes the reference. Moving
  pooled leaf records to v6 is a format change and is escalation E2.
* **v5**: the candidate refuses it by design, and could not act on a v5 pack in
  this batch because its schema identity (`application_id = 1279677261`,
  `user_version = 4`) is deliberately not the reference's. Recorded as a scope
  statement; whether the table should keep naming v5 as a supported reader is
  escalation E2.

**Oracle.** `physical_formats.rs::the_pooled_lane_assignment_and_the_v5_scope_are_the_shipped_ones`
saves a real pooled leaf, reads the pack versions of its leaf row and its value
group row out of the Store, and asserts v1 Ordinary + v6 PooledMetadata + the
`object_role` code, then asserts v5 is refused as an unsupported framing.

### W6.5 — remaining simplification findings

| Finding | Change |
| --- | --- |
| `emit_file_state` implemented three times | `tree.rs`'s verbatim copy is deleted; the frontier calls `mapping::emit_file_state` |
| `assemble_inner`'s unused `assemble` scope and `let _ = assemble` | parameter removed |
| an empty `if` holding only a comment | replaced by the comment plus an ignored binding the pattern already provides |
| the per-edit `FileState` rebuild whose only live field is `bytes` | replaced by a running `result_len` |
| `rightmost_payload`'s `let last` / `let _ = last` | removed; the empty-branch check is explicit |
| three identical FULL-fallback blocks in `select_pooled` | one `pooled_full` helper |
| the dead `policy` parameter on `MutationOwner::acquire` | removed with its call site |
| the value-group body copied twice per group | copied only when compression did not apply |
| `tree::coalesce` duplicating `edit::coalesce_adjacent` | `coalesce` now uses the pairwise rule |
| `inode_leaf::rebuild_leaf`'s duplicate assignment and `let _ = prefix` | the comment states why the prefix is not carried |
| `dependencies::Availability`'s two sets for one fact | one `known` set |
| `write_value_groups`'s `skip`/`take` re-derivation | one slice cursor over `fresh`, plus a coverage check |
| `mapping/read.rs`'s linear demand scan | **kept, and stated**: the wave is bounded by `READ_WAVE_OBJECTS` (32) and a demand can revisit an earlier identity, so a scan over a 32-element vector is bounded work and a map would add a per-wave allocation |

## Commands, exits and raw output

`w6-verify.log` (26 focused targets + the workspace suite + clippy/fmt/boundary/tools,
every command exit 0):

* storage: `metadata_pool` 14, `metadata_pool_index` 5, `metadata_chain` 2,
  `metadata_window` 2, `delta_chains` 9, `delta_payload` 13, `pack_locator` 8,
  `physical_formats` 6, `policy_capacity` 8, `visibility` 7,
  `persistence_failure` 7, `cas_reuse` 8, `cas_roundtrip` 6, `core_pipeline` 5,
  `memory_bounds` 7, `edit_pipeline` 5.
* content: `edit_bounds` 9, `edit_localized` 5, `edit_reference` 2 (nine sealed
  cases), `edit_model` 6, `edit_batch` 7, `streaming` 9, `file_complete` 14,
  `file_read` 8, `object_identity` 10, `inode_leaf` 6, `timing` 7.
* `cargo +1.85.1 test --workspace --locked --no-fail-fast` — **43 targets,
  263 tests, 0 failed**, 95.4 s.
* clippy, fmt, the product-boundary check, both tool suites, `git diff --check` —
  exit 0.
* production LOC pair — core `11166 -> 10983` (delta **-183**); C1 4508 → 4388,
  C2 5926 → 5863, telemetry unchanged.

## What this artifact does not prove

* W6.3 and W6.4 are **recorded, not decided**: the shipped lane assignment and the
  v5 scope are pinned by `physical_formats`, but whether the design table should
  change (or the implementation move to v6) is the owner's call (escalation E2).
* The `-183` production LOC is a deletion of unused code and constants plus one
  refactor; it is not an algorithmic saving and no performance claim is made.
* `mapping/codec.rs` still encodes with `node.validate(true)`, so the
  `MIN_ENTRIES` rule binds on decode; the review noted the invariant holds because
  `commit_node` re-decodes every reached child with `root = false`. That remains
  the design: the builder does not know a page's final position when it emits it.
