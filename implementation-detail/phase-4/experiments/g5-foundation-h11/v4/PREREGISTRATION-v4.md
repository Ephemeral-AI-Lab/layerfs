# H11 retained-control preregistration v4

Status: **FROZEN BEFORE ANY V4 MEASURED ROW** after the preserved v3 screen/static disposition.

## Preserved predecessor

V3 remains `SCREEN PASS / STATIC REVISE`. Its N=1,000 screen completed in `6,779,038,292 ns`, matched exact identity/work/storage, reported whole-harness Q `705,901 -> 0`, and closed its 14-file inventory and owner-bound lock release. V3 never ran a gate. Workspace tests passed `166 / 1 ignored / 0 failed`, but workspace clippy rejected a phase-timer helper unused by the ordinary binary; isolated H11 clippy additionally found the retained eight-argument observation helper and an appended helper after the G3 test module. V3 source/results are never rewritten, rerun, or relabeled.

## V4 one-variable repair

V4 changes only source/static integration:

1. `Store::open_measured` now accepts optional phase output. Existing callers pass `None`; H11 and its focused test pass `Some`. The old extra helper is deleted.
2. Phase clocks are created only when output is requested, so ordinary callers retain zero new timer calls.
3. The H11 revision triple is one value, reducing the observation function below clippy’s argument limit without changing inputs.
4. The H11 G3 helper is inserted before the retained `#[cfg(test)] mod tests`, changing compilation order only.

All v3 evidence repairs, exact Q rules, historical tuples, authenticated storage/reachability counting, timer labels, analyzers, runner, lock/fsync custody, fixture, expectations, schedule, thresholds, and limitations are unchanged except their versioned v4 names.

## Gates

```text
focused Store-open timer test + H11 Q test
-> zero-row v4 dry-run
-> one N=1,000/sample=1 v4 screen <20 s
-> one workspace/H11 clippy -D warnings + tests/fmt/Python/diff closure
-> exact balanced eight-row v4 gate <20 s
```

The gate runs once only after static PASS. Failure is preserved as v4 `REVISE` and repaired in v5; thresholds are not changed.

