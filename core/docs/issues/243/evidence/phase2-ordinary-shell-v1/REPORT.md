# #243 Phase 2: one-attempt ordinary Workspace shell baseline

**Status: functional FAIL for the mixed package refresh; three functional PASS observations with latency INELIGIBLE. The campaign is diagnostic and not admission-complete.** Source: `6d7421c6de85d2cce19c2810fc34e9b453a8030a` (tree `98285433d132d646be9fd0a4295256f21857977c`). This is the existing ordinary POSIX/FUSE implementation after the Phase 0 ioctl retirement. No Phase 3 optimization was made. The parent #232 56-shape generic-shell gate remains open.

The frozen selection is [PHASE1_CONTRACT.md](../../PHASE1_CONTRACT.md), committed at `83c2d6fb5` and corrected before preparation at `6d7421c6d`, with registry SHA-256 `1a7e1a3f7ea40ea14ed9f97865260c936df53601cfd5d0082c0db4041849cd3c`. The exact commands, byte recipe, immutable image fixtures, three prepared master hashes, release binary hashes, source and harness seals are in [PREPARED_PUBLIC.json](PREPARED_PUBLIC.json). Its inherited `source.contract_commit` names an older #232 runner constant; `source_commit` above and these #243 commits govern this attempt. The original unredacted `prepared.json` and immutable binaries/image context remain in this worktree's `benchmark-results/fs-bench-pro/issue243-shell-package-v1-prepared-02/`. Its SHA-256 is recorded in the public copy; only the local cursor key is redacted. The tool-free Linux image ID was `sha256:8405f00c4f318f2fffdedc5cec3ec2bb81f8b378bad45e76b881ecdcbbfcf71a`. Host: Darwin ARM64; sandbox: Linux aarch64. Every attempt exported `LAYERFS_CONSTRUCTION_WORKERS=1` and used a validated independent writable `shutil.copyfile` Store/History copy. The compiled source, image and fixtures were unchanged across the four rows.

The exact commands used were:

```sh
python3 core/benchmark/fs-bench-pro/shell_package.py self-check
python3 core/benchmark/fs-bench-pro/shell_package.py prepare --output benchmark-results/fs-bench-pro/issue243-shell-package-v1-prepared-02
python3 core/benchmark/fs-bench-pro/shell_package.py run --prepared benchmark-results/fs-bench-pro/issue243-shell-package-v1-prepared-02/prepared.json --output benchmark-results/fs-bench-pro/issue243-shell-package-v1-attempt-01
```

The first preparation path, `...-prepared-01`, is retained as a **pre-run failure**, not a baseline sample. Its seed returned definite `Busy` before a driver receipt because the new example requested `Server::owner()` before `Server::listen()`. Commit `6d7421c6d` fixed that setup order before the successful `...-prepared-02` seal and before any four-case attempt. The failed preparation's exact seed stdout/stderr and case are under [preparation-failure-01](preparation-failure-01/); the full build and Store files remain in the local path.

## Observed rows

| Case | Functional | Exec ms | Commit ms | Complete command ms / frozen limit | Independent verifier ms | Cleanup | Latency |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| [mixed refresh](mixed-refresh-v1/receipt.json) | **FAIL**: final root lockfile rename returned `EIO`; no Commit | 1,156.070 | — | 7,072.206 / 25, declared exception | 11.186, FAIL as no new head | PASS | INELIGIBLE |
| [4 KiB overwrite](overwrite-4k-v1/receipt.json) | PASS, full old/new tree | 17.845 | 24.186 | 909.021 / 15 | 166.071, PASS | PASS | INELIGIBLE |
| [16 one-byte writes](repeated-one-byte-v1/receipt.json) | PASS, full old/new tree | 208.898 | 33.895 | 1,119.839 / 15 | 95.473, PASS | PASS | INELIGIBLE |
| [failed command](failed-command-no-commit-v1/receipt.json) | PASS, definite nonzero Exec, no Commit, old head unchanged | 28.023 | — | 5,872.131 / 15 | 53.376, PASS | PASS | INELIGIBLE |

These are single raw observations in milliseconds, not medians or comparative speed claims. The complete-command wall starts when the host driver process is launched and includes SDK setup, sandbox/container lifecycle, mounted operation, Status, unmount, log capture and deletion. It excludes the prepared-master clone and separate verifier. The mixed and failed-command rows spent 5.511 s and 5.504 s respectively in reported cleanup/log capture after the operation; these are part of their complete command. The mixed row is a functional no-go despite staying under the declared 25 s exception. The verifier for that row correctly failed because no new Commit existed. A separately labelled **read-only post-attempt diagnostic**, without another Exec or performance sample, verified the old Commit/head and all 17 original paths, nine files and 1,114,501 bytes in 50.664 ms: [diagnostic receipt](mixed-refresh-v1/failure-diagnostic.json). The original failed verifier result remains unchanged.

The successful overwrite oracle traversed 18 paths, ten files and 11,600,261 bytes in each old and new tree; the repeated-write oracle traversed 18 paths, ten files and 1,180,037 bytes. The failed-command oracle traversed the complete original 17-path tree and confirmed the Branch head still equals its old Commit. These read-only checks used published Store/History, not the mounted Workspace.

## Route and count evidence

| Case | Status `write` aggregate | `rename` | `setattr` | `lookup` | `getattr` | `range_state` / `range_edit` | accepted range bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Mixed | 36 | 2 | 0 | 159 | 198 | 0 / 0 | 0 |
| 4 KiB | 1 | 0 | 0 | 1 | 4 | 0 / 0 | 0 |
| Repeated | 16 | 0 | 0 | 16 | 34 | 0 / 0 | 0 |
| Failure | 1 | 0 | 1 | 4 | 7 | 0 / 0 | 0 |

The mixed command's first temp-file rename (UI) preceded the failing second rename (`package-lock.json.next` over the existing root lockfile). The shell returned status 1 with `mv: can't rename 'package-lock.json.next': I/O error`; the driver issued no Commit. The FUSE adapter maps several Workspace errors to `EIO`, so the receipt identifies the failing syscall and location, **not** the exact Workspace error variant. This should be the first Phase 3 root-cause investigation. Status `write` also counts namespace mutations and cannot be presented as the number of FUSE WRITE callbacks. No operation-specific FUSE CREATE/MKDIR/UNLINK trace was available. The current ordinary command visibly exercises them, but this receipt cannot provide their exact counts. `LFS_PIECE_COUNT`, `LFS_PIECE_PAGES`, and `LFS_FINISH_SUBSTEP` lines were absent because this uninstrumented image did not enable those diagnostic environment flags; the values are **UNAVAILABLE**, never zero. [DERIVED.json](DERIVED.json) records field availability and the raw LFT1 events. The source has an explicit `ENOTTY` ioctl response and no active range dispatcher; the added privileged Linux mounted negative test was not run in this macOS Core test run, so runtime `ENOTTY` confirmation remains open.

## Raw CPU and RSS, with scope

The table is the `LFT1` sample for the host SDK driver's `sdk.shell_package` operation. CPU cells are sampled **user/system** deltas in milliseconds for a shared process; RSS is the highest sampled **host driver process** RSS in MiB. All windows had at least two samples and zero reported gaps, but their first sample preceded `opened_ns` and their last preceded `closed_ns`, so the CPU values are not exact operation or Exec/Commit CPU. They include concurrent SDK/monitor work, and the RSS values are neither a true peak nor container/cgroup memory. This is raw context only.

| Case | Host CPU user / system ms | Host sampled RSS MiB | Samples | Linux daemon Exec CPU user / system ms | Linux daemon Exec sampled RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Mixed | 179.594 / 226.422 | 14.703 | 117 | 200 / 150 | 36.734 |
| 4 KiB | 5.822 / 8.475 | 29.625 | 5 | 20 / 10 | 34.250 |
| Repeated | 26.067 / 31.475 | 32.953 | 25 | 50 / 30 | 34.293 |
| Failure | 5.708 / 7.002 | 11.453 | 4 | 20 / 10 | 34.430 |

The daemon's Commit LFT1 windows had three samples for 4 KiB and four for repeated writes, with sampled RSS 34.375 and 34.418 MiB. Both reported `cpu_shared_ns=[0,0]` at this sampling resolution; that is **not proof of zero Commit CPU**. All daemon resource observations are process-shared, sampled, and likewise not phase-exclusive or cgroup totals. The exact first/opened/last/closed timestamps, source, sample count and gap values are retained in [DERIVED.json](DERIVED.json) and raw `driver.stderr` for each case. No phase-qualified RSS or cold-cache latency PASS is claimed.

## Custody and next step

The local attempt directories retain each writable Store/History copy. The committed [raw row files](./), `campaign.json`, diagnostic, and [DERIVED.json](DERIVED.json) are append-only evidence; `derive.py` recomputes the summary from the raw receipts/stdout/stderr and records its own SHA-256. `SHA256SUMS` in each raw row was written at attempt completion; the mixed row's later read-only diagnostic has its separate `DIAGNOSTIC-SHA256SUMS`. The repository copy omits only the large mutable Store/History files and the local cursor key. No row was resampled, no workload, worker count, cache policy or limit was changed after collection.

The changed Core tree passed `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` at Phase 0 and, at the final source identity, `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`, `python3 core/tools/check_product_boundary.py`, and `python3 -m unittest discover -s core/tools -p 'test_*.py'`. The locked release SDK driver, independent verifier and Linux daemon built for the attempt; the three prepared masters and three functional cases exercised them live. The privileged mounted `ENOTTY` regression was compiled but ignored on the macOS host and was **not** executed. No CI or aggregate preflight ran.

The raw receipt schema omitted several mandatory Core fields from [benchmark rules §12](../../../../../../docs/general/benchmark_rules.md), including explicit source arm/treatment, field provenance statuses, failure class, and campaign receipt-completeness status. Some are inferable from the prospective contract, but they cannot be written into a historical raw receipt after collection or treated as measured zeros. Therefore **receipt completeness and admission eligibility are FAIL**, while the raw functional observations and one-attempt custody remain useful diagnostics. The absent piece/Commit counters and uncontrolled cache independently block an admission claim. No repaired or relabelled PASS receipt is issued.

Phase 3 should identify which ordinary `rename` path produced the root lockfile `EIO` from the retained source and a labelled count/error diagnostic, then address it without altering the frozen baseline. It should separately obtain operation-specific callback and piece/Commit counters and a complete prospective receipt schema, preserving field availability until measured. The 56 #232 shapes, larger replay and piece-index bounds, and cold-cache eligibility remain open.
