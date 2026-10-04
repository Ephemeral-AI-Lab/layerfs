# Group-row acquisition metadata treatment,2026-10-04

Owner follows the 157-state regression investigation with “proceed”. The prior
qualified product remains ed6807c6f, its evidence ab92d1322. The run costs
186.952407542s against182.203124208s reference. Save/custody grows14.435152497s,
filesystem saves9.956693168s. Complete candidate SQL evidence has330219pack
acquisitions and three metadata queries each; all-unit mapping query stepping
costs18.493871s (inclusive/nested observation, not total exclusive attribution).
These are existing receipts, no unchanged resample.

## Treatment

Combine descriptor/control retrieval using SQLite CASE: control is returned only
when its stored length is24..=4120, even if constraints were bypassed. Descriptor,
control length, directory/domain and mapping checks still precede every unit BLOB.
Read mapping rows directly into validated Unit entries through the existing
measured statement lifecycle, avoiding generic Record cells and a second mapping
vector. Query now runs twice/acquisition rather than three times. No cache,
transaction/worker/buffer/publication change, no schema migration, no dependency
change and no integrity waiver. Whole reads still reconstruct full SHA/bytes.
Measured generic statement error handling, lifecycle counters, one attempt and
unknown-outcome quarantine remain identical. Mapping query retains LIMIT257 and
validates every returned entry/count against the authenticated layout.

The existing five-lane whole-pack test gains an exact public statement-work
assertion:6before,5after (includes transaction/descriptor boundary). A new test
bypasses SQL constraints to inject4121control bytes and proves rejection before
any group BLOB opens. The real SQLite observer calibration counts bounded combined
control extraction/acquisition and rejects oversized extraction asNULL. Old
control query observation remains supported; same observer applies to both arms.

## Prospectively selected comparison

Run stride1 Disposable group-rowv3 under the existing300s complete performance
and30s separate unchanged bounded-content proof envelope; one fresh cold sample
per arm, reference then candidate. Fresh paths:
`issue302-unit-metadata-history157-reference1` and
`issue302-unit-metadata-history157-candidate1`. Normal `runner.py run`, case
`phase7-sqlite-disposable-history-stride1-group-rows-v3`, arm baseline/candidate;
candidate uses reference root-pins.json. Rebuild/reuse sealed worktree-local
release/locked vehicles under the same frozen observer/harness; original reference
product7edddbdb8. Record preparation reuse, fixture/binary/product/dependency seals,
source/per-state DB cold/cleanup and every failure. No sharing old performance
because the product/observer identities changed. Use these same real-run count
receipts to validate2metadata statements/acquisition; no separate performance
resampling or stack-trace diagnostic. Original10% time margin and92342273B strict
storage ceiling remain. Original83947520B target deviation is reported separately.

17/53states, Durable, Init, full retained-store payload audit and all-seven
admission are NOT_RUN at this new artifact; older results keep original identities.
PostgreSQL/MinIO M4pause remains unchanged. No CI/retired aggregate preflight.

## Frozen functional evidence

[checks/unit-metadata1](checks/unit-metadata1) records the expected old6-vs-5
failure and repaired full Core checks:510tests/97targets/0ignored, workspace
all-target Clippy-Dwarnings, fmt,453production-file boundary,23tool selftests and
6observer calibration tests PASS. Source is finalized before this verification;
no unchanged repeated checks or performance arms. ProductLOC uses exact
first-parent/staged Rust/SQL snapshots and tools/production_loc.py; comparison
record is stored beside the checks. Final measured outcomes are appended after
both arms complete, never altering historical evidence.

## Final matched result

Source **8ddb4c8e23cbc9731c75ff2016bd75c9dc6ede10**, product/compilation/harness,
observer and fixture identities are pinned in the [sealed comparison index](checks/unit-metadata-final1/comparison.json).
Both raw manifests pass every recorded file length and SHA256 check. Reference
and candidate use the identical new observer seal, frozen harness and prepared
corpus. Each has one fresh source/per-state-DB cold performance sample, with
separate independent proof. The reference original product remains 7edddbdb8.

| Metric | Reference | Candidate | Outcome |
| --- | ---: | ---: | --- |
| Product comparison ns | 181705802667 | 182770115625 | Candidate +0.585734%; unchanged 10% margin PASS |
| Complete performance command ns | 196317434375 | 195555031834 | Both within 300000000000 ns |
| Separate proof ns | 16516482417 | 18850230625 | Both PASS under 30000000000 ns |
| Final allocated storage B | 86179840 | 84926464 | Candidate <92342273 B PASS |
| Save VFS requested B | 11939753738 | 8298991902 | Candidate −30.492772% |
| Save logical pack acquisitions | 271880 | 259270 | Counter scope unchanged |
| Child CPU ns | 130816975000 | 131400953000 | Whole-child scope, not phase-only memory |

Candidate independent root pins, canonical inventory, all 157 custody states,
904143 namespace paths and cleanup match. Bounded proof authenticates 921174
logical content bytes and acquires 5347088 bytes, below unchanged 8MiB/32MiB caps.
Pack bodies remain 74809772 B; canonical inventory remains 104618 objects /
871337620 B. Final storage is exactly unchanged from the earlier group-row
candidate: 84926464 B (80.9921875 MiB), 978944 B / 1.166138% over the original
83947520 B target, inside the owner-approved 10% tolerance and informative 5% band.
Effective total-store/canonical ratio is 9.746677%, or 10.259907× smaller; this is
combined retained encoding/deduplication plus full database overhead, not codec-only.

The same 330219 acquisitions now execute 660438 metadata queries rather than
990657: exactly 330219 fewer, one removed per acquisition. Mapping query still
has 13695193 step calls and 330219 executions, so all **13364974 returned mapping
rows** are still processed/validated. The typed path eliminates one generic cell
vector per mapping row and the intermediate Record collection; no metadata
validation was skipped. Real calibrated observer coverage has zero omitted SQL
classes, zero mapping full scans/sorts/reprepare and unchanged acquisition counts.
Counts from the old candidate are labelled mechanism evidence, with immutable
original receipt hash, not a new sample or an old/new matched speed comparison.

The result does **not** demonstrate full removal of the slowdown: candidate is
still 1.064312958 s / 0.585734% slower than the new matched reference. The earlier
matched pair was +2.606587%; it belongs to its original source/observer/window.
Do not assert the old/new clock difference is an exclusive measured effect of
this change. New Save/custody is 88.594990378 s versus reference 75.973952578 s;
filesystem is 41.901109094 s versus 52.545945870 s. Repeated all-unit mapping work
and separate-row publication remain; no claim attributes every remaining second.
No unchanged-arm resample, deadline increase, extra worker or cache relaxation.

At this artifact, 17/53-state histories, Durable, Init, exhaustive retained-store
payload audit and all-seven admission remain **NOT_RUN**. Historical passes keep
original identities. PostgreSQL/MinIO M4 pause remains unchanged. No CI, retired
preflight, push, PR, merge or release. Source commit production LOC is
**140027 →140045 (+18)**; reference 65417 unchanged, core 74610 →74628. Exact
first-parent/staged changed blobs and the independent full counter agree; see
[production-loc.json](checks/unit-metadata1/production-loc.json).
Final evidence-only commit has **140045 →140045 (delta 0)**, confirmed by exact
parent/staged product blob equality. Its hash is in the final handoff.

The first reference CLI invocation omitted required --baseline-root and was
rejected by argument validation before output/build/cold/child/sample. The log
remains at issue302-unit-metadata-functional1/reference-launch.log; the valid
invocation's reference-launch2.log remains separately. No performance sample
was discarded. The expected pre-treatment statement-count failure is retained
with the full repaired checks. A conflicting git -m/-F option refused before
commit; the staged tree was committed with the recorded exact LOC comparison.
