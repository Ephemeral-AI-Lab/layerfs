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
