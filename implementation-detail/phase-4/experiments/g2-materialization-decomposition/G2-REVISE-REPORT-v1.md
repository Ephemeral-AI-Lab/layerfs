# G2 materialization decomposition — terminal REVISE

Status: `G2 REVISE / DIAGNOSTIC SOURCE REVERTED / G1 RETAINED`

Date: 2026-08-22

Repository / branch: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` /
`codex/empty-worktree`

Starting and retained HEAD:
`d79f0e0e2582d1bc491410224fec2b6cef7482e9`

No checkpoint commit was created.

## Result first

The one authorized G2 campaign ran once, acquired all 18 scheduled rows, and
sealed without a rerun. The primary decomposition and all measured operation
guards were technically exact. The official terminal disposition is still
`G2 REVISE` because the final analyzer incorrectly applied the read-only
`allocated_store_delta_bytes == 0` predicate to the intentionally mutating
`same-middle` edit guard. Both edit arms had the same expected
`16,777,216-B` allocated-store delta and byte-identical resulting database.
The independent recomputation reported PASS/INSUFFICIENT_EVIDENCE, so the
required analyzer agreement failed. The sealed result was not amended.

The diagnostic source was reverted byte-for-byte to the retained G1 source:

```text
benchmark source  157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2
FastCDC source     bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6
```

The technically valid primary result finds no eligible constant-factor
candidate. Four large families each exceed 33 ms, but all four are mandatory
under the frozen authority/output contract. The only statically removable
family, the second Bytes framing decode, is only `0.141476 ms` at median.

## Grounded lanes and cross-review

Three independent read-only lanes inspected actual current source, G1 and
Canonical-v2 raw evidence, benchmark runners, timer/counter definitions,
authority/error/Q contracts, and primary SQLite/Rust sources.

- Pager/storage found that G1 moved page work from COMMIT into mapping without
  reducing final bytes: 24,658 spills, 26,668 dirty writes, 8,753,408-B cache,
  13,086,720-B RSS, and 308.884052-ms total. Read-only `dbstat` found 23,451
  overflow pages among 26,660 4-KiB pages, but total database overhead above
  canonical bytes is only 3.878%.
- Whole-operation traced full create, same/count-changing edits,
  reconstruction/ranges, open/head, and first-after-reopen. It identified the
  338.776/366.357-ms reconstruction boundary as the largest undecomposed local
  operation and confirmed that current “materialize” is only a logical
  hashing/counting sink.
- Concurrency/resource traced the benchmark's single owned `Connection`, the
  production engine's `Mutex<Connection>`, rollback-mode spill locking,
  differing 5,000-ms benchmark versus 100-ms production busy timeouts, and the
  thread-local Q ownership that makes a worker pipeline unsafe without a new
  aggregate accounting design.

Cross-review rejected skip-work counterfactuals as controlling evidence: a
borrowed SQLite BLOB cannot be deferred without retaining about 105 MiB or
rereading the database. It selected exact in-path scalar timers, an observer
ceiling, a checked residual, and no diagnostic concurrency claim. Each lane
agreed that page size, an intermediate spill threshold, and a pipeline should
not outrank G2.

## Ranked direction matrix

| Rank | Direction | Measured/removable budget | Operation effects | Resource/storage/lock risk | Terminal recommendation |
|---:|---|---|---|---|---|
| 1 | Exact G2 decomposition | 328.897-ms control / 332.405-ms instrumented center; no removable >=33-ms family | Observation only; directly describes reconstruction | Fixed scalars; 1.067% parent perturbation; no writes/transactions | Completed technically, but terminal `REVISE` on analyzer disagreement |
| 2 | Destination-authority-gated incremental materialization | Current full reconstruction 328.897 ms; changed-byte benefit unmeasured | Directly targets no-op and same-size/small-change repeated materialization | Requires exact destination identity, mutation custody, atomic publication, fallbacks | Best following mechanism direction after G2 is validly closed |
| 3 | 16-KiB SQLite page profile | 59.404-ms median acquisition is large but mandatory; page contribution inside it remains unavailable | Create/read `?`; small edit/range may regress | 2,000 pages implies about 32-MiB cache; byte fixing stacks a second variable; migration/profile risk | Defer |
| 4 | Bounded ordered pipeline | Standalone CDC gross ceiling 43.787 ms; actual overlapable split unavailable | Create/full-rewrite only; reads/materialization/open unchanged | Worker/cancellation/error order, thread-local Q, aggregate RSS/CPU | Defer |
| 5 | `cache_spill=4096` | No net-wall evidence; G1 already beat 20,000 by 19.169 ms | Large writer only; small edits and reads directly unchanged | About 8 MiB more page cache before overhead; only possible upside is unmeasured later EXCLUSIVE acquisition | Defer |

SQLite primary references used by the decision are the
[cache-spill pragma](https://www.sqlite.org/pragma.html#pragma_cache_spill),
[rollback-mode locking](https://www.sqlite.org/lockingv3.html#writing_to_a_database_file),
[atomic-commit spill sequence](https://www.sqlite.org/atomiccommit.html#cache_spill_prior_to_commit),
[database-status counters](https://www.sqlite.org/c3ref/c_dbstatus_options.html),
[B-tree overflow format](https://www.sqlite.org/fileformat.html#cell_payload_overflow_pages),
and [page-size pragma](https://www.sqlite.org/pragma.html#pragma_page_size).

## Prospective variable and commands

The sole implemented variable was environment-gated, benchmark-private scalar
timing around existing acquisition, authentication, validation, commitment,
fingerprint, and secondary decode work. No existing operation was skipped,
reordered, deferred, or repeated inside the parent.

Frozen identities:

| Item | SHA-256 |
|---|---|
| Preregistration | `0d4007b6493fefc3c8fdd5f6db5a8d31362fb13e747931aea9dfffa5f88504af` |
| G1 control executable | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` |
| Instrumented executable | `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5` |
| Instrumented source | `e5ff84e32547de7116585f03138bb76e898fb337527ab97b14c6794a45ff8c7c` |
| Source-only diff | `a905d044a2cb0440e20d4bd53995196ebaac86724a5932de366b509c02279ec9` |
| Methodology manifest | `a4689069932a7ee0be7a8a72a6a85f895812d314f61488f11ac5c42b8e06dbd3` |
| Dry-run | `0835e284bbe4d39af46b999dd70670427a869c7b16f34de93f9b35aaca4d7d8f` |

Premeasurement commands passed:

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark g2_materialization_decomposition_
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark row_json_
cargo fmt --all -- --check
git diff --check
cargo build --offline --release -p layerfs-engine --bin phase4_create_edit_benchmark
python3 implementation-detail/phase-4/experiments/g2-materialization-decomposition/run_g2.py --dry-run
```

The candidate release executable was built once. The dry-run created zero
rows, zero database copies, and zero benchmark children, and froze equal
primary temporal centers of 6.5.

## Exact schedule and primary result

Schedule:

```text
uncounted warmup: AB materialize-warm
observer probes: 5 at the exact warmup-B timer count
measured primary: AB / BA / AB / BA materialize-warm
guards: AB materialize-fresh; AB read-range-1m; AB reopen; AB same-middle
```

Rows: `18/18`; measured/non-warmup rows: `16`; no replacement or selective
rerun. Primary analysis completed through its 20-second gate; its exact elapsed
was not retained because the later failure status replaced the success status.
The sealed total was `24.241535459 s`, below the 120-second ceiling.

Primary warm reconstruction:

| Pair | Order | G1 control | Instrumented | Ratio |
|---:|:---:|---:|---:|---:|
| 1 | AB | 326.433042 ms | 326.823833 ms | 1.001197 |
| 2 | BA | 331.490292 ms | 336.040125 ms | 1.013725 |
| 3 | AB | 326.732167 ms | 331.519125 ms | 1.014651 |
| 4 | BA | 330.932708 ms | 335.236875 ms | 1.013006 |
| Position-balanced center | — | **328.897052 ms** | **332.404990 ms** | **1.010666** |

Both positions passed: position 1 ratio `1.027729`; position 2 ratio
`0.993841`. All 4/4 pairs were within the 5% observer-equivalence gate.

The exact timer-region count was `32,307` in every B row. Five complete
observer probes were `1.703042–1.800250 ms`; the maximum `1.800250 ms` passed
the `3.385132-ms` gate. Direct timers plus raw residual equaled the parent in
every row.

## Reconstruction decomposition

These are component-wise medians across the four measured B rows. Components
are disjoint; the median cells may come from different rows and are not added
as a synthetic median parent.

| Family | Median | Min–max | Approximate parent share | Removable under current contract |
|---|---:|---:|---:|:---:|
| Canonical authentication | **94.816564 ms** | 93.913153–95.427038 | 28.52% | No |
| Closure commitment | **88.483070 ms** | 88.034252–89.149856 | 26.62% | No |
| Source fingerprint / logical sink | **87.889943 ms** | 86.808772–88.409192 | 26.44% | No |
| SQLite/BLOB acquisition | **59.403771 ms** | 55.794692–61.152901 | 17.87% | No |
| Occurrence commitment | 0.408711 ms | 0.394961–0.434994 | 0.12% | No |
| Mapping/topology validation | 0.199333 ms | 0.168215–0.269459 | 0.06% | No |
| Second Bytes decode/length | **0.141476 ms** | 0.135557–0.144867 | 0.04% | **Yes, but below gate** |
| Raw residual | 1.671903 ms | 1.573096–1.779415 | 0.50% | Ineligible composite |

Authentication, closure, source/output verification, and acquisition each
exceeded 33 ms in every row and both positions, but none can be removed while
preserving exact identity, closure, output, and storage authority. Removing
the source fingerprint from this benchmark would move required exact output
verification outside the timer, not implement native output. The second decode
is a real local redundancy but its `0.141476-ms` budget is decisively too small.

The fresh-process B guard was `331.838083 ms` reconstruction plus
`2.722583 ms` open/head, with OS/device cache still
`warm_or_unknown_after_manifest_preflight`. Its decomposition corroborated the
warm result. It is not controlled-cold evidence.

## Exact work, cache, memory, and storage

Every timed one-pass reconstruction retained:

```text
source/output bytes                         104,857,600
chunk references                                  5,284
authenticated objects                              5,371
authenticated canonical bytes                105,122,401
borrowed chunk BLOB reads / bytes          5,284 / 104,926,292
leaf batches / references                       83 / 5,284
read-operation SQL queries / rows                170 / 5,371
transactions / COMMITs                               0 / 0
Q operation high-water / terminal             32,195 / 0
```

Warm B rows observed current SQLite cache `8,753,408 B` before and after the
timed pass, `6,010` hits, `3,240` misses, zero cache writes, zero spills, and
zero status errors. Fresh B began at `18,944 B`, ended at `8,753,408 B`, and
observed `5,971` hits / `3,279` misses. These are connection-local logical
cache counters, not physical I/O or a true high-water.

Measured warm-row process resources include the untimed primer plus timed
pass:

| Resource | Control center | Instrumented center |
|---|---:|---:|
| Maximum RSS mean | 18,857,984 B | 18,706,432 B |
| Maximum RSS observed max | 20,398,080 B | 19,677,184 B |
| Peak footprint mean | 13,496,710 B | 13,488,518 B |
| User CPU mean | 1.0325 s | 1.0375 s |
| System CPU mean | 0.0925 s | 0.0975 s |

All read/materialization/open rows retained the exact database hash
`7db8d50d...9890`, logical database `109,199,360 B`, allocated store
`109,203,456 B`, zero allocation delta, and no journal/WAL/SHM residue.

The evidence namespace itself is bounded but large because the protocol
retains 18 private 100-MiB database copies: approximately `2,129,952 KiB`
apparent and `2,162,736 KiB` allocated by `du`. This is campaign evidence, not
candidate durable storage. No evidence file was deleted after acquisition.

## Guard results and analyzer defect

| Guard | Control | Instrumented | Exact result |
|---|---:|---:|---|
| Fresh reconstruction | 332.175208 ms | 331.838083 ms | PASS |
| Returned 1-MiB range | 2.991125 ms | 2.127584 ms | PASS; single guard pair only |
| Reopen / visible head | 2.792375 ms | 2.540042 ms | PASS; single guard pair only |
| Same-middle durable edit | 7.639500 ms | 6.947333 ms | Semantic/performance PASS; analyzer predicate defect |

Both edit arms produced the same root, transition, closure, final database
hash, logical endpoint `109,314,048 B`, allocated endpoint `125,980,672 B`,
and expected `16,777,216-B` allocated-store delta. The candidate was not an
edit optimization and no speed claim is made from the single pair.

The final analyzer's unconditional zero-allocation predicate generated exactly:

```text
17-guard-same-middle-pos1-A:allocated-store-delta
18-guard-same-middle-pos2-B:allocated-store-delta
```

This is a methodology bug, not permission to amend the sealed gate. The final
analyzer therefore emitted `G2 REVISE`; the independent analyzer emitted
`G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE`. Their
disagreement forced the official terminal REVISE disposition.

## Evidence custody

| Artifact | SHA-256 |
|---|---|
| Raw JSONL, 18 rows | `6f7124cc8d4fdd248b89770da5576f2546f105304e3d486ddb2f9c7ce5352af2` |
| Primary analysis | `0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282` |
| Final analysis | `e926187cd9da28647b3b4695616efe237192721105270844750ac49e5a35bb21` |
| Independent recomputation | `803c9658a3a3ab4238a15a03fe8b7ec8dcef7a313bfee5af4530533fdd5ee5d7` |
| Status | `1f4112e0bd48a44000f0096b2c7db5a1d9ac3892672ce2ff7356035ba513e97c` |
| Payload manifest, 178 entries | `28c1b86a3fd3715785617da84195e5ed2cbd5a880dcc883f57f8e51d5edd2d13` |
| Terminal | `b859de6dce9aef9caba43dbf43fd5eb2b7ea24630f7f18ff206749d431e6f2a1` |
| Terminal verification | `d004339854fded0c39af5a7b05a6fea78e398a703846e5eec43ad180f971b1be` |

The result root and files are read-only, its execution lock is absent, and
terminal manifest verification is PASS.

## Operation-by-operation disposition

- Create/write: retained G1 remains 308.884052 ms with its 8.35-MiB cache and
  12.48-MiB accepted RSS peak. G2 changed and measured no create mechanism.
- Same-count and count-changing edits: no retained change. The same-middle
  guard had exact parity; one-byte and count-changing evidence remain the
  retained Canonical-v2 results.
- Cold materialization: unavailable. Neither arm controlled OS/device cold
  state or wrote a native destination.
- Warm/fresh logical reconstruction: exactly decomposed; no removable 33-ms
  constant-factor lane selected.
- Incremental/native materialization: not implemented or measured.
- Authenticated reads/ranges: exact 1-MiB guard passed; no retained change.
- Open/head and first-after-reopen: open/head guard passed; G2 did not alter or
  remeasure first-after-reopen authority.
- Concurrency: `NotRun` because the selected variable was non-retained scalar
  observation with no thread/connection/cache/lock-policy change. G1's
  concurrent-reader latency and first-spill/EXCLUSIVE interval remain
  unqualified.
- Storage: candidate read paths wrote zero bytes; edit A/B storage was exact.
  No format, schema, migration, sidecar, value log, or second durable copy was
  added.
- Memory/Q: bounded scalar observation only; operation Q remained 32,195 and
  terminal zero; measured RSS stayed below the prospective 20-MiB diagnostic
  guard.

## Validation and stop boundary

Before the campaign, focused G2 parity and row-JSON tests passed. Because the
sealed terminal result is REVISE, the full workspace/Clippy static-closure
suite was not run. After source reversion:

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark g1_writer_memory_  PASS
cargo fmt --all -- --check                                                         PASS
git diff --check                                                                   PASS
```

No 500-MiB work, page-size experiment, second spill threshold, worker/pipeline,
native materializer, G3 implementation, WP5/Phase 5 work, push, amend, merge,
rebase, reset, clean, or evidence deletion occurred.

## Following Phase-4 step

Do not start G3 from this terminal state. G1 remains the clean source baseline,
but G2 is not formally closed because its analyzers disagree. If the work is
continued, use a fresh G2-v2 namespace and preregistration that scopes the
zero-allocation predicate to read-only guards while requiring exact A/B storage
parity for mutating guards. Do not rerun or modify this v1 namespace.

If a valid G2-v2 confirms the technically observed result, select no
constant-factor micro-optimization. The next mechanism direction should then
be destination-authority-gated incremental materialization, with no-op,
one-byte, 1-MiB replacement, invalid-authority fallback, fault atomicity,
native storage/durability, and causal concurrency guards. The current evidence
does not justify 16-KiB pages, a higher spill threshold, or a worker pipeline.
