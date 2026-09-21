# Issue #192 optimization handoff

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Date: 2026-09-20. Task: continue the agreed bridge simplification, bounded large-load
profile and multi-writer correctness design under
[#192](https://github.com/Ephemeral-AI-Lab/layerfs/issues/192). Read
[the optimization specification](optimization-spec.md) first, then its linked
original packet and repository/core AGENTS. This handoff records a partial,
unqualified implementation and a design revision; it does not mark #192 complete.

## 1. Do not reopen these decisions

- Multiple concurrent writer operations in the same logical Store are required.
  Four connections returning Capacity while one writer works are insufficient.
- Duplicate encoding and physical chunk/pack bytes from identical concurrent
  submissions are accepted for simpler coordination. Do not introduce global
  writer serialization or a chunk reservation service to prevent this waste.
- One construction producer per operation is not one writer per service. Preserve
  the namespace-init exception and the benchmark construction-worker contract.
- Bound application ownership/counts and account for OS memory separately.
  The 256 KiB socket option admission assumption is superseded as design policy.
- Crash-durable acknowledgement, resumable upload and automatic mutation replay
  are not required. Preserve unknown outcomes; do not add WAL, sync calls or spool.
- Keep five operations, local C1/C2, authenticated remote delivery and the common
  authorized direct handler. No FUSE, Workspace, history/Commit or cloud provider
  implementation enters this foundation through this handoff.
- Keep tests short. Separate build/setup costs and native proofs from functional
  test execution and from #193 performance qualification.

## 2. Workspace, pins and ownership

| Item | Value |
| --- | --- |
| Implementation worktree | `/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs` |
| Branch | `codex/pair3-foundation` |
| Original base / C2 source basis | `10b9d4a6cf9d88267d508cb010cc82950e080d77` |
| Published reviewed packet | `21f6af702919c23fafc88890361bef7bfb831140` |
| Published implementation candidate | `5f637f17c17d134deb6b567dfcbd290b112457d8` |
| Candidate PR | [#200](https://github.com/Ephemeral-AI-Lab/layerfs/pull/200), draft; not ready/merged/released |
| Selective C1 checkpoint | Four files from `e54af84653cd1a22b3f26631dbd19eafb895a7b4`, see [checkpoint](optimizer-checkpoint.json) |
| Legacy transport reference | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` |
| Core lockfile | Preserve the file at the candidate pin until an explicit integration-owner change; snapshot SHA is in the inventory below. |

The documentation commit containing this handoff intentionally does not commit
unfinished runtime edits. Its production source is identical to its first parent.
The existing local worktree is dirty with the pending changes described below.
Recheck `git status`, branch, worktrees and source hashes before changing anything;
never reset another worker's work or import uncommitted optimizer source.

Another task owns C1/C2 optimization. **The owner explicitly prohibited further
cross-task communication with that sibling.** Do not message, wake or coordinate
with it. Use the shared resource locks directly. New local subagents, if authorized,
must have disjoint ownership and must not revert others' work. One integration
owner controls core manifests/lockfile and shared public contracts. Reconcile
required C2 schema/API changes through explicit issue/source checkpoints; do not
silently edit another worktree or restore its old algorithms.

## 3. Portable recovery snapshot of the pending work

[Inventory](evidence/optimization-custody-20260920/inventory.json) records selected
file hashes, runtime/manifests/lock inventory and the exact base. The
[unapplied patch](evidence/optimization-custody-20260920/unqualified-worktree.patch)
contains only 25 changed/new core manifest/source/test/example files owned by this
work. It excludes the source checkout, other worktrees, targets, third-party files
and unrelated documentation. Runtime inventory SHA-256:
`1ff1e6d0d2c0846c7f4d306fa535cb937108674043c100aab28e60977df29c88`.

This is **UNQUALIFIED recovery material**, including a known failing socket
experiment. It is not runtime source shipped by the documentation commit. Its
[isolated-index reconstruction check](evidence/optimization-custody-20260920/recovery-check.json)
confirms all 25 reconstructed file hashes; that check proves recovery integrity,
not product correctness.

- In the existing dirty implementation worktree, inspect and match the inventory;
  do not apply the patch a second time.
- In a fresh clean checkout of this documentation commit, the production baseline
  is still `5f637f17c`. First inspect the patch and run `git apply --check` against
  it. Apply only when intentionally reconstructing the pending work; there is no
  automatic restoration instruction for a changed/dirty checkout.
- Preserve raw review receipts under `evidence/review-*`. Their `source.json`
  inventories differ across attempts. A receipt's HEAD alone is not its source
  identity, and successful older binaries do not qualify the reconstructed tree.

Local edits to `core/docs/architecture/14-service-runtime.md`,
`06-m0-decisions.md`, and the untracked `08-review-qualification.md` draft are
also preserved in the original worktree, but are not part of this source recovery
patch or documentation commit. They contain exploratory/older qualification
wording. Reconcile them with the final implemented source; do not stage the whole
dirty packet or treat those drafts as overriding this specification.

## 4. What exists, what failed, what may be reused

| Area | Actual state / next treatment |
| --- | --- |
| Foundation at `5f637f17c` | Three crates and bounded telemetry candidate; original direct/Docker operation proofs exist in [acceptance](07-acceptance.md). It still has global A=1 and old socket-buffer policy. |
| Native socket experiment | **FAIL**. New `socket.rs`, preconnect descriptor/poll/listen code, `net` feature and regression are not a qualified fix. The recommended design returns to ordinary TCP without universal option-size admission. Preserve observations and use the regression for the revised behavior. |
| Ordinary diagnostics | Pending fix uses bounded nonblocking stderr writes and explicit executable exit codes. The established-session full-stderr regression passed; process-launch-budget failures remain separate. Requalify when integrated. |
| Telemetry lifecycle | Pending `Arc<Owner>` fix makes final-handle destruction stop the writer exactly once; submission rechecks closure under queue lock. Concurrent-drop/native checks passed on recorded identities. |
| C1 reference/boundary tests | Pending setup simplification reuses sealed/captured deterministic lengths and immutable prepared Store data; original inputs/assertions retained. C1 production algorithms unchanged by these edits. |
| C2 codec/window tests | Decoder workspace reuse retains every truncation and adds valid decode after errors. Window fixture keeps 131,200 values while batching setup saves at meaningful boundaries. Structured-ID experiment was reverted after being slower. |
| Test compilation profile | Pending test `opt-level=1`, `zstd-sys=3`; debug assertions/overflow checks retained, dev/release profiles unchanged. Build and execution times recorded separately. |
| Full suite | A pre-socket-experiment source/profile passed 512 tests in 15.05 s with compiled artifacts. Three ignored entries were two child helpers invoked by passing parents and real ENOSPC run separately. This is not latest-tree PASS. |
| C2 multi-writer semantics | **NOT IMPLEMENTED/QUALIFIED** by this work. Acquisition, object insert/reuse, save visibility, pack/ordinal allocation, cache scope and cleanup must be redesigned/qualified together. |
| Large-file profile | **NOT FROZEN**. Candidate 64 MiB / 8,192-frame / 10 s guards are not general large-load qualification. No verified MB/s or multi-GiB/concurrent-writer throughput. |

Key receipts, with their limited scope:

- [Socket diagnostic](evidence/review-native-socket-diagnostic-20260920/result.json):
  four of 64 sockets reported 392,384 receive bytes after a successful 128 KiB
  request; the OS internal cause is not established.
- [Preconnect regression](evidence/review-preconnect-focused-20260920/result.json):
  failed at connection zero, Capacity. Do not continue expanding the socket helper
  merely to preserve an unsupported physical-memory claim.
- [Cached workspace command](evidence/review-short-workspace-cached-20260920/result.json):
  exact full command passed before the new socket experiment; fresh-executable and
  earlier slow-target failures are retained, not overwritten or promoted.
- [Four-daemon envelope](evidence/review-envelope-deployment-command-20260920/result.json):
  actual multi-session/A=1 refusal proof; **not multiple successful writers**.
  Its socket-ceiling arithmetic is not proof of total kernel physical memory.
- [Native telemetry](evidence/review-native-final-20260920/result.json) and
  [Linux ownership test](evidence/review-linux-envelope-command-20260920/result.json):
  co-hosted queues/recordings/windows/encoders/closing producers; not a simultaneous
  multi-writer C1/C2 memory proof.
- [ENOSPC](evidence/review-enospc-command-20260920/result.json): disposable guarded
  HFS+ volume, original result preserved and volume detached.
- [Linux scoped native tests](evidence/review-linux-native-scoped-command-20260920/result.json):
  six native cases. One Python-dependent JSON test was omitted from scratch after
  its explicit failed attempt; it passed on macOS and actual Linux output was
  separately parsed on the host. Do not call this an all-tests Linux PASS.

The last recorded complete product-route image before the socket experiment was
`sha256:bab1522c12fd2b5f2dc75b21102f08190ea473faa3f15cb2baf3143afc8d7b0a`,
Linux executable SHA-256
`669060a924842560c4afb56a99ad5bb35cba8bfa47c12eae00b9c000b8752399`.
Forward/off/local/both and short error/slow/cleanup/lost-response proofs exist for
that image and corresponding recorded service binary. Current local host binaries
may instead be from the failed preconnect build. Verify hashes; never launch an
old executable as fallback or label it the current implementation.

## 5. First implementation actions

1. Read the optimization spec, live #192/#181 and actual C1/C2 public APIs. Record
   the source/lock/profile and ownership checkpoint actually selected. The earlier
   concurrency sketches are design input, not qualification of the current APIs.
2. Freeze D01-D05 from the spec. Resolve catalog collisions with unpublished
   writers, bounded final publication/lookup, allocation/ordinals, save-isolated
   cleanup and transaction arbitration. Do not reopen the multiple-writer decision
   or require zero duplicate space. Document any chosen locator-metadata tradeoff.
3. Integrate only the useful pending diagnostic/telemetry/test fixes. Replace the
   failed socket experiment with the simpler carrier design. Preserve version,
   credentials, public outcome semantics and no-replay behavior.
4. Implement C2 ownership/publication prerequisites at an explicit checkpoint
   before enabling concurrent writers in the service. Update schema/open behavior,
   architecture documents and affected correctness cases together. Merely replacing
   the service AtomicBool with a larger counter is unsafe.
5. Integrate a bounded multi-writer service budget, trusted caller boundary, one
   local deadline, session shutdown ownership and qualified large-input profile.
6. Run focused external/native cases, then the required final checks once the
   implementation stabilizes. Rebuild the exact host/Linux artifacts incrementally
   before current-binary proofs. Do not rerun passing selections without a change,
   failure or unresolved concern that justifies it.
7. Update the original matrix plus O01-O11, source pins, actual LOC and remaining
   failures. Publish for review. #192 cannot close on an A=1/direct-only proof;
   #193 performance remains a separate frozen campaign.

## 6. Resource-safe commands and environment

Use the implementation worktree explicitly for every command. Inspect effective
Cargo/environment configuration before running:

```sh
cd /Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs
export CARGO_TARGET_DIR=/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs/core/target
export CARGO_BUILD_JOBS=2
export LAYERFS_CONSTRUCTION_WORKERS=1
```

One build/test owner uses this target sequentially. Build jobs are not product
construction workers. Keep Cargo/Rustup caches and lockfiles; no clean, vendor,
patch, registry edit or legacy executable fallback. Use `--offline` only when all
required dependencies are already available. Intentional manifest/lock changes
belong to the integration owner.

Acquire the global advisory locks in this order, deduplicating resolved paths:
`$TMPDIR/layerfs-infra-measurement.lock`, then
`/tmp/layerfs-infra-measurement.lock`. Use nonblocking flock and back off if held.
Never create or flock private benchmark `.measurement.lock` markers; those use
exclusive-create ownership. Do not communicate with or interrupt the sibling.

Docker resources use unique `layerfs-issue192-*` names and
`io.layerfs.task=issue192`. Product proof profile was one CPU, 128 MiB memory/swap,
16 PIDs, no capabilities/mounts, and log driver none; forward is read-only with no
container reports. Any changed resource profile must be prospectively recorded.
Container isolation does not remove shared host resource interference.

Required final checks, individually (not an aggregate gate):

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Also explicitly select native telemetry, Linux target and real Docker launch
paths; default/portable checks do not qualify those. The existing cross-build
uses target `aarch64-unknown-linux-musl` and local Rust 1.85.1 `rust-lld`; inspect
availability and record the effective linker instead of assuming another host
has that absolute toolchain path.

Read repository benchmark/release rules before measurements. Use fresh append-only
output paths. A compile/test timeout is not PASS. Test-profile optimization and
cached executable reuse are not product performance evidence. Do not run the
retired preflight, create a replacement aggregate gate, or claim CI green.

## 7. Documentation commit and completion report

This handoff/spec commit changes documentation and archives unapplied recovery
material/receipts only. Production LOC must be recounted on its exact first parent
and final index; current expected baseline is reference 65,417 + core 24,349 =
89,766, external adapters zero. The actual report is
[production-loc-optimization-docs.json](evidence/production-loc-optimization-docs.json).
Use the repository counter and stable runtime Rust/SQL classification, excluding
inline/external tests, docs, examples, tooling and the unapplied patch. Record the
real unchanged product total, not zero-sized production. Confirm committed tree
matches the final counted staged tree.

For future implementation commits, count actual changed runtime code, update
architecture and record before/after/delta in every message. End the implementation
handoff with published branch/spec/core/lock/schema/image pins, all V/T/ENV and
O-case dispositions, exact commands and cleanup, LOC per commit, unresolved
limitations and #193 readiness. No performance, durability or completion claim
follows from this documentation checkpoint.
