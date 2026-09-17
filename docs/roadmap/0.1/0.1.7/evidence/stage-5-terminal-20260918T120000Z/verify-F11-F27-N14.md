# Verification: round-4 documentation remedies R2-F11 / N-8 / F27 and N-14

> Verifier: independent verification subagent (stage-5 terminal pass).
> Method: read the code first, then check every statement of each dated note
> against it; falsify by grepping the component-decoupling docs for competing
> wording; reproduce the load-bearing boundaries behaviorally. No tracked file
> was modified, staged, committed or reverted; the only repository write is
> this file. Probe artifacts live under `/tmp/walk-probe/`.

## Verdicts

| Row | Verdict |
| --- | --- |
| R2-F11 / N-8 (+ the F27 note) — transaction and batch bounds stated as commit/flush triggers | **PASS** |
| N-14 — caller-owned input bound declared as an explicit adapter obligation | **PASS**, with two recorded imprecisions inside the declaration (§N-14 findings 6-7): the whole-tree walk ceiling is attributed to `FilesystemResources::check` though it is a fixed constant, and the inline "refused above 4,095 bindings" figure is off by one (code refuses above 4,096). Neither affects the row's substance. |

## Tree identity — discrepancy against the task's claimed commit

- Claimed frozen commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40` is **not present**
  in this repository's object database: `git cat-file -t <hash>` → exit 128
  ("could not get object info"); `git branch -a --contains <hash>` → exit 129
  ("no such commit"). `git rev-parse <hash>` echoes the hash (exit 0) without an
  existence check — not evidence of existence.
- Actual HEAD: `99743b2cff2470e6634874d7ee14b9d37d0ba16e`
  ("docs(stage5): retain the round-4 check logs on the round tree"). It shares only
  the 9-character prefix `99743b2cf` with the claimed hash. Working tree clean
  (`git status --porcelain` empty), so the verified tree equals HEAD.
- Everything below was verified on actual HEAD. If the intended frozen commit was
  a different tree, this file's findings apply to the tree present, not to it.

## Concurrent workspace modifications (read this before checking citations)

The working tree was clean at session start (`git status --porcelain` empty). During
this verification, **sibling verification agents sharing the workspace modified
tracked files** (the tree is no longer "frozen" in the working copy):

- `core/crates/layerfs-content/src/filesystem/limits.rs` (scratch-constant and
  walk-ceiling doc edits; `MAXIMUM_WALK_ENTRIES` value unchanged at 4,096),
- `core/crates/layerfs-content/src/filesystem/sorted/budget.rs`,
- `core/crates/layerfs-content/tests/filesystem_limits.rs`,
- `core/crates/layerfs-storage/src/encoding/codec.rs`,
- `core/crates/layerfs-storage/tests/storage_limits.rs` (a new test appended after
  :143; the test this file cites at :128-145 is unchanged in content),
- `docs/roadmap/0.1/0.1.7/component-decoupling/filesystem-tree.md`,
- `…/physical-encoding-and-packing.md`, `…/stage-5-completion-report-20260917.md`,
  `…/stage-5-report.md`.

Two of these touch rows verified here: a sibling changed filesystem-tree.md's
declaration from "refused above **4,095** bindings" to "refused above **4,096**
bindings", and rewrote limits.rs's walk-ceiling comment to state "a build that
states exactly 4,096 is accepted, 4,097 is the first refusal" — the same boundary
this verification's probe found independently (see N-14 finding 5). Those edits
are **uncommitted working-tree changes, not what round 4 landed**.

**All path:line citations in this file refer to the committed tree at HEAD
(`99743b2cff…`)**, cross-checked with `git show HEAD:<path>` / `git diff --quiet`
where the working tree diverges. Files cited here that remain byte-identical to
HEAD: `admission-and-persistence.md`, `owner.rs`, `batch.rs`, `policy.rs`,
`store.rs` (unmodified throughout), `input.rs`, `validate.rs`,
`tests/policy_capacity.rs`, `tests/filesystem_bounds.rs`,
`stages-1-5-review-20260917T230700Z.md`, evidence `README.md`. Citations against
files the siblings moved: `filesystem-tree.md:457-470` (committed; the sibling's
edit touches :463), `limits.rs:54-72` (committed), `stage-5-report.md:420, 539,
659-660` (committed; the same rows sit at :420, :542, :672-673 in the working
tree). The two targeted test receipts ran on files byte-identical to HEAD; the
full-suite run and the probe compiled the working tree as it stood, whose
walk-relevant code (`validate.rs`, `MAXIMUM_WALK_ENTRIES`) is semantically
identical to HEAD.

## Commands run (all read-only on the repository; exit codes in brackets)

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD` / `git status --porcelain` / `git log --oneline -1` | 0 / 0 / 0 |
| `git cat-file -t 99743b2cf3a86…` | 128 (object absent) |
| `git rev-parse 99743b2cf3a86…` | 0 (echoes input; no existence check) |
| `git branch -a --contains 99743b2cf3a86…` | 129 (no such commit) |
| `grep`/`sed`/`read` over `core/crates/layerfs-storage/src/{cas/owner.rs,cas/batch.rs,cas/store.rs,policy.rs}`, `core/crates/layerfs-content/src/filesystem/{input.rs,limits.rs,validate.rs,update.rs}`, the component-decoupling docs and the round-2 review | 0 (hits) / 1 (no hits, where noted) |
| `git show 134b8df73 --stat` | 0 |
| `git diff 134b8df73..HEAD -- <the two remedied docs>` | 0, empty diff (notes unchanged since round 4) |
| `git log -S "Correction 2026-09-18 (round-2 findings F11 and F27)" -- …admission-and-persistence.md` and `-S "Declared 2026-09-18 (round-2 finding N-14)" -- …filesystem-tree.md` | 0 (both notes added by `134b8df73`) |
| `git diff f288d2af7 HEAD -- core/crates/layerfs-content/src/filesystem/validate.rs` | 0 (19 lines: only the `MAXIMUM_CYCLE_CHECK_ENTRIES` derivation changed; walk logic unchanged since the reviewed snapshot) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -p layerfs-content` | 0 (all suites pass) |
| `cargo +1.85.1 test … -p layerfs-storage --test policy_capacity` | 0 (9 passed) |
| `cargo +1.85.1 test … -p layerfs-content --test filesystem_bounds` | 0 (10 passed) |
| `cargo +1.85.1 run --manifest-path /tmp/walk-probe/Cargo.toml --offline` (public-API boundary probe, mirrors `filesystem_bounds.rs`'s wide build) | 0 |

## Row 1 — R2-F11 / N-8 (+ F27): PASS

### The note exists where the claim says

`docs/roadmap/0.1/0.1.7/component-decoupling/admission-and-persistence.md`:
heading "Reference bounds to preserve and reconcile" at :336, table at :338-346
(SQL transaction row at :344), sources at :348-356, and the dated note at
:358-371 — "*Correction 2026-09-18 (round-2 findings F11 and F27): two of these
rows are triggers, not caps, in the replacement core.*" — immediately after the
table and before §6 at :373. Round-4 claim rows: `stage-5-report.md:659` and
evidence `README.md:28`.

### Code: accumulation first, `maybe_commit` after (the note's core semantics)

`core/crates/layerfs-storage/src/cas/owner.rs` — three `transaction.bytes +=`
accumulation sites at HEAD (the round-2 F11 finding and the task brief name two;
the third, value-group bodies, behaves identically):

1. `write_value_groups`, :672-673 — `self.transaction.rows += 1;` /
   `self.transaction.bytes += group.body.len() as u64;` → `self.maybe_commit()` at :704, after the write loop.
2. `seal_group` member loop, :823-824 — `self.transaction.rows += 1;` /
   `self.transaction.bytes += member.canonical_length as u64;` → `self.maybe_commit()` at :827, after every member of the group.
3. `write_pack`, :839-840 — `self.transaction.rows += 1;` /
   `self.transaction.bytes += write.bytes.len() as u64;` — `write_pack` itself never
   commits; its callers (:802 → :827, :656 → :704) run `maybe_commit` after it.

`maybe_commit` at :845-862 compares the *accumulated* counters with `>=`:

```rust
if self.transaction_open
    && (self.transaction.rows >= self.capacities.transaction_rows
        || self.transaction.bytes >= self.capacities.transaction_bytes)
{
    write::commit(&self.connection)?;
```

So the crossing write is already inside the open transaction when the figure is
observed; nothing refuses an object for exceeding it. The doc comment at :844
("Commits and lazily restarts the shared write transaction when it is full")
states the same. `SaveOperation::accept` (`cas/store.rs:291-300`) shows the batch
side: `push` returns the drained wave and `flush` runs — a flush trigger.

### Statement-by-statement check of the note (:358-371)

| Note statement | Code check |
| --- | --- |
| "observes the SQL transaction figure *after* the write that crosses it (`cas/owner.rs::maybe_commit` runs after each member/write is accumulated)" | TRUE — order verified above; `maybe_commit` is called only at :704 and :827, always after accumulation. Nuance: it runs after each seal/write *wave* (a whole group's members), not after each single row. |
| "one open transaction can hold up to one maximal object or singleton pack above the figure before the next `COMMIT`" | TRUE in substance — a maximal object (`CANONICAL_LIMIT` = 16 MiB, `policy.rs:61`) or singleton pack (`SINGLETON_PACK_LIMIT` = 16 MiB + 4 KiB, `policy.rs:88-90`) is accumulated in full and only the next `maybe_commit` observes the crossing. Precision: not a strict worst case — one singleton seal charges *both* the pack BLOB (:839-840) and the canonical length (:823-824) before :827, and a group seal charges its whole member list; the round-2 review itself says "roughly one maximal object or pack above the declared figure" (`stages-1-5-review-20260917T230700Z.md:396-397`). |
| "a maximal 16 MiB canonical object exceeds 4 MiB minus 1 by itself and is still accepted, because an object is atomic and cannot be split across transactions" | TRUE — no admission path compares an object against `transaction_bytes`; each object is one row inside the open transaction (`owner.rs:809-820`); `maybe_commit` never refuses on size. 16,777,216 > 4,194,303. |
| "Read the row as **commit trigger: up to 8,191 submitted rows and 4 MiB minus 1 canonical bytes, after the write that crosses it**" | Matches the review's prescribed remedy verbatim (`stages-1-5-review-20260917T230700Z.md:398`: "commit trigger: up to ... after the write that crosses it"). |
| F27: "the byte bound is a **flush trigger**, and one oversized object is deliberately admitted into an empty batch before the flush (`cas/batch.rs`)" | TRUE — `cas/batch.rs:50-53` gates the bound check on `!self.objects.is_empty() && (…)`, so an oversized object entering an *empty* batch is accepted (:59-68) and the drain fires on the *next* push; the in-code doc states it (:43-47: "An object larger than the whole byte bound is still accepted into an empty batch: the declared singleton path must remain usable"). Behaviorally exercised by `policy_capacity.rs:110-131` (`a_larger_incompressible_whole_file_record_uses_the_singleton_lane`, passed: a >512 KiB canonical whole-file object accepted through `accept` into an empty batch). |
| "Both statements match the round-2 review's reading of the code; no bound was weakened to write them." | TRUE — both notes were added by docs-only commit `134b8df73` (`git show --stat`: documentation and evidence files only; `owner.rs`, `batch.rs`, `policy.rs`, `input.rs` untouched by it); both docs unchanged from `134b8df73` to HEAD. |

### Constants the figures trace to (all real, `core/crates/layerfs-storage/src/policy.rs`)

`TRANSACTION_ROW_LIMIT = 8_191` (:69), `TRANSACTION_CANONICAL_BYTES_LIMIT =
4 * 1024 * 1024 - 1` (:71), `BATCH_OBJECT_LIMIT = 512` (:65),
`BATCH_CANONICAL_BYTES_LIMIT = 512 * 1024` (:67), `CANONICAL_LIMIT = 16 MiB`
(:61), `SINGLETON_PACK_LIMIT` (:88-90); wired into `StorageCapacities`
(:324-327) and compared in `maybe_commit` (`owner.rs:847-848`) / used by
`PendingBatch` (`batch.rs:28-29`). The note cites figures, not names; the
round-2 review cites the names (`…review-20260917T230700Z.md:385, 1233-1234`).
Pinned by `tests/storage_limits.rs:128-145`.

### Falsification — any other product doc stating the transaction figure as a hard cap?

Grep of `docs/roadmap/0.1/0.1.7/component-decoupling/*.md` for `8,191|8 191|4 MiB`:
**no current product contract other than the remediated row itself.** Every hit:

| Hit | Classification |
| --- | --- |
| `admission-and-persistence.md:344` (and :342, :364, :366) | Current contract, remedied in place by the attached note (:358-371) that re-reads the row as a commit trigger. |
| `stages-0-2-report.md:67` — "write transaction `8191` rows / `4 MiB - 1` canonical bytes" | Dated record (stage 0-2 implementation report for #166/#167); cap-styled wording predating F11; not retro-edited. |
| `stages-3-4-final-review.md:396` — "≤ 8 191 rows and ≤ 4 MiB − 1 canonical" (also :315) | Dated record (self-review pinned to snapshot `dfd54fd8e`); cap-styled; predates the remedy. |
| `cluster-1-2-source-audit.md:178` — "<=8,191 objects, canonical bytes <4 MiB" | Dated/proposal record ("Status: Proposal; … not a released contract"), an audit of the *reference* source (`a8a1ba848`), describing the reference's own cohort limit. |
| `stages-1-2-review-20260916T185553Z.md:88-89, 923` | Dated review; trigger-correct ("`maybe_commit` fires after 8,191 rows or 4 MiB − 1", "not a total cap"). |
| `stages-3-4-review-20260916T233008Z.md:961` | Dated review; trigger-correct ("checkpoint bound … the row/byte check is applied after the write, so it can overshoot by one group plus one pack"). |
| `stages-3-4-review-20260917T022248Z.md:242, 919` | Dated review; trigger-correct ("limits checked after the write … may overshoot (F-13)"). |
| `stages-1-5-review-20260917T160000Z.md:1376` | Dated review; trigger-correct ("are triggers that re-BEGIN, not hard caps"). |
| `stages-1-5-review-20260917T230700Z.md:385, 393-394, 1097, 1234, 1371` | The round-2 review itself (F11); trigger-correct. |
| Outside the task's grep scope, checked for completeness: `docs/roadmap/0.1/0.1.7/architecture-overview.md:468, 690` ("≤ 8 191 objects / ≤ 4 MiB−1 bytes per SQL cohort") | Dated research record of the *reference* ("Status: Research; informative and not a product contract", source pin `40af8529`, cites `crates/layerfs-layerstack-store`), not a replacement-core contract. Older 0.1.1-0.1.6/`docs/versioned`/`docs/releases` hits are reference-era history. |

## Row 2 — N-14: PASS (two imprecisions recorded)

### The declaration exists where the claim says

`docs/roadmap/0.1/0.1.7/component-decoupling/filesystem-tree.md` §9 ("## 9.
Memory, complexity and independent measurement" at :442), declaration paragraph
at :457-470, marked "*Declared 2026-09-18 (round-2 finding N-14): what the
operation's ceilings cover, and what the caller owns.*" Round-4 claim rows:
`stage-5-report.md:660`, evidence `README.md:27`.

### Statement-by-statement check

1. **"FilesystemResources::check enforces every ceiling the operation itself
   owns - scratch, ordering bytes, pending records, merge buffers, read waves"** —
   TRUE: `core/crates/layerfs-content/src/filesystem/input.rs:97-125` validates
   all five caller-declared fields (`scratch_bytes` ≥ 1,024 at :99-103;
   `maximum_pending_records` ≥ 1 at :104-108; `merge_buffer_bytes` ≥ `ROW_BYTES`
   at :109-113; `base_read_batch` ≥ 1 at :114-118; `ordering_bytes` ≥ `ROW_BYTES`
   at :119-123). The round-2 review's N-14 row (`…review-20260917T230700Z.md:998`)
   and seam row (:1430, citing `input.rs:56-144`) agree.
2. **"… and the whole-tree walk ceiling"** — IMPRECISE ATTRIBUTION: the walk
   ceiling is a fixed constant (`MAXIMUM_WALK_ENTRIES = 4_096`,
   `core/crates/layerfs-content/src/filesystem/limits.rs:72`), enforced during
   validation (`validate.rs:302, 319, 603-604, 663`) — it is *not* a
   `FilesystemResources` field and is not checked by `FilesystemResources::check`.
   The operation does own and enforce it; the sentence's list wrongly implies the
   check covers it. (The review's wording at :998 has the same blur.)
3. **"The operation's *input slices* (`directories`, `inodes`, `new_inodes` in
   `FilesystemInput`) are deliberately borrowed and unbounded in C1"** — TRUE:
   `input.rs:137` `pub directories: &'a [DirectoryUpdate]`, :139
   `pub inodes: &'a [InodeUpdate]`, :141 `pub new_inodes: &'a [u64]` — borrowed
   slices; `FilesystemInput::check` (:157-215) enforces ordering, uniqueness,
   serial ranges and allocation preconditions only, with **no length bound on any
   slice**. The only `.len()` use on them in the crate is a capacity hint
   (`update.rs:298`). Matches review:1430 ("**none declared** — the caller owns
   this bound (N-14)") and review:1377 ("no bound on caller arrays").
4. **"an input of N bindings costs the caller O(N) memory to hold"** — TRUE by
   construction (borrowed slices the caller allocated).
5. **"one build is still refused above 4,095 bindings by the whole-tree walk
   ceiling"** — the bound's existence is TRUE, the figure is OFF BY ONE. Probe
   (public API, `/tmp/walk-probe`, mirroring `filesystem_bounds.rs:743-805`'s wide
   build): 4,094 ACCEPTED / 4,095 ACCEPTED / **4,096 ACCEPTED** / 4,097 REFUSED
   ("invalid record: cycle check work limit"). This matches the code
   (`validate.rs:601-605`: pre-increment, then `visited > MAXIMUM_CYCLE_CHECK_ENTRIES`
   — the 4,097th examined entry refuses) and the round-2 review's own wording
   ("the cycle walk refuses above 4,096 entries", review:1377). The same off-by-one
   appears in the in-code comment `limits.rs:63-66`, `stage-5-report.md:420` and
   the report's §13.2 measurement claim (:539 "4,095 accepted, 4,096 `BUILD
   REFUSED`") — the "4,096 BUILD REFUSED" part does not reproduce on this tree.
   Direction: the doc claims refusal one binding *earlier* than enforced; it
   understates the operation's allowance, it does not overstate a bound.
6. **"a Stage 7 adapter must impose a protocol-level request ceiling, enforced by
   refusal, before native changes reach C1"** — matches the round-2 review's
   seam-table wording VERBATIM (`…review-20260917T230700Z.md:1430`: "a
   protocol-level request ceiling, enforced by refusal") and the review's adapter
   list (:1469: "The adapter also owns the request ceiling C1 does not declare
   (N-14)").
7. **"C1 itself claims no bound on caller-held input, and a caller that hands C1
   an unbounded slice owns the memory it costs."** — TRUE per the code (finding 3).

### Falsification — any current product doc claiming C1 bounds the caller's input?

Grep of the component-decoupling docs for bounded-input wording
(`bounded input|input is bounded|bounds the input|bounds the caller|bounded
request|caller-held|request ceiling`): **no counter-claim.** Every near hit:

| Hit | Classification |
| --- | --- |
| `proposal.md:421` — checklist "Every selected component has an independent entry point, bounded inputs/outputs…" | Unchecked generic criterion in a doc whose status is "Proposal; … not a released contract"; superseded for the C1 filesystem seam by the N-14 declaration. |
| `content-io.md:82` — "Use borrowed bytes for already resident bounded input" | About file-content *source bytes* (a different seam), not `FilesystemInput`. |
| `content-io.md:351, 357` — deployment diagrams "bounded input --> host: C1 -> C2 -> SQLite" | Labels the adapter-level *wire*, i.e. the adapter obligation itself; attributes the bound to the protocol, not to C1. |
| `admission-and-persistence.md:96, 118, 133` — "bounded input" | C2's own pending save batch (bounded by `BATCH_*`), a different component's owned input. |
| `content-io-memory-audit.md:62` | Reference-code audit row ("no unbounded input/result map"), a dated audit requirement. |
| `stages-1-5-review-20260917T230700Z.md:1442, 1606` | The review declaring the unbounded input slices a *declared hatch* and requiring the adapter's "bounded request protocol" — consistent with N-14. |

### Behavioral evidence

`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage
-p layerfs-content` — exit 0, all suites pass, including
`visibility.rs` ("A streamed file large enough to commit bounded transactions
early … the failure arrives after this save has already published packs",
`tests/visibility.rs:183-186`, 4 MiB streamed body), `persistence_failure.rs`
("after at least one bounded transaction was acknowledged", :80-88) and
`filesystem_bounds.rs:743` (`the_cycle_check_work_limit_is_reachable_and_reported`)
and `:819` (`a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound`),
both passed.

## UNVERIFIED

1. **The claimed frozen commit** `99743b2cf3a869b7d8897a1f16b82d742aeedc40` could
   not be inspected: it does not exist in this repository (exit 128/129 above).
   Verification ran against actual HEAD `99743b2cff…`. If the delegator intended
   a different tree, these findings must be re-run there.
2. **A single >4 MiB maximal object crossing the transaction figure in one
   accumulation unit was not executed directly** — no existing test saves one
   16 MiB object (the suite's largest whole-file saves are ~1 MiB canonical via
   the singleton lane and 4 MiB multi-chunk streams that commit early). The
   trigger order is verified statically (quoted lines above) plus the passing
   early-commit tests; the specific "one maximal object above the figure" runtime
   path is code-read evidence, not a reproduced measurement.
3. **The exact worst-case transaction overshoot** ("up to one maximal object or
   singleton pack") is a characterization, not a strict bound (a singleton seal
   charges pack BLOB *and* canonical length; a group seal charges its whole
   member list before `maybe_commit`). No worst-case measurement was constructed.
4. **The round-2 review's snapshot line numbers** (`owner.rs:840-844, :819, :835`)
   have drifted at HEAD (:845-862, :823-824, :839-840) because later code commits
   (`6c00e0f53`, `9327f6695`) touched the file; I verified semantics at HEAD and
   confirmed via `git diff f288d2af7..HEAD` that `validate.rs`'s walk logic is
   unchanged, but did not line-diff `owner.rs` against the reviewed snapshot.
5. **Older docs** (0.1.1-0.1.6, `docs/versioned/`, `docs/releases/`) containing
   the 8,191 / 4 MiB wording were not individually classified; they are
   reference-era historical records outside the task's grep scope.
