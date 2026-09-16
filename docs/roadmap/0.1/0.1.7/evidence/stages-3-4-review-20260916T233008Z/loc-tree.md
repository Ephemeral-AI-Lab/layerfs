# Annotated core/crates tree (production LOC / physical lines)

Generated from loc-per-file.csv by the reviewer. [N prod / M phys] is the audited counter value
and the physical line count. Every other file is labelled non-production.

~~~text
core/crates/
├── layerfs-content/
│   ├── examples/
│   │   ├── edit_timing_c1.rs  [non-production:example (180 phys)]
│   │   └── fingerprint_collision_search.rs  [non-production:example (175 phys)]
│   ├── src/
│   │   ├── file/
│   │   │   ├── cdc/
│   │   │   │   ├── gear.rs  [ 494 prod /  538 phys]
│   │   │   │   └── mod.rs  [   5 prod /   10 phys]
│   │   │   ├── edit/
│   │   │   │   ├── apply.rs  [ 391 prod /  447 phys]
│   │   │   │   ├── compare.rs  [  67 prod /   86 phys]
│   │   │   │   ├── concat.rs  [  19 prod /   29 phys]
│   │   │   │   ├── finish.rs  [  38 prod /   54 phys]
│   │   │   │   ├── input.rs  [ 286 prod /  390 phys]
│   │   │   │   ├── mod.rs  [  15 prod /   20 phys]
│   │   │   │   ├── split.rs  [  23 prod /   32 phys]
│   │   │   │   └── tree.rs  [ 552 prod /  645 phys]
│   │   │   ├── mapping/
│   │   │   │   ├── build.rs  [ 301 prod /  371 phys]
│   │   │   │   ├── codec.rs  [ 303 prod /  334 phys]
│   │   │   │   ├── mod.rs  [  15 prod /   20 phys]
│   │   │   │   ├── read.rs  [ 210 prod /  241 phys]
│   │   │   │   └── types.rs  [ 182 prod /  248 phys]
│   │   │   ├── .DS_Store  [non-production:junk (5 phys)]
│   │   │   ├── content.rs  [ 195 prod /  249 phys]
│   │   │   ├── mod.rs  [  17 prod /   23 phys]
│   │   │   ├── read.rs  [ 111 prod /  127 phys]
│   │   │   └── view.rs  [ 142 prod /  171 phys]
│   │   ├── object/
│   │   │   ├── access.rs  [  18 prod /   36 phys]
│   │   │   ├── codec.rs  [ 107 prod /  138 phys]
│   │   │   ├── id.rs  [  89 prod /  119 phys]
│   │   │   ├── inode_leaf.rs  [ 311 prod /  397 phys]
│   │   │   ├── mod.rs  [  23 prod /   29 phys]
│   │   │   ├── output.rs  [ 124 prod /  192 phys]
│   │   │   └── predecessor.rs  [  67 prod /  107 phys]
│   │   ├── .DS_Store  [non-production:junk (3 phys)]
│   │   ├── error.rs  [ 120 prod /  174 phys]
│   │   ├── lib.rs  [  24 prod /   41 phys]
│   │   └── policy.rs  [ 136 prod /  226 phys]
│   ├── tests/
│   │   ├── support/
│   │   │   └── mod.rs  [non-production:test (534 phys)]
│   │   ├── edit_batch.rs  [non-production:test (217 phys)]
│   │   ├── edit_bounds.rs  [non-production:test (398 phys)]
│   │   ├── edit_localized.rs  [non-production:test (610 phys)]
│   │   ├── edit_model.rs  [non-production:test (233 phys)]
│   │   ├── edit_noop.rs  [non-production:test (239 phys)]
│   │   ├── edit_reference.rs  [non-production:test (615 phys)]
│   │   ├── edit_single.rs  [non-production:test (281 phys)]
│   │   ├── edit_timing.rs  [non-production:test (202 phys)]
│   │   ├── edit_transitions.rs  [non-production:test (265 phys)]
│   │   ├── file_complete.rs  [non-production:test (328 phys)]
│   │   ├── file_read.rs  [non-production:test (206 phys)]
│   │   ├── inode_leaf.rs  [non-production:test (174 phys)]
│   │   ├── object_identity.rs  [non-production:test (303 phys)]
│   │   ├── streaming.rs  [non-production:test (147 phys)]
│   │   └── timing.rs  [non-production:test (239 phys)]
│   ├── .DS_Store  [non-production:junk (3 phys)]
│   ├── Cargo.toml  [non-production:manifest (11 phys)]
│   └── README.md  [non-production:docs (88 phys)]
├── layerfs-storage/
│   ├── examples/
│   │   ├── measure_components.rs  [non-production:example (354 phys)]
│   │   ├── measure_edits.rs  [non-production:example (582 phys)]
│   │   └── measure_pooled.rs  [non-production:example (322 phys)]
│   ├── sql/
│   │   └── schema.sql  [  48 prod /   69 phys]
│   ├── src/
│   │   ├── cas/
│   │   │   ├── batch.rs  [  58 prod /   87 phys]
│   │   │   ├── dependencies.rs  [  62 prod /   88 phys]
│   │   │   ├── finish.rs  [  16 prod /   25 phys]
│   │   │   ├── membership.rs  [  32 prod /   47 phys]
│   │   │   ├── mod.rs  [  11 prod /   16 phys]
│   │   │   ├── owner.rs  [ 710 prod /  890 phys]
│   │   │   ├── read.rs  [  74 prod /  101 phys]
│   │   │   ├── save.rs  [  52 prod /   73 phys]
│   │   │   └── store.rs  [ 395 prod /  530 phys]
│   │   ├── encoding/
│   │   │   ├── delta/
│   │   │   │   ├── candidates.rs  [ 126 prod /  161 phys]
│   │   │   │   ├── mod.rs  [   4 prod /    8 phys]
│   │   │   │   ├── read.rs  [ 202 prod /  261 phys]
│   │   │   │   ├── record.rs  [ 201 prod /  240 phys]
│   │   │   │   └── select.rs  [ 262 prod /  343 phys]
│   │   │   ├── pool/
│   │   │   │   ├── delta.rs  [ 262 prod /  289 phys]
│   │   │   │   ├── index.rs  [ 203 prod /  259 phys]
│   │   │   │   ├── leaf.rs  [ 118 prod /  161 phys]
│   │   │   │   ├── mod.rs  [   9 prod /   14 phys]
│   │   │   │   ├── read.rs  [ 255 prod /  294 phys]
│   │   │   │   └── value_group.rs  [  79 prod /  103 phys]
│   │   │   ├── codec.rs  [ 508 prod /  632 phys]
│   │   │   ├── decode.rs  [ 170 prod /  192 phys]
│   │   │   ├── full.rs  [ 177 prod /  213 phys]
│   │   │   └── mod.rs  [  11 prod /   17 phys]
│   │   ├── pack/
│   │   │   ├── assemble.rs  [ 223 prod /  252 phys]
│   │   │   ├── layout.rs  [ 381 prod /  466 phys]
│   │   │   ├── mod.rs  [  12 prod /   17 phys]
│   │   │   └── placement.rs  [ 123 prod /  159 phys]
│   │   ├── sqlite/
│   │   │   ├── cleanup.rs  [  58 prod /   77 phys]
│   │   │   ├── connection.rs  [  50 prod /   72 phys]
│   │   │   ├── lookup.rs  [ 141 prod /  177 phys]
│   │   │   ├── mod.rs  [  10 prod /   15 phys]
│   │   │   ├── pool.rs  [ 128 prod /  160 phys]
│   │   │   ├── schema.rs  [ 232 prod /  267 phys]
│   │   │   └── write.rs  [  83 prod /  117 phys]
│   │   ├── error.rs  [ 105 prod /  154 phys]
│   │   ├── lib.rs  [  11 prod /   30 phys]
│   │   └── policy.rs  [ 210 prod /  332 phys]
│   ├── tests/
│   │   ├── support/
│   │   │   └── mod.rs  [non-production:test (323 phys)]
│   │   ├── cas_reuse.rs  [non-production:test (265 phys)]
│   │   ├── cas_roundtrip.rs  [non-production:test (141 phys)]
│   │   ├── core_pipeline.rs  [non-production:test (268 phys)]
│   │   ├── delta_chains.rs  [non-production:test (269 phys)]
│   │   ├── delta_payload.rs  [non-production:test (286 phys)]
│   │   ├── edit_pipeline.rs  [non-production:test (228 phys)]
│   │   ├── memory_bounds.rs  [non-production:test (250 phys)]
│   │   ├── metadata_chain.rs  [non-production:test (231 phys)]
│   │   ├── metadata_fingerprint_collision.rs  [non-production:test (152 phys)]
│   │   ├── metadata_pool.rs  [non-production:test (345 phys)]
│   │   ├── metadata_pool_index.rs  [non-production:test (273 phys)]
│   │   ├── metadata_window.rs  [non-production:test (263 phys)]
│   │   ├── pack_locator.rs  [non-production:test (299 phys)]
│   │   ├── persistence_failure.rs  [non-production:test (277 phys)]
│   │   ├── physical_formats.rs  [non-production:test (136 phys)]
│   │   ├── policy_capacity.rs  [non-production:test (264 phys)]
│   │   ├── timing.rs  [non-production:test (175 phys)]
│   │   └── visibility.rs  [non-production:test (294 phys)]
│   ├── Cargo.toml  [non-production:manifest (14 phys)]
│   └── README.md  [non-production:docs (146 phys)]
├── layerfs-telemetry/
│   ├── examples/
│   │   ├── timer_composition.rs  [non-production:example (89 phys)]
│   │   └── timer_nested.rs  [non-production:example (102 phys)]
│   ├── src/
│   │   ├── timer/
│   │   │   ├── format.rs  [  69 prod /   82 phys]
│   │   │   ├── json.rs  [ 115 prod /  136 phys]
│   │   │   ├── mod.rs  [   8 prod /   25 phys]
│   │   │   ├── recording.rs  [ 232 prod /  291 phys]
│   │   │   ├── report.rs  [ 170 prod /  249 phys]
│   │   │   └── scope.rs  [ 135 prod /  206 phys]
│   │   └── lib.rs  [   3 prod /   16 phys]
│   ├── tests/
│   │   ├── compile_fail/
│   │   │   ├── attach_on_pending_scope.rs  [non-production:test (11 phys)]
│   │   │   ├── child_on_pending_scope.rs  [non-production:test (11 phys)]
│   │   │   ├── control_injected_scopes.rs  [non-production:test (37 phys)]
│   │   │   ├── escape_child_scope.rs  [non-production:test (15 phys)]
│   │   │   ├── run_active_handle.rs  [non-production:test (10 phys)]
│   │   │   ├── run_scope_twice.rs  [non-production:test (12 phys)]
│   │   │   └── send_scope_across_threads.rs  [non-production:test (17 phys)]
│   │   ├── timer.rs  [non-production:test (542 phys)]
│   │   ├── timer_compile_fail.rs  [non-production:test (146 phys)]
│   │   ├── timer_format.rs  [non-production:test (123 phys)]
│   │   └── timer_json.rs  [non-production:test (339 phys)]
│   ├── Cargo.toml  [non-production:manifest (7 phys)]
│   ├── README.md  [non-production:docs (216 phys)]
│   └── USAGE.md  [non-production:docs (303 phys)]
└── .DS_Store  [non-production:junk (3 phys)]
~~~
