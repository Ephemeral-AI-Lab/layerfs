# LayerFS 0.1.2 Developer Preview

> Source-only Developer Preview release.

LayerFS 0.1.2 adds failure-atomic regular-file range editing through one
shared owner-side/FUSE piece engine while preserving the v0.1.1 storage, CLI,
daemon, projection, and explicit Workspace lifecycle contracts.

## Benchmark headline

Every 256 KiB count-changing temp-copy sample had a batch-average mutation
time below 10 ms/op. Full LayerFS lifecycle medians were approximately
25–343 ms across the 1/10/100-operation cases, and the 1/10/100 MiB
supplement demonstrates larger-file suffix-relocation scaling.

| Owner-side case | N | Edit median ms | Commit median ms | Lifecycle median ms |
| --- | --- | --- | --- | --- |
| Owner prepend, 10 B on 32 MiB | 3 | 9.167 | 2.869 | 26.432 |
| 100 owner overwrites, 4 KiB on 256 KiB | 3 | 11.763 | 3.359 | 25.368 |
| 100 owner inserts, 4 KiB on 256 KiB | 3 | 10.700 | 4.096 | 26.986 |

Family-level disposition:

| Evidence | Samples / receipts | Headline statistic | Disposition |
| --- | --- | --- | --- |
| Universal conformance | 51 native tests + 3 real-FUSE + scoped Clippy | create-handle direct-I/O coherence and mmap boundary pass | pass |
| Same-count | 84 / 6 proofs + 1 timing | aggregate A/A `1.004258` | target-pass |
| Count-changing primary | 150 + 45 controls / 7 | max candidate/baseline `1.096621` | tolerated-pass (<1.10) |
| Count-changing scaling | 18 / 18 | 100/10 delete `1.257939`; shrink `1.205768` | target-pass (≥0.90) |
| Store unique-100000 | 9 family performance / 3 family verifiers | `662,831,104` B vs `600,000,000` B | exact footprint blocker |

Copied-payload MiB/s is secondary for 256 KiB temp-copy cases. The scaling
supplement covers periodic destructive middle-edit suffix relocation; it does
not claim CDC uniqueness, ObjectId generalization, or near-size-independent
structural mutation. The exact tables, min–max ranges, timing boundaries,
resources, verifier walls, and manifest hashes are in
[benchmark-results.md](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/release-notes/0.1.2/benchmark-results.md).

A FUSE file handle returned by `create` uses direct I/O: same-handle and
concurrent-handle I/O is coherent, mmap on that still-open handle returns
`ENODEV`, and mmap works after close/reopen through the retained-cache handle.

The Store footprint goal is not waived: the primary median is
`662,831,104` bytes, `62,831,104` bytes above target.
Physical packs remain deferred to open issue #18.

## Start here

LayerFS 0.1.2 is source-only. Build from the immutable tag:

```bash
git clone --branch v0.1.2 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs
cargo build --release -p layerfs-cli
./target/release/layerfs --version
```

Prebuilt executables, crates.io packages, and runtime images are not published.
