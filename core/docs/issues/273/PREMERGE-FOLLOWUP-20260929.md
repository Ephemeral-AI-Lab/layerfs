# #273 follow-up: original #248 gate and mounted cases, retained red cases

> **Status:** source-pinned functional checkpoint at `96d196dec56edd78c930882d621660c512c5c194`
> (tree `a65e74ac2aab5b3dc39059b818a19b5ce379f0d7`), product commit
> `b551ebbd80c6e419e75f63ccfad2e5abe6a1ec19`. This report is docs-only;
> it does not retrospectively update [the preceding handoff](PREMERGE-CONTINUATION-20260929.md),
> repair a failing receipt, update a PR head or approve a merge.

## Original registered #248 public selection — functional PASS, numeric INELIGIBLE

Ran exactly `issue248-separated-4097-v1` **once** on the clean source, with
`core/benchmark/fs-bench-pro/checkpoint5_273.py oracle`, `prepare --arm candidate`
and `run --selection issue248-separated-4097-v1` in this owned worktree. The
shared frozen #271 gate Store/history was independently byte-copied and sealed;
locked release `benchmark_shell`, verifier and aarch64-musl daemon binaries,
static writer and immutable image were built in this worktree. The candidate
prepared image at interval 1024 was
`sha256:635c2e415eb8f8a1cd86ef2f2cd4b2233ed9bb9f5badfb387689d5d7a43cb3b5`.
Product call: public SDK WorkspaceApi mount, single Exec of the original
`/fixtures/bin/write-separated separated data.bin 4097`, Commit and cleanup;
not the 4,096-write stage substitute.

- Recorded **4,097 FUSE WRITE callbacks**, writer progression to 4,097,
  complete checkpoints, complete C1 source counters, both separate independent
  verifier executions **PASS**, unmount/delete **PASS**. Emitted
  `LFS_C1_EDIT_LOAD v=1 nodes_read=0 stored_nodes_read=0 draft_nodes_read=0`;
  `LFS_FILE_INPUT v=1 edits=4097` (the latter does **not** substitute for C1
  source provenance). Complete command 6,964,812,708 ns, below the unchanged
  25 s ceiling; one construction worker.
- Host Store/history clone: `darwin-shared-mmap-invalidate-mincore-v2`, zero
  resident pages on both post-invalidation and final whole-input checks;
  recorded launch gap 3,792 ns. This does **not** prove private container,
  VM/backend/device or host cache identity. The container-side private backing
  is not invalidated between Exec and Commit. No reset phase-local peak was
  established. Receipt statuses: `functional_status=PASS`,
  `row_status=INELIGIBLE`, `commit_cache_status=INELIGIBLE`,
  `resource_status=PARTIAL`, `admission_eligible=false`. This is **not** a
  matched performance arm, speed admission or reason to relabel nine historical
  INELIGIBLE rows; frozen control remains **NOT_RUN**.
- Owned *gitignored, local-only* artifacts: `core/target/issue273-next-oracle-01/oracle.json`
  SHA256 `402c2161b96f1dd7610b67eb14ed1c6cd53e1bbd8ad8893153c82c24a63ffb95`;
  `core/target/issue273-next-prepared-01/prepared.json` SHA256
  `56c9f18a57321d4aeb07d9e5d1ae8dc493bbd1892319bda607456f6897fce7a7`;
  `core/target/issue273-next-248-gate-01/receipt.json` SHA256
  `bd615dd872ed219367b758283a45fbf39a71e53291cc7d1749ca627d560bcf15`.
  The receipt directory's `SHA256SUMS` SHA256 is
  `e8dc0c9799b2a0af305ca1c143e576fcbd6afe50d327f160295fd1825e32c49f`.
  These strings are custody pointers, **not** published GitHub raw evidence.

## Previously ignored Linux mounted tests — both PASS

Ran each exact ignored integration test once with the pinned Linux readable
binary SHA256 `c5ea98dff7820578ad08264d234b9b8888949166acd9288f43e0a957ac927908`,
privileged `/dev/fuse`, isolated private ext4 Docker volume, `TMPDIR=/stage`,
`--cpus=2`, `LAYERFS_CONSTRUCTION_WORKERS=1` and the pinned rust image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
These in-process host-fixture cases did **not** substitute the native-service
routes. Their independently named test volumes were deleted after exit:

| Exact `--ignored --exact` selection | Result | Local-only receipt SHA256 |
| --- | --- | --- |
| `mounted_active_create_write_rename_unlink` | 1 PASS, 0 FAIL; owned volume deleted | `core/target/issue273-next-readable-mounted_active_create_write_rename_unlink/result.json`: `bcfcd13bf33ee0cdd621809432791cbcd72abf02c8b91ae027a250892e89915b` |
| `mounted_active_notifier_eio_keeps_published_receipt` | 1 PASS, 0 FAIL; owned volume deleted | `core/target/issue273-next-readable-mounted_active_notifier_eio_keeps_published_receipt/result.json`: `f7616fe6366bed0654c27175efa2ee2915516e2006c109a33c8f2b866761b263` |

## Final-source failed selections — no waiver or repair

Ran the original registered stage selections with unchanged quota, fixture
source and Linux stage test binary SHA256
`2cc9e2731b583960b87ce34610e4c1bd4451e11f7907b1988e892b16a4c61ef3`
(the test executable was built against the earlier proof source `05875fae8`;
no product/test source changed since that source). The `result.json` records
source `96d196dec` and the actual test-binary hash. Their checks remain
`NOT_RUN`; the runner marked both selection attempts `FAIL`:

- `stage_headroom`: `Backing(Allocate, StorageFull)` at `stage.rs:2344`,
  2 MiB deliberately occupied private quota; receipt SHA256
  `847c7e40964ddd6e41c2256c906b1117fc4579e2212542d0596047387aaca97c`
  in local-only `core/target/issue273-next-stage-headroom-01/result.json`.
- `stage_lowering`: `Backing(Acquire, StorageFull)` in external test helper
  `native_workspace.rs:574` while splicing a 64 MiB input under the unchanged
  64 MiB private quota, **before** the Stage lowering check; receipt SHA256
  `600737f01fca1a82ae92f49a009ee88b7bc8c7bc6a2deb69c797720f22587e67`
  in local-only `core/target/issue273-next-stage-lowering-01/result.json`.

Both failing attempts retain their original `FAIL` results and the captured
container logs/inspection; only the resources **named by those own results**
were forcibly removed afterward. That is harness cleanup, **not** successful
Workspace close. No quota/deadline was raised, no assertion deleted, no failed
attempt promoted. Explicit owner disposition of each is still **PENDING**;
requested in [#276 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5881598410).

## Independent remaining blockers

Four deterministic SDK lease falsifiers (deadline, exhausted response Budget,
held G1 lease across known C1/local C5 failure, uncertain checked release) are
**NOT_RUN**: the baseline live lease PASS, stage C1/C5 failure and uncertain
SaveFile proof do not combine into these routes. The earlier `c0ab3e5c8`
unregistered-token assertion failure's unexpected error code was not captured;
static source shows a well-formed unregistered token *should* map to Denied,
and a later labelled diagnostic and clean-source proof passed, but the
original FAIL's precise cause remains **UNRESOLVED**. No diagnostic pass
retroactively explains or erases that failure.

[#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) defers only the
Docker Desktop peak-reset observation; no independently verified common
private/VM/backend/device/host-cache and phase-local resource profile was
established, nor did the owner give a numeric-profile ruling. Explicit ruling
requested in [#276 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5881598410).
Functional-only #256/#270 deferrals remain NOT_PROVED. **NO MERGE, no release
admission, numeric INELIGIBLE**; no CI/preflight claim. No product code, limits,
frozen control, historical receipts or PR heads were changed by this report.

Production LOC for this docs-only commit: reference 65,417 -> 65,417, Core
70,653 -> 70,653, combined 136,070 -> 136,070 (delta +0). Method:
`python3 tools/production_loc.py --json --root <first-parent / staged-snapshot>`;
product scopes `crates/` and `core/crates/`, excluding docs, tests and tools.
