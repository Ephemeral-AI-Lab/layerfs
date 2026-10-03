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
