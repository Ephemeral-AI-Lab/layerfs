# VF-3 verification — meaningful assertions in the Stage 5 suites

- **Verifier role**: independent reproduction per the terminal-handoff contract. No roadmap
  report, commit message or prior receipt is treated as evidence; every row below was
  re-derived on this tree.
- **Tree**: `git rev-parse HEAD` → `99743b2cff2470e6634874d7ee14b9d37d0ba16e`, working tree
  clean (`git status --short` empty). Note on identity: the full hash named in the task,
  `99743b2cf3a869b7d8897a1f16b82d742aeedc40`, is **not** a git object on this machine
  (`git cat-file -t` → `fatal: could not get object info`, exit 128). It matches HEAD's
  unique short prefix `99743b2cf`, so the verification ran on the intended commit; the
  long form in the task appears mistranscribed after the first 7 hex digits.
- **Claim under test (VF-3)**: the Stage 5 suites carry meaningful assertions — the round-2
  named non-discriminating case (attribute-value bound) is fixed at the real bound, and no
  other Stage 5 case asserts something a wrong implementation would also satisfy.

## Verdict

**PASS** — with two minor named-vs-asserted gaps and two precision notes, all quoted below.
The named case discriminates exactly at 32,768/32,769 with the bound's figure pinned by
independent literal assertions. Every other case I inspected carries at least one assertion
that fails under a plausible wrong implementation of what the case tests (exact
`limit`/`actual` error identities, both-sides boundary probes, independent-oracle root/byte
comparisons, counter-vs-backing cross-checks). The two gaps are cases whose *names* promise
more than their bodies assert; neither is vacuous in the round-2 sense (the round-2 instance
asserted nothing at all about the claimed bound — a 4,096-byte write passes under any
enforcement ≥ 4,097), and the over-promised property of gap 1 is asserted elsewhere in the
same suite. Under the strictest reading of the claim's second clause ("every case's name
must be fully asserted by its own body"), gaps 1–2 would be FAIL instances; under the
round-2 calibration they are not. Both readings are stated here so the owner can rule.

## 1. The named case is fixed and discriminates (confirmed)

`core/crates/layerfs-content/tests/filesystem_failure.rs:546-592`,
`the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary`:

- `:554-560` — `limit = MAXIMUM_ATTRIBUTE_VALUE_BYTES`, asserted `== cdc::MAXIMUM_CHUNK_BYTES`
  ("a value it accepts has a representation and a value it refuses has none").
- `:562-568` — writes exactly `limit` bytes (`vec![0x5a; limit]`) through `emit_value`;
  a refusal panics ("the value exactly at the declared bound must be accepted").
- `:569-579` — reads the value back whole: `read_back.len() == limit` **and**
  `read_back.iter().all(|b| *b == 0x5a)` ("not a prefix").
- `:581-591` — writes `limit + 1` bytes; requires
  `Err(ContentError::ObjectLimitExceeded { limit: refused, actual })` with
  `refused == limit && actual == limit + 1` — the refusal must name the declared bound.

The bound's figure is not free to drift:

- `core/crates/layerfs-content/src/file/cdc/gear.rs:17` — `MAXIMUM_CHUNK_BYTES = 32_768`;
  `src/filesystem/limits.rs:53` — `MAXIMUM_ATTRIBUTE_VALUE_BYTES` aliases it.
- `core/crates/layerfs-content/tests/file_read.rs:294-297` —
  `assert_eq!(READ_WAVE_BYTES, READ_WAVE_OBJECTS * 32_768)` pins the chunk maximum to the
  **literal** 32,768 (with `READ_WAVE_OBJECTS = 32` at `src/file/mapping/read.rs:24`), and
  `file_read.rs:669-672` pins `READ_WAVE_BYTES == READ_WAVE_OBJECTS * MAXIMUM_CHUNK_BYTES`;
  together they force `MAXIMUM_CHUNK_BYTES == 32_768`.
- `core/crates/layerfs-content/tests/filesystem_profile.rs:20-40` — the frozen profile
  text is rebuilt from the declared bounds and the identity asserted to be its hash, so a
  bound that moves the text moves the accepted profile.

Discination check (wrong implementations that would fail):

- enforcement at 4,096 (the round-2 bug shape): the 32,768-byte at-bound write is refused →
  panic at `:567`. **Fails.**
- no enforcement: the 32,769-byte write returns `Ok` → the `matches!` at `:584-591` fails. **Fails.**
- wrong error or wrong reported figure: `refused == limit && actual == limit + 1` fails. **Fails.**
- silent bound change (constant + enforcement moved together): the literal-32,768 identity
  in `file_read.rs:296` and the frozen profile identity fail. **Fails.**

History (context, not evidence): round-2 review
`stages-1-5-review-20260917T230700Z.md` VF-3 row = PARTIAL (F6), "the 1 MiB attribute-bound
case writes 4096 bytes". `git show 2fe2a4642` confirms the pre-fix body wrote a hardcoded
`vec![0x5a_u8; 4096]` as its "exactly at the bound" half — an at-bound assertion any
enforcement ≥ 4,097 satisfies, i.e. non-discriminating. The fix replaced it with the
`limit`/`limit + 1` pair quoted above. The round-1 review's other named instance
(`stages-1-5-review-20260917T160000Z.md`, "`filesystem_read.rs:144-148` accepts either
outcome") is also fixed in this tree: `filesystem_read.rs:135-182`
`listing_is_bounded_by_bytes_as_well_as_count` now returns exactly 1 entry for the
exactly-fitting 15-byte bound (`:142-143`) and refuses every bound from 1 to 14 that cannot
fit one row, each with the caller's own `limit` echoed (`:148-160`), plus
`InvalidRecord("listing limit")` for a zero count (`:161-164`) — a refusal, never an empty
page indistinguishable from end-of-listing.

Run: `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test
filesystem_failure --locked` → **8 passed, 0 failed, exit 0** (target includes the case).

## 2. Hunt — suites inspected line-by-line (all run, all green)

Prioritized per the task: `filesystem_limits.rs` (6 tests), `filesystem_bounds.rs` (10),
`filesystem_failure.rs` content (8) and storage (4), `filesystem_ordering.rs` (12),
`filesystem_ordering_scan.rs` (2), `storage_limits.rs` (4). Round-3-named cases:
walk-ceiling (`filesystem_bounds.rs`), read-wave (`file_read.rs` + `cas_roundtrip.rs`),
encoder-context (`file_read.rs:523-559`), policy-validation (`policy_capacity.rs`),
edit-single-read (`edit_single.rs:293-362`), into-parts (`object_identity.rs:381-417`).
Also read: `filesystem_read.rs`, `filesystem_attributes.rs`, `filesystem_codec.rs`,
`edit_bounds.rs`, `edit_batch.rs`, `edit_model.rs` (helper), `filesystem_reference.rs`
(helper), `filesystem_profile.rs`; skimmed `filesystem_topology.rs` (17 refusal cases, each
asserting a specific `InvalidRecord` variant), `filesystem_updates.rs`,
`filesystem_sorted.rs`, `filesystem_hardlinks.rs`, storage `filesystem_pipeline.rs` (by
name). A mechanical sweep of **all 173 tests** in the Stage 5 targets flagged every test
body with ≤1 hard assertion; each flagged candidate was opened and verified to delegate to
a shared oracle helper (e.g. `edit_single.rs:61-90 expect_final`,
`edit_batch.rs:86-101 expect`, `edit_model.rs:98-116 expect_model`) that compares model
bytes, length **and** the root of an independent fresh construction. No empty bodies; the
only `#[ignore]` is the fixture generator (`fixture_seal.rs`), deliberate and guarded by a
test that requires it to stay ignored (`fixture_seal.rs:83-91`).

Strong patterns found throughout (why the suite as a whole discriminates):

- Both-sides boundary probes with the bound echoed in the error:
  `filesystem_limits.rs:39-58` (255/256), `:61-78` (4,096/4,097 with the component count
  held at its limit), `:81-100` (256/257 with `over.len() <= MAXIMUM_PATH_BYTES` explicitly
  guarding that the byte bound is *not* the discriminator), `:103-119`, `:122-150`
  (`ObjectLimitExceeded{limit, actual}` for domain and key);
  `storage_limits.rs:23-43` (group body at `GROUP_LIMIT`, +1 refused with
  `limit`/`actual`), `:46-75` (record count 8,191/8,192 with a
  `count_binds_first` guard so the byte ceiling cannot be what refuses);
  `cas_roundtrip.rs:204-237` (read wave at ceiling not capacity-refused; +1 refused with
  `what`/`limit`/`actual`); `edit_bounds.rs:575-661` (edit-stream ceiling at
  `MAXIMUM_EDITS_PER_OPERATION` really run with exact expected bytes, +1 refused with
  `BoundedCapacityExceeded{what:"edit.stream"}`); `filesystem_ordering.rs:334-360`
  (append exactly `ROW_BYTES` accepted, +1 byte refused, failed append wrote nothing);
  `filesystem_attributes.rs:603-644` (`MAXIMUM_ATTRIBUTE_KEYS` +1 refused with exact
  `limit`/`actual`); `filesystem_codec.rs:325-375` (page ceiling: over-count and over-size
  both `NonCanonicalPagePartition`, accepted sizes `<= MAXIMUM_PAGE_BYTES`).
- Independent oracles, not tautologies: `edit_single.rs:85-89` / `edit_batch.rs:99-100`
  (edited root == root of a fresh construction of the same final bytes);
  `filesystem_codec.rs:56-173` and `filesystem_reference.rs:275-415` (sealed reference
  bytes and object sets); `filesystem_ordering.rs:139-269` (spilled and non-spilled builds
  must reach the **same canonical root identity**); `filesystem_attributes.rs:658-743`
  (patch route vs from-scratch build in a separate empty store, plus the emitted-value
  identity).
- Counter-vs-observation cross-checks: `filesystem_ordering.rs:237-241,310-330,641-648`,
  `filesystem_bounds.rs:497-538,722-739` (product counters compared against what the
  recording backing itself observed — not self-computed).
- Round-3-named cases verified: walk-ceiling `filesystem_bounds.rs:743-807` and `:819-910`
  (see precision note 3); read-wave `file_read.rs:162-182` (peak live payloads ≤
  `READ_WAVE_OBJECTS` through a counting store), `:265-303` (`max_payload_batch ==
  READ_WAVE_OBJECTS` exactly), `:562-735` (crafted chunk of `MAXIMUM_CHUNK_BYTES + 1_024`
  refused with exact `limit`/`actual` and no bytes emitted; the largest legal wave
  served); encoder-context `file_read.rs:523-559` (empty leaf accepted as root, refused as
  non-root, `MIN_ENTRIES - 1` refused / `MIN_ENTRIES` accepted as a child); policy
  validation `policy_capacity.rs:51-107` (six invalid policies refused with
  `UnsupportedPolicy` and **nothing created**; an out-of-band 196,608 value that is inside
  the DDL range but outside the policy range fails the open — the DDL range is
  distinguished from the policy range); edit-single-read `edit_single.rs:293-362`
  (base root demanded exactly once, no identity demanded twice); into-parts
  `object_identity.rs:381-417` (advisory predecessors — ids, bound, entries, provenance —
  survive `into_parts`, which the old tuple form dropped).
- Honest unreachable bounds, documented not faked: `filesystem_limits.rs:152-200`
  (`MAXIMUM_TREE_LEVEL`/`MAXIMUM_PAGE_BYTES` named with the arithmetic that rules a fixture
  out), `edit_bounds.rs:749-837` (deferred-state ceiling proved out of reach, premise
  checked).

## 3. Suspicion list (quoted, with the wrong implementation that would pass)

1. **`core/crates/layerfs-content/tests/filesystem_ordering.rs:721-772` —
   `a_spilled_lookup_agrees_with_a_full_scan_of_every_tier`.** The name and doc comment
   (`:722-725`: "must answer exactly what a scan from the front answers, for every serial,
   in every order") promise a lookup/scan agreement check. The body's only assertions are
   `:763-767` `rows_spilled > 0` ("the operation must really spill") and `:768-771`
   `runs.rows_read > 0` ("lookups into spilled runs must be charged"). **Wrong
   implementation that passes**: spilled lookups that return wrong or arbitrary rows — the
   update still succeeds, counters are still nonzero, no result is read back or compared.
   Mitigation: the over-promised property is asserted elsewhere in the same suite —
   `run_lookup_answers_every_serial_in_every_order` (`:775-846`) checks `find()` against a
   `visit_newest_first` truth map in ascending/descending/interleaved/repeated orders plus
   absence, and `the_pending_threshold_changes_only_where_the_rows_live` (`:139-269`)
   requires the spilled whole-operation build to reach the same canonical root identity as
   the non-spilled one. The case's *body* is a spill/charge liveness check wearing an
   agreement name.
2. **`core/crates/layerfs-content/tests/filesystem_sorted.rs:237-266` —
   `a_late_source_error_propagates_and_publishes_nothing`.** Asserts only
   `:262-263` `matches!(outcome, Err(ContentError::Io))`. The "publishes nothing" half of
   the name has no assertion — the sink is never inspected (no `store.order().len() == 0`,
   no root-role count, unlike the analogous contract cases in
   `filesystem_failure.rs:139-143,285,400-404`). **Wrong implementation that passes**: one
   that propagates the source error *and also* emits objects or a root into the sink. The
   error-identity half does discriminate (a swallowed error fails); only the second half of
   the name is unasserted, and no other case in this suite covers it for the
   bindings path.
3. **Walk-ceiling bracket is coarse (precision note, not vacuous).**
   `filesystem_bounds.rs:743-807` accepts a build of `limit - 8 = 4,088` entries (`:787`)
   and refuses `limit + 512 = 4,608` (`:793-805`); `:819-910` grows a directory to
   `limit + 24` and refuses its rename. `MAXIMUM_WALK_ENTRIES = 4_096`
   (`src/filesystem/limits.rs:72`) is pinned by no literal assertion. **Wrong
   implementation that passes**: enforcement moved anywhere in ~(4,090, 4,608]. The cases
   do discriminate against the failure modes they name — an unbounded walk (the +512 build
   would be accepted), a cycle misreport (the exact `InvalidRecord("cycle check work
   limit")` is asserted at `:797-805` and `:899-908`), and whole-tree walks on unrelated
   operations (the growth step at `:887-889` must be accepted) — but the boundary is
   bracketed at ±~500, not ±1 as in `filesystem_limits`.
4. **Page-ceiling count check cannot be isolated (precision note).**
   `filesystem_codec.rs:354-360` refuses `ceiling + 1` rows of 6-byte names as
   `NonCanonicalPagePartition`. With any legal (≥ 1-byte) name the encoded size of
   `ceiling + 1` rows also exceeds `MAXIMUM_PAGE_BYTES`, so the size check refuses the same
   input. **Wrong implementation that passes**: one with no row-count check (the size check
   alone refuses). The case's actual claim — an over-size page is a partition refusal, not
   a size error (`:363-369`) — is discriminated, and the count arithmetic is pinned at
   `:336`.
5. **Supplementary fixture-only assertion.** Storage
   `filesystem_failure.rs:218-225` (closing block of
   `a_corrupted_pack_fails_the_read_without_touching_retained_state`) compares in-memory
   `built.bag` bytes against their own decode/re-encode — no store involvement, so any
   wrong store passes it. It is a fixture self-check appended to a case whose load-bearing
   assertions (`:201-215`: the corrupted read fails, fails deterministically, database
   state unchanged) do discriminate.

## 4. Commands and exit codes

| Command | Result |
| --- | --- |
| `git rev-parse HEAD` | `99743b2cff2470e6634874d7ee14b9d37d0ba16e`, exit 0 |
| `git status --short` | empty (clean), exit 0 |
| `git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` | exit 128 — not an object; HEAD matches its short prefix |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_failure --locked` | 8 passed, 0 failed, exit 0 |
| same, `-p layerfs-content --test filesystem_limits` | 6 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_bounds` | 10 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_ordering` | 12 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_ordering_scan` | 2 passed, exit 0 |
| same, `-p layerfs-content --test file_read` | 15 passed, exit 0 |
| same, `-p layerfs-content --test edit_single` | 10 passed, exit 0 |
| same, `-p layerfs-content --test object_identity` | 11 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_attributes` | 11 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_codec` | 9 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_read` | 4 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_topology` | 17 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_reference` | 2 passed, exit 0 |
| same, `-p layerfs-content --test filesystem_profile` | 2 passed, exit 0 |
| same, `-p layerfs-content --test edit_bounds` | 12 passed, exit 0 |
| same, `-p layerfs-content --test edit_batch` | 7 passed, exit 0 |
| same, `-p layerfs-content --test edit_model` | 6 passed, exit 0 |
| same, `-p layerfs-content --test edit_noop` | 7 passed, exit 0 |
| same, `-p layerfs-content --test edit_transitions` | 7 passed, exit 0 |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-storage --test storage_limits --locked` | 4 passed, exit 0 |
| same, `-p layerfs-storage --test filesystem_failure` | 4 passed, exit 0 |
| same, `-p layerfs-storage --test policy_capacity` | 9 passed, exit 0 |
| same, `-p layerfs-storage --test cas_roundtrip` | 9 passed, exit 0 |
| read-only python sweep of all 173 Stage 5 test bodies for ≤1-assert tests | exit 0; 45 flagged, all verified as helper-oracle-driven |

22 targets run by me, 166 tests, 0 failed, 0 ignored. No repository file was modified;
build artifacts stayed under `core/target`; this file is the only file written.

## 5. UNVERIFIED

- Not read line-by-line (covered only by the mechanical sweep and, for some, by the
  round-4 suite log — not by my own inspection):
  `filesystem_hardlinks.rs` (one test read), `filesystem_updates.rs` (skimmed),
  `filesystem_sorted.rs` (flagged case read; rest skimmed), `filesystem_timing.rs`,
  `edit_timing.rs`, `edit_localized.rs`, `edit_reference.rs`, `edit_noop.rs` and
  `edit_transitions.rs` (assert-count/sweep only), `file_complete.rs`, `streaming.rs`,
  `inode_leaf.rs`, `fixture_seal.rs` (ignored generator), storage `filesystem_pipeline.rs`
  (test names only), and the remaining storage suites (`core_pipeline`, `edit_pipeline`,
  `delta_chains`, `delta_payload`, `metadata_*`, `pack_locator`, `physical_formats`,
  `provider_errors`, `visibility`, `cas_reuse`, `codec_frames`, `connection_profile`,
  `memory_bounds`, `persistence_failure`, `metadata_fingerprint_collision`) — not read and
  not run individually by me.
- Timing-measurement claims (TR rows) and the measurement harness are out of scope for
  VF-3 and were not exercised.
- The full-hash identity discrepancy in §"Tree" could not be resolved further from inside
  this checkout: no object matches `99743b2cf3a8...`; verification is anchored on short
  prefix `99743b2cf` = HEAD with a clean tree.

---

## RE-VERIFICATION (post-remedy)

- **Request**: the two named-vs-asserted gaps (§3 items 1 and 2) were remediated in commit
  `3ecb952c8` ("fix(stage5): remedy every round-4 verification finding, with receipts");
  re-verify only the two strengthened tests and record a final verdict.
- **Tree**: `git rev-parse HEAD` → `3ecb952c8ed530706700e09647f42ee51bf09f98`, parent
  `99743b2cff` (the tree this file's original verification ran on). **The working tree is
  not clean**: `core/crates/layerfs-storage/src/encoding/codec.rs` carries an uncommitted
  change (+11/−10 lines, every changed line a `//!` module-doc comment, 0 code lines —
  verified doc-only), so the runs below compiled the working tree, not exactly the commit.
  It cannot affect behavior and neither re-verified target is in `layerfs-storage`.
  Both remediated test files are themselves clean at HEAD.

### Remedy 1 — `core/crates/layerfs-content/tests/filesystem_ordering.rs:721-808`

`a_spilled_lookup_agrees_with_a_full_scan_of_every_tier` now carries the agreement its name
promises, in its own body, beside the original liveness assertions:

- `:772-780` — a second run of the **same input** (same session, scope, directory changes,
  inodes, new_inodes) with `FilesystemResources::default()`, guarded by
  `unspilled_resources.maximum_pending_records > entries` — the comparison arm is asserted
  capable of holding every row, so a shrunk default fails loudly here instead of silently
  making the comparison vacuous.
- `:800-803` — `unspilled.counters.references.rows_spilled == 0`: the comparison arm is
  asserted to really not spill.
- `:804-807` — `result.root == unspilled.root`: the spilled run and the in-memory run must
  produce the **same root identity**.

Discrimination (wrong implementations that now fail): a spilled lookup returning wrong
rows yields a different root in the spilled arm while the in-memory arm is the answer key
→ `:804-807` fails; an implementation whose "no-spill" arm also spills → `:800-803` fails;
a default pending map shrunk to ≤ 200 → `:777-780` fails. The original spill/charge
assertions (`:763-771`) are retained. Both halves of the name are now asserted in-body.

Run: `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test
filesystem_ordering --locked` → **12 passed, 0 failed, exit 0**.

### Remedy 2 — `core/crates/layerfs-content/tests/filesystem_sorted.rs:237-267`

`a_late_source_error_propagates_and_publishes_nothing` was rewritten to construct the
objects directly over a local sink (`FilesystemObjects::new(&base, &mut sink)` with
`base`/`sink` both local `TreeStore`s) and now asserts both halves of its name:

- the propagation half: `matches!(outcome, Err(ContentError::Io))`;
- the publication half: `sink.is_empty()` with the message "a failed merge publishes
  nothing, but {} objects were emitted".

The dead `Failing` consumer and `name("unused")` statements from the reviewed version are
gone (verified in the `3ecb952c8` diff); a single `let _ = lookup;` remains to keep the
import used — cosmetic.

Discrimination (wrong implementations that now fail): one that propagates the error but
still emits objects (e.g. streaming rows as it goes, emitting ~49 rows' worth before the
failed 50th) → `sink.is_empty()` fails; one that swallows the error → the `Err(Io)` match
fails. All-or-nothing publication is now pinned for the bindings path.

Run: `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test
filesystem_sorted --locked` → **6 passed, 0 failed, exit 0**.

### Final verdict for VF-3

**PASS.** The round-2 named case discriminates exactly at 32,768/32,769 with the bound's
figure pinned by independent literal assertions and the frozen profile identity (§1);
every other case inspected in the original pass carries at least one discriminating
assertion (§2); and the two named-vs-asserted gaps that qualified the original verdict are
now remediated and re-verified to assert, in their own bodies, exactly what their names
promise, with wrong-implementation scenarios that fail each new assertion. The two
precision notes from §3 stand as observations, not findings: the walk-ceiling figures were
reportedly tightened by the same commit's R2-F7 erratum (a build stating exactly 4,096
bindings accepted, 4,097 the first refusal) — **not re-verified by me**, out of the
requested scope; the page-ceiling count-check isolation note (§3 item 4) is unchanged and
was never a vacuous case. No vacuous Stage 5 case remains in everything inspected.

Post-remedy commands and exit codes: `git rev-parse HEAD` (exit 0), `git status --short`
(exit 0, shows the doc-only codec.rs modification), `git show 3ecb952c8 --stat/-- <files>`
(exit 0), `git diff` on codec.rs (exit 0, doc-only verified), both test targets above
(exit 0). No repository file modified by me except this evidence file (the append the
parent requested); build artifacts under `core/target` only.
