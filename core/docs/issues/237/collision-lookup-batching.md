# #237: paged collision lookup experiment

> Status: **NO-GO; treatment remains isolated and unadopted.** Source base
> `970854f2c`, candidate `8e3e03520`. The one-shot matched 10k pair is
> exploratory because source metadata cache state is unqualified and the
> control lost one Service telemetry event.

## Side-by-side mechanism

| Step | v0.1.6 release | Core at `970854f2c` | This candidate |
| --- | --- | --- | --- |
| Fresh-ID eligibility | A 4-MiB bitmap is enabled only if the Store has no pack or object row at admission start. Bit negatives avoid object lookup; positives still use exact SQL and canonical comparison. | The file Save starts after prerequisite objects already exist. It permits overlapping private saves. | No fresh-Store assumption or long-lived negative cache. |
| Collision SQL | The common-source 10k trace issued 438 object SELECTs; the bitmap omitted 24,683 absent IDs. | Each newly written row calls `lookup::candidates` separately, including the owner row. | One existing `lookup::candidates` call per 128 new rows in each preparation wave. |
| Foreign candidate | Exact reconstructed bytes decide equality. | Exact reconstructed bytes decide equality before the wave commits. | Same exact comparison, scope switch, and transaction/lock. |
| Memory | 4-MiB bitmap, only in an initially empty admission. | One row's candidate vector at a time. | At most 128 queried IDs and 128 × 64 returned locator candidates per page; no operation-wide ID set. |

The static bitmap cannot be copied safely into Core. The 10k file Save is not
the first Save, and a concurrent owner may insert a private locator after this
Save begins. A negative result fixed at acquisition could then hide an exact
collision. Maintaining a cross-process bitmap with invalidation would add more
coordination than the measured collision region warrants.

The earlier same-source Core comparison measured **43.921 ms** of file-Save
collision queries. A later pre-ImportBatch integrated diagnostic measured
**42.744 ms**, nested inside **44.292 ms** of candidate validation. Neither is
a measurement of the current integrated ImportBatch source. Even removing the
entire 43.921-ms region from the standalone integrated 1,110.332-ms observation
would leave **1,066.411 ms**, about **315.785 ms** above the 750.626-ms v0.1.6
observation. This is an optimistic arithmetic bound across distinct source
identities, not a speed claim.

## Treatment and checks

`cas/collision.rs` pages the current wave's written rows at 128 IDs, uses the
existing membership SQL and per-ID ownership bound, discards the current Save's
own locators, and groups foreign candidates by identity. For every foreign
candidate it still validates role, length and authenticated canonical bytes
under that candidate's read scope. The current wave retains its SQLite write
transaction through validation, so a foreign writer cannot insert between the
page lookup and byte comparison. Pack data stays in SQLite BLOBs; SQLite pages
remain 4,096 B; the whole-file cutoff remains 128 KiB; the C2 owner remains one
thread.

Focused functional checks: `multi_writer`, `write_admission`, `cas_reuse`, and
`c2_bulk_admission` on the candidate; the Core source boundary guard. Before a
product claim, run the required locked Core tests, formatting and Clippy at the
final identity. Report any failures with their exact outputs.

At candidate source commit `c11d806ea`, the focused locked test command passed
**34/34 tests** across those four test binaries. The added concurrent-writer
case exercises 129 distinct IDs with two private owners, crossing the 128-ID
lookup page boundary and checking that both exact owner rows persist. The
product boundary guard passed. `cargo +1.85.1 check --manifest-path
core/Cargo.toml --locked -p layerfs-storage` passed before the test addition;
formatting was corrected after its first `--check` identified one long import
line. The full Core gate is not yet run.

For the performance pair, use source base `970854f2c` as control and the final
candidate commit as treatment, both through the same
`cold_diagnostic.py --case namespace-10000 --independent-source-copy
--fixed-operation-identity` driver with a fresh `--out` directory per arm.
Prepare each independent byte copy outside the timer, verify final 0/27,503
resident source payload pages and the manifest, keep metadata residency
unqualified, skip in-timer full verification, and perform a separate full
reopened readback. Record exact product/harness/binary seals, host overlap,
public time, C2 collision-query time, Save outcomes, Store geometry, SQLite
page size and failures. Take one sample per arm and preserve both raw receipts.
The parent research lane owns the shared-host timing window; this branch must
not start the pair until that lane releases it.

This treatment predicts fewer SQL calls, not fewer COMMITs. Its expected call
count is the sum over waves of `ceil(new rows in wave / 128)`, whereas the
current path makes one call per new row. The timed receipts do not export this
count or `SaveOutcome.profile.diag.collision_query_ns`; both actual values are
**NOT_MEASURED** for the pair. A new 10k run merely to recover those diagnostics
was rejected after the preregistered public and size gates failed.

## One-shot result and disposition

The [prospective record](evidence/collision-batch/prospective.json) was written
before either public run. It pins exact source/product/harness/binary identities,
fixture, fresh output paths and the go criteria. Both worktrees reused an
independent, validated copy of the sealed 10k master *outside* the timer, then
made a fresh independent source byte copy for their own sample. Each final
preflight found **0/27,503** source payload pages resident; source metadata
residency was not measured. The control was run first, then the candidate, once
each. Their complete raw outputs remain under their respective worktree-local
`benchmark-results/fs-bench-pro/issue237-collision-{control,candidate}-10k-20260923-a/`;
the compact receipts, telemetry, cold checks, source patch, build records,
geometry, readbacks and [hash manifest](evidence/collision-batch/evidence-manifest.json)
are archived beside this report.

| 10k / 300,000,000-B observation | Control `970854f2c` | Paged candidate `8e3e03520` | Candidate change |
| --- | ---: | ---: | ---: |
| Public Init | **1,092.106 ms** | **1,194.798 ms** | **+102.692 ms / +9.40%** |
| Raw throughput | 274.699 MB/s | 251.089 MB/s | −23.610 MB/s |
| Complete performance command | 1,998.939 ms | 2,126.475 ms | +127.536 ms |
| Service file-loop child | 899.175 ms | 950.359 ms | +51.184 ms |
| Service finish-file-Save child | 5.888 ms | 41.568 ms | +35.680 ms |
| Service sampled maximum RSS | 63,258,624 B | 63,438,848 B | +180,224 B; sampling misses boundaries |
| Store apparent / allocated | 333,381,632 / 335,609,856 B | 334,159,872 / 335,609,856 B | +778,240 / 0 B |
| Store object rows / packs | 24,683 / 1,261 | 24,683 / 1,264 | 0 / +3 |
| Pack BLOB capacity | 330,563,584 B | 331,350,016 B | +786,432 B |
| Page size / whole-file cutoff | 4,096 / 131,072 B | 4,096 / 131,072 B | unchanged |
| In-timer verifier | SKIPPED | SKIPPED | perf-only |
| Separate reopened full readback | PASS: 10,000 files / 300 MB | PASS: 10,000 files / 300 MB | same exact root |
| Telemetry / row status | INCOMPLETE / INCOMPLETE | PASS / DIAGNOSTIC | control lost one Service event |
| Collision-query SQL count / wall | **NOT_MEASURED / NOT_MEASURED** | **NOT_MEASURED / NOT_MEASURED** | structural query reduction only |

The root in both arms was
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`;
both separate readbacks walked 10,101 paths and returned the same manifest.
The control and candidate Store files were 4-KiB-page SQLite databases, while
pack bytes stayed in their `object_packs.data` BLOBs. The History DB was
86,016 B in both arms. Each receipt records only Docker Desktop background
processes in its `competing_work` snapshot; the parent research lane's test
process and this worktree's interrupted full Core test had stopped before the
control public timer. These snapshots do not prove an idle host throughout.

The candidate failed the preregistered **public-time** threshold (at least
10 ms faster) and **apparent Store-size** equality. It also did not establish
the collision-wall threshold because that profile was not exported. The three
extra packs show that physical placement differed, but the pair does not
isolate whether this came from altered producer/owner timing or another
runtime factor. The slower file and finish-save children are observations,
not proof that the paged SQL itself cost more. There is no second sample or
adjusted gate. The candidate remains unmerged and should not be used as the
next step toward the 0.751-s v0.1.6 observation; the raw candidate gap grew
to **444.172 ms**, versus **341.480 ms** for its same-window control.

The focused 34 C2 tests, format check, and product boundary guard passed. A
full Core test command was interrupted before the pair began to clear the
shared host; it is **NOT_COMPLETE**. Full workspace Clippy and the tool
self-tests were not run for this rejected prototype. No release qualification
or verified performance claim is made from these checks.
