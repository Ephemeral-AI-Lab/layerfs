# #219 round 12 — the reads are two sites at 49.6 % each, and the prefetch reads 10 pages

Pre-registration: `pre-registration.md`. Row: `benchmark-results/issue219/ns19-M1-sites-20260921T083052Z`,
**PASS, 13/13 gates, 14/14 pins**, `source_commit` `fff509f3d`, clean. Product instrumentation only.

## 1. The partition

| site | inode pages | share | what it is |
| --- | ---: | ---: | --- |
| **`bindings`** | **13,718** | **49.6 %** | the per-binding loop's own record lookups |
| **`cycles`** | **13,718** | **49.6 %** | the effective-tree cycle walk |
| `allocation` | 216 | 0.8 % | the allocator precondition, 64 serials per wave |
| `prefetch` | **10** | **0.0 %** | the batch's grouped prefetch |
| `aliases` | 0 | 0 % | the parent-alias pass |
| `reachability` | 0 | 0 % | the base-less build walk |
| **sum** | **27,662** | **100.0 %** | equals `inode_pages_read` exactly |

`build_validate_ns` 228.42 ms (K1 222.15, J1 221.70 — the phase is stable across three rows), wave
ratio 27,657/27,662 = 1.0002 unchanged.

**The instrument is sound**: the six sites sum to the counted total exactly, because each is charged
as the growth of the total the operation already maintains.

## 2. The prediction was missed, and the miss is the finding

The registration predicted `bindings` ≥ 60 % with `cycles` ≤ 25 % and a dominance test at 60 % as its
first refutation clause. Measured: **49.6 % / 49.6 %**, and **no site holds 60 %** — so clause 1
fired and is reported as fired. What the row shows instead is sharper than the prediction:

- **`prefetch` reads 10 pages.** Ten. The grouped demand that every design comment presents as the
  mechanism that makes validation cheap is answering almost nothing on this workload.
- **`bindings` and `cycles` read the same 13,718 pages** — the same number to the page. Two sites
  asking the same questions and each paying for the answer.

## 3. The cause, and it is a documented design decision

`ValidationState::lookup_optional` memoises the records it *finds* and **deliberately does not
memoise absence** (`validate.rs`, the state's own documentation): *"Absence is deliberately not
memoized: a serial with no stored record keeps today's lookup and today's charge, so an absent
serial's accounting is bit-identical."* The stated reason is **accounting**, not correctness — a
serial that is not in the base keeps its old charge so a counter comparison stays bit-identical.

This row's batches bind **files the operation is allocating**, so their serials are absent from the
base by construction. Every one of them therefore costs a full descent in the binding loop, and the
cycle walk then asks the identical serials and pays for them again:

- 2 update batches x ~3,400 bindings x 2 pages per descent ≈ **13,700 pages per site**, which is the
  13,718 measured on each;
- 27,436 of the 27,662 pages = **99.2 %** of `validate`'s reads are absent-serial descents, and
  **half of them are the same descents bought twice**;
- the grouped prefetch cannot help, because it only memoises what it finds.

So `validate`'s 222–228 ms is not an algorithmic walk and not an expensive read path: it is
**~27,400 authenticated descents of an inode table to learn, twice each, that a serial the caller
announced as new is not in the base.**

## 4. The treatment this produces (registered next, not here)

**Memoise absence** — a bounded set of serials the operation has already shown to be absent — and let
the prefetch record absence too, since `lookup_many` already returns `Option` per serial. Predicted,
on the instruments this round established:

| instrument | now | predicted |
| --- | ---: | ---: |
| `validation_pages_bindings` | 13,718 | **~0** (the prefetch already read every child) |
| `validation_pages_cycles` | 13,718 | **~0** |
| `validation_inode_pages_read` | 27,662 | **≤ 500** |
| `build_validate_ns` | 228.42 ms | **≤ 40 ms** |
| `operation_work_ns` | 1681.30 ms | **−180 to −215 ms** |

The counter-compatibility reason for the current behaviour is exactly what the treatment spends: the
row's `validation_objects_read` / `inode_demands` charges would fall, and any pinned counter that
moves must be re-pinned **before** the run and declared, as round 8 declared its own.

Checks as run: `cargo test -p layerfs-content` **262 passed / 0 failed**; `-p layerfs-storage`
**215 / 0**; the whole core workspace `--no-fail-fast` **621 passed / 0 failed**;
`clippy --all-targets` clean; `fmt --check` clean; `core/tools/check_product_boundary.py` PASS;
harness suite 120 passed / 3 failed (the three pre-existing `registry_negative` cases).

Production LOC: **31377 -> 31409 (delta +32)**. Method `python3 tools/production_loc.py --root <tree>`,
first parent `a4f72cd1b` against the committed tree; the harness change is not product source.
