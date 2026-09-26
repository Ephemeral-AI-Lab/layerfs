# #245 Workspace shell brainstorm mini v1

> **Status:** Research; informative and not a product contract.
>
> Prospective exploratory benchmark specification for [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245).
> This document and [the registry](../../../benchmark/fs-bench-pro/registry/workspace-shell-brainstorm-mini-v1.json) are committed before runner implementation or sampling. The runner is not implemented and no row below is measured yet.

The future runner, fixtures and receipts are **verification/benchmark tests**
after the relevant product implementation. They are excluded from production
source and production LOC. This contract freezes the test workload; it does
not add a product implementation phase.

## Claim and public operation

The family `workspace_shell_brainstorm_mini`, version 1, covers **all ten
categories** in [the load-bearing brainstorm](LOAD_BEARING_CASES.md). Its 13
cells split fresh/append, printed/file logs, and metadata/content scans. These
are smaller, deterministic *shape probes*; they do not satisfy the full
4,097-run, 1,025-file, 500 MiB, 192/256 MiB or 64 MiB issue gates. The mini
may identify a failed public route, unexpected amplification or a promising
count-driven diagnostic. It supports no cold-cache speedup or release claim.

`operation_contract_id = workspace-api-exec-posix-fuse-brainstorm-mini-v1`;
`operation_surface = workspace-posix-fuse`;
`operation_entrypoint = WorkspaceApi::exec`;
`orchestration_executor = host Rust SDK example`;
`mutation_executor = /bin/sh -c inside the mounted Linux sandbox`;
`implementation_route = ordinary FUSE callbacks → Workspace private backing →
optional SaveFile/namespace Commit`;
`projection = Linux FUSE ordinary callbacks`.
One opaque UTF-8 command from the registry is passed unchanged to one public
Exec. There is one explicit Commit after a zero exit only when `commit=true`;
`print`, `find` and `grep` end at Exec and leave the Branch head unchanged.
The host driver never calls internal mutation methods. Fixture generation and
independent Store/History verification occur outside the operation timer.

## Exact prepared bytes and case selection

Use the existing pinned Alpine base image and the existing release
`benchmark_init`, `benchmark_shell` and independent `verify_shell` examples.
The runner makes one closed old Store/History master through public SDK Init,
fork and an untimed seed Commit. Every cell gets its own validated writable
byte copy of that master. All regular fixture files have mode 0644, all
directories 0755, and no symlinks or network inputs exist. The initial
`seed.txt` is `0\n`; the seed command changes its first byte to `1`, so the
post-seed old manifest has `1\n`.

The exact old tree, apart from parent directories implied by each path, is:

- `seed.txt = b"1\n"`; `large.bin = b"A" × 8 MiB`;
  `packages/big/existing.bin = b"B" × 64 KiB`;
  `logs/build.log = b"start\n"`;
  `package-lock.json = b'{"version":1}\n'`.
- `packages/docs/doc-{i:03d}.txt`, `i=0..127`: repeat the UTF-8 row
  `doc {i:03d} line\n` and truncate to exactly 32 KiB.
- `packages/old/subtree/file.txt = b"old file\n"`,
  `packages/old/subtree/nested/child.txt = b"old child\n"`,
  `packages/old/anchor.txt = b"old anchor\n"`,
  `packages/new/anchor.txt = b"new anchor\n"`,
  `packages/obsolete/dead.txt = b"remove me\n"`.
- `packages/{replacement,large,many}/keep.txt = b"keep\n"`.

The image-only `/fixtures/stress/` tree contains:

- `many/pkg-{p:02d}/file-{i:02d}.txt = b"pkg {p:02d} file {i:02d}\n"`
  for `p=0..12`, `i=0..9` (130 files).
- `large/file-{i:03d}.txt = b"large {i:03d}\n"` for `i=0..128`, plus
  `large/dist.txt = b"L" × 8 MiB`.
- `replacement/src-{i:02d}.txt = b"replacement {i:02d}\n"` for `i=0..15`;
  `file-v2 = b"updated file\n"`.
- `patch4k = b"X" × 4 KiB`, `replace2m = b"R" × 2 MiB`,
  `fresh8m = b"F" × 8 MiB`, `append1m = b"C" × 1 MiB`,
  `prefix1k = b"P" × 1 KiB`, `log2m = b"G" × 2 MiB`.
- `stdout.txt = b"O" × 16 KiB`, `stderr.txt = b"E" × 16 KiB`.

The registry owns **exact command bytes**, order, Commit choice and expected
route minima. The runner's pure `expected_files(case_id)` derives each complete
post-Commit manifest from these formulas; no case-specific product route exists.
The seeded old tree has 141 regular files. The old anchor keeps
`packages/old/` present after its inherited subtree moves. There is one
source arm per cell and
no baseline/candidate latency comparison in mini v1; a later comparison needs
a separately frozen arm contract and cache qualification.
The 13 IDs are `many-packages-mini-v1`, `large-package-mini-v1`,
`move-replace-mini-v1`, `remove-copy-mini-v1`, `tiny-edit-mini-v1`,
`existing-edit-mini-v1`, `fresh-file-mini-v1`, `append-mini-v1`,
`prepend-mini-v1`, `print-logs-mini-v1`, `file-log-mini-v1`,
`find-tree-mini-v1`, `grep-tree-mini-v1`. The `find` and `grep` cells use
independent master copies and mounts; the control never warms the grep cell.
The literal grep needle `LAYERFS_NEEDLE_ABSENT_245` is absent from every
generated file. A match (`grep` exit 0) or error (`grep` exit >1) makes the
shell command fail; only exit 1 is translated to success.

## Boundaries, metrics, and result rules

Time with a monotonic clock immediately before and after the single SDK Exec
call; time Commit separately when selected; record their enclosing operation
and external complete-command wall plus cleanup. Mount, fixture preparation,
clone, build, image creation, independent oracle and report are outside the
operation timer. Each performance command must finish within 15 s; only the
registered `large-package-mini-v1`, `fresh-file-mini-v1` and
`prepend-mini-v1` have prospective 25 s complete-command exceptions. The
independent verifier is bounded below 10 s. These harness limits do not change
the current 30 s product Exec timer or future #249 policy.

Keep one attempt per cell per source identity, fresh append-only outputs,
sealed release binaries, one construction worker, immutable image ID and
actual source/product/harness/fixture hashes. Never rerun an unchanged cell,
drop a failing row or change a size/deadline after seeing a result. Record raw
Exec result (exit, retained stdout/stderr lengths/hashes/truncation), exact
public-call and Commit counts, FUSE projection callback counts, Store/History
size before/after, private-backing status when the public Status exposes it,
spool/Store allocation when attributable, LFT1 host sampled CPU/RSS scope,
cleanup, and source/image/cache identities. Projection `write` aggregates
several mutations; it is **not** the exact FUSE WRITE syscall count. The
current public Status has READ callback counts but no READ-byte counter;
record actual READ bytes and physical Store reads as `UNAVAILABLE` until a
legitimate product counter or external trace exists, never infer them from
fixture bytes or `grep`'s intended work.
Use the unchanged sandbox configuration (currently 1 GiB shared Workspace
backing, 16 MiB host Workspace memory and 512 MiB container memory); no row
may raise these to fit. OOM, unexplained quota refusal, unclean teardown or
incorrect output is a functional failure. Numeric resource admission beyond
these configured bounds is not claimed by this mini.

The cold cache state of the cloned host Store, container FUSE path and
write-to-Commit transfer is currently uncontrolled. Every elapsed time is a
raw diagnostic with `performance_status=INELIGIBLE`; no speedup, cold grep
rate or numerical PASS follows. If a later cold contract is added, version
the scenario and enforce the same whole-input invalidation/residency check
independently for `find` and `grep`, without warming either measured path.
Correctness and cleanup remain separately checkable. After the timed run, the
independent verifier reopens Store and History, checks exact old/new Branch
head lineage, and traverses every path, type, mode, size and SHA-256; it
rejects extra/absent names. The three read-only cells require zero Commit,
unchanged head and no projected writes. The print cell also requires exactly
the retained first 8 KiB of each stream and both truncation flags; the file
log cell requires the complete committed file and empty Exec output. Other
cells require empty output, confirmed zero exit and the declared Commit.
Every failure is retained as `FAIL`, `INELIGIBLE` or `NOT_RUN` with its reason.

The planned runner will support `self-check`, `prepare --output FRESH`, and
`run --prepared PREPARED_JSON --output FRESH`; `run` executes all 13 cells in
registry order. Preparation reuses one validated master and release build;
execution clones it once per cell. `prepared.json`, build/image/seed logs and
manifests live under the fresh preparation directory. Each execution cell
gets its own `case.before`, raw driver/verifier output, `receipt.json` and
hash manifest; `campaign.json` lists all 13 statuses, including unrun rows.
No mini row is promoted into #248, #256,
#258 or #249 acceptance without its own full-size, prospectively committed
contract and public proof.
