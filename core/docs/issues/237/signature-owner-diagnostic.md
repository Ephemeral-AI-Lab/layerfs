# #237 C2 signature diagnostic

> One count-only 10k run complete; public row **INCOMPLETE**. Temporary probe
> source was not a speed treatment and is archived, not adopted.

## Question and side-by-side source finding

The v0.1.6 and Core signature functions both calculate the eight smallest
distinct mixed hashes over 16-byte rolling windows. For an eligible file with
no explicit predecessor, v0.1.6 computes the signature during its file
constructor (`crates/layerfs-layerstack-store/src/objects.rs:883-899`) and
passes it to admission. Core computes it on its one C2 owner in
`cas/selection.rs::pending_base_for` before representation selection. Core
passes that signature onward, except that a losing whole-file PREFIX trial
recomputes it in `encoding/delta/select.rs`.

| Signature step | v0.1.6 | Core integrated source |
| --- | --- | --- |
| Rolling-window algorithm | Eight smallest mixed 16-byte window hashes | Same algorithm |
| Fresh small file without explicit predecessor | Existing file constructor computes it; admission receives it | One C2/SQLite owner computes it in `pending_base_for` |
| Explicit predecessor | Admission computes it only if needed | C2 computes it before selection for WholeFile objects |
| Losing PREFIX trial | Reuses saved signature when present | Recomputes despite `prehashed`; diagnostic found zero such trials |
| Candidate index | 1,024-slot session-local | 8,192-slot persisted Store index |

There is no matched v0.1.6 signature-time counter, so the table identifies
code paths rather than a measured v0.1.6 CPU-time advantage.

The sealed 10k fixture manifest has 9,499 files at or below the 128-KiB
whole-file cutoff, totaling 33,747,000 source bytes. The other 501 files
total 266,253,000 bytes. File size does not prove which object role C1 emits;
the diagnostic will count actual whole-file signature calls and traversed
bytes. The original v0.1.6 one-shot result is 750.626 ms; Core's latest
integrated result is 1,110.332 ms, leaving 359.706 ms of raw difference
across distinct observation windows.

## Prospective count-only experiment

This is **not a speed arm**. The temporary dirty-source probe records, once
per file/prerequisite/tree SaveOutcome, signature calls, bytes, nanoseconds,
FULL-loss rescans, delta outcomes, C2 SQL/COMMIT counters, and the existing
SaveProfile. It never emits a per-object log. The exact patch is archived at
[`instrumentation.diff.gz`](evidence/signature-diag/instrumentation.diff.gz); it will
be removed before any product commit. Probe duration is diagnostic only and
cannot be compared to the clean 1,110.332 ms row as a speed claim.

The run will reuse the sealed prepared master outside the timer, make an
independent writable byte copy for this arm, invalidate and check all source
payload pages, recheck residency immediately before the public operation,
and record the final resident-page count. Metadata residency is not qualified.
The public selection is `namespace-10000`, performance-only, one run. Output
is a fresh `benchmark-results/fs-bench-pro/issue237-signature-diag-10k-01`.
The source commit is `970854f2ccbf82bb12f47840d081c7a882e388fd` plus the
archived temporary diff. The benchmark driver is
`docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py`.
The prepared manifest SHA-256 is
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.

The diagnostic preserves all result and telemetry status fields. A full
reopened readback is separate from timing. No second sample replaced the
incomplete result.

## One diagnostic result

The single run produced [raw evidence](evidence/signature-diag/raw/) at
`benchmark-results/fs-bench-pro/issue237-signature-diag-10k-01`. The temporary
patch SHA-256 is
`fc90119fb88db414d12dec8041d6635e7e6a029a70c0be343e79805690058a1d`;
the product seal is
`e99c1dbc25153c7502c51310bd850aecc03d22f822e2d16b1872c01c94234129`.
The Service binary SHA-256 is
`d44ec2c3fb74e6bd96d0212946b7f42c232f59394aafa139d3ef34835217e456`.

| Aggregate | File Save | Prerequisite Save | Tree Save |
| --- | ---: | ---: | ---: |
| Signature calls | 9,399 | 0 | 0 |
| Signature bytes scanned | 33,747,000 | 0 | 0 |
| Signature computation | 96.164 ms | 0 | 0 |
| FULL-loss duplicate scans | **0** | 0 | 0 |
| Delta trials / FULL losses | 0 / 0 | 0 / 0 | 0 / 0 |
| Inserted objects | 24,364 | 11 | 308 |
| Object INSERT statements | 1,393 | 2 | 4 |
| C2 COMMITs | 89 | 4 | 21 |
| Pack rows created | 1,259 | 2 | 3 |
| SQL bucket | 144.634 ms | 0.244 ms | 5.141 ms |
| COMMIT bucket | 246.451 ms | 0.512 ms | 2.550 ms |
| Save owner release | 278.623 ms | 0.222 ms | 1.394 ms |
| SQLite connection release within owner release | 278.611 ms | 0.220 ms | 1.337 ms |

The complete file SaveOutcome, including the other profile fields, is in
[`service.stderr`](evidence/signature-diag/raw/service.stderr). There were
**no PREFIX trials**, so the identified FULL-loss rescan branch was never
entered. The one-line reuse change has no 10k benefit on this fixture. Even
removing all observed C2 signature computation would target only 96.164 ms
locally; the 359.706-ms clean-result versus v0.1.6 difference cannot be
explained by this mechanism alone. That is a local bound across different
source identities and observation windows, not a predicted speedup.

The 278.623-ms file owner release occurs after the
`history.import_finish_save` timer child closes but before the public request
returns. The child reports 6.944 ms; the owner drop and SQLite connection
release are therefore visible only in the public wall and `SaveOutcome`'s
`finish_drop_ns`/`release_connection_ns`. This is a second measured residual
worth investigating independently of signatures. It is not proof that
deferring the drop would improve real work; any treatment must remove the work,
not move it beyond the public timer.

The public response returned the expected root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
Source payload residency was **0/27,503 pages** in the final recheck, with a
6.067-ms recheck-to-timer gap. The public harness receipt is **INCOMPLETE**
because of daemon telemetry loss and retained incomplete stderr; its default
cache field remains `source-cache-uncontrolled-v1`, while the supplementary
cold sidecars qualify source payload only. Metadata residency is unmeasured.
The raw operation time was 1,353.389 ms and is not comparable as a speed arm
because this build adds diagnostic timing and output. In-timer verification
was `SKIPPED`; no separate reopened readback was run for this diagnostic.

## Byte-identical single-owner variants tried

The standalone [microbenchmark source](evidence/signature-diag/signature_microbench.rs)
checks exact eight-hash equality for random and repeated inputs across lengths
0–79, 325, 326, 20,782, 131,072 and 1,048,576 bytes. It then makes one pass
per implementation over 9,399 synthetic in-memory files with the observed
small-file size distribution. It does not read or warm a source fixture, and
its result is **not** a public cold-source performance sample. The
[output](evidence/signature-diag/microbench-output.txt) was:

| Implementation | One-pass function wall | Equality |
| --- | ---: | --- |
| Exact current reference | 95.648 ms | Baseline |
| Algebraically equivalent rolling recurrence | 96.579 ms | PASS |
| Four-window pipelined recurrence | 95.898 ms | PASS |

Both alternatives were equal for the checked inputs and returned the same
aggregate checksum on the synthetic fixture profile. Neither showed a useful
one-pass improvement; both are **rejected as 10k treatments**. This avoids a
costly product build and public run for an approach with no micro-level signal.
No signature algorithm or producer offload was merged into C2.
