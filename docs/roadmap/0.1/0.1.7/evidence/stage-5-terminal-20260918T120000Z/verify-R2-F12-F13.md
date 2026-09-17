# Independent verification: R2-F12 / N-9 and R2-F13 / N-10 (telemetry public API)

Verifier: subagent, read-only. Repository frozen at `99743b2cf3a869b7d8897a1f16b82d742aeedc40`
(`git rev-parse HEAD` = `99743b2cff2470e6634874d7ee14b9d37d0ba16e`). Working tree clean
(`git status --short` reported no tracked modifications at start and end; the only
artifacts created were ignored build output under `core/target/` and the git-ignored
`target/` of the re-run diagnostics client). Claims under test are fixed in round-3
commit `2fe2a4642` ("fix(core): bound the read wave, right-size the value limit,
validate policies"), whose message states under "Telemetry": "`TimingReport::completeness()`
returns Disabled, Complete or Clipped; `is_incomplete()` is now true only for a clipped
measured report" (R2-F12) and "A node that never returned - a panicked child - reports
the new `NodeOutcome::Unknown` instead of the `Ok` it was created with" (R2-F13), and
closes with "Pinned behaviours changed on purpose: a disabled report is no longer
'incomplete', a still-running node is no longer `Ok`". No roadmap report, suite result
or commit message below is treated as evidence by itself; every row is reproduced from
source, tests or a fresh run.

## Verdicts

| Row | Claim | Verdict |
| --- | --- | --- |
| R2-F12 / N-9 | Disabled and clipped timing reports distinguishable through public API; `completeness()` = Disabled/Complete/Clipped; `is_incomplete()` true only for clipped measured report (disabled NOT incomplete) | **PASS** |
| R2-F13 / N-10 | A child that never returned (panic caught by caller) reports `NodeOutcome::Unknown`, not `Ok` | **PASS** |
| Harness check | No example/harness still treats a disabled report as clipped | **PASS** |

## Commands run (all read-only; exit codes)

| # | Command (from repository root unless noted) | Exit |
| --- | --- | --- |
| C1 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-telemetry --locked` | 0 |
| C2 | `cargo +1.85.1 test ... \| grep -E "^(running\|test \|test result\|     Running)"` (re-run of C1 to list test names) | 0 |
| C3 | `cargo run --quiet` in `docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T020000Z/diagnostics/s5check` | 0 |
| C4 | `git show 2fe2a4642 -- core/crates/layerfs-telemetry/{tests/timer.rs,src/timer/recording.rs}`; `git show 5e2a20a0c:...timer.rs`; `git show f823dc09c:...timer.rs`; `git log --all --oneline -- .../tests/timer.rs` | 0 |
| C5 | `grep -rn "NodeOutcome::Ok\|outcome()" core/crates/layerfs-telemetry/tests/` and `-rn "is_incomplete\|completeness()\|Completeness" core/ --include="*.rs"` (plus targeted `sed` reads of each hit) | 0 |
| C6 | `grep -rn "is_incomplete\|completeness()" crates/ benchmark/ tools/ --include="*.rs"` (usage outside core) | 0 |

C1 result: `test result: ok` for every binary — `tests/timer.rs` 21 passed, 
`tests/timer_compile_fail.rs` 1 passed, `tests/timer_format.rs` 6 passed,
`tests/timer_json.rs` 10 passed, doc-tests 0; **38 passed, 0 failed**.

## Findings — R2-F12 / N-9

**Public API source.**
- `core/crates/layerfs-telemetry/src/timer/report.rs:61-69` — the three-state enum:
  `pub enum Completeness {` ... `Disabled,` ... `Complete,` ... `Clipped,` with docs
  "Disabled recording and a clipped report are different states with different
  consequences, so they are different values".
- `report.rs:289-294` — `pub fn is_incomplete(&self) -> bool { match &self.root { Some(root) => root.is_incomplete(), None => false } }`: a disabled report (no root) returns **false**.
- `report.rs:301-307` — `completeness()`: `None => Completeness::Disabled`, `Some(root) if root.is_incomplete() => Completeness::Clipped`, `Some(_) => Completeness::Complete`.
- Publicly exported at `core/crates/layerfs-telemetry/src/timer/mod.rs:24`:
  `pub use report::{Completeness, NodeOutcome, TimingNode, TimingReport};`.

**Pinned tests (C1, all pass).** `core/crates/layerfs-telemetry/tests/timer.rs:217-247`
`disabled_and_clipped_reports_are_distinguishable`:
- :220-221 `assert_eq!(disabled.completeness(), Completeness::Disabled); assert!(!disabled.is_incomplete());`
- :226-227 `assert_eq!(complete.completeness(), Completeness::Complete); assert!(!complete.is_incomplete());`
- :234-235 `assert_eq!(clipped.completeness(), Completeness::Clipped); assert!(clipped.is_incomplete());`
- :244-245 attaching a disabled report to a measured node → `Completeness::Clipped` + `is_incomplete() == true` (disabled child detail marks the measured parent clipped; the disabled report itself is still not "incomplete").
Also `tests/timer.rs:207-212` (`disabled_recording_runs_the_operation_and_records_nothing`):
comment "Pinned behaviour changed deliberately: a disabled report measures nothing, which
is legitimate, so it is no longer reported as incomplete" then `assert!(!report.is_incomplete()); assert_eq!(report.completeness(), Completeness::Disabled);`.

**The old pinned test was changed on purpose.** `git show 2fe2a4642 -- .../tests/timer.rs`
shows the pre-round-3 line at that spot was `-    assert!(report.is_incomplete());`
(i.e. the old test pinned a disabled report as incomplete) replaced by the two lines
above, and the whole `disabled_and_clipped_reports_are_distinguishable` test is new in
that commit. The file has exactly two commits in history (`f823dc09c` original,
`2fe2a4642` round 3).

**Independent falsification (C3).** Re-ran the round-3 diagnostics client; relevant
output lines verbatim: `D1 disabled: completeness Disabled, is_incomplete false,
has_root false` / `D2 complete: completeness Complete, is_incomplete false` /
`D3 attached-disabled: completeness Clipped, is_incomplete true` / `D8 the three states
are distinct: true`. Source of those lines: `.../s5check/src/main.rs:238-275` — they
read only `Timing::disabled`, `Timing::record`, `completeness()`, `is_incomplete()`.

## Findings — R2-F13 / N-10

**Source that makes a never-finished child Unknown.**
`core/crates/layerfs-telemetry/src/timer/recording.rs:272-289` (`Inner::collect`):
- :274 `let incomplete = slot.incomplete || slot.running;` — a still-running slot marks the node (and thus ancestors, via :295 `incomplete |= collected.is_incomplete()`) incomplete.
- :275-278 comment: "A slot that is still running when the tree is collected never returned: its scope's closure panicked or was dropped. Reporting the outcome it was created with would serialise a panic as success, so it reports that its outcome is unknown."
- :279-283 `let outcome = if slot.running { NodeOutcome::Unknown } else { slot.outcome };` and :284-288 elapsed → `Duration::ZERO` for a still-running slot.
Slots are created with `outcome: NodeOutcome::Ok, running: true` (`recording.rs:118`, `:218`), so without this branch a panicked child would serialize as `Ok`. `git show 2fe2a4642 -- .../recording.rs` confirms the old code returned `(slot.outcome, incomplete, elapsed)` directly — this is exactly the fixed defect. `NodeOutcome::Unknown` is defined at `report.rs:15-18` with `is_unknown()` at :33-35.

**Pinned test (C1, passes).** `tests/timer.rs:267-297`
`a_panic_caught_inside_the_operation_leaves_the_recording_usable`: a child
`root.child("unstable").run(|_| ... panic!("child panic"))` is wrapped in
`std::panic::catch_unwind` inside the operation (:271-274), then:
- :291 `assert_eq!(unstable.outcome(), NodeOutcome::Unknown);`
- :292-293 `assert!(unstable.outcome().is_unknown()); assert!(!unstable.outcome().is_error());`
- :294 the sibling that did return is `NodeOutcome::Ok` (contrast case)
- :285-286 `assert!(unstable.is_incomplete()); assert_eq!(unstable.elapsed(), Duration::ZERO);`
- :295-296 `assert!(root.is_incomplete()); assert_eq!(report.completeness(), Completeness::Clipped);`
Comment :287-290: "Pinned behaviour changed deliberately: a scope that never returned has
an unknown outcome. It was serialised as `Ok` because the slot was created with that
outcome, which made a panic look like success to any consumer reading the outcome
rather than the incomplete flag."

**Was the old behaviour still pinned anywhere?** No.
- The pre-round-3 version of that test (`git show 5e2a20a0c:...timer.rs`, identical to
  the original `f823dc09c` version) asserted the panicked child's `is_incomplete()` and
  `elapsed() == ZERO` but **no outcome assertion at all** — the old `Ok` came from the
  source bug, not from a pinned test; round 3 added the `Unknown` assertions.
- Exhaustive grep of `NodeOutcome::Ok`/`outcome()` assertions in the crate's tests
  (C5): `tests/timer.rs:73, :123, :139, :294, :427-428` — every one is a node that ran
  to completion (e.g. :294 is the successful "after" sibling; :427-428 are completed
  operations whose attachment was disabled). **No test asserts Ok-on-panic.**
- Presentations print it: `src/timer/format.rs:29` renders ` [unknown]`;
  `src/timer/json.rs:70-72, :82-83` serialize any non-Ok outcome via `as_str()`
  (`report.rs:45-51`, Unknown → `"unknown"`).

**Independent falsification (C3), verbatim:** `D4 panicking child result Ok(())`
(the operation result is Ok because the caller caught the panic — this is the
misleading surface the fix addresses) / `D5 panicking child node
Some(("unstable", Unknown, true))` / `D6 child outcome is Unknown, not Ok: true` /
`D7 report completeness Clipped`. Source: `.../s5check/src/main.rs:250-273`.

## Findings — harness / example usage (no disabled-as-clipped)

- Telemetry's own examples `core/crates/layerfs-telemetry/examples/timer_composition.rs`
  and `timer_nested.rs` contain **no** `is_incomplete`/`completeness` usage (C5) — they
  neither mistreat disabled nor demonstrate the new API (noted, not a claim violation).
- Every `is_incomplete()` gate in storage/content harnesses is a clipped-failure gate on
  a measured row: `core/crates/layerfs-storage/examples/measure_edits.rs:299`,
  `measure_filesystem.rs:317` (`require_complete`), `measure_pooled.rs:193`,
  `measure_components.rs:145`, `core/crates/layerfs-content/examples/filesystem_timing_c1.rs:596`.
  With `is_incomplete()` now false for a disabled report, a disabled report can no longer
  enter these "INCOMPLETE/clipped" failure paths.
- Harnesses that handle a disabled report distinguish it by `has_root()`, not by
  treating it as clipped: `core/crates/layerfs-storage/tests/filesystem_pipeline.rs:781-783`
  `assert!(!report.has_root(), "a disabled run records no tree, whatever it planned")`
  for the `recording=false` arm, and `core/crates/layerfs-storage/tests/timing.rs:220`
  `assert!(!disabled_report.has_root())`.
- No code path anywhere asserts `is_incomplete() == true` for a disabled report (C5:
  the only `is_incomplete` assertion adjacent to `Timing::disabled` is the telemetry
  test asserting `!disabled.is_incomplete()`), and there is no `is_incomplete`/
  `completeness()` usage outside `core/` (C6: `crates/`, `benchmark/`, `tools/` — 0 matches).

## UNVERIFIED

- The storage/content timing tests and examples were read, not executed (only
  `-p layerfs-telemetry` was run, per instructions); their `is_incomplete` assertions
  were verified by source inspection, not a green run of those suites.
- Behaviour under a panic that unwinds *through* the whole operation (no inner
  `catch_unwind`) was verified only by the test `panic_unwinds_without_a_fabricated_report`
  (:250-264, passes: the panic propagates and no report is fabricated) — I did not
  independently re-run that scenario outside the suite.
- Root `crates/` reference implementation and the `benchmark/` tree were not checked
  for the same two properties (grep shows they never call the new API; they predate it).
- The D1-D8 output of s5check is from the round-3 evidence client's own Cargo project
  (its `Cargo.toml` pins `layerfs-telemetry` by path); I confirmed it builds and runs
  against the frozen tree, but did not audit its remaining A/B/C/E probes, which are
  outside these two rows.
