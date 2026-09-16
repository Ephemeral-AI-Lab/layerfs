# LayerFS core (replacement product workspace)

> **Status:** Current product workspace; three components are implemented and the
> reference tree still ships the released product.

`core/` is the home of the replacement LayerFS product. It is an independent
Cargo workspace with its own manifest, lockfile and target directory, and it is
excluded from the reference workspace at the repository root, which keeps
`crates/` available during migration.

Members are added only when a component boundary is agreed. The workspace
currently contains three packages.

| Package | Responsibility | Status |
| --- | --- | --- |
| [`crates/layerfs-telemetry`](crates/layerfs-telemetry/README.md) | Environment-independent parent/child timing trees | Implemented standalone; adapter integration is follow-up work |
| [`crates/layerfs-content`](crates/layerfs-content/README.md) (C1) | Canonical objects, complete-file construction, bounded logical reads | Implemented for the frozen 128 KiB/8/4 profile; edit and tree paths are later scope |
| [`crates/layerfs-storage`](crates/layerfs-storage/README.md) (C2) | Exact CAS reuse, supported FULL encoding, pack placement, real SQLite | Implemented FULL-only; DELTA, pooling and cloud placement are later scope |

Rules for anything under `core/` are in [`AGENTS.md`](AGENTS.md): production code
only in `src/`, external tests and runnable examples, production files limited to
999 physical lines, `lib.rs`/`mod.rs` limited to
200 physical lines of declarations, re-exports and thin delegation, and a
production LOC comparison for every commit.
Required capabilities fail explicitly; no retries/fallbacks, WAL or added durability
work, and no third-party patches are part of the current replacement design.

## Commands

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

### Real component runs

`examples/measure_components.rs` runs one real sample per mode against an
explicit input and a fresh output path. These smoke runs prove wiring and
readback; they are not release-admissible benchmarks.

```sh
run="$(mktemp -d /tmp/layerfs-stage02.XXXXXX)"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_bytes(bytes(range(256)) * 64)' "$run/input.bin"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c1 --input "$run/input.bin" --timings "$run/c1.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c2 --input "$run/input.bin" --store "$run/c2.sqlite" --timings "$run/c2.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode pipeline --input "$run/input.bin" --store "$run/pipeline.sqlite" --timings "$run/pipeline.json"
```

The two workspaces are checked separately: the commands above use the core manifest,
and the reference workspace uses its own. There is no aggregate repository gate —
`tools/preflight.sh` is permanently retired by owner decision (ledger L32), and it
runs no checks. Do not restore it.

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

The reference workspace keeps its own commands and lockfile:

```sh
cargo test --manifest-path Cargo.toml --workspace --locked
```

Design and migration: [`docs/roadmap/0.1/0.1.7/component-decoupling/`](../docs/roadmap/0.1/0.1.7/component-decoupling/README.md).
