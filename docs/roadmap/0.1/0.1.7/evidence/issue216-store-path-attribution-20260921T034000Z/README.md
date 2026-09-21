# Why the store path runs at ~150 MiB/s: not construction, not the wire

> **Status: DIAGNOSTIC attribution.** Product numbers are the product's own phase
> receipts; the SQLite rows are an **external mimic** in plain Python `sqlite3` with
> the declared profile, no product code. Not a gate, not a release claim.

## 1. The product splits 19% construction, 81% storage

`measure_ingest`, 512 MiB incompressible, one sample, product phases
(`product/measure-ingest-512mib-noise.txt`):

| Phase | Time | Rate | Share of end-to-end |
| --- | ---: | ---: | ---: |
| content construct (C1) | 0.829 s | **617.66 MiB/s** | 19% |
| accept (C2 save) | 3.095 s | **165.43 MiB/s** | 71% |
| finish (publication commit) | 0.419 s | — | 10% |
| **end to end** | **4.349 s** | **117.73 MiB/s** | 100% |

Counters for the same run: `inserted=27845 packs_created=2229 pack_appends=11682
commits=13913`, Store 520.7 MiB for a 512 MiB payload. So construction is the
*fast* half by 4x, and the question "because of construction?" answers itself.

## 2. The same construction, with content that deduplicates

`measure_ingest`, 64 MiB repetitive (`product/measure-ingest-64mib-repeat.txt`):
content construct 0.100 s (641.99 MiB/s - unchanged), accept **0.029 s
(2213.40 MiB/s)**, save total 0.036 s (1778.75 MiB/s), because only **0.3 MiB** was
stored (`inserted=12 reused=4118`).

Same C1 work, same code path, 60x less stored bytes: the save phase is ~60x faster.
**The storage half is proportional to stored bytes, not to CPU.**

## 3. What plain SQLite costs for those byte patterns

External mimic, same declared profile (MEMORY journal, `synchronous=OFF`,
`temp_store=MEMORY`, `busy_timeout=0`), one sample per arm, 512 MiB stored per arm:

| Pattern | Blob | Page | Appends per blob | Commit | Rate |
| --- | ---: | ---: | ---: | --- | ---: |
| sequential insert | 250 KiB | 4096 | 0 | each | **209.3 MiB/s** |
| sequential insert | 1 MiB | 4096 | 0 | each | 242.3 MiB/s |
| sequential insert | 250 KiB | 16384 | 0 | each | **272.9 MiB/s** |
| append to a growing blob | 250 KiB | 16384 | 1 | each | 150.3 MiB/s |
| append to a growing blob | 250 KiB | 65536 | 1 | each | 156.4 MiB/s |
| append, aggressive (grow to 2x, ~5 appends) | 250 KiB | 4096 | up to 6 | each | **46.2 MiB/s** |
| append, aggressive | 250 KiB | 4096 | up to 6 | per 16 | 50.9 MiB/s |

Two things follow.

1. **The engine's ceiling for this shape is a few hundred MiB/s.** Plain sequential
   BLOB inserts - no rewrites, no product code - top out at 209-273 MiB/s on this
   profile. Multi-GB/s is not reachable by writing ~1x the payload through SQLite
   pages, whatever the layer above does.
2. **Rewriting a growing BLOB costs 2-5x**, and a larger page size buys ~30%
   (209 -> 273 MiB/s) while a coarser commit cadence buys ~10% (46.2 -> 50.9 MiB/s).
   The product's 165 MiB/s accept rate sits inside this band and above the
   aggressive mimic, so the cost is the pattern, not LayerFS-specific overhead.
   (Its own counters put ~5.2 appends per pack: 11682 appends / 2229 packs.)

## 4. Where the route's ~100 MiB/s comes from

The authenticated transport alone is 802 MiB/s on this build (1 stream) and the
in-process construct+save path 117-143 MiB/s, so a route that runs at ~100 MiB/s is
store-bound: the wire is 5-8x faster than the thing it feeds. More writers do not
help (the budget ladder plateaus at 2-4 and falls at 8), a faster AEAD does not
help (already 802 MiB/s), and faster construction does not help (already 620 MiB/s).

## 5. What would actually move it, and what that costs

- **Never rewrite a pack** (write-once packs, or place each sealed group into its own
  pack): removes the append amplification, the largest single term. In the mimic that
  is 46 -> 209 MiB/s. It changes the physical layout and the Store hash, so it is an
  owner decision - the #209 format round produced exactly such a change (-1.55 s
  commit, -1.30 s operation on stride10) and it was **reverted by owner decision**.
- **Larger pages** (16 KiB): ~30% in the mimic (209 -> 273 MiB/s), also a Store
  format change.
- **Fewer stored bytes**: the product already dedups and delta-encodes; §2 shows the
  lever is real (60x less storage -> 60x faster save).
- **Not levers**: more writers, faster AEAD, faster C1, or higher commit cadence.

## 6. Gaps

- The product split is one sample (512 MiB, incompressible) plus one dedup sample;
  the mimic is one sample per arm, external, and not admission-eligible.
- The mimic reproduces byte volume and append shape, not the product's exact pack
  geometry; it bounds the engine, it does not replace an instrumented write
  amplification count, which the product does not currently report.
- Both hosts states are the same loaded machine (load ~6-7 of 14 cores).
