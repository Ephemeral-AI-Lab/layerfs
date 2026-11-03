# Lead checks at the final product tree (`3e257b91c3`)

Item 8 (#535 phase 8) / #536 §12 item 7: the #535 §11 check set on the
final product commit. Run in `layerfs-issue535-connect` at `54161c9e2d`
(`HEAD`), whose product tree equals `3e257b91c3`
(`product-tree-vs-3e257b91c3.txt` is empty: `git diff --stat` over `api`,
`engine`, `core/crates`, `connect`, `tools`). macOS host, `cargo +1.85.1
--locked --offline`, the worktree's registered Cargo home and target.

| Check | Result | Log |
| --- | --- | --- |
| `test --all-targets --no-run`, `clippy --all-targets -D warnings`, `fmt --check`, `check_product_boundary.py`, `check_fuser_integrity.py`, tools unittests | PASS (`ALL rc=0`) | `checks.log` |
| clippy `--all-features` for layerfs-machine, -control, -api, -cli, -persistence, -store-service; `--no-default-features` for api, control, cli | PASS (no warning) | `clippy-features.log` |
| Tests per package, each under a 120 s limit (`sweep.sh run`) | PASS: api with `machine` 100, without 81; control with `machine` 75, without 71; machine 83; sandbox 71; bridge 30; store-backup 19; cli 6; 0 failed | `sweep.log` |

Not run here: Linux targets (lane L3 ran the Linux tests named in its
part at `35ed671b6c`; the two later fixes are covered by lanes R2 and R3),
FUSE mount targets (nightly lane, unchanged since the verify lanes),
PostgreSQL-gated targets (run against a throwaway server for `cc216bf638`).
