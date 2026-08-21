# Phase-4 current product baseline v1 — preregistration

- Status: **PASS / COMPLETE — CP-0009 current baseline retained**
- Date: 2026-08-21
- Starting HEAD: `febc20f046bba84ccdce1256363d77799eabf2db`
- Parent evidence: CP-0007 and CP-0008
- Profile: unchanged K64/F64 + DIR256K
- Objective: one current-release-binary product-workflow baseline for research
  candidate selection; no optimization or format change.

## One changed variable

Add a fixed 1-MiB authenticated sequential-range operation and a
baseline-specific runner/row vocabulary. No mutation, construction proof,
canonical bytes, identities, mapping topology, SQLite schema, durability,
transaction, COMMIT, or verification behavior changes.

## Product boundaries

```text
durable-submit:
  source/CDC start -> canonical CAS/mapping -> proof -> durable COMMIT return

same-open durable edit:
  authority separately visible -> mutation/proof -> durable COMMIT return

first-edit-after-reopen:
  authority establishment + durable edit

logical materialization:
  authenticated reconstruction into the benchmark sink

fresh-process logical materialization:
  fresh process/SQLite connection + reconstruction; OS cache warm-or-unknown

authenticated range:
  routing + authenticated returned bytes

verification lifecycle:
  publication + fresh reopen + scrub + reconstruction + ranges; correctness,
  not ordinary submit latency
```

## Schedule

At 1 and 10 MiB, one smoke each:

```text
full write
warm materialization
fresh-process materialization
tiny boundary range suite
1-MiB sequential authenticated range (bounded by source length)
reopen/head ready
```

At 100 MiB, one warmup plus three measured samples each:

```text
full write
same-count middle edit
warm materialization
fresh-process materialization
tiny boundary range suite
1-MiB sequential authenticated range
reopen/head ready
```

Also retain one nonmedian 100-MiB `+1` early and middle structural guard. CP-0008
remains the controlling count-changing scale diagnostic.

Expected total: 42 rows. Hard package wall: 120 seconds. Per-command cap: 60
seconds. Build, focused tests, and fixture generation precede timing where
declared.

## Custody

Prepare immutable masters once and copy a fresh database/authority/expectation
triplet per row. Read-only operations at one size share the same authenticated
read master because their base bytes and expectations are identical. Each
child hashes and rejects its complete copied inputs. Rehash every master after
the final row. Delete all transient fixtures and SQLite images.

## Baseline row vocabulary

```text
schema: phase4-current-baseline-v1
purpose: product_workflow_baseline
acceptance_scope: baseline
sample_kind: smoke | warmup | measured | structural-guard
measurement_boundary: one exact boundary from the list above
promotion: false
candidate_comparison: false
cache_state: fresh process/application cache where declared;
             OS/filesystem cache warm-or-unknown
```

Only full-write and returned-byte operations may report throughput. Edit rows
report latency and exact changed work.

## Hard gates

- exact selected profile, fixture fingerprint, CDC sequence, roots,
  transitions, closure, reconstruction, and ranges;
- one transaction/COMMIT for mutations; zero for reads;
- exact timer equations and publication result;
- exact Q with terminal zero;
- unchanged schema/storage/identity work for this no-algorithm-change baseline;
- 1-MiB range returns exactly `min(source length, 1 MiB)` and authenticates
  before exposure;
- no claim of cold physical storage, native checkout, or candidate speedup.

## Deliverable

Retain one raw JSONL, one independent analysis, one report, and one baseline
manifest linking CP-0008 scale evidence. The resulting exact release binary is
the control for the next research-selected balanced A/B.

## Terminal result

The accepted 42-row package completed in 51 seconds. All rows pass exact
identity, transaction/COMMIT, timer, resource, range, and terminal-Q gates.

```text
100-MiB durable submit:             640.109209 ms
same-open same-count edit:            9.737250 ms
warm logical materialization:       425.800708 ms
fresh-process logical materialize:  433.512791 ms
tiny authenticated range suite:       0.770666 ms
authenticated returned 1-MiB range:   3.285167 ms
fresh-process reopen/head:             3.007750 ms
```

The 1-MiB range itself authenticates 60 objects/1,090,255 canonical bytes at
a 315.337-MiB/s median returned-byte rate. CP-0008 remains the count-changing
scale authority. The next candidate must run adjacent balanced A/B against the
exact CP-0009 control binary; historical median subtraction is forbidden.

Controlling report:
[CP-0009](../test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md).
