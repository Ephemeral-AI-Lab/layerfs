# Concrete importer optimization results

Status: Research; informative and not a product contract.

Two optimizations are implemented in the external MinIO+SQLite importer:
prepared file-root UPDATE reuse and exact-size small-file input with explicit EOF.
All registered commands and exact output proofs passed. The SQL UPDATE region
showed32.41% less elapsed time in its isolated pair. The small path issues9.13%
fewer read calls and requests25.43x less cumulative input buffer capacity, but its
observed command-wall reduction cannot be attributed cleanly to the treatment:
file-open time differed between the independent trees and read time worsened.
No end-to-end import speedup, cold speed or product admission is claimed.

[Prospective contract](CONSTRUCTION-OPTIMIZATION-V1.md) committed at
f97fee392fffa513306b02e24b92c57e0863cdcc before implementation. Treatment/runner
source91af4e2a63349f382bf70b7db6d19de81eed0980, published before collection.
Owned branch codex/issue290-storage-probes; worktree
/Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs. No product source,
codec, chunk/whole-file thresholds, compression level, SQL cache quota, transaction
cadence, producer count or durability setting changed. No source repository edits,
MinIO upload/server, #288 campaign or historical receipt changes.

## One-attempt mechanism comparisons

| Case | Complete command wall | Command result | Exact proof |
|---|---:|---|---|
|O-root-uncached-v1|0.986772s|COMPLETE|PASS|
|O-root-cached-v1|0.566092s|COMPLETE|PASS|
|O-small-stream-v1|11.809248s|COMPLETE|PASS|
|O-small-sized-v1|10.875895s|COMPLETE|PASS|

Root arms <=15s; separate bidirectional root/metadata proofs1.494470s and1.270273s
<=10s. Both cover103,108 original roots, matching all original entries/xattrs.
Small arms <=25s declared count exception and cover every101,494 nonempty file
below128KiB,523,127,919 bytes. Each arm compares every canonical root against the
independently reconstructed original and checks exact input size/EOF. Byte-oracle
acquisition validated every SHA256 and produced independent ordinary byte copies
for both trees together once in35.869011s. No APFS clone/source hardlinks for
source bodies; source fixtures reused once, not regenerated per arm.

All numerical rows are cache INELIGIBLE/performance_claim=false. Setup writes can
leave pages resident, inter-arm OS cache equality is unverified, metadata identity
reads can warm reference pages, and root catalog fs::copy may use OS copy/clone
behavior. Performance results are exploratory observations, not admitted matched
cold comparisons. Existing original full import34.5s/4.6s and attribution receipts
remain their original sources and are not treated as a baseline here.

## Root statement reuse

Uncached UPDATE elapsed0.573948s versus cached0.387906s across103,108 calls:
5.566us versus3.762us/call, observed reduction32.41%, absolute0.186042s. Statement
reuse replaces repeated Connection.execute preparation with prepare_cached;
query/binds/root values and256-file COMMIT/BEGIN cadence unchanged. Both arms
have402 cadence batches plus1 final commit;0.032706s versus0.031253s transaction
elapsed. Object construction, compression and821 interleaved object commits are
excluded equally, so this is not the full import SQL schedule. Complete walls also
include fresh private catalog setup/metadata enumeration; their difference is not
credited entirely to statement reuse. Actual Rust-linked SQLite3.51.0, MEMORY/OFF,
cache512KiB/mmap0, no fsync or new WAL setting.

This cached update is the normal external importer path now. No saving in the
full import's1.463s original root-update span is predicted from this isolated pair.

## Known-size small input

Normal importer uses the sealed manifest size for files<128KiB, allocates that
bounded amount, read_exact plus one-byte EOF check, then public construct_bytes.
Empty files preserve their canonical empty representation and large files retain
construct_stream. This is for closed immutable import inputs; mutable Workspace
size metadata cannot become an authority for skipping EOF or capture validation.
The SQL mode baseline remains only in the registered external comparison.

| Metric | Stream baseline | Exact-size treatment |
|---|---:|---:|
| Read calls |223,387|202,988|
| Original bytes read |523,127,919|523,127,919|
| Cumulative requested input capacity |13,303,021,568 bytes|523,127,919 bytes|
| Open elapsed |5.027805s|3.668968s|
| Read elapsed |5.889139s|6.319687s|
| C1 inclusive (contains read) |6.569117s|6.969483s|
| C1 non-read/non-consumer residual |0.673817s|0.643587s|

20,399 fewer reads9.13%; exact-size capacity demand25.43x smaller. Both emit101,494
whole-file objects/525,462,281 canonical bytes and identical file roots. Requested
capacities are cumulative allocator demand, not real allocation totals, phase
RSS/peak or physical containment. Maximum live input remains<=131071 bytes.

Observed complete command11.809248s versus10.875895s is0.933353s lower, but source
open improved1.358837s while read worsened0.430548s and C1 inclusive worsened
0.400366s. The code does not change file-open algorithm, so that wall difference
cannot be sold as the fast path's causal saving. Source cache/filesystem/scheduling
variation remains unresolved. Retain the treatment for its exact output, lower
read-call count and capacity demand; latency benefit needs a qualified comparison.
No unchanged arm is repeated to obtain a nicer number.

## Verification and limits

Independent reference: original103,108-file byte/reconstruction proof reused by
identity; source fixture SHA256 matched captured manifest; exact roots checked
inside every source arm; root-arm metadata/root rows compared in both directions.
Tiny normal-import fixture confirms empty/small/large/dedup roots, metadata,
locators and physical pack bytes unchanged after both optimizations. Exact-size
reader boundary fixture0/1/8192/131071 bytes passes and short/trailing/131072-byte
capacity requests refuse explicitly. No weakening EOF or dropping input.

Locked release example builds, scoped warning-denying Clippy, full-workspace fmt,
product boundary272files and7 guard selftests passed. A provider-version receipt
addition initially omitted its local variable and failed compilation; retained
failed log, then covering corrected build passed. No performance attempt failed
or was repeated. Full Core/Linux/physical-resource suites not rerun for external
example/tooling-only changes, no such capability claim. No CI/preflight.
[Checks](evidence-construction-optimization-v1/checks/importer-checks.json),
[summary](evidence-construction-optimization-v1/summary.json),
[identities](evidence-construction-optimization-v1/identity.json),
[all evidence hashes](evidence-construction-optimization-v1/MANIFEST.json).

Private fixtures/catalogs retained under benchmark-results/storage-probes/
optimization-fixtures-v1 and construction-optimization-v1 in the owned worktree.
No provider was started; all children/connections closed. Original master/import
untouched. Full end-to-end import comparison is NOT_RUN: original full construction
exceeds25s; this mechanism matrix does not silently rerun it under180s diagnostic
limits, shrink its corpus or claim full-cycle speed.

Remaining large costs are real source opens/reads and C2 payload encoding. These
changes are modest. A materially faster whole initialization likely needs evidence
for a larger source/encoder intervention or a separately authorized admitted
initialization-concurrency profile. Commit/capture producer limits remain intact;
no codec-level/page-size/quota or worker change was used to improve these rows.

Per-commit production LOC forf97fee392 and91af4e2a6 unchanged: combined117,426 ->
117,426 (+0), reference65,417 ->65,417 (+0), Core52,009 ->52,009 (+0). Same
production_loc.py SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
Git archive first-parent/final staged snapshots and committed-tree confirmation,
shipped SQL included, tools/examples/tests/docs excluded. Report-only checkpoint
also changes no product source; exact comparison is recorded in its commit.
