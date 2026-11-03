# §10b `related` on a `machine.remove` refused `InUse` (`32aec70a64`)

The one §10b case the final run left NOT_RUN. Running it showed the defect
fixed in `32aec70a64` (`inventory::can_remove_machine` refused with
`detail: null`). Checked live at `32aec70a64`, 2026-11-03 09:44 UTC.

- CLI: `cargo +1.85.1 build --locked --offline -p layerfs-cli` at
  `32aec70a64` (debug, default features), sha256
  `0f32f420f29612bfb3b003acd3767bfeaf87ba94066fb82cf21a79752f55eb87`.
- Bundle: `k3-bundle` (hash `185b0240…`, built at `54161c9e2d`); the fix is
  in the caller's `machine.remove` and changes nothing the worker runs.
- One microsandbox machine `lfs535mr`, a sqlite Store `mr` (project `p`
  Inited on native, pushed to the machine), a 0700 scratch home.

| Step | Result |
| --- | --- |
| `machine remove lfs535mr` while it holds Store `mr` | **PASS**: `InUse` "machine lfs535mr holds Stores [mr], Sandboxes [] and hosts []", `detail.related` `[{kind: "store", name: "mr"}]` |
| `store pull mr`, then `machine remove lfs535mr` | PASS (exit 0) |

The Sandbox and hosted-machine entries are covered by the `inventory` test.
Removed after: the machine (`machine remove`; `msb list` shows none), the
Store and the scratch home. Log: `check.log` (commands, answers, wall times).

## Checks at `32aec70a64`

The #535 §11 check set (`checks.log`: `ALL rc=0`) and the packages the fix
touches, each under a 120 s limit (`sweep.log`): layerfs-control with
`machine` 75, without 71; layerfs-api with `machine` 100, without 81; 0
failed. The other packages are unchanged since `3e257b91c3`
(`receipts/3e257b91c3/lead/`).
