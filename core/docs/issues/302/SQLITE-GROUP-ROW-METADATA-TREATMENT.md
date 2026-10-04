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
