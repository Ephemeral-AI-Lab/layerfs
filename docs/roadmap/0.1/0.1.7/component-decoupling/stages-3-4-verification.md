# Stages 3–4 verification contract (frozen before collection)

> **Status:** frozen measurement/verification contract for
> [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) (Stage 3) and
> [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) (Stage 4).
> Target LayerFS v0.1.7; not a released contract.

This document freezes what a Stages 3–4 claim may rest on. It was written before
any comparison collection and reuses the existing harness mechanisms
(`benchmark/fs-bench-pro/`, the shared Cargo target, the receipt/ledger rules in
[docs/general/benchmark_rules.md](../../../../general/benchmark_rules.md)). No new
framework, runner or monitoring subsystem is introduced by this batch.

## 1. Frozen identities

| Item | Value |
| --- | --- |
| Candidate source | the commit that adds this file (recorded in the ledger entry) |
| Reference source | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` |
| Reference role | source/oracle input only; never a candidate dependency, include, linked fallback or alternate runtime path |
| Toolchain | `cargo +1.85.1`, `cargo +1.96.0` for `fmt` |
| Candidate packages | `layerfs-content` (C1), `layerfs-storage` (C2) |
| Construction worker | single (default wiring); every run exports `LAYERFS_CONSTRUCTION_WORKERS=1`; the namespace-init exception does not apply to any case here |
| Cache/state | declared per case; fixtures are prepared outside the timed scope and never re-prepared inside it |
| Acknowledgement | the case is complete only when the save's final transaction was acknowledged and the readback was authenticated |

## 2. Frozen cases

Sizes are exact byte counts; seeds are the deterministic generators the candidate
tests use (`noise` = fixed xorshift stream, `patterned` = index-derived bytes).

| Family | Case | Inputs | Success criterion |
| --- | --- | --- | --- |
| Payload FULL/DELTA | `whole-file-delta-win` | 100 000-byte base, 1 000 changed bytes, explicit predecessor | one PREFIX record; readback equals the supplied canonical bytes |
| Payload FULL/DELTA | `whole-file-delta-lose` | unrelated base and target of equal size | one trial, FULL selected, readback exact |
| Payload FULL/DELTA | `chunk-delta-win` | 200 000-byte file chunk pair | PREFIX selected; readback exact |
| Edit single | `small-overwrite` | base `T/2`, one 512-byte overwrite | root equals a fresh construction of the final bytes |
| Edit single | `chunked-overwrite` | base `2T`, one 4 096-byte overwrite | readback equals the independent model |
| Edit transitions | `grow` / `shrink` / `empty` | `T-1` + `T` inserted / `2T` - `3T/4` deleted / full delete | exact root for the small and empty cases, exact bytes for the chunked case |
| Edit batch | `multi-edit` | three ordered edits in current-result coordinates | model equality, root equality |
| No-op | `equal-replacement` / `empty-stream` | byte-identical replacement / no edit | the base root is returned unchanged and no object is emitted |
| Policy | `cutoff-128k/256k/1m` | boundary lengths `T-1`, `T`, `T+1` | representation boundary follows the configured cutoff |
| Policy | `singleton-1m` | 1 048 575-byte incompressible whole-file record | stored in its own pack, read back exactly, one database file on disk |
| Index/footprint | `store-bytes` | each pipeline case | total retained pack bytes and database bytes reported; a DB size delta is never reported as write I/O |

## 3. Measurement rules for any later comparison

1. One sample per case per arm unless an owner-approved campaign says otherwise;
   no best-of selection; diagnostics are labelled and reported beside the gate.
2. A fresh `--output` per run; receipts are append-only and never overwritten.
3. Both arms must match on public operation semantics, inputs, algorithm profile,
   cache state, work/acknowledgement boundary and harness treatment. Where the
   v0.1.6 public API cannot expose an equivalent C1 or C2 operation, the result is
   a non-comparative diagnostic and the speedup is **unproven**.
4. Complete command wall time ≤ 15 s, declared exceptions ≤ 25 s, verification
   ≤ 60 s. A case that cannot fit is reused from a qualifying receipt or recorded
   `NOT_RUN` with its measured wall time.
5. Cache state is declared and enforced equally; `INELIGIBLE` rows are reported,
   never dropped; cold and warm rows are never pooled.
6. Resource reporting covers retained pack/database/value-group footprints,
   written versus rewritten bytes, and the simultaneous live capacities of every
   lane. `SQLite cache_size` is not a memory cap and RSS is not a universal
   safety claim.

## 4. Current state of collection

**No comparison campaign was collected for this batch.** The only artifacts are
the single-sample smoke runs recorded in
[`evidence/stages-3-4-smoke-20260916T210931Z/`](../evidence/) (see the directory
README for the exact commands and observed numbers). They demonstrate wiring and
correctness of the three modes; they are explicitly **not** performance evidence
and no v0.1.6 comparison, speedup or storage-reduction claim is made anywhere in
the Stages 3–4 report.

The families this contract lists remain available for the later qualification
issue; the candidate entry points they need exist in
`core/crates/layerfs-storage/examples/measure_edits.rs` and the external test
targets named in the report.
