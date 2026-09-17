# W9 evidence — documents, counter and the plan's actual-size table

## W9.1 — the acceptance report no longer contradicts itself

`stages-3-4-report.md` §1 said physical metadata pooling was **NOT IMPLEMENTED**
and that the decoded frontier was **PARTIAL**, while the same document's §5/§6 and
the source said otherwise, and its LOC block was five commits behind the reviewed
snapshot. Three corrections are committed in place, each dated and each naming
what it replaces:

* §1 pooling: the grammar, the value groups, the ordinals, the pooled reader and
  the bounded index all exist and are exercised; the paragraph now says so and
  points at the two lane-assignment deviations.
* §1 frontier: the stored-node split/concat route exists, keeps an untouched
  subtree's identity, and is proven by `edit_reference`'s nine sealed cases.
* the LOC block: restated with the corrected counter (core 10 983, reference
  65 417, combined 76 400) and with the pointer to the closeout report.
* the per-target test table: the `timing` (C1) cell said 11 where the target runs
  **7**; every other cell is now the count the workspace run reports.

## W9.2 — the transition self-criticism replaced by the real gap

The paragraph that called small → large and large → small a "rebuild" defect is
replaced by the measured statement: `file/edit/apply.rs:106-109` does exactly what
handoff §3.C prescribes (stream the retained bytes and replacements through the
complete builder, or assemble retained ranges without reading a discarded one),
with no fallback and no second runtime mode, and what is *missing* is a
measurement of the transitions' read amplification. The residual gap is now stated
as a gap rather than as a code defect.

## W9.3 — the oracle paperwork

* `edit_reference.rs` no longer opens with "this target fails today, on purpose";
  the header says the target passes, records the nine matching cases, and dates the
  correction.
* The oracle README's `git diff` claim now says `crates/layerfs-content/src` (the
  claim was true for the sealed source and false for the whole path, which also
  carries the two examples added afterwards).

## W9.4 — the per-file actual-size table

`stages-3-4-report.md` §8 now carries the table the file plan's §4 requires
(`path | action | before | after | delta | recommended | verdict | physical |
responsibility`) for every plan row, the directory totals against the plan's ranges,
the disjoint package totals and the list of files that are not plan rows. It is
generated from `tools/production_loc.py --files` and the plan's own table, and the
plan's `before` column was verified by the review to equal the pre-Stage-3 base.

## W9.5 — the counter

Two defects, both fixed in `tools/production_loc.py`, each with a focused test in
`tools/test_production_loc.py` (17 tests, all passing):

1. **A predicate that merely mentions "test" is shipped code.** The reviewed rule
   removed any item whose `#[cfg(...)]` contained "test", which deleted
   `cfg(not(test))`, `cfg(any(test, feature = "test-instrumentation"))` and
   `cfg(any(debug_assertions, feature = "test-instrumentation"))` bodies. The rule
   now removes an item only when a test build is the only way to satisfy the
   predicate: `test`, `all(..., test, ...)`, `any` where every alternative implies
   test; `not(...)`, a feature and a debug-assertion predicate never do.
2. **A test-only file under `src/` is not product code.** A module declared by a
   test-implying `#[cfg]` - and, transitively, a module declared inside such a file
   - is excluded whole. That is how the reference's eleven `*_tests.rs` modules and
   the nested `objects/admission/issue100_diagnostic.rs` were being counted.

Measured on the same tree with the same scanner:

| Rule | Reference production LOC |
| --- | ---: |
| corrected counter | **65 417** (193 files) |
| reviewed cfg rule (over-removal) | 64 461 → **956 lines removed that ship** |
| test-only files counted | 69 500 → **4 083 lines of test modules** |
| the review's quoted figure | 68 476 |
| combined, corrected (core 10 983 + reference 65 417) | **76 400** |
| combined, as quoted by the review | 79 405 |

The same corrected counter is applied to the pre-Stage-3 base `c38961f2f`
(core **6 152**: C1 2 336, C2 3 084, telemetry 732) and to the current tree
(core **10 983**: C1 4 388, C2 5 863, telemetry 732); core grew +4 831. The core
subtotal does not move with the correction, because `core/crates/*/src` contains no
`cfg` attribute at all. No combined total is quoted anywhere in this batch without
both scopes having been counted by the corrected counter.

## W9.6 — pointers

`core/README.md`, the 0.1.7 roadmap index, `implementation-plan.md` and
`implementation-issues.md` each gained a short closeout paragraph pointing at the
closeout report, and the experiment ledger gained entry **L33** with the closeout's
numbers, identities, the commands that actually ran and the two open owner
decisions.

## Commands, exits and raw output

`w9-verify.log`: the 17 counter tool tests, the counter over the current tree, over
a fresh export of `c38961f2f`, and per file, the core tool tests, the product
boundary check, the workspace suite (43 targets, 263 tests, 0 failed), clippy, fmt
and `git diff --check` - every command exit 0.

## What this artifact does not prove

* The document corrections are corrections of *this document's* contradictions;
  they do not re-verify the implementation, which the packet evidence does.
* The counter's numbers are only as good as its classification. Its scope,
  exclusions and method are unchanged from the audited counter except for the two
  fixed defects, and both scopes are counted by the same code in the same run.
* `stages-3-4-report.md`'s §8 table uses the plan's `before` column as its
  baseline; the per-file `before` values were not recomputed from `c38961f2f` file
  by file (the review verified the column matches the base for these files).
