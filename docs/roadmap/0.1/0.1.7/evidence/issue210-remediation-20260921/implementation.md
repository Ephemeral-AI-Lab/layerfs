# Pair 2 history remediation implementation

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This is new evidence for #210, based on reviewed product
`92e56635ae4559d175fe3cd455f36f9fe6b5b498` and imported documentation
`35740836f84b9ef687da2f70d785c9e6b2ed2fd2`. The original reviewed worktree and
its branch were not changed. Work used the isolated managed worktree
`/Users/yifanxu/.codex/worktrees/b995/layerfs` on
`codex/pair2-history-remediation`.

The repaired contract was recorded before dependent changes in
[the P0 note](../../../../../../core/docs/architecture/proposal/commit-history/remediation-contract-20260921.md).
Architecture [paper 16](../../../../../../core/docs/architecture/16-history.md)
follows the source. Native history callers now supply a cursor capability;
profile-2 root descriptors and failure context change as described there. C5 DDL
and C2 schema 7 are unchanged, as are valid profile-1/opcode-1–5 encodings.

## Reproduction and repaired behavior

The archived diagnostic sources were temporarily installed as external baseline
probes before production edits and removed after execution. All 13 diagnostics
reproduced on this baseline (8 catalog, 1 short-page codec, 2 delivery/budget,
2 service). Their PASS means reproduction, not acceptance. The original receipts
remain untouched. New external regressions assert corrected behavior and reuse
the existing service/catalog fixtures instead of copying a second fixture suite.

| Finding | Source-backed disposition and regression |
| --- | --- |
| R01 | Typed conflict and deciding-transaction stage context reach direct/native failures. Service `stale_loser_retains_its_exact_stage`, `delayed_discard_token_cannot_consume_a_replacement_stage`, `refreshed_stack_head_reports_typed_stale_base_on_direct_and_native_routes`, and `composite_failure_keeps_acknowledged_stage_distinct_from_absence` cover retained, absent and acknowledged/unknown disposition. The composite provider refusal is an external public-contract test, not an I/O-loss qualification. |
| R02 | Cursor v2 authenticates subject/anchor/position with an authority key. `forged_sibling_position_and_obsolete_checksum_are_refused` rejects sibling substitution and recomputed public checksums under both domains. |
| R03 | Decoder uses legal minima 117/71/116/85/309. Bridge `minimum_and_maximum_record_encodings_match_the_frozen_widths` covers all five types including short headless Branch/genesis pages. |
| R04 | Name cursors carry immutable IDs and look up checked ordering keys; `all_legal_name_widths_paginate_by_identity` covers 1/33/34/63 bytes. |
| R05 | Resume uses authenticated immutable anchors; `advancing_the_actual_subject_preserves_both_anchor_forms` covers actual Branch/stack growth and both start forms. `explicit_membership_ceiling_is_capacity_but_old_authenticated_anchor_stays_valid` covers 4,352 appended Commits and full 4,354-record traversal. |
| R06 | One DirectoryUpdate per directory, grouped independently of adjacency. Service `empty_and_interleaved_directories_have_real_canonical_pages` covers root-only, empty child/nested/interleaved directories; existing manifest tests cover files, symlinks and invalid inputs. |
| R07 | Unknown body/COMMIT errors suppress RAII rollback using rusqlite 0.40.2 DropBehavior::Ignore; cleanup failure is unknown; uncertain provider denies reads/writes. `unknown_statement_outcome_quarantines_without_implicit_rollback` uses a disposable external broken SQL trigger, observes the retained writer lock and unchanged committed rows. This is conservative statement-error behavior, not a real I/O/connection-loss schedule. |
| R08 | Service captures C5 then opens that immutable root through C1 outside C5. Descriptor includes actual root serial; `branch_root_descriptor_validates_content_scope_profile_and_actual_serial` covers missing/wrong-role/scope/profile and serial 9 after prior reservations, including native reply. Init returns known construction descriptor; Fork remains metadata-only with absent serial. |
| R09 | Open compares SQLite-parsed frozen definitions, explicit indexes, constraints and table properties; stored binding must derive catalog identity. `reopen_refuses_each_independent_schema_or_binding_corruption` includes missing index, CHECK/FK/STRICT/column/binding/range corruption and sqliteX/sqliteY foreign objects. Read-only continuity tests remain. |
| R10 | Exact-source UpToDate comes first, with immutable provenance revalidation; stale base precedes NoChanges/insertion. `stale_base_precedes_no_changes_and_derived_provenance_collision` and `exact_source_publication_revalidates_immutable_provenance` cover both orders. The original sibling collision receipt remains valid historical observation; governing spec requires its normal stale-base request to return StackMoved before insertion/provenance collision testing. No row is overwritten. |
| R11 | Exclusive end checked before allocation publication. `last_representable_reservation_is_consumed_and_terminal_refusal_is_atomic`, `concurrent_reservations_never_overlap_and_discard_does_not_refund`, and existing allocation tests cover real high-water fixtures, unchanged refusal, concurrent admission and no refund. |
| R12 | Encoder/decoder enforce complete 16 KiB history result budget; `history_page_budget_includes_tags_counts_and_continuation` covers exact boundary and oversized input/encoding. |
| R13 | Init/StageChanges/Commit are content mutations; remaining commands are metadata-only. `classification_is_exhaustive_and_semantic` covers every variant; service metadata test checks unchanged C2 file identity. |
| R14 | Native client refuses all non-ReadFile ResultData before writing. `mutation_result_data_is_rejected_before_any_output` covers authenticated legacy and history mutation peers. |
| R15 | No remediation required: pre-existing fixed-width expect in service read helper, not a newly reachable panic. That source is unchanged; no repository-wide cleanup was authorized or performed. |

## Independent source review

A read-only reviewer subagent inspected the repaired diff, pinned contract and
locked rusqlite 0.40.2 source. It found and re-reviewed two corrected issues:
a repeated membership walk that capped valid continuation traversal, and the
SQL LIKE underscore wildcard hiding foreign objects. Final source disposition:
no outstanding actionable finding. See [review record](independent-review.md).
This review does not substitute for executing tests or qualification schedules.

## Verification boundary

Pre-commit focused catalog regressions, bridge codec/delivery tests and direct/
native service history tests passed. The boundary guard and warning-denying
workspace Clippy passed after fixing the schema-row type-complexity lint.
Early failed attempts are retained in the later validation bundle: a schema test
needed the external SQLite defensive mode disabled to edit its disposable fixture;
old test assertions needed to inspect the new contextual error; native typed
failures exposed the old generic three-byte frame cap. None is omitted as a
passing run. Full committed-head checks and actual route receipts are recorded
separately after this implementation commit, not borrowed from the author/reviewer.

No benchmark, performance campaign, aggregate gate, CI/preflight, worker/cache/
timeout retuning, third-party change, retry or guessed rollback was performed.
The seven C5 tables, C2 schema 7, W=2/two saves/Q=0, exact successful finish before
stage and no cross-database lock nesting remain. Full H01–H14 qualification is
not asserted; H04/H06/H08/H14 require their separate observations.

## Production LOC for this commit

Production LOC: 95665 -> 96344 (delta +679).

Core: 30248 -> 30927 (+679). Reference: 65417 -> 65417 (+0).
Combined: 95665 -> 96344 (+679). No reference retirement, relocation or new
application-adapter scope; this is additional repaired product behavior/validation.
Tests, documentation, tools, manifests and generated artifacts are excluded.

Method: export first parent and final staged tree using `git archive <tree>
core/crates crates tools/production_loc.py`, then run the identical
`python3 tools/production_loc.py --root <export> --detail --json` counter on both.
Counter SHA-256:
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
It strips comments/blanks and legacy inline test code, counts Rust product source
plus shipped runtime SQL, and reports core/reference scopes separately. Final
committed-tree equality and raw count receipts are retained in the validation
record; no unstaged/untracked source contributes to the comparison.
