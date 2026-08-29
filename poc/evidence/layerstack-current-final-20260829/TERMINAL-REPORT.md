# LayerStack cold-storage rewrite — exact-source terminal report

Verdict: **PASS_LAYERSTACK_LOCAL_STORAGE**

Date: 2026-08-29 (Asia/Shanghai)

## 1. Frozen source and delivery

The authoritative source is committed and pushed:

- Source commit: `74a40a7ac070d2b0b4106c1d02306dfab866af79`
- Branch: `main`
- `origin/main`: `74a40a7ac070d2b0b4106c1d02306dfab866af79`
- `canonical/main`: `74a40a7ac070d2b0b4106c1d02306dfab866af79`
- Exact Docker-input tree: `1ca2e7f6d6e5fa868febf479be91f93c0e7a4ea8`
- Tree construction: SHA-1 of `source-manifest.sha256`, which is the sorted SHA-256 manifest of all 129 Docker build inputs under `Cargo.toml`, `Cargo.lock`, `crates/`, `tools/`, and `containers/layerfs-fuse/`.
- Benchmark script SHA-256: `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`

`source-manifest-verify.txt` proves that every current source input matches the manifest. The image labels repeat the same commit, tree, and benchmark checksum. There is no post-benchmark runtime-source delta.

The source commit excludes unrelated untracked `docs/cli-tui/`, `docs/tui/`, and stale evidence directories. The evidence in this directory is delivered separately without modifying the benchmarked source commit.

## 2. Terminal disposition

Every binding terminal class is present in `terminal-gates.txt`:

```text
PASS_EXACT_SOURCE_TREE
PASS_STRUCTURE_42_7_7_CAP
PASS_SCHEMA_3_9_AND_8_24
PASS_DIRECT_TWO_DB
PASS_STACKED_THREE_DB
PASS_FUSE_DIRECT_TWO_DB
PASS_FUSE_STACKED_THREE_DB
PASS_ALL_FOURTEEN_OPERATIONS
PASS_TRANSACTION_ORDER_AND_VISIBILITY
PASS_CAS_CDC_IDENTITY
PASS_DIRECT_DEDUP
PASS_STACKED_DEDUP
PASS_TEN_INSTALL_STORAGE
PASS_NO_EXTRA_STORE_TURNS
PASS_REASONABLE_CONCURRENCY
PASS_FS_BENCH_DIRECT_OVERLAY
PASS_FS_BENCH_DIRECT_TMPFS
PASS_FS_BENCH_STACKED_OVERLAY
PASS_FS_BENCH_STACKED_TMPFS
PASS_WORKSPACE_FORMAT_TEST_CLIPPY
PASS_CLEANUP
PASS_LAYERSTACK_LOCAL_STORAGE
```

## 3. Phase completion and ownership

| Phase | Production owner | Focused proof |
|---|---|---|
| Canonical filesystem, rope, object codec, FastCDC | `layerfs-core` | canonical fixture oracle, extent/namespace/logical suites, shifted-stream FastCDC oracle |
| Shared schema, SQL, admission, transfer, merge-base, three-way | `layerfs-storage-core` | storage-core unit/integration suite, 100k merge-base, fixed-page membership/admission, final-only candidate objects |
| Branch persistence, coherent snapshot, Commit/Merge, layered reads | `layerfs-branch-store` | coherent Branch/head/root concurrency, layered error fallback, merge and transfer suites |
| Stack persistence, history composition, signed publication | `layerfs-stack-store` | authority, read-only history, remote session, Add/Push topology suites |
| Layer authority and final Stack publication | `layerfs-layer-store` | adversarial transfer, bounded publication, 100k relationship validation |
| Generic transient capture | `layerfs-workspace` | failed spool I/O zero-state-advance, sparse 64 MiB edit, rename/open-unlink/lifecycle tests |
| Application topology and exact 14-operation matrix | `layerfs-sdk` | API, Direct/Stacked topology, conflict, hash-count, realistic ten-install tests |
| Kernel presentation | `layerfs-mount` | build/Clippy, real direct and stacked FUSE functional oracle, commit/reopen digest |

## 4. Capture lifecycle after simplification

Workspace capture now has only two terminal operations:

```text
Workspace::open from one coherent Branch snapshot
    -> transient sparse overlay and semantic StagedChange stream
    -> Workspace::commit()
       or Workspace::discard()
```

The removed API and state are not retained behind aliases:

- no `FinalizedWorkspace`;
- no public `quiesce` or `finalize` stage;
- no `Direct::finalize` or `Stacked::finalize` forwarding;
- no duplicate public `Vec<Change>` plus private `Vec<StagedChange>`;
- no unused finalization-memory setting.

The resulting state machine is `Active | Committed | Discarded`. `BranchStore::branch_snapshot` holds the existing FIFO operation permit across Branch and Commit/root reads. Writes use all-or-error `write_all_at`, and write/truncate publish overlay state only after spool I/O succeeds. Candidate completion admits only generated objects reachable from the final root and stops at existing base objects. The sparse spool and global Workspace mutex remain intentionally unchanged.

Focused proof names are preserved in `final-workspace-gate.txt` and `focused-proof-summary.txt`:

- `branch_snapshot_never_mixes_a_head_with_another_heads_root`
- `failed_spool_io_does_not_advance_the_overlay`
- `candidate_keeps_only_objects_reachable_from_the_final_root`
- `sparse_overlay_commits_a_64_mib_base_without_hydrating_it`

## 5. Exact structure and API

`structural-audit.txt` reports:

- root workspace members: **10**
- storage plus SDK production Rust files: **42**
- Workspace production Rust files: **7**
- Mount production Rust files: **7**
- forbidden production filenames: **0**
- handwritten production files over 1,000 LOC: **0**
- serde/JSON/custom flat-Snapshot hits: **0**

The two largest files are cohesive and within the binding cap:

- `layerfs-storage-core/src/sql.rs`: 1,000 lines
- `layerfs-storage-core/src/merkle.rs`: 997 lines

The SDK compile/runtime API test proves the exact 14-operation union and the legal Direct/Stacked route matrix. Stacked does not expose the Direct-only `create_branch_from_layer` route. Internal numeric transfer results are named `AdmissionStats` and `TransferStats`; genuine benchmark validation artifacts remain receipts.

## 6. Persistent schema and runtime database census

All schema manifests are exact, and wrong-shape rejection passes:

- BranchStore: **3 tables / 9 columns**
- StackStore: **8 tables / 24 columns**
- LayerStore: **8 tables / 24 columns**, identical Full DDL to StackStore
- no Workspace, session, transfer, receipt, metrics, cache, recovery, or GC table

### Direct runtime

| Database | Runtime path | Tables/columns | Objects | Object payload | SQLite bytes | Final head |
|---|---|---:|---:|---:|---:|---|
| Branch | `/var/lib/layerfs/branch.sqlite` | 3 / 9 | 35 | 3,443 B | 45,056 B | Commit `12abe95ef74c25807a938625d7a20912ef739df897160c415f9287e114914628d7` |
| Layer | `/var/lib/layerfs/layer.sqlite` | 8 / 24 | 12 | 1,034 B | 98,304 B | Layer `32fdd9096a320557dc07487031f92fee7616955a0c97592f38cf19b40389438c08` |

### Stacked runtime

| Database | Runtime path | Tables/columns | Objects | Object payload | SQLite bytes | Final head |
|---|---|---:|---:|---:|---:|---|
| Branch | `/var/lib/layerfs/branch.sqlite` | 3 / 9 | 35 | 3,443 B | 45,056 B | Commit `12c197bf2c8db5449d34b86e887101fc393b9b424c5dea8ab5b605e73ac1a486a2` |
| Stack | `/var/lib/layerfs/stack.sqlite` | 8 / 24 | 12 | 1,034 B | 98,304 B | Stack `22d1fd808e75ff2538e870a9bd7697e4e47e4237a4b7e67795e604ddf20c48f682` |
| Layer | `/var/lib/layerfs/layer.sqlite` | 8 / 24 | 12 | 1,034 B | 98,304 B | Layer `325133245a6015894eb0fe1388065aac84e8f42d38bfeb116b4bca862759b39096` |

The complete table names, column names, row counts, and history IDs are in `direct-db-census.json`, `stacked-db-census.json`, and `schema-audit.txt`.

## 7. FastCDC quality and canonical identity

The independent from-scratch shifted-stream fixture is frozen in `fastcdc-shifted-stream.txt`:

```text
source bytes:             4,194,304
inserted prefix:              4,093
original FastCDC chunks:        220
shifted FastCDC chunks:         220
shared canonical suffix:        219 chunks
shared suffix bytes:      4,175,722
fixed-block shared suffix:        0 chunks
shared ObjectId digest:   61c91a10ca34fae310f4ab363fc5bfe35aa57a5fa2961d9fea3a11777509de2f
```

The test compares canonical encoded payload `ObjectId`s, not only raw boundaries. The fixed-block oracle is required to fail suffix reuse and does.

## 8. Realistic ten-install storage accounting

The deterministic package tree contains directories, 52 regular package files across `bin`, `lib`, `share/doc`, and `etc`, executable modes, a hard link, and a symlink. Its central canonical payload set is approximately 4.15 MiB.

| Topology | Database | Installs | Object rows | Object payload bytes | Metadata rows | Total DB bytes |
|---|---|---:|---:|---:|---:|---:|
| Direct | Branch | 10 | 433 | 4,147,543 | 12 | 4,415,488 |
| Direct | Layer | 10 | 438 | 4,148,012 | 25 | 4,476,928 |
| Stacked | Branch | 10 | 433 | 4,147,543 | 12 | 4,411,392 |
| Stacked | Stack | 10 | 438 | 4,148,012 | 36 | 4,472,832 |
| Stacked | Layer | 10 | 438 | 4,148,012 | 38 | 4,472,832 |

The ten installations reuse one canonical payload set per database. Only O(10) Branch/AddResult/Stack provenance rows grow. Direct concurrent Add callers converge on one Layer; Stacked creates one provenance Stack step per accepted Branch, then a repeated Push is `UpToDate`. Raw lines are in `sdk-focused.txt` and `focused-proof-summary.txt`.

## 9. Transfer, transaction, ordering, and concurrency proof

The measured large loopback transfer reports:

```text
P_o=16 H=1 J=2 turns=18
command_frames=20 payload_frames=446 reply_frames=19
bytes=8,470,834
```

Therefore actual synchronous request/reply turns satisfy the binding limit:

```text
turns = 18 = P_o + H + 1
```

Payload batching does not add a synchronous turn per object transaction. The 446 payload frames are streamed inside the operation envelope. The exact raw output is in `sdk-focused.txt`.

Other executable transfer invariants include:

- known descendant root: zero source-byte reads below the known root;
- 1,025 unique roots: 3 membership pages;
- 1,025 repeated identical roots: 1 membership page;
- 1,025 facts: 3 membership pages, 9 admission transactions, 4 turns;
- object admission: at most 128 objects and 4 MiB per batch;
- fact admission: at most 128 rows and 64 KiB per batch;
- peak transfer buffer: at most the 4 MiB object-batch bound;
- final Commit fact and Branch visibility are folded into the final transaction;
- the 129-fact Branch test uses 2 transactions and its `UpToDate` replay uses 0;
- roots and immutable dependencies precede Branch/AddResult visibility; copied Stack head is last;
- raw Transfer/End, cross-kind End, and mismatched pinned Begin/End sessions expose zero rows and move no head.

The receiver operation gate test queues ten callers, proves strict FIFO order `0..9`, maximum active operation count 1, no starvation, and a successful independent `SELECT 1` while the operation permit is held. No SQLite write transaction spans network waiting.

## 10. Query and bounded-history proof

- The 100,000-Commit linear merge-base lookup completes in **467.854 ms** in the recorded debug run.
- The diamond/unrelated-descendant query plan contains indexed immediate-parent searches through `commits_parent` and `commits_merge_parent`; it contains no transitive `descendants` materialization.
- The 100,000-pair Stack publication relation test performs one Stack position walk plus exactly **1,563** fixed 64-pair validation pages and asserts completion below 30 seconds.
- Commit ancestry is stepped in 512-row UNION-deduplicating recursive CTE pages.
- Candidate/object/history scratch is memory-first and spills at the fixed 8 MiB bound; publication and fact insertion remain bounded and paged.

Raw query evidence is in `merge-base-100k.txt`, `merge-base-plan.txt`, `storage-core-focused.txt`, and `final-workspace-gate.txt`.

## 11. Real FUSE functional closure

Both topologies mounted `fuse.layerfs` at `/workspace`. Docker inspection shows only the named database volume at `/var/lib/layerfs`; `/workspace` is not a bind mount, volume, tmpfs substitution, or ordinary directory.

Both Direct and Stacked passed:

- create/read/append/truncate;
- nested mkdir/find;
- rename/unlink;
- symlink/readlink;
- hard-link inode identity;
- chmod;
- mmap, flush, and fsync;
- open-unlink lifetime;
- clean signal-driven unmount and Workspace commit;
- reopen from the same physical database volume.

Both pre-commit and reopened semantic tree digests are exactly:

```text
7ca5a169755fdf57135af08860185b936e8c1147f1ca194920e50d5878bf5ba8
```

Results:

- `PASS_FUSE_DIRECT_TWO_DB`
- `PASS_FUSE_STACKED_THREE_DB`

The mount configuration uses four FUSE worker threads, `clone_fd`, `default_permissions`, a 1 MiB maximum write, and a 1 MiB maximum readahead. Functional operation categories are the syscall evidence for the supported contract; the production mount does not add a metrics or syscall-counter table.

## 12. Authoritative fs-bench results

All four exact-source populations used the unchanged 12-scenario matrix, `REPS=3`, `WARMUP=1`, randomized target order, `/workspace` as the real FUSE target, and network scenarios filtered out.

| Topology | Control | G | Rsum | SL (ns) | Spread | Result |
|---|---:|---:|---:|---:|---:|---|
| Direct | overlay | 3.2941303709 | 2.2398729746 | 3,230,610,043 | 1.0304981179 | PASS_OPTIMIZED |
| Direct | tmpfs | 3.6164849475 | 2.1282630012 | 3,217,505,696 | 1.0520985263 | PASS_OPTIMIZED |
| Stacked | overlay | 3.2187027177 | 2.1273061221 | 3,539,241,833 | 1.0280664127 | PASS_OPTIMIZED |
| Stacked | tmpfs | 3.6618833668 | 2.2171585918 | 3,444,096,835 | 1.0386276401 | PASS_OPTIMIZED |

Every aggregate gate and every Cloudflare-relative scenario gate is true. `receipt-summary.json` indexes the receipts and their raw/stdout checksums.

One initial Stacked-overlay population is retained as `stacked-overlay-attempt1-*`. A single `copy 64 MiB` sample stalled until awk's `%d` field saturated at exactly 2,147,483,647 ns, which failed the statistics and Spread gates. No source or runtime setting changed. The full population was rerun once and passed. `stacked-overlay-attempt1-diagnosis.json` records the failed checks; it is not substituted for or hidden inside the final receipt.

## 13. Image and runtime custody

- Image: `layerfs-fuse:final-1ca2e7f6`
- Image ID: `sha256:fc477aca0f63d3da3a7b5abe13b356e5eac4ca8380a33cd6b610cfd21d688a7c`
- Platform: `linux/arm64`
- Image size: 598,979,253 bytes
- Image source commit label: `74a40a7ac070d2b0b4106c1d02306dfab866af79`
- Image source tree label: `1ca2e7f6d6e5fa868febf479be91f93c0e7a4ea8`
- Image benchmark label: `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`

The Docker build itself ran Linux/arm64 mount tests, Clippy with warnings denied, and the release build before producing the image.

All temporary Direct and Stacked containers and database volumes were removed after evidence capture. Those disposable volumes are not recoverable; their schema/data censuses and file listings are retained here. `cleanup-after.txt` confirms zero matching containers and zero matching volumes.

## 14. Final source gate

The exact source commit passed, after all implementation and capture simplification changes:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

The full raw output is `final-workspace-gate.txt`. No source changed between that gate, the image source manifest, the FUSE runs, and the four final receipts.

## 15. Retained scope boundaries

These are deliberate boundaries, not failed terminal requirements:

- Workspace crash recovery and retry journals remain out of scope.
- Garbage collection and rollback remain out of scope.
- The transient Workspace has no database table.
- The global Workspace mutex remains because the passing workload does not justify per-inode lock machinery; immutable read plans release it before payload I/O.
- Aggregate/package LOC estimates remain review guidance, while the 1,000-line per-file cap is enforced.
- No hidden Push/Add, alternate CDC, cache Store, transfer/service crate, compatibility layer, or extra operation was added.

## 16. Evidence index

- Source: `source-freeze.txt`, `source-manifest.sha256`, `source-manifest-verify.txt`, `image-seal.txt`
- Structure/API: `structural-audit.txt`, `sdk-focused.txt`, `terminal-gates.txt`
- Schema/storage: `schema-audit.txt`, `direct-db-census.json`, `stacked-db-census.json`, volume file lists
- Capture: `workspace-focused.txt`, `final-workspace-gate.txt`
- CDC: `fastcdc-shifted-stream.txt`
- Transfer/ten-install: `sdk-focused.txt`, `storage-core-focused.txt`, `focused-proof-summary.txt`
- Query plans: `merge-base-100k.txt`, `merge-base-plan.txt`
- FUSE: functional, mountinfo, Docker mounts, container inspection, digest, and reopen inspection files for Direct and Stacked
- Benchmarks: four final receipt JSON files plus corresponding raw JSON, stdout, and verifier output
- Failed population disclosure: `stacked-overlay-attempt1-*`
- Cleanup: `cleanup-before.txt`, `cleanup-after.txt`
- Final integrity: `sha256sums.txt`
