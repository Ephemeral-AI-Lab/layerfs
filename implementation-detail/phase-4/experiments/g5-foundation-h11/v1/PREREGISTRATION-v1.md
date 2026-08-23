# H11 retained-control preregistration v1

Status: **FROZEN BEFORE THE FIRST MEASURED H11 ROW**. The oracle and one revision-1 smoke sample were method validation, not evidence rows. No target result root existed when this contract was frozen.

## Question and control

Does current-root work remain bounded while obsolete retained history grows? The sole measured screen uses the deterministic 1-MiB retained G4 implementation, Canonical-v2, FastCDC, K64/F64, SQLite `FULL` + `DELETE` + `temp_store=FILE` + `mmap=0`, `cache_size=1500`, and the accepted first/full batched native materializer. It is a mechanism/history sentinel, not an optimization A/B or population-level latency claim.

The executable is built once in release mode before measurement and bound by `method/METHOD-MANIFEST-v1.tsv`. The frozen G4 sources are included at build time without modification. The source fixture, 1,001-row expected manifest, and deterministic operation log are also hash-bound.

## Chronology

The exact eight-child schedule is `schedule/SCHEDULE-v1.tsv`: `(1,1)`, `(10,1)`, `(100,1)`, `(1000,1)`, `(1000,2)`, `(100,2)`, `(10,2)`, `(1,2)`. Every child creates a private SQLite image, builds the requested revision count, closes/reopens, measures current head, a fixed 64-KiB range, full reconstruction, revision `N+1` same-size edit, and first/full native materialization, verifies selected historical roots, reports reachability/storage/resources, then removes its image and native output.

The repository-global `target/BENCHMARK_LOCK` is acquired with `O_EXCL` and no waiting. Complete wall from acquisition through primary analysis, independently coded recomputation, cleanup, manifest generation, and terminal verification must be `<=20,000,000,000 ns`.

## Identity and hard gates

- Every sample must be `PASS`; its post-edit root/transition/output digest must equal expected revision `N+1`.
- Revisions 1, `floor(N/2)`, N, and prior configured checkpoints are transition-, byte-, occurrence-, and closure-exact.
- History edit counts, transactions, and commits are exactly `N-1`; the first post-reopen edit is exactly one transaction and one COMMIT.
- SQL/query/row/BLOB/authentication/work counters for each current-root operation are exactly equal across all eight rows. This parity gate is not subject to latency materiality.
- Current-live object, canonical-byte, and mapping-byte counts are exactly history-independent; all retained roots are reachable and retained-unreachable count is zero.
- Added-history slopes from N=1 to N=1,000 are at most 16 stored objects, 65,536 canonical bytes, 8,192 mapping bytes, and 131,072 allocated-store bytes per revision.
- Peak whole-child RSS is `<=20,971,520`; every individual buffer is `<=1,048,576`; Q ends at exactly zero.
- Descriptor leak is false; permit leak is false; seed/temp residue is zero; child work roots are absent at terminal; the global lock is removed after terminal verification.
- Logical, apparent, and allocated storage are reported separately. Physical I/O, continuous storage peak, and controlled-cold state remain `Unavailable` because this harness has no VFS/syscall attribution.

## Prospective latency materiality

For each of reopen/head, head lookup, range, reconstruction, first edit after reopen, and materialization, N=1 is control and N=1,000 is candidate, with two fixed samples each. Report all raw values, sum, mean, ratio, absolute mean delta, and both branches. A product-material regression exists only when both exact predicates are true:

```text
candidate_sum * 100 > control_sum * 105
candidate_sum - control_sum >= 2,000,000 ns
```

The rule changes latency disposition only. It cannot waive any identity, topology, exact error, work, authority, durability, reconciliation, cleanup, Q, RSS, buffer, descriptor, storage, chronology, custody, timing-bucket, analyzer-agreement, or observability gate.

## Frozen outcomes

Any hard failure or product-material current-root latency regression yields `H11_REVISE_EXACT_BLOCKER`. Otherwise, exact primary/independent normalized agreement and complete wall `<=20 s` yield `H11_PASS_G5_C_GATE_READY`. No rerun rescues an outcome and no post-outcome gate amendment is permitted.
