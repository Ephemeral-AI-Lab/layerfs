# #237: v0.1.6 exact-source 100k storage reference

> **Status: Research; informative and not a product contract.** Frozen before
> this reference run. The historical v0.1.6 100k row used different source
> bytes; the Core release control and C1 receipts remain unchanged.

## Question and source

Measure the released v0.1.6 directory Init and closed SQLite Store on the
**exact** Core SDK SHAKE seed-1 `namespace-100000` source: 100,000 files,
1,001 directories including root, and 500,000,000 logical file bytes. The
prepared master's `manifest.tsv` SHA-256 is
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`.
Use a fresh independent byte copy. Validate every source path, kind, mode,
mtime, size and file SHA-256 against the master manifest before GO. The
reference operation surface is one public
`Client::initialize_layerstack(..., LayerStackInitialization::Directory(copy))`
call, with a fresh Store and Client prepared outside its monotonic timer.
No synthetic or sampled legacy fixture can replace this input.

The product is the peeled `v0.1.6` tag commit
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, whose `crates` tree is
`dcc4fb6fd01115dcbf91ba02df414e91eb5733be`. The benchmark-only
READY/GO route starts from `b0730c70a567f5019ba0435d73bfa9f6a04b4004`,
which has the identical product tree. Adapt only the harness for this exact
100k source and full oracle. Build the benchmark driver with locked Cargo
**release** profile in the isolated v0.1.6 worktree. Record the final harness
commit, binary hash and compiler/build inputs before the call.

## One-call protocol and custody

Run exactly one reference call at this identity in a fresh, nonoverwriting
output. The wrapper copies the closed master outside the timer. After the
benchmark process signals READY, it validates and invalidates all source
payload pages, confirms zero resident pages across the whole copy, rechecks
without faulting, then sends GO. The Rust timer begins immediately before
the public call and ends at its return. No source payload read or warmup may
intervene between final residency check and the timer. Preserve blocked,
failed, ineligible and over-budget receipts; never resample this arm. Record
the cold backend, final resident/total pages and recheck-to-timer gap. Source
directory/inode metadata residency is unqualified, so even a payload-cold
row is diagnostic and not a fully cold admission result.

The fixed output is
`benchmark-results/issue237-v016-exact-100k-20260924-01` in the isolated
v0.1.6 worktree. The command, from that worktree's root, is:

```sh
python3 benchmark/fs-bench-pro/issue237_v016_reference.py \
  --master /Users/yifanxu/.codex/worktrees/bb86/layerfs/benchmark-results/fs-bench-pro/sdk-prepared/namespace-100000-b69e710dfd0474a5 \
  --cold-driver /Users/yifanxu/.codex/worktrees/bb86/layerfs/docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --binary target/release/fs-benchmark-pro \
  --out benchmark-results/issue237-v016-exact-100k-20260924-01
```

The expected raw layout includes `identity.json`, `source-copy.json`,
`cold-preflight.json`, `cold-recheck.json`, `stdout.txt`, `stderr.txt`,
`receipt.json`, `store/store.sqlite`, and the separate `readback/` Store copy.

## Frozen executable identity before GO

The isolated, clean benchmark-only commit is
`61d1eb10d8096f1315640fc2623542a34685850d`, with benchmark tree
`667b1317dc2295ee70ff411f272962573ce59afd` and unchanged release
product tree `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`. Locked Cargo
release build used Rust `1.85.1`; its executable SHA-256 is
`641f710b76599d27437556490c9acd9a4636a9f7dfe6ae33dd1dc9ca211ddca0`.
The wrapper SHA-256 is
`91b2ea0007e4966d386421ad1cc415db11a190b0dda1b5bfb5b31b89eed8427a`;
the Rust benchmark `main.rs` and `repository_init.rs` SHA-256 values are
`9eda4c5c9d21bb509360ce7c7df304d9d1df448c62a3c221056c65330351b09e`
and `8540684f77113a6875393ceb1ab426e7e5d765e38c7b5c5b9e826da8c88d3e1b`.
The shared cold driver SHA-256 is
`a730c67107ecfbb11f3cf91e260e5b9b34438e41accec665b95953d6d275ddfe`.
The fixed output path was absent when these identities were frozen.

The declared complete performance command limit is 15 s; the independent
full verifier expectation is under 10 s. Safety watchdogs do not relax
either limit. Record public time, command wall, process CPU and sampled RSS
with their exact scopes. Keep preparation, readback, Store queries and SQL
plans outside the public timer. A build overlapping this worktree's timed
operation is forbidden; any competing other-worktree build is recorded.

## Required readback and space geometry

After the process closes the Store, copy that Store once for separate
readback. Reopen it and verify the complete 101,001-path manifest: each file
hash/size and each file/directory kind, mode and mtime, including empty
directories and absence of extra paths. Record the verifier wall and any
partial/failed coverage; a sampled checker is insufficient. Do not mutate
or compact the measured Store.

Query the closed original Store read-only for apparent (`st_size`) and
allocated (`st_blocks * 512`) bytes; SQLite page size, page count and freelist;
`dbstat` page allocation by table/index; object, pack, metadata and signature
rows; and pack BLOB length, declared used length and internal directory
vacancy where the old format exposes them. Report `NOT_MEASURED` for a field
that the old format cannot expose without inference. Preserve raw SQL/geometry
and Store hash. Core's comparison denominator is its content Store **plus**
History; report its two files separately and combined. Compare this matched
reference against the already retained Core release control
(`5.077702667 s`, `554,098,688 B` apparent, `558,145,536 B` allocated) and
C1, without rerunning either. Keep differing public API, Store format and
cache limitations explicit. This diagnostic may identify a space mechanism;
it does not itself certify a release speedup or authorize C2 adoption.
