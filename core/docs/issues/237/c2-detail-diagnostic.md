# #237: C2 collision lookup detail at 10k

> **Status:** Research; informative and not a product contract.

D11 counted the file Save's 24,364 inserted objects and 0.786 s of single-owner
`accept` work, but did not print the already collected `DiagProfile` fields.
The [D12 preregistration](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/preregistration.md#d12-release-10k-collision-validation-detail-diagnostic)
asked whether the one-row-at-a-time candidate lookup in
[`validate_candidates`](../../../crates/layerfs-storage/src/cas/collision.rs)
cost more than 50 ms, enough to investigate a bounded batched lookup. The
temporary [instrumentation diff](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/d12-instrumentation.diff.gz)
only adds reporting around the existing operation; it changes no query or
storage algorithm and was restored after this run.

## One retained diagnostic

The single release-profile public native Init used 10,000 files and
300,000,000 logical bytes. Its full source-payload hash/invalidation and
immediate nonfaulting recheck each saw **0 resident pages of 27,503**; the
recheck-to-timer gap was **6.182 ms** ([preflight](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/cold-preflight.json),
[recheck](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/cold-recheck.json),
[launch](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/cold-launch.json)).
The fresh Store used **4,096-byte SQLite pages** and had 24,683 object rows,
including 24,364 from the file Save ([geometry](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/store_geometry.json)).
The public call returned a root in **1.874694666 s**; the complete command was
**2.809913375 s**. Verification was **SKIPPED**. The daemon dropped one
telemetry event, so the [receipt](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/receipt.json)
is **INCOMPLETE**, even though the operation, Service and daemon cleanup
completed. The original harness still says `source-cache-uncontrolled-v1`:
zero resident *payload* pages does not prove cold directory/inode metadata.
No gate PASS or comparative speedup is claimed.

The retained [Service stderr](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/service.stderr)
reports these nested and disjoint counters for the **whole file Save**:

| Counter | D12 value | Scope |
| --- | ---: | --- |
| File Save inserted / reused | 24,364 / 198 | Existing `SaveOutcome` counts. |
| One-owner `accept` calls | 24,562, totaling 771.409 ms | Within the 1,168.448 ms `history.import_files` child; receiver wait was another 393.324 ms. |
| `DiagProfile::collision_query_ns` | **43.306 ms** | Individual `lookup::candidates` calls inside validation. |
| `DiagProfile::validate_ns` | **44.850 ms** | Contains collision queries; do not add the two values. Its residue is 1.543 ms. |
| Disjoint Save SQL / COMMIT | 188.689 / 227.521 ms | Aggregates over the entire Save, including finish; not exclusive to `accept`. |
| Pack placements / object INSERT statements | 1,259 creations + 6,463 appends / 7,776 | This run's group and statement counts. |

Other already collected D12 spans help locate work without adding them to
the table above:

| `DiagProfile` total | D12 time, ms | Nesting |
| --- | ---: | --- |
| `wave_ns` | 62.249 | Locator/presence query in `flush_batch`; inside its 545.959 ms total. |
| `offer_total_ns` | 410.120 | Whole `offer`, including selection and any seals it triggers; overlaps FULL and some seal totals. |
| `seal_total_ns` | 228.798 | Whole `seal_group`; contains group codec, placement, pack write, row INSERT, member loop and possible commit. |
| `write_pack_total_ns` | 123.805 | Inside `seal_total_ns`; includes its pack SQL, so not additional to the Save SQL bucket. |
| `insert_objects_ns` | 71.157 | Inside `seal_total_ns` and the Save SQL bucket. |
| `probe_ns` | 40.645 | Inside the 69.841 ms FULL encode bucket. |

These spans overlap by design. In particular, `offer_total_ns` and
`seal_total_ns` are neither disjoint nor a complete partition of the owner
wall; a seal can also run outside an offer. The 43.306 ms collision-query
total is one small part of a pipeline with hundreds of milliseconds of
owner, codec, pack, SQL, commit and release work. This report does not infer a
sum or a stable saving from nested diagnostic fields.

The source performs one `lookup::candidates` call per row in
`validate_candidates`; the file Save's 24,364 inserted rows imply about
**1.78 µs per candidate call** from 43.306 ms total. The lookup API already
accepts a slice and pages its input. Batching might reduce calls, but the
entire currently charged lookup span is below the preregistered **50 ms**
investigation threshold. **Reject batched collision lookup as the next 10k
treatment.** Even eliminating all 43.306 ms locally would leave far more than
the 0.428571 s public-call target; channel/worker overlap means this subtraction
is a prioritization bound, not a prediction of a new caller time. D11 and D12
have different instrumented source identities and are not a matched speed pair.

One separate D12 observation deserves a later count-driven diagnosis: the
Save's `finish_call_ns` was **358.793 ms**, of which **350.800 ms** was owner
drop and **350.786 ms** was the SQLite connection release. The named
`history.import_finish_save` child was only 7.991 ms because the timer ends
before owner release ([finish source](../../../crates/layerfs-storage/src/cas/store.rs)).
D11's `finish_call_ns` was 54.223 ms, so this one observation shows
substantial variation, not a stable connection-close cost. It is a larger
single-call observation than the collision queries, but changing when the
connection is released requires separate causal and correctness evidence.

The exact reproduction invocation was
`python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --out benchmark-results/fs-bench-pro/issue237-d12-collision-detail-a`.
The raw output directory remains in the isolated worktree; its receipt,
sidecars, logs, telemetry and Store geometry were copied to the linked
append-only evidence directory. The Store SQLite file remains only in that
private raw output. No second sample of this identity was taken.
