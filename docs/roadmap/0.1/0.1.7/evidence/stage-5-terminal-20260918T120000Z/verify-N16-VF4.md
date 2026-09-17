# N-16 / VF-4 verification — limits boundary evidence (round-3 claim check)

Verifier: independent subagent, 2026-09-18. Read-only on the repository; the only
file written is this one. Build artifacts under `core/target` only.

## Verdicts

| Claim | Verdict | One-line reason |
| --- | --- | --- |
| N-16 — every public limit has boundary evidence | **INCOMPLETE** | both suites exist and pass with both-sides probes for every cheaply reachable named boundary, but `MAXIMUM_LEVELS` (32 tiers) — a constant the round-2 finding itself named — still has no boundary case, no derived note, no §6 row and no test of its refusal; `MAXIMUM_READ_DEMANDS` is likewise an orphan |
| VF-4 — §6 limits table complete and correct; no false "enforced" | **INCOMPLETE** | no row claims "enforced" without an enforcing check (all eight verified against code and passing cases), but the table is not complete (no rows for `MAXIMUM_LEVELS`, `MAXIMUM_READ_DEMANDS`) and the scratch row's "default 4 MiB − 1" does not match the code's actual default (4 MiB) |

The delivered part is real and reproducible; the shortfalls are concrete and cited below.

## Identity

- `git rev-parse HEAD` → `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (exit 0), tree clean
  (`git status --porcelain` → 0 lines, exit 0).
- **Discrepancy:** the task's frozen commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40`
  is not HEAD; the two share only the 9-char prefix `99743b2cf`, which resolves
  uniquely to HEAD. Read-only constraints prevented checking out any other tree;
  every finding below is on HEAD's tree (the round-4 tree, §15 of the report).

## Commands run (all from the repository root)

| Command | Exit | Result |
| --- | --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_limits --locked` | 0 | 6 passed / 0 failed |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-storage --test storage_limits --locked` | 0 | 4 passed / 0 failed |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_failure --test filesystem_bounds --test filesystem_attributes --test file_read --test filesystem_codec --test inode_leaf --locked` | 0 | 59 passed / 0 failed (15/11/10/9/8/6 per suite) |
| `grep -rn "MAXIMUM_LEVELS" core/crates/layerfs-content/tests/` | 1 | no matches — no test names or exercises the tier limit |
| `grep -rn "MAXIMUM_READ_DEMANDS" core/crates/layerfs-content/tests/` | 1 | no matches — no test names or exercises the read-demand limit |
| `grep -rni "unverified\|derived" core/crates/*/tests/` | 0 | the only "Derived, unverified at scale" label is `filesystem_limits.rs:159` |
| `grep -rn "pub const MAXIMUM\|pub const .*_LIMIT\|pub const .*_BYTES\|pub const MINIMUM" core/crates/*/src` | 0 | full public-constant inventory used for the falsification pass |

## 1. The two boundary suites (claim 1) — VERIFIED

`core/crates/layerfs-content/tests/filesystem_limits.rs` (6 tests, all pass):

| Test (line) | Boundary pinned |
| --- | --- |
| `a_component_name_is_accepted_at_255_bytes_and_refused_at_256` (:39) | `MAXIMUM_NAME_BYTES=255` accepted at 255, refused at 256 with `PathLimitExceeded`; empty component refused |
| `a_path_is_accepted_at_4096_bytes_and_refused_at_4097` (:61) | `MAXIMUM_PATH_BYTES=4,096` at exactly 256 components; 4,097 refused |
| `a_path_is_accepted_at_256_components_and_refused_at_257` (:81) | `MAXIMUM_PATH_COMPONENTS=256`; 257 components refused while total bytes stay ≤ 4,096, so the component bound is what binds |
| `a_symlink_target_is_accepted_at_4096_bytes_and_refused_at_4097` (:103) | `MAXIMUM_SYMLINK_TARGET_BYTES=4,096`; 4,097 refused; embedded NUL refused at any length |
| `an_attribute_domain_and_key_are_accepted_at_their_bounds_and_refused_over` (:122) | domain 64/65 and key 255/256 refused with `ObjectLimitExceeded{limit, actual}` naming the declared bound |
| `the_limits_this_suite_cannot_bound_are_named_with_their_reason` (:168) | pins `MAXIMUM_TREE_LEVEL=31`, `MAXIMUM_PAGE_BYTES=8,192`, and asserts the derived arithmetic: `MINIMUM_INODE_BRANCH_CHILDREN^30` (64^30) overflows u128 / exceeds the u64 serial space (:184-200) |

`core/crates/layerfs-storage/tests/storage_limits.rs` (4 tests, all pass):

| Test (line) | Boundary pinned |
| --- | --- |
| `a_group_body_is_accepted_at_its_bound_and_refused_over_it` (:23) | `GROUP_LIMIT=65,536`: one record framing to exactly 65,536 framed bytes accepted; +1 byte refused `CapacityExceeded{what:"pack.group_body"}` |
| `a_group_record_count_is_accepted_at_its_bound_and_refused_over_it` (:46) | `RECORD_COUNT_LIMIT=8,191` rows accepted (count asserted to bind before the body ceiling); 8,192 refused `Integrity("group record count")`; empty group refused |
| `a_group_record_directory_is_accepted_at_its_bound_and_refused_over_it` (:78) | `record_range` directory: count 0, out-of-range ordinal, width mismatch, group beyond `SINGLETON_PACK_LIMIT`, non-ascending ends all refused |
| `the_declared_group_and_record_ceilings_are_the_ones_this_suite_pins` (:129) | `GROUP_LIMIT=65,536`, `RECORD_COUNT_LIMIT=8,191`, `GROUP_COUNT_LIMIT=256`, `TRANSACTION_ROW_LIMIT=RECORD_COUNT_LIMIT`, `TRANSACTION_CANONICAL_BYTES_LIMIT=4 MiB−1` |

Both suites exist at HEAD, were added by round-3 (`2fe2a4642`, commit message names
them), and pass. Claim 1's coverage list matches the files exactly.

## 2. Round-2 N-16 named gaps, one by one

Round-2 finding (`stages-1-5-review-20260917T230700Z.md:1000`): "`MAXIMUM_PATH_BYTES`,
`MAXIMUM_PATH_COMPONENTS`, group/record/transaction constants, `MAXIMUM_LEVELS` and
any 31-level tree have no test". WP-J (`stage-5-terminal-handoff-20260917.md:295-302`)
adds name bytes 255/256 and "a tree deeper than the two levels every current test builds".

| Named gap | Covered by | Status |
| --- | --- | --- |
| Path bytes 4,096/4,097 | `a_path_is_accepted_at_4096_bytes_and_refused_at_4097` (filesystem_limits.rs:61) | COVERED |
| Path components 256/257 | `a_path_is_accepted_at_256_components_and_refused_at_257` (:81) | COVERED |
| Name bytes 255/256 | `a_component_name_is_accepted_at_255_bytes_and_refused_at_256` (:39) | COVERED |
| Storage group/record/transaction constants where reachable | storage_limits tests 1–4 (body, count, directory, ceilings; transaction row limit pinned equal to the record count whose both-sides case exists) | COVERED |
| `MAXIMUM_LEVELS` (32 tiers) | **nothing** — see Finding F1 | **GAP** |
| A tree deeper than the two levels every current test builds | derived note for `MAXIMUM_TREE_LEVEL` (31) with executable arithmetic, labelled "Derived, unverified at scale." (filesystem_limits.rs:152-200) | PARTIAL — see Finding F2 |

## 3. §6 limits table, row by row (stage-5-report.md:395-414)

All 16 rows carry a Kind. Spot-checks of every "enforced" row against the code:

| Row (line) | Kind | Enforcement found | Case |
| --- | --- | --- | --- |
| Name component (:399) | enforced | `PathName::new` refuses > 255 | filesystem_limits :39 PASS |
| Path (:400) | enforced | `LogicalPath::new` refuses > 4,096 bytes / > 256 components | :61, :81 PASS |
| Page (:401) | format | encoders bound 8,192; codec refuses level > 31 | codec/leaf suites PASS; derived note for depth |
| Inode leaf (:402) | format | min/max rows enforced in `object/inode_leaf.rs` | inode_leaf 6 PASS |
| Inode branch (:403) | format | 64–127 children in `sorted/format.rs` | sorted/codec suites PASS |
| Directory page (:404) | format | `MINIMUM_FILLED_PAGE_BYTES=3,277` | `a_directory_leaf_never_exceeds_the_page_ceiling` (filesystem_codec.rs:325) PASS |
| Attribute key (:405) | enforced | `AttributeKey::new` refuses domain > 64 / key > 255 | filesystem_limits :122 PASS |
| Attribute value (:406) | enforced | `MAXIMUM_ATTRIBUTE_VALUE_BYTES` derived from `cdc::MAXIMUM_CHUNK_BYTES` (limits.rs:53) | `the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary` (filesystem_failure.rs:547) PASS |
| Symlink target (:407) | enforced | `SymlinkTarget::new` | filesystem_limits :103 PASS |
| Inode serial (:408) | enforced | identity.rs:48 `serial == 0 \|\| serial > MAXIMUM_INODE_SERIAL` refused | filesystem_failure.rs:491-495 (`i64::MAX + 1` refused) PASS |
| Read wave (:409) | enforced | read.rs:104 (objects) and :151-153 (bytes vs `READ_WAVE_BYTES`) | file_read.rs:266-296 asserts `READ_WAVE_BYTES == READ_WAVE_OBJECTS × 32,768`; read-wave receipt |
| Whole-tree walk entries (:410) | enforced | `MAXIMUM_CYCLE_CHECK_ENTRIES = MAXIMUM_WALK_ENTRIES` (validate.rs:55) | filesystem_bounds.rs:801, 903 (build refused above 4,095), :819 (rebind refused) PASS |
| Operation scratch (:411) | configurable | minimum ≥ 1,024 enforced (input.rs:99) | **default figure wrong — Finding F3** |
| Pending records (:412) | configurable | ≥ 1 enforced (input.rs:104); default 4,096 (reduce.rs:25) | consistent |
| Theoretical (:413) | not claimed | — | — |
| Verified (:414) | measured | — | — |

**No row claims "enforced" without an enforcing check.** The two rows round-1 VF-4
flagged (inode serial, attribute value) are now correct, and the previously
undocumented cycle-check limit is the walk-entries row with cases.

## 4. Falsification — orphan public constants

Full sweep of `pub const MAXIMUM|*_LIMIT|*_BYTES|MINIMUM` under `core/crates/*/src`.
Every filesystem limit constant has a §6 row, a boundary case, or both, except:

- **F1 `MAXIMUM_LEVELS` (32) — orphan.** `core/crates/layerfs-content/src/filesystem/references/runs.rs:41`.
  Enforced at runs.rs:223-226 (`ResourceUnavailable("ordering tiers")`) but no test
  exercises that refusal (the only `ResourceUnavailable` cases in the ordering suites
  are "ordering run cleanup" ×2 and "ordering backing capacity"); no §6 row; no
  derived note; no "derived, unverified at scale" label. Round-2's own gap list
  (`evidence/stages-1-5-review-20260917T230700Z/sa-adapters-limits.md:434`) names it;
  it remains untested at HEAD. The claim's bullet — that MAXIMUM_LEVELS is "recorded
  with its derived arithmetic and labelled" — is **not satisfied**: the only such
  label (filesystem_limits.rs:159) covers `MAXIMUM_TREE_LEVEL` (31), a different
  constant in a different module. The only place MAXIMUM_LEVELS appears in any
  arithmetic is a memory-budget line in a diagnostics receipt
  (`stage-5-terminal-20260918T120000Z/simultaneous-memory.log:9`, "derived bound
  MAXIMUM_LEVELS x merge buffer"), which is not boundary evidence for the limit.
- **F2 The deep-tree note is partial and carries two stale cross-references.**
  The note (filesystem_limits.rs:152-166) labels `MAXIMUM_TREE_LEVEL` (31) "Derived,
  unverified at scale" with real arithmetic (64^30 asserted in the test body, :184-200)
  — but (a) it names only the filesystem page-level constant; the extent/mapping
  tree's own `MAX_LEVEL = 31` (`file/mapping/types.rs:17`) is named by no note or
  case, and no fixture builds more than the two mapping levels every test builds
  (`file_read.rs:210` "a real two-level mapping tree"); (b) :157 cites a test
  `a_page_that_declares_an_impossible_level_is_refused` that **does not exist**
  anywhere (closest real coverage: `malformed_leaves_are_rejected`,
  inode_leaf.rs:134-155, and `malformed_and_reserved_input_is_rejected_once`,
  filesystem_codec.rs:241 — both refuse "role/level"); (c) :166 points at a
  `storage_bounds` suite that **does not exist** (the actual suite is `storage_limits`).
- **F3 §6 scratch row's default figure is wrong.** §6:411 says "default 4 MiB − 1".
  The operation's actual default is 4 MiB: `FilesystemResources::default()` sets
  `scratch_bytes = sorted::MAXIMUM_SCRATCH_BYTES` (input.rs:73) =
  `limits::MAXIMUM_OPERATION_SCRATCH_BYTES` = 4 MiB (limits.rs:34), and update.rs:246
  passes that value to the budget. The 4 MiB − 1 figure
  (`DEFAULT_OPERATION_SCRATCH_BYTES`, limits.rs:36) is used only by
  `Budget::default_limit()` (budget.rs:33-35), which has **no callers** (dead code).
  Also, no check enforces scratch ≤ `MAXIMUM_OPERATION_SCRATCH_BYTES` — only the
  ≥ 1,024 minimum (input.rs:99) — so the constant's "Largest bytes…" doc
  (limits.rs:30-33) overstates what is checked.
- **F4 `MAXIMUM_READ_DEMANDS` (4,096) — orphan.** `filesystem/objects.rs:20`,
  enforced at :90-95 (`ObjectLimitExceeded`), but no §6 row and no test (grep exit 1).
  Its storage-side twin `READ_OBJECT_LIMIT` does have a both-sides case
  (`cas_reuse.rs:272-324`), which makes the content-side gap conspicuous.
- **F5 Storage work/cache budgets — no boundary cases (secondary).**
  `ENCODE_WORKSPACE_BYTES`/`DECODE_WORKSPACE_BYTES` (encoding/codec.rs:48,50),
  `INDEX_BYTES` (delta/candidates.rs:15), `PROGRAM_LIMIT` (pool/delta.rs:18),
  `DEPENDENCY_PACK_CACHE_BYTES` (policy.rs:113), `POOLED_VALUE_CACHE_BYTES` (:121),
  `METADATA_DECODED_WORK_LIMIT` (:123), `METADATA_MATCH_BUDGET_BYTES` (:133),
  `COMPARE_WINDOW_BYTES` (content file/edit/compare.rs:19) have no boundary cases —
  the same list round-2 recorded at sa-adapters-limits.md:434. They are documented
  as work budgets in the Stages 3-4 closeout docs, are outside Stage-5 §6's scope,
  and were not in N-16's named list; recorded here for completeness under the
  "every public limit" reading.
- Not orphans: `MAXIMUM_ATTRIBUTE_KEYS` (no §6 row but a both-sides case at
  filesystem_attributes.rs:603-640, refusal with the declared limit at :638-640);
  `MAXIMUM_DIRECTORY_LEAF_ROWS` (derived quotient documented limits.rs:85-92, case
  filesystem_codec.rs:325); `PACK_LIMIT`/`SINGLETON_PACK_LIMIT`/group-count refusals
  (physical_formats.rs:69-95); `MAX_CANONICAL_OBJECT_BYTES`/`MAX_OBJECT_FIELD_BYTES`/
  `MAXIMUM_EDITS_PER_OPERATION`/`MAXIMUM_DELTA_MAX_DEPTH` (Stages 3-4 W10 boundary
  work, commit c56c28dd1); `EDIT_DEFERRED_LIMIT` recorded UNRUN with its derived
  arithmetic (c56c28dd1; edit_bounds.rs:764). `GROUP_COUNT_LIMIT` is pinned as a
  constant (storage_limits :129) and its singleton/zero cases exist
  (physical_formats.rs:84-95), but the ordinary-lane 256/257 group boundary is not
  probed — minor.

## UNVERIFIED

- The task's stated frozen commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40` could not
  be resolved or checked out (read-only); all evidence is from HEAD
  `99743b2cff2470e6634874d7ee14b9d37d0ba16e`, which shares the 9-char prefix.
- Whether a legal level-31 tree (filesystem or mapping) can actually be built is
  accepted as derived: the arithmetic is asserted in the test, but no fixture
  constructs more than two mapping levels or deep filesystem trees.
- The diagnostics receipts (`walk-ceiling.log`, `read-wave.log`, etc.) were not
  re-run; the product tests they cite were run instead and pass.
- Storage work-budget constants (F5) were not probed for enforcement semantics;
  their classification as work budgets follows the Stages 3-4 documentation.
- The full workspace suite and clippy were not re-run (only the suites named by the
  task plus the six §6-backing suites); round-4's `check-*.log` receipts record them.

## Bottom line

Round 3 delivered the suites it advertised, and every reachable boundary in the
claim has a real, passing, both-sides probe. What was not delivered: any evidence
or derived record for `MAXIMUM_LEVELS` — the one constant the round-2 N-16 finding
named that is not a cheap fixture bound — plus two orphan/misreferenced items in the
note and table (`MAXIMUM_READ_DEMANDS`; the scratch default figure; the phantom test
and suite names). N-16 and VF-4 as claimed are therefore INCOMPLETE, not PASS.
