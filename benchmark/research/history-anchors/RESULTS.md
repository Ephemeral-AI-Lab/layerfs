# Deterministic multiscale history-anchor results

Baseline: LayerFS `main` at `1e81e9b8cf871324341c221a51b0a0239c580da9`.

Verdict: **YES — focused PR justified**.

## 1. Plain-language conclusion

The idea is useful for one narrow job. LayerFS can already open a known Commit
directly, so anchors do not improve ordinary historical reads. However, current
Branch membership checks walk the parent chain. A distant Branch-Commit Diff
performs those checks before comparing content roots. Sparse deterministic
anchors greatly reduce that planning walk at depth 100 and 1000.

## 2. Existing LayerFS mechanisms

- `commit(id)` is already a direct primary-key lookup.
- a snapshot reader can already read a known Commit root without walking
  parents;
- content Diff already compares Merkle roots directly after validation;
- paginated Commit history exists;
- `branch_contains_commit` still proves membership by a recursive parent walk;
- no equivalent generation number, checkpoint, or multiscale ancestry index was
  found on this baseline.

This prototype therefore does not replace existing historical lookup or Merkle
Diff mechanisms. It isolates only ancestry validation and Diff planning.

## 3. Baseline

Two independent formal runs produced 56 rows each. Direct early Commit lookup
visited zero parent-history nodes at every depth and stayed near 3 microseconds.
The ancestry-dependent operations scaled with depth:

| Depth | Early ancestor nodes | Early ancestor latency | Distant Diff nodes | Distant Diff latency |
| ---: | ---: | ---: | ---: | ---: |
| 10 | 10 | 18.8 us | 11 | 144.8–145.4 us |
| 100 | 100 | 53.7–53.9 us | 101 | 182.3–185.0 us |
| 1000 | 1000 | 490.8–510.9 us | 1001 | 651.9–684.9 us |

Depth 1000 full-history traversal returned the complete canonical sequence in
eight queries. The benchmark records returned rows and queries separately.

## 4. Sparse-anchor comparison

| Depth | Strategy | Distant Diff nodes | Distant Diff latency | Logical metadata |
| ---: | --- | ---: | ---: | ---: |
| 10 | baseline | 11 | 144.8–145.4 us | 0 B |
| 10 | fixed-10 | 11 | 108.4–108.5 us | 410 B |
| 10 | multiscale | 4 | 107.6–107.7 us | 574 B |
| 100 | baseline | 101 | 182.3–185.0 us | 0 B |
| 100 | fixed-10 | 20 | 109.0–109.2 us | 4,469 B |
| 100 | multiscale | 6 | 108.8–110.1 us | 6,109 B |
| 1000 | baseline | 1001 | 651.9–684.9 us | 0 B |
| 1000 | fixed-10 | 110 | 110.1 us | 45,059 B |
| 1000 | multiscale | 10 | 109.7–109.9 us | 61,459 B |

At depth 1000, multiscale metadata is 0.957% of the 6,422,528-byte Store.
Pure index construction took about 85 us at depth 1000. Reconnect plus copying
the immutable Store snapshot, reading canonical history, and rebuilding took
5.20–5.29 ms.
That reconnect number intentionally includes the snapshot-copy cost and is not
presented as a production startup estimate.

All candidate membership answers, Diff entries, and history order matched the
public operations exactly. Both runs reported every row correct and every Store
unchanged. Counts, canonical storage, original SQLite bytes, and Commit IDs were
identical before and after candidate use and after dropping the sidecar.

The metadata figure is a compact logical encoding budget. It is not the
allocator-resident size of the Rust `BTreeMap`, nor a claim about future SQLite
page overhead.

## 5. PR decision

**YES — focused PR justified.**

The patch is an isolated benchmark/prototype only. It does not modify Commit
identity, Layer or Branch semantics, Store schema, public API, product behavior,
or the draft v0.1.4 benchmark registry/evidence.

The optional path-change hint was not implemented. Anchors are the only variable
in this experiment.

## 6. Related design precedents

Skip lists, append-only history skip indexes, Git commit-graph generation data,
Git changed-path Bloom filters, Merkle DAG history indexes, and ForkBase-style
lineage systems are relevant precedents. Spectral sparsification is only a
structural analogy. None is presented as a LayerFS theorem or proof.

Raw evidence SHA-256:

- run 1: `9232b8dc29eb633085b90e9b6d6c3bbc93e519130410d54473495c5d2b1441f7`
- run 2: `568c375feee53ea9563f69134fb8dfecb2c570b8f41e51aad24f046faa3bbcfa`
