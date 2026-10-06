# Backed acquisition checkpoint: retained failures

> **Status:** Historical receipt. Append-only; do not relabel as passes.

1. **SQLite scratch refused by the product boundary guard.** The first arrangement
   placed an operation-owned SQLite scratch inside `layerfs-project` and added
   `rusqlite` to its manifest. `python3 -B core/tools/check_product_boundary.py`
   reported `FAIL: 2 product-source boundary violations`
   (`engine dependency in domain layerfs-project -> rusqlite`, default and Linux
   target entries). The guard was not changed. The arrangement was withdrawn and
   replaced by Content's public ordering-run backing. Receipts `01-host-build` and
   `02-host-tests` belong to that withdrawn source (cohort `df128acd...`) and are
   not evidence for the committed source. The guard run that failed was made
   directly, without a receipt file; this entry is its record.
2. **`10-linux-tests`: exit 101.** `tests/init_sqlite.rs:25` unwraps
   `Handles::create`, which returns `BackendUnavailable` on Linux because the
   global Store is macOS-only. Init is never reached. The test file is unmodified
   and is not platform-gated; the defect predates this checkpoint and is not
   repaired here. Nine bodies in four earlier binaries passed in that run,
   including all three backed-acquisition bodies. Cargo stopped before
   `tests/namespace_scaling.rs`; `11-linux-namespace-scaling` ran that one binary
   once and passed. Receipt 10 stays FAILED.

No test reached its explicit wall timeout.
