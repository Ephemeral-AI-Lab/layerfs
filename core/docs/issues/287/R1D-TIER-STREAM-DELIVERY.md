# R1d compatibility tier-stream scoped delivery

> **Implemented and selected owning cover PASS, 2026-10-01.** Developed against
> published `50a4f3a19`; the actual frozen production hashes below identify this
> correction. This is compatibility C1 reference-stream evidence, not full R1,
> whole-Core, candidate speed, native, strict or physical admission.

The [prospective freeze](R1D-TIER-STREAM-FREEZE.md) preceded product edits and
root waited for the running full command to complete before authorizing them.
The correction affects SC-03/05/06/07. It changes the old public C1 compatibility
ReferenceReducer touched/final consumers; supplied native Canonical8 Count/
Release, canonical formats, caller quotas and worker/deadline profiles remain
unchanged. Old compatibility touched collections and reducers remain explicit
populations; this delivery does not declare them globally retired.

## Problem, final behavior and reviewed custody

The retained first full-Core command failed the original parent quota matrix:
pending1/ordering864/base-batch1 refused actual960. Its parent-value insertion
order was already correct: five binding additions to serial7 precede values2..6.
The resulting input runs contain five96-byte records. Materializing another
five-row consolidated run requires480 input +480 output bytes. The historical
864 pass omitted an adopted96-byte input charge; R1d commit `d1e478ffb` fixed
that omission. This correction preserves its exact live-input accounting and
removes the actual output storage rather than weakening the quota.

Both touched scanning and final consumption use one fixed32-tier head cursor.
Tier order supplies newest precedence and pending rows win ties. The cursor
decodes/validates hidden older duplicates, exact metadata/length, full grammar,
strict serial order, first/last bounds and checked offsets/counts. One inline
96-byte read buffer replaces per-tier full-scan buffers. The touched consumer
leaves unchanged-tier point/high-water controls intact. Final transfer destroys
their lookup buffers and old Vec allocation, moves original handles into a
fixed32 array, ends the levels Vec allocation, and creates final heads. Pending
rows move through the original BTreeMap into-values iterator without a second
row Vec. No output file or in-place input append occurs in either stream.

FinalRows closes actual exhausted handles only after all their heads have been
consumed; errors keep the first failure and do not issue another read. RunStore
keeps transferred input bytes charged through known checked backing release.
Failed removal remains visible through actual backing-held bytes. The existing
canonical caller drops final rows and checks cleanup before emitting a final
FilesystemRoot; the caller and public explicit consolidate algorithm required
no changes. Generic FileBacking cached length is not promoted into a native
identity/fstat, strict-heap or physical-storage guarantee.

Production compilation passed the actual Rust layout assertions comparing
TierHeads with32 old RunReaders plus minimum96-byte buffers, and final owned
handles+heads with32 old OptionRun/OptionLookupScan controls plus those minimum
buffers. This is the existing fixed compatibility working composition, separate
from the unchanged encoded pending/run/output ordering ceiling. It introduces
no new64KiB budget, native8, global176MiB, cache, physical or formal-maximum fit.
There is no allocation measurement or platform-independent numeric sizeof claim.

## Exact commands and retained outcomes

Root ran the commands against its immutable
`core/target/r1-eight-source/core/Cargo.toml` copy with Cargo1.85.1 and `--locked`.
The worker ran no Cargo, build, test or measurement command. Direct rustfmt and
`git diff --check` passed before the source handoff.

| Retained command directory under `benchmark-results/fs-bench-pro/` | Outcome | Cause/scope |
| --- | --- | --- |
| `issue287-r1-final-full-core` | FAIL | Original864 quota gate refused actual960; full command retained. |
| `issue287-r1-tier-stream-legacy-quota-cover` | FAIL before compilation | Requested nonexistent `filesystem_references` test target. |
| `issue287-r1-tier-stream-target-name-cover` | Compile FAIL | New external test imported undeclared shared-support module. Product type/layout assertions compiled; no product correction followed. |
| `issue287-r1-tier-stream-external-module-cover` | PASS, exit0; complete command6.752859875s | Root corrected only the external test to an explicit support-module path, matching the existing run-seek test. Four selected files,28 tests. |

The final command, reproduced from its retained `command.json`, is:

```sh
cargo +1.85.1 test --manifest-path core/target/r1-eight-source/core/Cargo.toml --locked -p layerfs-content --test filesystem_parent_lookup --test filesystem_tier_stream --test filesystem_reference --test filesystem_ordering -- --show-output
```

| Selected test file | Passed | Recorded test-body wall | Proved scope |
| --- | ---: | ---: | --- |
| `filesystem_parent_lookup` | 5 | 0.07s | Original matrix and all eight required pending1/864-or960 and pending2/960-or1152 cells at batches1/64; independently fixed canonical root. Parent read demands remain22/10/6 versus explicit18/9/6 at batches1/3/64. |
| `filesystem_reference` | 2 | 0.34s | Construction/update match sealed reference v1 roots and reachable pages. |
| `filesystem_tier_stream` | 3 | 0.02s | Independent complete Count/Effect event fold across overlapping tiers and pending supersession, batches1/64; observer proves no post-mutation creates/appends/flushes; closed EOF; corrupt hidden older duplicate; actual retained input files and post-finish truncation; terminal no-reread and one checked cleanup. |
| `filesystem_ordering` | 18 | 0.10s | Existing grammar, thresholds, carries, lookup, input storage admission, failure/no-root, failed-removal custody, checked cleanup and untouched-tier scan behavior. |

The original parent quota assertions were unchanged. The final raw matrix still
refuses smaller unsupported cells, including pending1/672-or768 at actual864 and
pending2/864 at actual960. Those refusals were not dropped or turned into a
budget expansion. This command is a functional owning cover, not a speed arm or
benchmark qualification. No prior performance case was resampled.

## Frozen source identities and remaining gates

| File under `core/crates/layerfs-content/` | SHA256 |
| --- | --- |
| `src/filesystem/references/runs.rs` | `e1c240f635924176baabe467950efedf7f9c33ade4878265d2ca31cdd36ad413` |
| `src/filesystem/references/reduce.rs` | `3abb9813e98c7ba7c7c075fd2911efc733c8b2081c03794c67fef0ff41a33ff8` |
| `src/filesystem/references/tier_stream.rs` | `ad325fd2fd3b0c7256b52fc41e55d4967ad0bdddec71789318e96cdb6e437c6a` |
| `src/filesystem/references/mod.rs` including prior coordinated Canonical8 declarations | `83d95d4ec1fdfc47d0c49e568d0a48ea60ed56eaeca1fc3d874661976aa386b9` |
| `tests/filesystem_tier_stream.rs`, root's explicit external module correction | `665b1b6fb1400a80ac7ded67f77177ff13930792f1c2e556de9e87f1e4d71faa` |

The narrow cover does not rerun or replace the distinct explicit-consolidate,
run-seek or allocation-probe receipts. Final owning Clippy/fmt/boundary and
whole-Core completion must be reconciled by root at the final coherent source;
this document does not claim they passed after this correction. Native/global/
cache/physical/strict-provider qualification, formal maxima, protected closing
before join, R3 early content release and candidate performance qualification
remain separate open gates. #288 remains read-only/delegated and R2–R7 remain
unstarted at this delivery scope. No aggregate R1 COMPLETE claim is made.
