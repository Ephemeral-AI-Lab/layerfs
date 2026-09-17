## 2. Findings

Severity: **S1** blocks an issue's own acceptance · **S2** real defect, real missing work
or real evidence defect · **S3** documentation, latent trap or hygiene. Correctness and
data-loss findings come first; **no data-loss or corruption defect was found**.

### S1 / S2 — acceptance and evidence

**F-1 (S1) — the two qualification rows have no artifact, and the same document
contradicts itself about their state.**
`stages-3-4-closeout-report.md:34–55,113,115` record G13/G15 as
`PASS (owner waiver, 2026-09-17)`; `:439–444,471–475,510–514,526–527` (the issue-ready
`4) still say both are **OPEN** and "Neither issue may be closed until G13 (and, for
#169, G15) is PASS or waived in writing by the owner"; `:3–6` still reads "Status: in
progress … It closes nothing until every row of the gate table below is PASS". Trigger:
a reader taking either half at face value. Impact: one document carries two acceptance
states. **Smallest fix:** update the banner and `4 to the waiver state, or mark `4 in `2
as the pre-waiver text.

**F-2 (S1) — the waiver is recorded only by the implementing agent.**
A repo-wide search for `waiver|waived` over the Stages 3–4 scope returns only
agent-authored text (`stages-3-4-closeout-report.md` ×12, `stages-3-4-verification.md`
×2, `implementation-issues.md` ×2, `w8/README.md` ×4, commit messages `b29f8e4a3` and
`2b4f8b47d`). No owner-authored artifact exists in the tree. The v0.1.6 precedent records
an owner waiver of this class as `TARGET_MISS`, "not a pass"
(`release-notes/0.1.6/waivers.md:9–11`). **Smallest fix:** attach the owner's actual
direction as a retained artifact.

**F-3 (S2) — no matched campaign exists, at any n, on any of the three axes.**
`stages-3-4-verification.md:109` "No v0.1.6 comparison campaign was collected for this
batch, and none is claimed"; `:67` is `NOT_RUN`; the matched-C1 ledger `:83–86` says
"**Memory: not measured.** … #168's 'simultaneous index/codec/SQL memory' gate is
therefore **unmeasured**, not passed"; the timing ledger `:89–97` lists "Not run — No
v0.1.6 timing comparison; No release-profile arm; No warm/cold contrast".

**F-4 (S2) — the matched pair's two arms measure different quantities.**
The reference prints `store.objects.len() - objects_before` (a content-deduplicated
map-size delta) while the candidate prints `collector.objects.len()` for both
`objects_written` and `nodes_created` (a raw emission count, duplicates included). The
ledger presents "nodes read / created 11 / 3 vs 10 / 7" side by side as if
commensurable. Impact: even at n≫1 this pair could not support a storage conclusion.
**Smallest fix:** compute one defined quantity on both sides and state which.

**F-5 (S2) — read amplification on the transition is unmeasured.**
`stages-3-4-verification.md:68` — "no case exists". #169 acceptance item 2 requires it.

**F-6 (S2) — a report number contradicts its own retained receipt, and the earlier
review already reported it.**
`evidence/stages-3-4-smoke-20260916T210931Z/README.md:34` states `edit.save`
**158.824 ms** for `pipeline small-to-large`; the retained
`grow/pipeline-edit-save.json:3` says `"elapsed_ns": 193977292` = **193.977 ms**. The
prior independent review recorded exactly this as defect **E-D1**
(`evidence/stages-3-4-review-20260916T233008Z/evidence-audit.md:76,565`), and it is
**still uncorrected at HEAD**. The same README quotes the `shrink2` run under the
`shrink` label (`shrink/pipeline-edit-save.json` = 6 060 625 ns against the README's
5.970 ms, which is `shrink2`'s 5 970 333 ns). Impact: unreconciled numbers in retained
evidence. The round is admission-ineligible anyway, so this is an evidence-hygiene
failure rather than a performance one. **Smallest fix:** annotate the row with the
receipt value, as was done for the clipped `e1c` arm.

**F-7 (S2) — the timing round contains an undeclared arm.**
21 directories exist; the addendum declares 20 (3 E1 + 15 E2 + 2 E3). The extra is
`e3b-pipeline-chunked-on`, a second *timed* run of `e2-pipeline-chunked` with a **9.7×
larger wall time** (0.871 s vs 0.090 s), against the addendum's own rule ("If any
declaration here changes, a new addendum must be committed before a new receipt is
taken"). The ledger's budget section reports only "0.047 s to 0.103 s". **Smallest
fix:** declare it as a diagnostic and report both walls.

**F-8 (S2) — the timing receipts are not tied to the final artifact.**
Recorded identities `d7e5c785…` (`measure_pooled`) and `f7cdc5ff…` (`measure_edits`) do
not match the current binaries `7214908d…`/`d23f47cc…`; the example *sources* are
unchanged since `dfd54fd8e` (19 commits behind HEAD) and no sealed binary copy exists.
**Smallest fix:** archive the executable by content hash whenever a receipt is taken.

**F-9 (S2) — the frozen single-worker identity is unsatisfied in every performance
receipt.** `stages-3-4-verification.md:23` against 0 hits in the timing and matched-C1
rounds; `grep -rn 'env::var' core/crates/*/src/` is empty, so no core product code reads
the variable at all. Impact: the "single construction worker" claim is unverified for
the exact round any comparison would rest on.

### S2 — code defects and unbounded owners

**F-10 (S2) — `pending_values` is an unbounded per-save map** in no ledger row:
`cas/owner.rs:121` (`BTreeMap<[u8;73],u32>`), written at `:479` for every newly assigned
value and never cleared, reset or bounded — not on `maybe_commit`, not on index
invalidation (`:896–909`), only by dropping the owner (≈80–96 B/entry, derived). A single
save admitting many pooled leaves with distinct values accumulates one entry per distinct
value for the whole operation. `METADATA_INDEX_VALUES` bounds the ordered set, not this
map. **Smallest fix:** bound it with the same constant and reset semantics.

**F-11 (S2) — the assembled pack copy overlaps the retained open tail, uncharged.**
`pack/assemble.rs:175–252` allocates `Vec::with_capacity(assembled_length)` while
`open.groups` still owns every constituent group body, so ≥2× the lane limit is live at
the copy; on the singleton lane that is up to ≈2 × 16.78 MiB, plus SQLite's own BLOB copy
and the MEMORY journal of the same write. The design intent
(`content-io-memory-audit.md:140`) is that the losing alternative is released before the
winner is built. **Smallest fix:** drain the tail into the assembly, or charge the
overlap as a declared peak.

**F-12 (S2) — cleanup's final pack deletion is one unbounded transaction.**
`sqlite/cleanup.rs:93–96` deletes all remaining packs in a single statement under a
MEMORY journal: deleting N packs totalling S bytes dirties ≈S/4096 journal pages in one
transaction. The objects pass is correctly paged and budgeted (`:41,84–91`). This
contradicts gate **G10**'s claim that "cleanup transactions" are bounded.

**F-13 (S2) — transaction row/byte limits are post-hoc.**
`cas/owner.rs:794–811` tests `rows ≥ 8191` / `bytes ≥ 4 MiB−1` *after* `seal_group`
(`:772–773`) and `write_pack` (`:788–789`) have added to the accumulators, so one
transaction may overshoot by one group plus one pack (up to ≈16.78 MiB). The declared
limits are steady-state bounds, not peak bounds.

**F-14 (S2) — `pool::next_ordinal` truncates at the ordinal ceiling.**
`sqlite/pool.rs:48–51` accepts the end value 2³² (range `1..=u32::MAX+1`) and then does
`next as u32`, which is 0; `cas/owner.rs:563–573` then returns ordinal 0, which fails
downstream as `Integrity("metadata group row")` / `Integrity("metadata index
chronology")` rather than the intended ordinal-maximum refusal. Reachable only after
4 294 967 295 pooled values, and the outcome is still a refusal rather than a wrong
write, but the guard is defeated and the error is mislabelled.

**F-15 (S2) — the file state is never cross-checked against what the tree covers.**
`FileState.logical_len` and `extent_count` are decoded (`file/mapping/codec.rs:296–306`)
and used to accept a requested range (`file/read.rs:142–148`) but are not compared with
the mapping tree's actual coverage; `descend` (`file/mapping/read.rs:166–214`) can finish
having emitted fewer bytes than the range asked for and still return `Ok`. Honest
construction cannot produce that state, so this is a corrupt/forged-root gap rather than
a live defect — but it is the one read-path check the C1 contract's "authenticated
logical read" wording implies and does not have. Related: `ExtentSlice::new`
(`file/mapping/types.rs:36–38`) validates only non-zero length, so a `u32`
`source_offset` of 2³²−1 decodes "successfully" and is rejected only at read time
(`read.rs:120–122`).

### S3 — arithmetic, dead code, documentation

**F-16 (S3) — the schema/profile change is explicit and defensible; recorded here for the
required compatibility audit.** `user_version` 2 → 4; one column added
(`store_policy.metadata_delta_max_depth`, now four tables / twenty-one columns); cutoff
CHECK widened `1024..131072` → `131072..1048576`; depth CHECKs widened `0..8`/`0..4` →
`0..50`; `object_role` `1..5` → `1..6`; older Stores rejected, never migrated
(`sql/schema.sql:1–11`, `sqlite/schema.rs:138–147`, commit `64e3f9d6a`). This is the
"one explicit supported-range contract" the handoff asks for. **Note the narrowing:** the
*minimum* accepted cutoff rose from 1 KiB to 128 KiB, so any Stage 2 Store created with a
small cutoff is unreadable by design.

**F-17 (S3) — `ConstructionPolicy::capacities()` does unchecked arithmetic on an
unvalidated public value.** `content/policy.rs:131–132` computes
`self.small_file_threshold_bytes as usize - 1` and `:133` calls
`conservative_frame_bound`, without calling `validated()`. `ConstructionPolicy::new` is
`pub const` and documented "call `validated()` before use" (`:59–70`), so
`ConstructionPolicy::new(0,8,4).capacities()` panics in debug and wraps in release. All
in-tree production callers validate first (`StorageCapacities::from_policy` at
`storage/policy.rs:298–300`), so it is not reachable through C2's public API; it is a
footgun on C1's public surface. **Smallest fix:** return `ContentResult` or
`debug_assert!` the invariant. Related: `StorageCapacities` has all-`pub` fields and no
`#[non_exhaustive]` (`storage/policy.rs:253–294`), so a caller-built literal can make
`codec.rs:71,430,488` degenerate (panic, not UB).

**F-18 (S3) — declared-but-never-read constants and functions.**
`ConstructionCapacities::mapping_node_limit` (`content/policy.rs:144`) and
`PackLane::records_per_group()` (`pack/layout.rs:100–105`, which would declare 1 record
for `PooledMetadata` while the enforced cap is 165) are never read. Five
`storage/policy.rs` constants and `SaveHandoff::failure` have zero references in any
`src/`, `tests/` or `examples/` file. `pool/leaf.rs:112–121` `ordinals()` and
`pool/index.rs:186–188` use `Vec::contains` dedup inside a loop (O(candidates²); ≈8.6·10⁹
comparisons if all 131 072 retained entries share a fingerprint). The derive-only
`EditObjects.committed`/`parent_refs`/`detached`/`published` are uncharged, and
`settle()` (`tree.rs:243–248`) linearly rescans `detached`, so it is O(n²) in that set.

**F-19 (S3) — measured numbers exist in prose without receipts.**
(a) `w7/README.md:82–87` gives six RSS before→after pairs; `w7/w7-verify.log` retains
three tables and **none matches** (0 occurrences of the README's `5226496`/`13336576`).
(b) Gate G4's with-fix frontier vector `[2208,2288,2448,2768,3408]` appears only in
`w3/README.md:114`; `edit_bounds` prints peaks only on failure and no log runs it with
`--nocapture`, so the passing vector has no receipt (the *control*
`[6308,…,129728]` does, at `w3/w3-fails-without-fix.log:12`).
(c) `w7/README.md:94` and gate G12 say "3.45 **MiB**"; the measured value is
3 450 007 **B** = 3.45 **MB** = 3.29 MiB.
(d) `w7/README.md:77–78` and G12 call the fixture "the E1b shape"; E1b is 128 leaves with
227 retained entries, while the fixture is 24 leaves / 2 400 values, and E1a (24 leaves)
retained only 123 entries. **Smallest fix:** label every number with the receipt that
produced it, or mark it unavailable.

**F-20 (S3) — LOC accounting: the scope changes mid-batch, and 15 commits carry no scope
statement.** Every one of the 31 commits carries exactly one `Production LOC:` line (the
narrow claim in gate G20 is true), and my independent recount reproduces every core-only
figure. But the reported *scope* changes silently: commits `64e3f9d6a..2b2dbc028` report
the **combined** total (74 628 → 78 891, reference 68 476);
`01d9f70f3..91c3a0741` report **core-only** (10 415 → 10 929) with an explicit `Scope:`
note; and the 15 commits from `97414bac4` to HEAD report core-only with **neither
`Scope:` nor `Method:`**. The consecutive pair 78 891 → 10 415 is a −68 476 scope change,
not a production change; `01d9f70f3` explains it in prose, the later commits do not. In
addition, the counter correction in `6566a95a3` changed the reference subtotal from
**68 476 to 65 417**, so combined figures quoted earlier (for example `c99192b16`'s
"combined 79405") are stale by 3 059. **Smallest fix:** add `Scope:` and `Method:` to
every commit message and restate the reference subtotal, or reduce the line to the delta.

**F-21 (S3) — documents disagree with each other and with the source.**
(a) `stages-3-4-report.md` has **two sections numbered 8** (`:356`, `:378`), and its two
LOC tables describe two different snapshots (W1 and W9), **neither of which is HEAD** —
`file/edit/tree.rs` physical lines are 645 (W1) / 805 (W9) / **901** (HEAD), and its `3
file list matches neither snapshot.
(b) `stages-3-4-report.md:521–528` lists eight files "without a plan row", of which
`object/inode_leaf.rs`, `file/edit/split.rs`, `concat.rs`, `finish.rs`, `sqlite/pool.rs`,
`pack/layout.rs` and `encoding/pool/*` **all have plan rows**
(`stages-3-4-file-plan.md:100,118,119,120,221,232,214–219`). The only genuine unplanned
Stage 3–4 addition is `file/edit/tree.rs`.
(c) G18 and `report `8.2` still certify core 10 983 / combined 76 400 while W10, the issue
summaries, the final round and the tip commits say 11 058 / 76 475.
(d) `verification.md:67` says the matched campaign is `NOT_RUN` because "the owner
decision … **is open**", while `:110–113` in the same file records the waiver as resolved.
(e) The report attributes to the previous review "956 over-removed lines and 4 083
test-module lines"; that review measured **1 027** and **3 737**
(`stages-3-4-review-20260916T233008Z.md:579–580`).
(f) `physical-encoding-and-packing.md:498` states that metadata "has 16 edges and 128-KiB
canonical closure"; the source has `METADATA_CHAIN_CANONICAL_LIMIT = 65 536` and
`METADATA_CHAIN_ENCODED_LIMIT = 139 281` (`storage/policy.rs:126,128`).
(g) Closeout `2.1 tabulates 10 findings but its prose says "Eleven findings" and names F9
as closed; F3 (the measurement blocker) has no row at all. Gate G20's second clause
therefore rests on a table that leaves an S2 finding (F10) expressly unanswered
(escalation E2) and omits F3/F9 from the row set.
(h) `w9/w9-verify.log:875` retains a `git diff --check` **exit 2**, cleared only in the
later `final` round.

**F-22 (S3) — zero tests cross the codec FFI boundary.**
`grep` over `core/crates/*/tests` for `zstd`/`decompress`/`CompressionWorkspace`/
`parse_frame_header` returns nothing; the zstd magic literal occurs once in the tree, in
the implementation. The twelve `unsafe` sites in `encoding/codec.rs` are therefore
exercised only indirectly through real saves and reads, never with a hostile frame. Three
corruption oracles are also union/blanket (`delta_chains.rs:254–269` accepts `Engine(_)`;
`cas_reuse.rs:181–200` accepts `Collision|Integrity`; `inode_leaf.rs:120–175` is bare
`is_err()`), and no test encodes "must reject rather than panic" (no `#[should_panic]`
anywhere under `core`).

**F-23 (S3) — four of the ten closeout packets carry no control log for their new
oracles.** W1–W6 each retain a `*-fails-without-fix.log` (exit 101 once or twice each;
W6's also holds the disclosed control-A run that "proves nothing", exit 0); **W7, W8, W9
and W10 retain only a verify log**, although W7 backs gate G12 (memory), W8 backs G14/G16,
W9 backs G17/G18 and W10 backs G19. Gate G7's claim that "each new case fails without its
fix" is therefore demonstrated for part of the batch only, and the w4 packet itself concedes
that "W4.2, W4.3 and W4.7's role case … their 'fails without the fix' status is
source-derived, not measured". This is an evidence-quality limit, not a correctness defect:
the oracles still run and pass. Raw census:
`evidence/stages-3-4-review-20260917T022248Z/packet-control-audit.txt`.
**Smallest fix:** add the missing control run for any oracle whose margin the batch intends
to rely on, or label those four rows source-derived.

### Positive findings worth recording

* **P-1.** The reference tree is **unmodified**: since the implementation base only two
  *development examples* were added under `crates/layerfs-content/examples/`
  (`rope_edit_oracle.rs`, `rope_edit_timing.rs`); `crates/layerfs-content/src` and
  `crates/layerfs-layerstack-store/src` are byte-identical to v0.1.6 `44cf74848`.
  Reference production LOC is unchanged at 65 417.
* **P-2.** Isolation holds in source: C1 depends only on `blake3` +
  `layerfs-telemetry` and contains no `rusqlite`/`zstd`/storage/Workspace/history
  reference; C2 contains no call to C1's `construct_file`/`apply_edits`/`build_streaming`.
* **P-3.** Both `unsafe`-free crates enforce it with `#![forbid(unsafe_code)]`; all twelve
  product `unsafe` sites are in one file and were read individually.
* **P-4.** The schema/format deviation (pooled leaf records in the v1 ordinary lane, v5
  refused) is recorded in code and in the design document and pinned by a
  `physical_formats` case, rather than silently applied.

---

