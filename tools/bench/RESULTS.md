# Compact build prototype: measurements

Date: 2026-11-16. Environment: LayerFS sandbox on the R730 host, 88 cores,
125 GiB RAM, workspace machine filesystem `/dev/sda2` (ext4), SQLite 3.40.1.
Each run is a fresh process; the source is `sync`ed and evicted with
`posix_fadvise(POSIX_FADV_DONTNEED)`, the page cache is 32 MiB and
`synchronous=OFF`. The 50 GB case was not run.

Synthetic store: 1 MiB `pack.body` per pack, `object_location` one row per
object, `kept_node`/`store_totals` proportional; 25% of packs dead. The
delete step mirrors the build's two statements. Verification is metadata
only (counts, sums, distinct digests; `payload_readback: false`).

## Results

| case | strategy | total s | clone s | delete s | compact s | steps | final GB | written GB | source GB/s |
|---|---|---|---|---|---|---|---|---|---|
| 0.27 GB | current | 3.45 | 0.89 | 1.07 | 1.49 | - | 0.202 | 1.020 | 0.073 |
| 0.27 GB | prototype | 2.35 | 0.23 | 0.96 | 1.17 | 16,395 | 0.202 | 0.550 | 0.107 |
| 0.27 GB | prototype, no vacuum | 1.03 | 0.26 | 0.77 | 0.00 | 0 | 0.269 | 0.413 | 0.244 |
| 10.77 GB | current | 249.1 | 102.5 | 42.2 | 104.4 | - | 8.075 | 40.79 | 0.040 |
| 10.77 GB | prototype | 866.1 | 20.6 | 44.9 | 800.6 | 655,750 | 8.077 | 24.16 | 0.012 |
| 10.77 GB | prototype, no vacuum | 64.0 | 20.1 | 43.9 | 0.00 | 0 | 10.766 | 16.52 | 0.157 |

`current` = shipped build (`VACUUM INTO`, delete, full `VACUUM`).
`prototype` = `copyfile`, delete, `PRAGMA incremental_vacuum` stepped in
transactions of 8,192 pages until the freelist is empty.
`write GB` is `/proc/self/io` `write_bytes`.

## Findings

- `copyfile` is ~5x faster than `VACUUM INTO` at 10 GB (20.6 s vs 102.5 s):
  the copy does not rewrite the b-tree.
- The delete step is size-bound, not strategy-bound: ~43 s at 10.77 GB.
- `PRAGMA incremental_vacuum` on SQLite 3.40 relocates one page per statement
  execution; `N` does not fan it out. At 10 GB that is 655,750 page
  relocations. In one transaction batch they cost ~1.2 ms each on this medium
  (random page writes), so the reclaim step alone is 800 s and the prototype
  is 3.5x slower than the full `VACUUM` overall, although it writes 40% fewer
  bytes (24.2 GB vs 40.8 GB).
- At 256 MiB the prototype wins (2.35 s vs 3.45 s) and writes about half the
  bytes; the crossover is between 0.27 GB and 10 GB on this medium. The win
  comes from avoiding the rewrite, the loss from per-page relocation I/O.
- Copy + delete alone (`prototype, no vacuum`) is the lower bound (64 s at
  10 GB) but leaves the file unshrunk (10.77 GB), so it cannot be the rebuild.
- The existing bounded maintenance job (`RECLAMATION_PAGE_LIMIT = 512`,
  `lifecycle/reclamation.rs`) calls `PRAGMA incremental_vacuum(pages)` once
  and therefore removes one page per job on this SQLite, not up to 512; a
  budget-honouring loop would be a separate fix.

Recommendation: keep the full `VACUUM` rebuild for 10 GB-class Stores on
rotational media, or adopt copy+delete+incremental only below a size
threshold. Reclaim cost, not discarded bytes, is the binding constraint.
