# Stage 0 resource coverage amendment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source review found that the current formatter delivers none of the 42 existing
Storage diagnostic fields per fixed Store reader and none of the 31 numeric
global Overlay Resources fields. Commit's Storage receipt describes its separate
Save handle and cannot substitute for READ storage work. Resources comprises 17
StoredCounts, five AllocationState fields, six AllocationWork fields and three
page/debt fields. These are coverage gaps before the baseline.

Taken under the owner's direction of 2026-10-09: expose existing reader
Diagnostics one reader at a time. Under a nonblocking pool observation lock,
copy counters from an idle, assigned or retained original reader, then release
the lock before formatting. An actively leased reader or unavailable lock is
explicitly UNAVAILABLE. Acquire no lease, perform no object demand, retain no
new snapshot/cache/registry and do not wait for a reader to finish.

Reuse the existing Resources command with global=true and no Route. Admit only
that exact read-only combination before the routed-command check; all existing
routed validation remains. This runs the existing db.resources(None) operation
once. No synthesized mutable Route, new SQL/table/index/column or authoritative
state is introduced. It has one observer job, three existing Startup statements
(accounting point, page count and free page count), two file metadata calls and
no transaction or base demand. Its actual work is charged as observation work,
separately from product-phase counts; instrumentation is never a count reduction.
Collect global Resources before the owner snapshot so the charged job is visible.
Preserve failure as unavailable data and original operation custody; no retry.

Add numeric section 28 for global Resources and section 29 for reader Storage.
Sections 26 and 27 already describe native opcodes and abort work; preserve them.
The schema has at most 55+2R rows, under a declared 64+2R framing cap, with the
same 4096-byte row cap. R is the already configured fixed reader count. This
changes diagnostic coverage, not a product cache, worker, queue, buffer or byte
credit allowance. Output still streams one row directly and retains no aggregate
vector. Ordinary unobserved control operations do none of this work.

The telemetry agent owns Store read_service/open, daemon diagnostics/schema,
the exact owner command/admission handling, external owning tests and affected
architecture documentation. The lead retains cleanup-query ownership and all
execution/Git/evidence duties. Tests must establish leased UNAVAILABLE, unchanged
scheduler bytes and capacities, exact observer job/statement cost, route-free
global read-only Resources, absence after terminal cleanup, fixed numeric bounds,
ordinary zero diagnostic work and cardinality independent of unrelated state.
Build first, then bounded host/Linux proofs; review before acceptance.
