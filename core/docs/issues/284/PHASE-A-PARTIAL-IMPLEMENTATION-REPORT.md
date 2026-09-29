# #284 Phase A: partial integration, implementation NOT ready for Phase B

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owned branch `codex/issue284-phase45-integration` at product commit
> `99ee82e5790a2dc20f5db0dfd37d645a06265019`, product tree
> `6875b3e21bf6ea951a2b18ca623c2f2fe13e4f8c`.
> Corrected base T0 `05eb5c14849f1b874383fd1600ba9288f103ad93`;
> donor P0 `11a864fc133844cae7a4247b1f243d84d5763b10`;
> frozen design `f9f9abb37332e23d1968a5ed204ff70e3e66d783`.
> Review target `codex/issue264-phase45` remains
> `6eb7553671d3000160ad023a57b335e52dd81a26` at this checkpoint.
> Docker/image: **NOT_BUILT**; no application/Sandbox/FUSE live image or
> benchmark receipt was created. Rust checks used this worktree's `core/target/`
> and repository-root `.cargo/config.toml` AEAD profile, without a RUSTFLAGS override.

## Outcome and precise unfinished seam

**Not ready for Phase B evaluation.** This report does **not** complete the
selective active Workspace port. The edited product has two independently
runnable prerequisites (C1 ordering repair and authenticated v2 internal file
save), but **still uses T0's legacy private RootOwner on LocalEdit**.
`State`/`View` have no active selection; no `backing/active/` owner, funded G1
capture, live G2, C5 affected install or public selected-view leases have been
integrated. No new mode, fallback, fake carrier, path-bearing donor namespace or
partial active write acceptance was introduced. Consequently, new LocalEdit
**does not yet** check version-2 capability at attach, and v2 availability on
the Service does not imply active Workspace compatibility. There is no new
capability blocker or approved-format question: implementation remains undone.
This is a deliberately bounded unfinished handoff, not an implementation-ready
claim; do not start Phase B from it.

The next executable implementation step is to add P0's path-independent active
index/page/pack/extent/HotRef/retirement plus precharged completion and active
attachment into **T0's** component-based State (without copying P0 state).
Then adapt selected serial/component resolution and all ordinary namespace and
content callers, wire a single coherent G1/G2 capture/upload/C5 route, verify
exact capability at LocalEdit attach *before* admitting dependent mutations,
and only then add identity-based leases and public control. Do not permit active
writes until capture and failure custody are executable together. At the final
state, run the owning Core and narrow live external checks before a separate
owner-directed Phase B task.

## Source-pinned reuse/adapt/exclude manifest

| Owner/files | Decision at this checkpoint | Source and conflict |
| --- | --- | --- |
| `layerfs-content/src/filesystem/update.rs`, `validate.rs`, `validate/cycles.rs`, `tests/filesystem_ordering.rs` | **REUSED** selected scoped repair and external proof | Exact `fbda0f0f1dbb6bb3ddd375694e3ea7edbab62ebd` on matching T0 inputs. No C2, History or parent-index schema change. |
| `docs/architecture/04-filesystem.md` | **ADAPTED** alongside the C1 algorithm | Same repair pin; names declared-new unreachable parents, per-serial declared demand, conservative resident walk. |
| Bridge `contract/{request,outcome,history}.rs`, `adapters/native/{client,connection,protocol/metadata,protocol/response}.rs` | **ADAPTED** internal capability 28, v2 29, response classification/codec and actual read timeout | P0 `11a864fc`; T0 opcode 20 unchanged. The donor's lease opcodes/entry wire were deliberately not imported before selected Workspace ownership exists. |
| Service `service/{handler,save/content,save/file_stream,save/file_stream/origin_runs}.rs` | **ADAPTED/REUSED** v2 parser and bounded, Service-local authenticated Base origin resolution | P0 `11a864fc`; no recursive daemon ReadFile, public ioctl/range API or Base-sized spool. External direct test includes v1 canonical identity control, denial, malformed versions, exact bytes. |
| Sandbox `session.rs` | **ADAPTED** same-call deadline retention | P0 `11a864fc`; no retry of a lost mutation. |
| Workspace `runtime/state.rs`, `runtime/ancestry.rs`, `filesystem/namespace{,_view}.rs`, `filesystem/rename.rs` | **RETAINED**, NOT PORTED | T0 `05eb5c148` NodeName, charged complete ancestry (LocalEdit and ReadOnly), identity reads and one-seal rename. P0 `NodePath`, extended paths, descendant scan, resident path rewrite and `rename_paths.rs` are **EXCLUDED**. |
| P0 `backing/active/*`, `commit/active*.rs`, `overlay/snapshot.rs`, `filesystem/active*.rs`, `runtime/view_leases.rs`, API/daemon/Bridge lease owners | **DEFERRED: NOT INTEGRATED** | Their algorithms are a future selective reuse, not a passed Workspace route. Must remove donor View/directory-handle/HeldEntry full locators and adapt rename under the final State gate. Do not copy shared owners wholesale. |
| C2, History, native FUSE and benchmark registry/runner files | **UNCHANGED** | No schemas, worker/quotas/deadlines, fixture scheduling, route selection or release admission changed. |

The donor's unverified pending readable fixture in another worktree was **not**
imported. External typed capability here uses
`Response::FileSaveCapabilities { version: 2 }`, **not** `maximum_version`.
Frozen prior algorithm control `48b51e874a41b3e1e6c6661e145316df8b408f07`
is a different history, remains **NOT_RUN**, and is not T0's descendant.

## What is implemented and proved, and what is not

- C1 retains cycle/reachability and alias validation. Only declared-new
  non-root parents occupy unreachable-parent membership; declared rows/changed
  names/prefetch demand use `maximum_touched_serials()`; decoded memo and
  effective walks remain charged at `ordering_bytes / 1024`. C1 canonical
  format and ordering-memory admission were not relaxed.
- v1 SaveFile opcode 20 remains accepted; authorized capability request 28
  returns exactly typed `version: 2`, save 29 demands a body version byte 2.
  Parser/Service validates a fixed-record origin spool, uses <=64 KiB bounded
  authenticated Base reads while holding its session, and validates input before
  C2 publication. Direct external byte proof covers repeated/backward Base,
  malformed version/body, v1 refusal and equal resulting canonical roots.
- Read-only native socket read deadlines remain `Deadline`, missing mutation
  results remain `Unknown`; expired held-session Hello is not reconnected under
  a new deadline. No replay on unknown outcomes. The new external TCP fixture
  withholds a reply after the peer receives the request and observes both types.
- No accepted active revision, G1/G2 Base coordinate preservation, funded
  completion, physical refund, C5 reconciliation, selected old-view authority,
  token/response Budget, checked lease release, live public SDK/daemon mount,
  or real FUSE evidence exists **at this source**. T0's existing generic Exec
  remains unchanged; it does not demonstrate any active provenance route.
- The initial new TCP test used the Workspace profile for a SaveFile request,
  causing the request to be rejected before peer delivery; the fixture was
  corrected to profile 1. The new codec test first indexed the whole envelope
  as if opcode were byte 0 instead of byte 26 and then expected Capacity for
  truncated metadata instead of actual InvalidInput; both expectations were
  corrected. Retained failed command outputs stay in worktree-local
  `core/target/issue284-v2-test.log`, `issue284-v2-codec*.log`; no failure
  was promoted to a pass. These are *test fixture* corrections, not product
  gate relaxations.

## Exact correctness checks (not performance samples)

| Selection / product source | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_ordering` at `4ea80fc17` (C1 files unchanged thereafter) | 18 PASS, 0 ignored. |
| `cargo +1.85.1 check --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-server -p layerfs-sandbox` during v2 construction | PASS. |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-bridge --test file_save_v2 --test native_connections -p layerfs-server --test direct` at `99ee82e57` | 1 + 6 + 10 PASS, 1 existing explicitly ignored 4097 Service test **NOT_RUN**, not a live proof; `core/target/issue284-frozen-compatibility.log`. |
| `cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples` at product tree `6875b3e2` | PASS (compiles only; not a live SDK/FUSE run). |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS. |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings` | PASS. |
| `python3 core/tools/check_product_boundary.py` and `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS, 319 product files; 9 Python tests. |
| `git diff --check` | PASS. |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace` | **NOT_RUN**: there is no frozen *integrated* implementation; full owning-workspace validation must follow the active port. Do not mislabel partial checks as this required check. |

All scratch/logs and incremental binaries are under this worktree's
`core/target/`; the Cargo tests produce host binaries only, **no image ID**.
No full registered Phase B selection was executed. The above cannot support
numeric admission or an end-to-end Workspace Commit claim.

## First-parent and staged-tree production LOC custody

Method for **each** commit: `python3 tools/production_loc.py --json --root
<snapshot>` on a `git archive` of the exact first parent and staged tree under
`core/target/issue284-loc/`. Count first-party product `crates/*/src`,
`core/crates/*/src`, their shipped SQL, nonblank/noncomment source only;
exclude tests, docs, examples, fixtures, tooling and inline test branches.
This target tree is counted, **not** P0.

| Commit | Parent -> staged tree | Reference | Core | Combined/delta |
| --- | --- | ---: | ---: | ---: |
| Existing foundation `05eb5c148` | `6eb755367` -> `8d00c2c32cf650005228b270bf8566c859af9d58` | 65,417 -> 65,417 (+0) | 58,706 -> 58,705 (-1) | 124,123 -> 124,122 (-1) |
| Bootstrap `db17f92ea` | `05eb5c148` -> `55d9187fdeca2dec64ce7b012593db1ed1451ff6` | 65,417 -> 65,417 (+0) | 58,705 -> 58,705 (+0) | 124,122 -> 124,122 (+0) |
| Scoped C1 `4ea80fc17` | `db17f92ea` -> `b65162e43051da54e4fbb0c61ca43ea857da379a` | 65,417 -> 65,417 (+0) | 58,705 -> 58,725 (+20) | 124,122 -> 124,142 (+20) |
| Internal v2 `99ee82e57` | `4ea80fc17` -> `6875b3e21bf6ea951a2b18ca623c2f2fe13e4f8c` | 65,417 -> 65,417 (+0) | 58,725 -> 59,135 (+410) | 124,142 -> 124,552 (+410) |
| This docs-only report | `99ee82e57` -> report staged tree (recorded in its commit) | 65,417 -> 65,417 (+0) | 59,135 -> 59,135 (+0) | 124,552 -> 124,552 (+0) |

T0's `-1` is a real foundation repair, not an active-port simplification.
The +410 includes a new Service owner-format parser and diagnostic
instrumentation, not a numeric performance claim. No reference product code
was relocated or retired in this phase.

## Phase B: ON HOLD / NOT_RUN

Keep the case IDs, limits and oracles of the frozen design
`f9f9abb37332e23d1968a5ed204ff70e3e66d783` and its
[benchmark layout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BENCHMARKS.md).
First dependency for **every** row: a complete, tested, sealed active component
Workspace plus correct image/profile/fixture identities and public route.
Nothing below has a new measured number or a PASS from this assignment.

- `workspace_write`: append/dispersed/repeated x 100/512/4097, original
  `#248 separated-4097`; 15/25 s complete limits, prepared 10 MiB fixture,
  original 8194-byte fixture for #248, independent byte and cleanup oracles:
  **ON HOLD/NOT_RUN**. Donor numeric rows stay **INELIGIBLE**.
- Native 8192/default 8 MiB, 10,240 capacity observation, prospective **SDK**
  8192 as a distinct registration: **ON HOLD/NOT_RUN**. A smaller NodeName can
  change capacity; do not manufacture the historical refusal. The unresolved
  pinned 32 KiB SDK Io observation is not qualified by 16 KiB checks.
- Original 64 MiB lowering with eight literal replacements, exact
  67,108,662 final bytes and 10 s Stage/60 s functional command limits; occupied 2 MiB Stage / 4 MiB Commit headroom,
  same-fund allocation/refund, retained-generation clean/one-edit Commit,
  mixed mutation/namespace and live-failure/cleanup: **ON HOLD/NOT_RUN**.
  Clean/one-edit retained-state controls retain their 15 s complete bound.
- `history_retention`: `history-stride10/3/1`, respectively 17/53/157 states,
  same existing C1/C2 driver, separate history profile: **ON HOLD/NOT_RUN**.
- `init_namespace`: release-only SDK 100/1000 and selected 10,000/100,000
  tiers, package/many-file scale, numeric cache/memory qualification:
  **ON HOLD/NOT_RUN**. No symmetric host-memory qualification here (#283).
- Frozen separate earlier control `48b51e874`: **NOT_RUN**, not a T0 arm.
  Broader >65,535-run/streaming/Commit-progress work stays #248; many-file
  scope stays #256; canonical parent-index work stays #276.

No issue was closed, no PR was marked ready/merged, and no release/numeric
admission is claimed. Continue **Phase A** before the owner can start Phase B.
