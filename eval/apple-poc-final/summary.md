# LayerFS Apple/APFS PoC final receipt

Disposition: **PASS for the frozen AppleWorkspaceV1 PoC scope**. The three rows
were produced after source freeze and the final workspace test/clippy closure.

Three isolated release runs completed S0–S12 with exact byte/tree, symlink,
hard-link, mode/mtime, ACL, BSD-flag, FinderInfo, resource-fork and supported
xattr checks before and after reopen/compaction. Every row reported zero owned
residue and an unchanged 4-to-4 file-descriptor baseline. The product Q gauge
observed a 4 MiB reservation high-water and returned to zero at every stage.

| Metric | Result |
|---|---:|
| Internal wall, median (range) | 3.325 s (0.092 s) |
| External gross wall, median (range) | 3.43 s (2.42 s) |
| CPU user + system, median | 1.28 s |
| Maximum RSS, median / maximum | 20,480,000 / 20,774,912 bytes |
| Peak footprint, median / maximum | 18,907,520 / 19,104,128 bytes |
| Product operation-owned Q current / high-water | 0 / 4,194,304 bytes |
| SQLite page/cache profile | 4,096 bytes / 1,280 pages |
| Conservative compaction peak, median / maximum | 809,810 / 809,810 bytes |
| Store bytes after compaction, median (range) | 368,794 (4,096) bytes |
| Created / authenticated-reused objects | 277 / 941 |
| Terminal owned residue / FD delta | 0 / 0 |

Commands:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
cargo build --release -p layerfs-eval
/usr/bin/time -l target/release/layerfs-eval apple-poc <isolated-APFS-directory>
```

The gross wall includes deterministic workspace preparation, exact post-checks,
and evaluator-owned run-directory cleanup; release compilation is excluded.
No p95 or production SLO is inferred from three samples.

The diagnostic makes no changed-root incremental materialization, hardware
power-loss, hostile-writer, production packaging, cross-platform performance,
or FSKit claim.
