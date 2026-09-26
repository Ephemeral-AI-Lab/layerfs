# #252 main-branch proof — 2026-09-26

**Verified main commit:** [`cab07b60`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/cab07b60ef69bc70323fb01ba23938b00daa1623),
merge of [PR #253](https://github.com/Ephemeral-AI-Lab/layerfs/pull/253).
The checkout was clean at source tree
`90965958d12b6ce1f8e1f372a236989e69eec92d`, exactly the tree of the
tested cutover branch. The [main proof manifest](evidence/SHA256SUMS) hashes
byte-identical copies of the local receipts and logs. The branch's earlier
passing **and failing** attempts remain in the [#252 report](../REPORT.md).

The merge commit's exact first-parent production LOC comparison is combined
**120,440 → 125,023 (+4,583)**, Core **51,712 → 56,295 (+4,583)**, legacy
reference **68,728 unchanged**. Both snapshots used the same
`core/tools/production_loc.py --root <snapshot> --json` method; test, tooling,
benchmark, documentation and legacy inline-test lines are excluded. The growth
is replacement-product migration scope, not an algorithmic size claim.

## Final-source checks

| Check | Result and retained log |
| --- | --- |
| `cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked --workspace --no-fail-fast -- --skip authenticated_generic_save_accepts_4097_separated_final_runs --skip native_directory_import_spills_ordering_without_an_entry_cap` | [PASS](evidence/checks/cab07b60-core-test.log): 717 passed, 0 failed, 3 ignored, 2 filtered; the two 4,097 selections were excluded per owner direction. |
| `cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings` | [PASS](evidence/checks/cab07b60-clippy.log) |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | [PASS](evidence/checks/cab07b60-fmt.log) |
| `python3 core/tools/check_product_boundary.py` and its `unittest` discovery | [PASS](evidence/checks/cab07b60-boundary.log), [9 tests PASS](evidence/checks/cab07b60-guard-tests.log) |
| Locked release Core examples; focused shell harness tests | [PASS](evidence/checks/cab07b60-examples.log), [9 tests PASS](evidence/checks/cab07b60-harness-tests.log) |
| Locked release aarch64 Linux Workspace/FUSE test build; actual Linux sequence suite | [PASS](evidence/checks/cab07b60-linux-build.log), [23 tests PASS](evidence/checks/cab07b60-linux-pieces-sequence.log) |

The owner accepted a changed-source **1,024** separated-final-run Workspace
proof for #252 and directed no further 4,097 run. Its [passing native
receipt](../evidence/workspace-1024-pass/result.json) is from the same Workspace,
Bridge, Service and C1 code; the later changes remove the FUSE ioctl callback
and add documentation. The main Linux cursor suite above independently covers
the middle-child offset fix. #248 retains larger-run and full Commit-phase
qualification; no missing gate is relabelled PASS.

## Main public shell receipts

The [prepared master record](evidence/shell/prepared.json) pins clean source
`cab07b60`, product seal
`68ae80f383e3d8ac8aae19f557cd2403e4ae7a986ec2534e63c69b48f74f5b0c`,
harness seal
`cb595b477fe9ceee594c448684b3f7fa129fbcbebde14b650fbc6521b749f438`,
locked **release** binaries and Linux image
`sha256:0729600354797e8463265167e1a275803844d0e82458d8963f7ab721e0629df4`.
Each selection received its own independent byte copy of the closed master;
one attempt was made per case. [The group report](evidence/shell/report.json)
lists all eight, including the failed-command/no-Commit selection.

| Selection | Complete command / limit | Functional | Verifier | Cleanup | Performance |
| --- | ---: | --- | --- | --- | --- |
| [Mixed refresh](evidence/shell/mixed-refresh-v1/receipt.json) | 2.323 / 25 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB overwrite](evidence/shell/overwrite-4k-v1/receipt.json) | 0.929 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Sixteen one-byte writes](evidence/shell/repeated-one-byte-v1/receipt.json) | 1.091 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Failed command, no Commit](evidence/shell/failed-command-no-commit-v1/receipt.json) | 5.848 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB append](evidence/shell/append-10mib-v1/receipt.json) | 0.896 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [10 MiB shrink and growth](evidence/shell/resize-10mib-v1/receipt.json) | 0.972 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [Temporary-file replacement](evidence/shell/temp-replace-v1/receipt.json) | 0.915 / 15 s | PASS | PASS | PASS | INELIGIBLE |
| [4 KiB prepend to 10 MiB](evidence/shell/prepend-4k-10mib-v2/receipt.json) | 4.354 / 15 s | PASS | PASS | PASS | INELIGIBLE |

The prepend issued 80 ordinary FUSE reads, 81 writes and one size SETATTR;
its Commit took 0.155 s. The [independent verifier
output](evidence/shell/prepend-4k-10mib-v2/verifier.stdout) confirms both
Branch heads, exact old and new paths, modes, lengths and SHA-256s, with
11,604,357 new-tree bytes. There is no LayerFS ioctl callback or active
edit-tool selector. No Docker container remained after the main campaign.
Cache residency of the host Store and FUSE backing was not controlled, so
the raw wall times are functional observations only and no cold latency
admission PASS is claimed.

The #245 E/F prerequisite source, #245 research and accepted Exec progress
fix arrived in `main` through PR #253. [PR #247](https://github.com/Ephemeral-AI-Lab/layerfs/pull/247)
is recorded as merged through that history. [PR #250](https://github.com/Ephemeral-AI-Lab/layerfs/pull/250)
and [PR #251](https://github.com/Ephemeral-AI-Lab/layerfs/pull/251) were closed
as superseded: the former's commits are ancestors of `main`; the latter's
accepted behavior was cherry-picked as `dec2c395d`, while its failed
diagnostic changes were not promoted. [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248)
now describes the generic `SaveFile` prerequisite and remains open before #249.
