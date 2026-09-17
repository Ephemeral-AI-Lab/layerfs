# W4 evidence — oracles repaired and unexercised cases closed (G7, G8)

## What the packet changes

Product source is unchanged by this packet: every change is in an external test
target or its support module. Production LOC is therefore `11053 -> 11053`
(delta 0).

| Item | Before | After |
| --- | --- | --- |
| W4.1 | `edit_bounds.rs::the_retained_frontier_does_not_grow_with_the_file` applied `Edit::delete(0, 0)`, which short-circuits to the base root, and asserted on the **base's** extent count | replaced in W3 by `the_retained_frontier_does_not_grow_with_the_edit_count`, which drives 1/2/4/8/16 real overwrites and asserts the producer counters |
| W4.2 | `policy_capacity.rs` compared two **cutoffs** at identical depths | `a_deeper_configured_depth_never_widens_a_work_or_live_budget`: depth 4 versus depth 50 across every chain/batch/transaction/pack/group bound, plus the depth each role consults, plus depth 0 disabling one role only |
| W4.3 | `object_identity.rs` asserted the 8 MiB / 16 MiB constants and exercised only the field bound | `the_envelope_ceiling_is_enforced_at_plus_or_minus_one`: exactly 16 MiB is not refused *for its total* (the field ceiling is what fires), 16 MiB + 1 is refused by the envelope limit, and `FinalizedObject::new` refuses both before any Store sees them |
| W4.4 | `streaming.rs` built a `CountingSource` and never read it; `many_files_do_not_retain_the_whole_workload` computed `peak` and discarded it | `input_requests_stay_bounded_by_the_declared_windows` asserts the largest request, the request count and that the source delivers each byte exactly once; the many-files case asserts per-file emission counts stay at the first file's shape while the total grows |
| W4.5 | `edit_pipeline.rs::a_multi_edit_transition_round_trips_through_the_store` looped over three **single-edit** vectors | renamed to `a_single_edit_transition_round_trips_through_the_store`; new `a_multi_edit_chunked_stream_round_trips_in_current_result_coordinates` runs insert → overwrite → delete in current-result coordinates through a real Store, asserts the read-wave counters and compares the logical bytes read back through the real C1 read path |
| W4.6 | `assert_children_precede_parents` was called only from complete construction | called from `edit_bounds` (both the frontier case and the bounded-sink case) |
| W4.7 | three names overclaimed their oracle; no slow bounded sink | `metadata_pool.rs`: renamed to `a_group_holds_one_leaf_and_never_reaches_the_group_capacity` and now asserts the catalogue rows; `delta_chains.rs`: `a_wrong_role_dependency_is_rejected` now points the base at a **real stored object of another role**, so the role check fires (was: a non-existent id, so it only proved a missing dependency); `metadata_pool_index.rs`: renamed to `a_failed_save_leaves_no_usable_state_in_the_set` and now re-pools the failed save's own private values, which is the only request that can reach its phantom ordinals; new `edit_bounds.rs::a_bounded_sink_that_fills_during_emission_fails_the_edit_once` |
| W4.8 | value-group compression never executed; no pooled tamper, instruction or chain-work case | `metadata_pool.rs`: `a_compressible_group_is_stored_as_a_zstandard_frame` (reads the pack directory entry and requires `GroupCodec::Zstandard` with stored bytes shorter than the body), `a_damaged_pooled_delta_leaf_is_refused` (tampers a real delta record and requires an integrity refusal), `a_pooled_chain_past_the_canonical_budget_is_refused_on_both_sides` (the writer's `work_exceeded` at 8 × 8144 = 65 152 bytes accepted versus 73 296 refused, and the reader's own `pooled chain work` check on a spliced chain) |

## The W4.3 store limitation, stated

The 16 MiB envelope and the 8 MiB field are **format** ceilings, not reachable
acceptance paths: the whole-file envelope is cutoff-bounded (1 048 598 canonical
bytes at the largest accepted cutoff), a chunk payload is at most 32 781 and a
pooled leaf at most 8 144, so no role can produce an object at either magnitude.
`object_identity.rs` therefore exercises the bounds where they are enforced - the
codec and `FinalizedObject::new`, the only constructor that feeds a save - and
this paragraph records why a stored object at 16 MiB cannot be constructed.

## Commands, exits and raw output

`w4-verify.log` (append-only; the clippy failure and its re-run are both kept):

| Command | Exit | Result | Wall |
| --- | ---: | --- | ---: |
| `-p layerfs-content --test edit_bounds` | 0 | 9 passed | 16.1 s |
| `--test edit_batch` / `edit_noop` / `edit_model` | 0 | 7 / 7 / 6 passed | 1.0-1.3 s |
| `--test edit_localized` | 0 | 5 passed | 3.6 s |
| `--test object_identity` | 0 | 10 passed | 1.1 s |
| `--test streaming` | 0 | 8 passed | 0.3 s |
| `--test edit_reference` | 0 | 2 passed (nine sealed cases) | 56.2 s |
| `-p layerfs-storage --test metadata_pool` | 0 | 13 passed | 3.5 s |
| `--test metadata_pool_index` / `metadata_chain` | 0 | 5 / 2 passed | 1.1 / 1.3 s |
| `--test metadata_window` | 0 | 2 passed | 8.0 s |
| `--test policy_capacity` / `delta_chains` / `delta_payload` | 0 | 8 / 8 / 13 passed | ~1.2 s each |
| `--test physical_formats` / `edit_pipeline` / `visibility` | 0 | 5 / 5 / 7 passed | ~1.2 s each |
| `cargo +1.85.1 test --workspace --locked --no-fail-fast` | 0 | **43 targets, 258 tests, 0 failed** | 109.6 s |
| `cargo +1.85.1 clippy --workspace --all-targets -- -D warnings` | 101 then 0 | an `assertions_on_constants` lint in the new test, fixed by a `const` block; re-run exit 0 | 1.1 s / 0.3 s |
| `cargo +1.85.1 fmt --all --check` | 0 | clean | 0.4 s |
| `python3 core/tools/check_product_boundary.py` | 0 | PASS | 0.1 s |
| `python3 -m unittest discover -s core/tools` | 0 | 5 tests pass | 0.1 s |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | 13 tests pass | 0.1 s |
| `git diff --check` | 0 | clean | 0.2 s |
| production LOC pair | 0 | core 11053 → 11053 (delta 0) | - |

`edit_reference` (56.2 s) and the workspace suite (109.6 s) are declared as
verification runs above the 15 s performance-selection budget; no performance row
is derived from them.

## Controls (`w4-fails-without-fix.log`)

* **Control A — pooled index never invalidated** (`PoolIndex::invalidate` becomes
  a no-op): `a_failed_save_leaves_no_usable_state_in_the_set` fails with
  "the failed save's ordinals must not survive in the set" (16 entries retained).
* **Control B — value-group compression removed**:
  `a_compressible_group_is_stored_as_a_zstandard_frame` fails with
  "a compressible group must be stored compressed".

W3's control log already holds the W4.1 controls (release removed → the frontier
doubles with the edit count; hold-time counter → `nodes_created` ≠ emitted).

## What this artifact does not prove

* W4.2, W4.3 and W4.7's role case are boundary/coverage oracles: they fail if the
  property regresses, but no control run was made for them beyond A and B, so
  their "fails without the fix" status is source-derived, not measured.
* The pooled tamper case flips the last byte of the delta's pack. Whatever that
  byte is - instruction, inserted value or framing - the requirement is that the
  read refuses; the case asserts an integrity refusal, not which check fired.
* Nothing here is a measurement.
