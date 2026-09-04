# Phase 1 performance and admission review

Review role: independent performance/admission worker. Builds, preparation,
measurements and verification are serialized by the root agent. This document
records source inspection and experiment hypotheses; it does not claim a timing
or correctness pass without the corresponding root-owned evidence.

## Source and reuse ledger

- Implementation checkout: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-workspace-commit-engine`;
  repaired implementation base `a40b17e0` (full identity in the parent report).
- Reference checkout: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-bulk-create-feasibility`;
  specification revision `94bfdf40`. Read `two-phase-optimization-spec.md`,
  `report.md` and retained command receipts. No branch-wide cherry-pick.
- Selectively reused `2c30e7ef9d7fddeddb47ca6e8ec864bfbf11a5fe`: only the
  `schema.rs` opt-in SQL trace capture repair. Instrumented executions retain no
  SQL strings unless the existing explicit `reset_sql_trace` request enables
  capture. Existing explicit trace checks continue using the same API.
- Selectively reused `21c2929052c5bdf487b6d9c72fc0d9f8c3549063`: only the
  benchmark's `diagnostic-host-fuse` placement and independent
  `workspace-colocated-verify-existing` adapter, ported into the current split
  `src/workspace_bench.rs`. No frontier selector or specialized product code from
  that commit is reused.
- New comparator adapter uses the current ordinary registry's expected descriptors
  and existing public SDK initialization, monitoring, Store accounting and
  process-resource machinery. It does not add a release workload family.
- Historical `736fa13f` create and `22b31552` delete evidence remains bound to its
  own source/binary. Neither is a matched control for the new candidate. No
  compatible matched initialization measurement was found in retained evidence.

## Admission implementation and ownership

Hypothesis: moving finalized payload ownership and carrying a checked batch of
at most 8,191 objects / less than 4 MiB of allocated payload reduces copies and
underfilled transactions without changing membership, collision, accounting,
publication or failure semantics. Predictions: admitted payload copy bytes become
zero, moved payload bytes equal admitted missing canonical bytes, transaction
counts decrease when the former 127-object bound was binding, and unchanged
candidate/reused object equations continue to hold.

The normal path still calls `plan_candidate`, `insert_object_batch`, causal
`LaterAdmissionBatch` and final-publication checkpoints. It rejects an
initialization-only `all_missing` plan. It never calls empty-Store initialization
or its failure cleanup. Initialization retains its separate checked semantics.
Normal admission consumes canonical vectors in memory; spilled canonical bytes
are read directly into their owned final batch vectors. Batches continue across
all directory boundaries; there is no operation-family dispatch.

The vector reserves 8,191 object headers (458,696 bytes on the measured 64-bit
layout) once. Pending payload vector capacities, not just lengths, govern the
less-than-4-MiB payload limit. One incoming object can coexist before a flush;
`admission_pending_owned_peak_bytes` includes it and all reserved headers.
Existing candidate memory (8 MiB), candidate indexes (64 MiB category limits),
spilled candidate pending storage (1 MiB), Workspace state, reference/sort/tree
state and handoff remain additional simultaneous ownership. No new worker or
queue is added. The fixed cleanup slot owns at most five existing `PathBuf`
allocations transferred without cloning.

Final candidate object/index/order and missing-order artifacts are explicitly
unlinked before admission while retained file descriptors own readable bytes.
Failed removals remain in a bounded Store-owned cleanup slot. The next gated
Store operation and Workspace End/Discard retry cleanup or return an error,
retaining the session on failure. Historical SQLite objects are never removed.
Construction-error cleanup of intermediate Workspace runs remains a separate
review obligation; the final admission cleanup change does not prove every
private artifact path is covered.

Added checks (run outcomes belong in parent evidence): owned in-memory vector
pointer preservation; reading an already-unlinked spill; two full 8,191-object
batches plus a 46-object tail; existing/colliding object rejection; deterministic
cleanup failure, retained path ownership, blocked subsequent operation and retry.
The reliability `large-dirty` input still contains fifteen 1-MiB files plus two
64-KiB files, crossing the spill and multiple byte-limited admission boundaries.
Its causal fault receipt must independently show an early committed batch.

The specification's 8,191-object machinery differs from existing release report
checks that enforce 127 for ordinary Commit. Those report requirements have not
been edited. These results remain exploratory, not frozen-profile qualification.

## Counter interpretation and remaining bottleneck hypotheses

Existing benchmark `observed()` emits the Debug representation of every Commit
receipt, so the new admission, handoff and generic counters are present without a
second manually maintained printer. New generic values cover reference events,
reference bytes/capacity, mutation bookkeeping time, tree reads/emissions/reused
children and tree peak bytes. Fields must be populated from actual observations;
missing raw syscall/hash/cleanup subphase instrumentation is unavailable, not zero.

1. `changes.rs::reference_delta` binary-searches a fixed-record run via `pread` for
   every materialized node. A 100k create can therefore issue roughly 1.7 million
   41-byte random reads before duplicate scans. This is a prediction from source,
   not a measurement. Add `Run::at` calls/bytes and elapsed time. If confirmed,
   consume sorted references alongside identities or use bounded page buffering.
2. `Run::next` performs two reads per record and `Run::push` one write per record;
   every external merge pass repeats this traffic. Count reads/writes, bytes,
   merge passes and peak simultaneously open runs before changing buffering.
   A bounded block buffer is the first experiment if syscall overhead dominates;
   charge all simultaneous input/output buffers rather than giving each run a
   separate uncounted allowance.
3. `build_frontier_candidate` initially iterates every materialized node and reads
   its base inode/metadata even if unchanged. A sparse edit after a large listing
   can thus scale with materialized tree size despite persistent subtree reuse.
   This is a mandatory sparse-locality concern. Count materialized visits and
   authenticated base reads for a one-file edit after enumerating a large tree.
4. Handoff preparation batch-looks up inodes but individually verifies changed
   directory edges, metadata objects and file-state roots. This can move old
   refresh work into CandidateFinish. `handoff_binding_checks` does not count
   directory page reads or metadata/rope reads; add those counters or attribute
   their unavailable residual explicitly. Preserve the membership proof when
   replacing lookups with page-batched checks.
5. Existing changed-file compilation can decode portable metadata once for content
   mutation and again for final metadata comparison. Reuse exact already-known
   results if measured lookup/canonical-read counts demonstrate significance.

Create target <=10 s and generic delete target <=0.5 s are three-seed median
research targets. This review supplies no target pass. Shared-stage parity cannot
be inferred from a broad target pass, from specialized historical delete, or from
unaccounted producer imbalance.

## Private measurement commands and initialization qualification

Use a unique container/output identity for every attempt and archive source plus
binary/hash and command exit statuses. Retained exploratory image:
`sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e`.

```sh
docker run -d --name UNIQUE --cpus 2 --memory 2g --memory-swap 2g \
  --pids-limit 256 --device /dev/fuse --cap-add SYS_ADMIN \
  --security-opt apparmor=unconfined \
  --mount type=volume,destination=/data --entrypoint sleep IMAGE infinity
```

Retained private build cache exists under reference
`investigations/bulk-create/evidence/container-dense-delete-10-s1/failed-state/`.
Copy it into the private volume, synchronize only the pinned current source, then
build with `CARGO_HOME=/data/cargo CARGO_TARGET_DIR=/data/target CARGO_BUILD_JOBS=2`
and `cargo build --offline --locked --release -p fs-benchmark-pro`. Do not reuse a
prior binary as if compiled from the current source.

After cloning one selected qualified prepared Store to `/data/sample`:

```sh
fs-benchmark-pro workspace-run /data/sample /unused-input tiny-bulk-create-500 1 performance diagnostic-host-fuse
fs-benchmark-pro workspace-colocated-verify-existing /data/sample tiny-bulk-create-500 1
```

Use the corresponding `tiny-bulk-delete-500` and seed for delete. Verification
runs as a separate command; preserve performance receipts before it warms or
allocates additional caches. Snapshot cgroup `cpu.stat`, `memory.peak`,
`memory.events`, `memory.swap.current`, `pids.current`, container image/limits,
mounts and host interference separately. Cleanup checks inspect owned mounts,
processes and spool files before disposing only the private container/volume.

For the matched initializer, the retained native witness directory exists at
reference `investigations/bulk-create/evidence/native-500-s1/fixture`. Copy its
contents to a native volume `/native`; normalize the root to mode 0750 and mtime
1700000000. Build and qualify the final source using the same sealed helper:

```sh
# cwd /native; preparation and qualification are outside the initializer timing.
fs-benchmark-workload workspace-apply tiny-bulk-create-500 1 0 performance
fs-benchmark-workload workspace-verify-tree tiny-bulk-delete-500 1 0
# The delete input is exactly the final create tree including witness.
LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE=a10001 fs-benchmark-pro workspace-colocated-initialize /data/init /native tiny-bulk-create-500 1
fs-benchmark-pro workspace-colocated-verify-existing /data/init tiny-bulk-create-500 1
```

The diagnostic nonce must be nonempty hexadecimal. The adapter checks exact
ordinary expected file/byte counts, records SDK initialization wall and process
CPU/resources, and emits existing canonical/admission receipts. Preparation,
full input qualification and independent output verification stay separate.
The branch fork needed by the existing verifier is outside the initialization
interval. Record that interval and process command wall separately.

The final tree has two top-level producer tasks: `bulk` (100,000 files / 500 MiB)
and `witness` (200 files / 1 MiB). Initializer worker count is bounded by
`min(available_parallelism, 8, task_count)`; two workers do not establish two busy
producers throughout the run. Record per-producer wall, blocked time, files,
bytes and completion offsets from the existing v3 diagnostics. Do not rearrange
paths to distribute `bulk` among additional producers. Witness encoding by
initializer versus witness reuse by Commit is a genuine semantic difference.
Compare canonical bytes/objects, shared construction/admission intervals and
CPU, then explain Commit-only checks, refresh, cleanup and overlapping work.

## Selected retained preparation identities

All six selected inputs exist. Create preparation is the tier-independent 1-MiB witness; its `tiny-bulk-create-1` input is the same input for create500 at that seed. The current fixture-info plan digest and immutable clone hash must match the retained metadata before use. These retained source receipts are not new candidate qualification.

### tiny-bulk-create-1, seed 1

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/03638fe317044f1e9c30324e80e38d8da9ea76a348697b596900839bd015ee98`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/03638fe317044f1e9c30324e80e38d8da9ea76a348697b596900839bd015ee98/store/store.sqlite`
- Store SHA-256: `13b8c4b9576fc5377e5d87fe501979c805e5ac52af25670cf6dc5c17dd8e698d`
- Input plan SHA-256: `b792c818d27b61bd872e68350f88116fdc53df169f2981e27d365a6114554b30`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.03638fe317044f1e9c30324e80e38d8da9ea76a348697b596900839bd015ee98.prepare-hobl289o/store", "tiny-bulk-create-1", "1"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

### tiny-bulk-create-1, seed 2

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/10e4ae7dcef50ae3f1b544fb117db03969662a76afc4bfdb01ff52d8245f4195`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/10e4ae7dcef50ae3f1b544fb117db03969662a76afc4bfdb01ff52d8245f4195/store/store.sqlite`
- Store SHA-256: `4c940d57b10b822b8c6d0727367a45a30e6fcc6e723c88c6473b303f14905380`
- Input plan SHA-256: `d1dccf7829ac95307fca610135c2b305cf550d6a9e5f6f06039f689f3884b3e1`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.10e4ae7dcef50ae3f1b544fb117db03969662a76afc4bfdb01ff52d8245f4195.prepare-esinz_u7/store", "tiny-bulk-create-1", "2"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

### tiny-bulk-create-1, seed 3

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/7e65caee868fd0b2dca444937ae47978c3cc6ba6ef669d11af3f2bd1714f9e94`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/7e65caee868fd0b2dca444937ae47978c3cc6ba6ef669d11af3f2bd1714f9e94/store/store.sqlite`
- Store SHA-256: `97cb0546554bb1267d8e11fe1a9fdcaebf547b432d82c382786e14c6d0b8c67a`
- Input plan SHA-256: `6bca5a09716d254b395e05011e8065caef802530134ad4b90640f9899958a777`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.7e65caee868fd0b2dca444937ae47978c3cc6ba6ef669d11af3f2bd1714f9e94.prepare-jvzqymjx/store", "tiny-bulk-create-1", "3"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

### tiny-bulk-delete-500, seed 1

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/6b562e1ebe88e1dfa1a8f8aa6ec0a5140b275497d362d3b321518bb545947e91`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/6b562e1ebe88e1dfa1a8f8aa6ec0a5140b275497d362d3b321518bb545947e91/store/store.sqlite`
- Store SHA-256: `7fa7c8c46f648b5555fd5bbe4947f713bd01dacd0048dfb249f252f43aad5677`
- Input plan SHA-256: `d694a2456a3ec6ebfe8da6e041ec22fb2f954e63e336f897d82acef4f76dfd71`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.6b562e1ebe88e1dfa1a8f8aa6ec0a5140b275497d362d3b321518bb545947e91.prepare-8ez5f4p1/store", "tiny-bulk-delete-500", "1"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

### tiny-bulk-delete-500, seed 2

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/39f8e0985e1086f46011ee53b37ad5e790db567e8a3085661845591d31a49a4c`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/39f8e0985e1086f46011ee53b37ad5e790db567e8a3085661845591d31a49a4c/store/store.sqlite`
- Store SHA-256: `44e3c3865a716de4277852960280d6164d9fdfd39932548dd1eb75380212bae2`
- Input plan SHA-256: `a765cfbe269fa7977612901c71457d47ff37d3190c849db7d0e84d4bb62072a7`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.39f8e0985e1086f46011ee53b37ad5e790db567e8a3085661845591d31a49a4c.prepare-dx3ffrdf/store", "tiny-bulk-delete-500", "2"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

### tiny-bulk-delete-500, seed 3

- Cache: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/6c6e9f3f4d6ad577c34c05760bd271258c770531ffd87aa52ebe876a2c62dbba`
- Store: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/6c6e9f3f4d6ad577c34c05760bd271258c770531ffd87aa52ebe876a2c62dbba/store/store.sqlite`
- Store SHA-256: `6789eb086aa74b934103816f759a207c17ec5c79c72d72dd861696bd3b133a4a`
- Input plan SHA-256: `0fc5b5ffb16d27b33a629e96e2c75cdeef6853e8b6bf73f3b8c2f5b3e590e1df`
- Original preparation command: `["/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/fs-benchmark-pro", "workspace-prepare", "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/phase1-prepared/.6c6e9f3f4d6ad577c34c05760bd271258c770531ffd87aa52ebe876a2c62dbba.prepare-0rbubk6c/store", "tiny-bulk-delete-500", "3"]`
- Existing custody: `cache.json`, `fixture.json`, `input-manifest.tsv`, `evidence.sha256`, `producer-build`.

If a retained input is missing or incompatible, prepare only that selected replacement with `fs-benchmark-pro workspace-prepare /data/prepared-SEED/store tiny-bulk-create-500 SEED` or `tiny-bulk-delete-500 SEED`. Preserve stdout fixture/branch/plan receipt, the parent `input-manifest.tsv`, producer and helper hashes, preparation source and Store SHA-256. Use the existing custody/qualification machinery; do not run a whole campaign or prepare unselected families.
