# Stage 6 · C1 benchmark families (`layerfs-content`)

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Issue: [#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182), under
> Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
> Sibling documents: [`c2-families.md`](c2-families.md),
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md),
> [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md),
> [`gates_and_oracles.md`](gates_and_oracles.md).
>
> This is the case specification that must exist **before** benchmark
> implementation or sample collection.
>
> **No figure in this document is a measurement.** Every number is either a
> *declared constant read from source* or a *case configuration*. The only
> measured values quoted anywhere below are cited to an existing receipt and
> labelled diagnostic.

## 1. The one rule these families obey

> "**C1 opens no database, pack or file.** It asks a caller-supplied provider for
> already-authenticated canonical bytes, and it hands finalized canonical objects
> to a caller-supplied consumer."
> — `core/docs/architecture/01-boundary.md:15-18`

That sentence is what makes every family here independently measurable. A C1
family is therefore measured with:

| Role | Supplied by | Production type |
| --- | --- | --- |
| bytes in | the harness | `impl Read` / `&[u8]` |
| bytes out | the harness | `&mut dyn FinalizedConsumer` (`DiscardingConsumer` when nothing is stored) |
| timing | the harness | `layerfs_telemetry::timer::TimingScope` |

**Excluded from every C1 family:** C2, SQLite, packs, Workspace, Branch, Commit,
LayerStack, FUSE, POSIX, daemon, SDK, container, Monitor. A C1 family that needs
any of them is a `pipeline.*` family and belongs to the C2 document.

### Modes

Each family below is a **C1-only** family. The same production functions are also
reachable as `c2` and `pipeline` modes (as `measure_edits --mode c1|c2|pipeline`
already does); those modes are specified in [`c2-families.md`](c2-families.md).
Only `c1` mode is database-free.

## 2. Why these families, and what was dropped

The filter is **"does the measured operation need the runtime?"** — not "is it a
commit". Twenty-five v0.1.6 families were triaged; eleven survive on the C1 side.

| Dropped | Reason |
| --- | --- |
| `workspace_reliability` | fault injection — forbidden by `core/AGENTS.md` and #171 |
| `local_snapshot` (as a family) | Workspace lifecycle; the **fixture** is retained (§3.7) |
| `multi_workspace_development` | multiple Workspaces |
| `git_tool_workflow` | git/SDK orchestration; the **edit schedule** is retained |
| `branch_development`, `dedup_branch_history` | Branch and history semantics |
| `repository_history` | history replay |
| `historical_access` | sealed v2 Store, mounted retained state (already `NOT_RUN`) |
| `init_namespace`'s **2.7 s cold Init target** | a Stage 7 runtime target, not a C1 target |

Reason codes used in `benchmark/AGENTS.md` terms: **R1** Workspace/lifecycle ·
**R2** Branch/history/LayerStack · **R3** FUSE/POSIX/daemon/SDK · **R5** sealed
artifact unavailable · **R6** fault injection · **R7** multi-worker init.

## 3. The eleven families

Case IDs in the **source** column are the v0.1.6 IDs, kept only as provenance.
Core case IDs must be re-prefixed `c1.*`: reusing a v0.1.6 family ID for a
different operation surface is the `docs/general/benchmark_rules.md` §8 hazard.

| # | Core family | Source family | Cases | Tier / config | Counters and oracle |
| --- | --- | --- | ---: | --- | --- |
| C1-1 | `c1.construct.whole-file` | `payload_create_read` (create half) | 4 | 1 / 10 / 100 / 500 MiB; canonical ≤ cutoff−1+23 | `CdcCounters`, `DiscardingConsumer{canonical_bytes, peak_object_bytes}` |
| C1-2 | `c1.construct.chunked` | `payload_create_read` (create half) | 4 | same ladder, chunked route | `CdcCounters.chunks_emitted`, mapping pages, `MappingBuild.peak_pending` |
| C1-3 | `c1.cdc.chunk-count` | `edit_canonical_chunk_count` | 12 | 3 ops × 4 sizes; `START=147,456`, `LEN=65,536` | asserts `initial_count`, `final_count`, `final_sha256`, `file_root`, `map_sha256` |
| C1-4 | `c1.edit.length-preserving` | `edit_length_preserving` | 12 | 3 ops × 4 sizes; rotations `[0,5,10,3,8]` | `EditCounters`, read-amplification, manifest sha pinned |
| C1-5 | `c1.edit.length-changing` | `edit_length_changing` | 32 | 8 ops × 4 sizes; rotations `[0,13,26,7,20]` | `EditCounters{nodes_read, nodes_created, payloads_created, payload_bytes, peak_deferred_bytes}` |
| C1-6 | `c1.transition.boundary` | `file_size_transition` | 7 | 131,071 / 131,072 / 131,073; 4,096 and 1 MiB controls | whole-file ↔ chunked → empty; committed-length oracle |
| C1-7 | `c1.many-tiny` | `tiny_file_churn` | 20 | 5 kinds × 4 tiers; cutoff straddle | `validate_entries` byte oracle |
| C1-8 | `c1.tree.construct-traverse` | `directory_construction_traversal` | 12 | 3 kinds × 4 tiers | `SortedWork` dirs+inodes, `ObjectWork`, `untouched_subtrees` |
| C1-9 | `c1.tree.namespace-mutation` | `namespace_mutation` | 4 | 1 kind × 4 tiers | `ReleaseWork`, binding add/remove, `pages_reused` |
| C1-10 | `c1.change-locality` | `workspace_change_locality` (**renamed**) | 12 | 4 kinds × 4 tiers, minus the SDK-surface kind | **the COW claim**: `untouched_subtrees` vs `pages_reused` |
| C1-11 | `c1.fs.build-scale` | `init_namespace` (C1 half) | 8 | 100 f/5 MB/1 dir · 1,000/20 MB/10 · 10,000/300 MB/100 · 100,000/500 MB/1,000 (+4 `-text-v1`) | `FilesystemUpdateCounters`; build only |

### 3.1 Case IDs per family

```text
C1-1/C1-2  payload-create-{1,10}m-compact-v2, payload-create-{100,500}m
C1-3       overwrite-fixed-64k-chunk-count-{preserve,increase,decrease}  × 4 sizes
C1-4       overwrite-{head,middle,tail}-4k                               × 4 sizes
C1-5       insert-middle-4k, delete-middle-4k, append-tail-4k, prepend-head-4k,
           grow, shrink, truncate, zero-extend                            × 4 sizes
C1-6       v016-boundary-{small-control,below,exact,above,large-control,
           roundtrip,alias-roundtrip}-v1
C1-7       tiny-{create,stat,unlink}-{1,10,100,500}[-compact-v2|-mixed-v4]
           tiny-bulk-{create,delete}-{1,10,100,500}[-compact-v2|-mixed-v3]
C1-8       directory-{construct,metadata-scan,content-scan}-{1,10,100,500}[-compact-v2|-mixed-v4]
C1-9       namespace-subtree-relocate-delete-{1,10,100,500}[-compact-v2|-mixed-v4]
C1-10      workspace-{clean-commit,fixed-move,dense-rewrite}-{1,10,100,500}[-compact-v2|-mixed-v4]
C1-11      namespace-{100-compact-v3,1000-compact-v3,10000,100000}[-text-v1]
```

`-compact-v2` / `-mixed-v3` / `-mixed-v4` are **fixture-profile variants, not
tiers**. Only the four tier values `{1, 10, 100, 500}` are tiers.

### 3.2 Retained fixture — `local_snapshot`

The family drops (R1), the fixture is kept: **25,000 one-byte files**
(`f00000`..`f24999`), 256 edited ordinals `97*j`, root mode `0o755`, files
`0o644`. Use as a C1 many-tiny-objects case; drop the three-Commit schedule.
Retained fixture constant: `FILE_COUNT = 25_000`.

## 4. Boundary ladder (each row two-sided: accepted / refused)

Every figure below is enforced by a named check in
`core/crates/layerfs-content/src/filesystem/limits.rs` or the file-path policy.

| Surface | Constant | Value |
| --- | --- | --- |
| Small-file cutoff | `DEFAULT_SMALL_FILE_THRESHOLD_BYTES` | 131,072 (accepted range 131,072..=1,048,576, power of two only) |
| Cutoff boundary cases | `BOUNDARY_{BELOW,EXACT,ABOVE}` | **131,071 / 131,072 / 131,073** |
| Chunk grammar | `MINIMUM/TARGET/MAXIMUM_CHUNK_BYTES` | **8,192 / 16,384 / 32,768** |
| CDC boundary cases | `dedup_cdc_locality::boundaries()` | 0, 1, 8,191, 8,192, 16,384, 32,768, 32,769 |
| Whole-file canonical | `WHOLE_FILE_CANONICAL_OVERHEAD` | `cutoff − 1 + 23` = 131,094 |
| Object envelope | `MAX_CANONICAL_OBJECT_BYTES` / `MAX_OBJECT_FIELD_BYTES` | 16 MiB / 8 MiB |
| Mapping pages | `MIN_ENTRIES` / `MAX_ENTRIES` / `MAX_LEVEL` | 64 / 128 / 31 |
| Mapping node | `MAX_NODE_OBJECT_BYTES` | 8,192 |
| Edits per operation | `MAXIMUM_EDITS_PER_OPERATION` | **4,096** |
| Deferred edit bytes | `EDIT_DEFERRED_LIMIT` | 8 MiB − 1 |
| Compare window | `COMPARE_WINDOW_BYTES` | 65,536 |
| Read wave | `MAXIMUM_READ_DEMANDS` / `READ_WAVE_OBJECTS` | 4,096 / 32 |
| Navigation batch | `BATCH_CHILDREN` | 256 |
| Tree page | `MAXIMUM_PAGE_BYTES` / `EMPTY_PAGE_BYTES` | 8,192 / 44 |
| Directory leaf fill | `MINIMUM_FILLED_PAGE_BYTES` | 3,277 (2/5 rule); max rows 740 |
| Inode leaf rows | `MINIMUM/MAXIMUM_INODE_LEAF_ROWS` | **50 / 100** |
| Inode branch children | `MINIMUM/MAXIMUM_INODE_BRANCH_CHILDREN` | **64 / 127** |
| Names / paths | `MAXIMUM_NAME_BYTES` / `MAXIMUM_PATH_BYTES` / `MAXIMUM_PATH_COMPONENTS` | 255 / 4,096 / 256 |
| Symlink target | `MAXIMUM_SYMLINK_TARGET_BYTES` | 4,096 |
| Operation scratch | `MAXIMUM_OPERATION_SCRATCH_BYTES` | 4 MiB |
| Validation walk | `MAXIMUM_WALK_ENTRIES` | **4,096 per walk** — 4,096 accepted, 4,097 is the first refusal, and a directory whose effective subtree reaches it can never be renamed again |
| Attribute domain / key / value | limits.rs | 64 B / 255 B / 32,768 (32,769 refused) |
| Attribute key listing | `MAXIMUM_ATTRIBUTE_KEYS` | 4,096 |
| Ordering | `DEFAULT_MAXIMUM_PENDING` / `DEFAULT_ORDERING_BYTES` | 4,096 / 64 MiB |
| Profile | `PROFILE_DESCRIPTION` | `scoped-inline/v1;…;page8192;depth31;directory-fill2/5` |

**Fixture-unreachable, must be declared as such and never faked:**
`MAXIMUM_TREE_LEVEL` 31 (needs ~590 MB of input to force the level-1 cascade) and
`MAXIMUM_PAGE_BYTES` 8,192 (encoders refuse above it, so an 8,193-byte page
cannot be produced).

## 5. Inherited measurement contract

Applies verbatim from
[`docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/CONTRACT.md`](../../../../docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/CONTRACT.md)
§1–§2:

1. **One sample per case per arm.** No best-of, no n3. Repeats are labelled determinism diagnostics.
2. **Fresh `--output` per run.** Receipts append-only, never overwritten.
3. **Cache state declared and equal.** Construction families declare *warm in-process fixture; bytes are read before the timed region*, so no file I/O occurs inside the timer. Edit, transition and filesystem families read a **prepared base**, which is acquired as a per-sample byte copy and then **de-warmed** (`msync(MS_INVALIDATE)` + `mincore`, `resident_pages == 0`) before the clock starts. States are never pooled. Full discipline in [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md).
4. **One worker.** `LAYERFS_CONSTRUCTION_WORKERS=1` exported and asserted in the receipt (today nothing enforces `AGENTS.md` §3.8).
5. **Budget:** ≤ 15 s complete command; declared exceptions ≤ 25 s; verification ≤ 60 s.
6. **Counters are the gate; `elapsed_ns` is diagnostic.** Phase 0 proved every work counter bit-identical across runs while wall time moved **+17.6 %** on the same binary and input.

### Scaling gate (new)

Tiers exist to convert a point measurement into an algorithmic statement. Each
tier is a **separate registered case sampled once** — never n3 of one case. The
gate is the **per-doubling ratio**, not an absolute time:

| Claimed growth | Expected per-doubling ratio |
| --- | --- |
| O(1) — chunked construction/read memory | flat (≈ 1.0) |
| O(n) — construction, whole-file edit | ×2.0 |
| O(n log n) — tiered ordering merge | ×2.1–2.4 |
| O(n²) — the residual to hunt | ×4.0 |

### Selection lanes (new)

A full C1 run is **~127 cases × one sample each**, which is neither required nor
useful for every edit. Two lanes, mirroring the v0.1.6 `--smoke` flag:

| Lane | Selection | Size | Use |
| --- | --- | ---: | --- |
| `--smoke` | the smallest legal tier of each family | ~11 | the ordinary development loop |
| full | every registered case | ~127 | admission |

**Tier policy.** Four tiers is the working ladder for byte-size families — redundancy
against the established ±17.6 % elapsed spread; three is enough where the axis is
discrete. Top tiers that cannot fit the 15 s budget (500 MB payloads, 100k-file
trees) are **cut from the default set or declared on the ≤ 25 s exception list,
never shrunk to fit.**

**Pipeline is not a C1 lane.** Integrated C1→C2 timing is five cases (see
[`c2-families.md`](c2-families.md)), not a mode applied to every C1 family: a pure
CDC or read family has nothing to hand off.

## 6. Vehicles: what exists, what must be built

| Vehicle | Status | Gap |
| --- | --- | --- |
| `measure_components --mode c1` | exists | input-size tier is caller-written; cap 8 MiB |
| `measure_edits --mode c1` | exists | `nodes_read` **not** printed |
| `edit_timing_c1` | exists | **only** vehicle exposing `nodes_read`; **no size knob** (fixed 3.3 MB base) |
| `filesystem_timing_c1` | exists | **no `--entries` knob**; fixture sizes hard-coded 0/32/200 |
| `measure_filesystem` | exists | same; `--entries` **panics** (exit 101) — RUNPLAN's D7–D9 were invalid |
| `filesystem_primitives_candidate` | exists | `--files/--changes/--samples`; the only matched reference arm |
| `edit_memory_probe` | exists | heap peak only; fixed fixture |
| read-path vehicle (`read_all`/`read_range`/`FileView`) | **missing** | must be built |

**Known counter caveats — validate before gating:** `SortedWork.pages_read` still
undercounts batched merges (a third defect of the P1-11 family remains: an inner
engine inside `Engine::apply_root` returns its work to nobody); `pages_reused`
means *re-encoded to identical bytes*, which is **not** the COW claim —
`untouched_subtrees` (referenced by identity, never read) is.

## 7. Memory and CPU

C1 has **no CPU measurement anywhere** and no OS-RSS measurement inside `src/`.
Existing: a counting `GlobalAlloc` in three external targets
(`memory_ledger.rs`, `edit_memory_probe.rs`, `filesystem_ordering_scan.rs`) and a
phase-boundary `ps -o rss=` in `memory_ledger.rs`. Product-side reservation
counters (`peak_scratch_bytes`, `peak_deferred_bytes`, `peak_pending`,
`peak_run_bytes`, `FileBacking::peak_bytes`) are **declared charges, not RSS**.

Required additions: parent-side `getrusage(RUSAGE_CHILDREN)` for exact CPU
(lifetime-labelled), a phase-local RSS poller emitting the §10 field bundle, and
promotion of the counting allocator into a shared harness target. Sampler lives
in the **harness**, never in product `src/`.

## 8. Explicit non-claims

- No C1 latency or throughput target exists anywhere today; no number in this document is a gate until the Stage 6 contract is frozen.
- No v0.1.7-vs-v0.1.6 ratio for any C1 family: the only legitimate matched pair is `component.primitives` (3 cases).
- No total-RSS cap; only application-owned bounds that are established.
- No `init_namespace` 2.7 s target (Stage 7 runtime).
- No cold-cache claim except under an explicit, parameterized cold contract.
- No worker-count, timeout or cache-policy relaxation to turn a miss green.

## 9. Dependencies and open decisions

- Where the harness lives (`core/benchmark/` vs a `component` route in `fs-bench-pro`).
- Whether CPU/memory sampling enters `layerfs-telemetry` or stays harness-only.
- The two known harness defects: `collect.py` overwrites its log path while `commands.tsv` appends; and command lists drift from the real arg parsers.
