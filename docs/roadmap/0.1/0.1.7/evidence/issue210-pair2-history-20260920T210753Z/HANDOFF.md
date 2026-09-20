# #210 — pair 2 (history) implementation handoff

> Status: Research; informative and not a product contract. No performance,
> release or qualification claim is made by this document.

Issue: [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210). Design parent:
[#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180). Specification and pre-publication
audit read at `c85cf6b69b3809d860caaad764a09e86a54ece9a` (PR #211, not merged).

Pull request: [#212](https://github.com/Ephemeral-AI-Lab/layerfs/pull/212).
This document closes nothing. #179, #180, #192, #193 and #209 remain open and are not implied by it.

## Identities

| Item | Value |
| --- | --- |
| Worktree | `/Users/yifanxu/.codex/worktrees/pair2-history-implementation/layerfs` (independent; source checkout untouched) |
| Branch | `codex/pair2-history-implementation` |
| Base (audited baseline) | `a02168adbb1b02571941654919cefca12dbc1f42` |
| Commits | `018d9c366`, `572d61f03`, `daccb403a`, `6e310b249` (tip) |
| Toolchain | `cargo +1.85.1` |
| Lock delta | `core/Cargo.lock` gained exactly one package, `layerfs-history`; every other pin and checksum byte-identical |
| `core/crates/layerfs-history/sql/schema-v1.sql` | `b95ae1749245b9047b007f031fa4d84bbb4d80f311fb8547060fd46da634fb95` |
| `core/crates/layerfs-daemon/tests/history_route.py` | `0c9870638a8aaa616dc2c664b8e4629fd3c31703e53b6f3562a752d2002d1664` |
| `core/docs/architecture/16-history.md` | `f18ab9659031ffbac66ebfe7897abfc85d05b6e6f304637cff9010a9a83d4c44` |
| `layerfs-service` binary used by the route driver (debug) | `8ff7116c020e2d868136638dc20ab9248df19d10fcb2eab92b21c5c942b47852` |

## What landed

One new crate, `core/crates/layerfs-history`, plus its service capability. Operations:
`init_layerstack`, `fork`, `read_branch`, `stage_changes`, `commit_staged`, `commit`, `add_layer`,
`discard_stage`, `reserve_inodes`, and typed get/list, Commit-history, Layer-history and stage
inspection. Catalog schema 1 is a separate file (application id `1279677256`) with the seven
specified tables; C2 stays schema 7 and is never touched from C5.

Service and bridge: opcodes 6 (`HistoryQuery`) and 7 (`HistoryCommand`) on operation profile 2, an
exhaustive checked permission mapping, exhaustive semantic dispatch replacing `opcode >= 3`, and
complete request/result codecs, count/length validation, failure decoding and native-client
response matching. The daemon's generic framed relay is reused unchanged.

## Verification (re-run at `6e310b249`, clean working tree)

| Command | Result |
| --- | --- |
| `cargo +1.85.1 test --offline --workspace` | 103 test binaries, 0 failures |
| `cargo +1.85.1 build --offline --workspace --examples` | PASS |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS |
| `cargo +1.85.1 clippy --offline --workspace --all-targets -- -D warnings` | 0 errors |
| `python3 core/tools/check_product_boundary.py` | PASS, 193 files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK |
| `cargo +1.85.1 build --offline -p layerfs-history --no-default-features` | PASS |
| `python3 core/crates/layerfs-daemon/tests/history_route.py --output <dir>` | PASS, 13 cases, host client |

External test counts: 16 service-level cases through `Service::handle`; 18 catalog-level cases
through the public `HistoryCatalog` API; 11 codec-level cases in
`layerfs-bridge/tests/history_protocol.rs`; 13 driver cases over the production service/daemon
transport.

No CI, aggregate preflight, benchmark or performance run was used or claimed. `tools/preflight.sh`
was not run and is not restored.

## Acceptance cases

| ID | Status | Where |
| --- | --- | --- |
| H01 | PASS | service `empty_namespace_initializes_and_reads_back`, `manifest_initialization_builds_a_real_namespace`, `wrong_role_root_is_refused` |
| H02 | PASS | catalog `fork_shares_content_and_uses_the_selected_commit_base`, `history_reads_return_the_recorded_ancestry_and_chain` |
| H03 | PASS | service `stage_commit_add_layer_and_read_back` |
| H04 | **NOT_RUN** | no production observation establishes overlapping C2 save lifetimes or the reverse-completion schedule; no fault hook, sleep or alternate algorithm was added to fabricate it. The existing C2 W=2 receipt is not counted as service/history evidence. |
| H05 | PASS | service `stale_loser_retains_its_exact_stage`, `multiple_no_change_stages_are_up_to_date`; catalog `a_stale_loser_keeps_its_exact_stage` |
| H06 | PASS (partial) | catalog `two_branches_commit_independently_and_one_wins_the_stack_head`; the "independent-stack work is not held behind a catalog lock" half is argued from the short-transaction structure, not measured |
| H07 | PASS | service `delayed_discard_token_cannot_consume_a_replacement_stage`, `already_published_source_is_up_to_date_before_stale_head_refusal`; driver R10–R12 |
| H08 | **PARTIAL** | known/unknown outcome classes and retained stages are asserted; an actual identical-root publication by a second writer is not exercised |
| H09 | PASS | catalog `a_read_only_reopen_reads_every_record_and_mutates_nothing`, `a_stage_written_by_one_handle_is_visible_to_a_read_only_handle`; driver R12 |
| H10 | PASS | catalog `reservations_are_monotone_half_open_and_per_scope`, `a_consumed_reservation_survives_a_later_failure`, `counts_are_checked_and_the_terminal_endpoint_is_refused` |
| H11 | PASS | service `legacy_mask_grants_no_history`, `history_profile_and_opcode_must_agree`; bridge `permission_bits_are_total_and_legacy_mask_grants_nothing`; driver R00 |
| H12 | PASS | service `metadata_only_commands_never_touch_the_content_store` |
| H13 | PASS | catalog `history_pages.rs`; `--no-default-features` build |
| H14 | **NOT_RUN** | no component-substitution harness was run in this workstream |

Deployment: the host production-transport route PASSes. The Linux Docker daemon/service route is
**NOT_RUN** — this checkout has no Linux target installed for the pinned toolchain and no image
built from this source, so no Docker execution is claimed.

## Defects found by the new tests and fixed in this branch

1. **Lineage continuation off by one** (`018d9c366`): a cursor named the last delivered record while
   an ancestry walk resumed *at* it, so every continued page repeated a record. Every range now
   resumes strictly after the named record.
2. **Over-strict manifest count pre-check** (`572d61f03`): it used a typical entry width (56 bytes)
   instead of the smallest legal one, so a manifest of small entries was refused as `Capacity`
   before it was read. The bound is now 24 bytes.
3. **Understated LayerStack record width** (`6e310b249`): the catalog assumed 116 bytes where the
   codec writes 179, so it would have packed a page the codec then refused — 128 records instead of
   the 90 that fit. A service-level test now measures the encoded width of a maximal record of every
   kind and holds it to the declared figure.
4. **Failure-class disagreement** (`6e310b249`): `ReserveInodes` with a zero count was `Capacity` in
   the bridge and `InvalidInput` in C5; the bridge now agrees with the catalog.

## Production LOC

`python3 tools/production_loc.py --root <tree> --detail`, first parent versus the committed tree.

| Commit | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `018d9c366` feat: implement C5 history | 90811 | 95661 | +4850 |
| `572d61f03` test: drive the history route | 95661 | 95662 | +1 |
| `daccb403a` test: same-stack publication race | 95662 | 95662 | 0 |
| `6e310b249` test: codec coverage + two bounds | 95662 | 95665 | +3 |

First commit subtotals: core 25394 → 30244 (+4850: `layerfs-history` +2523 new, `layerfs-bridge`
+1421, `layerfs-service` +906); reference 65417 → 65417 (0). Tip: core 30248, reference 65417,
combined 95665.

Component variance against the specification's plan: history+SQL 2523 (plan 2505), service 1738−832
= +906 (plan 900), bridge +1425 (plan 650). The bridge is 2.2x its component allocation; the
combined change is inside the 3500–5000 envelope. The overshoot is concentrated in the codecs (18
suboperations, 16 reply shapes, 5 record shapes, each with a hand-written encoder and decoder) and
in pre-mutation validation. Two deliberate duplications are recorded in the PR rather than removed:
the manifest grammar is validated at both the bridge trust boundary and inside C5, and
`check_prepared_lists` repeats bounds the legacy prepared-update arm already applies.

## Limitations

Stated in full in `core/docs/architecture/16-history.md` section 16.9. In short: no writable service
restart (read-only reopen only); no host import or arbitrary root registration; `stage_changes` is
limited to the existing-inode prepared-update surface; no rebase/merge/retry/conflict resolution; no
GC or serial refund; no crash-atomic cross-store claim; failure frames carry the typed class but not
a byte-packed expected/actual block; Linux/macOS only.

## Independent review

**Not performed.** The subagent channel was unreachable in this environment: `codex exec` fails on a
model/CLI version mismatch for every available model, and the `claude` CLI has no valid credential.
The audit recorded here is adversarial self-review plus the external tests above, and must not be
read as independent verification. `REVIEW-PROMPT.md` in this folder is the prompt for that review.
