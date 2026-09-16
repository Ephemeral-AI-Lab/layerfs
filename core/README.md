# LayerFS core (replacement product workspace)

> **Status:** Current product workspace scaffold; the first component is
> implemented and the reference tree still ships the released product.

`core/` is the home of the replacement LayerFS product. It is an independent
Cargo workspace with its own manifest, lockfile and target directory, and it is
excluded from the reference workspace at the repository root, which keeps
`crates/` available during migration.

Members are added only when a component boundary is agreed; the workspace
currently contains exactly one package.

| Package | Responsibility | Status |
| --- | --- | --- |
| [`crates/layerfs-telemetry`](crates/layerfs-telemetry/README.md) | Environment-independent parent/child timing trees | Implemented standalone; adapter integration is follow-up work |

Rules for anything under `core/` are in [`AGENTS.md`](AGENTS.md): production code
only in `src/`, external tests and runnable examples, `lib.rs`/`mod.rs` limited to
200 physical lines of declarations, re-exports and thin delegation, and a
production LOC comparison for every commit.

## Commands

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

Both workspaces are checked by the repository pre-push gate:

```sh
tools/preflight.sh
```

The reference workspace keeps its own commands and lockfile:

```sh
cargo test --manifest-path Cargo.toml --workspace --locked
```

Design and migration: [`docs/roadmap/0.1/0.1.7/component-decoupling/`](../docs/roadmap/0.1/0.1.7/component-decoupling/README.md).
