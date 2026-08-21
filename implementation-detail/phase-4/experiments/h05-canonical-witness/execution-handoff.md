# Handoff: H05 canonical-witness private screen

Copy the prompt below into the next Codex task. It combines the remaining
orchestration, no-row validation, and one authorized private screen. It does
not authorize a full campaign or product integration.

---

`/goal` Complete the missing H05 screen orchestration, validate it without
producing a measured row, then run the exact frozen `<=120`-second H05 private
screen once and stop with `RETAIN-FOR-FULL-CAMPAIGN` or
`REVERT / H05 LOCAL NO-GO`.

## User authorization and stop boundary

On 2026-08-21 the user explicitly authorized these three items as one handoff:

1. add only the missing H05 runner, independent analyzer, dry-run schedule
   assertion, screen-input custody, lock/timeout recording, and protected
   smoke;
2. validate that orchestration without producing a measured row; and
3. after that validation passes, run the frozen private H05 screen exactly
   once under the hard 120-second ceiling.

This authorization does **not** permit:

- rebuilding or modifying either frozen executable;
- editing the benchmark candidate source;
- running a full five-or-more-pair campaign;
- selectively rerunning, replacing, or deleting a row;
- interpreting a screen as product PASS;
- integrating H05 into production;
- starting canonical-v2, H09/prolly, WP5, materialization implementation,
  SQLite tuning, compression, a carrier, workers, async, or another Phase-4
  experiment;
- physically reverting the preserved dirty candidate source after a failed
  screen; report the frozen `REVERT` disposition and stop for user direction;
- committing, resetting, cleaning, deleting, or modifying historical evidence.

Stop immediately after the private screen evidence, independent analysis,
manifest, and terminal disposition are complete.

## Repository and custody scope

Work only in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
```

Required branch and committed HEAD:

```text
branch  codex/empty-worktree
HEAD    febc20f046bba84ccdce1256363d77799eabf2db
```

Never touch sibling `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

The worktree is intentionally dirty. Preserve every existing tracked and
untracked file, including CP-0007/8/9, the complete research tree, the dated
full-grind roadmap, and all existing H05 premeasurement artifacts. Do not
rewrite an existing manifest or custody file merely to include new outputs;
create versioned screen outputs and a new terminal manifest.

## Read first

Read completely before editing or executing:

1. `implementation-detail/phase-4/experiments/h05-canonical-witness/preregistration.md`
2. `target/phase4-h05-canonical-witness-screen-20260821-v1/CONTROL-CUSTODY.md`
3. `target/phase4-h05-canonical-witness-screen-20260821-v1/PRE-MEASUREMENT-VALIDATION.md`
4. `target/phase4-h05-canonical-witness-screen-20260821-v1/PRE-MEASUREMENT-MANIFEST.tsv`
5. `implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md`
6. `implementation-detail/phase-4/test/run-phase4-current-baseline.sh`
7. `implementation-detail/phase-4/test/analyze-phase4-current-baseline.py`
8. `research/phase-4/decision-map.md`
9. `research/phase-4/foundations/benchmark-and-evidence.md`
10. `research/phase-4/foundations/invariant-matrix.md`
11. `research/phase-4/while-waiting-phase-4-to-finish/task-b-witness-authority/report.md`
12. the frozen control and candidate source copies under the H05 artifact
    root, reading them only to understand CLI/evidence-version behavior.

Do not treat the research routing documents as performance evidence. CP-0009
and the new adjacent H05 rows are the performance authority.

## Exact frozen inputs

Before adding orchestration, freeze and verify `pwd`, branch, HEAD, status,
toolchain/environment, and every existing premeasurement-manifest entry.

Required hashes:

```text
CP-0009 control source
3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a

CP-0009 control source diff from HEAD
b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84

CP-0009 control executable
9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7

H05 candidate source
e675d2fc7646745eaf709f61703ff84098949ce4319cb4e6882b96698d95d031

H05 candidate executable
15a668739e96de064a5a7dff1c0b1278406fa077f089687da210e83451e257dd

H05 delta from frozen CP-0009 source
e68864f2f1e3bac7bd3fb5158c0bba11f224e20aaee7ce894ecb42569cb98070

complete H05 source diff from HEAD
c365686b476a8072e919f6ef0328e1b0534b43f28fbea2bceb254477451fd052

H05 preregistration
a35a8abeb735b9a1a098d17bb92e7578c26af5765f25ad72b146b8754f6fc9c3

retained 100-MiB fixture
size       104857600 bytes
SHA-256    63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
```

The exact frozen executables are:

```text
target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009
target/phase4-h05-canonical-witness-screen-20260821-v1/candidate/phase4_create_edit_benchmark-h05
```

Do not invoke Cargo, rustc, or a build command. Do not use a newly built
`target/release` executable. A custody mismatch is a hard stop; do not repair
or regenerate the frozen operand.

Confirm before edits that the H05 artifact root contains zero H05 warmup or
measured rows. The copied historical `control/cp-0009.raw.jsonl` is not an H05
row and must remain byte-for-byte unchanged.

## Sole implementation scope

Add the smallest dedicated orchestration needed for the screen, preferably:

```text
implementation-detail/phase-4/experiments/h05-canonical-witness/run-screen.sh
implementation-detail/phase-4/experiments/h05-canonical-witness/analyze-screen.py
```

Use the standard library and existing repository patterns. Add no dependency,
framework, generic benchmark abstraction, or reusable campaign system.

The scripts may create only new versioned outputs beneath:

```text
target/phase4-h05-canonical-witness-screen-20260821-v1/
```

Preserve all existing files there byte-for-byte. Put new preflight, command,
environment, base-custody, smoke, raw, analysis, summary, and terminal-manifest
outputs in clearly named new files or one new screen-results subdirectory.

Do not edit:

```text
crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs
target/.../control/phase4_create_edit_benchmark-cp0009
target/.../candidate/phase4_create_edit_benchmark-h05
target/.../control/phase4_create_edit_benchmark-cp0009.rs
target/.../candidate/phase4_create_edit_benchmark-h05.rs
```

## Exact dry-run schedule assertion

The runner must construct one complete plan and compare it to this literal
expected plan before preparing a fixture, copying a base, running the protected
smoke, or launching either executable:

```text
pair 0  warmup   AB
pair 1  measured AB
pair 2  measured BA
pair 3  measured AB
```

Exact row sequence:

```text
A B | A B | B A | A B
```

Where:

```text
A = frozen CP-0009 control
B = frozen H05 candidate
```

The dry-run mode must emit the constructed and expected plans, assert exact
equality, report the executable/source/fixture hashes it would require, and
exit before any preparation or execution. Preserve that output.

The normal execution path must call the same plan constructor and assertion;
there may not be a separate unchecked timing schedule.

## Screen-input custody

Before the timed command:

1. verify the exact retained fixture size and SHA-256;
2. prepare each pair once outside both arm timers;
3. physically copy byte-identical database and authority starts to isolated A
   and B paths;
4. construct arm-appropriate expectation files from the same exact fixture:
   the control uses its existing expectation version, while the candidate uses
   `LFS-H05-EXPECTATIONS-1` and its independently computed fixed canonical
   commitment;
5. record source/base/authority/expectation hashes for every pair and arm;
6. prove A/B database and authority copies for a pair are byte-identical before
   execution, while explicitly accounting for the intentionally versioned
   expectation difference;
7. keep all preparation outside the measured operation boundary;
8. record logical/apparent/allocated main/journal/sidecar endpoints and residue
   using only supported observations.

Do not reuse a mutated arm as the next arm's base. Do not infer physical I/O
from logical bytes, allocation, wall time, RSS, Q, or SQLite pager counters.

## Lock, host quiescence, and timeout

Before the protected smoke or any screen row:

1. confirm no other Cargo, benchmark, SQLite campaign, compression, profiler,
   or filesystem-intensive research process is active;
2. record the quiescence check and environment;
3. acquire and record `BENCHMARK_LOCK=H05_SCREEN` using one exclusive local
   mechanism that fails closed if already held;
4. ensure the lock is released on every success/failure/interrupt exit;
5. enforce one hard wall ceiling of 120 seconds for the complete screen
   command, including protected smoke and all eight scheduled rows;
6. on timeout, preserve every completed/started row, record the timeout, mark
   the screen failed, release the lock, and stop without rerun.

Do not use a blocking wait that hides output for more than 60 seconds. The
screen itself may run up to its prospectively frozen 120-second ceiling, but
the agent must keep the user informed before and after it.

## Required protected candidate smoke

Before the measured plan, run exactly one non-controlling candidate smoke on
the exact frozen candidate. It must protect:

- same-count edit;
- `+1` early and `+1` middle edits;
- warm and fresh logical materialization;
- returned authenticated 1-MiB range;
- reopen/head;
- fresh scrub;
- reconstruction and exact ranges;
- exact identities, transaction/COMMIT count, resource cleanup, and terminal
  `Q=0`.

The smoke is a correctness/resource gate, not a performance operand. If it
fails, emit no measured row, preserve the failure artifact, classify the
screen `REVERT / H05 LOCAL NO-GO`, and stop.

Do not substitute the old release self-test for this exact protected smoke.

## Row and evidence contract

Preserve every started row. Never selectively rerun, delete, replace, truncate,
or relabel a row. No warmup or measured arm may be restarted after it produces
an artifact or begins its measured boundary.

The final raw screen evidence must contain exactly:

```text
2 warmup rows
6 measured rows
8 total scheduled rows
```

The independent analyzer must parse the raw rows rather than trust a runner
summary. It must verify at least:

- exact plan and row order;
- warmup/measured labels and pair membership;
- exact frozen executable/source/fixture/base custody;
- arm-appropriate expectation versions;
- every semantic result is PASS;
- exact source, CDC, canonical object, mapping, root, transition, closure,
  reconstruction, and range identities/work;
- one writer transaction and one publication COMMIT;
- complete timer equations and no work moved after COMMIT;
- exact Q equation, cap, cleanup, and terminal zero;
- no final journal/WAL/SHM residue or unauthorized serialized metadata;
- supported CPU/RSS/peak/storage observations and honest unavailable reasons;
- protected candidate smoke PASS.

H05 counters are nested in the `canonical_cas_mapping` phase record. Do not
assume they are duplicated at the row top level.

For every 100-MiB candidate row require exactly:

```text
construction_source_hash_bytes                 0
construction_source_hashes                      0
construction_canonical_commitment_bytes   190224
construction_canonical_commitment_entries     5284
construction_canonical_commitment_hashes          1
construction_cdc_entries                      5284
```

For every control row require the frozen current behavior:

```text
construction_source_hash_bytes           104857600
construction_source_hashes                       1
```

Also require control/candidate equality for the unchanged work, including:

```text
raw_bytes_hashed             104857600
raw_hashes                        5284
canonical_id_bytes_hashed    105291554
canonical_id_hashes               5372
canonical_new_write_bytes    105291554
mapping_bytes                    365262
transactions / COMMITs              1 / 1
```

The independently computed candidate commitment must match every candidate
row. Any identity, authority, counter, timer, Q, storage, residue, durability,
transaction, COMMIT, cleanup, or protected-smoke mismatch is an immediate
screen failure regardless of wall time.

## Independent performance decision

For measured pair `i`, compute the candidate effect in the same unit from its
adjacent arms regardless of execution order:

```text
effect_i_ms = candidate_durable_wall_i - control_durable_wall_i

improvement_i_percent
  = 100 * (control_durable_wall_i - candidate_durable_wall_i)
        / control_durable_wall_i
```

Publish every pair's control/candidate wall, effect, and improvement before
the median. Publish arm medians, paired median, wins, min/max/spread, complete
timer components, CPU/RSS/Q/storage, and all limitations.

`RETAIN-FOR-FULL-CAMPAIGN` requires all of:

1. every semantic/resource/storage/custody gate passes;
2. the direct-counter equations above pass in every row;
3. all three measured pairs favor the candidate;
4. paired median durable improvement is at least 5%;
5. no work moves after COMMIT;
6. the protected smoke passes.

Otherwise emit exactly:

```text
REVERT / H05 LOCAL NO-GO
```

A retained screen is not `H05 PASS`; it authorizes only a separately
preregistered full campaign. A failed screen refutes only this exact H05
mechanism on this host/fixture/profile; it is not a universal lower bound.

## Validation before measurement

Before acquiring the benchmark lock or producing any row:

1. run the runner's no-execution schedule mode and preserve its output;
2. validate shell syntax and static path/hash checks;
3. validate the analyzer against synthetic or copied non-measured test input so
   that schedule, missing-row, wrong-order, wrong-hash, wrong-counter,
   transaction/COMMIT, Q, and threshold failures are exercised without
   invoking the frozen benchmark executables;
4. run `git diff --check` on the two orchestration files;
5. verify every original premeasurement-manifest row again;
6. verify the frozen executable/source/fixture hashes again;
7. confirm the H05 artifact root still contains zero H05 warmup/measured rows;
8. record a preflight PASS artifact and freeze the new runner/analyzer hashes.

Repair only orchestration or analyzer defects discovered before measurement.
Do not alter the preregistration, benchmark source, candidate executable,
control executable, one variable, schedule, row count, timer boundary, or gate.

If validation cannot pass without changing a frozen operand or rule, stop
without timing and report the exact blocker.

## Final artifact package

After the single authorized screen, retain at least:

- runner and independent analyzer with hashes;
- dry-run plan/assertion output;
- exact command and environment;
- lock/quiescence/timeout record;
- fixture/base/authority/expectation custody;
- protected-smoke raw/result;
- all eight raw scheduled rows;
- independent analysis and paired statistics;
- semantic/resource/storage audit;
- limitations and every `Unavailable(reason/source)` field;
- terminal report;
- complete versioned manifest covering all new files;
- final read-only manifest verification.

Run `git diff --check` and tracked/untracked status checks at the end. Do not
commit.

## Final response

Report:

1. terminal screen disposition;
2. confirmation that the screen was executed exactly once;
3. exact schedule and row counts;
4. changed orchestration files only;
5. frozen control/candidate/source/fixture hashes;
6. direct H05 counter equations;
7. all three paired durable results and paired median improvement;
8. protected smoke, identities, transaction/COMMIT, Q, storage, and residue
   results;
9. artifact root, manifest count/hash, and independent verification;
10. limitations;
11. explicit statement that no full campaign, integration, WP5, H09,
    canonical-v2, materialization implementation, SQLite tuning, revert, or
    commit was performed.

Stop. Even on `RETAIN-FOR-FULL-CAMPAIGN`, do not launch the full campaign.

---
