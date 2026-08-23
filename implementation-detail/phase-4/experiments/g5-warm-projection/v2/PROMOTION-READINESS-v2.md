# G5-2 v2 anti-cheat and promotion readiness

Status: `PREMEASUREMENT_REVISE`. This benchmark-private candidate is not
authorized as product architecture and must not be frozen, screened, or gated.

## Current blockers

- The benchmark-private `wp4m_*` database/schema is not an authoritative engine
  or product persistence surface.
- G5-1 must rerun from its clean accepted source and finish PASS after extraction;
  earlier G5-1 evidence does not authorize the changed engine boundary.
- Every fault in `method/FAULT-MATRIX-v2.tsv` needs fresh observed product
  counters. Literal zero expectations or analyzer-invented observations are not
  evidence.

## Required extraction

1. Select and name the authoritative engine and schema.
2. Extract Store open/authority, receipt decode, ObjectId authentication,
   transaction/one-COMMIT dispatch, durability, and reconciliation boundaries.
3. Keep `IntegrityMode::Verified` the default and Store-lifetime. Trusted local
   edit authority remains distinct and cannot create verified carry-forward.
4. Move filesystem syscalls, descriptor identity, clone, sync, rename, and
   directory-sync primitives into `layerfs-os`.
5. Move bounded projection policy, exact-vs-latest state, seed rotation, and the
   one-in-flight/one-pending service into `layerfs-vfs`.
6. Re-run focused source tests and capture exact-source hashes from those product
   modules. Benchmark-private copies, source-shape grep, or synthetic receipts do
   not authorize promotion.

## Promotion gate

Promotion requires exact product-source evidence, the complete observed fault
matrix, source/binary/input custody, both independent analyzers, a clean G5-1
PASS on the extracted boundary, then a new zero-row feasibility proof. Until
all are present, v2 remains PREMEASUREMENT_REVISE.
