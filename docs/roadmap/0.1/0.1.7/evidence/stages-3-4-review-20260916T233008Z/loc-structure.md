
# Stages 3-4 (#168 / #169): structure and production-LOC audit

**Independent review artifact.** Reviewer: delegated audit agent. Snapshot audited:
HEAD 91c3a0741fff64e8161d5c1b6e759f347ffbf757, branch main, working tree clean apart from this
evidence directory. Evidence: docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/.
Companion data in the same directory: loc-per-file.csv, loc-before-after.csv,
loc-file-plan-comparison.csv, loc-tree.md.

**Working-tree state at audit time** (git status --porcelain=v1): the single entry is
"?? docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/", i.e. this evidence
directory itself. No product source was modified, no build was run, no git state was changed.

**Scope of this audit.** Production-LOC counter semantics, per-file and per-directory structure,
physical-line ceilings, delivery against stages-3-4-file-plan.md, and the per-commit LOC
disclosure. Test oracles, acceptance criteria, performance and memory claims are other lanes.

---

## 0. Findings, most important first

| # | severity | finding |
| --- | --- | --- |
| F1 | **DEFECT (material; reference scope only)** | The counter removes every Rust item whose #[cfg(...)] attribute merely *contains* the substring "test" (tools/production_loc.py:135), deleting real production code: 3 #[cfg(not(test))], 4 #[cfg(not(feature = "test-instrumentation"))], 14 #[cfg(any(test, ...))] and 12 #[cfg(any(debug_assertions, feature = "test-instrumentation"))] items. Measured over-removal: **1027 lines**. The file plan warned about exactly this (stages-3-4-file-plan.md:24-26: "the prior review found reference cfg(not(test)) classification errors. Do not propagate uncorrected reference totals into a commit comparison") yet the batch propagated "reference 68476 (unchanged)" into **all 17 commit messages** and into stages-3-4-report.md:123. |
| F2 | **DEFECT (material; reference scope only)** | The same counter counts **3737 production LOC of test-only modules that live as separate files under reference src/**: 11 *_tests.rs files declared by a #[cfg(test)] mod ...; line plus objects/admission/issue100_diagnostic.rs, declared via #[path] inside the cfg(test)-gated objects/admission/native_tests.rs. blank_inline_tests removes the declaration line but cannot remove the separate file. The reported reference subtotal is therefore simultaneously inflated by 3737 and deflated by 1027; a corrected estimate is **about 65766**, not 68476. |
| F3 | **NOT CERTIFIED** | Because of F1/F2 I do **not** certify the reference migration subtotal 68476 or any combined total that embeds it (74628 / 77136 / 78875 / 78891 / 79369 / 79405 in the commit messages and in stages-3-4-report.md). |
| F4 | **VERIFIED-CORRECT (core scope)** | The C1/C2/telemetry numbers are unaffected by F1/F2: core/crates/*/src contains **zero** occurrences of the string "cfg" (grep -rn 'cfg' core/crates/*/src -> 0 matches), so the inline-test stripper removes **0 lines** from the product scope, and no test-only file lives under product src/. Audited totals: **core 6152 -> 10929 (delta +4777)**; layerfs-content 2336 -> 4385, layerfs-storage 3084 -> 5812, layerfs-telemetry 732 -> 732. These are the numbers a reviewer may rely on. |
| F5 | **DEFECT (reporting consistency; low)** | The headline "Production LOC: <before> -> <after> (delta <n>)" silently changes scope mid-batch: c38961f2f..b49931570 disclose the **combined** total (74628 -> 78891), while 01d9f70f3..91c3a0741 disclose the **core-only** total (10415 -> 10929). The before-value drops by 68476 between two consecutive commits with no scope label in the headline. The combined subtotal is still stated in each message body, so no information is lost, but the required format is no longer comparable across the range on its own. |
| F6 | **DEFECT (per-commit accuracy; low, self-corrected later)** | Two commit messages misallocate 58 lines between packages: da8ee5769 discloses "layerfs-content 3572 -> 3949" / "layerfs-storage 4356 -> 5718"; the audited values for that tree are **3891 / 5776**. b49931570 carries the same error ("content 3965" vs audited **3907**). Both headlines (core 10399, 10415; combined 78875, 78891) are correct. 2b2dbc028 corrects the record later; the two immutable messages stay wrong. |
| F7 | **VERIFIED-CORRECT** | All 17 commits in 5e8b8cbc2..HEAD carry a "Production LOC:" disclosure and a stated method; 15/17 reproduce exactly at headline level, and the correct pre-Stage-3 basis was used (64e3f9d6a states "Before = first-parent tree c38961f2f"). The counter is byte-identical to the hash quoted in the first message (sha256 09568dbd352a103ff097067a39060754d8c57330e9429d14a205475a45a80d59). |
| F8 | **DEFECT (deliverable gap; material for review)** | The per-file "actual-size report" the plan requires (stages-3-4-file-plan.md:283-299: path / action / before / actual after / signed delta / recommended final range / below-within-above / physical lines / explanation and responsibility) **does not exist** in stages-3-4-report.md. Its section 3 has directory totals (which I reproduce exactly), a seven-file "larger production files" list, and new/deleted lists - but no per-file range verdicts. Sections 2 and 7 below supply the missing analysis. |
| F9 | **VERIFIED-CORRECT (caps)** | No product file approaches the ceiling: largest is cas/owner.rs at **890 physical lines** (< 999); every product lib.rs/mod.rs is far under 200 (largest content/src/lib.rs, 41 physical); runtime SQL sql/schema.sql is 69 physical. The report's cap claims (stages-3-4-report.md:149-151) are accurate. |
| F10 | **DEFECT (plan deviation; partly declared)** | 8 planned files are absent and 1 unplanned file exists: file/edit/frontier.rs (deleted; superseded by file/edit/tree.rs - declared), file/mapping/predecessor.rs (its responsibility sits in file/edit/input.rs:291-302 - an **undeclared** merge), encoding/codec/{mod,profile,encode,decode}.rs (never created; encoding/codec.rs was **not retired** and grew 367 -> 508 - declared), pack/read.rs (merged into pack/layout.rs:291), pack/singleton.rs (merged into pack/assemble.rs:126 plus the v7 lane in pack/layout.rs). Product file count is **68**, not the planned 74. |

---

## 1. Counter audit (tools/production_loc.py, sha256 09568dbd...a80d59)

Commands actually run, from the repository root:

~~~sh
python3 tools/production_loc.py --help
python3 tools/production_loc.py --files
python3 tools/production_loc.py --detail
shasum -a 256 tools/production_loc.py tools/test_production_loc.py
python3 tools/test_production_loc.py
git status --porcelain=v1
git log --oneline 5e8b8cbc2..HEAD
~~~

Raw output excerpts:

~~~text
usage: production_loc.py [-h] [--root ROOT] [--detail] [--json] [--files]
...
Method: comments are blanked by a Rust-aware scanner (line/nested block comments,
normal/raw/byte strings, char literals and lifetimes) and a line counts once when
it still holds non-whitespace. See AGENTS.md, "Production LOC comparison for every
commit".

core        10929 production lines in   75 files
          layerfs-content                                     4385
          layerfs-storage                                     5812
          layerfs-telemetry                                    732
reference   68476 production lines in  206 files
combined    79405 production lines

$ python3 tools/test_production_loc.py
.............
Ran 13 tests in 0.023s
OK

$ shasum -a 256 tools/production_loc.py tools/test_production_loc.py
09568dbd352a103ff097067a39060754d8c57330e9429d14a205475a45a80d59  tools/production_loc.py
8e6fc0573214f16c351185ea568d193d8cfe8dd4ce73a1c7459e50e86b6f9639  tools/test_production_loc.py
~~~

### (a) Nonblank non-comment lines only? - **VERIFIED-CORRECT**

counted_lines (tools/production_loc.py:182-188) blanks comments first, then counts
sum(1 for line in text.splitlines() if line.strip()). Blank/whitespace-only lines are excluded
and a line holding both code and a trailing comment counts exactly once.

### (b) Rust block/line comments stripped? - **VERIFIED-CORRECT**

blank_rust (:34-77) blanks "//" to end of line (:57-61) and "/* */" with a **nesting-depth
counter** (:43-56 to enter/nest, :62-66), preserving every newline so line numbering is stable.
Its own test test_nested_block_comments_and_doc_comments_do_not_count
(tools/test_production_loc.py:33-39) is a genuine oracle: 3 comment lines vs 2 code lines, which
a non-nesting scanner would fail.

### (c) Inline #[cfg(test)] modules / test-only branches excluded? - **DEFECT (over-inclusive) + DEFECT (separate-file test modules)**

The gate is a substring test, tools/production_loc.py:135:

~~~python
if "test" not in attribute.replace(" ", ""):
    continue
~~~

Direct probe of the shipped functions (in memory; no file created):

~~~text
[cfg(test) mod]             counted lines = 1   (module removed - intended)
[cfg(not(test))]            counted lines = 0   <-- PRODUCTION CODE REMOVED
[cfg(any(test,x))]          counted lines = 0   <-- PRODUCTION CODE REMOVED
[cfg(feature=test-support)] counted lines = 0   <-- PRODUCTION CODE REMOVED
[cfg(test) on a separate FILE mod]
  after blank_inline_tests = '            \n                 \npub fn keep() {\n    1\n}\n'
  counted lines = 3        (the declaration is blanked; the separate FILE is still counted whole)
[cfg(feature="test-instrumentation")] counted lines = 0
~~~

Measured on the two real trees with the counter's own traversal, re-implemented so that only
#[cfg(test)] / #[cfg(all(test, ...))] spans are blanked:

~~~text
core:      counter = 10881 .rs lines;  pure-test-only removal = 10881;  =>     0 LOC removed from PRODUCTION
reference: counter = 67308 .rs lines;  pure-test-only removal = 68335;  =>  1027 LOC removed from PRODUCTION
           (+1168 reference runtime SQL both ways: reported 68476, corrected-figure basis 69503)
reference: 11 files matching /src/.*_tests\.rs counted as production = 3537 LOC
           + objects/admission/issue100_diagnostic.rs (200 prod LOC, #[path]-declared inside the
             cfg(test)-gated objects/admission/native_tests.rs)             = 3737 LOC test-only, counted
~~~

Attribute census over every counted file, using the counter's own #[cfg(...)] extraction rule
(455 attributes): #[cfg(test)] 201, #[cfg(feature = "test-instrumentation")] 88, #[cfg(unix)] 73,
#[cfg(feature = "live")] 41, #[cfg(target_os = "linux")] 36,
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))] 36, #[cfg(not(unix))] 18,
#[cfg(any(debug_assertions, feature = "test-instrumentation"))] 12, #[cfg(target_os = "macos")] 10,
#[cfg(any(test, all(target_os = "linux", ...)))] 14 (4 of them multi-line), #[cfg(not(test))] 3,
#[cfg(not(feature = "test-instrumentation"))] 4, plus 8 further forms. Every form containing "test"
is treated as a test gate.

**Impact on this delivery: zero for C1/C2/telemetry (0 lines removed), and a +/- error in the
reference subtotal (F2).** No file under core/crates/*/src contains the string "cfg" at all - which
is also what core/AGENTS.md ("product source contains product code only") requires.

### (d) Are core/crates/*/sql/*.sql counted as production? - **VERIFIED-CORRECT (counting); DEFECT (docstring, no measured impact)**

scope_files (:210-212) admits any <crate>/sql/**/*.sql for both scopes and counted_lines
(:186-187) routes .sql through blank_sql (:173-179). --files reports
"core/crates/layerfs-storage/sql/schema.sql 48 prod / 69 physical", and that 48 is inside the
layerfs-storage 5812 crate total (5764 src + 48 sql). But blank_sql's docstring claims "line and
block comments" while it only blanks lines whose first non-blank characters are "--"; a /* ... */
block is untouched. Probe:

~~~text
blank_sql('CREATE TABLE t (\n/* block\n   comment */\n  id INTEGER\n);\n-- line comment\n')
  -> counted 5   (the two block-comment-only lines survive; the file has 4 code lines)
grep -c '/\*' core/crates/layerfs-storage/sql/schema.sql   -> 0    # no impact on this file
grep -c '^[[:space:]]*--' core/crates/layerfs-storage/sql/schema.sql -> 15  # the 15 blanked lines
~~~

### (e) tests/, examples/, benches/, fixtures, target/, generated excluded? - **VERIFIED-CORRECT for the named set; LATENT GAP for fixtures/generated**

SKIP_DIRS = {"target", "tests", "examples", "benches", "node_modules"} (:30); .rs files are only
picked up when "src" is a path component (:208), so tests/, examples/ and benches/ are excluded
twice over and target/ (including a workspace-level core/target) by directory name. "fixtures",
"generated", "mocks", "testdata" and "vendor" are **not** in SKIP_DIRS - a latent gap with no
impact here: "find core/crates -type d ( -name fixtures -o -name generated -o -name mocks
-o -name testdata )" returns nothing, and there is no .rs file under core/crates outside src/,
tests/ or examples/. Cargo.toml, README.md/USAGE.md, .DS_Store and the benchmark tree are never in
scope. Four .DS_Store files exist on disk under core/crates but are git-ignored and untracked
(git check-ignore -v -> ".gitignore:4:.DS_Store"), so they cannot enter a committed snapshot.

### (f) String literals containing "//" - **VERIFIED-CORRECT; the shipped test does not discriminate**

blank_rust dispatches on the opening quote (:70-75) and blank_quoted (:89-100) skips a quoted
region *without* scanning it for "//", honouring backslash escapes; raw strings go through
is_raw_string_start/blank_raw_string (:80-111). Probe:

~~~text
[url-string]      raw = 'pub const U: &str = "http://x"; // trailing\n'
  after blank_rust = 'pub const U: &str = "http://x";            \n'  (string kept, comment blanked)
[raw-string]      'let x = r#"raw // text"#;'   -> unchanged, counted 1
[byte-raw-string] 'let x = br#"raw // text"#;'  -> unchanged, counted 1
~~~

**Oracle weakness (MISSING EVIDENCE, not an output defect):** the shipped test
test_strings_and_characters_do_not_open_comments (tools/test_production_loc.py:41-47) asserts only
a line count of 3. A scanner that treated the "//" inside "http://example" as a comment would also
return 3, because the line keeps non-blank code before the string. The property itself is true, as
the direct scan above shows.

### Additional counter observations

* physical_lines (:216-219) counts by iteration, which differs from wc -l for a file without a
  trailing newline. Compared for **all 281 counted files**: "files where counter physical_lines
  != wc -l: 0". **VERIFIED-CORRECT** - the physical-line column and the 999 cap are measured on
  exactly the number claimed.
* counted_lines/physical_lines read UTF-8 and raise UnicodeDecodeError on binary input (observed
  on .DS_Store during a whole-tree enumeration). It fails loudly rather than miscounting, and no
  in-scope file is non-UTF-8.
* Scope resolution is root-relative (scope_base, :191-194): the counter must be invoked with
  --root <tree>; run from core/ it silently returns zero files. Every number here uses --root,
  matching stages-3-4-report.md:116-118.
* The counter is **unchanged** in this batch (git log 5e8b8cbc2..HEAD -- tools/production_loc.py is
  empty) and its sha256 equals the hash quoted in c38961f2f's message, so all 17 commits used the
  counter audited here. Reproducible practice - but it also means the file-plan warning at
  stages-3-4-file-plan.md:24-26 was never actioned.

---

## 2. Per-file table

Every file under core/crates/ (139 files on disk, 135 tracked), with the audited counter value and
physical lines. "before prod" and "delta" come from the pre-Stage-3 snapshot (section 5a).
Machine-readable copy: loc-per-file.csv and loc-before-after.csv.

| path | class | prod LOC | physical | before prod | delta |
| --- | --- | ---: | ---: | ---: | ---: |
| `core/crates/.DS_Store` | non-prod:junk | 0 | 3 | - | - |
| `core/crates/layerfs-content/.DS_Store` | non-prod:junk | 0 | 3 | - | - |
| `core/crates/layerfs-content/Cargo.toml` | non-prod:manifest | 0 | 11 | - | - |
| `core/crates/layerfs-content/README.md` | non-prod:docs | 0 | 88 | - | - |
| `core/crates/layerfs-content/examples/edit_timing_c1.rs` | non-prod:example | 0 | 180 | - | - |
| `core/crates/layerfs-content/examples/fingerprint_collision_search.rs` | non-prod:example | 0 | 175 | - | - |
| `core/crates/layerfs-content/src/.DS_Store` | non-prod:junk | 0 | 3 | - | - |
| `core/crates/layerfs-content/src/error.rs` | production | 120 | 174 | 116 | 4 |
| `core/crates/layerfs-content/src/file/.DS_Store` | non-prod:junk | 0 | 5 | - | - |
| `core/crates/layerfs-content/src/file/cdc/gear.rs` | production | 494 | 538 | 494 | 0 |
| `core/crates/layerfs-content/src/file/cdc/mod.rs` | production | 5 | 10 | 5 | 0 |
| `core/crates/layerfs-content/src/file/content.rs` | production | 195 | 249 | 195 | 0 |
| `core/crates/layerfs-content/src/file/edit/apply.rs` | production | 391 | 447 | 0 | 391 |
| `core/crates/layerfs-content/src/file/edit/compare.rs` | production | 67 | 86 | 0 | 67 |
| `core/crates/layerfs-content/src/file/edit/concat.rs` | production | 19 | 29 | 0 | 19 |
| `core/crates/layerfs-content/src/file/edit/finish.rs` | production | 38 | 54 | 0 | 38 |
| `core/crates/layerfs-content/src/file/edit/input.rs` | production | 286 | 390 | 0 | 286 |
| `core/crates/layerfs-content/src/file/edit/mod.rs` | production | 15 | 20 | 0 | 15 |
| `core/crates/layerfs-content/src/file/edit/split.rs` | production | 23 | 32 | 0 | 23 |
| `core/crates/layerfs-content/src/file/edit/tree.rs` | production | 552 | 645 | 0 | 552 |
| `core/crates/layerfs-content/src/file/mapping/build.rs` | production | 301 | 371 | 236 | 65 |
| `core/crates/layerfs-content/src/file/mapping/codec.rs` | production | 303 | 334 | 303 | 0 |
| `core/crates/layerfs-content/src/file/mapping/mod.rs` | production | 15 | 20 | 15 | 0 |
| `core/crates/layerfs-content/src/file/mapping/read.rs` | production | 210 | 241 | 210 | 0 |
| `core/crates/layerfs-content/src/file/mapping/types.rs` | production | 182 | 248 | 182 | 0 |
| `core/crates/layerfs-content/src/file/mod.rs` | production | 17 | 23 | 10 | 7 |
| `core/crates/layerfs-content/src/file/read.rs` | production | 111 | 127 | 111 | 0 |
| `core/crates/layerfs-content/src/file/view.rs` | production | 142 | 171 | 0 | 142 |
| `core/crates/layerfs-content/src/lib.rs` | production | 24 | 41 | 16 | 8 |
| `core/crates/layerfs-content/src/object/access.rs` | production | 18 | 36 | 18 | 0 |
| `core/crates/layerfs-content/src/object/codec.rs` | production | 107 | 138 | 107 | 0 |
| `core/crates/layerfs-content/src/object/id.rs` | production | 89 | 119 | 89 | 0 |
| `core/crates/layerfs-content/src/object/inode_leaf.rs` | production | 311 | 397 | 0 | 311 |
| `core/crates/layerfs-content/src/object/mod.rs` | production | 23 | 29 | 12 | 11 |
| `core/crates/layerfs-content/src/object/output.rs` | production | 124 | 192 | 111 | 13 |
| `core/crates/layerfs-content/src/object/predecessor.rs` | production | 67 | 107 | 0 | 67 |
| `core/crates/layerfs-content/src/policy.rs` | production | 136 | 226 | 106 | 30 |
| `core/crates/layerfs-content/tests/edit_batch.rs` | non-prod:test | 0 | 217 | - | - |
| `core/crates/layerfs-content/tests/edit_bounds.rs` | non-prod:test | 0 | 398 | - | - |
| `core/crates/layerfs-content/tests/edit_localized.rs` | non-prod:test | 0 | 610 | - | - |
| `core/crates/layerfs-content/tests/edit_model.rs` | non-prod:test | 0 | 233 | - | - |
| `core/crates/layerfs-content/tests/edit_noop.rs` | non-prod:test | 0 | 239 | - | - |
| `core/crates/layerfs-content/tests/edit_reference.rs` | non-prod:test | 0 | 615 | - | - |
| `core/crates/layerfs-content/tests/edit_single.rs` | non-prod:test | 0 | 281 | - | - |
| `core/crates/layerfs-content/tests/edit_timing.rs` | non-prod:test | 0 | 202 | - | - |
| `core/crates/layerfs-content/tests/edit_transitions.rs` | non-prod:test | 0 | 265 | - | - |
| `core/crates/layerfs-content/tests/file_complete.rs` | non-prod:test | 0 | 328 | - | - |
| `core/crates/layerfs-content/tests/file_read.rs` | non-prod:test | 0 | 206 | - | - |
| `core/crates/layerfs-content/tests/inode_leaf.rs` | non-prod:test | 0 | 174 | - | - |
| `core/crates/layerfs-content/tests/object_identity.rs` | non-prod:test | 0 | 303 | - | - |
| `core/crates/layerfs-content/tests/streaming.rs` | non-prod:test | 0 | 147 | - | - |
| `core/crates/layerfs-content/tests/support/mod.rs` | non-prod:test | 0 | 534 | - | - |
| `core/crates/layerfs-content/tests/timing.rs` | non-prod:test | 0 | 239 | - | - |
| `core/crates/layerfs-storage/Cargo.toml` | non-prod:manifest | 0 | 14 | - | - |
| `core/crates/layerfs-storage/README.md` | non-prod:docs | 0 | 146 | - | - |
| `core/crates/layerfs-storage/examples/measure_components.rs` | non-prod:example | 0 | 354 | - | - |
| `core/crates/layerfs-storage/examples/measure_edits.rs` | non-prod:example | 0 | 582 | - | - |
| `core/crates/layerfs-storage/examples/measure_pooled.rs` | non-prod:example | 0 | 322 | - | - |
| `core/crates/layerfs-storage/sql/schema.sql` | production(runtime SQL) | 48 | 69 | 46 | 2 |
| `core/crates/layerfs-storage/src/cas/batch.rs` | production | 58 | 87 | 58 | 0 |
| `core/crates/layerfs-storage/src/cas/dependencies.rs` | production | 62 | 88 | 62 | 0 |
| `core/crates/layerfs-storage/src/cas/finish.rs` | production | 16 | 25 | 16 | 0 |
| `core/crates/layerfs-storage/src/cas/membership.rs` | production | 32 | 47 | 42 | -10 |
| `core/crates/layerfs-storage/src/cas/mod.rs` | production | 11 | 16 | 11 | 0 |
| `core/crates/layerfs-storage/src/cas/owner.rs` | production | 710 | 890 | 364 | 346 |
| `core/crates/layerfs-storage/src/cas/read.rs` | production | 74 | 101 | 65 | 9 |
| `core/crates/layerfs-storage/src/cas/save.rs` | production | 52 | 73 | 49 | 3 |
| `core/crates/layerfs-storage/src/cas/store.rs` | production | 395 | 530 | 327 | 68 |
| `core/crates/layerfs-storage/src/encoding/codec.rs` | production | 508 | 632 | 367 | 141 |
| `core/crates/layerfs-storage/src/encoding/decode.rs` | production | 170 | 192 | 103 | 67 |
| `core/crates/layerfs-storage/src/encoding/delta/candidates.rs` | production | 126 | 161 | 0 | 126 |
| `core/crates/layerfs-storage/src/encoding/delta/mod.rs` | production | 4 | 8 | 0 | 4 |
| `core/crates/layerfs-storage/src/encoding/delta/read.rs` | production | 202 | 261 | 0 | 202 |
| `core/crates/layerfs-storage/src/encoding/delta/record.rs` | production | 201 | 240 | 0 | 201 |
| `core/crates/layerfs-storage/src/encoding/delta/select.rs` | production | 262 | 343 | 0 | 262 |
| `core/crates/layerfs-storage/src/encoding/full.rs` | production | 177 | 213 | 112 | 65 |
| `core/crates/layerfs-storage/src/encoding/mod.rs` | production | 11 | 17 | 9 | 2 |
| `core/crates/layerfs-storage/src/encoding/pool/delta.rs` | production | 262 | 289 | 0 | 262 |
| `core/crates/layerfs-storage/src/encoding/pool/index.rs` | production | 203 | 259 | 0 | 203 |
| `core/crates/layerfs-storage/src/encoding/pool/leaf.rs` | production | 118 | 161 | 0 | 118 |
| `core/crates/layerfs-storage/src/encoding/pool/mod.rs` | production | 9 | 14 | 0 | 9 |
| `core/crates/layerfs-storage/src/encoding/pool/read.rs` | production | 255 | 294 | 0 | 255 |
| `core/crates/layerfs-storage/src/encoding/pool/value_group.rs` | production | 79 | 103 | 0 | 79 |
| `core/crates/layerfs-storage/src/error.rs` | production | 105 | 154 | 105 | 0 |
| `core/crates/layerfs-storage/src/lib.rs` | production | 11 | 30 | 11 | 0 |
| `core/crates/layerfs-storage/src/pack/assemble.rs` | production | 223 | 252 | 195 | 28 |
| `core/crates/layerfs-storage/src/pack/layout.rs` | production | 381 | 466 | 321 | 60 |
| `core/crates/layerfs-storage/src/pack/mod.rs` | production | 12 | 17 | 10 | 2 |
| `core/crates/layerfs-storage/src/pack/placement.rs` | production | 123 | 159 | 123 | 0 |
| `core/crates/layerfs-storage/src/policy.rs` | production | 210 | 332 | 148 | 62 |
| `core/crates/layerfs-storage/src/sqlite/cleanup.rs` | production | 58 | 77 | 58 | 0 |
| `core/crates/layerfs-storage/src/sqlite/connection.rs` | production | 50 | 72 | 50 | 0 |
| `core/crates/layerfs-storage/src/sqlite/lookup.rs` | production | 141 | 177 | 118 | 23 |
| `core/crates/layerfs-storage/src/sqlite/mod.rs` | production | 10 | 15 | 8 | 2 |
| `core/crates/layerfs-storage/src/sqlite/pool.rs` | production | 128 | 160 | 0 | 128 |
| `core/crates/layerfs-storage/src/sqlite/schema.rs` | production | 232 | 267 | 223 | 9 |
| `core/crates/layerfs-storage/src/sqlite/write.rs` | production | 83 | 117 | 83 | 0 |
| `core/crates/layerfs-storage/tests/cas_reuse.rs` | non-prod:test | 0 | 265 | - | - |
| `core/crates/layerfs-storage/tests/cas_roundtrip.rs` | non-prod:test | 0 | 141 | - | - |
| `core/crates/layerfs-storage/tests/core_pipeline.rs` | non-prod:test | 0 | 268 | - | - |
| `core/crates/layerfs-storage/tests/delta_chains.rs` | non-prod:test | 0 | 269 | - | - |
| `core/crates/layerfs-storage/tests/delta_payload.rs` | non-prod:test | 0 | 286 | - | - |
| `core/crates/layerfs-storage/tests/edit_pipeline.rs` | non-prod:test | 0 | 228 | - | - |
| `core/crates/layerfs-storage/tests/memory_bounds.rs` | non-prod:test | 0 | 250 | - | - |
| `core/crates/layerfs-storage/tests/metadata_chain.rs` | non-prod:test | 0 | 231 | - | - |
| `core/crates/layerfs-storage/tests/metadata_fingerprint_collision.rs` | non-prod:test | 0 | 152 | - | - |
| `core/crates/layerfs-storage/tests/metadata_pool.rs` | non-prod:test | 0 | 345 | - | - |
| `core/crates/layerfs-storage/tests/metadata_pool_index.rs` | non-prod:test | 0 | 273 | - | - |
| `core/crates/layerfs-storage/tests/metadata_window.rs` | non-prod:test | 0 | 263 | - | - |
| `core/crates/layerfs-storage/tests/pack_locator.rs` | non-prod:test | 0 | 299 | - | - |
| `core/crates/layerfs-storage/tests/persistence_failure.rs` | non-prod:test | 0 | 277 | - | - |
| `core/crates/layerfs-storage/tests/physical_formats.rs` | non-prod:test | 0 | 136 | - | - |
| `core/crates/layerfs-storage/tests/policy_capacity.rs` | non-prod:test | 0 | 264 | - | - |
| `core/crates/layerfs-storage/tests/support/mod.rs` | non-prod:test | 0 | 323 | - | - |
| `core/crates/layerfs-storage/tests/timing.rs` | non-prod:test | 0 | 175 | - | - |
| `core/crates/layerfs-storage/tests/visibility.rs` | non-prod:test | 0 | 294 | - | - |
| `core/crates/layerfs-telemetry/Cargo.toml` | non-prod:manifest | 0 | 7 | - | - |
| `core/crates/layerfs-telemetry/README.md` | non-prod:docs | 0 | 216 | - | - |
| `core/crates/layerfs-telemetry/USAGE.md` | non-prod:docs | 0 | 303 | - | - |
| `core/crates/layerfs-telemetry/examples/timer_composition.rs` | non-prod:example | 0 | 89 | - | - |
| `core/crates/layerfs-telemetry/examples/timer_nested.rs` | non-prod:example | 0 | 102 | - | - |
| `core/crates/layerfs-telemetry/src/lib.rs` | production | 3 | 16 | 3 | 0 |
| `core/crates/layerfs-telemetry/src/timer/format.rs` | production | 69 | 82 | 69 | 0 |
| `core/crates/layerfs-telemetry/src/timer/json.rs` | production | 115 | 136 | 115 | 0 |
| `core/crates/layerfs-telemetry/src/timer/mod.rs` | production | 8 | 25 | 8 | 0 |
| `core/crates/layerfs-telemetry/src/timer/recording.rs` | production | 232 | 291 | 232 | 0 |
| `core/crates/layerfs-telemetry/src/timer/report.rs` | production | 170 | 249 | 170 | 0 |
| `core/crates/layerfs-telemetry/src/timer/scope.rs` | production | 135 | 206 | 135 | 0 |
| `core/crates/layerfs-telemetry/tests/compile_fail/attach_on_pending_scope.rs` | non-prod:test | 0 | 11 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/child_on_pending_scope.rs` | non-prod:test | 0 | 11 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/control_injected_scopes.rs` | non-prod:test | 0 | 37 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/escape_child_scope.rs` | non-prod:test | 0 | 15 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/run_active_handle.rs` | non-prod:test | 0 | 10 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/run_scope_twice.rs` | non-prod:test | 0 | 12 | - | - |
| `core/crates/layerfs-telemetry/tests/compile_fail/send_scope_across_threads.rs` | non-prod:test | 0 | 17 | - | - |
| `core/crates/layerfs-telemetry/tests/timer.rs` | non-prod:test | 0 | 542 | - | - |
| `core/crates/layerfs-telemetry/tests/timer_compile_fail.rs` | non-prod:test | 0 | 146 | - | - |
| `core/crates/layerfs-telemetry/tests/timer_format.rs` | non-prod:test | 0 | 123 | - | - |
| `core/crates/layerfs-telemetry/tests/timer_json.rs` | non-prod:test | 0 | 339 | - | - |

Only 75 of the 139 files are production: 74 Rust files under src/ plus
core/crates/layerfs-storage/sql/schema.sql. The 64 non-production files are 46 test files
(10993 physical lines), 7 example files (1804), 3 Cargo.toml (32), 4 markdown docs (753) and
4 ignored .DS_Store (14). No file under core/crates is unclassified.

### 2.1 Physical lines of every lib.rs and mod.rs (200-line ceiling)

| entry file | production LOC | physical | <= 200? |
| --- | ---: | ---: | --- |
| core/crates/layerfs-content/src/lib.rs | 24 | 41 | yes (largest entry file) |
| core/crates/layerfs-storage/src/lib.rs | 11 | 30 | yes |
| core/crates/layerfs-content/src/object/mod.rs | 23 | 29 | yes |
| core/crates/layerfs-telemetry/src/timer/mod.rs | 8 | 25 | yes |
| core/crates/layerfs-content/src/file/mod.rs | 17 | 23 | yes |
| core/crates/layerfs-content/src/file/edit/mod.rs | 15 | 20 | yes |
| core/crates/layerfs-content/src/file/mapping/mod.rs | 15 | 20 | yes |
| core/crates/layerfs-storage/src/encoding/mod.rs | 11 | 17 | yes |
| core/crates/layerfs-storage/src/pack/mod.rs | 12 | 17 | yes |
| core/crates/layerfs-storage/src/cas/mod.rs | 11 | 16 | yes |
| core/crates/layerfs-telemetry/src/lib.rs | 3 | 16 | yes |
| core/crates/layerfs-storage/src/sqlite/mod.rs | 10 | 15 | yes |
| core/crates/layerfs-storage/src/encoding/pool/mod.rs | 9 | 14 | yes |
| core/crates/layerfs-content/src/file/cdc/mod.rs | 5 | 10 | yes |
| core/crates/layerfs-storage/src/encoding/delta/mod.rs | 4 | 8 | yes |

(For completeness: the only lib.rs/mod.rs over 200 physical lines anywhere under core/crates are
core/crates/layerfs-content/tests/support/mod.rs (534) and
core/crates/layerfs-storage/tests/support/mod.rs (323). Both are non-production test helpers; the
ceiling in core/AGENTS.md applies to product files.)

---

## 3. Recursive directory totals (production LOC)

| directory (parents include children) | before (pre-Stage-3) | after (HEAD) | delta | plan range | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| core/crates/layerfs-content/src | 2336 | 4385 | +2049 | 3632-5522 | within |
| core/crates/layerfs-content/src/file | 1761 | 3366 | +1605 | 2789-4179 | within |
| core/crates/layerfs-content/src/file/cdc | 499 | 499 | 0 | 499 | within |
| core/crates/layerfs-content/src/file/edit | 0 | 1391 | +1391 | 968-1646 | within |
| core/crates/layerfs-content/src/file/mapping | 946 | 1011 | +65 | 978-1460 | within (33 above the low bound) |
| core/crates/layerfs-content/src/object | 337 | 739 | +402 | 555-868 | within |
| core/crates/layerfs-storage/src | 3038 | 5764 | +2726 | 5008-8578 | within |
| core/crates/layerfs-storage/src/cas | 994 | 1410 | +416 | 1074-1874 | within |
| core/crates/layerfs-storage/src/encoding | 591 | 2587 | +1996 | 2096-3602 | within |
| core/crates/layerfs-storage/src/encoding/delta | 0 | 795 | +795 | 538-956 | within |
| core/crates/layerfs-storage/src/encoding/pool | 0 | 926 | +926 | 768-1336 | within |
| core/crates/layerfs-storage/src/pack | 649 | 739 | +90 | 794-1344 | BELOW (55 under the low bound) |
| core/crates/layerfs-storage/src/sqlite | 540 | 702 | +162 | 730-1230 | BELOW (28 under the low bound) |
| core/crates/layerfs-storage/sql | 46 | 48 | +2 | 80-130 | BELOW (32 under the low bound) |
| core/crates/layerfs-telemetry/src | 732 | 732 | 0 | outside the plan | n/a |
| C1 (layerfs-content/src) | 2336 | 4385 | +2049 | 3632-5522 | within |
| C2 (layerfs-storage/src + sql) | 3084 | 5812 | +2728 | 5088-8708 | within |
| C1 + C2 | 5420 | 10197 | +4777 | 8720-14230 | within |
| reference crates/ | 68476 | 68476 | 0 | retirement is later | coexists, not removed (and see F1/F2) |
| combined (core + reference) | 74628 | 79405 | +4777 | n/a | n/a |

Note on the last two rows: 68476 is the counter's raw reference number, which F1/F2 show is not a
valid production count. The genuinely audited product delta for this batch is the core figure:
6152 -> 10929, +4777.

Independent confirmation of the implementer's directory table: every "after" value above equals the
corresponding row of stages-3-4-report.md:131-144 exactly (4385 / 3366 / 1391 / 1011 / 739 /
5764 / 2587 / 795 / 926 / 1410 / 739 / 702 / 48 / 732). **VERIFIED-CORRECT.**

---

## 4. Annotated tree

Full annotated tree of core/crates with production LOC / physical lines per production file and an
explicit non-production label for every other file: see the companion **loc-tree.md** (identical
content reproduced below). Core/crates/.DS_Store and the three nested .DS_Store files are ignored
macOS junk, not tracked.

~~~text
core/crates/
├── layerfs-content/
│   ├── examples/
│   │   ├── edit_timing_c1.rs  [non-production:example (180 phys)]
│   │   └── fingerprint_collision_search.rs  [non-production:example (175 phys)]
│   ├── src/
│   │   ├── file/
│   │   │   ├── cdc/
│   │   │   │   ├── gear.rs  [ 494 prod /  538 phys]
│   │   │   │   └── mod.rs  [   5 prod /   10 phys]
│   │   │   ├── edit/
│   │   │   │   ├── apply.rs  [ 391 prod /  447 phys]
│   │   │   │   ├── compare.rs  [  67 prod /   86 phys]
│   │   │   │   ├── concat.rs  [  19 prod /   29 phys]
│   │   │   │   ├── finish.rs  [  38 prod /   54 phys]
│   │   │   │   ├── input.rs  [ 286 prod /  390 phys]
│   │   │   │   ├── mod.rs  [  15 prod /   20 phys]
│   │   │   │   ├── split.rs  [  23 prod /   32 phys]
│   │   │   │   └── tree.rs  [ 552 prod /  645 phys]
│   │   │   ├── mapping/
│   │   │   │   ├── build.rs  [ 301 prod /  371 phys]
│   │   │   │   ├── codec.rs  [ 303 prod /  334 phys]
│   │   │   │   ├── mod.rs  [  15 prod /   20 phys]
│   │   │   │   ├── read.rs  [ 210 prod /  241 phys]
│   │   │   │   └── types.rs  [ 182 prod /  248 phys]
│   │   │   ├── .DS_Store  [non-production:junk (5 phys)]
│   │   │   ├── content.rs  [ 195 prod /  249 phys]
│   │   │   ├── mod.rs  [  17 prod /   23 phys]
│   │   │   ├── read.rs  [ 111 prod /  127 phys]
│   │   │   └── view.rs  [ 142 prod /  171 phys]
│   │   ├── object/
│   │   │   ├── access.rs  [  18 prod /   36 phys]
│   │   │   ├── codec.rs  [ 107 prod /  138 phys]
│   │   │   ├── id.rs  [  89 prod /  119 phys]
│   │   │   ├── inode_leaf.rs  [ 311 prod /  397 phys]
│   │   │   ├── mod.rs  [  23 prod /   29 phys]
│   │   │   ├── output.rs  [ 124 prod /  192 phys]
│   │   │   └── predecessor.rs  [  67 prod /  107 phys]
│   │   ├── .DS_Store  [non-production:junk (3 phys)]
│   │   ├── error.rs  [ 120 prod /  174 phys]
│   │   ├── lib.rs  [  24 prod /   41 phys]
│   │   └── policy.rs  [ 136 prod /  226 phys]
│   ├── tests/
│   │   ├── support/
│   │   │   └── mod.rs  [non-production:test (534 phys)]
│   │   ├── edit_batch.rs  [non-production:test (217 phys)]
│   │   ├── edit_bounds.rs  [non-production:test (398 phys)]
│   │   ├── edit_localized.rs  [non-production:test (610 phys)]
│   │   ├── edit_model.rs  [non-production:test (233 phys)]
│   │   ├── edit_noop.rs  [non-production:test (239 phys)]
│   │   ├── edit_reference.rs  [non-production:test (615 phys)]
│   │   ├── edit_single.rs  [non-production:test (281 phys)]
│   │   ├── edit_timing.rs  [non-production:test (202 phys)]
│   │   ├── edit_transitions.rs  [non-production:test (265 phys)]
│   │   ├── file_complete.rs  [non-production:test (328 phys)]
│   │   ├── file_read.rs  [non-production:test (206 phys)]
│   │   ├── inode_leaf.rs  [non-production:test (174 phys)]
│   │   ├── object_identity.rs  [non-production:test (303 phys)]
│   │   ├── streaming.rs  [non-production:test (147 phys)]
│   │   └── timing.rs  [non-production:test (239 phys)]
│   ├── .DS_Store  [non-production:junk (3 phys)]
│   ├── Cargo.toml  [non-production:manifest (11 phys)]
│   └── README.md  [non-production:docs (88 phys)]
├── layerfs-storage/
│   ├── examples/
│   │   ├── measure_components.rs  [non-production:example (354 phys)]
│   │   ├── measure_edits.rs  [non-production:example (582 phys)]
│   │   └── measure_pooled.rs  [non-production:example (322 phys)]
│   ├── sql/
│   │   └── schema.sql  [  48 prod /   69 phys]
│   ├── src/
│   │   ├── cas/
│   │   │   ├── batch.rs  [  58 prod /   87 phys]
│   │   │   ├── dependencies.rs  [  62 prod /   88 phys]
│   │   │   ├── finish.rs  [  16 prod /   25 phys]
│   │   │   ├── membership.rs  [  32 prod /   47 phys]
│   │   │   ├── mod.rs  [  11 prod /   16 phys]
│   │   │   ├── owner.rs  [ 710 prod /  890 phys]
│   │   │   ├── read.rs  [  74 prod /  101 phys]
│   │   │   ├── save.rs  [  52 prod /   73 phys]
│   │   │   └── store.rs  [ 395 prod /  530 phys]
│   │   ├── encoding/
│   │   │   ├── delta/
│   │   │   │   ├── candidates.rs  [ 126 prod /  161 phys]
│   │   │   │   ├── mod.rs  [   4 prod /    8 phys]
│   │   │   │   ├── read.rs  [ 202 prod /  261 phys]
│   │   │   │   ├── record.rs  [ 201 prod /  240 phys]
│   │   │   │   └── select.rs  [ 262 prod /  343 phys]
│   │   │   ├── pool/
│   │   │   │   ├── delta.rs  [ 262 prod /  289 phys]
│   │   │   │   ├── index.rs  [ 203 prod /  259 phys]
│   │   │   │   ├── leaf.rs  [ 118 prod /  161 phys]
│   │   │   │   ├── mod.rs  [   9 prod /   14 phys]
│   │   │   │   ├── read.rs  [ 255 prod /  294 phys]
│   │   │   │   └── value_group.rs  [  79 prod /  103 phys]
│   │   │   ├── codec.rs  [ 508 prod /  632 phys]
│   │   │   ├── decode.rs  [ 170 prod /  192 phys]
│   │   │   ├── full.rs  [ 177 prod /  213 phys]
│   │   │   └── mod.rs  [  11 prod /   17 phys]
│   │   ├── pack/
│   │   │   ├── assemble.rs  [ 223 prod /  252 phys]
│   │   │   ├── layout.rs  [ 381 prod /  466 phys]
│   │   │   ├── mod.rs  [  12 prod /   17 phys]
│   │   │   └── placement.rs  [ 123 prod /  159 phys]
│   │   ├── sqlite/
│   │   │   ├── cleanup.rs  [  58 prod /   77 phys]
│   │   │   ├── connection.rs  [  50 prod /   72 phys]
│   │   │   ├── lookup.rs  [ 141 prod /  177 phys]
│   │   │   ├── mod.rs  [  10 prod /   15 phys]
│   │   │   ├── pool.rs  [ 128 prod /  160 phys]
│   │   │   ├── schema.rs  [ 232 prod /  267 phys]
│   │   │   └── write.rs  [  83 prod /  117 phys]
│   │   ├── error.rs  [ 105 prod /  154 phys]
│   │   ├── lib.rs  [  11 prod /   30 phys]
│   │   └── policy.rs  [ 210 prod /  332 phys]
│   ├── tests/
│   │   ├── support/
│   │   │   └── mod.rs  [non-production:test (323 phys)]
│   │   ├── cas_reuse.rs  [non-production:test (265 phys)]
│   │   ├── cas_roundtrip.rs  [non-production:test (141 phys)]
│   │   ├── core_pipeline.rs  [non-production:test (268 phys)]
│   │   ├── delta_chains.rs  [non-production:test (269 phys)]
│   │   ├── delta_payload.rs  [non-production:test (286 phys)]
│   │   ├── edit_pipeline.rs  [non-production:test (228 phys)]
│   │   ├── memory_bounds.rs  [non-production:test (250 phys)]
│   │   ├── metadata_chain.rs  [non-production:test (231 phys)]
│   │   ├── metadata_fingerprint_collision.rs  [non-production:test (152 phys)]
│   │   ├── metadata_pool.rs  [non-production:test (345 phys)]
│   │   ├── metadata_pool_index.rs  [non-production:test (273 phys)]
│   │   ├── metadata_window.rs  [non-production:test (263 phys)]
│   │   ├── pack_locator.rs  [non-production:test (299 phys)]
│   │   ├── persistence_failure.rs  [non-production:test (277 phys)]
│   │   ├── physical_formats.rs  [non-production:test (136 phys)]
│   │   ├── policy_capacity.rs  [non-production:test (264 phys)]
│   │   ├── timing.rs  [non-production:test (175 phys)]
│   │   └── visibility.rs  [non-production:test (294 phys)]
│   ├── Cargo.toml  [non-production:manifest (14 phys)]
│   └── README.md  [non-production:docs (146 phys)]
├── layerfs-telemetry/
│   ├── examples/
│   │   ├── timer_composition.rs  [non-production:example (89 phys)]
│   │   └── timer_nested.rs  [non-production:example (102 phys)]
│   ├── src/
│   │   ├── timer/
│   │   │   ├── format.rs  [  69 prod /   82 phys]
│   │   │   ├── json.rs  [ 115 prod /  136 phys]
│   │   │   ├── mod.rs  [   8 prod /   25 phys]
│   │   │   ├── recording.rs  [ 232 prod /  291 phys]
│   │   │   ├── report.rs  [ 170 prod /  249 phys]
│   │   │   └── scope.rs  [ 135 prod /  206 phys]
│   │   └── lib.rs  [   3 prod /   16 phys]
│   ├── tests/
│   │   ├── compile_fail/
│   │   │   ├── attach_on_pending_scope.rs  [non-production:test (11 phys)]
│   │   │   ├── child_on_pending_scope.rs  [non-production:test (11 phys)]
│   │   │   ├── control_injected_scopes.rs  [non-production:test (37 phys)]
│   │   │   ├── escape_child_scope.rs  [non-production:test (15 phys)]
│   │   │   ├── run_active_handle.rs  [non-production:test (10 phys)]
│   │   │   ├── run_scope_twice.rs  [non-production:test (12 phys)]
│   │   │   └── send_scope_across_threads.rs  [non-production:test (17 phys)]
│   │   ├── timer.rs  [non-production:test (542 phys)]
│   │   ├── timer_compile_fail.rs  [non-production:test (146 phys)]
│   │   ├── timer_format.rs  [non-production:test (123 phys)]
│   │   └── timer_json.rs  [non-production:test (339 phys)]
│   ├── Cargo.toml  [non-production:manifest (7 phys)]
│   ├── README.md  [non-production:docs (216 phys)]
│   └── USAGE.md  [non-production:docs (303 phys)]
└── .DS_Store  [non-production:junk (3 phys)]
~~~

---

## 5. Before / after / delta

### 5a. The real pre-Stage-3 implementation base

Commands:

~~~sh
git log --oneline 5e8b8cbc2..HEAD
git log --format='%H %P %s' 5e8b8cbc2..HEAD
for c in $(git log --reverse --format=%H 5e8b8cbc2..HEAD); do git show --stat --name-only $c; done
git diff --stat 5e8b8cbc2 c38961f2f -- core/crates      # empty
~~~

The range 5e8b8cbc2..HEAD holds 17 commits. The **first** commit, c38961f2f, is docs-only
(1 file, +341: docs/roadmap/0.1/0.1.7/study/radish/prompt-architecture-overview.md). The first
commit that changes product source is:

| item | value |
| --- | --- |
| first product-source commit | 64e3f9d6a2ce83f56beacb41b3b3828270d4831b "storage/content: implement Stages 3-4 physical encoding and localized edits" |
| its first parent = **pre-Stage-3 implementation base** | **c38961f2f4bedbc7afe8826c5d404c6d21b91657**, tree 61a57a19c80ae4dc5527a42d151f1a948d155d9a |
| planning baseline named in the file plan | 5e8b8cbc26e15180daf0ad4dbf3fd40901145811, tree b181f9f91d75de28449398da485b3ecba58a24ce |
| are they equivalent for this audit? | yes for product source: "git diff --stat 5e8b8cbc2 c38961f2f -- core/crates" is empty, so both carry core 6152 / reference 68476 |
| HEAD | 91c3a0741fff64e8161d5c1b6e759f347ffbf757, tree 4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9 |

Both snapshots were materialized with git archive and counted with the same command:

~~~sh
git archive <rev> core/crates | tar -x -C <tmp>
python3 tools/production_loc.py --root <tmp> --detail
~~~

Raw comparison (per revision, cached by tree id):

~~~text
REV PRE-BASE c38961f2f tree=61a57a19c80ae4dc5527a42d151f1a948d155d9a :: core 6152 in 52 files
   (layerfs-content 2336, layerfs-storage 3084, layerfs-telemetry 732)
REV AFTER:91c3a0741 tree=4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9 :: core 10929 in 75 files
   (layerfs-content 4385, layerfs-storage 5812, layerfs-telemetry 732)
DELTA core +4777 (content +2049, storage +2728, telemetry 0)
~~~

The 5e8b8cbc2 planning-baseline numbers quoted in the file plan (C1 2336, C2 3084 including 46
runtime SQL LOC, "combined 5,420 across 45 files", telemetry 732) reproduce exactly in the audited
pre-Stage-3 snapshot: 52 core files = 45 C1+C2 files + 7 telemetry files, 4385-2336 and 5812-3084
above. **CONSISTENT** (the plan's 45 is the C1+C2 subset, which is exactly what it claims to be).

### 5b. Per-file and per-directory before/after

Per-file before/after/delta for all 75 production files is in the section 2 table and in
loc-before-after.csv; per-directory deltas are in section 3. Files that changed or appeared:

~~~text
changed     116 ->   120  (+4)    core/crates/layerfs-content/src/error.rs
added         0 ->   391  (+391)  core/crates/layerfs-content/src/file/edit/apply.rs
added         0 ->    67  (+67)   core/crates/layerfs-content/src/file/edit/compare.rs
added         0 ->    19  (+19)   core/crates/layerfs-content/src/file/edit/concat.rs
added         0 ->    38  (+38)   core/crates/layerfs-content/src/file/edit/finish.rs
added         0 ->   286  (+286)  core/crates/layerfs-content/src/file/edit/input.rs
added         0 ->    15  (+15)   core/crates/layerfs-content/src/file/edit/mod.rs
added         0 ->    23  (+23)   core/crates/layerfs-content/src/file/edit/split.rs
added         0 ->   552  (+552)  core/crates/layerfs-content/src/file/edit/tree.rs
changed     236 ->   301  (+65)   core/crates/layerfs-content/src/file/mapping/build.rs
changed      10 ->    17  (+7)    core/crates/layerfs-content/src/file/mod.rs
added         0 ->   142  (+142)  core/crates/layerfs-content/src/file/view.rs
changed      16 ->    24  (+8)    core/crates/layerfs-content/src/lib.rs
added         0 ->   311  (+311)  core/crates/layerfs-content/src/object/inode_leaf.rs
changed      12 ->    23  (+11)   core/crates/layerfs-content/src/object/mod.rs
changed     111 ->   124  (+13)   core/crates/layerfs-content/src/object/output.rs
added         0 ->    67  (+67)   core/crates/layerfs-content/src/object/predecessor.rs
changed     106 ->   136  (+30)   core/crates/layerfs-content/src/policy.rs
changed      46 ->    48  (+2)    core/crates/layerfs-storage/sql/schema.sql
changed      42 ->    32  (-10)   core/crates/layerfs-storage/src/cas/membership.rs
changed     364 ->   710  (+346)  core/crates/layerfs-storage/src/cas/owner.rs
changed      65 ->    74  (+9)    core/crates/layerfs-storage/src/cas/read.rs
changed      49 ->    52  (+3)    core/crates/layerfs-storage/src/cas/save.rs
changed     327 ->   395  (+68)   core/crates/layerfs-storage/src/cas/store.rs
changed     367 ->   508  (+141)  core/crates/layerfs-storage/src/encoding/codec.rs
changed     103 ->   170  (+67)   core/crates/layerfs-storage/src/encoding/decode.rs
added         0 ->   126  (+126)  core/crates/layerfs-storage/src/encoding/delta/candidates.rs
added         0 ->     4  (+4)    core/crates/layerfs-storage/src/encoding/delta/mod.rs
added         0 ->   202  (+202)  core/crates/layerfs-storage/src/encoding/delta/read.rs
added         0 ->   201  (+201)  core/crates/layerfs-storage/src/encoding/delta/record.rs
added         0 ->   262  (+262)  core/crates/layerfs-storage/src/encoding/delta/select.rs
changed     112 ->   177  (+65)   core/crates/layerfs-storage/src/encoding/full.rs
changed       9 ->    11  (+2)    core/crates/layerfs-storage/src/encoding/mod.rs
added         0 ->   262  (+262)  core/crates/layerfs-storage/src/encoding/pool/delta.rs
added         0 ->   203  (+203)  core/crates/layerfs-storage/src/encoding/pool/index.rs
added         0 ->   118  (+118)  core/crates/layerfs-storage/src/encoding/pool/leaf.rs
added         0 ->     9  (+9)    core/crates/layerfs-storage/src/encoding/pool/mod.rs
added         0 ->   255  (+255)  core/crates/layerfs-storage/src/encoding/pool/read.rs
added         0 ->    79  (+79)   core/crates/layerfs-storage/src/encoding/pool/value_group.rs
changed     195 ->   223  (+28)   core/crates/layerfs-storage/src/pack/assemble.rs
changed     321 ->   381  (+60)   core/crates/layerfs-storage/src/pack/layout.rs
changed      10 ->    12  (+2)    core/crates/layerfs-storage/src/pack/mod.rs
changed     148 ->   210  (+62)   core/crates/layerfs-storage/src/policy.rs
changed     118 ->   141  (+23)   core/crates/layerfs-storage/src/sqlite/lookup.rs
changed       8 ->    10  (+2)    core/crates/layerfs-storage/src/sqlite/mod.rs
added         0 ->   128  (+128)  core/crates/layerfs-storage/src/sqlite/pool.rs
changed     223 ->   232  (+9)    core/crates/layerfs-storage/src/sqlite/schema.rs
~~~

Two observations for the acceptance lane. (1) file/edit/frontier.rs existed only inside the batch:
+95 production LOC at 64e3f9d6a, removed at 01d9f70f3; it is absent from both endpoints, which is
why it does not appear in the table. (2) cas/membership.rs is the only production file that
**shrank** in a delivery whose plan expected it to grow to 70-140 (42 -> 32, -10); it is a
refactor, not a relocation, and nothing in the report explains it.

### 5c. Per-commit first-parent accounting

Method for every commit: materialize the first-parent tree and the committed tree with
git archive, run the same counter on both, and read the disclosure out of the commit message:

~~~sh
for c in $(git log --reverse --format=%H 5e8b8cbc2..HEAD); do
  git archive $c^ core/crates | tar -x -C $TMP/parent
  git archive $c  core/crates | tar -x -C $TMP/commit
  python3 tools/production_loc.py --root $TMP/parent --detail
  python3 tools/production_loc.py --root $TMP/commit --detail
  git log -1 --format=%B $c | grep 'Production LOC'
done
~~~

The reference scope contributes 68476 to every "combined" figure: it is genuinely unchanged across
the range. Four commits touch crates/ (b49931570, 01d9f70f3, c99192b16, fba18606e), but only
crates/layerfs-content/examples/*.rs, which the counter excludes by design; the reference scope
counts 68476 in 206 files at both 44cf7484 and HEAD.

| commit | subject | committed core (audited) | combined (audited) | value disclosed in the message | verdict |
| --- | --- | ---: | ---: | --- | --- |
| `c38961f2f` | docs: add the Radish architecture-overview study prompt | 6152 | 74628 | `74628 -> 74628 (delta 0)` | MATCH |
| `64e3f9d6a` | storage/content: implement Stages 3-4 physical encoding and localized edits | 8660 | 77136 | `74628 -> 77136 (delta +2508)` | MATCH - per-package subtotals also match |
| `0c548c761` | docs(core): state the Stages 3-4 supported ranges and honest limits | 8660 | 77136 | `77136 -> 77136 (delta 0)` | MATCH |
| `24ef187d4` | style: apply rustfmt to the measure_edits example | 8660 | 77136 | `77136 -> 77136 (delta 0)` | MATCH |
| `da8ee5769` | storage/content: implement pooled physical metadata (Stage 3 checkpoint E) | 10399 | 78875 | `77136 -> 78875 (delta +1739)` | HEADLINE MATCH / SUBTOTAL MISMATCH - discloses layerfs-content 3949 and layerfs-storage 5718; audited 3891 / 5776 (58 lines misallocated between packages) |
| `b49931570` | content: establish the sealed reference edit oracle and fix chunked EOF append | 10415 | 78891 | `78875 -> 78891 (delta +16)` | HEADLINE MATCH / SUBTOTAL MISMATCH - discloses layerfs-content 3965; audited 3907 (same 58-line error carried forward) |
| `315a339fa` | content: add the required edit_model target and record it | 10415 | 78891 | `78891 -> 78891 (delta 0)` | MATCH |
| `2b2dbc028` | docs(core): correct the per-package LOC subtotals and record the core breakdown | 10415 | 78891 | `78891 -> 78891 (delta 0)` | MATCH - self-corrects the two misallocations above |
| `01d9f70f3` | content: replace the mapping rebuild with stored-tree copy-on-write edits | 10893 | 79369 | `10415 -> 10893 (delta +478)` | MATCH (headline scope switches to core-only) - body still gives combined 78891 -> 79369 |
| `5a0fef716` | storage: bound pooled metadata chains at admission and cover the window | 10929 | 79405 | `10893 -> 10929 (delta +36)` | MATCH - body still gives combined 79369 -> 79405 |
| `c255dcfaf` | docs: declare the Stages 3-4 measurement round before collecting it | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `dfd54fd8e` | tools: print the pooled policy and readback identities before collection | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `c99192b16` | content: prove the 80+100 join repartitions to exactly 90+90 | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `fba18606e` | docs: record the matched C1 pair and leave the qualification gates open | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `65f2a402d` | tools: apply rustfmt to the candidate matched-pair example | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `275c81ab7` | docs: commit the in-flight owner Stage 3-4 instruction documents | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |
| `91c3a0741` | docs: state the Stages 3-4 implementation-complete status | 10929 | 79405 | `10929 -> 10929 (delta 0)` | MATCH |

Verdicts: **17/17 commits carry a disclosure**; **15/17 match the audited numbers exactly**;
2 carry a correct headline with a 58-line per-package misallocation (F6). One scope discontinuity
at 01d9f70f3 (F5).

### 5d. v0.1.6 reference baseline, labelled

~~~sh
git archive 44cf748486863ab7c21ca47e731bd88e2b9a7b4a crates | tar -x -C $TMP/ref
python3 tools/production_loc.py --root $TMP/ref --detail
~~~

~~~text
REF 44cf748486863ab7c21ca47e731bd88e2b9a7b4a tree=3e022b48a31e06d4e072950c4396f0b2c70d56dd
    reference 68476 production lines in 206 files
REF HEAD tree=4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9
    reference 68476 production lines in 206 files
~~~

| scope | tree | counter value | status |
| --- | --- | ---: | --- |
| v0.1.6 reference (crates/) | 3e022b48a31e06d4e072950c4396f0b2c70d56dd | 68476 (206 files) | **coexistence** - retained, untouched, never a candidate dependency |
| current reference (crates/) | 4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9 | 68476 (206 files) | identical; delta 0 across the whole batch |
| replacement product (core/) | 4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9 | 10929 (75 files) | the deliverable of #168/#169 |
| combined | - | 79405 | reported for migration honesty only |

**Labelling:** the batch is **coexistence, not removal** - correct per AGENTS.md ("report their
production totals separately as well as the combined total"). No reference file was deleted and no
reference production file changed; the four crates/ commits add or edit development examples under
crates/layerfs-content/examples/, which are non-production by the counter's classification and by
the rule. The two commits that touch the reference tree for the oracle
(crates/layerfs-content/examples/rope_edit_oracle.rs, rope_edit_timing.rs) are examples, so the
"reference 68476 unchanged" claim is directionally right - it is the 68476 value itself that is
defective (F1/F2), not the claim that it did not move.

---

## 6. Caps

Command: per-file physical lines from the counter's physical_lines, cross-checked with wc -l for all
281 counted files (0 differences).

| cap | rule | result |
| --- | --- | --- |
| production file <= 999 physical lines | core/AGENTS.md | **PASS** - no product file exceeds 900; largest cas/owner.rs 890 physical (710 prod), then file/edit/tree.rs 645, encoding/codec.rs 632, file/cdc/gear.rs 538, cas/store.rs 530 |
| lib.rs / mod.rs <= 200 physical lines | core/AGENTS.md | **PASS** - largest product entry file is layerfs-content/src/lib.rs at 41 physical; all 15 product entry files listed in section 2.1 |
| runtime SQL counted under the same cap | core/AGENTS.md + core/tools/check_product_boundary.py:21,53-54 | **PASS** - schema.sql 69 physical |

Nothing is flagged, so the "read the file and judge whether it is declarations/reexports only"
step has no trigger file. For completeness I read the two largest entry files anyway:
layerfs-content/src/lib.rs (41 physical) is crate attributes plus pub mod declarations and
pub use reexports - **declarations and reexports only**; layerfs-storage/src/lib.rs (30 physical) is
the same - **declarations and reexports only**. No delegation logic hides in either. The same
scanner is what the boundary guard applies (check_product_boundary.py:21 sets 200 for entry files,
999 otherwise, and :53-54 restricts the scan to .rs/.sql under src/ or package sql/), so guard and
review agree on scope.

---

## 7. File-plan comparison

Plan: stages-3-4-file-plan.md sections 2 and 3. Machine-readable copy:
loc-file-plan-comparison.csv (75 plan rows with verdicts). Verdicts: **36 within, 25 below,
5 above, 8 absent, 1 not retired**.

| plan line | planned path | action | planned before | recommended range | actual prod LOC | physical | verdict |
| ---: | --- | --- | ---: | ---: | ---: | ---: | --- |
| 117 | `layerfs-content/src/file/edit/frontier.rs` | New | 0 | 180-300 | 0 | 0 | ABSENT |
| 112 | `layerfs-content/src/file/mapping/predecessor.rs` | New | 0 | 100-180 | 0 | 0 | ABSENT |
| 208 | `layerfs-storage/src/encoding/codec/decode.rs` | New | 0 | 260-420 | 0 | 0 | ABSENT |
| 207 | `layerfs-storage/src/encoding/codec/encode.rs` | New | 0 | 220-340 | 0 | 0 | ABSENT |
| 205 | `layerfs-storage/src/encoding/codec/mod.rs` | New | 0 | 8-16 | 0 | 0 | ABSENT |
| 206 | `layerfs-storage/src/encoding/codec/profile.rs` | New | 0 | 80-140 | 0 | 0 | ABSENT |
| 224 | `layerfs-storage/src/pack/read.rs` | New | 0 | 100-190 | 0 | 0 | ABSENT |
| 225 | `layerfs-storage/src/pack/singleton.rs` | New | 0 | 90-160 | 0 | 0 | ABSENT |
| 204 | `layerfs-storage/src/encoding/codec.rs` | Retire | 367 | 0-0 | 508 | 632 | NOT RETIRED |
| 115 | `layerfs-content/src/file/edit/apply.rs` | New | 0 | 130-230 | 391 | 447 | ABOVE |
| 114 | `layerfs-content/src/file/edit/input.rs` | New | 0 | 90-160 | 286 | 390 | ABOVE |
| 100 | `layerfs-content/src/object/inode_leaf.rs` | New | 0 | 160-260 | 311 | 397 | ABOVE |
| 194 | `layerfs-storage/src/cas/owner.rs` | Update | 364 | 320-520 | 710 | 890 | ABOVE |
| 210 | `layerfs-storage/src/encoding/delta/record.rs` | New | 0 | 100-180 | 201 | 240 | ABOVE |
| 116 | `layerfs-content/src/file/edit/compare.rs` | New | 0 | 80-140 | 67 | 86 | below |
| 119 | `layerfs-content/src/file/edit/concat.rs` | New | 0 | 210-350 | 19 | 29 | below |
| 120 | `layerfs-content/src/file/edit/finish.rs` | New | 0 | 110-190 | 38 | 54 | below |
| 118 | `layerfs-content/src/file/edit/split.rs` | New | 0 | 160-260 | 23 | 32 | below |
| 107 | `layerfs-content/src/file/mapping/mod.rs` | Update | 15 | 18-30 | 15 | 20 | below |
| 97 | `layerfs-content/src/object/access.rs` | Update | 18 | 40-90 | 18 | 36 | below |
| 93 | `layerfs-content/src/policy.rs` | Update | 106 | 150-250 | 136 | 226 | below |
| 233 | `layerfs-storage/sql/schema.sql` | Update | 46 | 80-130 | 48 | 69 | below |
| 195 | `layerfs-storage/src/cas/batch.rs` | Update | 58 | 90-160 | 58 | 87 | below |
| 198 | `layerfs-storage/src/cas/dependencies.rs` | Update | 62 | 90-170 | 62 | 88 | below |
| 200 | `layerfs-storage/src/cas/finish.rs` | Update | 16 | 20-50 | 16 | 25 | below |
| 197 | `layerfs-storage/src/cas/membership.rs` | Update | 42 | 70-140 | 32 | 47 | below |
| 192 | `layerfs-storage/src/cas/mod.rs` | Update | 11 | 14-24 | 11 | 16 | below |
| 199 | `layerfs-storage/src/cas/read.rs` | Update | 65 | 120-220 | 74 | 101 | below |
| 196 | `layerfs-storage/src/cas/save.rs` | Update | 49 | 90-170 | 52 | 73 | below |
| 209 | `layerfs-storage/src/encoding/delta/mod.rs` | New | 0 | 8-16 | 4 | 8 | below |
| 201 | `layerfs-storage/src/encoding/mod.rs` | Update | 9 | 12-24 | 11 | 17 | below |
| 217 | `layerfs-storage/src/encoding/pool/leaf.rs` | New | 0 | 140-240 | 118 | 161 | below |
| 215 | `layerfs-storage/src/encoding/pool/value_group.rs` | New | 0 | 140-240 | 79 | 103 | below |
| 190 | `layerfs-storage/src/error.rs` | Update | 105 | 120-200 | 105 | 154 | below |
| 189 | `layerfs-storage/src/lib.rs` | Update | 11 | 14-28 | 11 | 30 | below |
| 220 | `layerfs-storage/src/pack/mod.rs` | Update | 10 | 14-24 | 12 | 17 | below |
| 222 | `layerfs-storage/src/pack/placement.rs` | Update | 123 | 130-230 | 123 | 159 | below |
| 231 | `layerfs-storage/src/sqlite/cleanup.rs` | Update | 58 | 80-150 | 58 | 77 | below |
| 230 | `layerfs-storage/src/sqlite/write.rs` | Update | 83 | 120-220 | 83 | 117 | below |
| 92 | `layerfs-content/src/error.rs` | Update | 116 | 120-190 | 120 | 174 | within |
| 106 | `layerfs-content/src/file/cdc/gear.rs` | Retain | 494 | 494-494 | 494 | 538 | within |
| 105 | `layerfs-content/src/file/cdc/mod.rs` | Retain | 5 | 5-5 | 5 | 10 | within |
| 102 | `layerfs-content/src/file/content.rs` | Update | 195 | 150-230 | 195 | 249 | within |
| 113 | `layerfs-content/src/file/edit/mod.rs` | New | 0 | 8-16 | 15 | 20 | within |
| 110 | `layerfs-content/src/file/mapping/build.rs` | Update | 236 | 220-330 | 301 | 371 | within |
| 109 | `layerfs-content/src/file/mapping/codec.rs` | Update | 303 | 280-380 | 303 | 334 | within |
| 111 | `layerfs-content/src/file/mapping/read.rs` | Update | 210 | 180-290 | 210 | 241 | within |
| 108 | `layerfs-content/src/file/mapping/types.rs` | Update | 182 | 180-250 | 182 | 248 | within |
| 101 | `layerfs-content/src/file/mod.rs` | Update | 10 | 14-24 | 17 | 23 | within |
| 103 | `layerfs-content/src/file/read.rs` | Update | 111 | 90-160 | 111 | 127 | within |
| 104 | `layerfs-content/src/file/view.rs` | New | 0 | 90-160 | 142 | 171 | within |
| 91 | `layerfs-content/src/lib.rs` | Update | 16 | 18-35 | 24 | 41 | within |
| 96 | `layerfs-content/src/object/codec.rs` | Update | 107 | 107-150 | 107 | 138 | within |
| 95 | `layerfs-content/src/object/id.rs` | Retain | 89 | 89-89 | 89 | 119 | within |
| 94 | `layerfs-content/src/object/mod.rs` | Update | 12 | 14-24 | 23 | 29 | within |
| 98 | `layerfs-content/src/object/output.rs` | Update | 111 | 100-170 | 124 | 192 | within |
| 99 | `layerfs-content/src/object/predecessor.rs` | New | 0 | 45-85 | 67 | 107 | within |
| 193 | `layerfs-storage/src/cas/store.rs` | Update | 327 | 260-420 | 395 | 530 | within |
| 203 | `layerfs-storage/src/encoding/decode.rs` | Update | 103 | 100-180 | 170 | 192 | within |
| 213 | `layerfs-storage/src/encoding/delta/candidates.rs` | New | 0 | 100-180 | 126 | 161 | within |
| 212 | `layerfs-storage/src/encoding/delta/read.rs` | New | 0 | 170-300 | 202 | 261 | within |
| 211 | `layerfs-storage/src/encoding/delta/select.rs` | New | 0 | 160-280 | 262 | 343 | within |
| 202 | `layerfs-storage/src/encoding/full.rs` | Update | 112 | 110-190 | 177 | 213 | within |
| 218 | `layerfs-storage/src/encoding/pool/delta.rs` | New | 0 | 160-280 | 262 | 289 | within |
| 216 | `layerfs-storage/src/encoding/pool/index.rs` | New | 0 | 160-280 | 203 | 259 | within |
| 214 | `layerfs-storage/src/encoding/pool/mod.rs` | New | 0 | 8-16 | 9 | 14 | within |
| 219 | `layerfs-storage/src/encoding/pool/read.rs` | New | 0 | 160-280 | 255 | 294 | within |
| 223 | `layerfs-storage/src/pack/assemble.rs` | Update | 195 | 180-300 | 223 | 252 | within |
| 221 | `layerfs-storage/src/pack/layout.rs` | Update | 321 | 280-440 | 381 | 466 | within |
| 191 | `layerfs-storage/src/policy.rs` | Update | 148 | 180-300 | 210 | 332 | within |
| 227 | `layerfs-storage/src/sqlite/connection.rs` | Update | 50 | 50-80 | 50 | 72 | within |
| 229 | `layerfs-storage/src/sqlite/lookup.rs` | Update | 118 | 130-220 | 141 | 177 | within |
| 226 | `layerfs-storage/src/sqlite/mod.rs` | Update | 8 | 10-20 | 10 | 15 | within |
| 232 | `layerfs-storage/src/sqlite/pool.rs` | New | 0 | 120-200 | 128 | 160 | within |
| 228 | `layerfs-storage/src/sqlite/schema.rs` | Update | 223 | 220-340 | 232 | 267 | within |

### 7.1 Absent planned files and where their responsibility went

| planned file | plan range | status | responsibility today |
| --- | ---: | --- | --- |
| src/file/edit/frontier.rs | 180-300 | **deleted in-batch** | replaced by **file/edit/tree.rs** (stored-node split/concat/COW). Declared: stages-3-4-report.md:313 and commit 01d9f70f3. The report's "95 production lines" for the peak file is **VERIFIED** (counter on tree f3063af: frontier.rs 95 prod / 128 phys). |
| src/file/mapping/predecessor.rs | 100-180 | **absent** | the "reference cursor with original/current coordinate distinction" lives in **file/edit/input.rs:291-302** ("Lazy cursor that maps current-result coordinates onto base coordinates", base_cursor/result_cursor) - which is why input.rs is 286 prod, above its own 90-160 range. **Undeclared merge**: the report's relocation paragraph (stages-3-4-report.md:170-172) mentions only decode.rs and codec/. |
| src/encoding/codec/mod.rs, profile.rs, encode.rs, decode.rs | 8-16, 80-140, 220-340, 260-420 | **never created** | retained as the single **encoding/codec.rs** (508 prod, 632 physical, grew from 367). Declared: stages-3-4-report.md:172. |
| src/pack/read.rs | 100-190 | **absent** | grouped body/record view is **pack/layout.rs:291 group_view** (with ordinary_group_view :303 and whole_file_group_view :369), consumed by **encoding/decode.rs:40**. Present, merged. |
| src/pack/singleton.rs | 90-160 | **absent** | budget-checked singleton assembly is **pack/assemble.rs:126** (frame_group_bounded with SINGLETON_PACK_LIMIT) plus the v7 lane in **pack/layout.rs:38,110,241,269,351,437** and the capacity plan in **encoding/full.rs:194**. Present, merged. |

So: 2 of the 8 absent planned files have their responsibility genuinely missing-as-a-file but
present-in-code with a traceable owner (pack/read.rs, pack/singleton.rs); 1 is a declared
retirement deviation (codec/); 1 is an undeclared merge (mapping/predecessor.rs); 1 is a
declared deletion with a named replacement (frontier.rs). No planned responsibility is lost.

### 7.2 Unexpected additions

Exactly one production file at HEAD is not in the plan: **core/crates/layerfs-content/src/file/edit/tree.rs** (552 prod, 645 physical), the stored-node localized-edit algorithm. It is the
replacement for frontier.rs and is described in commit 01d9f70f3 and in the report. The plan
explicitly allows "a justified merge/split/rename ... report the changed map, actual counts and
reason" (stages-3-4-file-plan.md:8-11); the *map* change is reported, but the **recommended-range
comparison for it is not**, because the required per-file table does not exist (F8).

### 7.3 File-count compliance

| plan statement | planned | actual | verdict |
| --- | ---: | ---: | --- |
| final production files, C1 | 30 | 29 | 2 absent, 1 unplanned |
| final production files, C2 (including sql/schema.sql) | 44 | 39 | 6 absent, 0 unplanned |
| final production files, C1+C2 | 74 | 68 | -6 |
| telemetry (outside the plan) | - | 7 | unchanged |
| core production files total | - | **75** | matches check_product_boundary.py's "75 production files scanned" (stages-3-4-report.md:179) |

### 7.4 Files below the recommended range

25 of 75 rows are below. Most are small misses (mod.rs 1-4 lines under). The material ones are
concentrations of missing responsibility:

* **file/edit/{split,concat,finish}.rs**: 23 / 19 / 38 prod against 160-260 / 210-350 / 110-190 - a
  factor of 7-18 below. The join/coalesce/partition/root-collapse rules and the proven-finality
  rules the plan assigned here are largely implemented in **file/edit/tree.rs** (552 prod, 645
  phys) instead, which is the same consolidation as 7.2.
* **cas/**: batch 58 (90-160), save 52 (90-170), membership 32 (70-140), dependencies 62 (90-170),
  read 74 (120-220), finish 16 (20-50) - the coordination the plan spread over six files sits in
  **cas/owner.rs** (710 prod against a 320-520 range, +190 over) and **cas/store.rs** (395).
* **sql/schema.sql** 48 (80-130) and **sqlite/write.rs** 83 (120-220): the exact-roles/policy
  schema and the batched write helper are smaller than planned.
* **encoding/pool/value_group.rs** 79 (140-240) and **pool/leaf.rs** 118 (140-240): the pooled
  grammar is more compact than planned while **pool/delta.rs** (262) and **pool/read.rs** (255) are
  mid-range.

### 7.5 Files above the recommended range

| file | range | actual | over by | reading |
| --- | ---: | ---: | ---: | --- |
| encoding/delta/record.rs | 100-180 | 201 | +21 | minor |
| object/inode_leaf.rs | 160-260 | 311 | +51 | the checked 73-byte inode/value grammar plus the pooled physical layout in one file |
| file/edit/input.rs | 90-160 | 286 | +126 | absorbed the planned mapping/predecessor.rs cursor responsibility (7.1) |
| cas/owner.rs | 320-520 | 710 | +190 | absorbed the planned cas/ coordination split (7.4); 890 physical of 999 - the only file with little headroom |
| file/edit/apply.rs | 130-230 | 391 | +161 | dispatch plus the sequential edit operation and its retained-extent streaming |

None of these breaches a hard limit; the 999-physical ceiling still has 109 lines of headroom in
the worst case (cas/owner.rs). The pattern is consistent: an unperformed split shows up as one
above-range file plus several below-range files.

---

## 8. One-line responsibility per production file

Taken verbatim from each file's module doc (//!), which every production file carries except the
runtime SQL. The last column adds my size judgement.

| file | prod LOC | physical | one-line responsibility (module //! doc, verbatim) |
| --- | ---: | ---: | --- |
| core/crates/layerfs-content/src/error.rs | 120 | 174 | Typed content-construction and canonical-read failures. |
| core/crates/layerfs-content/src/file/cdc/gear.rs | 494 | 538 | Frozen two-byte rolling GEAR content-defined chunking. |
| core/crates/layerfs-content/src/file/cdc/mod.rs | 5 | 10 | Frozen content-defined chunking. (declaration-only entry file) |
| core/crates/layerfs-content/src/file/content.rs | 195 | 249 | Complete-file construction and the result it returns. |
| core/crates/layerfs-content/src/file/edit/apply.rs | 391 | 447 | Known-edit construction: one dispatch on the declared final length. |
| core/crates/layerfs-content/src/file/edit/compare.rs | 67 | 86 | Bounded applicability comparison for an exact no-op. |
| core/crates/layerfs-content/src/file/edit/concat.rs | 19 | 29 | Coalescing adjacent slices of one payload into a single extent. (small single-purpose file) |
| core/crates/layerfs-content/src/file/edit/finish.rs | 38 | 54 | Final emission of one known edit: the root, and only the root. (small cohesive file) |
| core/crates/layerfs-content/src/file/edit/input.rs | 286 | 390 | Checked ordered edit stream, its bounded replacement source and applicability. |
| core/crates/layerfs-content/src/file/edit/mod.rs | 15 | 20 | Known-edit construction: checked input, bounded frontier and final emission. (small single-purpose file) |
| core/crates/layerfs-content/src/file/edit/split.rs | 23 | 32 | Cutting one stored extent at a declared edit boundary. (small single-purpose file) |
| core/crates/layerfs-content/src/file/edit/tree.rs | 552 | 645 | Stored-node localized edit: split, concatenate and unchanged-subtree reuse. |
| core/crates/layerfs-content/src/file/mapping/build.rs | 301 | 371 | Streaming construction of the extent tree. |
| core/crates/layerfs-content/src/file/mapping/codec.rs | 303 | 334 | Checked canonical codec for extent nodes, file state and chunk payloads. |
| core/crates/layerfs-content/src/file/mapping/mod.rs | 15 | 20 | Extent-tree mapping: typed fields, canonical codec, streaming build and read. (small single-purpose file) |
| core/crates/layerfs-content/src/file/mapping/read.rs | 210 | 241 | Bounded extent traversal and ordered payload demand. |
| core/crates/layerfs-content/src/file/mapping/types.rs | 182 | 248 | Typed extent-tree fields and their checked invariants. |
| core/crates/layerfs-content/src/file/mod.rs | 17 | 23 | Complete-file construction, chunking, extent mapping and logical reads. (small single-purpose file) |
| core/crates/layerfs-content/src/file/read.rs | 111 | 127 | Logical file and range reads over the authenticated object provider. |
| core/crates/layerfs-content/src/file/view.rs | 142 | 171 | One operation-local authenticated view of an immutable base. |
| core/crates/layerfs-content/src/lib.rs | 24 | 41 | Canonical objects and complete-file construction for LayerFS (C1). (small single-purpose file) |
| core/crates/layerfs-content/src/object/access.rs | 18 | 36 | Narrow authenticated canonical-object provider. (small single-purpose file) |
| core/crates/layerfs-content/src/object/codec.rs | 107 | 138 | Canonical object envelope: framing, checked decoding and one-pass encoding. |
| core/crates/layerfs-content/src/object/id.rs | 89 | 119 | Canonical object identity: a fixed 32-byte digest over frozen framing. |
| core/crates/layerfs-content/src/object/inode_leaf.rs | 311 | 397 | Checked compact inode-value grammar: the physical-pooling input format. |
| core/crates/layerfs-content/src/object/mod.rs | 23 | 29 | Canonical object identity, framing, authenticated read and finalized output. (small single-purpose file) |
| core/crates/layerfs-content/src/object/output.rs | 124 | 192 | Finalized canonical output: the owned object handed to a bounded consumer. |
| core/crates/layerfs-content/src/object/predecessor.rs | 67 | 107 | Bounded advisory predecessors with explicit provenance. |
| core/crates/layerfs-content/src/policy.rs | 136 | 226 | Checked construction profile, capacities and the complete-file selector. |
| core/crates/layerfs-storage/sql/schema.sql | 48 | 69 | Shipped runtime SQL schema for the embedded Store (tables, constraints, indexes; read by sqlite/schema.rs). |
| core/crates/layerfs-storage/src/cas/batch.rs | 58 | 87 | Bounded accepted/pending ownership. |
| core/crates/layerfs-storage/src/cas/dependencies.rs | 62 | 88 | Incremental direct-reference availability. |
| core/crates/layerfs-storage/src/cas/finish.rs | 16 | 25 | Acknowledgement and the one failure boundary. (small single-purpose file) |
| core/crates/layerfs-storage/src/cas/membership.rs | 32 | 47 | Exact reuse and collision decisions. (small cohesive file) |
| core/crates/layerfs-storage/src/cas/mod.rs | 11 | 16 | Content-addressed save and read: one owner, bounded batches, real persistence. (small single-purpose file) |
| core/crates/layerfs-storage/src/cas/owner.rs | 710 | 890 | One save mutation owner: cursors, placement state and the write transaction. |
| core/crates/layerfs-storage/src/cas/read.rs | 74 | 101 | Batched object reads and the retained-pack visibility ceiling. |
| core/crates/layerfs-storage/src/cas/save.rs | 52 | 73 | Real batched save coordination. |
| core/crates/layerfs-storage/src/cas/store.rs | 395 | 530 | Public Store handle, the exclusive save operation and the C1 handoff adapter. |
| core/crates/layerfs-storage/src/encoding/codec.rs | 508 | 632 | Pinned Zstandard codec with caller-owned bounded workspaces. |
| core/crates/layerfs-storage/src/encoding/decode.rs | 170 | 192 | Record reconstruction: one stored record to its canonical object. |
| core/crates/layerfs-storage/src/encoding/delta/candidates.rs | 126 | 161 | Bounded admitted-FULL winner cache used when no explicit predecessor applies. |
| core/crates/layerfs-storage/src/encoding/delta/mod.rs | 4 | 8 | Payload DELTA: record framing, candidate acquisition, selection and chains. (declaration-only entry file) |
| core/crates/layerfs-storage/src/encoding/delta/read.rs | 202 | 261 | Iterative dependency-chain reconstruction. |
| core/crates/layerfs-storage/src/encoding/delta/record.rs | 201 | 240 | Delta record framing for the compact whole-file and native chunk lanes. |
| core/crates/layerfs-storage/src/encoding/delta/select.rs | 262 | 343 | Candidate eligibility, one prefix trial and complete-cost selection. |
| core/crates/layerfs-storage/src/encoding/full.rs | 177 | 213 | FULL representations and the shared per-object raw payload view. |
| core/crates/layerfs-storage/src/encoding/mod.rs | 11 | 17 | Physical encoding: pinned codec, supported records, delta selection, pooling. (small single-purpose file) |
| core/crates/layerfs-storage/src/encoding/pool/delta.rs | 262 | 289 | Pooled COPY/INSERT delta: the metadata instruction grammar and its producer. |
| core/crates/layerfs-storage/src/encoding/pool/index.rs | 203 | 259 | The bounded Store-owned ordered set of pooled value candidates. |
| core/crates/layerfs-storage/src/encoding/pool/leaf.rs | 118 | 161 | Pooled leaf records: the physical form of one inode leaf page. |
| core/crates/layerfs-storage/src/encoding/pool/mod.rs | 9 | 14 | Pooled physical metadata: value groups, ordinals and pooled leaf records. (small single-purpose file) |
| core/crates/layerfs-storage/src/encoding/pool/read.rs | 255 | 294 | Pooled reconstruction: value groups and pooled leaf bodies. |
| core/crates/layerfs-storage/src/encoding/pool/value_group.rs | 79 | 103 | Value groups: the exact physical representation of pooled metadata values. |
| core/crates/layerfs-storage/src/error.rs | 105 | 154 | Typed storage failures, including the unknown-persistence-outcome boundary. |
| core/crates/layerfs-storage/src/lib.rs | 11 | 30 | Physical content-addressed storage for LayerFS (C2). (small single-purpose file) |
| core/crates/layerfs-storage/src/pack/assemble.rs | 223 | 252 | Group framing and the one selected pack assembly per write. |
| core/crates/layerfs-storage/src/pack/layout.rs | 381 | 466 | Pack grammars, framing lanes and exact fit arithmetic. |
| core/crates/layerfs-storage/src/pack/mod.rs | 12 | 17 | Pack grammars, group framing, placement and record extraction. (small single-purpose file) |
| core/crates/layerfs-storage/src/pack/placement.rs | 123 | 159 | Exact append-or-new placement and the retained open-pack tail. |
| core/crates/layerfs-storage/src/policy.rs | 210 | 332 | Checked storage policy, format profile and derived capacities. |
| core/crates/layerfs-storage/src/sqlite/cleanup.rs | 58 | 77 | Bounded cleanup of a definitely failed, unpublished save. |
| core/crates/layerfs-storage/src/sqlite/connection.rs | 50 | 72 | Embedded connection profile: no WAL, no synchronous flush, no busy waiting. |
| core/crates/layerfs-storage/src/sqlite/lookup.rs | 141 | 177 | Bounded membership, location and presence queries. |
| core/crates/layerfs-storage/src/sqlite/mod.rs | 10 | 15 | Embedded persistence: connection profile, schema, bounded queries and cleanup. (small single-purpose file) |
| core/crates/layerfs-storage/src/sqlite/pool.rs | 128 | 160 | Bounded value-group catalogue access. |
| core/crates/layerfs-storage/src/sqlite/schema.rs | 232 | 267 | Exact candidate schema creation and validation. |
| core/crates/layerfs-storage/src/sqlite/write.rs | 83 | 117 | Batched bindings and the lazily started shared write transaction. |
| core/crates/layerfs-telemetry/src/lib.rs | 3 | 16 | Environment-independent parent/child timing trees for LayerFS. (declaration-only entry file) |
| core/crates/layerfs-telemetry/src/timer/format.rs | 69 | 82 | Readable presentation of a completed timing tree. |
| core/crates/layerfs-telemetry/src/timer/json.rs | 115 | 136 | Fixed-format JSON output for a completed timing tree. |
| core/crates/layerfs-telemetry/src/timer/mod.rs | 8 | 25 | Timing scopes, completed reports and their renderers. (declaration-only entry file) |
| core/crates/layerfs-telemetry/src/timer/recording.rs | 232 | 291 | Private clocks, bounded tree construction and attachment. |
| core/crates/layerfs-telemetry/src/timer/report.rs | 170 | 249 | Completed timing data: owned nodes, outcomes and completeness. |
| core/crates/layerfs-telemetry/src/timer/scope.rs | 135 | 206 | Parent/child scope lifecycle and disabled execution. |

**Thin-wrapper judgement: none.** Every production file declares a distinct responsibility in its
own module doc, and the small ones are real: file/edit/concat.rs:12 coalesce_adjacent implements a
checked canonical-partition rule; file/edit/split.rs:12 slice_of validates an edit boundary against
the extent; object/access.rs:18 defines the AuthenticatedObjects provider contract C1 depends on;
cas/finish.rs:12 terminate implements the one-terminal-disposition/quarantine boundary;
object/access.rs and cas/finish.rs are small because their contracts are small, not because they
forward. The ten smallest production files are all entry files (nine mod.rs plus
layerfs-telemetry/src/lib.rs): 3, 4, 5, 8, 9, 10, 11, 11, 11, 12 prod LOC - declarations and
reexports, which core/AGENTS.md explicitly allows as direct delegation. No file is an empty shell,
and no file's body is a single forwarding call to another module.

---

## 9. Reproduction

~~~sh
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
git status --porcelain=v1 && git rev-parse HEAD
python3 tools/production_loc.py --help
python3 tools/production_loc.py --files
python3 tools/production_loc.py --detail
python3 tools/test_production_loc.py
TMP=$(mktemp -d)
git archive c38961f2f4bedbc7afe8826c5d404c6d21b91657 core/crates | tar -x -C $TMP/pre
git archive HEAD core/crates | tar -x -C $TMP/post
python3 tools/production_loc.py --root $TMP/pre  --detail
python3 tools/production_loc.py --root $TMP/post --detail
for c in $(git log --reverse --format=%H 5e8b8cbc2..HEAD); do
  mkdir -p $TMP/p $TMP/c
  git archive $c^ core/crates | tar -x -C $TMP/p
  git archive $c  core/crates | tar -x -C $TMP/c
  echo "== $c"; python3 tools/production_loc.py --root $TMP/p; python3 tools/production_loc.py --root $TMP/c
  git log -1 --format=%B $c | grep 'Production LOC'
done
git archive 44cf748486863ab7c21ca47e731bd88e2b9a7b4a crates | tar -x -C $TMP/ref
python3 tools/production_loc.py --root $TMP/ref --detail
~~~

The counter-comparison helpers used for the counter audit (in-memory probes of blank_rust,
blank_inline_tests, blank_sql, plus the pure-test-only re-implementation and the per-file/plan
tables) were run with python3 reading tools/production_loc.py as a module; no product file was
written, and the only files created are the four companions in this evidence directory plus
temporary extraction trees under /tmp.

---

## 10. What I could NOT verify

| # | item | why |
| --- | --- | --- |
| 1 | Anything that requires building or running the product | Prohibited for this audit (the parent agent holds the verification suite). No cargo, no tests, no clippy, no fmt. |
| 2 | That the test bodies behind the named targets actually check the claimed properties | Out of this lane (test-oracle audit). I read only the LOC counter's own tests. |
| 3 | A corrected reference production total | F1/F2 are bounded but not fully resolvable without deciding policy for two cases: (a) whether test-instrumentation-gated code (1060 removed lines across 88 items) counts as product, and (b) whether the 3737 LOC of src/-resident test modules should leave the reference subtotal. The corrected figure quoted (about 65766) assumes (a) instrumentation does NOT count and (b) the test modules do NOT count. |
| 4 | The exact authorship of the 58-line misallocation in da8ee5769 | The two immutable commit messages are wrong; which crate the counter attributed the lines to at that time cannot be re-derived from the message alone (the tree re-count is unambiguous, so it is a reporting error, not a counting error). |
| 5 | Whether the retained evidence directories (stages-3-4-smoke-*, timing-*, matched-c1-*, oracle-*, fingerprint-collision-*) contain the LOC receipts they claim | Out of this lane; I only used them where the report cites them. |
| 6 | Stages 5-7 scope, issue state, and any acceptance verdict | Not this audit's assignment; no issue state was read or changed, and no commit/tag was created. |
