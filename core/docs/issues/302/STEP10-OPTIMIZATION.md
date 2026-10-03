# Four-path Init optimization: retained first treatment

Source a7ebff1b4ae4735e196721ba2a6e34428e04593a. Release/locked default-profile
matched pairs, one child/arm, source residency zero, independent sampled proofs
and scratch cleanup PASS. Candidate includes engine opening, baseline raw SDK
Init excludes Server::create; both complete child walls include bootstrap.
Strict speed FAIL remains. Numeric Init storage ceiling is undeclared.

| Files | Baseline operation ns | Candidate operation ns | Delta ns | Ratio | Allocated B baseline/current | Speed/joint gate |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 100 | 39,326,291 | 209,471,834 | +170,145,543 | 5.327 | 7,372,800 / 5,799,936 | FAIL / FAIL |
| 1000 | 132,328,709 | 517,663,959 | +385,335,250 | 3.912 | 23,101,440 / 21,737,472 | FAIL / FAIL |

Implementation: bounded shared-budget lane packing, immediate acknowledged S3
bodies, set-based object registration, reservation reuse and checked canonical
leaf decode reuse. No worker/profile/buffer/format relaxation. Public API tests
prove canonical readback, mixed-lane packing improvement, atomic conflicts/
rollback, first-wins races and retained unknown-outcome refusal. This bundle
does not isolate elapsed savings of individual changes.

Separate changed-mechanism diagnostics add PG nested stats and shipped mc trace.
Profiler overhead differs from the preceding fdfbee48f cause diagnostics; compare
work counts, not those rows as a before/after speed pair. Fixed authority/seed
roots still match. Server S3 counts reconcile exactly to client counters, all
responses200; zero trace omissions/invalid records. Readiness sentinel is setup,
trace stops before verifier. Trace is bounded16MiB and redacts bodies/auth.

| Count / aggregate duration | 100 | 1000 |
| --- | ---: | ---: |
| C2/C5 frontend calls | 20/20 | 34/20 |
| Bulk object-registration statements | 5 | 9 |
| Bulk statement execution ns | 3,004,627 | 13,529,248 |
| PUTs | 28 | 94 |
| GETs / distinct keys | 12/12 | 22/22 |
| Server PUT sum ns | 211,966,750 | 598,206,024 |
| Matched CreateFile sum ns | 116,663,041 | 354,379,546 |
| Matched Fdatasync sum ns | 42,434,126 | 140,244,703 |
| Upload window elapsed ns | 106,550,751 | 271,747,583 |

Server request durations overlap across four connections; storage/OS spans nest
inside API/CreateFile spans and must not be added. Temporary write UUIDs from
RenameData correlate measured keys to CreateFile/Fdatasync; unattributed service
events remain separate. This establishes server-side acknowledgement cost but
never excuses the strict gate or justifies weakening durability. Client-minus-
server is not exact network latency.

Earlier fixed-root counts: per-object INSERT346/2003 becomes bulk5/9; reserve9/17
becomes5/10; total PG44/61 becomes40/54. Aggregate SQL rows5/9 are returned lost-
array result rows, not object counts. Final canonical inventory2003 objects /
20,187,652 B confirms the work remains. PUT27/95 becomes28/94, GET5/18 becomes
12/22: mixed-lane synthetic packing improves, but registered Init receives
only marginal PUT benefit and more distinct dependency reads. Reread count is
not repeated fetching of the same key. The fourth path removes one redundant
canonical leaf decode, but required hash/authentication checks remain.

Child/verifier wall and proof scope are in raw receipts and analysis.json.
10000/100000 Init and all three direct history cases remain NOT_RUN; step10/11/M5
remain incomplete. One C5 history stress test retains Busy/55P03 during the
observed autovacuum window; do not claim all tests green. Other owning coverage
595 PASS/zero ignored, final Clippy/fmt/boundary/tools/harness checks PASS.

Append-only raw directories: issue302-opt{100,1000}-{baseline,candidate}-a7ebff1b4,
issue302-opt-cause{100,1000}-candidate-a7ebff1b4, issue302-opt-freeze-a7ebff1b4 and
issue302-opt-analysis-a7ebff1b4 under benchmark-results/fs-bench-pro. Existing
runner commands/case IDs, exact identity receipts/manifests and sealed archives
are retained. Old failure receipts unchanged. Source commit Production LOC:
143542 -> 143581 (delta +39), root exact-snapshot counter incl shipped SQL;
reference65417 unchanged, core78125->78164, old6025 unchanged, new7752->7774,
rest64348->64365.

A distinct hash-backend treatment follows this retained result. Locked sha2
0.10.9 defaults to software SHA256 on aarch64 unless its asm feature is enabled;
that feature is absent. Already-resolved published sha2 0.11.0 selects ARM SHA2
by default. Switching only C2's existing dependency preserves required key/body
validation; the locked package/version/checksum set is unchanged. This is a new
compilation identity, not a relabel/repeat of this treatment.

## Final hash treatment: 85c1260c9

Source85c1260c935d98ce7ec52ed9e7be948d2ee41472, locked release binaries. This
is a separately frozen changed compilation profile, not a repeat/selection of
the first candidate. Baseline rebuilt at unchanged7edddbdb8. Existing source
fixtures, service defaults/resources, constructor/upload counts, byte windows,
format, required validation and acknowledgement rules stay. No third-party
changes or new package/version/checksum. All four independent sampled proofs,
whole-input residency zero and scratch cleanup PASS.

| Files | Baseline raw SDK ns | Current engine-open + Init ns | Delta ns | Ratio | Joint gate |
| --- | ---: | ---: | ---: | ---: | --- |
| 100 | 35,506,542 | 195,970,292 | +160,463,750 | 5.519273 | FAIL |
| 1000 | 124,550,958 | 431,842,125 | +307,291,167 | 3.467192 | FAIL |

These raw observations do not establish parity or a statistical improvement
against earlier windows. In both frozen pairs current remains strictly slower.
The conservative scope asymmetry remains disclosed: current pays required
opening, baseline excludes Server::create from raw SDK timer; no opening term
is subtracted to manufacture a pass. Numeric Init storage ceiling is still
undeclared. Smaller allocation cannot justify the measured speed miss.

| Files/arm | Child command ns /15s | Separate verifier ns /9.5s | Allocated B | Proof paths; selected files / bytes |
| --- | ---: | ---: | ---: | --- |
| 100/baseline | 77,114,375 | 74,599,333 | 7,372,800 | 102; 53 / 3,354,003 |
| 100/candidate | 775,008,166 | 767,002,750 | 5,808,128 | 102; 53 / 3,354,003 |
| 1000/baseline | 186,794,417 | 76,686,417 | 23,101,440 | 1011; 70 / 6,430,827 |
| 1000/candidate | 451,406,833 | 382,220,000 | 21,741,568 | 1011; 70 / 6,430,827 |

Separately recorded complete runner command envelopes include build, service
lifecycle/setup, child, proof and allocation collection. Raw wall ns in declared
order:5,934,619,250;7,896,824,125;5,756,388,917;6,478,925,958, all within15s.
Including proof here is a conservative total-command bound, not a speed metric.
The one1000 cause command12,137,312,458 ns fits its declared25s outer exception
and also15s observed; child15s/verifier9.5s bounds were never enlarged. Final
release build5,991,157,667 ns /30s. Envelopes and prospective freeze retained.

Separate1000 cause diagnostic uses PG/mc observers, so its471,819,333 ns outer
operation and438,287,292 ns project timer are not the default-profile gate.
Trace2965 events/1201307 bytes, zero omitted/invalid; exact server/client
reconciliation93 PUT,18 GET, all200. Fixed authority/seed root agrees with all
preceding cause rows; post-proof inventory2003 objects/20,187,652 canonical B.

| Count diagnostic,1000 | Value |
| --- | ---: |
| Process CPU ns | 178,566,000 |
| C2/C5 frontend calls | 35/20 |
| Implicit C2 + explicit C5 transactions | 35 + 3 = 38 |
| Bulk object-registration statements | 10 |
| Bulk statement server execution ns | 13,347,500 |
| PUT body bytes | 20,038,352 |
| PUT validation aggregate ns | 11,911,622 |
| PUT continue wait / interim exchanges | 0 / 0 |
| PUT final-head aggregate ns | 811,500,256 |
| Server PUT API aggregate ns | 630,920,877 |
| Bounded upload window elapsed ns | 267,990,626 |
| C2 metadata registration inclusive ns | 39,295,833 |
| GET calls / distinct keys | 18/18 |

PUT validation now11,911,622 ns for20,038,352 B (0.59444 ns/B), versus the
preceding software-hash diagnostic60,988,418 ns for20,038,436 B (3.04357 ns/B).
This is an observed5.12x lower per-byte validation cost across those count
rows, not a guaranteed/end-to-end speedup. Process CPU270,540,000 to178,566,000
ns is consistent with less hash work, but covers the whole multithreaded child
and cannot isolate every CPU source. Actual implementation now uses the
published ARM-capable backend while checking every byte. No authentication
check or canonical/physical integrity rule was removed.

Server API630,920,877 ns aggregates concurrent requests; it cannot be added
to upload elapsed267,990,626 ns or nested storage sync spans. Default MinIO
acknowledgement remains the main observed cost. Header/wait simplification and
SHA acceleration do not erase server persistence work. GET18 keys are all
distinct: no claim that repeated-key caching would remove them. Physical pack
counts vary slightly with bounded Init emission/packing; do not attribute the
93 versus94 PUTs or18 versus22 GETs solely to hash throughput.

Disposition: first four paths implemented and measured, but Phase4.5 parity
and strict speed/storage admission remain unachieved. Set-based SQL and
reservation reductions are confirmed; packet utilization gains are modest for
registered fixtures. Further work should target acknowledgement scheduling/
bounded pipeline overlap and read/representation decisions, with correlated
counts and unchanged budgets. C5 retains20 statements; batching that path is
still an opportunity. No further speculative change or unchanged-arm rerun.

| Registered case | Final disposition |
| --- | --- |
| phase7-init-100-direct-engines-v2 | speed FAIL; functional/cache/cleanup PASS; numeric storage ceiling open |
| phase7-init-1000-direct-engines-v2 | speed FAIL; functional/cache/cleanup PASS; numeric storage ceiling open |
| phase7-init-10000-direct-engines-v2 | NOT_RUN |
| phase7-init-100000-direct-engines-v2 | NOT_RUN |
| phase7-history-stride10-direct-engines-v1 | NOT_RUN/unbound |
| phase7-history-stride3-direct-engines-v1 | NOT_RUN/unbound |
| phase7-history-stride1-direct-engines-v1 | NOT_RUN/unbound |

Final changed-hash covering proof275 storage/project tests PASS, independent
SHA256 vectors PASS, final all-target warning-denying Clippy/fmt/boundary PASS.
Unchanged relevant source proofs from the first treatment reused; the C5 stress
Busy failure is retained and unresolved. No claim of all-workspace/CI green.
Known first-treatment compile/quote/S3 assertion and hash-preparation/Clippy
failures remain in compressed checks. Phase memory peaks and off-platform
qualification remain unavailable. Services removed after evidence capture PASS.

Reproduction commands (fresh output; existing identity claims prohibit rerun):
runner.py run --case phase7-init-{100,1000}-direct-engines-v2 --arm
{baseline,candidate} --out <fresh absolute worktree result>, baseline additionally
--baseline-root <worktree>/target/phase7-baseline/layerfs. Count diagnostic:
runner.py run --case phase7-init-work-1000-v1 --arm candidate --out <fresh result>.
Exact argv and full-command clocks are retained in the wrapper envelopes.

Raw append-only custody: issue302-hash{100,1000}-{baseline,candidate}-85c1260c9,
issue302-hash-cause1000-candidate-85c1260c9, issue302-hash-freeze-85c1260c9 and
issue302-hash-analysis-85c1260c9. Each original manifest checked; compact copies
retained. No historical row overwritten/relabelled, push, PR, merge or retirement.

Commits: a7ebff1b4 Production LOC143542->143581(delta+39), reference65417
unchanged/core78125->78164/old6025 unchanged/new7752->7774/rest64348->64365.
85c1260c9 Production LOC143581->143581(delta+0), all subtotals unchanged.
Evidence-only final commit has the same unchanged total, counted from exact
parent/staged snapshots with tools/production_loc.py including shipped SQL.
