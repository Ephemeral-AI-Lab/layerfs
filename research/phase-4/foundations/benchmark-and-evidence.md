# Benchmark and evidence method for Phase 4 research

## What the benchmark must answer

Phase 4 needs four different experiment types. They are not interchangeable.

| Type | Question | Minimum useful output | Cannot claim |
|---|---|---|---|
| Attribution diagnostic | Where is time/work spent? | Disjoint timer equations, direct counters, observer cost, unavailable fields | Candidate speedup |
| Mechanism microbenchmark | Does one primitive have enough causal budget? | Same input/output work, calibrated overhead, cycles/instructions or direct calls | Complete durable improvement |
| Release candidate A/B | Does the complete product path improve? | Frozen binaries, adjacent balanced pairs, exact semantic/storage/resource equality | Portability beyond measured host/fixture |
| Scale/portability campaign | Does the mechanism retain its model? | 100/512-MiB slopes, protected edits/reads, multiple host/storage states where claimed | Universal lower bound |

The current F-series correctly separates these categories in principle. F4-A
and F4-A2 are diagnostics. F2-v3 is the accepted release A/B. F3's reductions
in SQL executions did not become speed evidence because the complete path was
slower.

## Current local performance authority

Accepted F2-v3, exact retained 100-MiB K64/F64 fixture:

| Metric | Control | Accepted candidate | Paired result |
|---|---:|---:|---:|
| Pre-COMMIT qualification | `387.465 ms` | `0.051 ms` | `-99.987%`, 5/5 |
| Combined qualification + COMMIT | `512.861 ms` | `168.477 ms` | `-67.513%`, 5/5 |
| Durable create | `916.310 ms` | `659.593 ms` | `-27.725%`, 5/5 |
| Complete lifecycle | `1,607.986 ms` | `1,353.841 ms` | `-15.772%`, 5/5 favorable |

The accepted candidate performs the same `5,372` creations,
`105,291,554` canonical new bytes, `365,262` mapping bytes, `26,676` dirty
main-database page writes, one transaction, and one COMMIT. Its standalone
COMMIT is slower because verifier work moved relative to dispatch; the
prospectively controlling combined tail and durable total pass. This is an
example of why a single nested phase cannot replace the complete equation.

F4-A's observer-heavy median (`636.836792 ms`) is diagnostic, not a new
baseline. Its five rows expose mechanism sizes; they are not balanced against
the accepted binary and must not be subtracted as realized savings.

## Evidence labels

Each output field must use exactly one label:

- `Observed(source/API)` — the actual counter or timestamp and its unit.
- `Derived(equation)` — all operands and the equation are present.
- `NotApplicable(reason)` — the mechanism truly does not exist in that arm.
- `Unavailable(reason/source)` — the mechanism exists but the platform/API or
  campaign cannot observe it.

Unsupported zeros are prohibited. SQLite cache writes are not VFS write
calls; VFS bytes are not physical-media bytes; apparent file length is not
allocated storage; `Q` is not RSS; wall time is not an fsync observation.

## Causal campaign design

### Before build or timing

1. Freeze branch, HEAD, status, source/diff hashes, toolchain, SQLite version
   and compile options, host, filesystem, power state, and fixture.
2. State one variable and its predicted direct counter equation.
3. Bound optimistic removable wall from an existing direct observation. Stop
   if the ceiling cannot reach the gate.
4. Preregister timer boundaries, units, equations, unavailable fields,
   observer/dispatch overhead, order, warmup, row count, exclusions, and all
   correctness/resource/storage gates.
5. Construct and assert the exact schedule before any measured row.
6. Build each release executable once. Verify source and binary hashes before
   every campaign.

### Row construction

- Generate and hash the source outside timers.
- Prepare each base once, then physically copy byte-identical isolated
  database/authority/expectation triples per arm.
- Run one uncounted warmup and adjacent balanced `AB/BA` measured pairs for the
  current >=5% candidate gate.
- Preserve every row. Never selectively rerun, delete an outlier, or amend a
  threshold after observation.
- Measure the complete affected boundary and all protected neighboring phases.
- Independently recompute the summary from raw rows.

Balanced adjacency controls short-term drift; it does not make five pairs a
universal performance proof. The primary measurement-bias paper by Mytkowicz
et al. shows that innocuous setup choices can reverse conclusions and
recommends randomization/causal analysis ([authors' publication page](https://sape.inf.usi.ch/publications/asplos09.html)).
Google Benchmark's official guidance likewise provides random interleaving to
reduce state-drift effects and warns that statistical comparison needs enough
repetitions ([official guide](https://github.com/google/benchmark/blob/main/docs/user_guide.md),
[comparison tools](https://github.com/google/benchmark/blob/main/docs/tools.md)).

For a small (<5%) effect, a portability claim, or selection among close
candidates, use more than the F-series five-pair screen: prospectively choose
the repetition hierarchy, randomize/interleave candidates, report confidence
or effect-size intervals, and repeat across independent process/host/storage
states. Kalibera and Jones provide a primary methodology for identifying the
levels at which variance occurs before spending repetitions
([paper and DOI](https://kar.kent.ac.uk/33611/)).

## Timer and work equations

Every raw row must close the complete timer hierarchy. For the accepted path:

```text
durable = mapping/construction + qualification + publication/outer-COMMIT

publication/outer-COMMIT
  = pre-dispatch caller work + dispatch-to-return + post-return caller work

complete lifecycle
  = durable + reopen/head + fresh scrub + reconstruction + range verification
```

Nested components are reported but never added twice. A mechanism is removable
only after mandatory replacement work and observer cost are subtracted.

CPU attribution should prefer repeatable retired instructions/cycles or a
controlled profiler for micro-optimizations, while wall time controls the
actual durable decision. SQLite's own performance practice uses a production-
like build plus Cachegrind because wall time is noisy, and explicitly warns
that one compiler/platform/workload does not generalize
([SQLite's official methodology](https://www.sqlite.org/cpu.html)).

## Hard semantic and resource gates

Every candidate row must preserve:

- exact source hash, CDC count and ordered fingerprint;
- exact canonical IDs/bytes, root, transition, closure, reconstruction, and
  ranges;
- exact created/reused/authenticated counts and BLOB/SQL work expected from
  the one variable;
- one synchronous writer transaction and one publication COMMIT;
- `FULL + DELETE`, atomic visible head, and fresh ambiguous reconciliation;
- exact timer and counter equations;
- exact `Q` accounting and terminal zero;
- bounded SQL/heap/output state;
- no journal/WAL/SHM residue or unauthorized metadata;
- protected CPU, RSS, peak footprint, logical/apparent/allocated storage,
  same-count edit, scrub, reconstruction, and ranges.

Any mismatch is `FAIL/REVISE`, even if wall improves.

## Statistical interpretation

For the current >=5% F-series screen, publish all five paired deltas, arm
medians, paired median, wins, min/max/spread, and the controlling threshold.
Do not turn five same-direction pairs into proof of a universal limitation.
Conversely, a candidate that misses a frozen >=5% gate in all five pairs is a
valid local `NO-GO` for that exact implementation and workload.

Research reports should use performance ranges as priors, not promises. A
candidate advances because a prospective experiment passes, not because a
paper, model, or microbenchmark quotes a larger number.

## Evidence package

Retain raw JSONL, stderr/stdout, commands, schedule assertion, environment,
source/diff/binary/fixture/base hashes, exact equations, independent summary,
storage/schema audit, semantic audit, tests/static checks, manifest, and
read-only verification. Historical failed roots stay byte-for-byte immutable.

