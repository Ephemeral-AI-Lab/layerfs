The v0.1.7 ladder has no case that measures the whole ingest path as a rate. `c1.construct.*` stops at construction — it drives `construct_bytes` and never opens a Store — and the `c2.*` families start from a Store. **Nothing in the registry reports a throughput figure at all.** This adds one.

## What it is

`core/crates/layerfs-storage/examples/measure_ingest.rs` — builds a deterministic payload, puts it through the real `construct_bytes` with `ConstructionPolicy::frozen_default()`, and saves every emitted object through one `Store::create` → `begin_save` → `accept` × N → `finish` into a real SQLite Store, timing each phase separately.

The fixture is built immediately before the timed scopes and **dropped before the save**, so no phase is credited a warm cache it did not pay for and the save does not carry two copies of the payload.

## Measured — 500 MiB, one sample per pattern

| | `noise` (incompressible) | `repeat` (one 64 KiB block repeated) |
| --- | ---: | ---: |
| objects emitted | 27,196 (26,981 chunks) | 32,254 (32,000 chunks) |
| **content construct** | 0.597 s — **838.18 MiB/s** | 0.571 s — **876.15 MiB/s** |
| **accept (all objects)** | 2.310 s — **216.48 MiB/s** | 0.205 s — **2442.22 MiB/s** |
| storage finish (publication) | 0.387 s | 0.001 s |
| **save total** | 2.701 s — **185.11 MiB/s** | 0.210 s — **2376.61 MiB/s** |
| **END TO END** | **3.298 s — 151.62 MiB/s** | **0.781 s — 640.16 MiB/s** |
| Store file | 508.4 MiB (1.0× payload) | 0.3 MiB (0.0× payload) |
| `inserted` / `reused` | 27,196 / 0 | 14 / **32,240** |
| packs created / appends | 2,177 / 11,411 | 2 / 4 |
| commits / statements | 13,590 / 13,588 | 8 / 6 |

**The two rows bracket the format's behaviour.** Incompressible bytes are stored in full and pay the whole write path; the repeated pattern is caught by content-addressed reuse — 32,240 of 32,254 objects are `reused`, the Store collapses to 264 KiB, and the save is **13× faster because it writes almost nothing**. The content phase barely moves between the two (838 vs 876 MiB/s), so **the rate is set by the write path, not by construction** — the same conclusion the #209 rounds reached from the other direction.

## What it does not claim

- **Two samples, one per pattern, no repeats.** The counts are deterministic — the two exploratory runs taken into a scratch directory first produced identical object, pack, append and commit counts — but the **times are single observations**, and this machine's level moves by up to 55 % between windows (L62 §8). No bar is set and none should be read off.
- **Warm in-process fixture**, declared: the construct phase reads its own recent writes, so its rate is not a storage claim.
- **Not a registered case.** It is an example, not a registry row: no budget class applies, it is not in any sweep, and it does not appear in `--list`. Promoting it would mean a new `Shape`, a runner row and a declared over-budget exception — happy to do that if you want it as a permanent row.

## Checks

`fmt --all --check` clean, `clippy -D warnings` clean, `check_product_boundary.py` **PASS** over 175 files. The example is outside `src/`, so it is excluded from production LOC and from the boundary scan, and it changes no product line.

Production LOC: 25403 → 25403 (delta 0).

Evidence: `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-ingest-20260920T232000Z/` — raw stdout per row, the source retained as `measure_ingest.rs.txt`, the repeat row's Store (264 KiB), and the noise row's Store **by sha256 only**: that Store is 508.41 MB and GitHub rejects any file over 100 MB at push time, so it is retained on the measuring disk and not in the repository. That is the first #209 row too large to commit, and it is worth a policy decision before more rows at this scale are taken.
