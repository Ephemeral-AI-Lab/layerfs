# LayerFS history-anchor research prototype

This isolated benchmark tests whether deterministic sparse history anchors can
reduce long-history ancestry work in the current immutable Commit model.

It does not register a v0.1.4 benchmark ID, alter the append-only registry,
write derived data into the Store, or claim a production implementation.

Run from this directory:

```sh
cargo test
cargo run --release -- --output results/comparison.tsv
```

See `DESIGN.md` for the frozen protocol and `RESULTS.md` for the two formal
runs and interpretation.
