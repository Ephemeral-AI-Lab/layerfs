# #785 Chunk delta bases by content similarity: experiment record

> **Status:** Research; informative and not a product contract.

Issue: [Ephemeral-AI-Lab/layerfs#785](https://github.com/Ephemeral-AI-Lab/layerfs/issues/785)
(labels `v3.0.0`, `optimization`). Recommendation recorded there: keep the 2.0.0
chunk path unchanged and revisit after 2.0.

## Question

On the real 2.0 Store qv2 (2 projects, 239 Branches, 4,514 Commits, schema 42,
4,754,953,132 bytes of packs), 374,317 chunks (2,071,395,695 bytes of records,
44% of the packs) are independent zstd frames. A chunk is offered a delta base
only from its file's predecessor at the same byte offset; chunks do not consult
the content-signature index that small files use. Would a similarity base shrink
those chunks, and what would it cost?

## Source and safety

- Input: an APFS clone of the pristine qv2 copy taken 2026-11-18 12:55
  (4,927,545,344 bytes, sha256 `57319d23498e849268622b86aa22450bd653ef401b571c3a9a233ace1b7a9473`).
  Both the clone and the pristine copy matched that checksum after the run.
- Every connection used SQLite `immutable=1`; no sidecar files were created.
- No product code changed. The live Store was not opened.

## Method

1. `experiment/sketch.py`: the product signature (`encoding/delta/candidates.rs`:
   8 smallest mixed rolling hashes over 16-byte windows) for all 1,176,510 chunks
   in admission order (pack, group, record). `experiment/validate.py` checks the
   implementation: 200/200 equal to the product's persisted whole-file signatures.
2. Lookup as the product index does it: per hash the newest earlier holder; at
   least 2 shared hashes; largest overlap, then newest.
3. Bases only from earlier Commits. Packs are mapped to Commits by cumulative
   stored bytes in creation order, which is approximate at boundaries (Commit
   accounting does not land exactly on pack boundaries).
4. Selector rules kept: depth cap 8 (base depth strictly below), one trial, zstd
   level 3 with the base payload as raw prefix, window log 20, checksum, kept only
   when the record is strictly smaller. A control re-encode of independent records
   matched the stored widths exactly.
5. Samples: 3,000 random independent chunks (`exp_chunk.py`, `exp_file.py`,
   `exp_policy.py`, seed 20261124) and 3,000 random delta chunks (`exp_policy.py`,
   seed 99). `intervals.py` gives 95% bootstrap intervals (2,000 resamples) and
   extrapolates to the population.
6. Signature timing: `sig.rs`, the product function copied unchanged, built with
   `rustc +1.85.1 --edition 2021 -C opt-level=3`, timed on 2,000 sampled chunks.

## Results

`results/final_intervals.json` holds every figure below.

| Rule | Index | Saving | Share of packs |
|---|---|---:|---:|
| (b) index base when the predecessor is absent or its trial lost | last 65,536 independent chunks | 0.724 GB [0.682, 0.763] | 15.2% |
| (b) | last 65,536 chunks of any kind | 0.804 GB [0.763, 0.847] | 16.9% |
| (b) | all earlier chunks | 0.846 GB [0.805, 0.893] | 17.8% |
| (b) | last 8,192 chunks | 0.556 GB [0.517, 0.594] | 11.7% |
| (a) one trial on the higher-overlap candidate, on existing delta chunks | last 65,536 independent | 0.566 GB [0.509, 0.627] | 11.9% |
| same-Commit bases allowed | all | 0.965 GB [0.920, 1.011] | 20.3% |
| per-file predecessor by similarity, then same-offset chunk | files | 0.340 GB [0.310, 0.371] | 7.1% |

Why sampled chunks are independent (`results/exp_file.json`, by guessed
predecessor file): no predecessor 436 (14.5%); a same-offset predecessor chunk
existed but was a poor base 2,530 (84.3%); predecessor at the depth cap 34 (1.1%).

Speed, median per 20 KB chunk, warm cache (`results/exp_chunk_earlier.json`,
`sig.rs`): signature 54 µs; independent frame 64 µs; base fetch and chain decode
114 µs (97 µs at depth 0 to 644 µs at depth 7); prefix trial 169 µs. Rule (b)
adds about 0.28 ms per independent chunk. 71% of Commits write no chunk; the
largest build Commits (9,299 to 19,042 independent chunks) gain 2.6 to 5.3 s of
chunk-encoding work. Whole-Commit wall time was not measured.

The Store anatomy behind the issue's context table comes from
`anatomy_scan.py` (record tags and encodings for all 38,680 packs),
`anatomy_cdc.py` (all 93,429 extent pages and 62,794 file states),
`anatomy_chains.py` (delta depths) and `anatomy_deltagain.py` (delta records
against independent zstd on 4,000-record samples).

## Not covered

Cascading effects of chosen bases on later chains, whole-Commit wall time, cold
caches, GC mark time and workloads other than build-heavy Rust trees.

## Reproduce

Run from a directory outside the repository, against a read-only Store copy and
the delta-edge file written by `anatomy_scan.py`:

```bash
python3 -I anatomy_scan.py STORE scan.json edges.bin
python3 -I validate.py EXPERIMENT_DIR STORE edges.bin
python3 -I sketch.py EXPERIMENT_DIR STORE edges.bin OUT 32 8
python3 -I exp_chunk.py EXPERIMENT_DIR STORE OUT 3000 earlier
python3 -I exp_chunk.py EXPERIMENT_DIR STORE OUT 3000 any
python3 -I exp_file.py EXPERIMENT_DIR STORE OUT
python3 -I exp_policy.py EXPERIMENT_DIR STORE OUT none
python3 -I exp_policy.py EXPERIMENT_DIR STORE OUT 65536
FULLONLY=1 python3 -I exp_policy.py EXPERIMENT_DIR STORE OUT 65536
python3 -I intervals.py OUT
```

`exp_chunk.py` writes the sample that the later scripts read, and `exp_file.py`
writes the cause classes and declared bases that `exp_policy.py` reads.

`sketch.py` is resumable and keeps each command under two minutes; rerun it
until it reports no parts left. `OUT` needs `edges.bin` copied into it.
