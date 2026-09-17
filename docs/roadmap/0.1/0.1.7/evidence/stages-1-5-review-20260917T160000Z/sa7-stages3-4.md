# SA7 — CUMULATIVE review of Stages 3–4 in the final tree (c99a8d9)

Read-only source review. Snapshot `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6`, branch main,
clean except the untracked review directory. `cargo` was not run (lead reviewer owns builds/tests),
so every "actual test/assertion" cell below is a **static** reading of the test source: existence,
assertion content and target. Execution status for every row is **NOT_RUN (by this reviewer)**.

Scope: `core/crates/layerfs-storage/src/{encoding,pack,sqlite,policy.rs}`,
`core/crates/layerfs-content/src/file/{edit,mapping}`, plus the contract/closure documents.

## 1. Criterion table

| Criterion | Source | Implementation location | Actual test/assertion (static) | Status | Gap |
| --- | --- | --- | --- | --- | --- |
| 1a Cutoff configurable, checked | handoff §3.A | `content/policy.rs:81-102`, `16-18`; `sqlite/schema.rs:180-222`; `sql/schema.sql:18-19` | `policy_capacity.rs:27,51,161,258` (accepted 128 KiB/256 KiB/1 MiB; 65 536/196 608/1 048 577 and profile 2 rejected, file not created) | PASS (static); NOT_RUN | — |
| 1b Three depths, separate defaults | handoff §3.A | whole-file 8 `content/policy.rs:20`; chunk 4 `content/policy.rs:22`; metadata 8 `storage/policy.rs:110`; max 50 `content/policy.rs:24` | `policy_capacity.rs:215-255` asserts metadata depth 12 persists, survives reopen, 50 accepted, 51 rejected; `cache:274` | PASS (static); NOT_RUN | — |
| 1c Depths independently bounded, not one number | handoff §3.A | `storage/policy.rs:130-136,152-171,174-196` (metadata checked at `:190-194` after `ConstructionPolicy::validated` at `:180-189`); role map `policy.rs:315-321`; DDL `schema.sql:20-26` | `policy_capacity.rs:274-364`: shallow(4,4) vs deep(50,50) derive identical chain/batch/transaction/pack/group/metadata-chain limits; `:221-224` metadata 12 leaves 8/4 untouched | PASS (static); NOT_RUN | — |
| 1d Persisted policy checked on open, before mutation | handoff §3.A | `sqlite/schema.rs:126-133` (expected vs stored), `:180-222` (re-validate), `schema.sql:11` user_version 4 | `policy_capacity.rs:71-107` (out-of-band in-range value accepted; in-DDL but policy-invalid 196 608 fails open), `:229-255` | PASS (static); NOT_RUN | see F2 (identity unchanged across the Stage 5 role widening) |
| 2a Exact reuse before any trial | handoff §3.B | `cas/save.rs:41-49,55-59,60-68` before `owner.offer` `:71`; exact byte compare `cas/membership.rs:30-47` | `delta_payload.rs:142-157`: `reused==1, inserted==0, trials==0` | PASS (static); NOT_RUN | — |
| 2b Explicit candidate ordering, one trial | handoff §3.B | `delta/select.rs:242-274` (CHUNK `:247-253` first supplied only; else advisory in order `:348-354`, cache only when none acquired `:257`); "exactly one trial" `:275-277`; cache tie-break `candidates.rs:139-165` | `delta_payload.rs:102-139` (two candidates → `trials==1`), `:160-182` (absent / wrong-role → FULL), `:247-275` (iterative chain, `max_depth==3`) | PASS (static); NOT_RUN | — |
| 2c Framed cost comparison | handoff §3.B | `select.rs:300-322` compares `prefix.record.len() < full.record.len()`; frame carries base id `delta/record.rs`, `full.rs:155-167`; base recorded in `EncodedRecord.base` | `delta_payload.rs:185-201`: `trials==1, full_losses==1, prefix_records==0, full_records==1` | PASS (static); NOT_RUN | comparison is record bytes incl. embedded base id, not a separate cost function — matches the reference framing |
| 2d Compression / lane selection | handoff §3.A/E | profile per lane `full.rs:139-158`; compact-vs-singleton `full.rs:179-214`; lane per role `pack/layout.rs:88-105`; versions `layout.rs:246-297` | `physical_formats.rs:27,44,69,97` (v1/2/4/6/7 recognized; 0/3/5/8/99 rejected with no trial decode); `policy_capacity.rs:110-143` singleton; `codec_frames.rs` 6 | PASS (static); NOT_RUN | — |
| 2e No error-driven fallback or retry on the delta path | handoff §2 | decision sites are policy-only: depth 0 `select.rs:233-240`, no candidate `:268-274`, work budget `:290-298`, cost loss `:316-322`; failures propagate with `?` at `:277, 301, 305`; codec errors `codec.rs:108-122`, no retry (only static-context `ZSTD_*_reset`) | `delta_payload.rs:224-244`: corrupt base → save `is_err()` and the later read fails too | PASS (static); NOT_RUN | — |
| 3 Iterative authenticated reconstruction | handoff §3.B | `delta/read.rs:139-183`: chain `Vec` with `with_capacity(depth+1)` `:140`, loop `:142-159`, depth bound `:147`, role `:152`, strict locator chronology `:155`, root-first decode loop `:164-180`, identity check `:176`; pooled equivalent `pool/read.rs:215-281`; per-chain work `read.rs:191-218` (canonical/encoded budgets) | `delta_chains.rs` 9 cases (`a_wrong_role_dependency_is_rejected`, `a_cyclic_dependency_is_refused_by_chronology`, `a_corrupt_intermediate_is_rejected_during_reconstruction`, budget and depth boundaries); `delta_payload.rs:247-275` (`edges==6`, `max_depth==3`) | PASS (static); NOT_RUN | no recursion on the read path; peak = chain vector ≤ 51 entries + one base + one decoded value |
| 4a Ordinals and first-encounter order | handoff §3.E | `cas/owner.rs:601-611` (`next_ordinal` read once, checked add), `sqlite/pool.rs:44-67` (`ordinal_end`, ceiling named not truncated) | `metadata_pool_index.rs:80` (smallest ordinal across leaves and reopen), `:133` (cold sync without reassigning), `:301,367` ordinal-ceiling pair, `:402` per-leaf memo bound, `:170` failure invalidation | PASS (static); NOT_RUN | — |
| 4b Value-group digests | handoff §3.E | `pool/value_group.rs:31-65` (body framed once, hashed, then compressed), `:68-76` authenticate, `:79-93` decode with role+count+grammar checks; catalogue digest `sqlite/pool.rs:70-89`; reader `pool/read.rs:134-155` | `metadata_pool.rs` 15 cases incl. `a_corrupt_value_group_is_rejected_once`, `a_missing_catalogue_row_is_rejected`, `a_damaged_pooled_delta_leaf_is_refused`, `a_compressible_group_is_stored_as_a_zstandard_frame` | PASS (static); NOT_RUN | — |
| 4c Window boundaries, wholesale reset | handoff §3.E | cap `storage/policy.rs:127`; reset-before-insert `pool/index.rs:80-83` and `:131-134`; cold-start recurrence `index.rs:224-248` (streamed, three scalars) | `metadata_window.rs:79-130` (fills exactly 131 072, one more resets to 1 entry, refused note leaves window intact), `:133-179` (1 312-leaf real Store crossing; retained 200; evicted value rewritten not lost) | PASS (static); NOT_RUN | — |
| 4d Retained caches bounded by a declared constant, enforced before allocation | handoff §2/§3.E | index 131 072 `policy.rs:127`; pooled value cache 512 KiB `policy.rs:106` enforced `pool/read.rs:147-153`; dependency pack cache 4 MiB `policy.rs:98` enforced `delta/read.rs:254-257`, `pool/read.rs:189-192`; pending memo ≤100 `owner.rs:39` checked `:505-508`; candidate cache `candidates.rs:35-44` compile-time size assert + `try_reserve_exact` `:86-95` | `metadata_pool.rs:752-820` asserts `peak_retained <= 512 KiB` after decoding >2× the bound; `metadata_pool_index.rs:271`; `memory_bounds` retained footprint | PASS (static); NOT_RUN | — |
| 5a Stored-subtree split / concat / coalesce | handoff §3.D | split `edit/tree.rs:552-635` (interior split releases the superseded draft `:575`), concat `tree.rs:650-805` (all four level relations, `depth > MAX_LEVEL` bound `:664-666`), coalesce `tree.rs:522-534` + `edit/concat.rs:12-29`, boundary slice `edit/split.rs:12-32`, half-partition `tree.rs:808-841` | `edit_bounds.rs:183-212` (80+100 join canonical after insert and seam delete), `:131-155` (63/64/127/128/129), `:158-180` (192/193), `edit_localized.rs:360-419` (only replaced payloads dropped; ≥ leaves−3 survive by identity) | PASS (static); NOT_RUN | see F1 (exact 90+90 asserted only for the case whose base pages are 89+90) |
| 5b Reference partition incl. 80+100 → 90+90 | handoff §3.D | mechanism `tree.rs:812-820` (180 entries → half 90 + 90) | Fixture `evidence/stages-3-4-oracle-20260917T034500Z/repartition-80-100.json:12`: edited pages `90/73c20991`, `90/3a98cf7d` (second = base page, survives); test `edit_reference.rs:208-226` builds it, `:562-606` fails on any root, partition or survivor difference | PASS (static); NOT_RUN | F1 |
| 5c No-op and both cutoff transitions | handoff §3.C | empty stream `edit/apply.rs:53-60`; equal replacement `:63-76`; bounded window compare `edit/compare.rs:19,31-86`; dispatch on the result `apply.rs:77-114`; whole-file→chunked `:345-364`; chunked→whole-file `:121-168`; any→empty `:78-85` | `edit_noop.rs` 7 cases (incl. shifted coordinates, long equal prefix then mismatch); `edit_transitions.rs` 7 (incl. `exact_boundaries_at_every_accepted_cutoff`, `every_conversion_direction_reaches_the_fresh_construction_root`, and the E3 read-amplification case `:592-700`) | PASS (static); NOT_RUN | — |
| 5d Finality, child-first emission, no speculative objects | handoff §3.D | commit walk `tree.rs:394-462` (children committed and descriptor ids patched before the page is encoded/hashed/published, `:432-458`; one encode at `:449`), `finish` `:365-376`, release of superseded drafts `:242-320`, `discard` `:542-546`; charge bound `tree.rs:31-33,332-347` | `edit_bounds.rs:250-333` (`nodes_created == emitted mapping objects`, children precede parents, peaks ≤64 KiB and non-growing, vector printed `:332`), `edit_batch.rs:197` (every emitted object reachable from the root) | PASS (static); NOT_RUN | — |
| 5e Old-root immutability | handoff §2 | base reached only through the read-only provider `edit/apply.rs:41`, `edit/tree.rs:110`, trait `object/access.rs:18-35` (no mutator); stored pages returned unchanged `tree.rs:400-408` | `edit_single.rs:178-206` (base bytes unchanged after an edit that changes the root); `edit_localized.rs:385-398` | PASS (static); NOT_RUN | the named case uses a small (whole-file) base; the chunked route's immutability is structural, not asserted per-object |
| 6 Waivers not promoted | completion round, closeout §1/§2 | — | see §3 below; no campaign receipt exists in `evidence/` | PASS (unchanged) | G13/G15 remain unmeasured |

## 2. Findings, severity ordered

**F1 (LOW — coverage labelling).** The case named `repartition-80-100` does not have two
80- and 100-entry base pages: its sealed fixture records base pages of **89 and 90** entries
(`evidence/stages-3-4-oracle-20260917T034500Z/repartition-80-100.json:11`) because the fixture
is built by canonical construction of the concatenated bytes (`edit_reference.rs:216-221`).
Trigger: handoff `stages-3-4-handoff.md:252-254` asks for "the 80-entry + 100-entry root join
that repartitions to 90 + 90". Observed: the exact 90+90 partition *is* asserted, and the
candidate matches the reference root, partition and surviving leaf (`edit_reference.rs:582-599`);
the >128-entry half-partition path (`tree.rs:812-820`) is exercised by the edited result.
Expected: the fixture is a valid instance of that path, but it is not the literal 80+100 join
the handoff names; the literal join is covered candidate-side only
(`edit_bounds.rs:183-212`, canonical occupancy, no exact-partition oracle). Consequence:
coverage labelling, not behaviour — nothing in the reference claim is false, but a reader may
believe an 80-vs-100 leaf join is oracle-checked. Smallest remedy: rename the case in
`stages-3-4-report.md:328` and `edit_reference.rs:208` to the base shape it actually seals
(179 extents, 89+90), or add one oracle generation whose base is the literal join.

**F2 (LOW — schema identity vs accepted-role widening).** `sql/schema.sql:54` widened
`objects.object_role` to `BETWEEN 1 AND 13` for the Stage 5 filesystem roles while
`user_version` stayed **4** (`schema.sql:11`, `storage/policy.rs:27`), and the open-time check
compares table names, column names/order and index presence only — never the CHECK text
(`sqlite/schema.rs:149-178`). Trigger: open a Store created by a Stages 3–4 build (CHECK 1..6)
with the final code. Observed: the open succeeds (identity matches, columns match); the
mismatch surfaces only when a filesystem-role object is first written, as a SQL constraint
failure. Expected: either the widened accepted range is part of the persisted identity, or the
open refuses a DDL the build no longer agrees with. Consequence: a late definite failure, never
corruption or silent reinterpretation. Related drift: the Stage 3–4 report's profile row still
says "Object roles 1 … 6" (`stages-3-4-report.md:122`) against the final tree's 1..13.
Smallest remedy: include the `objects` role CHECK bound in `validate_table`/`validate_schema`,
or bump `SCHEMA_VERSION` and restate the report row.

**F3 (INFO — report snapshot counts vs final tree).** `stages-3-4-report.md:251-273` counts
belong to the pre-review snapshot; the final tree's `#[test]` counts for the same targets are
higher: edit_bounds 12 (was 9), edit_transitions 7 (5), delta_payload 15 (13), metadata_pool 15
(14), metadata_pool_index 8 (5), policy_capacity 9 (8), physical_formats 7 (6), edit_pipeline 6
(5), codec_frames 6 (absent from the table). Trigger: citing §4 as the final count. Observed vs
expected: the completion round records 290 tests / 41 binaries (`stages-3-4-completion-round-20260917.md:22`),
so the growth is accounted for elsewhere; only the report table is stale. Consequence: a reader
under-counts the final tree. Smallest remedy: date the §4 table as the W-snapshot count, as §3
and §9 already are.

No S1/S2 correctness defect was found in the reviewed Stage 3–4 code. The four files those
stages own that the Stage 5 range touched (`encoding/delta/select.rs`, `encoding/full.rs`,
`pack/layout.rs`, `sql/schema.sql`; `git diff 30358b565..HEAD`) only add the filesystem roles
to the "stored unframed / never a delta base" sets, preserving the Stage 3–4 semantics; every
other Stage 3–4 file is unchanged since the closeout.

## 3. Owner-WAIVED vs MEASURED (do not promote)

| Row | Recorded as | Final tree |
| --- | --- | --- |
| G13 matched reference campaign under a pre-committed addendum with aligned byte accounting | **PASS (owner waiver, 2026-09-17)**, "accepted unmeasured ... creates no evidence, it accepts its absence" — `stages-3-4-closeout-report.md:113`, `:34-55` | still unmeasured; no campaign receipt in `evidence/` (only smoke, fingerprint, timing, matched-C1); registry row `NOT_RUN` `stages-3-4-verification.md:67` |
| G15 "existing-or-better" latency/storage/memory resolved or waived in writing | **PASS (owner waiver)** — `closeout-report.md:115` | still unmeasured; completion round `stages-3-4-completion-round-20260917.md:32-38`: 0 of 3 axes measured at any n |
| F3 no matched campaign exists | **not closed by the batch**, waived rows named — `closeout-report.md:139` | unchanged |
| E1 escalation | §1 records the waiver, §6 records it as **not answered** in this round — `closeout-report.md:34`, `:631-655` | waiver text covers G13/G15 only; nothing broader |
| E2 pooled-lane / v5 deviations | **unanswered owner decision**, not a waiver — `closeout-report.md:76-93,661-670` | unchanged; pinned by `physical_formats.rs:145` |
| A2 #168 item 6 | **met for the unclipped arms only**; the `e1c-pooled-512` arm stays clipped and was never re-collected — `closeout-report.md:672-690` | receipt untouched; under D1 the same arm now fails |
| W10 unrun items (8 MiB − 1 deferred refusal; directory/workspace dimensions) | UNRUN with source proof / out of scope — `closeout-report.md:540-543` | unchanged |
| E3 #169 item 2 read amplification | **measured, RUN** — `closeout-report.md:692-704`; case `edit_transitions.rs:592-700`; receipt `evidence/stages-3-4-read-amplification-20260917T034743Z/` | present in the final tree |
| G4 frontier peak vector | was README-only (F-19(b)); the final tree prints it — `edit_bounds.rs:329-332` | receipt gap closed, not a waiver |
| G12 pooled memory figure | measured (3 450 007 B) with the C4/C5 corrections — `closeout-report.md:112` | unchanged |

No waived row was upgraded by this review. Every row above is either a measured artifact, a
waiver that says it creates no evidence, or an open owner decision.

## 4. Reviewer limitations

* No build or test execution (rule: lead reviewer runs builds/tests); all statuses are static.
* Stage 5 filesystem code is outside this area and was inspected only where it touches the
  Stage 3–4 files named above.
