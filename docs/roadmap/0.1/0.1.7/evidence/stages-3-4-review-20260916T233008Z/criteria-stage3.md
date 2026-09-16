# Stage 3 / issue #168 — acceptance-criteria audit (physical encoding, delta, pooling, packing)

**Independent review — physical-encoding scope only (C1 + C2 of issue #168).**
Stage 4 / #169 (localized edits, transitions, multi-edit finality) is out of this
document's scope except where a C2 test also carries a #168 obligation.

## 0. Reviewed snapshot and method

| Field | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| HEAD | `91c3a0741fff64e8161d5c1b6e759f347ffbf757` (branch `main`) |
| Working tree | clean except the untracked evidence directory `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/` (`git status --porcelain`) |
| Reference | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` (root `crates/`), read-only oracle |
| Product under review | `core/crates/layerfs-content` (C1), `core/crates/layerfs-storage` (C2), `core/crates/layerfs-telemetry` |
| Runtime SQL | `core/crates/layerfs-storage/sql/schema.sql` |
| Verification-suite log | `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/checks.log` — produced by the parent agent at this exact HEAD: boundary guard, core tools unittests, LOC unittests, `fmt --check`, both focused test groups, `--workspace --locked --no-fail-fast`, `clippy -D warnings`, all rc=0 |

**Method.** Static reading of every product file named by the criteria, every test
body named by an implementer claim, the runtime SQL, the committed design/handoff
documents and the retained raw receipts. **No compile, build, test or clippy command
was run by this review** (hard constraint). Read-only `git`, `grep`, `wc`, `find` and
read-only `python3 core/tools/check_product_boundary.py` were used. Test *results*
quoted below come from `checks.log`, which the parent produced at the pinned HEAD;
they are cited as retained raw evidence, not re-derived here.

**Rule applied throughout.** A green test name is not evidence. Each row states what
the test's oracle actually asserts; where a test name overclaims its oracle, that is
reported as missing evidence rather than as a pass.

---

## 1. Stage 3 verdict

> ### **Stage 3 verdict: INCOMPLETE** (one demonstrated defect, three unmeasured/unexercised gates)

Stage 3 is **not** stage-complete and issue #168 is **not** close-ready.

**Blockers, most severe first**

- **B1 (correctness, demonstrated in code).** A `CHUNK` object whose *first* supplied
  advisory predecessor is absent, still unsealed in the same save, or of another
  role makes the whole save **fail** instead of selecting FULL by policy. This
  contradicts handoff §2 ("A missing/ineligible advisory candidate ... may select
  FULL by policy"), the required C2 case "absent/ineligible optional candidates"
  (handoff §4 row `delta_payload`), and the reference, which skips an absent hint.
  `encoding/delta/select.rs:210` → `select.rs:243` → `delta/read.rs:84`. The only
  test named for this rule (`delta_payload::an_absent_or_ineligible_candidate_selects_full`,
  `delta_payload.rs:159`) exercises `WHOLE_FILE` only.
- **B2 (unmeasured gate, required by the issue).** "Qualify ... simultaneous
  index/codec/SQL memory under the committed case contract", and acceptance item
  "storage/index/column footprint, memory and speed claims are measured". The
  measurement addendum declares no simultaneous-memory arm
  (`stages-3-4-measurement-addendum.md:92-106`) and the matched-pair ledger states the
  gate is "**unmeasured**, not passed" (`stages-3-4-matched-c1-20260917T050000Z/ledger.md`,
  conclusion 4). The implementer's own #168 comment says the same.
- **B3 (missing evidence, required by the handoff).** "Audit same-save delta-base
  eligibility and admitted-FULL cache deviations; preserve required candidate/selection
  behavior **or document the explicit decision**". The admitted-FULL cache is a faithful
  port (verified against the reference), but **no document anywhere in the reviewed tree
  or in the evidence directories records the same-save delta-base deviation**, and the
  deviation is not benign (it is the mechanism behind B1's fatal case for the chunk lane).
- **B4 (unexercised acceptance path).** "Exact value-group body/digest/**compression**".
  Every test fixture builds high-entropy 73-byte values (`ObjectId::for_bytes` is
  blake3, `object/id.rs:24-29`), so `compress_group_body` always returns `None` and every
  stored value group is `GroupCodec::Raw`. The compressed-group encode path
  (`encoding/pool/value_group.rs:50-53`) and its decode path
  (`encoding/pool/read.rs:121`) have **no coverage at all**.

**Non-blocking but report-worthy:** the test names `values_cross_a_group_boundary_at_one_hundred_sixty_five`
and `a_wrong_role_dependency_is_rejected` do not test what they claim; the pooled
metadata depth is accepted up to 50 but is effectively capped at ~15 by the producer's
own encoded-work estimate; several pooled-lane integrity error paths
(`pooled delta base identity`, `pooled instruction`, `pooled chain work`,
`value group identity`) are unreachable from any retained test.

---

## 2. Criterion table

Legend: **PASS** = code + a real oracle + retained evidence; **FAIL** = demonstrated
defect; **INCOMPLETE** = part present, at least one required piece missing;
**NOT_RUN** = nothing executed; **N/A** = out of scope with justification.

All test commands ran green at HEAD in `checks.log`; "oracle" below is what the test
body actually asserts, not what its name suggests.

### 2.1 Issue #168 original Acceptance items

| # | Criterion | Source location | Test target / name | Oracle actually asserts | Retained evidence | Status | Precise gap |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1a | Exact decoded bytes/identity preserved | `encoding/decode.rs:28-135`; `cas/read.rs:91-93`; `delta/read.rs:117-119,164-166` | `cas_roundtrip::every_initial_role_round_trips`, `whole_file_object_survives_close_and_reopen`; `delta_payload::an_explicit_predecessor_produces_a_readable_prefix_record` (`delta_payload.rs:59-70`); `edit_pipeline::*` | byte equality against the canonical object the caller constructed, plus `ObjectId::for_bytes(read)==id` | `checks.log`; `stages-3-4-timing-…/e2-*` | **PASS** | — |
| A1b | Candidate decisions, chain/chronology checks | `delta/select.rs:179-293`; `delta/read.rs:127-147` | `delta_chains::a_cyclic_dependency_is_refused_by_chronology` (`delta_chains.rs:201-236`) externally rewrites both `base_object_id` rows and asserts `Integrity("chronology")`; `an_increased_depth_is_honoured_up_to_its_boundary` asserts `edges==16` | cycle impossible in locator order; every chain step taken | `checks.log` | **PASS** | — |
| A1c | Wrong-role dependency rejected | `delta/read.rs:140-142` | `delta_chains::a_wrong_role_dependency_is_rejected` (`delta_chains.rs:166-198`) | **only** `StorageError::ObjectMissing(_)` — the injected `base_object_id` is `ObjectId::for_bytes(b"not the same role")` (`delta_chains.rs:179`), an id that names no stored object | `checks.log` | **INCOMPLETE** | The `Integrity("dependency role")` branch is never exercised: the fixture induces a *missing* base, not an existing base of another role. Test name overclaims its oracle. |
| A2 | Missing optional candidates are normal policy | `delta/select.rs:209-240` (`WHOLE_FILE`); `delta/select.rs:210` (`CHUNK`) | `delta_payload::an_absent_or_ineligible_candidate_selects_full` (WHOLE_FILE only); `native_delta_uses_the_first_supplied_eligible_candidate_only` (two *stored* candidates) | WHOLE_FILE: `absent_candidates`/`ineligible_candidates` ≥ 1 and `prefix_records==0`. CHUNK: only `trials==1` | `checks.log` | **FAIL** | No eligibility check on the CHUNK first candidate. See B1 / F1. |
| A2b | Failed acquisition/codec/SQL fails, never recovers through FULL | `delta/select.rs:243,269`; `encoding/full.rs:148-151`; `owner.rs:324-336` | `delta_payload::a_corrupt_base_fails_the_save_instead_of_selecting_full` (`delta_payload.rs:223-243`) | `save_one(...).is_err()` after `corrupt_first_pack`, and the subsequent read also errs | `checks.log` | **PASS** | — |
| A2c | A losing trial selects FULL (not a failure) | `delta/select.rs:270,284-292`; `owner.rs:512-521` | `delta_payload::an_unrelated_candidate_loses_the_cost_comparison`; `metadata_pool::a_losing_delta_trial_stores_the_leaf_in_full` (`metadata_pool.rs:177-204`) | `trials==1`, `full_losses==1`/pooled `trials==1`, `full_leaves==1`, `delta_leaves==0`, and the object still reads back | `checks.log` | **PASS** | — |
| A3 | Pooling keeps exact window, ordinal choice, authenticated equality, bounded memory | `pool/index.rs:80-97,151-215,224-246`; `pool/value_group.rs:68-76`; `policy.rs:116` | `metadata_window::the_window_retains_exactly_the_cap_and_resets_whole_groups` (asserts `len()==METADATA_INDEX_VALUES` then a wholesale reset to 1); `crossing_the_window_evicts_early_ordinals_and_the_reopen_replays_the_window` (1 312 real leaves, catalogue rows `(131001,100)`/`(131101,100)`, 131 200 ordinals, `pool_index_entries()==200`); `metadata_pool_index::a_shared_value_keeps_the_smallest_ordinal_across_leaves_and_reopen`; `metadata_fingerprint_collision::colliding_values_are_never_confused_through_the_store` | the real 131 072 boundary, the whole-window reset, the cold-start replay, and 73-byte equality over a genuine 64-bit fingerprint collision | `checks.log`; `stages-3-4-fingerprint-collision-20260917T021500Z/collision.json` | **PASS** | — |
| A3b | "catalogue work remains attributed" | `owner.rs:601-614` (rows/bytes charged), `pool/index.rs:151-158` (`retained_bytes`) | none | — | — | **INCOMPLETE** | Catalogue insert work is charged to the transaction budget but is never *reported*: no receipt separates catalogue/B-tree work from save time or memory. |
| A4a | Stable locators | `pack/placement.rs`; `sqlite/write.rs` | `pack_locator::a_full_lane_starts_a_new_pack_and_keeps_existing_locators` (snapshot before/after); `append_reuses_a_pack_without_changing_record_ordinals` (asserts `(pack,group,record)` = `(p,0,0)` and `(p,1,0)`) | existing `(pack_id, group, record)` triples do not move when a lane later appends | `checks.log` | **PASS** | — |
| A4b | Supported singletons and configuration overrides qualified | `encoding/full.rs:172-202`; `policy.rs:78-80`; `pack/layout.rs:106-137` | `policy_capacity::a_larger_incompressible_whole_file_record_uses_the_singleton_lane` (1 MiB cutoff, `noise(1 048 575)`, asserts exactly one file in the Store dir); `memory_bounds::the_supported_incompressible_singletons_are_stored_and_read_back`; `physical_formats::the_lane_table_names_one_limit_per_lane` | a maximum-size incompressible whole-file record stores and reads back byte-exactly from its own pack; no payload staging file appears | `checks.log` | **PASS** | — |
| A4c | "packing density" qualified | `pack/assemble.rs:174-251`; `pack/layout.rs:184-212` | none (only `packs_created`/`pack_appends` counters) | — | `stages-3-4-timing-…/ledger.md` (Store file size only) | **INCOMPLETE** | No occupancy/tail-waste/density measurement exists for any lane; the timing ledger reports only total Store bytes. |
| A5a | Base corruption cases covered | `cas/membership.rs`; `pack/layout.rs:224-275` | `delta_payload::a_corrupt_base_fails_the_save_instead_of_selecting_full`; `delta_chains::a_corrupt_intermediate_is_rejected_during_reconstruction`; `pack_locator::a_corrupt_pack_body_is_rejected_on_read`; `cas_reuse::a_corrupted_stored_record_is_rejected_as_a_collision`; `metadata_pool::a_corrupt_value_group_is_rejected_once` (`metadata_pool.rs:207-227`, digest tamper → error naming "value group") | real byte tampering in the SQLite BLOB, not fault injection | `checks.log` | **PASS** | — |
| A5b | Depth/work boundaries covered | `delta/select.rs:198,248-266,313-331`; `policy.rs:98-112` | `delta_chains::a_reduced_depth_stops_at_its_own_boundary` (depth 2 → `ineligible_candidates==1`, FULL); `an_increased_depth_is_honoured_up_to_its_boundary` (depth 16 → `edges==16`); `a_chain_that_would_exceed_its_work_budget_is_never_stored` (asserts `refused>=1` and every member re-reads) | the accepted cap and the first excluded edge, with byte budgets still enforced | `checks.log` | **PASS** | — |
| A5c | Pooled/metadata depth and work boundaries | `owner.rs:648-692`; `pool/read.rs:151-188`; `policy.rs:104-110` | `metadata_chain::small_records_reach_the_depth_cap_and_the_next_edge_is_stored_in_full` (`metadata_chain.rs:121-179`); `maximal_records_hit_the_canonical_budget_before_the_depth_cap` (`:182-231`) | nine-record chain at default depth 8; a tenth is FULL; 100-row leaves hit the 65 536-byte budget first with `work_exceeded` recorded; deleting an intermediate breaks only its dependents | `checks.log` | **INCOMPLETE** | Only the **default** metadata depth 8 is exercised. No test drives a *changed* metadata depth (`policy_capacity.rs:215-255` only asserts that 12 and 50 are *persisted*). |
| A5d | Low/high reuse, cold/reopen | `pool/index.rs:100-149,165-215` | `metadata_pool::repeated_values_reuse_the_same_ordinals_across_leaves_and_saves`; `metadata_pool_index::a_cold_store_synchronizes_from_the_catalogue_without_reassigning` (asserts a fresh Store starts at 0 entries yet reuses 8/8 and writes no second group) | reuse survives reopen and comes from the catalogue, not from the in-memory set | `checks.log` | **PASS** | — |
| A5e | Error cases covered | `cas/finish.rs`; `sqlite/cleanup.rs`; `cas/owner.rs:856-889` | `persistence_failure::a_late_input_failure_after_an_early_commit_cleans_up_once_and_keeps_earlier_objects` (exact `(objects,packs)` row counts restored); `a_second_save_cannot_acquire_ownership_while_the_first_holds_it`; `an_unknown_write_acknowledgement_cannot_be_induced_without_a_fault_hook` (explicit coverage statement) | one cleanup attempt, retained rows byte-identical, unknown outcome not fabricated | `checks.log` | **PASS** | — |
| A6 | Independent timers with complete save/read work | `cas/store.rs:203-233,261-267,293-309`; `telemetry` | `timing::an_independent_save_reports_its_real_scopes`; `recording_and_disabled_storage_execution_agree_exactly` (`timing.rs:85-121`, asserts equal `inserted`/`reused`/`packs_created` and equal read bytes); `a_bounded_report_stays_inside_the_recorder_limits` | real named scopes; enabled and disabled modes produce identical product results; reports stay inside node/depth caps | `stages-3-4-timing-20260917T031000Z/{ledger.md,README.txt,tool-identities.txt,e1a-*,e1b-*,e1c-*,e2-*,e3a-*,e3b-*,e3c-*}` | **PASS** | The receipts are debug-profile, pinned at commit `dfd54fd8e` (not HEAD `91c3a074`); `git diff --stat dfd54fd8 91c3a074 -- core/crates` touches only `examples/edit_timing_c1.rs` and `tests/edit_reference.rs`, so the *product* source is byte-identical, but the receipts do not carry HEAD's seal. |
| A7 | Footprint/memory/speed measured under approved cases; no invented win | `cas/store.rs:165-179`; `policy.rs:231-321` | none for memory | — | `stages-3-4-timing-…/ledger.md`; `stages-3-4-matched-c1-…/ledger.md` | **INCOMPLETE** | No simultaneous index/codec/SQL memory; no column footprint; no pooled-lane matched reference arm; the matched C1 pair explicitly declines a storage and latency conclusion and calls the memory gate "unmeasured, not passed". |

### 2.2 Added completion gates (E1/E2/E3) and the two continuation gates

| # | Gate | Source location | Test target / name | Oracle actually asserts | Retained evidence | Status | Precise gap |
| --- | --- | --- | --- | --- | --- | --- | --- |
| E1a | Checked canonical inode-leaf/value grammar | `object/inode_leaf.rs:14-34` (73 B value, 31 B header, 81 B row, 44 B pooled prefix, 12 B pooled row, ≤100 rows), `:103-153,175-215,288-365` | `inode_leaf` (6 tests): `a_leaf_round_trips_with_its_exact_layout` (canonical width `44 + rows*81`, physical width `44 + rows*12`, rows ∈ {1,2,63,64,100}); `malformed_leaves_are_rejected` (trailing byte, wrong role, wrong count, unordered/zero serial, wrong subtree total, 101 rows, zero rows); `a_pooled_value_object_is_exactly_ninety_four_canonical_bytes` | exact byte layout in **both** canonical and pooled form, plus every declared malformation rejected | `checks.log` | **PASS** | Every constant in the assignment is enforced in code: 73 (`inode_leaf.rs:14`), 31 (`:20`), 81 (`:22`), 44 (`:26`), 12 (`:28`), ≤100 rows (`:24`), 165/group (`policy.rs:83` + SQL `CHECK (count BETWEEN 1 AND 165)` at `sql/schema.sql:41`). |
| E1b | Exact value-group body/digest/**compression** | `encoding/pool/value_group.rs:31-65` (frame → hash body → then compress); `encoding/codec.rs:613-627` (`frame.len()+16 <= raw.len()`) | `metadata_pool::a_supplied_leaf_round_trips_through_sqlite_and_reopen`; `a_corrupt_value_group_is_rejected_once` | digest authenticates the **decoded** body (tampering the stored digest fails the read) | `checks.log` | **INCOMPLETE** | The hash-before-compress order is correct and visible in code, but no fixture ever produces a compressible group (all fixtures use blake3-derived, high-entropy values), so `GroupCodec::Zstandard` for the pooled lane — `value_group.rs:50-53` and `pool/read.rs:121` — is **never executed**. |
| E1c | Ordinal + catalogue persistence; pooled FULL; authenticated reopen/readback | `owner.rs:553-639`; `sqlite/pool.rs:52-150`; `pool/read.rs:249-280`; `encoding/delta/read.rs:105-125` | `metadata_pool::a_supplied_leaf_round_trips_through_sqlite_and_reopen` (asserts `pool.leaves==1`, `new_values==8`, `groups==1`, `full_leaves==1`, then exact bytes + identity after reopen); `a_pooled_leaf_survives_a_store_reopen_with_its_chain` | real save → finish → reopen → exact canonical bytes and identity | `checks.log` | **PASS** | — |
| E1d | "165-value boundary" | `policy.rs:83`; `owner.rs:576-587` | `metadata_pool::values_cross_a_group_boundary_at_one_hundred_sixty_five` (`metadata_pool.rs:136-174`) | only `groups==1` per leaf, plus a final byte-length assertion | `checks.log` | **INCOMPLETE** | The fixture writes three leaves of **100 distinct** values; no group ever holds 165 values and no leaf ever spans two groups. This is structurally impossible through the public path: `select_pooled` (`owner.rs:442-481`) can only pass ≤ `MAXIMUM_LEAF_ROWS` = 100 fresh values to `write_value_groups`, so `fresh.chunks(165)` (`owner.rs:576`) always yields exactly one chunk and its multi-group branch is dead. Test name overclaims its oracle. |
| E2a | Bounded Store-owned ordered index, reference window/reset | `pool/index.rs:24-64` (`BTreeSet<(i64,u32)>`, cap 131 072, invalidate-on-failure), `:72-97` (chronology, whole-window reset), `:100-149` (catalogue replay), `:224-246` (`retained_start` recurrence) | `metadata_window::the_window_retains_exactly_the_cap_and_resets_whole_groups`; `crossing_the_window_evicts_early_ordinals_and_the_reopen_replays_the_window` | the cap is *retained* (not evicted at equality), one value past it resets the window wholesale, the reset releases exactly one entry's bytes, a refused note leaves the window untouched, and a reopened Store reproduces the producer's window from the catalogue alone | `checks.log` | **PASS** | — |
| E2b | Exact-equality / smallest-ordinal choice | `pool/index.rs:160-215` (fingerprint is a filter only; `values.contains(&value)` compares all 73 bytes; `found.entry(value).or_insert(ordinal)` over ascending ordinals) | `metadata_pool_index::a_shared_value_keeps_the_smallest_ordinal_across_leaves_and_reopen` (catalogue read back externally: first group `(1,8)`); `metadata_fingerprint_collision::colliding_values_are_never_confused_through_the_store` (two genuine 64-bit collisions never share an ordinal; both read back byte-exact) | equality is decided by the full value, never the fingerprint; the ordinal chosen is the smallest retained match | `checks.log`; `stages-3-4-fingerprint-collision-20260917T021500Z/collision.json` + `search.log` | **PASS** | — |
| E2c | Pending / reopen / failure semantics | `pool/index.rs:60-64,116-123`; `owner.rs:875-889` | `metadata_pool_index::a_cold_store_synchronizes_from_the_catalogue_without_reassigning`; `a_failed_save_invalidates_the_set_instead_of_reusing_phantom_ordinals`; `a_failed_save_removes_its_private_value_groups` (`metadata_pool_index.rs:206-243`, catalogue row count unchanged, retained leaf still reads) | a fresh Store starts empty and re-derives; a failed save leaves no catalogue row and the retained object stays readable | `checks.log` | **INCOMPLETE** | (i) The "phantom ordinal" oracle cannot see phantom state: the failed save's private values (100..108) are never re-offered, so only *retained* values are checked. (ii) The "pending same-save reuse" path (`owner.rs:445-449`, `pending_values`) is never exercised: every reuse test uses separate saves. |
| E3a | Pooled COPY/INSERT DELTA with its own depth/work limits | `encoding/pool/delta.rs:23-120` (grammar + `apply`), `:127-227` (producer with a 128 KiB match budget); `owner.rs:653-680`; `pool/read.rs:151-188` | `metadata_chain::small_records_reach_the_depth_cap_and_the_next_edge_is_stored_in_full`; `maximal_records_hit_the_canonical_budget_before_the_depth_cap` | the depth policy (8) and the canonical byte budget cut the chain in the two regimes the report claims, `work_exceeded` records the refusal, and the producer never creates a dependency the reader refuses | `checks.log`; `stages-3-4-timing-…/e1a-c` (`full leaves = work-exceeded + 1` at 24/128/512 leaves) | **PASS** | Depth limit, canonical budget and encoded budget are separate from the payload lane's (`policy.rs:102-110`, `owner.rs:653,672-676`, `pool/read.rs:158-161,178-188`). |
| E3b | Intermediate authentication, base/value dependencies | `pool/read.rs:189-212` (base identity, output length, instruction validation); `pool/leaf.rs:71-99`; `pool/value_group.rs:68-93` | `metadata_chain` hole tests (delete an intermediate locator; dependents fail, non-dependents still read); `metadata_pool::a_corrupt_value_group_is_rejected_once`; `a_missing_catalogue_row_is_rejected` | a removed intermediate makes every leaf above it unreadable and nothing else | `checks.log` | **INCOMPLETE** | Only *removal* is induced. No fixture corrupts a pooled record's base identity, its instruction program, or its declared output length, so `pooled delta base identity` (`pool/read.rs:205`), `pooled instruction` (`pool/delta.rs:29-68`) and `pooled chain work` (`pool/read.rs:187`) are unexercised. `a_missing_catalogue_row_is_rejected` also accepts `ObjectMissing`, a weaker oracle than the code's `Integrity` path. |
| E3c | Visibility of unfinished private value groups; atomic writes | `owner.rs:601-610` (catalogue row in the same transaction as the pack), `cas/owner.rs:816-853` (seal → watermark → COMMIT) | `metadata_pool::an_unfinished_private_value_group_is_not_visible_to_a_reader`; `visibility::an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement` (asserts `ceiling_after == highest_after` only after `finish`) | before acknowledgement an unrelated reader is refused with `VisibilityCeiling`/`ObjectMissing`; acknowledgement publishes every pack | `checks.log` | **PASS** | Handoff rule "group sealing is not a commit; finish must include all required acknowledgement work" is honoured: `finish_inner` seals **all five lanes** (`owner.rs:818-820`) before advancing the watermark in its own final transaction (`owner.rs:845-850`). |
| E3d | One owned cleanup attempt | `cas/finish.rs:12-25`; `owner.rs:856-868`; `sqlite/cleanup.rs:30-77` | `metadata_pool_index::a_failed_save_removes_its_private_value_groups`; `persistence_failure::a_late_input_failure_after_an_early_commit_cleans_up_once_and_keeps_earlier_objects` (row counts exactly restored; a re-save adds nothing) | exactly one cleanup; previously successful objects and rows unchanged; no second deletion | `checks.log` | **PASS** | Cleanup deletes object rows newest-first before pack rows (`cleanup.rs:33-73`), satisfying the `objects`→`object_packs` FK order. |
| G1 | **Audit same-save delta-base eligibility and admitted-FULL cache deviations** | cache: `encoding/delta/candidates.rs:14-160` vs reference `crates/layerfs-layerstack-store/src/objects/small_candidates.rs:4-120`; eligibility: `delta/select.rs:210,301-331`; `save.rs:67-68`; `cas/dependencies.rs:49-72` | none | — | — | **FAIL** | (a) **Cache: no deviation found.** Constants (`INDEX_BYTES` 128 KiB, `SLOTS` 1024, `REFERENCES` 8192, `WINDOW` 16, `EMPTY`, overlap≥2, smaller-id tie-break) and the insert/find algorithms match the reference line-for-line. (b) **Eligibility: deviation, undocumented and fatal for CHUNK.** `advisory.first()` is taken with no `eligible()` check (`select.rs:210`); a first candidate that is absent, of another role, or merely *unsealed in this same save* raises `ObjectMissing`/`Integrity`/`Content` and fails the whole save. Same-save bases are in fact unreachable for the `Native` lane because its groups only seal at `GROUP_TARGET` = 48 KiB (`owner.rs:27,349-357`) or at `finish`, and `lookup::location` needs a row. `stages-3-4-report.md:49` claims CHUNK "considers the first supplied **eligible** candidate" — the code does not check eligibility. |
| G2 | **Qualify real pooling/payload/pack operations, retained footprint and simultaneous index/codec/SQL memory** | `cas/store.rs:165-179` (`pool_index_entries`/`pool_index_bytes`); `examples/measure_pooled.rs` | `e1a/e1b/e1c` arms; `e2-*`; `e3*` | real save-to-ack, real readback, real counters, retained index entries/bytes and Store size | `stages-3-4-timing-20260917T031000Z/{ledger.md,README.txt,tool-identities.txt}` | **INCOMPLETE** | Real operations and retained footprint: **PASS** (51 200 rows → 611 pooled values, 323 584 B Store at 512 leaves; `full = work_exceeded + 1`). Simultaneous index/codec/SQL memory: **NOT measured** — the addendum's ledger (`stages-3-4-measurement-addendum.md:92-106`) declares only product-reported capacity plus cited tests; the matched-pair ledger says the gate is "unmeasured, not passed". |

### 2.3 Handoff §2 hard invariants

| # | Invariant | Source location | Evidence / test | Status | Gap |
| --- | --- | --- | --- | --- | --- |
| H1 | No retry, error-driven fallback, busy handler, fsync/WAL/crash-durability, third-party patch | `sqlite/connection.rs:29-45` (`journal_mode=MEMORY` verified by reading it back, `synchronous=OFF`, `temp_store=MEMORY`, `foreign_keys=ON` verified, `busy_timeout(ZERO)`); `sqlite/connection.rs:54-72`; no `[patch]`/`[replace]` anywhere in `core/` | greps for `fsync|fdatasync|sync_all|sync_data|WAL|retry|resend|backoff` find only doc comments; `crates`/`layerfs_workspace` references: none | **PASS** | The only "fallback"-shaped construct is `or_else(QueryReturnedNoRows → Ok(None))` in `sqlite/pool.rs:95` and `sqlite/schema.rs:157` — error *classification*, not an alternate algorithm. |
| H2 | Missing/ineligible candidate ⇒ FULL; failure ⇒ fail | see A2/A2b | see A2/A2b | **FAIL** | B1. |
| H3 | Exact CAS comparison, canonical identity, frozen CDC, extent partition, dependency/chronology, retained visibility, stored versions | `cas/membership.rs`; `cas/read.rs:91`; `cas/save.rs:42-65`; `visibility.rs` | `cas_reuse` (7), `visibility` (5), `delta_chains` (7) | **PASS** | — |
| H4 | One writer owns a save; shared batches/packs; no per-object commit; sealing ≠ commit | `owner.rs:161-174` (single `BEGIN IMMEDIATE`), `owner.rs:773-790` (bounded commit), `owner.rs:816-853` | `persistence_failure::a_second_save_cannot_acquire_ownership_while_the_first_holds_it`; `cas_reuse::one_operation_can_save_two_files_into_shared_packs` (`packs_created==1`) | **PASS** | — |
| H5 | No payload spill/scratch, no whole-file reconstruction fallback, no input-sized collector, no per-object trace growth | `memory_bounds.rs:184-205` asserts the Store directory holds **exactly** `nospill.sqlite`; `memory_bounds.rs:105-140` bounds pending objects/bytes and retained tail | **PASS** for spill and batch bounds | Evidence for "no input-sized collector" and trace growth is by construction (`telemetry` node/depth caps asserted in `timing.rs:163-175`), not measured. |
| H6 | One construction worker except namespace init | no thread spawn in C1/C2 sources at all (`grep -rniE 'thread::|spawn\(|available_parallelism|rayon'` over `core/crates/*/src` → no matches) | **N/A** | No worker lane exists in C1/C2, so the single-worker rule constrains nothing here; the namespace-init exception belongs to a later stage. |
| H7 | ≤999 physical lines per product file; lib.rs/mod.rs ≤200 | `python3 core/tools/check_product_boundary.py` → `PASS: scanned 75 production Rust/SQL files`; largest production file `cas/owner.rs` = 890 physical lines; runtime SQL `sql/schema.sql` = 69 lines | **PASS** | The only >200 `mod.rs` files are test helpers (`tests/support/mod.rs`), outside the product rule. |
| H8 | Reuse dependency pins; no registry/plugin; no automatic migration | `core/crates/*/Cargo.toml` (blake3 `=1.8.5`, rusqlite `=0.40.2`, zstd-sys `=2.0.16`); `core/Cargo.lock` present; `schema.rs:27` `SCHEMA_VERSION = 4` | `cas_roundtrip::malformed_store_identity_is_rejected`; `policy_capacity::an_unsupported_or_conflicting_policy_is_rejected_before_mutation` | **PASS** | — |

### 2.4 Handoff §3 checkpoints relevant to Stage 3 (A, B, E)

| # | Checkpoint item | Source location | Test / evidence | Status | Gap |
| --- | --- | --- | --- | --- | --- |
| CA1 | Defaults T=128 KiB, whole-file depth 8, chunk depth 4 | `layerfs-content/src/policy.rs:14,20,22` | `delta_chains::the_default_depths_are_the_documented_ones`; `policy_capacity::the_minimum_supported_cutoff_is_the_published_minimum` | **PASS** | — |
| CA2 | Genuine configuration with checked ranges, persisted policy, reopen agreement | `policy.rs:84-105` (power of two, 128 KiB–1 MiB, depth ≤50), `sql/schema.sql:13-32` | `policy_capacity::every_supported_cutoff_is_persisted_and_reopened_unchanged` (131 072/262 144/1 048 576) | **PASS** | — |
| CA3 | Explicit rejection of bad values before any mutation | `policy.rs:166-188`; `cas/store.rs:106-143` | `policy_capacity::an_unsupported_or_conflicting_policy_is_rejected_before_mutation` (6 rejected policies, `!path.exists()`; a persisted 196 608 rejected at open) | **PASS** | — |
| CA4 | **256-KiB and 1-MiB cutoff cases really run end to end** | `encoding/full.rs:132-163`; `encoding/codec.rs` window 18→20 at `policy.rs:156-162` | `policy_capacity::boundaries_follow_the_configured_cutoff_not_a_frozen_one` — for each of **131 072 and 262 144**, three lengths (`cutoff-1`, `cutoff`, `cutoff+1`) are constructed with `construct_bytes`, saved through `SaveHandoff`, read back and asserted to be WHOLE_FILE vs chunked; `a_larger_incompressible_whole_file_record_uses_the_singleton_lane` covers 1 MiB with **incompressible** `noise` | **PASS** | The 1-MiB case exercises the singleton *save/read* path with a raw object built by `encode_whole_file`, not a full `construct_bytes` at 1 MiB; the 256-KiB case is a full construction. Both handoff-required cutoff values are present and really execute. |
| CA5 | Derived capacities; raising T or a depth does not raise other budgets | `policy.rs:134-153,282-312` | `policy_capacity::a_larger_cutoff_keeps_the_chain_and_live_budgets_unchanged`; `the_pooled_metadata_depth_is_persisted_and_checked_separately` | **PASS** | — |
| CA6 | "Meaningful non-default values for **both** depth fields" | `delta/select.rs:198`; `policy.rs:315-320` | `delta_payload::depth_zero_disables_prospective_delta_entirely` sets `(whole, chunk) = (0, 0)`; `delta_chains` changes only the **whole-file** depth (2, 16) | **INCOMPLETE** | No case raises the **chunk** depth above 4, and no case moves the two depths independently for a chunked workload. "Accepting a field never used is failure" is unmet for `chunk_delta_max_depth` in the *increased* direction. |
| CA7 | No clamping or fixed-depth check contradicting accepted config | `owner.rs:653,667` (uses `capacities.metadata_delta_max_depth`); `pool/read.rs:152,159`; `delta/read.rs:127,135` — all read the policy value | — | **INCOMPLETE** | No hard-coded depth clamp was found, but the **pooled** producer's encoded-work estimate `(depth+2) × METADATA_RECORD_LIMIT` (`owner.rs:673-674`) is compared against the fixed `METADATA_CHAIN_ENCODED_LIMIT` = 139 281 (`policy.rs:108`), so an accepted `metadata_delta_max_depth` above **15** can never admit a deeper chain than 15: the persisted field is accepted but not usable. `policy_capacity.rs:241-245` accepts 50 and asserts only the persisted value. |
| CB1 | WHOLE_FILE: explicit predecessor first, admitted-FULL winner cache only when none is acquired | `delta/select.rs:209-239` (`acquisition` → cache → `insert`) | `delta_payload::the_admitted_full_cache_supplies_a_candidate_within_one_save` (`delta_payload.rs:74-98`: no predecessor, `trials==1`, `prefix_records==1`) | **PASS** | — |
| CB2 | CHUNK: first supplied candidate, **not four trials** | `delta/select.rs:210` | `delta_payload::native_delta_uses_the_first_supplied_eligible_candidate_only` (`delta_payload.rs:101-138`: two candidates, `trials==1`) | **FAIL** | The "not four trials" half is proven; the "first **eligible**" half is not implemented (B1) and the test supplies only *stored* candidates. |
| CB3 | Framed-cost comparison including base identity | `delta/record.rs:37-92` (base id is inside the record); `delta/select.rs:270`; `owner.rs:512` | `an_unrelated_candidate_loses_the_cost_comparison`; `a_losing_delta_trial_stores_the_leaf_in_full` | **PASS** | Comparing `record.len()` is equivalent to comparing framed cost, because group framing adds `4 + 4×records` identically to both candidates for the same lane (`pack/assemble.rs:29-75`). |
| CB4 | Batch distinct independent candidate lookups | — | — | **N/A** | With the specified one-first-candidate rule there is no probe loop left to batch; the reference's point-probe loop is deliberately absent (handoff §3B). |
| CB5 | Iterative (not recursive) reconstruction | `delta/read.rs:128-171` (explicit `Vec` chain, then a reverse loop); `pool/read.rs:151-215` | `delta_payload::a_delta_chain_is_resolved_iteratively_and_reaches_every_step` (asserts `edges==6`, `max_depth==3`); `metadata_chain` nine-record chains | **PASS** | No recursion appears on the reconstruction path. |
| CB6 | Separate depth / encoded-work / decoded-work / live-memory bounds | `policy.rs:98-118`; `delta/read.rs:180-203`; `pool/read.rs:82-101,178-188`; `delta/select.rs:27,144-148` | `delta_chains::a_chain_that_would_exceed_its_work_budget_is_never_stored`; `metadata_chain` budget case; `metadata_pool_index::the_retained_set_is_bounded_by_entries_not_by_file_size` | **PASS** | Depth (`whole_file`/`chunk`/`metadata`), canonical work, encoded work, decoded value work (`METADATA_DECODED_WORK_LIMIT` 32 MiB, `pool/read.rs:85`), the value cache (`VALUE_CACHE_BYTES` 512 KiB, `pool/read.rs:27,95-98`), the depth cache (4 096, `select.rs:27,144`) and the ordered set (131 072, `policy.rs:116`) are genuinely distinct constants. |
| CB7 | Role, length, chronology, cycle, intermediate-identity checks | `delta/read.rs:140-146,164-166`; `pool/read.rs:148-150,164-175,203-209` | cycle and intermediate cases **PASS**; role case **INCOMPLETE** (see A1c) | **INCOMPLETE** | The role/length checks exist in code; only the cycle and intermediate ones have a real oracle. |
| CB8 | Captured read ceiling applied to cache **and** base reads | `cas/store.rs:210-219` (single `retained_pack_ceiling` read), `cas/read.rs:53-68` (every requested locator checked), `delta/read.rs:72,83,138` (ceiling passed to every `lookup::location`), `pool/read.rs:75-80` | `visibility::an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement`; `a_watermark_ahead_of_storage_is_rejected_at_open`; `the_watermark_survives_reopen_and_still_hides_a_later_open_save` | **PASS** | The per-wave pack cache can only contain packs whose locators passed the ceiling check. |
| CE1 | Placement before ONE selected assembly | `owner.rs:336-377` (select → `placement[lane].select_many` → `write_pack`), `pack/placement.rs`, `pack/assemble.rs:174-251` | `pack_locator::a_full_lane_starts_a_new_pack_and_keeps_existing_locators`; `cas_reuse::one_operation_can_save_two_files_into_shared_packs` | **PASS** | `assign_object_id`/`assemble` is called exactly once per sealed group; no candidate pack is built and discarded (`assemble.rs:1-7`). |
| CE2 | Bounded grouped acquisition; on-demand seal; stable locators; pack density | `owner.rs:269-302` (`seal_pending`), `cas/store.rs:320-376` | `pack_locator::same_save_reads_see_accepted_objects_before_the_finish_barrier`; `same_save_reads_resolve_objects_waiting_in_an_unfinished_group` (5 MiB file, >256 ids, every identity read inside its own save) | **INCOMPLETE** | Correctness of the on-demand seal is proven; the handoff asks to "test its **packing cost and memory**" — neither is measured or reported. |
| CE3 | Required format/profile readers, explicit dispatch, no trial decode, no RAW switch after error | `pack/layout.rs:224-275` (`match version` → v1/v2/v4/v6/v7, else `UnsupportedPolicy{field:"pack framing version"}`); `encoding/decode.rs:44-134`; `delta/record.rs:103-108` | `physical_formats::every_implemented_framing_is_recognized_by_its_own_version` (1, 2, 4, 6, 7); `unimplemented_and_unknown_framings_are_rejected_without_a_trial_decode` (0, 3, 5, 8, 99 → the named unsupported error; wrong magic → `Integrity`, truncated directory → `Integrity`); `the_lane_table_names_one_limit_per_lane`; `a_recipe_frame_larger_than_its_profile_is_refused` | **PASS** | Exactly the required matrix: v1/v2/v4/v6/v7 recognized, v3/v5 and unknown rejected, with no second decoder attempted after an error. |

### 2.5 Handoff §4 required external verification — row by row

| Row (handoff §4) | Target(s) | Test target / name | Oracle actually asserts | Status | Gap |
| --- | --- | --- | --- | --- | --- |
| 1 | C1 `inode_leaf` + `object_identity` | `inode_leaf` (6), `object_identity` (5) | exact grammar in both forms; malformed rows/order/counts/lengths rejected (`inode_leaf.rs:119-173`) | **PASS** | — |
| 2–7 | C1 `edit_single`/`edit_batch`/`edit_model`/`edit_transitions`/`edit_noop`/`edit_bounds`/`edit_file_read`/`edit_timing`/`file_complete`/`streaming` | present and green in `checks.log` | — | **N/A** | Stage 4 / #169 scope, reviewed separately; listed here only because `checks.log` shows they run at this HEAD. |
| 8 | C2 `delta_payload` | see A2, A2b, A2c, CB1–CB3 | — | **FAIL** | The CHUNK absent/ineligible sub-case is missing (B1); every other sub-case has a real oracle. |
| 9 | C2 `delta_chains` | see A5b, A5c, A1c | — | **INCOMPLETE** | Wrong-role oracle mismatch; no changed chunk depth; pooled changed depth untested. |
| 10 | C2 `metadata_pool` | see E1a–E1d | — | **INCOMPLETE** | 165-value boundary unexercised; group compression unexercised. |
| 11 | C2 `metadata_pool_index` | see E2a–E2c | — | **INCOMPLETE** | Phantom-ordinal oracle is weak; pending same-save reuse untested. (Handoff allowed "authentic fingerprint-collision fixtures where available" — a genuine searched collision is retained, so that sub-item is **PASS**.) |
| 12 | C2 `physical_formats` + `policy_capacity` | see CE3, CA2–CA5 | — | **PASS** | — |
| 13 | C2 `edit_pipeline` | `edit_pipeline::a_small_edit_round_trips_through_a_reopened_store`; `a_chunked_edit_round_trips_and_reuses_retained_payloads` (`edit_pipeline.rs:92-148`, asserts fewer inserted objects than the base holds); `a_multi_edit_transition_round_trips_through_the_store`; `a_standalone_route_and_an_integrated_route_agree_on_the_root` | real C1 edit → C2 → finish → reopen → readback of the root's exact canonical bytes; no full-store scan, no old-runtime dependency | **PASS** | The multi-edit case uses one edit per sub-case, so a *multi-edit chunked* save (the sequence that would exercise the CHUNK first-candidate path with a same-save predecessor) is absent. |
| 14 | existing C2 `cas_reuse`/`pack_locator`/`visibility`/`persistence_failure` | `cas_reuse` (7), `pack_locator` (7), `visibility` (5), `persistence_failure` (6) | duplicates within one wave and across waves insert once (`cas_reuse.rs:91-142` asserts `inserted == distinct`); unfinished-group owner reads; private early commits; retained watermark; failed-save cleanup with retained objects intact | **PASS** | — |
| 15 | existing C2 `memory_bounds`/`timing` | `memory_bounds` (7), `timing` (4) | declared limits enforced from the bytes (not from policy); direct canonical singleton; no payload file; enabled/disabled parity | **PASS** | "All live lanes/base/full/delta/pack/SQL/index/report ownership" is asserted as *declared capacity*; no observed heap/SQL figure exists (the target states this honestly in `memory_bounds.rs:1-6`). |

### 2.6 Reviewer-handoff area table (Stage 3 rows)

| Area | Finding | Status |
| --- | --- | --- |
| Isolation | `layerfs-content/Cargo.toml` depends only on `blake3` and `layerfs-telemetry`; no `rusqlite`/`sqlite` token anywhere in `layerfs-content/src`; no `layerfs-storage` reference; no `crates/`, `layerfs_workspace`, FUSE, daemon, history or checkpoint reference in either product crate. C1 logical edits run against a supplied provider (`edit_pipeline.rs:29-54` builds a `Provider` from a `Collected` with no Store). C2 saves supplied canonical objects without C1 construction (`metadata_pool.rs` builds leaves directly). | **PASS** |
| Configurable transparency | Defaults, ranges, rejection, derived capacities, reopen agreement, 256-KiB and 1-MiB end-to-end cases all present; the increased-chunk-depth demonstration and the pooled depth >15 usability gap are missing. | **INCOMPLETE** |
| Payload encoding | WHOLE_FILE ordering, framed-cost comparison and single-trial rule are correct; the CHUNK first-candidate eligibility check is missing and fatal. | **FAIL** |
| Dependencies | Iterative, separately budgeted, ceiling-aware; role-check oracle missing and pooled intermediate tamper uncovered. | **INCOMPLETE** |
| Physical metadata | Grammar, ordinals, catalogue, pooling, pooled DELTA, window and cleanup are real and well covered; 165-boundary and group-compression coverage are not. | **INCOMPLETE** |
| Pack/read path | Explicit version dispatch, bounded directory validation, placement-before-assembly, stable locators, singleton lane — all verified. Packing density not measured. | **PASS** (density INCOMPLETE) |
| Single edits / no-op / transitions / multi-edit finality | Not audited here. | **N/A** (Stage 4 / #169) |
| Storage regressions | All five named behaviours have real oracles with externally induced damage. | **PASS** |
| Independent measurement | Real C1/C2/integrated scopes exist and enabled/disabled parity is proven; the pooled lane's simultaneous memory is unmeasured and its receipts are one commit behind HEAD. | **INCOMPLETE** |
| Resource/safety | Declared capacities, no payload spill, FFI bounded and documented; no observed memory figure; no unsafe outside the zstd FFI wrappers, all with stated preconditions, validated frame headers and a cleared prefix after every decode. | **INCOMPLETE** (safety PASS, measurement INCOMPLETE) |
| Delivery/constraints | `check_product_boundary.py` PASS (75 files); no `#[test]`/`cfg(test)`/`mod tests` in any product `src/`; all tests are external under `core/crates/*/tests/`; `core/Cargo.lock` present and every parent command used `--locked`; no `[patch]`/`[replace]`; `preflight.sh` appears only in text stating it is retired. | **PASS** |

---

## 3. Findings detail

### F1 — HIGH — CHUNK delta candidates are used without an eligibility check (demonstrated defect)

```text
core/crates/layerfs-storage/src/encoding/delta/select.rs
209   let candidate = match role {
210       ObjectRole::Chunk => advisory.first().copied(),      <-- no eligible() call
211       _ => match acquisition(input, role, advisory, depth_cap)? { ... }
...
243   let base = acquire(input, base_id)?;                      <-- hard error if absent
```

`acquire` (`select.rs:333-343`) calls `Resolver::resolve_dependency`, which does
`lookup::location(...).ok_or(StorageError::ObjectMissing(id))` (`delta/read.rs:83-84`).
`flush_batch` passes the object's advisory list through unfiltered
(`cas/save.rs:67-68`), and `Availability::validate` checks only the object's *direct
references* (`cas/dependencies.rs:49-86`), never the advisory list. The result
propagates through `offer` → `SaveOperation::flush` → `terminate` and fails the whole
save. A first candidate of a different role instead fails inside `raw_payload`
(`encoding/full.rs:51-59`).

Trigger, stated at the C2 public API: build a `FinalizedObject` of role `Chunk`
carrying one advisory predecessor that is (a) not stored, (b) stored under another
role, or (c) accepted into this same save but not yet sealed. `WHOLE_FILE` handles
(a) and (b) correctly today (`select.rs:301-331`); `CHUNK` does not.

Same-save reachability (inference from the code path, **not executed**): `Native`
lane groups seal only at `GROUP_TARGET` = 48 KiB (`owner.rs:27,349-357`) or at
`finish` (`owner.rs:818-820`), so a chunk accepted earlier in the save has no locator
row. C1 supplies such an id whenever a later edit's retained left subtree ends in a
payload object emitted by an earlier edit in the same batch
(`file/edit/apply.rs:238-243` `rightmost_payload(left)` → `mapping/build.rs:117-134`
`push_chunk(chunk, predecessor, …)`). No retained test builds a multi-edit, chunked,
single-save sequence, so this is unverified in either direction.

Smallest fix: apply the same rule as `WHOLE_FILE` — advance to the first *eligible*
advisory candidate (reusing `eligible()`, which already counts
`absent_candidates`/`ineligible_candidates`), keep the one-trial rule, and add the
missing `delta_payload` case for CHUNK.

### F2 — HIGH — the two issue-required gates are unmeasured

- "Qualify real pooling/payload/pack operations, retained footprint and **simultaneous
  index/codec/SQL memory**": the addendum declares no such arm
  (`stages-3-4-measurement-addendum.md:92-106`); the receipts report
  `Store::pool_index_entries/bytes` only; the matched-pair ledger calls the gate
  "unmeasured, not passed".
- Acceptance item "storage/index/**column** footprint, memory and speed claims are
  measured": the pooled lane has no matched reference counterpart and no column-level
  footprint; the matched C1 pair (different scope) explicitly declines a storage,
  latency and memory conclusion (`stages-3-4-matched-c1-20260917T050000Z/ledger.md`,
  conclusions 2–4).

### F3 — MEDIUM — same-save delta-base eligibility is neither preserved nor documented

Required by the issue: "Audit same-save delta-base eligibility and admitted-FULL cache
deviations; **preserve** required candidate/selection behavior **or document the
explicit decision**". The cache half is a faithful port of
`crates/layerfs-layerstack-store/src/objects/small_candidates.rs` (identical constants
and algorithm). The eligibility half is neither preserved nor documented: same-save
objects are not eligible bases for the `Native` lane, and supplying one is fatal (F1).
No document in `docs/roadmap/0.1/0.1.7/component-decoupling/` records this decision.

### F4 — MEDIUM — pooled group compression has no coverage

`encoding/pool/value_group.rs:47-53` hashes the decoded body and stores a Zstandard
frame when `frame.len() + 16 <= raw.len()` (`encoding/codec.rs:613-627`). Every
fixture's 73-byte values are blake3 outputs of a seed (`object/id.rs:24-29`), i.e.
high entropy, and `fresh` is deduplicated, so no fixture can compress. Consequently
`value_group.rs:50` and its reader `pool/read.rs:121` are never executed in any
retained run — while the issue's completion gate E1 explicitly names
"body/digest/**compression**".

### F5 — MEDIUM — accepted metadata depth is effectively capped below its accepted range

`owner.rs:673-674` estimates the encoded chain cost as
`(cost.depth + 2) × METADATA_RECORD_LIMIT` = `(depth+2) × 8192` and compares it with
the fixed `METADATA_CHAIN_ENCODED_LIMIT` = `17 × 8193` = 139 281 (`policy.rs:108`).
`(depth+2) × 8192 > 139 281` for `depth ≥ 16`, so a persisted
`metadata_delta_max_depth` above ~15 can never admit a deeper chain than 15. Handoff
§3A: "accepting a field never used is failure"; `policy_capacity.rs:241-245` accepts
50 and asserts only the persisted value. The estimate also over-charges every real
record (most pooled records are far smaller than 8 192 B), so it refuses chains the
reader would in fact accept.

### F6 — LOW/MEDIUM — test names that overclaim their oracles

- `metadata_pool::values_cross_a_group_boundary_at_one_hundred_sixty_five`
  (`metadata_pool.rs:136-174`): three leaves of 100 distinct values each; no group ever
  holds 165 values and no leaf spans two groups. The multi-group branch
  (`owner.rs:576-587`) is unreachable because a leaf carries at most
  `MAXIMUM_LEAF_ROWS` = 100 fresh values (`inode_leaf.rs:24`).
- `delta_chains::a_wrong_role_dependency_is_rejected` (`delta_chains.rs:166-198`):
  injects a non-existent id, so the oracle is `ObjectMissing`, not the role check at
  `delta/read.rs:140-142`.
- `metadata_pool_index::a_failed_save_invalidates_the_set_instead_of_reusing_phantom_ordinals`
  (`metadata_pool_index.rs:160-203`): the failed save's values (100..108) are never
  re-offered, so the oracle cannot observe a phantom ordinal.
- `encoding/pool/value_group.rs:6` states "The group bound is 16 KiB, which fixes 165
  values per group". The framing maths (`pack/assemble.rs:29-75`: `4 + 86×count`) admits
  190 values in 16 KiB; 165 is the reference's SQLite `CHECK` bound
  (`crates/layerfs-layerstack-store/sql/schema/v10.sql:16`), which the candidate also
  uses (`sql/schema.sql:41`). The constant is right; the stated derivation is not.

### F7 — LOW — receipts pinned one commit behind HEAD

The timing round is sealed at `dfd54fd8e8ee5f633f9ff80de2967c4c25b6f2c2`
(`stages-3-4-timing-20260917T031000Z/README.txt`), receipts remain valid for the product
sources but not for HEAD's tree identity. `git diff --stat dfd54fd8 91c3a074 -- core/crates`
shows only `examples/edit_timing_c1.rs` (new) and `tests/edit_reference.rs` (changed), so
no product logic differs.

---

## 4. What I could NOT verify

1. **Nothing was compiled, built or executed by this review.** Every test result cited
   comes from `checks.log`, which the parent agent produced at
   `91c3a0741fff64e8161d5c1b6e759f347ffbf757`. I did not re-derive any of it, and I did
   not run `cargo test`, `clippy` or `fmt`.
2. **Whether F1 is reachable from a real C1 edit stream.** The C2-level defect is
   demonstrated by reading the code; the concrete multi-edit chunked sequence that would
   trigger it (an edit whose retained left subtree ends in an object emitted earlier in
   the same save) is inferred from `file/edit/apply.rs:238-243` and
   `file/mapping/build.rs:117-134` and was **not** executed. No retained test covers it.
3. **Whether any pooled value group has ever been stored compressed.** I established
   that every fixture's values are high-entropy and therefore incompressible; I did not
   inspect a receipt's on-disk group codec byte, so a compressed group may exist outside
   the reviewed fixtures. Reported as missing coverage, not as a defect.
4. **Pack density / occupancy and tail waste** for any lane: no measurement exists and I
   could not produce one without running the product.
5. **Simultaneous index/codec/SQL memory**, process memory and column footprint: no
   instrumentation exists in the reviewed tree; no retained figure.
6. **A pooled-lane matched v0.1.6 comparison**: the reference exposes no public pooled
   save/read operation, so speed and storage parity remain unproven. I did not attempt
   to construct one.
7. **The 165-value group path, the changed chunk-depth path, the increased pooled-depth
   path, the pooled intermediate-tamper path and the pending same-save reuse path**:
   unreachable or unexercised by the retained tests (F4, F6, A5c, E2c, E3b).
8. **Whether `metadata_delta_max_depth` > 15 behaves exactly as F5 describes in a live
   chain**: derived from `owner.rs:673-674` and `policy.rs:108` arithmetic; not executed.
9. **Stage 4 / #169 criteria** (single edits, no-ops, transitions, multi-edit finality,
   the 80+100→90+90 oracle, the matched reference edit pair): out of scope here.
10. **Release-level claims** (v0.1.7 release readiness, Workspace/FUSE/cloud limits):
    not reviewed.
