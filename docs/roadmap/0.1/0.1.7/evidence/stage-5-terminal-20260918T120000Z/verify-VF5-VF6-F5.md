# Verification: R2-F5 / VF-5 / VF-6 — comparison governance, eligible receipt, VF-6 deferral

Verifier: independent read-only verification agent, 2026-09-18.
Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` at HEAD
`99743b2cff2470e6634874d7ee14b9d37d0ba16e` (confirmed via `git rev-parse HEAD`,
exit 0) — matches the frozen commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40`.
No tracked file was modified, staged, committed or reset by this verification;
the only file written is this one.

## Verdicts

| Row | Verdict | Basis |
| --- | --- | --- |
| R2-F5 (owner decision: eligible collection governs, older marked superseded, sweep clean) | **PASS** (with two residual stale prose echoes documented in §D below — neither quotes the superseded rows as the qualification comparison) | Addendum §5 owner block, §5.1/§5.2 labels, hash + ancestry + ratio reproduction, full stamp sweep, index sweep |
| VF-5 (required comparison against the pinned reference, eligible receipt governs) | **PASS** | Receipt sha256 reproduced, source commit is an ancestor of HEAD, ratios recomputed from the receipt's own nanoseconds, identity MATCH reproduced field-by-field, one sample per case per arm |
| VF-6 (complete-operation comparison deferred to Stage 6 / #171; not waived, not promoted) | **PASS** | Addendum §6 owner disposition names Stage 6/#171; no Stage 5 document claims complete-operation performance |

## A. Commands run (all from the repository root, all read-only)

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (= frozen commit) |
| 2 | `shasum -a 256 docs/roadmap/0.1/0.1.7/evidence/stage-5-component-comparison-20260917T143008Z/run-1/receipt.json` | 0 | `43dbd9f440ea9b417278bd78498c33f5ad027901365f4940d00137fe2f014b24` — **matches** addendum §5.1 citation |
| 3 | `git merge-base --is-ancestor eb42c13477f85612fc864474af4489c2548d688d HEAD; echo $?` | 0 | **ancestor confirmed**; `git merge-base` returns `eb42c134…` itself; `git rev-list --count eb42c134..HEAD` = 18 |
| 4 | `python3` recompute of `candidate_ns / reference_ns` from the receipt | 0 | small 153542/235208 = **0.652792** → 0.653; wide 810792/2977625 = **0.272295** → 0.272; large-few-changes 1838958/3829000 = **0.480271** → 0.480 |
| 5 | `python3` field-by-field identity check of the receipt (six fields per case) | 0 | all six identity fields (`base_directory`, `base_table`, `base_root`, `updated_directory`, `table`, `root`) equal between arms in all three cases; declared `identity: MATCH` ×3 |
| 6 | `shasum -a 256 …/stage-5-component-comparison-20260917T073017Z/run-1/receipt.json` | 0 | `23c923acf114e943223eaea31864f8dd9620a13c8880baa3f350e9ffa7732c45` — matches §5.2 citation; its `source_commit` is `3b4941f1…` and its candidate command runs `--example filesystem_primitives_candidate` |
| 7 | `grep -rn "073017\|T073017Z" docs/roadmap/0.1/0.1.7/component-decoupling/*.md` | 0 | 18 hits, every one classified in §C below |
| 8 | `grep -rn "073017\|T073017Z" docs/roadmap/0.1/0.1.7/*.md` (and README/architecture-overview for stamps, ratios, "component comparison") | 1 | **no hits** — roadmap indexes are clean; no `index*.md` exists in `0.1.7/` |
| 9 | `grep -rn "436,000\|141,375\|2,800,292\|972,125\|3,895,708\|4,286,541\|0\.324\|0\.347" docs/ --include="*.md"` (excl. `evidence/`) | 0 | superseded row **values** appear only in: addendum §5.2 (labelled superseded), `stages-1-5-review-20260917T160000Z.md` §6.3 (labelled "diagnostic, not qualification"), `stages-1-5-review-20260917T230700Z.md` (pre-correction F5 finding). Other hits (0.1.3/0.1.5) are unrelated numbers. |
| 10 | `grep -rn "complete-operation\|complete operation" …` (stage-5 docs + indexes, excluding disclaimer lines) | 0 | no overreach found — see §E |
| 11 | `git log --follow` on the addendum; `git log` on the round-2 review; `git merge-base --is-ancestor 6b7170e05 b069cb33a` | 0 | review landed `6b7170e05` 2026-09-17 23:47:44 +0800; owner dispositions `b069cb33a` 2026-09-18 01:33:16 +0800; review **is an ancestor of** the dispositions commit → the review's F5 finding describes the pre-correction state |
| 12 | `git show b22712844 --stat`; `git diff --name-only 3b4941f1 b22712844 -- core/` | 0 | `b22712844` touched `core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs` (14 lines) after the old collection — the AGENTS.md §3.3 invalidation rationale for the supersession is real |
| 13 | `cat …/T143008Z/driver.stdout` | 0 | transcript ends `EXIT=0`; every per-case value identical to `receipt.json` |

## B. The governing receipt (VF-5 core evidence)

`docs/roadmap/0.1/0.1.7/evidence/stage-5-component-comparison-20260917T143008Z/run-1/receipt.json`:

- `source_commit`: `eb42c13477f85612fc864474af4489c2548d688d` (receipt line 2) — ancestor of HEAD (command 3).
- `working_tree_dirty`: `false`; `profile`: `release`; `toolchain`: `+1.85.1`.
- `samples`: `"one per case per arm"`; both arms' commands carry `--samples 1`.
- Per case, `identity: "MATCH"` and the six identity fields are byte-equal between the reference and candidate arms (command 5).
- Ratios recomputed by this verifier from the receipt's own `elapsed_ns` (command 4): **0.653 / 0.272 / 0.480** — matching `candidate_over_reference` `0.652792 / 0.272295 / 0.480271` recorded in the receipt and the §5.1 table.
- `driver.stdout` (same evidence dir) ends `EXIT=0` and its numbers are identical to the receipt — consistent with §5.1's "driver transcript exit 0".
- The evidence dir's own `README.md:3-11` states the earlier `073017Z` receipt "is not identity-matched and its rows are **diagnostic only**" — exactly what §5.1 quotes at addendum lines 81-82.

## C. Addendum §5 / §6 and the sweep (R2-F5 / VF-5 / VF-6)

`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-verification-addendum-20260917.md`:

- **:64-66** owner decision, quoted verbatim: *"The eligible collection at `eb42c1347` governs (0.653/0.272/0.480, identity MATCH, an ancestor of this tree) and the addendum §5 is corrected to cite it; the older collection is marked superseded."* — matches the owner's 2026-09-17 decision as given.
- **:68-72** "The eligible collection governs… The collection this section used to publish is retained, unedited, as §5.2 and is **superseded**: its candidate arm ran an example name that changed in `b22712844`… the pair is not identity-matched." (rationale verified by command 12).
- **§5.1 (:74-99)** cites the receipt path and sha256 `43dbd9f4…` (:78-80, reproduced by command 2), the ancestor statement (:76-77, confirmed by command 3), one sample per case per arm (:77-78), and the governing rows (:84-88): `235,208/153,542 → 0.653`, `2,977,625/810,792 → 0.272`, `3,829,000/1,838,958 → 0.480`, all `MATCH` — all reproduced from the receipt itself.
- **§5.2 (:101-123)** retains the older `073017Z` collection, labelled "**Superseded** collection (retained as the record, 2026-09-17)" (:101) and "**They are diagnostic only** and must not be quoted as the qualification comparison" (:104-105), citing its receipt and sha256 `23c923ac…` (:109-111, hash reproduced by command 6) and keeping the old rows unedited (:113-117) with the old reading quoted and marked superseded (:119-123).
- **§6 (:127-143)**: the complete-operation row "stays **NOT_RUN** with that reason… no complete-operation speed claim is made" (:133-135); the owner disposition is quoted verbatim — *"VF-6: i think we can defer it to stage 6."* (:137) — and the deferral is to **Stage 6 ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171))** (:138-139), stated as "a deferral with a named owner, not a waiver and not a PASS: Stage 5 makes **no** complete-operation performance claim, and Stage 6 inherits the row" (:141-143). The row is not promoted anywhere in the addendum.

### Stamp-sweep classification (command 7 — every hit)

| Hit | Class | Why |
| --- | --- | --- |
| `stage-5-verification-addendum-20260917.md:110` | intentional retention | §5.2 itself, labelled superseded/diagnostic (required by the owner decision) |
| `stage-5-completion-report-20260917.md:92` | dated correction (history) | owner-decision correction block declaring the old collection "diagnostic only" and the eligible one governing |
| `stage-5-remediation-handoff-20260917.md:286` | dated record (**allowed**) | R17 remediation instruction to re-collect; mentions the stamp as the invalidated receipt |
| `stage-5-remediation-wp4-wp7-handoff-20260917.md:139` | dated record (**allowed**) | same R17 text |
| `stage-5-report.md:539` | dated correction (history) | §13.4 correction: the old collection is "not identity-matched and diagnostic only"; eligible governs |
| `stage-5-terminal-handoff-20260917.md:223,230` | governance description | WP-C records the correction and the sweep; does not quote the old rows as qualification |
| `stages-1-5-review-20260917T160000Z.md:36,333` | dated record (**allowed**) | round-1 review: R17 finding; "ineligible as qualification" |
| `stages-1-5-review-20260917T230700Z.md:199,204,209-211,225,577,931` | dated record | round-2 review, landed 23:47 **before** the correction commit 01:33 (command 11); its F5 finding describes the pre-correction addendum |

**No hit is current guidance quoting the superseded `073017Z` rows as the qualification comparison.**
The roadmap indexes (`docs/roadmap/0.1/0.1.7/*.md` incl. `README.md`, `architecture-overview.md`, plus `docs/roadmap/README.md`, `docs/roadmap/0.1/README.md`) contain no stamp, no ratio and no "component comparison" citation at all (command 8, exit 1).
Note: the claim's parenthetical names three dated records; the round-2 review and the two correction blocks also mention the stamp, but only as history/correction text — an undercount of mentions, not a violation.

## D. Residual findings (documented, not verdict-changing)

1. **`stage-5-completion-report-20260917.md:140`** (§6 "What is not claimed", last bullet): "Two component cases are faster and one is 10% slower; nothing here is averaged across cases." — This is the **superseded** collection's reading. Under the governing collection all three cases are faster (§5 table at :104-112, repointed by the correction block at :90-102). The correction block explicitly says "Nothing else in this report changes" (:101-102), so this stale paraphrase was knowingly retained. It quotes no rows, no stamp and no receipt, so it is not a quotation of the superseded rows as the qualification comparison — but it contradicts the same document's governing §5.
2. **`stage-5-completion-report-20260917.md:287`** (§10.4, "Remaining concerns and boundaries"): "**The large-few-changes component case is 10% slower than the reference.** The cause was not profiled; the row is reported rather than explained away…" — Again the superseded reading presented as a stage-level remaining concern; under the governing collection that case is 0.480 (faster). Strongest counter-evidence found: a reader of §10 alone would take away a fact that is false under the governing collection.
3. **`stage-5-report.md:472`** (§11.3): "two cases faster, one 10% slower, all identities matching" — same stale paraphrase, but this one is **explicitly resolved** by the §13.4 correction at :537-546 ("§11.3 overstates the comparison — resolved by owner decision, 2026-09-17"), per that document's stated policy "This section corrects claims in the sections above; it does not rewrite them" (:456).
4. `stages-1-5-review-20260917T160000Z.md:228` carries the same "one is 10% slower" phrasing — allowed dated record.

## E. VF-6 falsification — no Stage 5 document claims complete-operation performance

- `stage-5-completion-report-20260917.md:129-135` (§6 "What is not claimed"): "**No complete-operation comparison.** … This row is `NOT_RUN` with that reason. No complete-operation speed claim follows from §5."
- `stage-5-completion-report-20260917.md:283-284` (§10.2): "Complete-operation comparison stays `NOT_RUN`; Stage 6 (#171) qualifies the whole core…"
- `stage-5-completion-report-20260917.md:279` (acceptance row): "complete-operation comparison `NOT_RUN` under the frozen addendum §6 … so no complete-operation or superior-speed claim … PASS at component scope".
- `stage-5-verification-addendum-20260917.md:59-60` (§4.4): "No ratio is computed across cases and no complete-operation, storage or cold-cache claim follows from this family."; §6 as quoted in §C above.
- `stage-5-terminal-handoff-20260917.md:224-226` (WP-C item 2): "deferred to Stage 6 (#171) by owner decision, recorded in addendum §6. Not a waiver, not a PASS; Stage 5 claims no complete-operation performance."
- `stages-1-5-review-20260917T230700Z.md` Q4: "Speed: not established. No complete-operation comparison exists and none is possible against the pinned reference for the composed operation…"; VF-6 row: "`NOT_RUN` | declared before collection with a source-backed reason; not waived, not promoted"; :1656: neither collection "supports 'as fast as v0.1.6'".
- Design-doc mentions of complete operations (`filesystem-tree.md:524`, `finalized-object-handoff.md:370`, `admission-and-persistence.md:415`, `content-io.md`, `stage-5-matrix-remediation-20260917.md:143`) are requirement/future-contract language ("Require existing-or-better…", "remain Stage 6 (#171) and are named as unavailable rather than read as zero") — not Stage 5 performance claims. The pipeline rows in completion-report §5 (:116-127) are single-arm elapsed times, explicitly not a comparison. **No overreaching claim found.**

## F. UNVERIFIED (could not or did not check)

1. **The measurements were not re-run.** No cargo build or harness execution was performed (read-only verification). The ratios were recomputed from the receipt's *recorded* nanoseconds; the receipt's internal consistency (driver transcript ↔ receipt) and its hash were checked, but nothing here establishes that re-running the comparison at `eb42c134` today would reproduce these numbers.
2. **Cache state** ("warm in-process fixture") is declared in the receipt and addendum but was not independently observed.
3. **Issue #171 itself** (its state/content on the tracker) was not checked — the deferral is verified only as recorded in the repository documents.
4. The sweep covered `docs/` for the superseded row values, the stamp, and "10% slower" phrasing; docs outside the repository (e.g. the tracker, external discussions) were not swept.
5. Whether the `T143008Z` receipt's wall-time budget claim ("all inside the ordinary complete-command budget") holds was accepted from the receipt's own `wall_s` values (max 5.944 s) — not re-timed.
