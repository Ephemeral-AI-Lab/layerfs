# Root reference recovery catalogue (R9b)

> **Status:** Appended recovery catalogue; not release evidence.

This directory is appended beside the historical receipts. It records, for every
tracked file outside the excluded trees that names the root reference (`crates/`
at the repository root, and `core/reference-tests`), where that reference can be
recovered after the trees are retired. It deletes nothing, rewrites no receipt and
changes no verdict; `original_verdict_touched` is `false` in every row. The scope,
the two classes, the pin search order and the four recovery bases are lead decisions
recorded for owner review, not owner rulings. "Choices made by the generator" lists
what no lead decision covers.

## Files

| File | Content | SHA-256 |
| --- | --- | --- |
| `generate.py` | generator (this README is its output) | `1ac2782b2c8b54d990628087fa1f87318fcb1612d7eeb747c997c07a5615bb42` |
| `reference-recovery-catalogue.jsonl` | header line + 2347 rows | `2eb396bf78f390d2c2cfa4f9ce3e244da23c903b7a771bff7dc024ba39da9875` |
| `missing-pins.txt` | 7 distinct `MISSING_OBJECT` pin values | `2f1c26653fde9b35a2857f11c73de8d5bbdcc39ef38d0d51e7c386ce839dd19b` |
| `selfcheck.py`, `selfcheck.txt` | reproducibility and blob-identity check and its output | not embedded here; see the hand-back |

## Header

| Item | Value |
| --- | --- |
| catalogue source commit (every blob is read at this commit) | `ea812d3f8283801f7957a7c09c84a67ae6cb9e86` |
| recovery commit | `f8a0a5ff1cd5f93b90bf16f964705b4c7d773615` (catalogue_commit_ancestry_only) |
| reference tree (`<recovery>:crates`) | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| reference-tests tree (`<recovery>:core/reference-tests`) | `3a7fcdcc6f8d1610c896b9e2a2f89d2a0297f027` |
| pushed equivalent `origin/main` | `f96d97651be5299f153ccde2bc8d921dd58807ad`; `crates` `498dd1917812ae90efb8841f57e22bfc284e96fb`; `core/reference-tests` `3a7fcdcc6f8d1610c896b9e2a2f89d2a0297f027`; same trees: **yes** |
| tag `v0.1.6` | commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; `crates` `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`; reference tree differs from it by 5 A |

`origin/main` is a local remote-tracking ref; this script performs no network access and
does not show that the remote still holds that commit. Reachability labels in rows
(`remote_tracking_or_tag`, `catalogue_commit_ancestry_only`, `other_local_object`)
describe the local ref state at generation time in the same sense.

Paths by which the reference tree differs from tag `v0.1.6`:

- `A crates/layerfs-content/examples/rope_edit_oracle.rs`
- `A crates/layerfs-content/examples/rope_edit_timing.rs`
- `A crates/layerfs-content/examples/stage5_component_reference.rs`
- `A crates/layerfs-content/tests/stage5_reference_fixtures.rs`
- `A crates/layerfs-workspace/tests/deep_history_diagnostic.rs`

## Method

`python3 generate.py --output-dir <dir>` (optional `--rev`, `--recovery-commit`,
`--exclude-pin-key`). It runs git plumbing and text processing only: `ls-tree`,
one `cat-file --batch` stream over every in-scope blob, `cat-file --batch-check`,
one `log --diff-filter=A` pass, `rev-list`, `rev-parse`, `diff-tree`. It does not read
the working tree or the index, so uncommitted edits cannot change the result.

1. **Scope.** Every blob tracked at the catalogue source commit outside `crates/`, `core/crates/`, `core/vendor/`, `core/reference-tests/` and
   outside this directory: 54552 blobs (49739 text, 4813 binary; binary means a NUL byte
   in the first 8,000 bytes).
2. **Name sets, from git only.** Root package directories ever present under
   `crates/` (26) and core package directories ever present under `core/crates/`
   (23); every path ever added under either tree; the recovery tree's paths.
   Root-only bare names in the recovery tree: `layerfs-cli`, `layerfs-layerstack-store`, `layerfs-materialization`, `layerfs-monitor`, `layerfs-workspace-core`. Historical root-only bare names:
   `layerfs-branch-store`, `layerfs-core`, `layerfs-driver`, `layerfs-durable-store`, `layerfs-engine`, `layerfs-layer-store`, `layerfs-mount`, `layerfs-os`, `layerfs-stack-store`, `layerfs-storage-core`, `layerfs-sync`, `layerfs-tui`, `layerfs-vfs`, `layerfs-working-store`.
3. **Signals.** Each `crates/layerfs-<pkg>[/path]` mention is resolved: a `core/`
   prefix is core; a Markdown link (and, outside `core/docs`, `docs`,
   `release-notes`, a `../` path) is resolved against the file's directory; otherwise
   a root-only directory name is the reference, a core-only one is core, and a
   shared name (`content`, `daemon`, `fuse`, `service`, `storage`, `workspace`) is decided by whether the named path ever existed
   under only one of the two trees. A path that existed under both or neither is
   `ambiguous_shared_path`. `v0.1.6` immediately between a package name and
   ` (path)` (cargo's `name vX (path)` form) is not a signal. Binary blobs are
   searched for path and name signals only.
4. **Class.** `STRONG` if any of: `root_link`, `root_only_package`, `root_only_package_historical`, `root_path`, `root_path_historical`, `root_only_name`, `fs-benchmark-pro`, `layerfs-eval`, `reference-tests`. Otherwise `REVIEW` if any of: `ambiguous_shared_path`, `unknown_package_path`, `historical_root_only_name`, `v0.1.6_substantive`, `reference_arm`, `reference_phrase`, `path_v0.1.6`.
   Files with neither are not rows.
5. **Pin.** A pin is a 40-hex value directly after a recognised key and `:`/`=`
   (quotes or backticks allowed). 40-hex prefixes of longer hex strings are ignored,
   as are prose and table mentions with no key, and short hashes. Search order: the
   file itself, then files directly in the same directory, then files directly in
   the parent and grandparent directories; `original_pin.found_in` and
   `original_pin.association` record which. Within a file the first key in this
   order wins, then the earliest occurrence:
   - `lead_order`: `layerfs_source_commit`, `source_commit`, `commit`, `source_tip`, `head`, `git_head`, `revision`
   - `tree_like`: `reference_tree`, `crates`, `layerfs_source_tree`, `source_tree`, `staged_tree`
   - `audit_listed`: `org.opencontainers.image.revision`
   - `extended`: `reference`, `reference_source`, `root_reference_tree`, `tag_commit`, `source`, `source_head`, `measured_source_commit`
   Among neighbouring files the best key rank wins, then the path order. A value
   that resolves to a blob is not a pin. Deliberately not pins: `source_parent`, `contract_commit`, `reporter_head`, `source_sha`, `parent`, `first_parent`, `implementation_parent`.
6. **Resolution.** `git cat-file --batch-check` gives the object type; the reference
   tree at a pin is `<pin>:crates`, or the pin itself when it is a tree that some
   known commit carries as `crates`. Nothing is inferred for a value with no local
   object: the row is `MISSING_OBJECT`, `recover_reference_at` is null and
   `recoverable_as_executed` is false. `recoverable_as_executed` is true only for a
   pin found in the row's own file; a resolving pin found in a neighbouring file sets
   `recoverable_by_association` instead.
7. **Recovery basis.** `recovery_commit_same_tree`: the pin's reference tree equals
   the recovery tree. `original_pin`: it resolves to a different reference tree, so
   the row points at its own pin. `recovery_commit_current_tree_not_execution_pin`:
   no pin was recorded, or the pin resolves but carries no root `crates` tree; the
   recovery commit is the final reference tree, not the tree that was executed.
   `unrecoverable_missing_pin`: the recorded pin is not in this clone.
8. **`introduced_commit`.** The earliest commit in topological order that added the
   path (`git log --topo-order --reverse -m --no-renames --diff-filter=A`). It is
   an upper bound on when the file was produced, never an execution pin.

### Choices made by the generator

These go beyond the lead decisions and are open to review like the rest.

- Shared package names are disambiguated against every path that ever existed under
  either tree, not only the paths present now, so a core path that was later renamed
  counts as core and is not a row.
- `path_v0.1.6` makes a file a `REVIEW` row when only its own path carries `v0.1.6`
  or `v016`; this is the only way a binary blob with no path string becomes a row.
- Within the tree-like keys, `reference_tree` and `crates` come first because they
  state the reference tree itself. A key spelled `core/crates` is not `crates`.
- The `extended` tier is not in the audit's key list. It is used only when a file
  carries no listed key; `original_pin.key_tier` marks every such row.
- Parent, contract, reporter and digest keys are not pins (step 5).
- `recoverable_as_executed` is withheld from pins found in a neighbouring file;
  those rows carry `recoverable_by_association` instead.
- `fs-benchmark-pro` and `layerfs-eval` give `STRONG`, following the audit.

## Counts

Rows: **2347** (`STRONG` 1216, `REVIEW` 1131). Rows that are binary blobs: 4.

### By signal

| Signal | Class it gives | Rows carrying it | Rows where it is the only signal |
| --- | --- | --- | --- |
| `root_link` | STRONG | 69 | 31 |
| `root_only_package` | STRONG | 407 | 77 |
| `root_only_package_historical` | STRONG | 3 | 0 |
| `root_path` | STRONG | 235 | 26 |
| `root_path_historical` | STRONG | 5 | 0 |
| `root_only_name` | STRONG | 468 | 214 |
| `fs-benchmark-pro` | STRONG | 444 | 345 |
| `layerfs-eval` | STRONG | 29 | 1 |
| `reference-tests` | STRONG | 10 | 5 |
| `ambiguous_shared_path` | REVIEW | 260 | 35 |
| `unknown_package_path` | REVIEW | 0 | 0 |
| `historical_root_only_name` | REVIEW | 3 | 1 |
| `v0.1.6_substantive` | REVIEW | 694 | 397 |
| `reference_arm` | REVIEW | 30 | 5 |
| `reference_phrase` | REVIEW | 650 | 494 |
| `path_v0.1.6` | REVIEW | 154 | 68 |

### By top directory

| Directory | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `docs/roadmap/0.1/0.1.4` | 502 | 122 | 624 |
| `docs/roadmap/0.1/0.1.7` | 186 | 261 | 447 |
| `core/docs/issues/307` | 87 | 228 | 315 |
| `docs/roadmap/0.1/0.1.3` | 166 | 49 | 215 |
| `core/docs/issues/237` | 26 | 97 | 123 |
| `core/docs/issues/286` | 1 | 93 | 94 |
| `docs/roadmap/0.1/0.1.5` | 75 | 6 | 81 |
| `core/docs/issues/302` | 7 | 68 | 75 |
| `core/docs/architecture/proposal` | 30 | 28 | 58 |
| `benchmark/fs-bench-pro` | 25 | 28 | 53 |
| `docs/roadmap/0.1/0.1.6` | 22 | 16 | 38 |
| `core/docs/issues/236` | 5 | 24 | 29 |
| `core/benchmark` | 3 | 20 | 23 |
| `release-notes/0.1.6` | 5 | 11 | 16 |
| `core/docs/benchmark/fs-bench-pro-storage-content` | 0 | 13 | 13 |
| `core/docs/architecture` | 3 | 7 | 10 |
| `core/docs/issues/303` | 1 | 9 | 10 |
| `core/docs/benchmark/fs-bench-pro` | 1 | 7 | 8 |
| `core/docs/issues/232` | 2 | 6 | 8 |
| `core/docs/issues/245` | 0 | 7 | 7 |
| `(repository root)` | 3 | 3 | 6 |
| `core/tools` | 0 | 6 | 6 |
| `docs/roadmap/0.1/0.1.1` | 6 | 0 | 6 |
| `docs/versioned/0.1.6` | 5 | 1 | 6 |
| `release-notes/0.1.4/qualification` | 6 | 0 | 6 |
| `docs/research/history` | 4 | 1 | 5 |
| `docs/roadmap/0.1/0.1.2` | 5 | 0 | 5 |
| `release-notes/0.1.3/qualification` | 4 | 1 | 5 |
| `docs/versioned/0.1.3` | 4 | 0 | 4 |
| `docs/versioned/0.1.4` | 4 | 0 | 4 |
| `docs/versioned/0.1.5` | 4 | 0 | 4 |
| `core` | 1 | 2 | 3 |
| `docs/general` | 1 | 2 | 3 |
| `docs/versioned/0.1.0` | 3 | 0 | 3 |
| `core/docs/architecture/deferred` | 1 | 1 | 2 |
| `core/docs/issues/241` | 0 | 2 | 2 |
| `core/docs/issues/301` | 0 | 2 | 2 |
| `docs/roadmap/0.2/agent-branch-reconciliation` | 2 | 0 | 2 |
| `release-notes/0.1.3` | 2 | 0 | 2 |
| `release-notes/0.1.4` | 1 | 1 | 2 |
| (20 further values) | 13 | 9 | 22 |

### By `original_pin_status`

| Status | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `LOCAL_COMMIT` | 886 | 932 | 1818 |
| `NOT_RECORDED` | 253 | 165 | 418 |
| `MISSING_OBJECT` | 36 | 23 | 59 |
| `LOCAL_TREE` | 41 | 11 | 52 |

### By `recovery_basis`

| Basis | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `original_pin` | 751 | 397 | 1148 |
| `recovery_commit_same_tree` | 175 | 546 | 721 |
| `recovery_commit_current_tree_not_execution_pin` | 254 | 165 | 419 |
| `unrecoverable_missing_pin` | 36 | 23 | 59 |

### By recoverability

`recoverable_as_executed` is true only when the row's own file records a pin that
resolves to a reference tree in this clone. `recoverable_by_association` is true
when such a pin was found only in a neighbouring file: that is an association by
directory, not a statement made by the row's own file, so it is counted apart.

| Value | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `by_association (neighbouring pin)` | 641 | 502 | 1143 |
| `as_executed (own pin)` | 285 | 441 | 726 |
| `neither` | 290 | 188 | 478 |

### Where the pin was found, and under which key

| Association | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `sibling` | 384 | 406 | 790 |
| `self` | 306 | 443 | 749 |
| `none` | 253 | 165 | 418 |
| `ancestor_1` | 197 | 31 | 228 |
| `ancestor_2` | 76 | 86 | 162 |

| Pin key | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `layerfs_source_commit` | 550 | 182 | 732 |
| `source_commit` | 64 | 419 | 483 |
| `commit` | 204 | 234 | 438 |
| `source` | 57 | 61 | 118 |
| `head` | 22 | 25 | 47 |
| `tag_commit` | 9 | 18 | 27 |
| `source_tip` | 11 | 12 | 23 |
| `reference_tree` | 15 | 5 | 20 |
| `crates` | 14 | 4 | 18 |
| `staged_tree` | 11 | 1 | 12 |
| `source_head` | 4 | 0 | 4 |
| `reference` | 1 | 2 | 3 |
| `org.opencontainers.image.revision` | 0 | 1 | 1 |
| `revision` | 0 | 1 | 1 |
| `root_reference_tree` | 0 | 1 | 1 |
| `source_tree` | 1 | 0 | 1 |

| Key tier | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `lead_order` | 851 | 873 | 1724 |
| `extended` | 71 | 82 | 153 |
| `tree_like` | 41 | 10 | 51 |
| `audit_listed` | 0 | 1 | 1 |

| Reachability of a `LOCAL_COMMIT` pin | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `remote_tracking_or_tag` | 822 | 724 | 1546 |
| `catalogue_commit_ancestry_only` | 64 | 208 | 272 |

### `MISSING_OBJECT`

59 rows depend on 7 distinct pin values that resolve to no object in this clone
(`missing-pins.txt` lists each with its keys, row counts and one example path).
22 of those rows also carry, in the file where the pin was found, another recorded
identity that does resolve to a reference tree (`candidate_reference_trees`); the
catalogue reports it but does not substitute it for the missing pin.

| Missing pin value | Keys | STRONG | REVIEW | rows |
| --- | --- | --- | --- | --- |
| `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed` | `source_tip` | 11 | 12 | 23 |
| `2753453933c55ed7f93eb21619c235668a01ef4c` | `layerfs_source_commit` | 19 | 0 | 19 |
| `26a80a8efae63b7606c845f20c69aa3a1ccef292` | `head`, `layerfs_source_commit` | 4 | 4 | 8 |
| `e54af84653cd1a22b3f26631dbd19eafb895a7b4` | `source_commit` | 1 | 5 | 6 |
| `67f0943b394d920b6c142aad8c6af94340342ae7` | `revision` | 0 | 1 | 1 |
| `bfbc46c11dbb4d8dd949b54845e222cf42cd822f` | `layerfs_source_commit` | 1 | 0 | 1 |
| `ca4e1c3a1c61129bd95ee57dd744bc53c2e635bb` | `source_commit` | 0 | 1 | 1 |

### `NOT_RECORDED`

418 rows have no recognised pin in the file, its directory, or two levels up
(`STRONG` 253, `REVIEW` 165). 7 of them carry a 40-hex value under a key this
script does not treat as a pin (`unlisted_hex40_keys`).

| Directory | STRONG | REVIEW | rows |
| --- | --- | --- | --- |
| `docs/roadmap/0.1/0.1.4` | 139 | 2 | 141 |
| `docs/roadmap/0.1/0.1.7` | 23 | 37 | 60 |
| `benchmark/fs-bench-pro` | 15 | 20 | 35 |
| `docs/roadmap/0.1/0.1.6` | 14 | 11 | 25 |
| `core/benchmark` | 2 | 14 | 16 |
| `core/docs/issues/302` | 0 | 14 | 14 |
| `core/docs/benchmark/fs-bench-pro-storage-content` | 0 | 13 | 13 |
| `core/docs/architecture` | 3 | 7 | 10 |
| `core/docs/issues/232` | 2 | 5 | 7 |
| `core/docs/issues/303` | 1 | 6 | 7 |
| `(repository root)` | 3 | 3 | 6 |
| `core/tools` | 0 | 6 | 6 |
| `docs/roadmap/0.1/0.1.1` | 6 | 0 | 6 |
| `docs/versioned/0.1.6` | 5 | 1 | 6 |
| `core/docs/issues/236` | 1 | 4 | 5 |
| `core/docs/benchmark/fs-bench-pro` | 1 | 3 | 4 |
| `docs/roadmap/0.1/0.1.5` | 2 | 2 | 4 |
| `docs/versioned/0.1.3` | 4 | 0 | 4 |
| `docs/versioned/0.1.4` | 4 | 0 | 4 |
| `docs/versioned/0.1.5` | 4 | 0 | 4 |
| `core` | 1 | 2 | 3 |
| `docs/general` | 1 | 2 | 3 |
| `docs/versioned/0.1.0` | 3 | 0 | 3 |
| `core/docs/architecture/deferred` | 1 | 1 | 2 |
| `core/docs/architecture/proposal` | 1 | 1 | 2 |
| (23 further values) | 17 | 11 | 28 |

### Reference trees other than the recovery tree

1148 rows resolve to 42 distinct reference trees that differ from `498dd1917812ae90efb8841f57e22bfc284e96fb`. For these
rows the recovery commit does not hold the reference as executed;
`recover_reference_at` is the row's own pin. The carrier is one known commit whose
`crates` tree is that tree (a remote-tracking or tagged one when there is one).

| Reference tree | STRONG | REVIEW | rows | Carrier commit | Carrier reachability | Is tag `v0.1.6` |
| --- | --- | --- | --- | --- | --- | --- |
| `dcc4fb6fd01115dcbf91ba02df414e91eb5733be` | 146 | 215 | 361 | `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb` | remote_tracking_or_tag | yes |
| `a7a2bdf7f6f0a8be7d1492777a467b49da6e7410` | 137 | 63 | 200 | `593f4ad018bf34b3f180baf66e1ae5cf40c36647` | remote_tracking_or_tag |  |
| `37bc8d24e1606c3d92246e688429738b22db39a6` | 128 | 44 | 172 | `c5ab5d7bf363cfbaa47cbc20af879d437dbae6e3` | remote_tracking_or_tag |  |
| `b367c6f8f6749bef51401b2c6bd556ee16b83608` | 42 | 35 | 77 | `9cfb4be477116646258ea0621280ed13b1824c6d` | remote_tracking_or_tag |  |
| `7e39ac0f3b676ea92b06126864656c1d1098e403` | 50 | 21 | 71 | `c48bb4903f456136ccbcdba78de38b9042d2755a` | remote_tracking_or_tag |  |
| `b0cd19d6f52ee78c227a2cfd50102ed26303ad43` | 41 | 0 | 41 | `c59d5c78dcc1b17aeb13fa30ed0711391dabc8a7` | remote_tracking_or_tag |  |
| `2b958618b668422bfefdc3dae47d7b581a3610e2` | 34 | 6 | 40 | `dc1573023850ba6c175d3c8ed9549a1ea800ef5f` | remote_tracking_or_tag |  |
| `cd5f4deb67337f90f236bb3eb7867c84c4ee5b67` | 28 | 0 | 28 | `448a74bfef5947a0ad91594bf3e5c67865e9a731` | remote_tracking_or_tag |  |
| `89622174f96d902cefbf38c8ccfd41f60e15aa3e` | 18 | 1 | 19 | `56470d418a5cad5d27616e995f88f845573854da` | remote_tracking_or_tag |  |
| `f86f893953e852f1a087d6e3551bf05ed0873ffc` | 19 | 0 | 19 | `948068a4835b67a023c4e1abf19fdd0977c05a01` | remote_tracking_or_tag |  |
| `e9e140714b538ccf270684e175b49d8c635fb8eb` | 18 | 0 | 18 | `cecd4621576735b61ac105c8daaf68b66fd4ca8b` | remote_tracking_or_tag |  |
| `44d10c053ad16c477307787baaae29b8e7c54a2e` | 14 | 0 | 14 | `5f30d03e83dd7045cc388611820659c5b978bb67` | remote_tracking_or_tag |  |
| `1e70890ade2349acd8429584142df3b3ba37b81a` | 11 | 0 | 11 | `1658f1b76728cf5ba00b69e6664efcf1039dd4ce` | remote_tracking_or_tag |  |
| `20624eeb32c2f461b35300906dc0dfc990265a22` | 5 | 5 | 10 | `fba18606ebeb5fa217cb8168063c16a5edc1d61b` | remote_tracking_or_tag |  |
| `15f2e1a3b8db673aae8513c2771980ab40829b93` | 7 | 0 | 7 | `548c3663cb9394be73ecbcc74129edc8cb80d063` | remote_tracking_or_tag |  |
| `d50055f448a62ebb8289ec44e12cca3e4dc12d5c` | 7 | 0 | 7 | `70955bc253a0a7f97f931e16b2e44eea2465d9a0` | remote_tracking_or_tag |  |
| `2e6c329289e0f097a767b5315693a6e83b07a6ee` | 5 | 0 | 5 | `fa18ebde8b6b98ad8d9e0f23eac3346f561026ae` | remote_tracking_or_tag |  |
| `bc957260cd15d67527671f14aa7bafd228745a2e` | 2 | 3 | 5 | `eb42c13477f85612fc864474af4489c2548d688d` | remote_tracking_or_tag |  |
| `de0fc4fa0f33f485ccf3739353d4920b15f14e30` | 5 | 0 | 5 | `95d09244e1467f3174e14c996d83f1735b973345` | remote_tracking_or_tag |  |
| `2f4c78cfc766305089b9c9764b4dfc44eb92a64e` | 4 | 0 | 4 | `d4f26f0d16f0f91c1f75767cf699012b8794ac01` | remote_tracking_or_tag |  |
| `d08dbf89663d4c2ce8a0c3ecebe7e8bc63494ebe` | 4 | 0 | 4 | `b17a729639d6bd1bce4a86afe112b0c82bccc028` | remote_tracking_or_tag |  |
| `53f2de30a18871d9ba9db337d8ffad670e5d87ba` | 3 | 0 | 3 | `a047e5dc48483f5b8189e19470ebdda37d4b8840` | remote_tracking_or_tag |  |
| `830bd4ee1c5919ccf758627fbbee821f4e77bd96` | 3 | 0 | 3 | `5eb1414f00bfcac2f05422afc4b0595036a694e4` | remote_tracking_or_tag |  |
| `7e478ded358b9f10db61b41c631870c48114e532` | 2 | 0 | 2 | `9929c27dc3c402dacf2986c711e5791aaa6d936e` | remote_tracking_or_tag |  |
| `7f70265e7246f28671a68e96ad3fecd863048fbe` | 2 | 0 | 2 | `1ac1ce4b56064a548929b070568d2373daa9e30d` | remote_tracking_or_tag |  |
| `802f34dbf7a82bf3eecf58e78be69dc99028d4da` | 2 | 0 | 2 | `4fbc5f837476558fd7569320885a6498ad000240` | remote_tracking_or_tag |  |
| `cd588f93891aaa269eb29f3b2582f894ef08ae9a` | 1 | 1 | 2 | `3e308a8f2578f816d8f3c6897f9ec89b885945ec` | remote_tracking_or_tag |  |
| `d4d7d948068842595c714d41dc1a206155c0293a` | 2 | 0 | 2 | `ee78028ba56741002a627b3a8875d3c888c86708` | remote_tracking_or_tag |  |
| `14668177f10c9b4b646eba2477a0aa790b72e4a8` | 1 | 0 | 1 | `93568b10cf2f8cc82d313db1a2eb40397a268770` | remote_tracking_or_tag |  |
| `24365126b97468a5674ba68880a6ae7d80bfcb88` | 1 | 0 | 1 | `01204acaf1f4b0f3e2aae9ab27c0c578b249e4da` | remote_tracking_or_tag |  |
| `683ca5aa575d43a390a4671dcf48fac7a553e440` | 1 | 0 | 1 | `27761a2f4494df7b9660c9a951bb402e764a2d16` | remote_tracking_or_tag |  |
| `783c0e8886a6610d8d2b2d2634921b5663e53af7` | 1 | 0 | 1 | `3b452f675116c86d2467e7d89a0bd8751a881440` | remote_tracking_or_tag |  |
| `8d1f74557bda9e70546cfa0b4ee0f564066856b7` | 1 | 0 | 1 | `0d2c0830fb185832125095b9f7b2b023b6f8ae02` | remote_tracking_or_tag |  |
| `acfc4b88c1d1e0b23ed8d666b05189416dc322d5` | 1 | 0 | 1 | `6d0f6acbfb0406e0f38301d8122aa425aa985b81` | remote_tracking_or_tag |  |
| `b9a0ed767fae883b0f39cd24e8a2ddf08ebed9cf` | 0 | 1 | 1 | `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d` | remote_tracking_or_tag |  |
| `bc656fc0563262310e7658e4d1eceb0a737e5c52` | 1 | 0 | 1 | `6f3fd25eac6336007e6d5936443f2e9b576b13e4` | remote_tracking_or_tag |  |
| `c03dc2a1714cb3553f6b9382ca11de6c604298a4` | 1 | 0 | 1 | `09a027622f5f14e851ac646bee45b2de41fa63f4` | remote_tracking_or_tag |  |
| `ce3afcd83986a51f5ecbf80157eb904dbbbd0151` | 0 | 1 | 1 | `fbf32e84662d00993c033515e113437965395494` | remote_tracking_or_tag |  |
| `d2b883255e42feaf2a0e52f02651a967c5c5939d` | 1 | 0 | 1 | `3fdd356c5ee06cbecd9e247d5891e7e7dcd6daa5` | remote_tracking_or_tag |  |
| `e2a8702a418b56619b431c2b3538ba43da780a69` | 0 | 1 | 1 | `01d9f70f36d870d0f410653b2c3a118ded5dc431` | remote_tracking_or_tag |  |
| `f81b1f1c8d635146ca6f1e81ca06b1423a56860e` | 1 | 0 | 1 | `45ec8918cd9da1899bed5181c5f5dc334b6f3d07` | remote_tracking_or_tag |  |
| `fc3a501c9e769f63d38ac8dcc88ef9501b5bfc2f` | 1 | 0 | 1 | `1d365f5a89b5cddd20cb2cd7132f11cfcb8cb36c` | remote_tracking_or_tag |  |

### Other counts

| Count | Rows |
| --- | --- |
| pin resolves locally but has no root `crates` tree (basis falls back to the recovery commit, `recoverable_as_executed` false) | 1 |
| file where the pin was found records identities resolving to more than one reference tree (`candidate_reference_tree_count` > 1) | 40 |
| an explicit reference key in the row's own file resolves to a reference tree different from the selected pin's (`explicit_reference_agrees_with_pin` false) | 0 |
| rows naming `core/reference-tests` (`recover_reference_tests_at` set) | 10 |
| rows with no `introduced_commit` | 0 |

### Files that are not rows

| Excluded class | Files |
| --- | --- |
| names `crates/layerfs-…` only as core paths, no other signal | 5697 |
| `v0.1.6` only as a cargo package version, no other signal | 0 |
| both of the above, no other signal | 1639 |

## What this catalogue does not establish

- **`REVIEW` rows are unreviewed.** They carry only a weak signal (a shared-name
  path, a `v0.1.6`/`v016` mention, a reference phrase, or a path component). Nobody
  has decided whether each names the root reference.
- **`STRONG` is a text rule, not a reading.** It means a root-only name or path
  occurs in the file. It does not show the file is a receipt, or that the reference
  was executed to produce it. `fs-benchmark-pro` and `layerfs-eval` are the root
  benchmark and evaluation packages that build against the reference; a file naming
  only them is `STRONG` by this rule.
- **Missing pins cannot be shown equal to the recovery tree.** For a
  `MISSING_OBJECT` row nothing in this clone shows which reference tree was executed.
- **A 40-hex token under a commit-like key may not be a commit.** A missing value
  may be a commit never fetched here or an identifier of something else, such as a
  workload corpus revision. This script does not classify missing values.
- **A neighbouring pin is an association.** When `original_pin.found_in` is another
  file, the row's own file did not state that pin. In a large shared directory the
  neighbour is chosen by key rank and then path order, and may concern other work.
  1143 rows rest on such a pin (`recoverable_by_association`).
- **A selected pin is one of possibly several.** `candidate_pin_count` and
  `candidate_reference_trees` show the others recorded in the same file; the script
  does not decide which a multi-arm receipt means. Pins written in prose or tables
  without a key, and short hashes, are not read at all.
- **`NOT_RECORDED` rows point at the final reference tree, not the executed one.**
- **`introduced_commit` is an upper bound**, and is the earliest add of that path
  even if the path was later removed and added again.
- **Recoverability is local.** A `LOCAL_COMMIT` or `LOCAL_TREE` exists in this
  clone's object database; rows whose reachability is not `remote_tracking_or_tag`
  depend on local history, and tree pins may be unreferenced objects.
- **No build, test or measurement ran.** Nothing here shows that a recovered tree
  builds or reproduces any recorded number.
