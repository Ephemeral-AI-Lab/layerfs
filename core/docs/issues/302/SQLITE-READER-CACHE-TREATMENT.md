# Operation-owned sealed-pack reuse treatment

Status: Count-based mechanism proven; competitive speed remains unproven.

Based on fd65493c1. Investigation of the retained100000 Init row found4597
individual payload acquisitions totaling1,079,610,281B, plus270,179,431B
batched acquisition. Those counters do not identify every call site. Source
inspection establishes a narrower defect: ReadState recreates its pack map for
every demand although the Reader is operation-owned and its other caches persist.

The public-API test reader_reuses_authenticated_pack_within_one_operation first
failed: the second demand increased read_packs from1 to2 for the same immutable
object. Retained failure log target/phase7-agent/reader-cache-before.log. Moving
the existing map into ReadState then passes: repeated and duplicate demands
return identical canonical bytes without another acquisition, and a fresh Reader
must acquire again. Existing transaction ownership tests also pass (4 total).
No speed claim follows from the deterministic count test.

The4MiB aggregate pack-cache eviction and oversize-singleton exception are
unchanged, as are SHA/framing/domain validation, reconstruction budgets, output
order, write/publication units, WAL/FULL/fullfsync and worker counts. This cache
is paid for inside the operation, never fixture preparation or an earlier arm.

A prospective single matched1000-file v2 pair will assess this mechanism before
another all-tier qualification. Other earlier FAIL receipts remain unchanged.
There is no claim that operation-owned read reuse fixes Save-wave prefetch
thrashing, duplicate backend authentication, cold-preconditioning overhead,
allocation overages or the missing history driver/proof.


## Matched1000-file outcome at1735fb8b0

One release/locked sample per arm. Complete-product Phase4.5=143347625ns;
candidate=224717708ns, ratio1.56764, **time FAIL**. Exact arithmetic:
2247177080 >1576823875. Roots match. Cold-content attestation, independent
sampled proof, cleanup and allocation PASS. Final allocation23101440/20561920B.
Performance envelopes254645208/1105380958ns; verification50093167/539248542ns,
both under their15s/9.5s bounds. The verifier is excluded from product time.

Candidate statements1284, executed VM209982, transactions39, write commits20,
commit_ns119135582. C2read_packs4/3402630B, payload_reads0, reserve8,publish9.
These are cumulative scoped diagnostics; nested spans overlap. This small case
cannot attribute the100000-file payload reads. It supplies no evidence of a
material Init speed improvement over the earlier treatment227274084ns: the
windows differ and there is one sample per changed identity.

Raw receipts: benchmark-results/fs-bench-pro/issue302-reader1000-{baseline,
candidate}-treatment1; comparison JSON inissue302-reader1000-comparison-treatment1.
Reproduce once at the frozen source with runner.py run --case
phase7-sqlite-init-1000-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-path>, then --arm candidate
with its own fresh output. Read benchmark_agent_report.md before each invocation.
Do not replay either measured arm at this identity.

Code commit1735fb8b0: Production LOC137501->137502(+1), reference65417,
core72084->72085; active28040->28041, inactive44044 unchanged; exact first-parent
snapshot counter recorded in commit. Full-workspace tests not repeated; owning
four transaction/cache tests and scoped all-target Clippy pass, fmt/boundary
and23 boundary self-tests pass. No CI/preflight/push/PR/merge.

Remaining: all four prior Init time failures and large-tier allocation/envelope
failures remain. Histories17/53/157 remain NOT_RUN. All-seven goal ACTIVE.
Next source-backed investigation is Save-wave chain prefetch: it fetches the
entire frontier before traversing it, while aggregate pack bytes may exceed its
unchanged cache bound. A count diagnostic must establish reacquisition before
any change. Durable exchange composition remains the larger small-case path.
