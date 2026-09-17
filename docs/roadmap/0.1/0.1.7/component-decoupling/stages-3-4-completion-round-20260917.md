# Stages 3–4 completion round — what was closed, what was not, and against which denominator

> **Status: dated additive report.** Written 2026-09-17 from the completion round
> that follows the independent review
> [`stages-3-4-review-20260917T022248Z.md`](stages-3-4-review-20260917T022248Z.md)
> and supersedes the work list in
> [`stages-3-4-final-completion-prompt.md`](stages-3-4-final-completion-prompt.md).
> Target LayerFS v0.1.7; not a released contract. Additive: it corrects reports and
> adds receipts, and rewrites no collected receipt.

## 0. Identity

| Item | Value |
| --- | --- |
| Reviewed snapshot | `b29f8e4a3` (clean tree) |
| Start of this round | `b29f8e4a3`, working tree dirty with four post-review artifacts |
| First action | committed those artifacts unchanged as `7eca9c369`, so every later build has the clean tree it needs |
| Code commit | `94d6ab8a2` — Package B (production + tests) |
| Case commit | `841d9d2b1` — the read-amplification case (tests only) |
| Reference revision | `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, unchanged, source/oracle input only |
| Receipts | `evidence/stages-3-4-read-amplification-20260917T034743Z/` and `evidence/stages-3-4-corrections-and-harness-20260917T035811Z/` |
| Checks | `cargo +1.85.1 test/clippy --manifest-path core/Cargo.toml --workspace --locked`, `cargo +1.96.0 fmt --all --check`, `core/tools/check_product_boundary.py`, both tool suites, `tools/production_loc.py`, `git diff --check` — all exit 0, **290 tests across 41 test binaries** |

## 1. The two denominators, stated once more

Both remain true and neither cancels the other.

* **The batch as scoped and closed** — code, tests, oracle, gates, packets — was
  and is substantially complete: 18 of 20 gate rows rest on a named artifact and
  11 of 13 declared registry cases are RUN. This round did not change that reading;
  it added one RUN row (read amplification, now 12 of 13).
* **#168/#169's own "existing-or-better qualified latency/storage/memory"
  requirement is still 0 of 3 axes measured.** No campaign exists at any n, and
  this round did not collect one. The only reference pair remains n=1,
  in-process, Store-free, with interleaved observations and two arms that compute
  different quantities. **Nothing in this report may be read as satisfying that
  requirement**, and nothing here upgrades G13 or G15, which stay `PASS (owner
  waiver)` and unmeasured.

## 2. Deliverables

| # | Deliverable | Status |
| --- | --- | --- |
| 1 | §A1 case + control + one fresh evidence directory, registry row updated | **Done** — 12 measured rows across three cutoffs plus a Store-charged row, six negative controls, row flipped to `RUN` (`stages-3-4-verification.md` §2.1, §8.7) |
| 2 | Package B fixes with tests, or recorded decisions with derived bounds | **Done for B1–B8**; **B9.1 and B9.3 implemented**, **B9.2, B9.4, B9.5 recorded as decisions** with reasons (§4) |
| 3 | Package C corrections applied in the reports that quote the numbers | **Done, C1–C13** (§3) |
| 4 | Package D fixes with receipts, or waivers naming each | **D1–D6 landed** with receipts except D6, which is a label by the requirement's own alternative (§5) |
| 5 | §6 answered in writing | **E3 answered and closed by evidence; E1 and E2 recorded as unanswered owner decisions** with the consequence stated (`stages-3-4-closeout-report.md` §6) |
| 6 | If E1 = go: the addendum, sealed pair, aligned boundary, reference arm, campaign | **Not done — E1 was not answered.** The prerequisites are landed so the decision is executable; no sample was taken and no addendum was committed |
| 7 | A fresh additive report beside the review, with its own evidence directory | **This document** + `evidence/stages-3-4-corrections-and-harness-20260917T035811Z/` |

## 3. Package C — corrections applied

Each is a dated block in the report that carries the number. **No receipt was
rewritten, re-labelled or promoted**, and no failing or `INELIGIBLE` row was
deleted. The append-only idiom used throughout is the one already in
`evidence/stages-3-4-oracle-20260916T214846Z/README.md` and `w9/README.md` §W9.1.

| # | Where | What the correction says |
| --- | --- | --- |
| C1 | smoke `README.md` | `edit.save` for small→large was **193.977 ms** (`grow/pipeline-edit-save.json:3`); the quoted 158.824 ms has no receipt and is withdrawn, not replaced |
| C2 | same | the row quoted `shrink2` under the `shrink` label; both receipts are tabulated (`shrink` 6 060 625 / 54 762 208 ns, `shrink2` 5 970 333 / 54 928 042 ns) and the label, not the receipt, was wrong |
| C3 | `w7/README.md` | the `rss before → after` column matches none of the three retained ledgers; the withdrawn values and all three ledgers are tabulated side by side and the column is marked *unsourced* |
| C4 | `w7/README.md`, G12 | 3 450 007 B is 3.45 MB / **3.29 MiB** |
| C5 | `w7/README.md`, G12 | the fixture is not "the E1b shape" (E1b is 128 leaves / 227 values); it is a 24-leaf, 2 400-value fixture of its own |
| C6 | `w3/README.md` | the with-fix frontier vector now has a receipt: the test prints it, and this tree reproduces `[2208, 2288, 2448, 2768, 3408]` exactly |
| C7 | `stages-3-4-report.md` | the duplicate `## 8.` is resolved — the second is now `## 9.` with §9.1–§9.3 — and each LOC table states the snapshot it belongs to (`file/edit/tree.rs` 805 W9 / 901 reviewed tip / 913 now) |
| C8 | same | six of the eight "files without a plan row" **have** plan rows; the only genuine unplanned addition is `file/edit/tree.rs` |
| C9 | G18, §9.2, L33, `w9/README.md` | 10 983 / 76 400 are the W9 snapshot's; **the tip is 11 058 / 65 417 / 76 475**, and combined figures quoted before `6566a95a3` are stale by 3 059 |
| C10 | `physical-encoding-and-packing.md:498` | metadata's canonical closure is **65 536 B** (`METADATA_CHAIN_CANONICAL_LIMIT`), with `METADATA_CHAIN_ENCODED_LIMIT` 139 281 — not "128-KiB" |
| C11 | report §3 | "956 / 4 083" are **this batch's** measurements; the earlier review measured **1 027 / 3 737**. Both stand as their own |
| C12 | `closeout-report.md` §2.1 | F3 now has a row (recorded as *not closed by this batch*); the row census is stated (12 rows); F10 is relabelled as an **unanswered owner decision (E2)** rather than a closed S2 |
| C13 | `stages-3-4-evidence-closeout-prompt.md` §A2 items 8–9 | verified and closed, with what each was verified against |

## 4. Package B — code defects

| # | Finding | What changed | Proof |
| --- | --- | --- | --- |
| B1 | `pending_values` unbounded (F-10) | bounded at one leaf's row count and **reset per leaf**, because the catalogue rows and the retained window answer those values from then on; a leaf that somehow exceeded the bound fails closed | `one_leaf_may_hold_every_row_the_pooled_value_memo_is_bounded_by`; control F (bound lowered to 4 → `Integrity("metadata pending values")`); the existing cross-leaf reuse case still passes |
| B2 | uncharged peaks (F-11, F-13) | a **closed** pack is assembled by consuming its groups, so each body is released as it is copied; the losing compact record is released before the singleton record is built; the retained-tail overlap and the post-hoc transaction check are **declared** with their multiplicities | `closing_a_pack_and_retaining_its_tail_assemble_the_same_bytes`; the retained-tail overlap is declared in `assemble`'s own doc and the ledger |
| B3 | unbounded cleanup delete (F-12) | the pack pass is paged and charged exactly like the objects pass | `cleanup_pages_the_pack_rows_instead_of_deleting_them_in_one_statement`; control E (`0 page(s)`) |
| B4 | `next_ordinal` truncation (F-14) | the ceiling reports itself as `Integrity("metadata ordinal maximum")`; the reader keeps a wide `ordinal_end` cursor so a full Store stays readable | two boundary cases; control D returned ordinal **0** |
| B5 | no coverage cross-check (F-15) | the state is checked against the root page it decoded, the traversal is checked to have emitted the range exactly, and `ExtentSlice::new` is bounded by the frozen chunk grammar so an unsatisfiable slice fails at decode | three corrupt-root cases; controls B and C |
| B6 | unchecked arithmetic (F-17) | the invariant `validated` enforces is asserted, and the limits are derived with saturating arithmetic so an unvalidated candidate stops in debug and fails closed in release | compiled invariant; no in-tree caller passes an unvalidated policy |
| B7 | dead code and quadratic shapes (F-18) | deleted `mapping_node_limit`, `records_per_group` (and its two test assertions) and seven unreferenced `storage::policy` constants; the index's candidate dedup is an ordered set, `pool/leaf::ordinals` asks a set, and `settle` no longer restarts a linear scan per release | compiled; the constants are `grep`-verifiable |
| B8 | codec FFI untested (F-22) | a six-case decode family, and two union oracles tightened to their exact variants — which exposed that the `cas_reuse` case never reached the collision branch its name claimed, and that two `inode_leaf` cases patched the envelope while claiming to patch a role tag and a count | `codec_frames` 6/6; `delta_chains`, `cas_reuse`, `inode_leaf` tightened |
| B9.1 | one point query per row | the leaf's distinct unknown values go to the index in **one** call | the batched call is the only `find` on that path |
| B9.3 | six re-hashes per admission | `select` takes the identity the caller already holds | `id` is threaded from `FinalizedObject::id()` |

**B9.2, B9.4 and B9.5 are decisions, not implementations.** B9.2 (a fresh
`PoolReader` per pooled-leaf resolution) and B9.5 (group-body clones, a twice-decoded
leaf, pending-canonical rescans) touch the read path that the campaign's aligned
byte-accounting boundary will itself need to re-instrument; changing them now would
move the very quantities the campaign is supposed to measure, without a measured
verification case to justify the move. B9.4 is inherited from v0.1.6
(`crates/layerfs-content/src/file/content.rs:252,279`) and is therefore not a Stages
3–4 win in any case. Each is recorded with that reason rather than as a silent
omission, and none is reported as a saving.

## 5. Package D — harness prerequisites

| # | Item | Status |
| --- | --- | --- |
| D1 | telemetry clipping | **Landed**: a clipped tree is a hard failure for a measured row. Reproduced on the 512-leaf arm — 1 024 nodes, `exit: 1`, named error. The budget itself is unchanged |
| D2 | budget metric | **Landed**: all four examples print `wall_seconds` from a timer started in `main` |
| D3 | worker identity | **Recorded**: the variable is now exported in a receipt, and `verification.md` §8.8 states that no core source reads it, so the single-worker property is by construction, not by environment |
| D4 | C2 lane ignores `--case` | **Landed as disclosure**: every run of that lane prints what it is and what it stored. The finding reproduces exactly — four of five cases write `b59c7b24d91ad7d4…`, 294 912 B |
| D5 | seal the matched pair | **Landed**: binaries archived by sha256 with identity files under the git-ignored `benchmark-results/host-store/binary-archive/`. **The timing round is not repaired** — its hashes can never match again |
| D6 | control logs | **Labelled, not repaired**: W7–W10 now carry a dated section stating they are source-derived, with the reason a manufactured control would be worse |

## 6. Per-commit LOC disclosure (F-20), and the historical audit

**The rule going forward**, applied to this round's commits: every commit message
states the **scope** (core / reference / combined, with file counts), the **method**
(`python3 tools/production_loc.py`, the exact scope and exclusions, the same counter
for both snapshots) and **both subtotals** — or reports only the delta. All three
commits of this round do.

**The historical audit stands as the review recorded it** and is not re-litigated
here: all 31 commits in `c38961f2f..b29f8e4a3` carry exactly one `Production LOC:`
line and their core-only figures recount correctly, but the scope is unstable — 7
report combined, 9 report core-only with a `Scope:` note, and **15 report core-only
with neither `Scope:` nor `Method:`** — and the counter correction in `6566a95a3`
moved the reference subtotal from 68 476 to 65 417, leaving earlier combined figures
stale by 3 059. Those messages are historical records and are not rewritten; the
rule above governs from `7eca9c369` onward.

| Commit | Production LOC | Scope and method |
| --- | --- | --- |
| `7eca9c369` docs | 76 475 → 76 475 (delta 0) | core 11 058 / reference 65 417 / combined 76 475; docs only |
| `94d6ab8a2` code | 76 475 → 76 577 (delta **+102**) | core **11 160** in 75 files / reference 65 417 / combined 76 577; tests excluded |
| `841d9d2b1` tests | 76 577 → 76 577 (delta 0) | unchanged; external tests only |

## 7. What was not done, and why

* **No campaign, at any n, on any axis.** E1 was not answered by the owner and this
  round does not answer it for them. Latency, storage and memory stay unqualified;
  G13/G15 stay unmeasured; #171 is named as the carrying issue if the answer is
  "no".
* **The clipped `e1c-pooled-512` arm was not re-collected.** #168 item 6 is
  therefore **met for the unclipped arms only**, and the clipping is stated beside
  it rather than buried: 57.46 % of that arm's scope is unattributed, the receipt is
  untouched, and the arm is now also a *failing* run under D1.
* **No control log was manufactured for W7–W10.** They are labelled.
* **No control was produced for B2's assembly equivalence or for B8's codec family**,
  with the reasons in the A1 round's §6; both are guards rather than defect oracles.
* **B9.2, B9.4, B9.5 are decisions**, per §4.
* **The read-amplification case is not a performance claim.** It is a correctness
  and accounting case in the debug profile with in-process fixtures; every number in
  it is a count of bytes, not a timing.
* **Reference retirement, #165/#170/#171/#172 and the release decision are
  untouched.** No tag, no deployment, no history entity, no Workspace dependency,
  and `tools/preflight.sh` stays retired.

## 8. Reproduction

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
python3 tools/production_loc.py
git diff --check

cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked \
  -p layerfs-content --test edit_transitions -- --nocapture
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked \
  -p layerfs-storage --test edit_pipeline -- --nocapture
```

Every command, its exit code, its wall time, its tool identity and its raw log are
in the two receipt rounds named in §0. The negative controls are in their
`*-fails-without-fix.log` files with the patch that produced each one quoted
alongside.
