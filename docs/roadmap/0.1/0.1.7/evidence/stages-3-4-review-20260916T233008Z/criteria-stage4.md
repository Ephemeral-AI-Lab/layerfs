# Issue #169 — Stage 4 acceptance-criteria audit (localized edits, frontier, transitions, finality)

**Reviewer:** independent pass, criteria-stage4. **Date of review:** 2026-09-17.
**Reviewed snapshot:** `git HEAD 91c3a0741fff64e8161d5c1b6e759f347ffbf757` (branch `main`, ahead 16,
working tree clean except this evidence directory — verified with `git status --porcelain=v1 -b`).
**Reference revision:** `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` (v0.1.6, exists in this repo).
**Method:** static reading of product source and external test bodies, `git` read-only commands,
`grep`/`sed`, and inspection of the retained receipts. **No `cargo`, no build, no test execution,
no clippy, no preflight** (per assignment and `tools/preflight.sh` retirement). Every status below is
therefore "code + retained-evidence based"; no status rests on a test run performed by this reviewer.
**Not fabricated:** every unavailable metric is written `null` with its reason.

## 0. Verdict

| Stage | Verdict | One-line reason |
| --- | --- | --- |
| **Stage 4 / #169** | **INCOMPLETE** | The large→large stored-tree edit path is real, path-local and reference-equivalent on nine sealed fixtures, and the mapping-rebuild route is gone. But D2's *owned decoded* unfinished frontier is not implemented (drafts stay **encoded**, superseded drafts are never dropped, and every draft is encoded+hashed, not only final nodes), D3's frontier bound has no valid test, child-first emission is unasserted for the edit path, several required handoff cases (slow bounded sink; store-backed node-work check; store-backed route-equality test) are absent, and the qualification gate is explicitly unqualified. |

No original acceptance bullet can be marked a clean PASS on evidence: two are PASS-with-gap, three are
INCOMPLETE, one (qualified latency/storage/memory) is FAIL as evidence.

## 1. What is genuinely demonstrated

1. **The whole-mapping rebuild is gone from the large→large route.** `core/crates/layerfs-content/src/file/edit/frontier.rs`
   does not exist at HEAD; `core/crates/layerfs-content/src/file/edit/mod.rs:5-11` declares only
   `apply, compare, concat, finish, input, split, tree`. `apply.rs:206-304` is the routed implementation.
2. **Path-local split/concat/coalesce, read from the code.** `tree.rs:290-369` splits a stored subtree
   along one path (`tree.rs:350-367` descends only `children[index]`, reusing `summaries[..index]` and
   `summaries[index+1..]` by identity); `tree.rs:384-518` concatenates by descending the right/left
   spine only; `tree.rs:259-284` coalesces adjacent slices of one payload; `tree.rs:521-554` applies the
   half partition. An untouched subtree is carried as `NodeSummary` (id+bytes+extents+level) and is
   never read again unless a split/join needs its boundary.
3. **Reference equivalence exists and is a separate execution.** `crates/layerfs-content/examples/rope_edit_oracle.rs`
   lives in the **reference** tree and calls the reference APIs directly
   (`layerfs_content::file::rope::{self, FileMutationBatch}`, build at L207/L274-280); the candidate test
   `core/crates/layerfs-content/tests/edit_reference.rs` only reads the JSON fixtures
   (`edit_reference.rs:374-443`). `git diff --stat 44cf748 -- crates/layerfs-content/src` is **empty**, so the
   reference library executed is byte-identical to the seal; the only delta in the whole root tree is the
   two added `examples/` files (411 insertions).
4. **The candidate reproduces the reference root, page partition and surviving leaf identities** for nine
   cases, including `repartition-80-100`, `untouched-sibling`, `unequal-height-join`, `height-growth`,
   `root-collapse`, `batch-normalized` (`edit_reference.rs:559-603`; comparisons at L472-475, L498-502,
   L579-596). The module states the right rule at `edit_reference.rs:16`: "Equal final bytes or a valid
   canonical tree are explicitly not the oracle."
5. **Localized I/O is externally counted, not asserted from inside the product**
   (`edit_localized.rs:33-47` recording provider; L275-419): at 24 MB one three-extent overwrite demands
   `pages_read.len() <= 6` of a 12-page mapping (L302-313), reads **exactly** the three replaced payloads
   (L316-330), emits ≤6 new pages and ≤4 chunks (L361-369), drops exactly the replaced payloads (L380-388)
   and keeps every other base payload referenced by identity (L389-411). At 8 MB vs 24 MB the mapping grows
   5→12 pages while payload reads stay equal and page reads grow by ≤2 (L470-526).
6. **Transitions are bounded and enumerated; nothing walks the whole retained extent sequence.**
   - small→large: `apply.rs:106-109` routes a *whole-file* base to `stream_combined` (`apply.rs:339-357`),
     which reads `view.whole_file_bytes()` (`apply.rs:379-381`, `view.rs:70-78`). A whole-file base has
     **no mapping tree at all**, and its length is below the cutoff (≤1 MiB), so there is no retained
     extent sequence to enumerate.
   - large→small: `apply.rs:83-105` assembles retained ranges through `view.read_range`, which descends
     only into children overlapping the range (`file/mapping/read.rs:189-212`, skip at L193-199 and
     L225-227). No full walk.
   So the implementer's admission at `stages-3-4-report.md:278-281` ("Small → large and large → small still
   rebuild the mapping") is accurate but **does not violate D2's no-enumeration clause**: `3.C` of the
   handoff mandates exactly these two routes, and both are bounded by the cutoff/final size.
7. **No new runtime mode and no error-driven fallback**: `apply.rs:106-109` dispatches once on the base's
   recorded representation; there is no `Result`-driven retry of another route anywhere in `file/edit/`.

## 2. Criterion table — original issue #169 acceptance bullets

| # | Criterion (issue-169 "Acceptance") | Source (read) | Test target + name | What the oracle actually asserts | Retained raw evidence | Status | Precise gap |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Overwrite/insert/delete/append at boundaries, head/middle/tail; repeated edits; no-op results **match the reference profile** | `edit/tree.rs:290-518`, `apply.rs:206-304` | `edit_single.rs::{overwrite_in_the_middle(92), overwrite_at_the_head_and_the_tail(105), insert_and_delete(123), append_at_end_of_file(144), complete_deletion(158), a_chunked_base_reads_and_edits_without_a_full_pass(262)}`; `edit_batch.rs` (7); `edit_reference.rs:559` | `edit_single`/`edit_batch` assert final bytes and equality with a **fresh candidate construction** (`edit_batch.rs:100`), i.e. candidate self-consistency; **only `edit_reference` compares against the reference** (root L579-584, partition L585-590, survivors L591-596) | `evidence/stages-3-4-oracle-20260916T222738Z-corrected/` (8 fixtures), `…-20260917T034500Z/repartition-80-100.json` | **PASS (9 reference cases)** | Reference equivalence is 9 frozen shapes, not per-operation boundary cases; head/tail/append have no reference root. |
| A2 | Small→large, large→small, empty transitions at **every accepted cutoff** with **sound physical read amplification accounting** | `apply.rs:75-110`; `policy.rs:14-18,86-93` | `edit_transitions.rs::every_conversion_direction_reaches_the_fresh_construction_root (102)`, `exact_boundaries_at_every_accepted_cutoff (76)`, `compressible_and_incompressible (205)`, `known_and_streamed (229)`, `unsupported_cutoffs (255)`; `edit_pipeline.rs:150` | Cutoffs 128 KiB/256 KiB/1 MiB (`edit_transitions.rs:18`); lengths 0/1/T-1/T/T+1; every direction equals a fresh construction of the expected bytes (L128, L155, L181, L199) and reads back equal. **No read/byte accounting is asserted anywhere in this target.** | `evidence/stages-3-4-timing-20260917T031000Z/e2-{c1,c2,pipeline}-{small-to-large,large-to-small}/` | **INCOMPLETE** | The second half of the bullet — *sound physical read amplification accounting* for a conversion — has **no measurement and no assertion**: `edit_transitions.rs` uses only `MemoryStore` roots, and the receipts print emitted objects/bytes, not the reads a transition performs (the transition route's reads are `null`). |
| A3 | Exact root/partition equivalence **and frontier finality proven** before deleting old structure | `tree.rs:105-202` | `edit_reference.rs:559`; `edit_batch.rs:196` | Roots/partitions: PASS (above). Finality: the *only* evidence is code structure — `hold_node` never emits (`tree.rs:105-139`), `commit` walks the final root children-first at the end (`tree.rs:167-202`), and `every_object_the_edit_emits_is_reachable_from_its_root` (`edit_batch.rs:196-217`) checks reachability of every new object | — | **INCOMPLETE** | No artifact (test or written proof) states *why no later admitted edit or join can change an emitted node*; the property holds only because emission is fully deferred, which is also the reason for the D2/D3 gaps below. |
| A4 | Bounded input/base/output/frontier coexistence; **slow consumer**; late failures | `tree.rs:31,105-139`; `apply.rs:217` | `edit_bounds.rs:275-317` (short source), `319-361` (rejecting consumer), `241-273` ("retained frontier"), `214-238` (many edits) | Late source failure → `ContentError::Io` once (L313-316); rejecting consumer → `OutputRejected`, "the rejection is returned once" (L359-360). **"Slow consumer" has no test at all** — the only consumers in the edit targets are `MemoryStore` (`support/mod.rs:83-90`), `Ledger` which stores everything (`edit_localized.rs:50-61`) and the rejecting one | — | **INCOMPLETE** | (i) slow bounded consumer: **NOT_RUN**; (ii) the frontier-bound test is vacuous — see §4.1. |
| A5 | C1-only edit and C2-only save timings + integrated traces **use the same real algorithms** | `apply.rs:38-112`; `examples/measure_edits.rs` | `edit_timing.rs` (4); receipts `e2-c1-*`, `e2-c2-*`, `e2-pipeline-*` | `edit_timing.rs:134-185`: the same edit recorded vs disabled returns the same root **and the same provider read count** (`counts.objects() == disabled_counts.objects()`, L180-184); scope tree contains real `edit.base`/`edit.split`/`edit.finish` (L87-101). Receipts: `e2-c1-small/stdout.log:8` result root `8ddfe36c…` **equals** `e2-pipeline-small/stdout.log:8` edited root; `e2-c1-chunked` `ca6c30a4…` equals `e2-pipeline-chunked` | `evidence/stages-3-4-timing-20260917T031000Z/` (21 arms, per-arm `command.txt`, commit `dfd54fd8e…`) | **PASS** | The store-backed equality lives only in example receipts; the test named for it (`edit_pipeline.rs:193`) does **not** exercise the store-backed route (§4.4). |
| A6 | Existing-or-better qualified latency/storage/memory; no full-store scan, hidden spool, retry or full-rebuild fallback | `apply.rs:106-109`; `view.rs:81-111` | `edit_localized.rs` (5) | No full-store scan: PASS for large→large (≤6 of 12 pages at 24 MB, L302-313). No retry/fallback: PASS by code. **Latency/storage/memory: unqualified** | `evidence/stages-3-4-matched-c1-20260917T050000Z/` | **FAIL (as evidence)** | n=1 per arm; ledger's own table records 3 candidate and ≥4 reference executions but the same paragraph says "executed four times in total" (`ledger.md:52-58`) → internally inconsistent; byte boundary not like-for-like (reference 13 826 B vs candidate 47 357 B, `ledger.md:41-43`); memory `null` — "Memory: not measured" (`ledger.md:83-86`); the candidate binary's source was reformatted after the run (`65f2a402d`), so HEAD source ≠ the measured binary. |

## 3. Criterion table — the added completion gates D1–D4 and qualification

| Gate | Exact source | Test target + name | What the oracle actually asserts | Retained raw evidence | Status | Precise gap |
| --- | --- | --- | --- | --- | --- | --- |
| **D1** sealed oracle + tests separating validity from exact roots/partitions; 80+100→90+90; untouched siblings; unequal heights; root growth/collapse | `tests/edit_reference.rs:30-54, 446-615` | `the_candidate_reproduces_the_reference_root_and_partition` (9 cases), `the_oracle_fixtures_are_the_sealed_reference_revision` | Oracle identity of the operation is asserted **before** any tree: candidate tuples == fixture tuples (`L472-475`); base root == reference base root (`L498-502`); then edited root, page partition (`entries/id8`) and surviving non-root leaf ids must all match (`L579-596`) | `…oracle-20260916T222738Z-corrected/` (8 JSON), `…oracle-20260917T034500Z/repartition-80-100.json`; superseded `…oracle-20260916T214846Z/` retained | **PASS, with provenance caveats** | (a) The "sealed revision" assertion is a **string grep** (`edit_reference.rs:606-614`) against a literal the printer hard-codes (`rope_edit_oracle.rs:283`) — it proves the fixture *says* 44cf748, not that 44cf748 produced it. (b) The corrected and repartition directories contain **only JSON fixtures**: no README, no command, no binary hash, no transcript (verified by `ls`). (c) `…oracle-20260916T214846Z/README.md:5` claims "`git diff 44cf748 -- crates/layerfs-content` is empty"; at HEAD that diff is **not** empty (411 insertions in the two added `examples/` files). Only `-- crates/layerfs-content/src` is empty. (d) The literal "80-entry + 100-entry **root join**" is never constructed: both sides fresh-construct the concatenated bytes (`rope_edit_oracle.rs:201-210`, `edit_reference.rs:213-223`), so the recorded base is a 2-child root of **89/90** entries, not 80/100 (`repartition-80-100.json:base_pages`); the repartition property itself (89+90 → 90+90, far leaf `3a98cf7d…` surviving) is genuinely exercised. |
| **D2** stored ObjectId+summary **versus owned decoded unfinished nodes**; real path-local split/concat/coalesce; **no full retained-extent enumeration for eligible localized edits** | `tree.rs:50-58, 82-139, 290-554`; `apply.rs:206-304` | `edit_localized.rs` (5); `edit_reference.rs:559` | Localized enumeration: PASS (§1.5). Representation: the unfinished state is `deferred: BTreeMap<ObjectId, Vec<u8>>` — **encoded bytes keyed by id** (`tree.rs:55`), read back by cloning and **re-decoding on every access** (`tree.rs:82-101`), created by **encoding + hashing immediately** (`tree.rs:106-107`) | — | **FAIL (partial)** | The "owned **decoded** unfinished nodes" state does not exist. Consequences, all verifiable in code: (i) every draft is encoded and hashed even when later superseded (`tree.rs:105-139`) — the handoff's "encode/hash only final nodes" is not met; (ii) an overwritten draft is **never dropped**: the map only ever grows, and the charge is monotonic (`tree.rs:112-132`) — "drop overwritten drafts" is not met; (iii) each boundary operation re-decodes a node it just encoded (`tree.rs:90-101` called from `split`/`concat`), i.e. the reference's encode→decode cost pattern is reduced to one map but not removed. The *enumeration* clause **is** satisfied. |
| **D3** emission rules prove no later admitted edit/join changes an emitted node; bounded simultaneous frontier/read/output ownership; no speculative unreachable output; no per-edit tree accumulation | `tree.rs:105-202`; `apply.rs:217, 299` | `edit_batch.rs:196`; `edit_bounds.rs:241` | Reachable-only emission: PASS (every new object reachable from the returned root, `edit_batch.rs:208-216`). Structural finality: PASS by construction (nothing is emitted before the stream ends). Bounded frontier: **no valid evidence** — see §4.1 | — | **INCOMPLETE** | (a) The named frontier test measures a no-op (see §4.1). (b) The deferred ownership is charged **monotonically across the whole operation and never released** (`tree.rs:112-132`): it is not "independent of total edit count" as §3.D requires; a long-enough stream (API accepts up to `MAXIMUM_EDITS_PER_OPERATION = 4 096`, `input.rs:22`) fails the operation at `EDIT_DEFERRED_LIMIT = 8 MiB − 1` (`tree.rs:31,116-122`). No test induces that failure (`grep BoundedCapacityExceeded` in `layerfs-content/tests`: none). (c) **Child-first emission for edits is not asserted anywhere**: `support::TrackingConsumer::assert_children_precede_parents` (`support/mod.rs:175-195`) is used only by `streaming.rs:50-55` and `91-98` (complete construction), never by an `edit_*` target, although the handoff's `edit_batch + edit_model` row requires "child-first output". (d) Producer-reported `EditCounters`/`peak_deferred_bytes` (`tree.rs:36-48,76`) are **unused by any external test**. |
| **D4** exact reference root/partition; old-root preservation; expected subtree IDs; no-op/transition/multi-edit/failure cases; **actual node-work checks through the real C1 and integrated paths** | `tree.rs`, `apply.rs`; C2 `src/cas/read.rs:91-92` | `edit_reference.rs:559`; `edit_single.rs:178`; `edit_noop.rs` (7); `edit_transitions.rs` (5); `edit_bounds.rs:275,319,363`; `edit_localized.rs` (5); `edit_pipeline.rs` (4) | Reference root/partition/subtree ids: PASS. Old root: C1 `the_base_objects_are_never_rewritten` (L198-205) + base readback (`edit_localized.rs:413-418`). No-op/transition/multi-edit/failure: all have tests. Node-work: **C1 yes** (`edit_localized` counting provider), **integrated no** | `evidence/stages-3-4-timing-20260917T031000Z/e2-pipeline-*` | **INCOMPLETE** | (a) No C2/integrated **node-work check**: `edit_pipeline.rs` discards `StoreReadCounters` in all four tests (`L81,132,181,226`), and no C2 test counts pages/nodes for the edit path. (b) **No store-backed old-root readback**: nothing re-reads the *base* root through the reopened store after an edit save (`grep base_root|old root` in `edit_pipeline.rs`: none). (c) The route-equality test does not test the route it is named for (§4.4). |
| **Qualification** equivalent successful localized edits with actual work, time, storage and memory; "a bounded-memory mapping rebuild does not satisfy this issue" | `stages-3-4-verification.md`, addendum | `matched-c1` receipts; `e2-*` receipts | Same content-addressed root in both arms (`b6dca354…c65207b`, `ledger.md:41-43`; both `stdout.log` files) — a correctness match, not a qualification | `…matched-c1-20260917T050000Z/` (8 files), `…timing-20260917T031000Z/` (110 files) | **INCOMPLETE (self-declared and confirmed)** | n=1 declared sample per arm and the ledger's sample count is self-contradictory (`ledger.md:52-58`); memory `null` (never instrumented in either round — no RSS/allocation field exists in any receipt); storage boundaries not like-for-like (7 objects/47 357 B vs 5/13 826 B, `ledger.md:41-43`); the round-1 receipts are candidate-only in the **debug** profile (`README.txt:4`); no cache-state field in any raw artifact (prose only). |

## 4. Findings that change the verdict (most important first)

### 4.1 DEMONSTRATED MISSING EVIDENCE — the frontier-bound test measures a no-op
`core/crates/layerfs-content/tests/edit_bounds.rs:241-273` (`the_retained_frontier_does_not_grow_with_the_file`)
applies `Edit::delete(0, 0)` (L249) — a zero-length deletion with no replacement. In `apply_edits` that
stream is not empty (`apply.rs:53`), and `compare_replacements` classifies the zero-length replace segment
as equal (`compare.rs:46-51`), so the operation returns the **base root** without touching the frontier
(`apply.rs:62-74`). The recorded "peaks" are then `support::extent_count(&result, root)` of the *base*
(L266), and the assertions `peaks[0] <= 192 && peaks[1] <= 192*32` (L272) are satisfied by the base's own
40/400 extents. The test asserts nothing about the edit frontier. This is the only test named for the D3
frontier bound, so **D3's "bounded simultaneous frontier" has no external evidence at all**.

### 4.2 FAIL (partial) — D2's decoded frontier is not implemented
`tree.rs:50-58` shows the operation's private state: a reader, a consumer, a `BTreeMap<ObjectId, Vec<u8>>`
of **encoded** drafts and a monotone charge. `hold_node` encodes and hashes every draft on creation
(`tree.rs:106-107`), never removes a superseded draft (`tree.rs:123-130`), and `load_node` re-decodes a draft
on each visit (`tree.rs:90-101`). The completion handoff requires "Unfinished subtree: **owned decoded
entries** + checked summary" and "prove node final -> seal children -> encode/hash -> emit"; the design
table in `stages-3-4-handoff.md:417` requires "Owned decoded frontier; drop overwritten drafts and
encode/hash only final nodes". Three of those four clauses are unmet. The route is nonetheless genuinely
path-local (§1.2/§1.5), so D2's *no-enumeration* clause is met and the defect is in representation,
draft lifetime and wasted encode/hash work — not in the asymptotic mapping walk.

### 4.3 INCOMPLETE — deferred ownership grows with the edit stream, not with height/fanout
`EDIT_DEFERRED_LIMIT` (`tree.rs:31`) makes the failure clean, but `charged` only ever increases
(`tree.rs:112-132`), so a 4 096-edit stream (the accepted maximum, `input.rs:22`) can exhaust 8 MiB of
drafts that are mostly unreachable and therefore never emitted. §3.D requires the bound to be "a function
of supported height/fanout, independent of total edit count/file bytes". No test exercises the ceiling.

### 4.4 INCOMPLETE — the "integrated route agrees" test does not use the integrated route
`core/crates/layerfs-storage/tests/edit_pipeline.rs:192-227` asserts `root == constructed.root` (L223), but
both are in-process C1 values: `root` comes from the in-memory `edit()` helper (L21-56) and `constructed`
is a fresh `construct_bytes` (L213-222); the edit's base is never read through the C2 Store. The genuine
store-backed equality exists only in the example receipts (`e2-c1-small` vs `e2-pipeline-small`,
`8ddfe36c…`; `e2-c1-chunked` vs `e2-pipeline-chunked`, `ca6c30a4…`). The other three pipeline tests do
reopen the store and compare the root's canonical bytes against identity-checked reads
(`src/cas/read.rs:91-92`), but they only read the root object — they do not reassemble the file and do not
compare to a standalone root.

### 4.5 INCOMPLETE — weak oracle behind the "bounded comparison" claim
`edit_noop.rs:151-181` ("a_long_equal_prefix_then_a_mismatch_stops_the_comparison_early") asserts only
`counts.objects() > 1` (L177-180), which is also true if the comparison had read the entire file. The
bounded early return is real in code (`compare.rs:78-80` returns at the first differing 64 KiB window,
`compare.rs:19,53-55`), but the *test* does not demonstrate it, and no test counts comparison reads. The
"both passes charged" claim is likewise unasserted beyond `objects() > 1`.

### 4.6 Product-source hygiene (minor, but real)
- `file/edit/split.rs:12` `slice_of` is re-exported (`edit/mod.rs:19`) and **never called**; `tree.rs:315-341`
  splits extents inline instead, without `slice_of`'s coverage check.
- `file/view.rs:114-142` `walk_extents` has **no caller** in `core/`.
- `EditCounters`/`EditObjects::counters()` (`tree.rs:36-48,76`) are unused outside `src/`.

### 4.7 Receipt hygiene
`…oracle-20260916T214846Z/README.md:5` overstates the seal (see D1 caveat c). The matched-pair ledger
contradicts itself on sample count (`ledger.md:52-58`: "executed four times in total" while its table lists
three reference executions including a "tool re-run"). The round-1 directory label
(`…20260917T031000Z`) disagrees with its own recorded collection time; the ledger explains it as a host
timezone offset.

## 5. Explicit answers to the two questions in the assignment

1. **Do the small→large and large→small transitions still rebuild the whole mapping, and does that violate
   D2/#169?** They rebuild (small→large through the complete builder, `apply.rs:106-109,339-357`;
   large→small by whole-object assembly, `apply.rs:83-105`), and the implementer discloses this
   (`stages-3-4-report.md:278-281`). It **does not violate D2's "no full retained-extent enumeration"**
   clause: a small base has no mapping tree to enumerate, and large→small reads only ranges of the final
   (sub-cutoff) content through a range-limited descent (`mapping/read.rs:189-212`). It also does not
   violate the issue's own adjustment, which scopes D to the "large-to-large retained-extent walk/mapping
   rebuild". What it *does* mean is that "localized" is a property of large→large only; the transition
   bullets still lack the read-amplification accounting the acceptance bullet demands.
2. **Is the oracle a separately sealed reference execution, or a candidate reimplementation?** It is a
   separate execution of the reference: an example binary inside the reference tree calling the reference's
   own `FileMutationBatch`, reading nothing from the candidate, with `crates/layerfs-content/src` identical
   to 44cf748. The *provenance paperwork* is weak: the fixtures' revision field is a hard-coded literal, the
   test's seal assertion is a substring check, and the two directories that carry the D1-relevant fixtures
   have no README, command, hash or transcript.

## 6. What I could NOT verify (explicit)

- **Any executed result.** No test, example, build or clippy was run (`cargo` forbidden by the assignment).
  Every PASS above is a code+fixture reading; I cannot confirm that `edit_reference`,
  `edit_localized`, `edit_bounds`, `edit_transitions`, `edit_pipeline` currently pass.
- **Actual physical read amplification for transitions**: `null` — no test or receipt reports reads for the
  small→large/large→small routes; `edit_transitions.rs` uses an in-memory store and asserts roots only.
- **Memory**: `null` — nowhere instrumented. Round 1 records only declared retained capacities
  (`ledger.md:83-84`), round 2 states "Memory: not measured" (`ledger.md:83-86`). No RSS/allocation/cgroup
  figure exists in any receipt, and I could not measure one.
- **Reference-produced timing beyond the single matched pair**: `null` — the reference appears in only two
  places on disk (the oracle JSON fixtures and `matched-c1/reference-arm/`), one execution each.
- **Whether the fixture JSONs were produced by the command claimed**: no transcript, no binary hash, no
  working-tree record exists for the corrected or repartition generations, so the seal rests on the
  unchanged `src` and on the printer's source ($git diff$ evidence I re-ran), not on a run record.
- **Static-LOC/file-plan conformance** (`stages-3-4-file-plan.md` per-file/per-directory estimates) and the
  999/200 physical-line caps were **not** audited here; this report covers #169 acceptance criteria only.
- The **`edit_pipeline`, `measure_edits`** C2 bodies were inspected, but I did not audit #168 pooling,
  the fingerprint-collision fixture's authenticity, or C2 failure/visibility suites.
