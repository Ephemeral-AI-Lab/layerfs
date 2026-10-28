# #463 implementation record

> **Status:** Current implementation record; layer split complete with the
> owner-authorized FP-21 verification exception. Not a release contract or
> performance qualification.

## Owner decisions

The owner instructed proceeding with [the migration spec](MIGRATION_SPEC.md)
(G1). In this task's conversation the owner then selected: “Allow project →
bridge; name it engine; relocate now once branch conflicts are resolved.”
This records G2 (Project may use Bridge provisioning types), G3 (`engine`) and
G4 (relocate after branch conflicts are resolved). No push, PR or merge is
requested. Work is isolated on `issue463-layer-split`, from local main
`c137267f59`, with registered private Cargo resources.

## Prerequisite — binding test placement

The unmodified parent boundary guard failed on four existing test-only
attributes in `api/binding/src/discovery.rs`, `pool.rs` and `sandbox.rs`.
The owner explicitly selected fixing their placement in a separate prerequisite
commit. Commit `05db985693` removes the private test wiring and exercises the
ordinary built addon through Node's exported methods instead.

Removed test locations, with all eight behaviors covered at their new homes:

- `discovery.rs::tests::a_dropped_guard_releases_the_lock_while_a_forked_child_holds_a_copy`
  → [native_discovery.rs](../../../../engine/crates/layerfs-node/tests/native_discovery.rs).
- `tests/unit/admission.rs` (two tests: initial admission and deadline Busy)
  → [native_admission.rs](../../../../engine/crates/layerfs-node/tests/native_admission.rs).
- `tests/unit/reclaim.rs` (five tests: gone-container cleanup, unrelated 404
  custody, replacement-number custody, stopped-container deletion, failed
  deletion custody) → `native_discovery.rs` above.

The lock regression uses a bounded test-only preload fixture to fork while the
actual binding owns the lock. The child retains its open file description while
a later exported binding call acquires the released lock. The admission deadline
proof pays the full public 60-second wait; no private shorter wait or test-only
product API is introduced. [The test guide](../../../../engine/crates/layerfs-node/tests/README.md)
describes the requirements. JavaScript API test targets remain outside CI under
the owner's existing exclusion.

Checks passed: Linux Node addon build; test compilation; six native discovery
tests in 0.61 s (30 s ceiling); two native admission tests in 60.52 s (90 s
ceiling); Node all-target warning-denying Clippy; workspace Rust formatting;
full product boundary guard; 63 Python tool tests (120 s ceiling); changed
Python files formatted with Ruff 0.17.2; whitespace check. These fixtures open no
Store. The registered Linux target received Node 24 from the pinned runtime
image for the external calls.

Production LOC: 102158 -> 102158 (delta 0).
Core 96788; API 5370; all other live product scopes absent. The two-line
reduction from the spec's 102160 snapshot predates this work (merged #436).

## Step 1 — layer guard

Commit `2775b3fa44` maps all fifteen Rust crates, including `layerfs-node`, to
layers. It checks production/build/dev edges (including target tables and
aliases), resolved test includes and core rusqlite dependencies. Unknown crates
fail closed. The exact production allowlist and `DOMAIN_CRATES` remain in place.

The temporary exception is daemon → API, for its dev-dependency and exactly the
six includes from `merge_report`, `concurrent_commits` and `lane_merge` to the
two named API helpers. Step 2b must remove it.

Checks passed: full guard (953 product inputs), 63 Python tool tests under a
120 s ceiling, Ruff 0.17.2 format/check for the guard and its tests, whitespace.
Rust/runtime checks do not apply to this tooling-only step. No tests deleted.

Production LOC: 102158 -> 102158 (delta 0), same subtotals as the prerequisite.

## Step 2a — Project Store composition

Commit `0f442d5bb7`: the three implementation files move to
[`layerfs-project::store`](../../../../engine/crates/layerfs-project/src/store/mod.rs).
Their bodies remain unchanged. Project gains the approved Bridge and
Persistence production edges; the Node binding calls this Project Init directly.
`layerfs-api` retains its public names as re-exports and its Control owner calls
Project's install operation. Namespace Init retains its existing public API.
Project leaves `DOMAIN_CRATES`; the exact first-party allowlist and direct
third-party database-driver refusal remain enforced.

Existing API Init and local-install tests still drive Init through the API, so
they stay in API as the spec requires. No tests are deleted in this step.
The full all-target test/example build and warning-denying Clippy passed for
all fifteen packages. Source comparison confirms that the moved files differ
only by the declared imports. The full guard, 64 Python tool tests, Rust
formatting, Ruff formatting and new local documentation links passed.

Content, Telemetry and Storage package tests passed under individual 100-second
execution ceilings. An additional package-specific compile was interrupted
before any Persistence test ran, after observing redundant feature-unification
rebuilds; its receipt is retained. The run then selected the exact executable
identities from the successful full-workspace build, in fresh Linux containers
with private ext4 scratch, 100-second execution ceilings, and FUSE capabilities
where required. Project and API targets passed.

Verification stopped on `layerfs-daemon::fp21_detach_stages`: both OPEN and
READDIR cases failed their five-second internal observation of a request held
inside its provider read. The binary returned failure in 10.14 s; it did not
reach the command ceiling. The final execution plan records 234 passed executable runs, one failed run,
112 selections reusing completed Content/Telemetry/Storage and unaffected native
binding proofs, and no unrun selections.

The test and all of its runtime/fixture code are byte-unchanged by this step.
Its fixture calls namespace Init directly, not the moved Store Init route.
`Store::new(cache_bytes=0)` disables only the shared canonical cache, while
`ReadSession::cached` separately retains up to 3 MiB of pack bodies, decoded
groups and values per Workspace reader. The fixture warms that reader before
arming its provider gate. Those caches can prevent the assumed provider call;
this is the source-based explanation, without a rerun or a product change.
The failed container, scratch volume, original output and execution plan are
retained. The owner explicitly authorized: “Preserve the FP-21 failure and
continue with an explicit verification exception.” The original result remains
FAIL; the test is neither weakened nor rerun. All remaining selections subsequently passed. The literal Node native build,
paired Linux SDK bundle build and sealed SDK `init_refusals.mjs` proof also
passed (the latter under a 90-second ceiling).

Production LOC: 102158 -> 102162 (delta +4). Core 96794; API 5368,
using the existing physical scopes before relocation.


## Step 2b — daemon test ownership

Commit `3cae8b1a1e`.

Eight test targets move from daemon to API: `mount_control`,
`kernel_mount_routes`, `kernel_application`, `observed_application`,
`r5_1_actual_daemon_commit`, `merge_report`, `concurrent_commits` and
`lane_merge`. Their assertions survive unchanged. The first five keep cheap
path includes into lower-layer daemon fixtures; the last three now include
API's own helpers locally. Three targets locate the separately built ordinary
daemon binary through a small API test helper, since Cargo's binary variable
is package-local.

Daemon's `host_install` and `host_handoff`, their three shared install/Init
fixtures, and the `resource_prepare` example use Project Store composition
instead of API. API gains daemon/FUSE and the already locked nix test edges;
daemon drops its API dev-dependency. No dependency version changes. No tests
are deleted. The boundary guard removes both temporary upward exceptions, and
its self-tests require daemon → API to fail in every dependency/include case.
CI's mount lane follows the four Linux-only moved targets into API, while
`mount_control` remains in the heavy group at its new home.

Checks passed: all fifteen packages' all-target no-run compilation and
warning-denying Clippy, literal native addon build, full boundary/fuser guards,
64 Python self-tests, Rust formatting, changed Python Ruff formatting and
whitespace. All 121 focused Project/API/daemon executable selections passed;
225 unaffected selections reuse their existing proofs. The unchanged FP-21
selection retains its original FAIL under the explicit owner exception, without
a rerun, and no selection remains unrun. The moved `observed_application`
proof took 6.30 s; its SLOW timing is recorded while it stays in the mount lane.

Production LOC: 102162 -> 102162 (delta 0). Core 96794; API 5368,
using the physical scopes before relocation.


## Step 3 — relocation

Commit `ce2c0eb411` moves nine engine crates and the Rust API, the workspace
manifest/lock, nextest configuration, tools and authorized fuser package/patch
records to their selected roots. The four core crates remain in place. Relative
manifest paths, cross-crate test includes, native scripts, worktree resources,
CI paths and the LOC counter follow the new layout. The redundant old core
counter is deleted after verifying no live caller outside historical docs.

The exact comparison checks all 954 production files: identical body bytes and
per-file LOC, with 572 files relocated. Cargo metadata preserves all fifteen
packages, their versions/features and every first-party edge, kind, target,
alias and dependency feature. The lockfile and third-party source bytes remain
unchanged. Every moved file is recognized with `git diff -M20%`; the two path
manifests' extensive rewrites fall below Git's default similarity threshold.
This is a source relocation, not a simplification.

The full root all-target no-run build and warning-denying Clippy pass for all
fifteen packages. Linux executable verification records 346 passed selections,
one retained FP-21 failure and none unrun, under individual 100-second ceilings.
The fixture-seal guard initially failed because it still required
`core/Cargo.toml`; this additional live path assumption was not in the spec's
initial inventory. Its repaired case,
`no_product_source_can_reseal_during_an_ordinary_test_run`, finds the root
manifest and scans all relocated first-party product/tooling roots. All original
assertions remain; the five unchanged cases reuse their initial passing proof.
The original failure receipt is retained. No tests are deleted in this step.

Also passed: literal native build script; paired daemon/addon SDK bundle build;
sealed SDK `init_refusals.mjs` under a 90-second ceiling; full boundary guard
(954 inputs); fuser provenance (33 manifests/locks/configurations); all 154
combined Python tool tests under 120 seconds; Rust formatting, Ruff 0.17.2
formatting and whitespace. The selected Linux build/runtime images and private
volumes retain their registered identities. Only this worktree's target record
was updated and prepared; no other worktree's record was migrated.

Production LOC: 102162 -> 102162 (delta 0), using the new counter on both exact
parent and staged snapshots. Per-crate totals are identical. Layer subtotals:
core 38241; engine 56640; API 7281. No upward guard exception remains. The
runtime FP-21 exception remains explicitly separate from the passing checks.

## Follow-up — the Rust leaves `api/`

On 2026-10-27 the owner rejected the `api/` layout above and directed moving its
Rust out. Commit `b98ac4c2f7` moves `api/layerfs-api` to
`engine/crates/layerfs-api` and `api/binding` to `engine/crates/layerfs-node`
(the addon now builds at `engine/crates/layerfs-node/layerfs.node`), and
flattens the packages: `api/agent/sdk` → `api/agent`, `api/agent/cli` →
`api/cli`, `api/how-to-use` → `api/docs`. `api/` now holds only the published
JavaScript packages. The guard and the LOC counter classify `layerfs-api` and
`layerfs-node` as engine by crate name in both layouts, so the earlier records
above keep their original API subtotals.

Production LOC: 102162 -> 102162 (delta 0); all 954 files keep their per-file
counts. Under the name-based classification the subtotals are core 38241,
engine 61288, API 2633. Checks: identical Cargo metadata, macOS all-target
check and formatting, the boundary guard, 155 Python tool tests, Ruff, the
JavaScript stand-in tests and a sealed Linux release bundle. dsh ran all ten
scenarios on that bundle; the sandbox scenario's unmount race was a dsh-layerfs
scenario timing issue, fixed there, not a LayerFS change.

## CI timing record

Commit `ef4c26b02b` records the existing Persistence `writer_admission` target's
9.74-second Linux functional run in SLOW, as repository policy requires. It is
separate from the path relocation and changes only CI selection data. All six
CI-selection/executable self-tests pass under a 120-second ceiling, as do Ruff
formatting and whitespace. Rust/runtime checks do not apply to this data-only
change; the completed relocation proofs remain applicable. No tests deleted.

Production LOC: 102162 -> 102162 (delta 0); core 38241, engine 56640, API 7281.

## Step 4 — documentation

The documentation commit containing this record splits the implementation guide
into core, engine and API ownership. Common source-size, verification, profile,
execution and failure rules remain binding across all product layers. Engine
owns daemon/FUSE, the authorized fuser patches, Persistence seal control and
Project Store composition, including the approved Bridge/Persistence edges.

Maintained guides, crate READMEs, current design navigation and native proof
instructions follow the new paths. All six named SDK skill references were
checked: five needed edits; LWW still points to the surviving core Content
source. The #337 receipt path retains its historical source identity. Archived
queue-scheduling documentation and dated records are unchanged. Links to retired
source in maintained guides now use their verified immutable source commits;
claims and original qualification limits are preserved.

Local links/anchors, status banners, command paths and whitespace are checked.
Rust/runtime checks do not apply to this docs-only commit. Production LOC stays
102162 -> 102162 (delta 0), with the same layer subtotals.

## External consumers

No outside repository is changed. layerfs-benchmark's `common/lib.sh` still
hard-codes `core/target/fs-bench-bin`; fs-bench-pro remains frozen and unrun.
dsh-layerfs and magpie-layerfs retain their pins; their next pin bumps must build
from the new root manifest. No follow-up issue, push, PR or merge is requested.


## Relocation coordination

All five branches named in the spec still have unmerged `core/crates` changes:
`issue442-fs-mode`, `fix/merge-lost-link-count`, `fix/merge-replaced-directory`,
`pr424-log-cap` and `claude/amazing-borg-cc92de`. The first three have open PRs
#445, #418 and #419; #424 remains open under its remote fix branch. The local
Claude worktree also remains registered. No branch is rebased or edited by this
task. After this inventory was reported, the owner confirmed: “Their owners are
ready to rebase; relocate now.” Step 3 proceeded after step 2 verification;
these branches need rebasing, and this task does not edit them.

A fresh read of all open PR file lists also finds `core/crates` edits in:

- #402: `feat/sandbox-local-backend`.
- #403: `macos`.
- #415: `issue379-capacity-guard`.
- #418: `fix/merge-lost-link-count`.
- #419: `fix/merge-replaced-directory`.
- #424: `fix/issue-423-sandbox-log-cap`.
- #427: `feat/macos-native-on-main`.
- #435: `fix/write-through-non-directory`.
- #441: `fix/reclaim-busy-retry`.
- #445: `issue442-fs-mode`.
- #448: `fix/stat-symlink-size`.
- #452: `windows-native-store`.
- #459: `codex/docker-api-diagnostics`.

Those remote branches will also encounter the path move on their next rebase.
This is coordination inventory only; their work is not edited or merged here.

## Counting and retained evidence

The prerequisite and steps 1/2 use their unchanged root
`tools/production_loc.py --json`. Step 3 uses the new counter on both layouts,
classifying crates by name; later commits use that same counter unchanged.
All comparisons use exact parent/staged snapshots, exclude comments and inline
tests and include shipped SQL and the JavaScript API. Staged tree hashes are checked
against the resulting committed trees. Counts are source-size comparisons only.

reference absent (retired R9c; last counted 65,417 at tree 498dd1917812).

Build/test logs and exact LOC receipts are retained under the registration's
`builds/` and `artifacts/` directories outside Git. No benchmark is run.
Durable execution is NOT_RUN — disabled by the owner until explicit
reauthorization. Cache release and final disk evidence are recorded below.


## Cache/evidence handoff and disk

The exact cache releases are exported in `artifacts/cache-release-export.json`
and the protected derived scope under registration
`issue463-layer-split-0e1ca01332b4`. The cleanup plan/apply procedure removed
this assignment's private target and Cargo dependency volumes:
`lfs-dev-issue463-layer-split-0e1ca01332b4-target` and
`lfs-dev-issue463-layer-split-0e1ca01332b4-cargo`. The repaired fixture-seal
failure's verified-empty scratch volume was also removed. The old host
`core/target` and new host `target` directories contained no build cache and
were removed only after verifying they were empty. Other worktrees, their warm
caches and resource records are untouched. Prepare this registration before a
future build; sealed SDK bundles remain available outside the caches.

Four completed stopped containers were archived and retired. The original
cleanup attempt retained three after Docker reordered its Mounts array between
plan and apply. Inspection confirmed equal full identities after sorting that
array; a task-local wrapper canonicalized its order without dropping identity
fields or changing repository cleanup code. The original partial report and
subsequent successful report are retained. No active container was retired.

The original FP-21 output, FAIL receipt, configuration and logs remain retained.
Its original executable was archived and verified byte-exactly before the cache
was released: SHA-256
`a3f8aed65b262fe920c9fd34bdf0ca96cd0805d39d5cb3991b0e060d5bba0dba`.
Recovery tree `f3dcbed8566d0f8311949d74fcfcb4b702ff7de5` is retained at
`refs/cleanup/20261026T190651Z-21eeb1b8bb82/197f54e38b82bd8d6efa`.
Its original scratch volume
`lfs-dev-issue463-layer-split-0e1ca01332b4-c4033190aba64b258223c701b3e1175c-tmp`
remains protected until its owner resolves the staging assumption. Archival and
cache disposal do not change the original FAIL or authorize a replay.

Settled host free space at 2026-10-26T19:12:32.054565+00:00: 367572975616 bytes
(367.57 GB). Docker guest free: 95400038400 bytes
(88.85 GiB). The cache cleanup
initially observed only 765952 net host bytes; after delayed
reclamation the observed net was 60979404800 bytes. These are
free-byte observations including concurrent activity, not allocation estimates.
Host space stayed above the 100 GB threshold; no 300 GB recovery episode was
triggered. Cleanup reports record no lost pins.
