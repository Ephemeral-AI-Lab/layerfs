# Payload ingest throughput: payload → C1 construction → C2 save

> Status: Research; **a measurement, not a qualification**. Two samples (one per
> pattern), one process each, taken 2026-09-21 on the merged `main` tree. Admission
> `INELIGIBLE`, every budget class `NOT_RUN`.

## Why this exists

The v0.1.7 ladder has no case that measures the whole path as a rate.
`c1.construct.*` stops at construction — it drives `construct_bytes` and never opens
a Store — and the `c2.*` families start from a Store. Nothing in the registry
reports a throughput figure at all. This measures payload → content processing →
storage → SQLite save as one rate, with the phases split.

The vehicle is [`measure_ingest.rs`](measure_ingest.rs.txt), an example in
`core/crates/layerfs-storage/examples/` (retained here as source as well as in the
tree). It is a measurement tool: examples are outside `src/`, so it is excluded from
production LOC and from `check_product_boundary.py`'s scope, and it changes no
product line.

## What it does

1. **Fixture** (untimed, declared): a deterministic splitmix64 payload of
   `--bytes`, built immediately before the timed scopes and **dropped before the
   save**, so no phase is credited a warm cache it did not pay for and the save does
   not carry two copies of the payload.
2. **Content processing**: the real `construct_bytes` with
   `ConstructionPolicy::frozen_default()`, emitting through a collecting
   `FinalizedConsumer`.
3. **Storage and SQLite**: `Store::create` → `begin_save` → `accept` × every
   emitted object → `finish` (the publication commit), each sub-phase timed
   separately.

## The two rows, 500 MiB each

| | `noise` (incompressible) | `repeat` (one 64 KiB block repeated) |
| --- | ---: | ---: |
| **payload** | 500.0 MiB | 500.0 MiB |
| objects emitted | 27,196 (26,981 chunks) | 32,254 (32,000 chunks) |
| canonical emitted | 501.6 MiB | 501.9 MiB |
| **content construct** | 0.597 s — **838.18 MiB/s** | 0.571 s — **876.15 MiB/s** |
| store create | 0.003 s | 0.003 s |
| storage begin | 0.002 s | 0.002 s |
| **accept (all objects)** | 2.310 s — **216.48 MiB/s** | 0.205 s — **2442.22 MiB/s** |
| storage finish (publication) | 0.387 s | 0.001 s |
| **save total** | 2.701 s — **185.11 MiB/s** | 0.210 s — **2376.61 MiB/s** |
| **END TO END** | **3.298 s — 151.62 MiB/s** | **0.781 s — 640.16 MiB/s** |
| Store file | 508.4 MiB (1.0× payload) | 0.3 MiB (0.0× payload) |
| `inserted` / `reused` | 27,196 / 0 | 14 / 32,240 |
| packs created / appends | 2,177 / 11,411 | 2 / 4 |
| commits / statements | 13,590 / 13,588 | 8 / 6 |

Store hashes: noise `601c5624c78989f7ec8b34c92e0e6b86df644818588fa11e21b534da2c63e234`,
repeat `fc196571a6dafe98e5f87c7b37bb2c8a55554702dbdb53052a59d8f7fe797b6e`.

**The noise row's Store is not retained at all.** It was 508.41 MB, and GitHub
rejects any file over 100 MB at push time (`GH001: Large files detected`), so it was
**deleted on owner direction on 2026-09-21** with the sha256 above as its only
record. The row is reproducible in about four seconds from the committed source, so
nothing is lost but the bytes. The repeat row's Store is 264 KiB and **is** committed.

**Owner policy from 2026-09-21: large files are not saved.** A row whose Store is too
large for the repository keeps its hash and its stdout, not its bytes. This was the
first #209 row to hit the limit — a consequence of measuring a 500 MiB ingest rather
than a 49 MB Store — and the earlier #209 rounds had already been drifting towards
it, carrying Stores of 52 MB (L57, L59, the confirmation window, the format round)
and 85.7 MB (the stride1 row). Those remain committed; the policy applies to new
rows.

**The two rows bracket the format's behaviour.** Incompressible bytes are stored in
full (508.4 MiB of Store for a 500 MiB payload) and pay the whole write path; the
repeated pattern is caught by content-addressed reuse — 32,240 of 32,254 objects are
`reused`, the Store collapses to 264 KiB, and the save is **13× faster** because it
writes almost nothing. The content phase barely moves between them (838 vs 876 MiB/s),
so **the rate is set by the write path, not by construction** — which is the same
conclusion the #209 rounds reached from the other direction.

## What this does not claim

- **Two samples, one per pattern, no repeats.** The counts are deterministic (the two
  exploratory runs taken into a scratch directory before these produced identical
  object, pack, append and commit counts), but the **times are single observations**
  and this machine's level moves (L62 §8). No bar is set and none should be read off.
- **Warm in-process fixture.** The payload is built by this process immediately before
  the timed scopes, so the construct phase reads its own recent writes. That is
  declared, not hidden, and it is why the construction rate is not a storage claim.
- **Not a registered case.** It is an example, not a registry row: no budget class
  applies, it is not in any sweep, and it does not appear in `--list`. Promoting it
  would mean a new `Shape`, a runner row and a declared over-budget exception.

## Reproduce

```sh
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml \
  -p layerfs-storage --example measure_ingest
bash run_ingest.sh
```
