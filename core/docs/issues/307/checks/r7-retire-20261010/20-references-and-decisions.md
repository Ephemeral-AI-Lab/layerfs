# R7-retire: what refers to the predecessors, and two side decisions

> **Status:** Dated checkpoint receipt. Reference audit at `a6655ef45`, before
> any removal. No sample was taken.

Raw search output: [receipt 04](04-references.txt). The search covers every
tracked file outside the seven directories for the package names
`layerfs-{bridge,daemon,fuse,sandbox,sdk,workspace}-legacy`, `layerfs-server`
and `layerfs_server`, and every `#[path]` or `include!` that leaves its crate.

## 1. Manifests and lockfiles

| Reference | Finding | Action |
| --- | --- | --- |
| `core/Cargo.toml` `exclude` | Seven entries, one per directory. Nothing else in the manifest names them | Remove an entry in the commit that removes its directory. Four entries stay with the four crates kept for O-10 |
| `core/Cargo.lock` | No package entry for any of the seven | None |
| Root `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml` | No reference; the root workspace excludes `core/` | None |
| Manifests of the 13 active members | No dependency, dev-dependency or path into the seven | None |

## 2. Source includes

No active crate's `src/`, `tests/` or `examples/` reaches one of the seven
directories by `#[path]`, `include!`, `include_str!` or `include_bytes!`. The
cross-crate `#[path]` lines that exist go to other active crates' test support
and to `core/benchmark/fs-bench-pro-storage-content`.

In the other direction, three tests of `layerfs-fuse-legacy` include
`../../layerfs-workspace/tests/support/native_workspace.rs`, a file the active
Workspace does not have. That is one more reason they cannot be built.

## 3. Scripts and tools

| Reference | Finding | Action |
| --- | --- | --- |
| `core/tools/check_product_boundary.py` | Scans `src/` of every directory under `core/crates`, the seven included, for line caps and prohibited constructs. Lists `layerfs-server` among the components a domain crate may not name. Applies no dependency rule to a package it does not know, so the legacy manifests pass unchecked | Add the removed directories as retired packages: the scan fails if such a directory or a manifest with such a package name returns. A dependency on a retired package is refused for the 13 known packages by the existing allow-list; the guard still applies no dependency rule to the four kept predecessors, whose manifests it does not know (`layerfs-sdk-legacy/Cargo.toml` keeps its now dangling `layerfs-server` path, [receipt 43](43-dependency-check.txt)). The scanned-file count drops with each removal |
| `core/tools/test_check_product_boundary.py` | Asserts that a `layerfs-sandbox-legacy` manifest with an `layerfs-api-core` dependency passes | Replace with the assertion that a retired package manifest is refused |
| `tools/production_loc.py` | Counts every `core/crates/*/src`; names none of the seven | Not edited. Its SHA-256 is unchanged |
| `core/target/rx-linux.sh` (untracked helper) | Filters `-legacy/` out of its source hash list | None; untracked and still correct |
| Python route scripts inside the seven directories | Start `layerfs-server` and the old daemon from a `BIN` directory | Removed with their directories; nothing outside them calls them |

## 4. Benchmark harnesses under `core/benchmark`

No harness opens a file inside one of the seven directories. Nine lines name
`layerfs-server` as a Cargo package or as a Rust crate:

| Harness file | What it does | State before this stage |
| --- | --- | --- |
| `fs-bench-pro/families/phase7_sqlite.py:315`, `phase7_storage.py:60`, `diagnostics/run_namespace_cause.py:38` | `-p layerfs-server` for the **baseline arm only**, built in a dedicated baseline worktree at that family's pinned commit | Unaffected by HEAD. The package exists at the pinned commit |
| `fs-bench-pro/diagnostics/baseline_init.rs`, `sqlite_reference_init.rs` | `use layerfs_server::…`; copied into the baseline worktree's SDK examples for the baseline build | Same |
| `fs-bench-pro/runner.py:40` (retained SDK Init family), `families/workspace_write.py:167`, `shell_package.py:186` | `-p layerfs-server` against the current checkout | Already unbuildable at HEAD: `cargo pkgid -p layerfs-server` matches no package ([receipt 02](02-cargo-cannot-load.txt)), and the `layerfs-sdk` examples they also build went with F12. The [harness guide](../../../../../benchmark/fs-bench-pro/AGENTS.md) routes these families to "their compatible pinned source" and withdraws the host-mediated topology from new selections |
| `fs-bench-pro/shared/evidence_registration.py:77` | Refuses a product seal whose artifact path contains `-legacy/` or `/layerfs-server/` | Still correct after removal |

The harness is not edited in this stage: the families are frozen, the task
takes no sample, and removal changes nothing they can do. What they need is
R8's: a registered `NOT_RUN — mechanism removed` disposition for the
host-mediated families (303/08 O-16, itself awaiting the owner).

`core/benchmark/fs-bench-pro/r7`, `r7-runtime`, `r7-passthrough`, `r7-cache`,
`r7-tools`, `cluster2-platform` and `fs-bench-pro-storage-content` name none of
the seven.

## 5. Documentation the product needs

| Document | Reference | Action in this stage |
| --- | --- | --- |
| Root `AGENTS.md` | "Do not revive `layerfs-server`" | Unchanged; still the rule |
| `core/AGENTS.md` | Names `layerfs-workspace-legacy`, `layerfs-fuse-legacy`, "excluded predecessor crates … remain intact", "Do not revive `layerfs-server`" | Updated in the final commit to the actual state |
| `core/docs/architecture/75-native-request-service.md` | "preserved byte-for-byte in `layerfs-fuse-legacy`" | Updated with the removal |
| `core/docs/architecture/71-ordinary-engine-runtime-foundation.md` | Old Sandbox "moved intact to `layerfs-sandbox-legacy`" | Updated with the removal |
| `core/docs/architecture/14-service-runtime.md`, `16-history.md`, `03-files.md` | Describe `layerfs-server` source at their own pins | Dated note added: the package is removed, with the commit that still holds it |
| `core/docs/api/operations.md` | Live link into `layerfs-server/src/service/handler.rs` | Link replaced by the recovery commit; the paragraph was already stale (its Bridge link died with F12) |
| `core/docs/benchmark/fs-bench-pro/exec2edit.md` | Planning text for the #232 route over `layerfs-server` | Dated note added |
| `core/docs/architecture/20-workspace-base.md`, `21-daemon-owner.md`, `22-sdk-runtime.md`, `23-native-bridge.md` | Each says its predecessor is preserved and "S11 must remove it" | Unchanged: those four crates are still present |
| `core/docs/architecture/proposal/service-daemon-transport/07-public-operations.md` | Link to a `layerfs-server` file that already did not exist | Unchanged: a proposal at its own pin |
| `core/docs/issues/**` (534 files) | Dated plans, handoffs and receipts | Unchanged: historical records keep their paths and verdicts. The commit that still holds each directory is recorded in the completion record |

## 6. `core/reference-tests`

60 tracked files: `phase45-storage/` (53) and `phase7-services/` (6) plus a
README. They are Rust test and example sources for interfaces retired before
this rollout (native CAS and private Save, PostgreSQL and MinIO). There is no
manifest in the tree, so they belong to no package; nothing builds them; the
pinned counter does not count them; no tracked file outside `core/docs/issues`
refers to the directory. They are not source of the seven predecessors.

Recorded direction: the directory's own README says "Archived; retained for
historical evidence only" and "Retirement is not an algorithmic
simplification"; the root guide says to retain historical evidence. The
rollout says nothing about when, or whether, they go.

**Decision: left in place.** Reason: the only recorded statement about them
says they are retained, and no document authorizes removing them. Listed as an
owner question, with a recommendation, in the completion record.

## 7. `core/crates/layerfs-api/cli` and `mcp`

Each holds one tracked file, a README: "Placeholder. No CLI command or project
operation is implemented here yet." No manifest, no source, zero production
lines; `core/Cargo.toml` lists only `crates/layerfs-api/sdk`.

Recorded direction points both ways. `core/AGENTS.md` says "Scaffold no empty
placeholder directories" and "Add members only with real product boundaries
and implementation". `core/docs/api/README.md` and `operations.md` say future
MCP and CLI adapters will exist and what they may do, without saying where.

**Decision: left in place.** Reason: the rule forbids creating placeholders
and does not say to delete these two, and the API documents still plan the
adapters. Listed as an owner question, with a recommendation, in the
completion record.
