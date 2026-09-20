# Pair 2 remediation validation and qualification

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Product source: `f2021367e11f6433d1d9e055198cac5ad280f6c9`.
Additional disjoint-edit external test: `8a3fc0a774d07bf03e83b2d10f6a6c213030a556`.
Both have identical production source; the latter also pins architecture paper 16
to the repaired product. This record adds evidence only. The original review at
92e56635 and its receipts remain unchanged.

Code-remediation disposition: R01–R14 repaired with external regressions; R10's
stale/provenance ordering is resolved according to the governing specification,
not the old observed Integrity error. R15 remains a pre-existing fixed-width
expect, unchanged. See [implementation traceability](implementation.md) and
[the independent staged-source review](independent-review-final.md). The reviewed
product diff SHA-256 exactly matches the committed product diff:
`6e10331a82a57cfa07b776054e66780f7fe61f39c55282d53166b2d446ec8e2c`.

## Explicit checks

Commands ran from `core/`, using toolchain 1.85.1, offline/locked dependencies and
`CARGO_TARGET_DIR=/Users/yifanxu/.codex/worktrees/b995/layerfs/target/remediation-core`.
`LAYERFS_CONSTRUCTION_WORKERS=1` was exported. No aggregate gate, CI or preflight
was run. Full tests include the standard external tests and doc-test discovery.

| Command | Repaired product result |
| --- | --- |
| `cargo +1.85.1 test --offline --locked --workspace` | f2021367: 601 passed; 105 executable/doc-test result groups, no failures. The disjoint-test head result is appended below. |
| `cargo +1.85.1 build --offline --locked --workspace --examples` | PASS |
| `cargo +1.85.1 fmt --manifest-path Cargo.toml --all -- --check` | PASS |
| `cargo +1.85.1 clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS |
| `cargo +1.85.1 build --offline --locked -p layerfs-history --no-default-features` | PASS; contract separation, not another working persistence backend |
| `python3 tools/check_product_boundary.py` | PASS, 194 production files |
| `python3 -m unittest discover -s tools -p 'test_*.py'` | PASS, six tests |
| `cargo +1.85.1 build --offline --locked --workspace --bins` | PASS, matched service/daemon binaries |

The [command record](raw/validation-commands.json), [binary/source identities](raw/final-identities.json),
[full test output](raw/final-cargo-test.log) and other `raw/final-*` logs are new
runs by this task. They are not the original author's or reviewer's runs. Source
and worktree were clean at f2021367 before/after these checks; later pending
handoff files do not enter product source. See the appended test-head record for
checks at 8a3fc0a7.

## Real routes and substitution

- Host: `python3 crates/layerfs-daemon/tests/history_route.py --output <fresh host-route-f2021367>`;
  **14 cases PASS** through the production daemon/service. See the
  [host receipt](raw/host-history-route.json) and [output](raw/final-host-route.log).
- Linux: installed aarch64-musl target, Zig 0.16.0 and Docker 29.5.2 were available.
  `cargo +1.85.1 zigbuild --offline --locked --target aarch64-unknown-linux-musl -p layerfs-daemon`
  built the current daemon into the separate owned `target/remediation-linux`.
  The repository scratch Dockerfile packaged that exact executable; no dependency
  was changed. The real `--image layerfs-history-remediation:f2021367` driver run
  passed **14 cases**. This is an ARM64 Linux Docker daemon talking to the matched
  macOS host service, as the existing route driver defines; it is not a Linux
  service/C2 persistence qualification. [Receipt](raw/linux-history-route.json),
  [capabilities](raw/linux-capabilities.json), [build](raw/linux-build.log),
  [image identity](raw/linux-image.json), [output](raw/final-linux-route.log).
- Linux image ID:
  `sha256:2c82dbcfcb176e91dd3fb8336c0d830c4a8dfcfd2c1a845b56ddf09b82c6a157`.
  Linux binary SHA-256:
  `ef3a3f7147a21f1af7eeed1315505b662e42d092ad2df947e97006705cec525c`.
- Existing C1/C2 substitution harness: all **four arms**, examples, and **four
  persisted-data cross checks passed**, and canonical identity lines matched.
  The five consumer files were unchanged. The scratch driver changed only owned
  paths, explicit pinned archive identity and mandatory offline/locked flags;
  [patch](raw/substitution-driver.patch), [identities](raw/substitution-plan.json),
  [matrix](raw/substitution-matrix.json), [output](raw/substitution.log).
  Baseline was the imported plan's product tree (35740836), candidate f2021367.
  C1/C2 source is identical across those revisions; this demonstrates preserved
  component consumption, not a new optimized substitution or full H14 history
  consumer proof. No timing value is used as a performance claim.

Every build target and image context is under this task's worktree. Route output
directories were fresh; no historical receipt or image was relabeled. The route
processes were reaped and no test container remained. Keys were generated only
by the external route driver and were not recorded.

## H01–H14 disposition

| Case | Status and exact evidence / remaining gap |
| --- | --- |
| H01 | PASS at bounded service scope: real Empty/manifest init, nested/empty/interleaved directories, file/symlink inputs, invalid/dangling/wrong-role refusals; direct tests and both routes. |
| H02 | PASS: Layer and historical-Commit forks share ancestry and selected Commit base; catalog lifecycle/conditional tests and explicit >4,096 membership Capacity regression. |
| H03 | PASS: separate save/stage/Commit/Layer boundaries and logical readback through real host/Linux daemon routes; immutable record/parent assertions. |
| H04 | **UNVERIFIED for history overlap/reverse failure.** Existing service ConstructFile overlap/third-admission test and C2 multi-writer tests passed, but that service test has two successful content operations. It does not observe real history C2-save overlap followed by B completion and A failure with B's stage surviving. Existing history driver is sequential; delayed metadata/END_INPUT is not a C2 lifetime observation. No hook/delay was added to fabricate it. |
| H05 | PASS: overlapping and disjoint same-Branch stages retain the exact stale loser, including native typed context; multiple no-change stages succeed. The disjoint namespace case is added in 8a3fc0a7. |
| H06 | PARTIAL: independent Branch Commit and same-stack expected-head winner tests pass. **Independent-stack upload overlap remains NOT_RUN**: no real overlapping prepared-update upload/save lifetime observation is supplied by the existing history driver. Source releases C5 before C1/C2; that source fact is not substituted for the missing schedule. |
| H07 | PASS: delayed commit/discard token cannot consume replacement stage, exact-source publication stays UpToDate after later advancement, and immutable provenance is revalidated. |
| H08 | PARTIAL: source review confirms exact operation finish before stage; the external SQL statement-error probe confirms unknown quarantine/RAII behavior; public-provider refusal tests confirm composite acknowledged stage context. **Real I/O/connection-loss at each C2 finish, stage insert, Commit and Layer boundary, and identical-root A-failed/unknown versus B-success schedules remain NOT_RUN.** SQL corruption/provider refusal is not relabeled as those observations. No product fault hook, synthetic sleep, guessed rollback/discard, retry or alternate algorithm was introduced. |
| H09 | PASS within selected continuity envelope: new-connection stage inspection, read-only reopen, missing/wrong cursor-capability refusal and mutation refusal without continuing writable authority. No writable restart or crash-recovery claim. |
| H10 | PASS within that envelope: real terminal allocator high-water fixtures, checked last accepted/first refused range, unchanged refusal state, concurrent admitted ranges, and no refund after failure/discard. Read-only reopening never acquires allocation authority; no serial scanning/recycling. |
| H11 | PASS: identity/profile/opcode/permission/role/cursor/count checks, exact terminal matching, metadata bounds and legacy mask31 refusal; native mutation ResultData leaves output untouched. |
| H12 | PASS: metadata-only command test observes unchanged C2 file identity; source has no C2 save or re-encoding in those commands. Scope/profile mismatches refuse explicitly. |
| H13 | PASS for bounded codecs/cursors, no-native contract build, macOS host route and ARM64 Linux daemon route. No Windows/WASM/cloud or Linux service persistence claim. |
| H14 | **PARTIAL / full history-consumer substitution UNVERIFIED.** Existing unchanged C1/C2 consumer matrix passed as described above. It exercises pipeline/filesystem consumers, not an unchanged full history consumer across distinct supported C1-only/C2-only/combined implementation revisions. No waiver is inferred. |

H04, the remaining H06/H08 schedules and full H14 keep M4/full acceptance
incomplete. Independent source review for M5 was obtained; review availability
is not the blocker. The remediation is prepared for review, not release or merge.
#210 and its related parent/component issues remain open.

## Failure and diagnostic retention

`raw/baseline-probes.log` records 13 reproductions before edits. Other preliminary
logs retain all observed failed attempts: non-exhaustive semantic classification
while editing; a SQLite defensive-mode refusal in the disposable schema fixture;
old contextual-error expectations; the native three-byte frame cap; and the
Clippy type-complexity lint. Each was corrected and rerun for a stated source/
fixture change. Later passing checks do not erase those failures. Preliminary
root-invoked Cargo commands did not load core's cwd-based target flags; final
qualification commands ran from core with its checked-in configuration.

There was no performance campaign or measurement claim, no third-party registry
edit, fork/vendor/patch, no fsync/WAL, no cache/buffer/worker/timeout retuning and
no unsupported durability or restart claim.

## Commit LOC accounting

| Commit | Core | Reference | Combined |
| --- | --- | --- | --- |
| Imported plan 35740836 | 30248 → 30248 (+0) | 65417 → 65417 (+0) | 95665 → 95665 (+0) |
| Remediation f2021367 | 30248 → 30927 (+679) | 65417 → 65417 (+0) | 95665 → 96344 (+679) |
| Disjoint test / source pin 8a3fc0a7 | 30927 → 30927 (+0) | 65417 → 65417 (+0) | 96344 → 96344 (+0) |
| This evidence-only commit | 30927 → 30927 (+0) | 65417 → 65417 (+0) | 96344 → 96344 (+0) |

Each new commit compares its first parent and final staged source with
`tools/production_loc.py --root <git-archive snapshot> --detail --json`; committed
tree equality is confirmed. Counter SHA-256:
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
Scope is first-party core/reference Rust plus runtime SQL; comments/blanks,
legacy inline test code, tests, docs, tools, manifests and artifacts are excluded.
No source relocation, duplication scope change or reference retirement occurred.
The imported plan's product subtree IDs equal its parent's exactly; the recount
matches the reviewed 30,248 + 65,417 totals. Raw snapshot counts and commit-tree
records are retained beside the other evidence.

## Final test-head check record

At `8a3fc0a774d07bf03e83b2d10f6a6c213030a556`, all eight explicit core checks
above were rerun after adding the disjoint-edit regression. **602 tests passed**
across 105 result groups; examples, fmt, warning-denying Clippy, no-native build,
boundary guard (194 files), six Python self-tests and workspace bins all passed.
[Commands and head identities](raw/test-head-validation-commands.json),
[test log](raw/test-head-cargo-test.log), and the other `raw/test-head-*` logs
retain the results. Only pending evidence files were untracked. Tracked source
and tests were unchanged before/after checks.

Host binary hashes after those builds exactly equal the f2021367 route-run
hashes, and the product-only Git diff hash remains the independently reviewed
value. The successful host/Linux route receipts remain labeled with their actual
f2021367 source and image identities; this report does not relabel them as
new samples. This final evidence-only commit changes no product or test input.
