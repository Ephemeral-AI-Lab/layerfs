# Handoff prompt: finish Stages 3–4 and close the remaining items

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract. Written
> 2026-09-17 from the independent review
> [`stages-3-4-review-20260917T022248Z.md`](stages-3-4-review-20260917T022248Z.md)
> and its evidence round
> `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260917T022248Z/`.
> Receipts and contracts are append-only: this prompt adds work and corrects reports; it
> never re-labels or rewrites a collected receipt.

## Copy/paste assignment

You are finishing **Stage 3 (#168)** and **Stage 4 (#169)** of the LayerFS v0.1.7
component-decoupling batch, under **#165**, in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

The implementation is done and externally tested: 273 tests pass across 43 targets, and
the sealed v0.1.6 oracle agrees on all nine edit cases including the 80+100→90+90
repartition. **What is not done is one acceptance item, a set of code-level defects, and
the evidence corrections that follow from both.** This prompt sequences exactly those.

### Read first

1. [`stages-3-4-review-20260917T022248Z.md`](stages-3-4-review-20260917T022248Z.md) —
   the independent review. Its §2 (findings F-1…F-23), §4 (criteria), §6 (statistics and
   the two memory conclusions) and §7 (limits) are the authoritative list of what is
   open. Its evidence round carries the raw commands and exits.
2. [`stages-3-4-handoff.md`](stages-3-4-handoff.md) and
   [`stages-3-4-file-plan.md`](stages-3-4-file-plan.md) — the original scope and the
   recommended per-file/directory LOC.
3. `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
   `docs/general/release-policy.md`, `docs/general/documentation-policy.md`.
4. [`stages-3-4-verification.md`](stages-3-4-verification.md) — the frozen contract (§1
   and §2 are frozen; do not rewrite them) and its §8 erratum.
5. [`stages-3-4-evidence-closeout-prompt.md`](stages-3-4-evidence-closeout-prompt.md) —
   the evidence-only subset of this work. **This prompt supersedes it where they differ**,
   because it also carries the code-level defects. Where it is more specific, follow it.

### Two denominators — state which one you are reporting against

* **The batch as scoped and closed:** 18 of 20 gate rows rest on a named artifact and 11
  of 13 declared registry cases are RUN. This part is substantially complete.
* **#168/#169's own "existing-or-better qualified latency/storage/memory" requirement:**
  **0 of 3 axes are measured.** No v0.1.6 comparison campaign exists at any n, and the
  only reference pair is n=1, in-process, Store-free, with interleaved observations and
  two arms that compute *different quantities*.

Both are true. Never report the first as if it satisfied the second.

---

## 1. Start identity and standing constraints

* Resolve and record the actual start commit and tree before editing. The reviewed
  snapshot is `b29f8e4a3708a3ece9b67fb719af990bbb0b1cb5`; the working tree is **no longer
  clean** (three docs and one prompt were appended after the review — see the review §0.1).
  Decide and record whether you commit those first.
* Reference revision stays `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`: source/oracle input
  only, never a candidate dependency, include, linked fallback or alternate runtime path.
  Reference retirement remains a later owner decision; while it is deferred the matched
  pair is still buildable, and that window is why §5.5 exists.
* **Prohibitions.** No retry, error-driven fallback, busy handler, `fsync`/`fdatasync`/
  `sync_all`/`sync_data`, WAL or added crash-durability machinery. No third-party patch,
  fork, vendor or registry edit; builds stay `--locked`. No successful-version rollback.
  No generic payload spill or whole-file reconstruction fallback. No Workspace/FUSE/
  daemon/history dependency in C1 or C2. One construction worker. No receipt rewrite,
  re-label or promotion; no deletion of a failing or `INELIGIBLE` row; no best-of; no
  raised limit, inflated timeout or extra worker to make a gate pass. No aggregate gate,
  workflow or wrapper — `tools/preflight.sh` stays permanently retired.
* **Checks** (explicit, per workspace — there is no CI and no aggregate gate):
  `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked`;
  `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings`;
  `cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check`;
  `python3 core/tools/check_product_boundary.py`;
  `python3 -m unittest discover -s core/tools -p 'test_*.py'`;
  `python3 -m unittest discover -s tools -p 'test_production_loc.py'`;
  `python3 tools/production_loc.py --files`; `git diff --check`.
* **Production LOC.** Every commit reports first-parent before → after (signed delta) with
  scope and method, plus reference/core/combined subtotals. Current tip: core **11 058**
  in 75 files (C1 4 463, C2 5 863, telemetry 732), reference 65 417, combined 76 475.
  Product files stay ≤999 physical lines and `lib.rs`/`mod.rs` ≤200.

---

## 2. Package A — in-batch acceptance work. **Do not defer.**

### A1 (blocks #169) — Physical read amplification on the representation transition

`stages-3-4-verification.md:68` records this row `NOT_RUN` — "no case exists". It is
**#169 acceptance item 2**, and it is *not* covered by the G13/G15 waiver. Four of the
batch's own documents forbid postponing it by relabelling it Stage 6
(`stages-3-4-completion-handoff.md:220`, `stages-3-4-continuation-prompt.md:74`,
`stages-3-4-verification.md:153`, `stages-3-4-continue-acceptance.md:98–100`).

Do it now, on the code it describes. It is the cheapest missing proof: `edit_transitions`
already passes 5/5 and the counters already exist and already print.

1. **Define the accounted quantity once**, in bytes as well as counts, *before* measuring.
   Use existing counters, do not invent a hook: C2 `StoreReadCounters`
   (seen working at `evidence/stages-3-4-closeout-20260916T235641Z/w10/w10-verify.log:1052`)
   and C1 `EditCounters` (`file/edit/tree.rs:36–52`).
2. **Case matrix** — for every accepted cutoff (128 KiB, 256 KiB, 1 MiB), on a chunked
   base: small→large (insert), large→small (delete), any→empty (full delete), plus an
   in-place overwrite control. Per case assert: the result equals the independent model
   **and** a fresh construction of the final bytes; a discarded range is never read; the
   base is acquired through the supplied provider and not re-read whole; the read counters
   are charged to the transition and the declared bound is asserted, not described.
3. **Placement.** Content-side cases in `core/crates/layerfs-content/tests/edit_transitions.rs`
   or a sibling target; anything needing a Store in
   `core/crates/layerfs-storage/tests/edit_pipeline.rs`. External tests only; no test-only
   public API in production.
4. **Control.** Each new assertion needs a run that fails without it, in a
   `*-fails-without-fix.log` — and avoid the no-op-patch trap (`w6/w6-fails-without-fix.log:1–4`
   shows the pattern for catching a patch that did not apply).
5. **Evidence.** One fresh directory
   `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-read-amplification-<UTC>/` with a README
   quoting its own raw log, commands, exits and wall times.
6. **Only then** flip `stages-3-4-verification.md:68` from `NOT_RUN` to `RUN`, naming
   that receipt. If the case cannot be built, leave it `NOT_RUN` with the measured reason
   and obtain a **written waiver naming this row** — the same standard G13/G15 were held to.

### A2 (blocks #168) — The clipped timing arm and the completeness of save/read evidence

`e1c-pooled-512` is telemetry-clipped: 57.46 % of its scope unattributed (review §6.1;
root 3 105 519 375 ns, retained children 1 320 950 951 ns). #168 acceptance item 6 requires
that save/read evidence "include all real acquisition/encoding/packing/SQL work". Either
re-collect that arm inside the 1 024-node budget, or record item 6 as met for the
unclipped arms only and state the clipping beside it.

---

## 3. Package B — code defects and resource hardening

All locations verified against the reviewed tree. Each needs a test or a recorded decision;
none may be fixed by deleting validation.

### B1 (S2) — `pending_values` is unbounded
`cas/owner.rs:121` (`BTreeMap<[u8;73],u32>`), written at `:479` for every newly assigned
value, never cleared, reset or bounded — not on `maybe_commit`, not on index invalidation
(`:896–909`), only by dropping the owner (~80–96 B/entry). `METADATA_INDEX_VALUES` bounds
the ordered set, **not** this map, and the map appears in no allocation-ledger row.
**Deliverable:** a bound with reset semantics, or a written justification with its derived
worst case and a ledger row.

### B2 (S2) — peak transients the declared limits do not cover
* `pack/assemble.rs:175–252` allocates the assembled pack while `open.groups` still owns
  every constituent body ⇒ ≥2× the lane limit live; on the singleton lane up to ≈2 × 16.78 MiB.
* `encoding/full.rs:183` and `:192` encode the same record twice on the oversized path
  (`delta/record.rs:38–46,63–78` treat `WholeFile` and `Singleton` identically).
* `cas/owner.rs:794–811` tests the transaction row/byte limits **after** `seal_group`
  (`:772–773`) and `write_pack` (`:788–789`), so one transaction may overshoot by one
  group plus one pack.
**Deliverable:** release the losing alternative before building the winner, and charge or
declare the overlap; or state each as a declared peak with its multiplicity.

### B3 (S2) — cleanup's final pack deletion is unbounded
`sqlite/cleanup.rs:93–96` deletes all remaining packs in one statement under a MEMORY
journal (≈S/4096 journal pages for S bytes). The objects pass at `:41,84–91` is correctly
paged and budgeted; this one is not, which contradicts gate G10's "cleanup transactions
bounded". **Deliverable:** page and charge it like the objects pass.

### B4 (S2) — `next_ordinal` truncates at the ordinal ceiling
`sqlite/pool.rs:48–51` accepts 2³² and then does `next as u32` ⇒ 0;
`cas/owner.rs:563–573` then returns ordinal 0, which fails as
`Integrity("metadata group row")` or `Integrity("metadata index chronology")` instead of the
ordinal-maximum refusal. Still a refusal rather than a wrong write, but the guard is
defeated and mislabelled. **Deliverable:** make the ceiling report itself, with a test at
the boundary.

### B5 (S2) — read paths do not cross-check what the tree covers
`FileState.logical_len`/`extent_count` are decoded (`file/mapping/codec.rs:296–306`) and
used to accept a range (`file/read.rs:142–148`) but never compared with the mapping tree's
actual coverage; `descend` (`file/mapping/read.rs:166–214`) can return `Ok` having emitted
fewer bytes than requested. `ExtentSlice::new` (`file/mapping/types.rs:36–38`) validates
only non-zero length, so a `source_offset` of 2³²−1 decodes and is rejected only at read
time (`read.rs:120–122`). **Deliverable:** the coverage check, and slice validation at
decode; a corrupt-root test for each.

### B6 (S3) — unchecked arithmetic on a public path
`content/policy.rs:131–133` computes `threshold as usize - 1` and calls
`conservative_frame_bound` without `validated()`, so
`ConstructionPolicy::new(0,8,4).capacities()` panics in debug. Not reachable through C2
(`StorageCapacities::from_policy` validates first), but it is public surface.
**Deliverable:** return `ContentResult`, or `debug_assert!` the invariant.

### B7 (S3) — dead code and quadratic shapes
Never-read: `ConstructionCapacities::mapping_node_limit` (`content/policy.rs:144`),
`PackLane::records_per_group()` (`pack/layout.rs:100–105`, which would declare 1 record for
`PooledMetadata` while the enforced cap is 165), five `storage/policy.rs` constants
(`:67,69,90,92,96`) and `SaveHandoff::failure` (`cas/store.rs:492–495`). Quadratic:
`pool/index.rs:186–188` and `pool/leaf.rs:112–121` dedup with `Vec::contains` inside a
loop (≈8.6·10⁹ comparisons if all 131 072 retained entries share a fingerprint);
`tree.rs:243–248` rescans `detached` linearly. **Deliverable:** delete the dead items;
bound or document the scans.

### B8 (S3) — the codec FFI boundary has zero direct test coverage
All twelve product `unsafe` sites are in `encoding/codec.rs`; `grep` over
`core/crates/*/tests` for `zstd`/`decompress`/`CompressionWorkspace`/`parse_frame_header`
returns nothing. Add a decode test family: one valid frame, one declared-size lie, one
truncated frame, one bad checksum, one wrong-window frame. Then tighten the union oracles
(`delta_chains.rs:254–269` accepts `Engine(_)`; `cas_reuse.rs:181–200` accepts
`Collision|Integrity`; `inode_leaf.rs:120–175` is bare `is_err()`) to exact variants.
**Deliverable:** the tests, plus an honest statement of what sanitizer/fuzz coverage still
does not exist.

### B9 (S3) — efficiency items the handoff's §6 intends
These are **estimates, not measurements** (review §5). The handoff requires evidence for
the intended reductions, so either implement them with the named verification case or record
a decision not to, with the reason.
1. `cas/owner.rs:447–488` calls the batch index API `PoolIndex::find` once per row with a
   one-element slice (`:469`), costing ~100 SQL point queries and ~1.2 MiB of group clones
   per 100-row leaf. Batch the distinct values into one call.
2. `encoding/delta/read.rs:117–138` builds a fresh `PoolReader` per pooled-leaf resolution,
   discarding the wave's pack and group caches (`cas/read.rs:69`); `cas/owner.rs:697` does
   the same although the owner already holds `self.pool_reader` (`:216`).
3. `encoding/delta/select.rs:217,241,257,283,296,309` re-hash the whole canonical object
   with `ObjectId::for_bytes` although `object.id()` is in hand (`cas/owner.rs:404–410`).
4. `file/edit/apply.rs:215–221` re-reads and re-decodes the file state (`tree.rs:886–901`)
   although `FileView` already holds it (`file/view.rs:22–24,62–67`, `Copy`). **This one is
   inherited from v0.1.6** (`crates/layerfs-content/src/file/content.rs:252,279`) — do not
   report it as a new win.
5. `pool/value_group.rs:50–64` clones the group body; `cas/owner.rs:444` then `:490`
   decodes the pooled leaf twice; and `cas/store.rs:322–334,362–366` copies and rescans
   pending canonicals.

**Do not** "simplify" by removing the `load_node(last,false)` probes (`tree.rs:699,752` —
they are a non-root partition validation), `compare_replacements` (mandated planned work),
or the retained-tail rewrite (`pack/placement.rs:129–149` — locator stability depends on it),
and do not eagerly publish builder pages (that would emit unreachable objects).

---

## 4. Package C — evidence-integrity corrections

Reports are corrected in place with a dated block naming what they replace; **no receipt is
ever rewritten.** This is the idiom already used in
`evidence/stages-3-4-oracle-20260916T214846Z/README.md` and `w9/README.md` §W9.1.

| # | Where | Problem | A correction must say |
| --- | --- | --- | --- |
| C1 | `evidence/stages-3-4-smoke-20260916T210931Z/README.md:34` | quotes `edit.save` **158.824 ms** | the retained `grow/pipeline-edit-save.json:3` says `"elapsed_ns": 193977292` = 193.977 ms; the old number is unsupported because the run's stdout was not retained. Reported as E-D1 then F-6 — **still uncorrected** |
| C2 | same README, lines 35 and 38–53 | quotes the `shrink2` run under the `shrink` label | `shrink/` = 6 060 625 ns with a 54 762 208 ns readback; `shrink2/` = 5 970 333 ns and 54 928 042 ns |
| C3 | `w7/README.md:82–87` | the `rss before → after` column matches none of the three ledgers in `w7/w7-verify.log` (lines 24–29, 45–50, 86–91) | cite and retain the run that produced them, or mark the column unsourced. The four heap columns match ledger 1 verbatim |
| C4 | `w7/README.md:94` and gate G12 | calls 3 450 007 B "3.45 **MiB**" | it is 3.45 MB / 3.29 MiB |
| C5 | `w7/README.md:77–78` and G12 | calls a 24-leaf, 2 400-value fixture "the **E1b** shape" | E1b is 128 leaves with 227 distinct values (`e1b-pooled-128/stdout.log`); E1a is the 24-leaf arm |
| C6 | `w3/README.md:114` | the with-fix frontier vector `[2208,2288,2448,2768,3408]` has no raw receipt | re-run `edit_bounds` with `--nocapture` into a fresh round, or label it unreceipted. The control vector **is** receipted (`w3/w3-fails-without-fix.log:12`) |
| C7 | `stages-3-4-report.md` | two sections numbered 8; its §3 and §8 tables describe the **W1 and W9** snapshots, neither of which is HEAD (`file/edit/tree.rs` is 645 / 805 / **901** physical lines) | reconcile to one snapshot and state which |
| C8 | `stages-3-4-report.md:521–528` | lists eight files "without a plan row", six of which **have** plan rows (`file-plan.md:100,118,119,120,221,232,214–219`) | the only genuine unplanned Stage 3–4 addition is `file/edit/tree.rs` |
| C9 | G18, `report §8.2`, ledger L33 | certify core **10 983** / combined **76 400**, while W10, the issue summaries, the final round and the tip commits say **11 058** / **76 475** | restate to the current tip |
| C10 | `physical-encoding-and-packing.md:498` | says metadata "has 16 edges and 128-KiB canonical closure" | source is `METADATA_CHAIN_CANONICAL_LIMIT = 65 536`, `METADATA_CHAIN_ENCODED_LIMIT = 139 281` (`storage/policy.rs:126,128`) |
| C11 | `report §3` (counter defects) | attributes "956 / 4 083" to the earlier review | that review measured **1 027 / 3 737** |
| C12 | `closeout-report.md §2.1` | tabulates 10 findings but says "Eleven findings" and names F9; **F3 has no row**; F10 (S2) is unresolved | fix the count and add the missing rows, or state which are out of the table's scope |
| C13 | `stages-3-4-evidence-closeout-prompt.md` §A2 items 8–9 | already reconciled in `verification.md` §8 and `closeout-report.md` §5 | verify and close those two rows |

**Also required: per-commit LOC disclosure (F-20).** All 31 commits in
`c38961f2f..b29f8e4a3` carry exactly one `Production LOC:` line, and the core-only figures
recount correctly — but the **scope is unstable**: 7 commits report combined, 9 report
core-only *with* a `Scope:` note, and **15 report core-only with neither `Scope:` nor
`Method:`**. The counter correction in `6566a95a3` also moved the reference subtotal from
68 476 to 65 417, leaving earlier combined figures stale by 3 059. Going forward, every
commit message states scope, method and both subtotals, or reports only the delta.

---

## 5. Package D — harness prerequisites (before any campaign)

* **D1 — Telemetry clipping.** A clipped tree must be a **hard failure** for a measured row.
  Either raise/stream the 1 024-node budget or cap arms so they cannot clip.
* **D2 — Budget metric.** `wall_seconds` is printed by no product tool
  (`grep -rn 'wall_seconds' core/crates/` is empty), so the ≤15 s per-command rule rests on
  an unrecorded wrapper. Print it in the tool, or retain and document the wrapper.
* **D3 — Worker identity.** `stages-3-4-verification.md:23` freezes "every run exports
  `LAYERFS_CONSTRUCTION_WORKERS=1`". No timing-round or matched-C1 `command.txt` contains
  it, and `grep -rn 'env::var' core/crates/*/src/` is empty. Export and record it, or
  delete the line by owner decision. An unverifiable frozen identity is worse than none.
* **D4 — The C2 lane ignores `--case`.** `examples/measure_edits.rs:390–450` truncates the
  fixture to `whole_file_raw_limit` and applies its own fixed patch, so four of five
  `e2-c2-*` arms wrote a **byte-identical** `store.sqlite` (`b59c7b24d91ad7d4…`, 294 912 B)
  and all five print the same readback. Make `--case` drive the stored workload, or rename
  the mode so it cannot be read as a per-case comparison.
* **D5 — Seal the matched pair.** The timing round's `tool-identities.txt` names sha256
  values that match nothing on disk (the examples were rebuilt; `measure_pooled` is now
  `7214908d5483…`), so that round is unreproducible from the tip. Archive the matched-pair
  binaries by sha256 (the `binary-archive/<sha256>/` pattern already used by
  `benchmark/fs-bench-pro`) and record the archive path in the ledger **before the next
  rebuild**, or the C1 pair will suffer the same fate.
* **D6 — Control logs.** Only 6 of the 10 closeout packets (W1–W6) retain a
  `fails-without-fix` control; **W7–W10 retain none**, although W7 backs the memory gate and
  W10 the limits gate. Add the missing control for any oracle whose margin the batch intends
  to rely on, or label those rows source-derived.

---

## 6. Owner decisions (blocking, not technical)

* **E1 — the matched campaign.** *Yes* → execute §7. *No* → a written re-scope naming #171
  as the carrying issue and restating that Stage 3–4 latency, storage and memory stay
  **unqualified**, so the closed state is never read as a measured comparison. The existing
  waiver covers **G13/G15 only**.
* **E2 — the two format/design deviations** (pooled leaf records staying in the v1 ordinary
  lane, distinguished by `objects.object_role`; v5 refused by scope), recorded in
  `physical-encoding-and-packing.md` lines 270–284.
* **E3 — #169 acceptance item 2 (blocking).** #168 and #169 are **already closed**
  (GitHub `updatedAt` 2026-09-17T02:15Z) on a waiver that does not mention read
  amplification. It therefore needs **either §A1's case or its own written waiver** recorded
  in `stages-3-4-closeout-report.md`. Leaving it unstated while the issue is closed is the
  one outcome this prompt exists to prevent.

---

## 7. The campaign, only if E1 = go

1. **Commit the versioned addendum first** (`verification.md:155–161`): cases and exact
   inputs/seeds; both arms' identities (commit, compilation seal, sha256 **and** the retained
   archive path from D5); cache state plus the contract that enforces it, with cold and warm
   rows never pooled; **n ≥ 5 per arm with every sample reported**; limits; the
   acknowledgement boundary; budgets. No sample before it is committed.
2. **Align the byte-accounting boundary.** Neither current arm opens a Store, and the two
   sides compute different quantities — the reference prints a content-deduplicated store-map
   delta and its batch's `nodes_created`, the candidate prints emission counts for both
   fields. One defined quantity, computed the same way on both sides, stated in the addendum.
3. **Build the reference-side memory instrument.** Today only the candidate has one
   (`examples/memory_ledger.rs`, one sample, debug, and it counts only the Rust global
   allocator — not libzstd's or libsqlite3's heaps). A memory comparison without a reference
   arm is not a comparison.
4. **Re-run `examples/memory_ledger.rs` at the final tip** and state the exclusions. The
   only receipt on disk is from `aa4b5a9e4`, not HEAD, so **no heap or RSS figure currently
   certifies this tree**.
5. **Report honestly:** every sample, no best-of, `INELIGIBLE` and clipped rows beside the
   gate row, no lifetime cgroup figure as a phase number, no heap figure inferred from a
   file size.

---

## 8. Closure definition

| Item | Closes when |
| --- | --- |
| **#169 item 2** / registry row "read amplification on the representation transition" | §A1's case passes with a raw receipt, **or** a written waiver names this row |
| **#168 item 6** | the clipped arm is re-collected inside budget, or item 6 is recorded as met for unclipped arms with the clipping stated beside it |
| Package B (B1–B9) | each has a test, or a recorded decision with its derived bound and reason |
| Package C (C1–C13) | every correction applied in the report that carries the number, dated and attributed; receipts untouched |
| Package D (D1–D6) | fixed with their own receipts, or an owner waiver naming each |
| §6 E1/E2/E3 | answered in writing in `stages-3-4-closeout-report.md` |
| **Stages 3–4 as a whole** | §2 and §3 done; §4 corrections applied; §5 landed or waived; §6 answered |
| G13 / G15 | already closed by the written waiver; stay **unmeasured** — do not re-open, do not upgrade |
| #165, #170, #171, #172 | untouched by this work; #170/#171/#172 stay open |

---

## 9. Deliverables and reporting

1. §A1 case + control + one fresh evidence directory, with the registry row updated.
2. Package B fixes with tests, or recorded decisions with derived bounds.
3. Package C corrections applied in the reports that quote the numbers.
4. Package D fixes with receipts, or waivers naming each.
5. §6 answered in writing.
6. If E1 = go: the committed addendum, the sealed pair, the aligned boundary, the reference
   arm, and the campaign receipts.
7. A fresh additive report beside the review, with its own evidence directory.

For every run report: the exact command, exit code, whole-command wall time, the tool
identity (commit + sha256 + archive path), the raw log path, and **every** non-passing line.
State plainly which of §2–§6 was not done and why. A run limit is not a technical blocker
and does not change these criteria.
