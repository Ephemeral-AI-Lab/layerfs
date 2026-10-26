# #463 implementation record

> **Status:** In progress; source/check record for the layer split. Not a release
> contract or performance qualification. Steps 3 and 4 have not started.

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
  → [native_discovery.rs](../../../../api/binding/tests/native_discovery.rs).
- `tests/unit/admission.rs` (two tests: initial admission and deadline Busy)
  → [native_admission.rs](../../../../api/binding/tests/native_admission.rs).
- `tests/unit/reclaim.rs` (five tests: gone-container cleanup, unrelated 404
  custody, replacement-number custody, stopped-container deletion, failed
  deletion custody) → `native_discovery.rs` above.

The lock regression uses a bounded test-only preload fixture to fork while the
actual binding owns the lock. The child retains its open file description while
a later exported binding call acquires the released lock. The admission deadline
proof pays the full public 60-second wait; no private shorter wait or test-only
product API is introduced. [The test guide](../../../../api/binding/tests/README.md)
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

The three implementation files move to
[`layerfs-project::store`](../../../crates/layerfs-project/src/store/mod.rs).
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


## Relocation coordination

All five branches named in the spec still have unmerged `core/crates` changes:
`issue442-fs-mode`, `fix/merge-lost-link-count`, `fix/merge-replaced-directory`,
`pr424-log-cap` and `claude/amazing-borg-cc92de`. The first three have open PRs
#445, #418 and #419; #424 remains open under its remote fix branch. The local
Claude worktree also remains registered. No branch is rebased or edited by this
task. After this inventory was reported, the owner confirmed: “Their owners are
ready to rebase; relocate now.” Step 3 is authorized after step 2 verification;
these branches will need rebasing, and this task does not edit them.

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

Each LOC comparison uses the unchanged root `tools/production_loc.py --json`
from exact parent/staged snapshots, excluding comments and inline tests and
including shipped SQL and the JavaScript API. Staged tree hashes are checked
against the resulting committed trees. Counts are source-size comparisons only.

reference absent (retired R9c; last counted 65,417 at tree 498dd1917812).

Build/test logs and exact LOC receipts are retained under the registration's
`builds/` and `artifacts/` directories outside Git. No benchmark is run.
Durable execution is NOT_RUN — disabled by the owner until explicit
reauthorization. No caches have yet been released; active private targets remain
warm until handoff.
