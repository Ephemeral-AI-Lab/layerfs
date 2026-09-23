# #237: single-scope SQLite locator experiment

> **Decision: reject as a demonstrated speed improvement.** The candidate is
> retained only on isolated branch `codex/issue237-sql-bulk-admission` at
> `0155bc544`. It is not merged into the root research product. The one-shot
> raw public call was faster, but the preregistered locator-SQL gate missed,
> Store/RSS observations regressed, and both rows lost telemetry. The
> [preregistration](sql-bulk-admission-prereg.md) and
> [raw evidence manifest](evidence/sql-bulk-admission/pair/evidence-manifest.json)
> preserve the question and every outcome.

## Mechanism and source comparison

The v0.1.6 release used 639 multirow object INSERT statements and 590
multi-pack INSERT statements in its earlier 10k common-source trace. The
integrated Core placement-width diagnostic showed that **1,193/1,346** file
placements carry 9–16 locator rows and only **18** create two packs in one
call. Its current-boundary multi-pack INSERT could remove at most 18
statements, so this treatment kept all packs as SQLite BLOBs and changed only
the object INSERT SQL from one scalar TEMP-scope subquery per row to one scope
join per existing bounded statement. The six object binds, at most 128 rows,
row order, same-save visibility, collision checks, pack writes, 4,096-byte
pages, 128-KiB whole-file cutoff, four Init constructors and one C2 owner are
unchanged. Read-only [EXPLAIN plans](evidence/sql-bulk-admission/plans.json)
compile **377→188** opcodes for a 16-row INSERT and **2,617→972** for 128
rows; opcodes are not runtime costs.

The clean treatment commit passed the focused external storage tests:
statement batching 4/4 (including low SQL-length, rollback, scope and
conflict), pack locator 10/10, multiwriter 5/5 and persistence failure 8/8.
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` and
`python3 core/tools/check_product_boundary.py` passed. Full Core workspace
test/Clippy and the sparse-history guard were not run for this rejected
candidate. Its commit records production LOC **117,202→117,200**
(reference 65,417 unchanged; Core 51,785→51,783).

## One matched 10k pair

The [prospective identity record](evidence/sql-bulk-admission/pair/prospective.json)
froze control then candidate, one public sample each, from independent
writable byte copies of the same seed-1 source manifest. Both used the same
[three-line once-per-Save reporting diff](evidence/sql-bulk-admission/pair/reporting.diff.gz)
(uncompressed SHA-256
`0abcfa20d2c1140ed9e8a4147d339a600d50dc00dc54c4ae76f55ee709f111dd`).
Each build was sealed and the differing exact Service and daemon binary hashes
are in that record and the per-arm build receipts. The reporting patch was
restored after the pair. Verification was `SKIPPED` in both timed calls and
run separately afterward. These are **exploratory** rows: source payload pages
were zero at launch, while inode/directory metadata cache state was
unqualified. Both daemon telemetry streams were incomplete, so neither row
is release-admissible.

| 10k/300-MB observation | Control `970854f2c` | CTE `0155bc544` | Candidate minus control |
| --- | ---: | ---: | ---: |
| Public Init, one raw sample | 1,320.471125 ms | 1,119.887333 ms | **−200.583792 ms** |
| Throughput, 300 decimal MB / public wall | 227.192 MB/s | 267.884 MB/s | +40.692 MB/s raw |
| Complete performance command | 2,233.769791 ms | 2,051.065125 ms | −182.704666 ms |
| Service file loop | 959.194292 ms | 938.565125 ms | −20.629167 ms |
| File-Save object INSERT statements | **1,407** | **1,407** | 0 |
| File-Save object INSERT region | 69.379333 ms | 63.860352 ms | **−5.518981 ms / −7.95%** |
| File-Save total SQL bucket | 144.077232 ms | 140.213828 ms | −3.863404 ms |
| File-Save COMMIT count | **90** | **90** | 0 |
| File-Save COMMIT bucket | 261.503170 ms | 247.062040 ms | −14.441130 ms |
| File-Save owner drop | 229.875625 ms | 80.171167 ms | −149.704458 ms |
| Service sampled max RSS, incomplete coverage | 62,685,184 B | 62,881,792 B | +196,608 B |
| Content Store apparent / allocated | 333,385,728 / 335,609,856 B | 333,914,112 / 335,609,856 B | +528,384 / 0 B |
| Pack rows, capacity / spare | 1,261; 330,563,584 / 24,597,985 B | 1,263; 331,087,872 / 25,113,963 B | +2; +524,288 / +515,978 B |
| Payload resident pages at final check | **0 / 27,503** | **0 / 27,503** | same declared state |
| Public row / timed verification | `INCOMPLETE` / `SKIPPED` | `INCOMPLETE` / `SKIPPED` | no admission claim |
| Separate full reopened readback | **PASS** | **PASS** | exact root and 10k files |

The public delta is far larger than the measured **5.519-ms** object INSERT
change. The file Save's teardown/drop span alone changed by **149.704 ms**;
its timing is outside the INSERT region. C2 accept plumbing changed by only
**11.588 ms**, and the named Service file loop changed by **20.629 ms**.
Integrated `ImportBatch` no longer emits receiver-wait time, so that metric is
`NOT_MEASURED` in this pair. The control's and candidate's `competing_work`
lists contain the same Docker processes, but list equality does not prove
equal interference. Lifecycle Service+daemon user/system CPU observations
were **1,828.012→1,784.922 ms**, with different coverage from the public
caller. These scopes cannot be subtracted into a causal explanation of the
200.584-ms raw gap. A prior clean integrated ImportBatch diagnostic at a
different identity was 1,110.332 ms, close to this candidate; no unchanged
arm was rerun to select a preferred number.

The control/candidate source recheck-to-timer gaps were **6.664/1.864 ms**;
both [cold sidecars](evidence/sql-bulk-admission/pair/control/cold-launch.json)
and [candidate sidecar](evidence/sql-bulk-admission/pair/candidate/cold-launch.json)
reported zero payload residency. Their reopened Stores both have 4,096-byte
pages, 24,683 object rows and the identical ordered object-ID SHA-256
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`.
Both public roots and both separate full readbacks match
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`:
10,101 paths, 10,000 files, 101 directories and all 300,000,000 B. The
[control](evidence/sql-bulk-admission/pair/control/readback.json) and
[candidate](evidence/sql-bulk-admission/pair/candidate/readback.json) proof
walls were separate from the public operation.

## Preregistered decision

The directional public-wall criterion (at least 20 ms lower) passed. The
file-Save locator region criterion (at least 15% lower) **failed at 7.95%**.
Exact root/IDs/readback passed. The no-growth Store criterion failed by
528,384 apparent bytes and +515,978 pack-spare bytes; sampled Service RSS
also rose by 196,608 B and did not cover both operation boundaries. The
one-shot pair and incomplete telemetry cannot establish a durable public
speedup. Keep the CTE commit as a rejected isolated experiment and prioritize
mechanisms that move measured C2 work without adding larger transactions or
changing pack storage.

Raw per-arm [control receipt](evidence/sql-bulk-admission/pair/control/receipt.json),
[candidate receipt](evidence/sql-bulk-admission/pair/candidate/receipt.json),
[Service logs](evidence/sql-bulk-admission/pair/control/service.stderr),
[candidate Service log](evidence/sql-bulk-admission/pair/candidate/service.stderr),
[Store geometry](evidence/sql-bulk-admission/pair/control/geometry.json),
[candidate geometry](evidence/sql-bulk-admission/pair/candidate/geometry.json)
and [database hashes](evidence/sql-bulk-admission/pair/db-hashes.json) back the
table. The full failed placement diagnostic and Python interpreter preflight
are retained beside them in the same evidence directory; neither became a
speed sample.
