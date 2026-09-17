# Stage-5 terminal verification — R2-F19, R2-F20, R2-F22, R2-F26

Reviewer: independent verifier, 2026-09-18. Scope: the four round-3 rows as
implemented by commit `2fe2a4642989920c25c80a8435e367243abb4a94`
("fix(core): bound the read wave, right-size the value limit, validate policies"),
verified against the frozen tree. No row is passed on the strength of a report,
receipt or commit message: every claim below was re-read in source and, where a
test exists, the test was run by this reviewer; where the shipped test did not
cover the claim, an independent reproduction was built outside the tree (under
`/tmp`) and run against both the frozen tree and the pre-fix parent tree.

## 0. Repository state and one identity discrepancy

- `git rev-parse HEAD` → `99743b2cff2470e6634874d7ee14b9d37d0ba16e`
  ("docs(stage5): retain the round-4 check logs on the round tree"), branch
  `main` — exit 0.
- The verification request's literal hash
  `99743b2cf3a869b7d8897a1f16b82d742aeedc40` **does not exist as an object**
  (`git cat-file -t …` → exit 128). It shares its first 8 hex digits
  (`99743b2c`) with HEAD; every other part differs. The verified tree is HEAD,
  which the round-3 commit `2fe2a4642` is an ancestor of
  (`git merge-base --is-ancestor 2fe2a4642… HEAD` → exit 0).
- Working tree was clean at session start (`git status --porcelain` → empty,
  exit 0). During the session, **sibling verifier agents** working in the same
  workspace left: modified `core/crates/layerfs-storage/src/encoding/codec.rs`
  (comment-only), modified
  `docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md` (+10 lines
  inserted around L188) and `physical-encoding-and-packing.md`, plus their own
  `verify-*.md` files in this evidence directory. None of those edits touch any
  file cited below (checked with `git diff --stat`); line numbers for
  `stage-5-report.md` are taken from the committed blob
  (`git show HEAD:…`) so the sibling insertion does not shift them. The
  full-workspace test run (command 8) compiled the sibling's comment-only
  codec.rs edit; the two targeted test runs (commands 6–7) build only
  `layerfs-content` and its dependencies and are unaffected.
- This reviewer created no repository file other than this one; all scratch
  work lives under `/tmp` (`/tmp/f20repro`, `/tmp/prefix-tree`).

## 1. Verdicts

| Row | Verdict | Basis |
| --- | --- | --- |
| R2-F19 | **PASS** (with one wording caveat, §2.4) | false single-allocation claim deleted; comment now states the two allocations the code performs and the round-2 ledger row records |
| R2-F20 | **PASS** (with one coverage caveat, §3.4) | chunked route demands the base root exactly once — independently reproduced on a chunked base at HEAD and shown to be 2 on the pre-fix parent tree |
| R2-F22 | **PASS** | `into_parts` returns `ObjectParts` carrying predecessors; test run by this reviewer passes; production save path consumes the advisory |
| R2-F26 | **PASS** | one constant owns the figure, default derived from it, sorted-page constant names it; no third restatement in production source |

## 2. R2-F19 — whole-file encoder comment vs the two-allocation reality

**PASS.**

### 2.1 The comment (current tree)

`core/crates/layerfs-content/src/file/content.rs:76-81` (doc comment on
`encode_whole_file`):

> Encodes the canonical object of a whole-file payload.
>
> The value is assembled in its own allocation and the canonical envelope is a
> second one, so an accepted payload of `n` bytes peaks at roughly `2n` plus the
> framing, as `content-io.md`'s memory ledger records. The single-allocation cut
> is **not** implemented here; this comment says what the code does.

and the inline note at `content.rs:86-90`:

> The cut this comment used to claim is not implemented: the value is built
> in its own allocation and `encode_bytes_object` allocates the canonical
> object around it, so the peak is roughly twice the value plus its framing.
> The memory ledger states that figure; the comment no longer claims
> otherwise.

### 2.2 The code does exactly what the comment says (two allocations)

- `content.rs:98` — `let mut value = Vec::with_capacity(WHOLE_VALUE_HEADER + bytes.len());`
  (value buffer, ~n+10 bytes; header `WHOLE_VALUE_HEADER` covers magic+version).
- `content.rs:102` — `let canonical = encode_bytes_object(&value)?;`
- `core/crates/layerfs-content/src/object/codec.rs:70-75` —
  `encode_bytes_object` allocates a second buffer:
  `let total = canonical_len(value.len())?; let mut canonical = Vec::with_capacity(total); …`
  (canonical envelope, ~n+23 bytes of framing per `policy.rs:196`
  `WHOLE_FILE_CANONICAL_OVERHEAD: usize = 23`).

So the construction peak is value (n+10) + canonical (n+23) ≈ **2n+33** — the
comment's "roughly `2n` plus the framing" is accurate.

### 2.3 The pre-fix comment claimed the opposite

`git show 5e2a20a0c:core/crates/layerfs-content/src/file/content.rs` lines
78-79 (parent of the fix commit):

> The value is written into its final canonical allocation once; there is no
> inner allocation that the outer object then copies.

`git show 2fe2a4642 -- core/crates/layerfs-content/src/file/content.rs` shows
exactly this text replaced by the current two-allocation statement — the false
claim is gone.

### 2.4 The ledger rows agree; one wording caveat

- Round-2 review's finding, `stages-1-5-review-20260917T230700Z.md:494`
  (committed blob): "…the value is allocated and then `encode_bytes_object`
  allocates a second buffer: **peak ~2n+33 for construction and ~3n for a
  whole-file edit**…".
- Round-2 review's replacement-core memory-ledger row, same file, line 1226:
  "whole-file construction value + canonical buffer | object field ceiling
  (`MAX_OBJECT_FIELD_BYTES = 8 MiB`) | **2 simultaneous buffers** | … | F19".
- `stage-5-report.md:593` (committed; §14 round-3 row): "the whole-file
  encoder's comment states the two allocations the memory ledger records".
- Stage-5-report §5's ledger (committed lines 356-370) holds the sorted-merge
  scratch owners only — it has no whole-file row; the F19 ledger row lives in
  the round-2 review (above), and it agrees with the comment (2 simultaneous
  buffers = value + canonical).

**Caveat (does not flip the verdict):** the comment states the two allocations
and the ~2n+framing **construction** peak; the **~3n whole-file-edit** figure is
recorded in the round-2 review (line 494) but is not restated in any code
comment (grep for "3n" over `core/crates/*/src` → no match). The claim's core —
no false single-allocation claim, two allocations stated, matching the code and
the ledger's 2-simultaneous-buffers row — is fully verified. The comment's own
citation "as `content-io.md`'s memory ledger records" is loose: `content-io.md`
§5 routes to `content-io-memory-audit.md` §3, whose whole-file row describes the
reference tree and whose replacement-core note routes onward; the concrete
2-buffer row is the round-2 review's. Minor citation imprecision, not a factual
error.

## 3. R2-F20 — chunked edit route reads/decodes the base root once

**PASS** for the code change (independently reproduced); the shipped test's
*coverage* is weaker than its doc comment implies (§3.4).

### 3.1 The route is handed the view's decoded state

`core/crates/layerfs-content/src/file/edit/apply.rs:111-117`:

> Representation::Chunked => match view.file_state()? {
>     // The view already acquired and decoded the base root, so the
>     // chunked route is handed that decoded state instead of reading
>     // and decoding the same object a second time.
>     Some(state) => replace_chunked(capacities, state, reader, &request, consumer, edit),
>     None => stream_combined(capacities, &view, &request, consumer, edit),

`replace_chunked` (apply.rs:213-215) now takes `state: crate::file::mapping::FileState`
by value and derives its `NodeSummary` from it (apply.rs:221-228, comment: "One
read of the base root per edit, not two: the state the view decoded is the state
this route starts from, and the summary is derived from it.").

The view supplies the decoded state without any re-read —
`core/crates/layerfs-content/src/file/view.rs:27-44` (`open` performs the one
`reader.read_canonical(root)` at :36 and `content::classify(&canonical)` at :37)
and `view.rs:62-67` (`file_state` returns the already-decoded
`FileContent::Chunked(state)`; no provider call).

Pre-fix parent tree (`git show 5e2a20a0c:…/edit/apply.rs`):
`replace_chunked(view, …)` began with
`let (state, mut summary) = edit.child("edit.base_read").run(|_| crate::file::edit::tree::read_state(reader, view.root()))?;`
— the second read+decode. The round-3 diff deletes it.

### 3.2 The counting-provider test exists and its assertion

`core/crates/layerfs-content/tests/edit_single.rs:292-362`
(`an_edit_reads_and_decodes_the_base_root_once`, added by `2fe2a4642`), with a
`Counting` provider recording every demanded id (:302-314), and the assertion at
:347-351:

> assert_eq!(
>     root_demands, 1,
>     "the base root is demanded exactly once per edit, not once for the view and \
>      once for its file state: {demands:?}"
> );

plus the stronger :356-359 ("no identity is demanded twice in one edit").
Run by this reviewer — see command 6: **10 passed / 0 failed**, including this
test. The round-3 receipt `edit-single-read.log` shows the same single-test run;
the reviewer did not rely on it.

### 3.3 Independent reproduction (the shipped test does not reach the chunked route)

The shipped test's base is `repeat(9_000, 0x51)` (:316) — 9,000 bytes, below the
frozen cutoff `DEFAULT_SMALL_FILE_THRESHOLD_BYTES = 131_072`
(`core/crates/layerfs-content/src/policy.rs:14`; `representation()` at
policy.rs:120-128 sends anything below the cutoff to `WholeFile`). So the
shipped test exercises `Representation::WholeFile` — not the chunked route its
doc comment describes. The reviewer therefore built an external reproduction
(`/tmp/f20repro`, a standalone crate path-depending on `layerfs-content` and
`layerfs-telemetry`; no repository file touched) implementing the same counting
technique over:

1. a **chunked** base/result — 262,144 bytes of xorshift noise, edit
   `Edit::new(100_000, 100_500, 500)` (final length 262,144 ≥ cutoff → chunked
   route), and
2. an exact mirror of the shipped test's whole-file scenario
   (`repeat(9_000, 0x51)`, `Edit::new(4_000, 4_032, 32)`).

Results (binary output, run twice per tree):

| Tree | Chunked edit: base-root demands | Whole-file scenario: base-root demands |
| --- | --- | --- |
| HEAD `99743b2cf` (frozen) | **1** | 1 |
| Pre-fix parent `5e2a20a0c` (via `git archive`, built under `/tmp`) | **2** | 1 |

This simultaneously proves (a) the fix is real — the chunked route demanded the
root twice before `2fe2a4642` and exactly once at HEAD — and (b) the shipped
test would have passed on the pre-fix tree too, because its 9,000-byte scenario
never reaches `replace_chunked`. (On HEAD's chunked run, one non-root identity —
an `ExtentLeaf` mapping page — is demanded twice; that is outside F20's claim,
which is only about the base root, but it explains why the shipped test's
stronger "no identity twice" assertion could not simply have been pointed at a
chunked base.)

### 3.4 Caveat

The claim's clause "a counting-provider test asserts the base root is demanded
exactly once per edit" is literally true (the test exists, asserts exactly that,
and passes), but **the test's scenario exercises the whole-file route**, so the
round's own receipt proves the fix only indirectly. The chunked-route behavior
itself was verified by this reviewer's independent reproduction (§3.3), so the
row passes on reproduced evidence, not on the shipped test.

## 4. R2-F22 — `into_parts` returns `ObjectParts` carrying the predecessors

**PASS.**

### 4.1 Source

`core/crates/layerfs-content/src/object/output.rs:153-168`:

> Moves the owned pieces to a consumer that stores or forwards them.
>
> The pieces are the identity, the role, the canonical bytes, the direct
> references **and the advisory predecessors**. A predecessor is an input the
> production save path consumes to choose a physical representation, so a
> consumer that takes ownership through this call has to receive it: a form
> that dropped it silently discarded an input the object was built with.
> pub fn into_parts(self) -> ObjectParts {
>     ObjectParts { id: …, role: …, canonical: …, references: …, predecessors: self.predecessors }

`ObjectParts` (output.rs:171-185) carries
`pub predecessors: AdvisoryPredecessors` (:183-184). Pre-fix
(`git show 5e2a20a0c:…/object/output.rs:154-156`): `into_parts` returned
`(self.id, self.role, self.canonical, self.references)` — the predecessors field
was dropped, exactly as the claim states.

### 4.2 Production save path consumes the advisory

`core/crates/layerfs-storage/src/cas/save.rs:69-72` (inside `flush_batch`'s
`None =>` arm for a new identity):

> let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
> owner.offer(object, &advisory, &mut availability)?

`owner.offer` (`core/crates/layerfs-storage/src/cas/owner.rs:348-360`) passes the
advisory into `self.select_record(object, advisory)?` — the physical-record
choice. (The save path holds the object and reads `predecessors()`;
`into_parts`/`ObjectParts` is the ownership-taking form for consumers that move
the pieces — e.g. `tests/support/mod.rs:87` — and now carries the same advisory
input instead of discarding it.)

### 4.3 Test

`core/crates/layerfs-content/tests/object_identity.rs:381-417`
(`moving_an_objects_pieces_keeps_its_advisory_predecessors`): builds a
whole-file object, attaches
`AdvisoryPredecessors::explicit(base)` (:396-398), moves via `into_parts()`
(:399), and asserts at :404-408:

> assert_eq!(
>     parts.predecessors.ids().collect::<Vec<_>>(),
>     vec![base],
>     "the moved pieces still carry the predecessor"
> );

plus the bound and entry/provenance checks (:409-416). Run by this reviewer —
command 7: **1 passed / 0 failed**. (Against the pre-fix tuple form this test
would not even compile — `parts.predecessors` did not exist — so it is both a
compile-level proof of the new shape and a runtime proof the predecessor
survives the move.)

## 5. R2-F26 — one scratch ceiling, one name

**PASS.**

### 5.1 The owner of the figure

`core/crates/layerfs-content/src/filesystem/limits.rs:29-36`:

> Largest bytes one filesystem operation may hold for its own unfinished pages.
>
> One name, one figure, and the default below derived from it. Two constants used
> to encode this same nominal ceiling - one as `4 MiB` and one as `4 MiB - 1` -
> so a reader could not tell which of the two the enforcement used.
> pub const MAXIMUM_OPERATION_SCRATCH_BYTES: usize = 4 * 1024 * 1024;
> /// Default bytes one filesystem operation may hold for its own unfinished pages.
> pub const DEFAULT_OPERATION_SCRATCH_BYTES: usize = MAXIMUM_OPERATION_SCRATCH_BYTES - 1;

### 5.2 The sorted-page constant names it

`core/crates/layerfs-content/src/filesystem/sorted/page.rs:22-26`:

> Largest work one operation may hold for its own unfinished pages.
>
> The figure is the one ceiling the filesystem profile declares, not a second
> copy of it: `limits` owns the number and every enforcement site names it.
> pub const MAXIMUM_SCRATCH_BYTES: usize = crate::filesystem::limits::MAXIMUM_OPERATION_SCRATCH_BYTES;

### 5.3 No second encoding of the figure; enforcement sites name it

- Pre-fix parent tree: `git show 5e2a20a0c:…/filesystem/limits.rs` had
  `DEFAULT_OPERATION_SCRATCH_BYTES: usize = 4 * 1024 * 1024 - 1;` (independent
  figure) and `page.rs:23` had
  `MAXIMUM_SCRATCH_BYTES: usize = 4 * 1024 * 1024;` (independent figure) — the
  two-constant problem the round-2 review row
  (`stages-1-5-review-20260917T230700Z.md:501`, committed) names.
- Exhaustive grep of `core/` (excluding `core/target`) for
  `4 * 1024 * 1024 | 4_194_304 | 4_194_303 | 4194304 | 4194303`: in production
  `src/` the only hits are `limits.rs:34` (the owner), plus three **different
  quantities** — `layerfs-storage/src/policy.rs:71`
  `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 * 1024 * 1024 - 1` (canonical bytes per
  open transaction, a storage capacity, asserted as such in
  `layerfs-storage/tests/storage_limits.rs:142-144`),
  `policy.rs:113` `DEPENDENCY_PACK_CACHE_BYTES = 4 MiB` (pack cache), and
  `references/runs.rs:39` `DEFAULT_ORDERING_BYTES = 64 MiB`. None of these
  encodes the operation-scratch ceiling; no third constant restates it.
- Enforcement sites all name the constants (no inline figure):
  `directory/update.rs:41`, `inode/update.rs:71` and `:84`,
  `input.rs:73` (all `MAXIMUM_SCRATCH_BYTES`), `sorted/budget.rs:34`
  (`DEFAULT_OPERATION_SCRATCH_BYTES`).
- `stage-5-report.md:597` (committed; §14 row): "one scratch ceiling, one name
  (`MAXIMUM_OPERATION_SCRATCH_BYTES`) | source".

## 6. Commands run (all from the repository root unless noted)

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` |
| 2 | `git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` | 128 | object does not exist (see §0) |
| 3 | `git status --porcelain` (session start) | 0 | empty (clean); later sibling edits listed in §0 |
| 4 | `git log --oneline -3` / `git log --format=… -1 2fe2a4642…` | 0 | fix commit confirmed on main |
| 5 | `git merge-base --is-ancestor 2fe2a4642… HEAD` | 0 | yes |
| 6 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_single` | 0 | 10 passed / 0 failed (incl. `an_edit_reads_and_decodes_the_base_root_once`) |
| 7 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test object_identity moving_an_objects` | 0 | 1 passed / 0 failed |
| 8 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` (full workspace, background) | 0 | every suite ok, no failures (compiled the sibling's comment-only codec.rs edit — §0) |
| 9 | `/tmp/f20repro` `cargo run` against HEAD path-deps | 0 | chunked base-root demands **1**; whole-file scenario 1 (§3.3) |
| 10 | `git archive 5e2a20a0c \| tar -x -C /tmp/prefix-tree` then `/tmp/f20repro` `cargo run` against the extracted pre-fix tree | 0 (pipeline) | chunked base-root demands **2**; whole-file scenario 1 (§3.3); binary verdict line "…exactly once: false" |
| 11 | `git show 2fe2a4642 -- <apply.rs, content.rs, limits.rs, page.rs, edit_single.rs>` | 0 | round-3 diffs quoted in §2.3, §3.1, §5.3 |
| 12 | `git show 5e2a20a0c:<content.rs, output.rs, apply.rs, limits.rs, page.rs>` | 0 | pre-fix sources quoted in §2.3, §3.1, §4.1, §5.3 |
| 13 | greps (`MAXIMUM_OPERATION_SCRATCH_BYTES`, `into_parts`, counting providers, `4 * 1024 * 1024` family, `2n`/`3n`) over `core/` and `docs/roadmap/0.1/0.1.7/` | 0 | hits quoted inline in §2–§5 |

## 7. UNVERIFIED / out of scope

- **Measured allocation peaks.** The ~2n+33 / ~3n figures were verified as
  *source-structure* claims (which buffers exist, their sizes), not by
  instrumenting allocations; no allocator counter was run. The ~3n whole-file
  edit figure (raw + value + canonical in `apply.rs`'s WholeFile arm) is
  consistent with the code (`assemble_final` allocates the result at
  apply.rs:143-150, then `encode_whole_file` adds the two encoder buffers) but
  was not measured either.
- **The ~3n figure in code comments.** It exists only in the round-2 review
  text; no code comment states it (§2.4 caveat).
- **content-io.md's own ledger row.** Whether `content-io.md`/`content-io-memory-audit.md`
  itself (as opposed to the round-2 review's replacement-core ledger) carries a
  two-buffer whole-file row for the replacement core: the audit's §3 note routes
  onward without one; the concrete row is the round-2 review's (line 1226).
- **The shipped F20 test's chunked-route coverage.** Not covered by the test
  itself (whole-file scenario only); verified independently instead (§3.3–3.4).
- **Non-root double demands.** One `ExtentLeaf` mapping page is demanded twice
  in a chunked edit at HEAD (§3.3) — outside all four claims; not investigated
  further.
- **CI/preflight, boundary guard, clippy, fmt, LOC tooling** — not part of these
  four rows and not run (the full test suite, commands 6-8, was run; a passing
  suite alone is not the evidence for any row above).

## 8. Result

All four rows **PASS** on reproduced evidence: R2-F19 (source + diff), R2-F20
(source + independent two-tree counting reproduction; shipped test verified but
its scenario noted as whole-file-route), R2-F22 (source + test run + pre-fix
drop shown), R2-F26 (source + exhaustive grep + pre-fix two-constant state
shown). One repo-identity discrepancy (§0) and two coverage caveats (§2.4,
§3.4) are recorded above; neither flips a verdict.
