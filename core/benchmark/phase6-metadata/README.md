# Phase 6 metadata prototype

> **Status: Research; informative and not a product contract.**

Standalone host SQL representation diagnostic for [#294](https://github.com/Ephemeral-AI-Lab/layerfs/issues/294),
sub-issue of #293. The committed [prospective v1 specification](../../docs/architecture/proposal/phase6-sqlite-minio/experiments/METADATA-V1.md)
owns the cases, profile, limits, oracles and claim boundaries.

From repository root:

```sh
cargo +1.85.1 build --release --locked --manifest-path core/benchmark/phase6-metadata/Cargo.toml
cargo +1.85.1 clippy --release --locked --manifest-path core/benchmark/phase6-metadata/Cargo.toml -- -D warnings
cargo +1.85.1 fmt --manifest-path core/benchmark/phase6-metadata/Cargo.toml -- --check
python3 -m unittest discover -s core/benchmark/phase6-metadata -p 'test_*.py'
python3 core/benchmark/phase6-metadata/run.py --output /absolute/fresh/run --masters /absolute/owned/pristine-masters
```

The collector requires a committed clean source and a worktree-local release
binary. It records exactly one invocation for each of the 15 cases, independent
byte-copy fixture setup, dependency parity and separate readonly verification.
No collection rerun is a way to replace a result. An aborted collection retains
its evidence and all pending cases as NOT_RUN.

`operation_ns` covers the prototype workload. Lifecycle cases additionally pay
all connection opens/configurations/closes; other cases report those separately.
`sql_calls`, SQL timings and VM counters cover instrumented workload statements;
engine-open PRAGMAs are included in lifecycle wall but not those SQL counters.
`distinct_sql_texts` is not a count of preparation cache misses. Counter collection
and selected-row result delivery add prototype overhead and are part of its work.
Memory fields are SQLite allocations with an operation-reset high-water mark;
physical process/file-cache containment is unavailable. Numerical latency remains
cache INELIGIBLE and cannot establish a LayerFS product speedup.

The v1 affected-range transaction intentionally permits proportional changed-row
work so it can expose the predeclared 512-row target failure. No fallback/profile
increase turns that failure into PASS. These tools allocate no product schema,
API, canonical root, durability or concurrency capability.
