# Stage 1.1 terminal source and evidence audit

Disposition: **Stage 1.1 correctness/durability PASS; Verified Stage 1.1M
performance `REVISE_NO_AUTHORIZED_OWNER`; TrustedLocalDev remains a separate
`PASS_PRIMARY_TRUSTED_CLASS`.**

The final product source is
`d1848200d249915d3f1e35af5556fdf6c1ec05c6`. The clean release evaluator is
4,026,320 bytes, SHA-256
`b056b535c7d3e0711a120731e414bbff213ca0be9c6a603cc3387da6633af624`,
and BLAKE3
`8fe897685cda24c850d58c35e27687a02389747232fcc862337bcf7de234ef01`.
The source manifest is SHA-256
`3b52e3ca1539bee8cacf4cf7a5243af88d494b78195917a53f5492c27f799218`.

## Final current-source regression

Attempt-024 passed the exact public SDK/VFS/Apple campaign:

| Gate | Exact result |
|---|---:|
| Rows | `47/47` |
| Edit/sub-edit operations | `51/51` |
| Durable transitions / COMMITs | `34/34` |
| Physical / canonical oracles | `51/51` / `34/34` |
| Save bursts / selected history | `4/4` / `8/8` |
| Complete = rows + outside rows | `12,745,738,083 = 8,355,633,375 + 4,390,104,708 ns` |
| Operation Store + scratch SQL | `7,242 + 103,338 = 110,580` |
| Admission + operation SQL | `22,674 + 110,580 = 133,254` |
| Storage-observation SQL | `54 * 3 = 162` statements |
| RSS / buffer / Q high / Q terminal | `28,442,624 / 1,048,576 / 8,388,607 / 0 B` |
| Store connections high / terminal | `2 / 0` |
| FD baseline / terminal | `5 / 5` |
| BUSY / LOCKED / rematerialization | `0 / 0 / 0` |
| Owned temp / sidecar residue | `0 / 0` |

The exact raw hashes are:

```text
rows.jsonl       57a11f4c85da0d5105b9ecb5954780770db00d18602b7610ac4d3b25e66bff6e
summary.json     168c4fa2dd118861be845b9383b6ba1919c2f25df50bd824f68a689a2b43b2f5
campaign-time    cf2bfd9cc01050b240958a077b76ef46b9db21cd4770cdc8d15f3c1c1e8ad673
```

Attempt-021 is the preserved 4/47-row failure that exposed the three trailing
storage-observation statements outside the old phase partition. Attempt-022
completed 47/51/34 but is a preserved FAIL because the fresh worktree had not
staged the exact accepted attempt-007 comparison artifact. Attempt-023 stages
the exact artifact and its evaluator summary says PASS, but independent audit
correctly rejects it: C08-001 serialized scratch `{2,20242,62540}` instead of
the additive Engine+VFS `{3,20263,62544}`, omitting 21 statements. Attempt-024
closes that predicate and is the controlling audited result.

## Accounting and fault closure

The terminal review repaired and focused-tested:

- identity-safe Apple failed-open cleanup at all twelve setup cuts;
- portable fresh/nested/hard-link M7 fault cuts and sync ordering;
- every executed Store SQL family on success and failure;
- partial fetched/authenticated/decoded work on integrity failures;
- source and candidate scratch receipts through compaction exits;
- terminal live-scratch rollback on complete, capture, discard and C09 routes;
- authority/topology scratch deltas on replace, rename and refresh;
- max high-water across sequential receipts and additive peaks only for
  simultaneously live distinct tables;
- disjoint `storage_observation` phase ownership for all 54 diagnostic reads;
- operation and through-row projection fact mutation rejection;
- Verified-after-Trusted reachable canonical-substitution rejection.

The final Store phase equation is:

```text
store_open 2
+ materialization 969
+ checkpoint 1,814
+ logical_edit 638
+ apfs_refresh 363
+ canonical_witness 1,626
+ verified_open 72
+ history_read 1,596
+ storage_observation 162
= 7,242 Store statements
```

The successful live-scratch operation owners are `native_edit` (54
statements), `apfs_refresh` (1,719), materialization (83), and C09
`explicit_cleanup` (one terminal rollback). C07 keeps one `33,304 B` peak
instead of multiplying the cumulative table high-water across sub-edits.
C08-001 now adds disjoint Engine and VFS scratch counts exactly:
`{2,20,242,62,540} + {1,21,4} = {3,20,263,62,544}`, while its high-water
remains the maximum `74,816 B` rather than a sum.

## Frozen performance disposition

The Verified performance population was not rerun. Its product-operation
latencies remain:

| Size | p50 | p95 | p50 MiB/s | p95 MiB/s | Result |
|---:|---:|---:|---:|---:|---|
| 0 MiB | `24.071333 ms` | `24.648250 ms` | N/A | N/A | measured |
| 24 MiB | `62.191459 ms` | `65.981500 ms` | `385.905` | `363.738` | FAIL |
| 96 MiB | `179.337500 ms` | `183.878333 ms` | `535.304` | `522.084` | PASS |

Fitted sustained bandwidth is `614.617 MiB/s`; fitted intercept is
`23.142779 ms`. Exact misses remain `8.858126 ms` (24 MiB p50), `7.314500
ms` (24 MiB p95), and `3.142779 ms` (fixed cost). The M1 `0.489333 ms` p95
excess remains `NONMATERIAL_MICROVARIANCE`, not a numerical PASS.

All 48 M7 v1 rows aliased `row_wall_ns` to product-operation wall. They remain
useful for the frozen product-operation latency/throughput denominator, but
are explicitly nonconforming as exact row-wall evidence and were not silently
promoted. The CPU scaling proof passes: p50 CPU `4.924/41.779/158.535 ms` at
0/24/96 MiB, fixed-subtracted per-MiB ratio `1.041995659 <= 1.25`.

The separate TrustedLocalDev attempt-003 remains `629.087/584.219 MiB/s` at
24 MiB p50/p95 and `1140.715/998.260 MiB/s` at 96 MiB. It is explicitly weaker
than Verified, selected only at Store open, and cannot cross publish/export/
share boundaries without close plus Verified reopen and retained-union scrub.

## Boundary and qualifications

No P0 or P1 remains. Five narrow P2 evidence qualifications are retained:

1. a scratch `ROLLBACK` I/O failure cannot return both its attempted-close
   observation and error through the current result type;
2. failure to obtain the first Apple creation identity safely preserves residue
   instead of deleting by name;
3. failed initial Verified-open diagnostics are internally proved but not
   caller-visible;
4. Engine compaction/integrity exports aggregate scratch totals, not fabricated
   owner/derived/operation subfamilies;
5. failed managed operations cannot return partial live-scratch diagnostics
   through the current error type, while every successful prescribed route is
   exact.

Stage 1.2 and Docker/FUSE were not started. The user-owned untracked `poc/21`
and `poc/23` specifications were preserved untouched.
