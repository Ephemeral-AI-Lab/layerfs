# C3 promotion: v0.1.6 exact-source side-by-side handoff

> **Status: Research; informative and not a product contract.** Handoff
> for the next agents after C3's product promotion. The #236 registered
> SDK selection remains debug-only; do not relabel a release diagnostic
> as its admission result.

## Ready-to-send prompt

You are continuing #237 after C3's placement rule was promoted to the
replacement Core product. **Begin with the released v0.1.6 arm on the
exact Core SDK `namespace-100000` SHAKE source**, then build a
stage-by-stage side-by-side explanation against C3. Use **release
binaries in both arms**. Pin product, harness, binary and fixture hashes,
public API, cache state and timer boundaries before interpreting any
number. Preserve every failed, incomplete and over-budget receipt.

The same-source v0.1.6 public performance arm has **already run once**.
First inspect and reuse its retained
[receipt and full oracle](https://github.com/Ephemeral-AI-Lab/layerfs/blob/e2c8e6937/core/docs/issues/237/v016-exact-100k-storage-result-20260924.md)
and the local raw output at
`/Users/yifanxu/.codex/worktrees/issue237-v016-exact-100k/layerfs/benchmark-results/issue237-v016-exact-100k-20260924-01/`.
Do **not** repeat that unchanged performance arm to get another time.
If the missing stage counts require running old code, freeze a new,
benchmark-only **count-driven diagnostic** on the identical fixture:
retain the peeled v0.1.6 product `crates` tree, instrument aggregate
stage/SQL/pack counts, label the run diagnostic, and report it alongside
the original one-shot performance row. It cannot replace or improve
the recorded 3.779070375-s public-call number.

## Exact first arm and source custody

- Peeled v0.1.6 tag commit:
  `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
  The old [benchmark-only branch](https://github.com/Ephemeral-AI-Lab/layerfs/tree/codex/issue237-v016-exact-100k)
  at `61d1eb10d` has the same product `crates` tree
  `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`.
- Exact shared source manifest SHA-256:
  `23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`;
  seed 1, 100,000 files, 1,001 directories, 500,000,000 logical
  bytes. The prepared master currently lives at
  `/Users/yifanxu/.codex/worktrees/bb86/layerfs/benchmark-results/fs-bench-pro/sdk-prepared/namespace-100000-b69e710dfd0474a5/`.
  Each public arm needs a fresh independent byte copy and a fresh Store.
- Old release driver SHA-256:
  `641f710b76599d27437556490c9acd9a4636a9f7dfe6ae33dd1dc9ca211ddca0`.
  Its READY/GO wrapper confirmed **0/126,206 resident payload pages**
  immediately before one
  `Client::initialize_layerstack(...Directory(copy))` call.
  Inode and directory metadata residency was not qualified.
- The old call took **3.779070375 s**. Its complete command took
  19.119348541 s, above the 15-s budget, and its separate
  all-path/all-byte reopened verifier passed in 21.895595708 s,
  above the under-10-s expectation. The Store was
  514,879,488 B apparent / 520,110,080 B allocated.
  Keep these misses visible.

The historical command shape, from the old worktree root, was:

```sh
python3 benchmark/fs-bench-pro/issue237_v016_reference.py \
  --master /Users/yifanxu/.codex/worktrees/bb86/layerfs/benchmark-results/fs-bench-pro/sdk-prepared/namespace-100000-b69e710dfd0474a5 \
  --cold-driver /Users/yifanxu/.codex/worktrees/bb86/layerfs/docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --binary target/release/fs-benchmark-pro \
  --out benchmark-results/issue237-v016-exact-100k-20260924-01
```

That output already exists; treat this as a provenance record, not an
instruction to rerun the unchanged arm. A new count-driven diagnostic
needs its own prospective instrument and fresh output identity.

## C3 arm and comparison work

The promoted C3 product commit is `92492722a`, followed by an
external-test SQL type correction at `c3a4766dc`. Its placement
algorithm closes Ordinary/Native/WholeFile packs at exact used
length each flush, while one pooled-metadata pack remains open for
cross-flush in-place appends. No new pack version, reader, page size
or worker count was introduced. Its prior one-shot
[research C3 release receipt](https://github.com/Ephemeral-AI-Lab/layerfs/blob/e2c8e6937/core/docs/issues/237/pack-space-c3-result-20260924.md)
used the same SHAKE source: 5.013439250-s public
`Client::init_project` call, 6.397864750-s complete command,
8.644194375-s full reopened oracle PASS and
523,399,168 B combined Store + History allocation.
That receipt belongs to its recorded research source/harness seal;
do not silently relabel it as a sample from the merged commit.

The next agents should:

1. Compare the **actual operation boundaries** first. Old
   `Client::initialize_layerstack` and Core `Client::init_project`
   are different public APIs. Do not turn their two raw times into a
   claimed version speedup or regression. Check the old reference
   scanner/import and the merged SDK route end to end.
2. For both release routes, build a table of **nonoverlapping stages
   where possible**: source scan/file opens/read bytes, producer
   construction and blocking, handoff counts, pack creates/appends
   and bytes, SQLite statements/BEGIN/COMMIT wall, root/namespace
   construction, publication, process setup and teardown. Mark
   stages that do not map one-to-one. Worker wall sums and nested
   SQL timers overlap and must not be added to public wall.
3. Inspect the two closed Stores read-only: 4-KiB page/count/freelist,
   `dbstat`, pack count/BLOB length/declared used/directory occupancy,
   object and metadata rows, `st_size` and `st_blocks*512`.
   Count Core content Store **plus** History. Separate SQLite
   file-length savings from filesystem allocation excess.
4. Attribute process memory and CPU using like-for-like windows.
   The old run reported 92,405,760 B process high-water RSS at
   public-call end; C3's driver `wait4` lifecycle peak was
   154,189,824 B. This raw ~61.8-MB difference has different
   observation scopes. Measure SQLite page cache, construction
   buffers and host page cache separately before assigning cause.
5. Preserve the separate #229 sparse-history gate. C3's dense result
   does not prove sparse compactness. The later
   [C5/C6 research](https://github.com/Ephemeral-AI-Lab/layerfs/blob/e2c8e6937/core/docs/issues/237/db-space-handoff-20260924.md)
   found a focused sparse benefit but both candidates missed their
   frozen dense allocated-space rule. Do not import those changes
   into this C3 promotion by implication.

Use the repository [benchmark rules](../../../../docs/general/benchmark_rules.md),
root and Core `AGENTS.md` before any new measurement. One sample per
arm/identity, fresh outputs, locked release builds, equal declared
cache treatment, full independent readback and explicit admission
status remain mandatory. An instrumented diagnostic may explain the
old/current stages; it is not another performance sample. The
registered #236 debug 100k row remains `NOT_RUN`.
