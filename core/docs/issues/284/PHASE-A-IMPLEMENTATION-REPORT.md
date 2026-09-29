# #284 Phase A implementation handoff

> **Status:** Dated implementation checkpoint; not release evidence or numeric
> admission. 2026-09-29. **Implementation ready for Phase B evaluation.**
> PR #285 remains draft; Phase B remains ON HOLD/NOT_RUN.

The selective active port is a coherent runnable implementation on the corrected
component-only namespace. New LocalEdit has one active mutation authority, exact
internal-v2 capability admission, funded immutable G1 capture/live G2, grouped
upload and affected C5 installation. Ordinary public SDK Mount/Exec/Commit and
real FUSE passed the final small correctness routes. This assignment runs no
Phase B campaign, recommends no merge, closes no issue and claims no numeric or
release admission. The earlier [partial report](PHASE-A-PARTIAL-IMPLEMENTATION-REPORT.md)
remains an accurate historical checkpoint at its own source.

## Exact identities and ownership

| Identity | Value |
| --- | --- |
| Owned checkout | `/Users/yifanxu/.codex/worktrees/phase45-active-integration/layerfs` |
| Owned branch | `codex/issue284-phase45-integration` |
| Published review base, rechecked | `codex/issue264-phase45`, `6eb7553671d3000160ad023a57b335e52dd81a26` |
| Corrected T0 foundation | `05eb5c14849f1b874383fd1600ba9288f103ad93` |
| Clean mechanism donor P0 | `11a864fc133844cae7a4247b1f243d84d5763b10` |
| Frozen design | `f9f9abb37332e23d1968a5ed204ff70e3e66d783` |
| Final product/test implementation commit | `671f4a46f8f354cf764d3d1555529b73a5c939e0` |
| Implementation tree | `ed5d82c664c9720a49d609cfbbb43d632adf890e` |
| Core product seal | `5afe905cdcaaa30283482b2dad8233b71696b9efa142a31168a088eaf3115fbd` |
| Test/example seal | `824b82bc56951e8ed051803ed130bdf9b448770058a11d5ab8579b5c3fabd014` |
| Compilation-input seal | `3c585598b3faaa3cb8bef84628c9a4a971b8d9c5680ff9148d3db0d109ca2672` |
| Cargo.lock SHA256 | `4c3d4301fb96b05e9ebb7e359b11a18bc725be2921dcb8533aa60bdb2ea5c614` |
| Repository-root ARMv8 config SHA256 | `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9` |
| Daemon binary SHA256 | `d3a6af30079224673ec23cd6aa1648a8dcd62eb12193874dc8e472c61a105557` |
| Immutable final daemon image | `sha256:c36ff06d4701b4de922bc571fb68c8c80ad8f80de04975f431cfd8084d1d24af` |

The initial checkout was already at the published incomplete checkpoint
`1ab1501c9b44b1d3907bdfcba8b5020698525fb4` with uncommitted active-port work.
Its branch/history/T0 ancestry were inspected, that work was preserved and
completed, and only this branch is committed/pushed. Other owners' worktrees and
#260/#263/#269/#274 heads were not changed. The `layerfs-273-finalize` checkout and
its pending edits were not accessed for mutation or imported. The readable
fixture was adapted explicitly with actual `version: 2`.

Seals use the method and per-file hashes in [PORT-MANIFEST.json](PORT-MANIFEST.json).
Construction checks/images were made from recorded uncommitted source bytes;
that exact product/test source is now committed above. No dirty/sealed speed arm
comparison was made. All Cargo targets, linker wrappers, logs, archives and host
SDK scratch were worktree-local. Linux semantic backing used the owned Docker
volume `layerfs-issue284-phase-a-native-01` on the supported ext4 profile.

The image is **Cargo dev / aarch64 Linux / root FUSE deployment / correctness
only**. It is not a release/SDK Init/performance arm. It uses immutable Alpine
base `sha256:ce64758a109eb420d874a118f87920e625e12d3634e03b4a5573fd9f6e5d3507` and
an unchanged published system SQLite static archive for the separate native test
executable. No package was patched, forked or vendored. The worktree linker
wrapper selects one CRT and the packaged static library; root ARMv8 Rust flags
were inherited, never replaced with RUSTFLAGS. A Phase B release build/image
needs its own matched compilation identity.

## Reuse/adapt/exclude and resolved conflicts

The complete source-pinned file manifest records T0, P0 and final SHA256 values.
The maintained [architecture description](../../architecture/17-workspace-active-components.md)
records actual source owners, bounds and algorithms in the same implementation
commit. Its source pin is advanced by this docs-only handoff.

| Owner | Disposition and semantic result |
| --- | --- |
| C1 filesystem ordering/validation | Earlier `4ea80fc17` selectively ports `fbda0f0f`: actual declared-new parent membership and reached-walk charges, retained alias/cycle/reachability validation. No parent index, C2 or History schema migration. |
| Internal capability/SaveFile | Earlier `99ee82e57` plus additive lease classifiers: v1 opcode20 remains; authenticated28 returns typed v2 and29 validates fixed-record v2 input. Service resolves backward/repeated Base runs locally with bounded windows. |
| `backing/active/` | Reuse P0 page/index/pack/extent/HotRef/retirement algorithms. Adapt namespace publication to carry cleanup disposition and clean-close ownership to exclude only its exact payload Arc. Physical refunds retain exact identity/block/unlink checks. |
| State and ancestry | Adapt T0 State individually; retain NodeName, parent/attached graph, identity reads, indexed updates and complete charged ancestor collection for LocalEdit and ReadOnly. An unchanged `runtime/ancestry.rs` is T0 authority. |
| Namespace callers | One active backend. Prepare component owners and logical N/I/D changes, recheck live ancestry/stamps before publication, then install only the moved resident Node edge. Legacy namespace mutation helpers are retired from Core. |
| Selection and origins | Filesystem `State.base` and file `I.base` remain distinct. Active origins precede canonical refresh. Matching regular revisions collapse to saved Base; later nonzero G2 Base coordinates, directory edges and symlink target extents survive C5. |
| Capture/completion | Same dirty-generation fund transfers to G1; clean capture admits a fund if absent; G2 admits its own. Saved file roots precede metadata calls; checked canonical outcomes precede C5. Native same-selector local resume never reissues the command. |
| Public leases | Additive API/Bridge/daemon surface uses P0 wire. HeldEntry stores issued identity/private content facts without a pathname. Root entry/pin admission is atomic and root metadata is selected at pin time. 32-lease, response-admission, token binding and checked release remain. |
| Native/FUSE lifecycle | Read Deadline versus uncertain mutation/release Unknown; held-session expiry without replay; one lifecycle slot and serialized authenticated session; existing conditional Host metadata admission. Tiny FUSE writes use the ordinary bounded charged publication route. |
| Exclusions | Donor NodePath, extended_path, child_path_active, PinnedDirectoryPath, rename_paths, HeldEntry.path, descendant walks/resident path rewriting, benchmark runners/registries and Host Exec timing wrappers are excluded. |
| Unchanged owners | Reference crates, C2/History product/schema, dependency manifests/lock, worker/quotas/deadlines, root AEAD flags and benchmark trees. Init's existing worker exception remains. |

Additional construction fixes preserve required semantics: remove unreachable
fresh directory/symlink declarations when their last binding disappears or is
replaced; advance non-directory I generation/revision with a later rename's D
key; allow active-owned symlink payloads to drain on clean close while real
external pins still refuse; explicitly release each accepted namespace mutation's
temporary old view so final-pin cleanup errors cannot disappear in Drop.

## Small correctness proof and custody

Final [evidence index](EVIDENCE-INDEX.json) preserves every failed diagnostic and
all final log hashes. Published `.log.gz` wrappers decompress to the exact raw
SHA256 bytes; original raw logs remain under `core/target` and at the
implementation commit. Compression changes publication representation only.
The staged raw-text publication initially produced added EOF blank-line warnings;
this docs-only wrapper change makes the final PR whitespace check clean without
rewriting raw observations or suppressing the source check.

| Final check | Actual scope/result |
| --- | --- |
| Native `inherited_workspace` filter `phase_a_` | **8 PASS** on final source: exact v1/future-version refusal before dependent inspection; arbitrary overlap, append, resize/Zero, 512-byte owned payload/backward Base copy; metadata and selected root metadata; hard links/open-unlinked; fresh nonfile remove/replacement/symlink move; G1/G2 and old selection bytes; clean/dirty completion credit; 32 leases and forged/stale/unissued identities; known save/unknown Commit; known canonical/local C5 failure and same-selector local resume; published unlink-failure custody. |
| Live SDK `agent_route`, image supplied | **4 PASS, none early-returned**: both inherited-directory routes, >4096-byte descendant access through FDs, 270-component POSIX/FUSE traversal with independent canonical serial oracle after Commit, and normal lifecycle/edit/Commit/history-conflict/cleanup. Actual SDK checked unmount and Sandbox deletion prove their own cleanup surfaces. |
| ReadOnly `readable` | **10 PASS** in owning workspace tests, including complete retention of all270 forgotten ancestors while a directory handle survives. Actual typed capability and identity-relative fixture grammar are used. |
| Bridge lease codecs | **2 PASS** in runtime group: unchanged entry/header wire roundtrip and malformed token/component refusals. Prior v1/v2/capability/native deadline/direct Service proofs remain covered by owning package tests. |
| T0 selected inherited regressions during adaptation | 4 PASS: retained ancestry/detached parent, inherited move/handles/Commit, serial symlink reads and frozen G1/later G2. These intermediate checks are recorded separately from the final-source cohorts. |

The native retained-failure facts are precise, not phase peaks: metadata denial
retained **16,384 allocated / 851,968 reserved bytes**, a known file root and zero
canonical calls; lost canonical result retained **24,576 allocated / 843,776
reserved bytes** and one canonical call, with no installed C5 revision. Public
Commit then refused Busy and did not replay. A seccomp fallocate denial after
known canonical success produced `KnownCommitLocalFailure`; native same-selector
local resume completed with exactly one canonical call and zero reserved bytes
at completion/zero allocated bytes at checked clean close. A seccomp unlink denial
returned `Published`, keeping accepted revision2 and directory identity10 plus
charged failed ownership. The failed-owner files are retained in the native
volume and the worktree-local `retained-native-custody.tar.gz`; no successful
refund or crash-recovery format is claimed for them.

External faults/fixtures live under tests/, with no test hook, inline test, test
feature or fake carrier in product src/. Successful small checks are finite
semantic evidence, not full campaign or numeric qualification.

Retained failures include: cross-compiler target/CRT/system-library setup errors;
an incompatible attach fixture that used Root rather than LocalEdit's required
Branch (fixed before its dependent-inspection assertion passed); fresh nonfile
unreachable-declaration failure; active-owned symlink close refusal; reused
container-PID fixture name colliding with deliberately retained failed custody
(fixture now uses a unique suffix); the first rootless daemon image's creation
failure (its exact uid65532/owned-volume permission refusal is retained; SDK's
existing mount profile requires root); the namespace Drop-hidden cleanup failure;
and existing Linux-only Clippy findings corrected without changing byte/count
oracles. All earlier v2 fixture failures remain in the historical partial report.

## Required owning Core checks

The literal aggregate `cargo ... test --workspace` was replaced by bounded
owning-package commands, as authorized by the handoff. All12 Core packages retain
test discovery; changed owners were checked after their covering fixes. Final
source/source-local build and evidence hashes are pinned above.

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-api-core -p layerfs-bridge -p layerfs-sandbox -p layerfs-workspace -p layerfs-fuse -p layerfs-daemon
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content -p layerfs-storage -p layerfs-history -p layerfs-telemetry
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-sdk -- --skip sdk_exec_
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-workspace -p layerfs-fuse -p layerfs-daemon
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
git diff --check
```

These commands **PASS**. The host SDK command's three skipped Exec tests were
executed in the actual image-supplied full `agent_route` command below, together
with the lifecycle test. Image-less early-return entries in the host SDK group
are **NOT_RUN there**, never counted as live proof. Host cfg-Linux exclusions and
existing ignores do not prove their external routes. The existing ignored4097
Service proof and the complete native legacy/FUSE/pressure/lowering suites remain
**NOT_RUN** here; compilation/Linux Clippy is not their execution.

```sh
LAYERFS_TEST_IMAGE=sha256:c36ff06d4701b4de922bc571fb68c8c80ad8f80de04975f431cfd8084d1d24af LAYERFS_CONSTRUCTION_WORKERS=1 TMPDIR="$PWD/core/target/issue284-phase-a" cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test agent_route -- --nocapture --test-threads=1
core/target/issue284-phase-a/build-linux.sh test --manifest-path core/Cargo.toml --locked --target aarch64-unknown-linux-musl -p layerfs-sdk --test inherited_workspace --no-run
# The final static test executable is mounted read-only; the owned volume is ext4.
docker run --rm --network none --mount type=volume,source=layerfs-issue284-phase-a-native-01,target=/proof --mount "type=bind,source=$PWD/core/target/aarch64-unknown-linux-musl/debug/deps/inherited_workspace-62b585d173875175,target=/check,readonly" -e LAYERFS_TEST_BACKING_ROOT=/proof -e TMPDIR=/proof alpine:3.21 /check phase_a_ --nocapture --test-threads=1
core/target/issue284-phase-a/build-linux.sh clippy --manifest-path core/Cargo.toml --locked --target aarch64-unknown-linux-musl -p layerfs-workspace -p layerfs-fuse -p layerfs-daemon -p layerfs-sdk --all-targets -- -D warnings
```

Extra Linux Clippy **PASS** covers actual Linux product code and external targets.
Boundary guard:356 production files, including 999-line/200-line/purity checks;
tool self-tests:9 PASS. No CI, retired preflight or substitute aggregate pre-push
gate ran. Routine test telemetry is preserved as test output; it is not a Phase B
sample, cache/memory qualification or a release number.

## Exact per-commit production LOC

Method: `python3 tools/production_loc.py --json --root <snapshot>` with the same
counter on exact first-parent and final staged Git source snapshots. Scope:
reference/core first-party Rust, nested API core/sdk and shipped SQL; exclude
comments/blanks, inline/test-only code, tests/examples/fixtures, tools,
benchmarks/docs, manifests and generated artifacts. Before/after comparison for
`671f4a46f8f354cf764d3d1555529b73a5c939e0` is parent `1ab1501c9` and staged/committed tree `ed5d82c664c9720a49d609cfbbb43d632adf890e`; the committed
tree was confirmed to match. No reference relocation or retirement occurred.

| Commit | Reference before→after/delta | Core before→after/delta | Combined before→after/delta |
| --- | ---: | ---: | ---: |
| Foundation `05eb5c148` | 65417→65417 (+0) | 58706→58705 (-1) | 124123→124122 (-1) |
| Bootstrap `db17f92ea` | 65417→65417 (+0) | 58705→58705 (+0) | 124122→124122 (+0) |
| Scoped C1 `4ea80fc17` | 65417→65417 (+0) | 58705→58725 (+20) | 124122→124142 (+20) |
| Internal v2 `99ee82e57` | 65417→65417 (+0) | 58725→59135 (+410) | 124142→124552 (+410) |
| Partial handoff `1ab1501c9` | 65417→65417 (+0) | 59135→59135 (+0) | 124552→124552 (+0) |
| Complete active port `671f4a46f` | 65417→65417 (+0) | 59135→70022 (+10887) | 124552→135439 (+10887) |
| This docs/evidence handoff | 65417→65417 (+0) | 70022→70022 (+0) | 135439→135439 (+0) |

Growth is the selected physical active mechanisms, capture/custody/lease owners
and their callers, with Core legacy namespace helpers retired. It is not an
algorithmic shrink claim or the donor's production total. The handoff commit
records its exact staged tree in its commit message; its final head/tree are also
published on PR #285 and issue #284.

## Phase B selection/dependency list — all ON HOLD/NOT_RUN

The frozen [benchmark layout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BENCHMARKS.md)
and [baseline inventory](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BASELINE-20260929.md)
remain authoritative for future case IDs, schedules, exact oracles and baseline
roles. No runner/registry was reorganized and no new timing baseline collected.
Each later selection depends on prospective source/control/harness/workload/
oracle/cache/host/profile/image identities and appropriate release builds.

| Future family/selection | Preserved dependency/limit/oracle | Status |
| --- | --- | --- |
| `workspace_write` 3x3 append/dispersed/repeated x100/512/4097 | Unchanged 10 MiB master; full bytes and cleanup; original15/25 s complete limits; one construction worker | ON HOLD/NOT_RUN |
| Original #248 `issue248-separated-4097-v1` | Separate8194-byte fixture and original oracle, not a matrix substitute | ON HOLD/NOT_RUN |
| Native8192/default8MiB, actual10240 boundary, prospective distinct SDK8192 | Preserve native/public distinction; smaller NodeName may change capacity; do not manufacture the old refusal | ON HOLD/NOT_RUN |
| 64 MiB lowering and reordered/duplicated ordinary Base copies | Original67,108,662 final bytes/eight literal replacement bytes across three edits and original functional bounds; independent old/new/G2/pinned oracles | ON HOLD/NOT_RUN |
| Occupied2MiB Stage/4MiB Commit and retained-generation `workspace_commit` controls | Original quota/physical/same-fund/refund/old-pin oracles; clean/one-edit15 s complete bounds | ON HOLD/NOT_RUN |
| `workspace_namespace`, `workspace_mutations`, full live-failure/cleanup | Full registered namespace/mixed/failure cohorts; actual active page work distinct from C1 visits and legacy m-* counters | ON HOLD/NOT_RUN |
| `workspace_shell_package` and broader many-file scope | Existing package IDs/oracles preserved; no package/large cardinality qualification by small tests | ON HOLD/NOT_RUN |
| `history_retention`: history-stride10/3/1 |17/53/157 states, existing C1/C2 backend and separate profile | ON HOLD/NOT_RUN |
| `init_namespace` SDK performance tiers | Existing release-only driver/verifier, original source/inventory oracle and Init worker exception | ON HOLD/NOT_RUN |
| Numeric/cache/memory qualification | Additional host memory remains #283; no RSS/VM/backend/device/cache admission from logical Budget or raw test telemetry | ON HOLD/NOT_RUN |
| Separate earlier control `48b51e874a41b3e1e6c6661e145316df8b408f07` | Different history, not T0's descendant or renamed T0 arm | NOT_RUN |

Implementation concerns for Phase B: complete scaled selection admission is not
proved; legacy external selections with v1-specific descriptors or old private
page-count expectations need explicit source-aware adaptation before execution,
without relaxing their semantic byte/custody oracle or rewriting old receipts.
The shared native fixture now recognizes both save versions. P0 Host Exec timing
wrappers/aggregated status counters were excluded; a prospective harness must
observe available active a-* data separately and mark unavailable counters as
unavailable, never zero. Per-file upload/C5 arrays remain O(E), not a streaming
completion claim. The old32KiB pinned SDK-read Io remains unresolved; finite
16KiB live byte checks do not prove128KiB support. #248 >65535-run/streaming/
Commit-progress, #256 many-file scale and #276 canonical parent indexing remain
uncompleted scopes. Historical numeric rows remain **INELIGIBLE**.

**Stopping point:** implementation ready for Phase B evaluation. Construction
stops after this handoff/push/PR and issue update. The owner starts evaluation in
a separate assignment. PR stays draft; no merge, ready-for-merge conversion,
issue closure or release admission is authorized or claimed.
