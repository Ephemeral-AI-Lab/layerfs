# H11 retained-control preregistration v8

Status: **FROZEN BEFORE ANY V8 MEASURED ROW**. V7 is preserved as a pre-measurement focused-test `REVISE`; no v7 dry-run, screen, gate, result root, or benchmark lock exists.

## Preserved V7 failure

V7’s source repair was not measured because its regression test called the private `test_path` inside a sibling retained test module and failed E0425. The controlling failure artifact is `FOCUSED-REVISE-v7.json`, SHA-256 `4af73ac3dedce07e3926fec25244f1864bb67bb2716deb0d061e6bda060058fd`.

Targeted review also found that the older prior-operation path dropped its capacity charge immediately before dropping the owned Vec. V8 corrects the lifetime order before measurement.

## V8 source/test repair

- Use a self-contained stdlib temporary test directory keyed by process/time, created atomically and owned by a Drop cleanup guard; no dependency or retained private helper.
- Drop `prior_operations` before `prior_operations_charge`.
- Retain V7’s substantive fixes: exact 32-KiB charged H11 edit-point preparation, charged historical replace operations, fresh executable, native v8 schemas, unchanged LayerFS identities/fixture/schedule/thresholds.

## Ladder

Run exact focused H11 preparation/historical-Q and whole-harness-Q tests, the Store timer test, and H11 clippy. Then build once, freeze method/source/executable hashes, run zero-row dry-run, fresh N=1,000 screen, one frozen-source workspace/H11 static closure, one fresh eight-row gate, and a fresh three-lane terminal audit. Any failure is preserved in v9.

