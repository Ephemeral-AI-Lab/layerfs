# Issue 179 bounded Linux implementation: closure report

> **Status: final-round record; target LayerFS v0.1.7, not released.**
> Written 2026-09-23. Product source `f802cc124` (mounted namespace syscall
> evidence and live link-count presentation) plus the task-3 documentation
> commit `c4f451239`; the final whole-core checks in section 3 ran on exactly
> that tree, logs under `core/target/pair1-evidence/round58/checks/`. This
> report closes [51](51-implementation-completion-spec.md) section 7's
> implementation-phase checklist. **It does not close the GitHub issue**: the
> push, the merge to the main line and the issue closure are owner acts, and
> the Linux clippy component gap (section 10) is still open with the owner.
> Nothing here is a performance, durability, crash-recovery or
> release-readiness claim.

## 1. Completion statement

**Bounded Linux implementation complete** in the exact sense [51 section 1](51-implementation-completion-spec.md#1-authority-and-concrete-deliverable)
defined: every required operation in 51 section 3 works through native
Workspace and its applicable actual FUSE path, and the small real-daemon
workflow passes with two explicit Commits. The claim is **not** full POSIX
conformance, arbitrary-workload support, performance or release readiness.
All numeric limits were kept (section 4); no registered selection was dropped;
no assertion was weakened to pass; no historical receipt was relabelled.

## 2. The 51 section 7 checklist, box by box

| 51 section 7 box | Status and evidence |
| --- | --- |
| Every required operation works through native Workspace and its applicable actual FUSE path; no stub remains | **Closed.** All ten native namespace selections PASS (`round57/final3-ns-<case>-01`: setattr, mknod, link, unlink, unlink_fresh, rmdir, rename, rename_base, generation, notification_failure) and both mounted selections PASS (`round57/final-ns-mounted-{kernel,durability}-01`), on source `f802cc124`. The earlier operations keep their source-pinned mounted receipts ([write 25](25-mounted-write.md), [resize 26](26-mounted-resize.md), [mkdir 40](40-mounted-mkdir.md), [create 44](44-mounted-create.md), [symlink 48](48-mounted-symlink.md)). Per-operation matrix rows and reproduction commands: [01 section 7.2/7.4](01-workspace-fuse-contract.md#74-phase-evidence-and-exact-reproduction-commands) |
| Fresh/existing objects, aliases, replaced/open-unlinked files, pinned views and subsequent generation completion preserve identity/data | **Closed.** `final3-ns-link-01` (shared inode, one save, fresh aliases before first Commit), `final3-ns-unlink-01`/`unlink_fresh-01` (open-orphan lifetime, no resurrection, no unbound declaration), `final3-ns-rename-01` (replacement with the destination FD held, the replaced FD readable after its Commit, same-inode no-op), `final3-ns-rename_base-01` (tombstone hides an inherited binding), `final3-ns-generation-01` (G+1 keeps later namespace and metadata changes), `final-ns-mounted-kernel-01` (st_ino/st_nlink identity through the real mount) |
| Attribute/namespace mutations validate and publish atomically; bounds and unsupported behavior explicit, no false success or partial refusal | **Closed.** `final3-ns-setattr-01` (invalid mixed request leaves everything unchanged; unsupported fields refused), `final-ns-mounted-kernel-01` (chmod, `UTIME_OMIT` mtime, mtime-only keeps the mode, `EEXIST`/`EINVAL`/`ENOTEMPTY` refusals with trees unchanged), `round54/final3-mkdir-refusals/reserve_denied/reserve_unknown-01`, and the capacity selection's diagnostic re-run `round54/diag-mkdir-capacity-01` (refusal consumes no reservation; the first attempt's FAIL receipt `final3-mkdir-capacity-01` stays on disk as the transient of section 8) |
| Small deterministic G/G+1 case and multi-entry notification-failure ownership check pass | **Closed.** `final3-ns-generation-01` and `final3-ns-notification_failure-01` |
| Real-daemon small-project scenario passes two explicit Commits, public saved-state inspection, remount and native clean closure | **Closed.** `round56/t1-small-project-0j` PASS (first completion), re-passed on the task-2 source as `round57/final3-t1-small-project-01` |
| Required final source checks pass; no observed small-workflow correctness failure concealed | **Closed with one declared verification gap.** Section 3 records this round's results: host test/clippy/build/fmt/guard/self-tests all clean; Linux test and build clean; the Linux clippy component is not installed in the pinned image (owner item since [54](54-issue179-namespace-closure-handoff.md) section 6, unchanged), so the clean host clippy stands. The open failures in section 8 remain open and listed, not concealed |
| Current architecture/operation matrix and exact reproduction commands updated; final report lists accepted bounds, optional unsupported features, deferred load work and historical open failures separately | **Closed.** Matrix and commands: commit `c4f451239` (01 sections 7.2/7.4/8.1; 02 section 2.3; 03 section 7; 13; 15; 28; README). This report's sections 4-8 are the four separate lists |
| Changes committed with exact per-commit production LOC accounting; no push, merge, release claim or automatic issue closure | **Closed for the agent's part.** Every commit message carries its `Production LOC:` line (section 9). Nothing has been pushed, merged or closed; section 10 names the owner acts |

## 3. Final source checks (this round, exact commands and results)

On the frozen tree `c4f451239` (product source `f802cc124` plus documentation),
Rust 1.85.1, locked and offline. Host commands from the repository root with
`CARGO_BUILD_JOBS=2`, `LAYERFS_CONSTRUCTION_WORKERS=1`,
`CARGO_TARGET_DIR=$PWD/core/target`; Linux commands once in the pinned image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` with
`--cpus 2`, the worktree mounted at `/work`, the cargo registry read-only and
`CARGO_TARGET_DIR=/work/core/target-linux` (repository-root `.cargo/config.toml`
supplies the ARMv8 AEAD flags inside the mount). Logs are append-only under
`core/target/pair1-evidence/round58/checks/`.

| Check | Result | Log |
| --- | --- | --- |
| Host `cargo +1.85.1 test --workspace --locked --offline` | **702 passed, 0 failed, 3 ignored** | `host-test.log` |
| Host `cargo +1.85.1 clippy --workspace --all-targets --locked --offline -- -D warnings` | **clean** | `host-clippy.log` |
| Host `cargo +1.85.1 build --workspace --bins --examples --locked --offline` | **clean** | `host-build.log` |
| Host `cargo +1.85.1 fmt --all -- --check` | **clean** | `host-fmt.log` |
| `python3 core/tools/check_product_boundary.py` | **PASS, 260 production files** | `boundary-guard.log` |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | **6 tests OK** | `guard-selftests.log` |
| Linux `cargo test --workspace --locked --offline` | **700 passed, 0 failed, 174 ignored** (the ignored set is the route-driver `#[ignore]` tests, which run through their registered routes, not `cargo test`) | `linux-test.log` |
| Linux `cargo build --workspace --bins --examples --locked --offline` | **clean** | `linux-build.log` |
| Linux `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | **blocked**: `cargo-clippy` is not installed for the toolchain `1.85.1-aarch64-unknown-linux-gnu` in the pinned image — the known owner item; the clean host clippy run stands | `linux-clippy.log` |

The whole-core `cargo test` suite was run exactly once, in this final round,
per [51 section 6](51-implementation-completion-spec.md#final-source-checks);
no earlier round of this phase claimed it.

## 4. Accepted bounds (kept unchanged through the phase)

From [51 section 2](51-implementation-completion-spec.md#2-freeze-the-scope-avoid-another-scalability-project),
all preserved:

- Prepared request: 128-entry admission rules and 32 KiB metadata.
- Resident nodes 256, handles 128, payload records 4096.
- Existing/captured-file replay 8 MiB and 256 edits; 1024 pieces; `MAX_FILE` 4 GiB.
- Default accounted Workspace memory 8 MiB; explicit disk quota and Workspace
  count; page/candidate bounds (128 data pages, 548 KiB candidate reservation
  plus 548 KiB completion escrow, 32-root registry, 65,536 slots, 12 cleanup
  frames, arena `update` admission of at most four cells per call for D/I/N/R
  keys and one for E/T keys).
- Topology work bound including the 4096-entry limit.
- One submission, one construction worker — `construction_worker_limit` and
  `construct_files` are single-producer in the default wiring, every run
  exports `LAYERFS_CONSTRUCTION_WORKERS=1`, and `init_namespace` remains the
  only multi-worker exception.
- Owner identity, current name grammar, Linux mount and direct-I/O profiles.

One owner-ruled addition, recorded in [54 section 3](54-issue179-namespace-closure-handoff.md):
the host admits one bounded second metadata call (a read-only inspection or
one serial reservation) that may overlap a call actually in flight; it never
carries content, and the host's minimum memory budget grows by one 128 KiB
call allowance for that slot ([13](13-portable-metadata.md)). The 8 MiB default
budget and every other registered bound are unchanged.

## 5. Optional unsupported features (explicit refusals, not oversights)

From [51 section 2](51-implementation-completion-spec.md#2-freeze-the-scope-avoid-another-scalability-project)
and [01 section 7.3](01-workspace-fuse-contract.md#73-optional-kernel-owned-or-unsupported-operations):
xattr APIs (`getxattr`/`listxattr`/`setxattr`/`removexattr`), ACL/capability
mapping, arbitrary uid/gid changes, atime/ctime/birthtime setters, special
files (FIFO/socket/device), rename **exchange/whiteout** (`RENAME_NOREPLACE`
is the only selected flag), `readdirplus`, `fsync`/`fsyncdir`, `fallocate`,
`lseek` `SEEK_DATA`/`SEEK_HOLE`, `copy_file_range`, remote/distributed locks
(`getlk`/`setlk` not forwarded), `bmap`, `ioctl`, `poll`, `setvolname`/
`getxtimes`, shared writable `mmap`, `O_SYNC`/`O_DSYNC`, immediate
`FUSE_INTERRUPT` cancellation, and every crash-recovery or durability
behavior. Unsupported requests fail explicitly; no silent no-op.

## 6. Declared limitations of the bounded profile

- **Moving a committed directory whose namespace record is absent is refused**
  (`Unsupported`): a committed directory's children resolve through the
  canonical tree by path, and this profile has no identity-keyed service query
  to re-anchor them after a move. Moving a directory this generation created
  works and is evidenced (`round57/final-ns-mounted-kernel-01`).
- **The re-resolved base-identity link-count corner**: within one baseline the
  resident node and a resolution must agree on the namespace link count, and a
  Commit that republishes the identity canonically refreshes the resident
  node; a re-resolved base identity whose delta link-count adjustment was lost
  with its resident node presents the canonical count until the next Commit
  republishes it. No registered selection reaches this corner
  ([01 section 8.1](01-workspace-fuse-contract.md#81-attribute-projection)).
- **The 4096 examined-binding limit is per validation walk**: parent-alias
  validation can traverse the whole base namespace even for a small moved
  subtree, and no typed capacity-versus-cycle result is available for that
  path ([01 section 9](01-workspace-fuse-contract.md#9-mutation-visibility-flags-and-the-rename-ceiling)).
- **C1's path grammar is narrower than arbitrary Linux filename bytes**
  (UTF-8, 255-byte names, 4096-byte paths, 256 components; no `.`, `..`, NUL,
  slash or backslash in names). Incompatible names are refused, not normalized.
- **The daemon admits exactly one control session**; a scenario needing both
  a control session and a public Service session owns one at a time
  ([55 section 3](55-issue179-realdaemon-handoff.md#3-the-blocker-a-rename-cannot-resolve-a-destination-parent-after-a-commit)).

## 7. Deferred load work (not prerequisites for this phase)

From [51 section 2](51-implementation-completion-spec.md#2-freeze-the-scope-avoid-another-scalability-project)
and [54 section 6](54-issue179-namespace-closure-handoff.md#6-what-is-left-to-close-179):
DSH and other package installations/uploads, large repositories, stress
suites, maximum-size campaigns; throughput, tail latency, RSS/cgroup and
cold-cache qualification; matched R6/#207; larger prepared-input protocols,
external indexed C1 input, payload-catalog scaling/packing and enlarged
resident-node capacity; new remote controls for every filesystem operation
and a failed-submission recovery protocol; R5b (the declared npm workflow);
the full writable target's capacity, concurrency and performance
qualification; and the many-file metadata-scale gate of
[02 section 2.4](02-overlay-snapshot.md#24-many-tiny-files-metadata-must-scale-independently-of-payload).

## 8. Historical open failures (kept append-only, none concealed)

- **Round43 native-create capacity gate FAIL** (`native-create/create-capacity-01`):
  admitted 93 fresh files and failed at the capacity gate. Retained open in
  [43](43-native-create.md)/[44](44-mounted-create.md); explicitly deferred as
  a near-limit capacity investigation by 51 sections 2 and 5.
- **Round49 captured-G replay FAIL** (post-rebase `refuse_extra`): the +1
  write returned `Capacity` with no observed state change but delivered one
  native `Inspect` first, which the caller forbids. Stays open
  ([49](49-fresh-file-streaming.md)); 51 section 5 recorded the
  acceptance-contract clarification (a necessary read-only consultation before
  refusal is permitted) but no new focused result was recorded in this phase.
- **`mkdir/capacity` transient `Service(Failure { code: Io })`**: hit once in
  round 54; a pre-existing transient transport outcome already recorded at
  `ab473145`, not a product refusal. The diagnostic re-run passed with
  byte-identical observations; the transient cause is not explained
  ([54 section 5](54-issue179-namespace-closure-handoff.md#5-verification-exactly)).
- **The three-fresh-directory C1 cycle suspicion** (51 section 5 step A) was
  not separately reproduced through the public C1 API; no registered selection
  of this phase hit a cycle failure, and the rename path's own bounded cycle
  refusal (`EINVAL`, `descends_from`) is evidenced through the mount.

## 9. Source growth and per-commit accounting

Phase basis `83b4dbf49` (51's source basis) → final tree (`f802cc124` plus
documentation), counted with the unchanged `tools/production_loc.py` on `git
archive` snapshots, scope `crates core/crates`:

| Scope | 51 basis | Final | Delta |
| --- | --- | --- | --- |
| Replacement core | 47,558 | 50,312 | +2,754 |
| — of which `layerfs-workspace` | 11,971 | 14,562 | +2,591 |
| Reference `crates/` | 65,417 | 65,417 | 0 |
| Combined | 112,975 | 115,729 | +2,754 |

The growth is the six namespace/attribute operations, the identity, ownership,
re-anchor, admission, carry and declaration-ledger machinery behind them, and
the live link-count presentation — not relocation, a scope change or reference
retirement. Test and documentation changes do not enter the count.

Product commits of the phase, in order: `96371122f` (the six operations),
`578ef7012` (namespace accounting), `5e4a89d0a` + `a7356d665` (the sixteen
selections and the bounded metadata call), `107c81a18` (replaced-identity
record, destination binding, `keep_name`), `a9657b85f` (moved identity's
record, completed real-daemon scenario), `f802cc124` (mounted syscall
evidence, live link-count presentation). Each commit message carries its exact
first-parent `Production LOC:` line. The documentation commits (`a7ee87d6e`,
`e26b40bf9`, `e00136c1a`, `22c0181fd`, `fd4bbb3b1`, `441296bd7`, `c4f451239`)
and this report's commit are docs-only, delta 0.

## 10. What remains for the owner

1. **Push** `codex/pair1-remote-mount` — the task-3 and final-round commits
   are local only.
2. **Merge to the main line and close #179.**
3. **The Linux clippy component**: the pinned image reports `cargo-clippy is
   not installed for the toolchain '1.85.1-aarch64-unknown-linux-gnu'`. Either
   add the component to the pinned image or accept the gap in writing; the
   clean host clippy run is what stands ([55 section 8](55-issue179-realdaemon-handoff.md#8-two-items-that-need-the-owner)).
4. Optionally, the one bounded check [54 section 6](54-issue179-namespace-closure-handoff.md#6-what-is-left-to-close-179)
   named: characterize the transient `Service(Failure { code: Io })` transport
   outcome rather than leaving it unexplained.

Beyond the bounded phase, section 7's deferred work remains open and does not
block this closure.
