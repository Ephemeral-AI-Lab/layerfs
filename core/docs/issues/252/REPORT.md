# Issue #252 — generic file save and legacy route removal

**Product proof source:** `0560a0cd558cc55bf08efcd5f7b38de204ef690f` (clean tree
`db9b0313f935ad37e23f80d75d6ed29532ecc973`, 2026-09-26). The
[evidence manifest](evidence/SHA256SUMS) hashes copied receipts and logs;
original local attempts under `benchmark-results/issue252-*` remain unchanged.
This report does not turn cache-ineligible functional rows into latency PASS.

## Cutover

The public mutation route is `WorkspaceApi::exec(command)` → ordinary Linux
FUSE `WRITE`/`SETATTR`/`CREATE`/`RENAME` → explicit Workspace Commit. Commit
walks the captured final `Base`/`Local`/`Zero` sequence, streams fixed 24-byte
extent records and non-base bytes through Bridge `SaveFile` opcode 20, and saves
one canonical C1 root before C5 publishes one Branch head. The Service validates
order, lengths, base bounds, exact input and zero bytes before opening a C2 save.
It keeps at most a 64 KiB input window and 64 KiB retained replacement in RAM;
larger input and derived C1 edits use bounded temporary disk spools. The 4 GiB
logical-file bound and separate 8 GiB wire and spool budgets remain explicit;
there is no fixed final-run admission count.

The current product has no `EditFile`, `ConstructFile` or
`UpdatePreparedFilesystem` operation/codec/handler, direct Workspace range-edit
entrypoint, projected range wire field, FUSE ioctl callback, or active edit-tool
selection. The pinned `fuser` default refuses unimplemented ioctl requests with
`ENOSYS`. `InitLayerStack`, `AddLayer` and `DiscardStage` stay in the history
grammar because the active Linux history fixture
`core/crates/layerfs-daemon/tests/history_route.py` uses them. Historical
receipts and source commits were not relabelled.

The C1 canonical builder remains internal. The release test
`final_extents_match_the_sealed_reference_root_and_partition` reproduces the
independent Stage 3–4 oracle's file root
`333d54974516c788dc731ee2e7c7522d0ff56a6982889b66e3caef5d469c310b`,
mapping root
`94f0c9bd755938ca564036ef70fdf2a7991a9693230fe16bfd28e185f2a6c1b1`,
and two branch children. The direct service tests also cover old-root readback,
zero/deletion/no-op, malformed order and length, authenticated transport and
a 10 MiB final replacement. The shared Workspace cursor correction at
`cad3edb3b` selects a nonzero offset by cumulative branch-child length; the
focused middle-child test and the final Linux sequence suite cover that path.

## Public ordinary-shell proof

The [closed prepared masters](evidence/final-shell/prepared.json) were made once
at the product proof source. Each row received an independent writable byte
copy. Product seal:
`68ae80f383e3d8ac8aae19f557cd2403e4ae7a986ec2534e63c69b48f74f5b0c`;
harness seal:
`cb595b477fe9ceee594c448684b3f7fa129fbcbebde14b650fbc6521b749f438`;
Linux image:
`sha256:068b0f2c75e934a1ccb3822fb6d8202ed1bf7a5e948b5f0bc52e80c0f0c6fe47`.
The release shell driver and independent verifier hashes are in the prepared
record. [The final run report](evidence/final-shell/report.json) binds all eight
one-attempt rows.

| Selection | Complete command / limit | Functional | Independent verifier | Cleanup | Cache/performance |
| --- | ---: | --- | --- | --- | --- |
| [Mixed package refresh](evidence/final-shell/mixed-refresh-v1/receipt.json) | 2.225 / 25 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB overwrite](evidence/final-shell/overwrite-4k-v1/receipt.json) | 0.908 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Sixteen small writes](evidence/final-shell/repeated-one-byte-v1/receipt.json) | 1.108 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Failed command, no Commit](evidence/final-shell/failed-command-no-commit-v1/receipt.json) | 5.854 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB append](evidence/final-shell/append-10mib-v1/receipt.json) | 0.917 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB shrink and growth](evidence/final-shell/resize-10mib-v1/receipt.json) | 0.911 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Temporary-file replacement](evidence/final-shell/temp-replace-v1/receipt.json) | 0.911 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [4 KiB prepend to 10 MiB](evidence/final-shell/prepend-4k-10mib-v2/receipt.json) | 4.542 / 15 s | PASS | PASS | PASS | INELIGIBLE |

The prepend used 80 ordinary FUSE reads, 81 writes and one size SETATTR. Its
Exec took 3.443 s and Commit 0.169 s; the verifier took 0.134 s separately.
The [verifier output](evidence/final-shell/prepend-4k-10mib-v2/verifier.stdout)
reopened the Store/history, checked all 18 old and new paths and both Commit
heads, and checked 11,604,357 new-tree bytes. The old head remains readable.
These timings are observations only: host Store and container FUSE backing
cache state was uncontrolled, so every shell performance row is `INELIGIBLE`.

## Separated-run and Linux proof

The owner stopped further 4,097-run work and accepted **1,024** separated
final runs for this issue. The earlier 4,097 authenticated Bridge → Service →
C1 focused test had passed during implementation; it is now ignored to prevent
another run, and no sealed log is claimed for it. The retained 4,097 Workspace
diagnostics remain FAIL at their original 60 s limits. No result from them is
promoted to a PASS. The [changed-source 1,024-run receipt](evidence/workspace-1024-pass/result.json)
and [test output](evidence/workspace-1024-pass/test.stdout) show native Workspace
→ Bridge → Service → C1 Commit, exact changed-span bytes and one saved root,
with functional PASS and cleanup PASS in 40.715 s of the unchanged
60 s functional budget. It retained 1,024 accounted payload records and no
failed payloads. This is not the >65,535-run or full Commit-phase proof assigned
to #248.

At the final product source, the [release Linux sequence suite](evidence/checks/0560a0cd5-linux-pieces-sequence-01.log)
passed all 23 tests, including nonzero middle-child cursor seeks and a 1,024-run
fold. The [mounted positional-write route](evidence/linux-write-pass/result.json)
passed its exact-byte, alias and incremental-Commit checks in 3.901 s; the
[mounted resize route](evidence/linux-resize-pass/result.json) passed shrink,
reextend, alias and incremental-Commit checks in 1.060 s. Both cleaned their
containers and volumes. The [locked release Linux build](evidence/checks/0560a0cd5-linux-fuse-workspace-build-02.log)
compiled all Workspace and FUSE test targets.

## Core gates and source size

On clean product source `0560a0cd5`, the [locked release Core test log](evidence/checks/0560a0cd5-core-test-01.log)
records **717 passed, 0 failed, 3 ignored and 2 explicitly filtered** across
169 test/doc-test binaries. The two filtered tests were the owner-excluded
4,097-run service case and the unrelated 4,097-entry native import case.
[Warning-denying release Clippy](evidence/checks/0560a0cd5-core-clippy-01.log),
[formatting](evidence/checks/0560a0cd5-core-fmt-01.log),
[production boundary](evidence/checks/0560a0cd5-boundary-01.log),
[guard self-tests](evidence/checks/0560a0cd5-guard-tests-01.log),
[release examples](evidence/checks/0560a0cd5-core-examples-01.log),
[shell self-check](evidence/checks/0560a0cd5-shell-self-check-01.log), and
[harness tests](evidence/checks/0560a0cd5-harness-tests-01.log) passed. No
aggregate preflight or CI run was claimed.

Each commit message records exact first-parent/staged production LOC by
`core/tools/production_loc.py --root <snapshot> --json`. Across this branch's
product changes, combined production LOC is **125,403 → 125,023 (−380)**:
Core **56,675 → 56,295 (−380)** and legacy reference **68,728 unchanged**.
Test, harness and documentation-only commits record delta 0. The old and
replacement implementations still coexist; removal of an active test harness
does not count as a product LOC reduction.

## Retained nonpassing evidence and scope limits

| Original attempt | Recorded outcome | Resolution or disposition |
| --- | --- | --- |
| [Initial fixture preparation](evidence/fixture-prep-fail/result.json) | FAIL before measurement; wrong host/target daemon binary | [Second preparation](evidence/fixture-prep-pass/result.json) used a validated closed 64 MiB master; initial failure retained. |
| [4,097 Workspace frontier](evidence/4097-frontier-fail/result.json) and [count](evidence/4097-count-diagnostic/result.json), [phase](evidence/4097-phase-diagnostic/result.json), [shared-payload](evidence/4097-shared-diagnostic/result.json) diagnostics | FAIL at 60 s; progress reached 1,536 writes at about 53–54 s in the latter diagnostics | No timeout, worker, quota or cache policy was relaxed. User directed no further 4,097 runs; 1,024 changed-source functional proof above is the accepted scope. |
| [Prepend v1](evidence/shell-prepend-v1-fail/receipt.json) | FAIL/timeout at 15.005 s; verifier NOT_RUN; cleanup FAIL | BusyBox `dd` split 128 KiB input into 4 KiB writes. Separate v2 scenario kept the in-place 10 MiB shift but used 128 KiB output writes and byte-based seek. The v1 receipt is unchanged; its owned Docker container/volume were manually removed after timeout. |
| [1,024 pre-fix Workspace](evidence/workspace-1024-fail/result.json) | FAIL at FileSave: `Preparing` / `KnownBeforeCommit` / `Io`; no PASS check | `metadata_cursor::seek` did not accumulate preceding child lengths on re-seek. Commit `cad3edb3b` fixes the shared cursor; changed-source attempt passes. Owned Docker resources were manually removed after the failed test. |
| [Retired ioctl test setup](evidence/ioctl-test-setup-fail/result.json) | FAIL at clean close (`Busy`) after setup had dirtied the file | Test setup was corrected; owner then directed permanent removal of ioctl. Commit `0560a0cd5` removes the production callback and active selection. The failed receipt remains. |
| [Early Core test and Clippy logs](evidence/prior-checks/41f2b902a-core-test-01.log) | FAIL on stale permission/status expectations, a pre-save admission assumption and test-only Clippy warnings | Corrected before the final green source; all original logs are retained under `evidence/prior-checks/`. |

Other than the owner-scoped 1,024-run ruling, #248 still owns the broader
>65,535-run, page-visit complexity, writer-progress across every Commit phase,
and full range-COW qualification. #249 stays downstream of #248. The #252
main-branch completion gate requires this cutover and its accepted #245 E/F
prerequisites to be merged; the raw receipts above are proof of the branch
source, not a claim that `main` is already verified.
