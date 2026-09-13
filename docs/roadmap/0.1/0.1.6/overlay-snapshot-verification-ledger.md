# Snapshot-Isolated Workspace: append-only verification ledger

Status: Dated planning checkpoint; not release evidence or a product contract.
Companion to [progress state](overlay-snapshot-progress.md); each row is immutable
once written. A later correction is a new row that invalidates the earlier one by
reference, never an edit. Fields follow the handoff's record shape: identity,
source, environment, result, validity.

## Rules used here

- A pass is reused only while its exercised code, fixture/oracle and environment are
  unchanged. A new repository HEAD is not by itself an invalidation.
- No unchanged benchmark is repeated to obtain a better number; no new numerical
  gate is introduced.
- Reporting-only targets stay reporting-only.

## Ledger

### L1 — V1 kernel visibility probe (hypothesis test)

- identity: FUSE write-through cached session, minimal dependency-free probe,
  `MAP_SHARED` writable mapping + stores; modes: no-trigger, mount `syncfs`,
  per-file daemon self-open `fsync`, open-unlinked variant.
- source: `docs/roadmap/0.1/0.1.6/evidence/v1-probe/probe.c`
  (sha256 `bd38ad8020c0011fd68b186dd483e1f31adee4c0d827626f532eeff28d8d608a`);
  log `…/run.log` (sha256 `00d1eb9ee9361facee2fa434715e9112ff6b3494cae71d8526e008226d1cdca3`).
- environment: image `rust:1.85.1-bookworm`, `--device /dev/fuse --cap-add
  SYS_ADMIN`, container kernel `6.12.76-linuxkit`, `gcc -O1 -pthread`.
- result: pass (hypothesis confirmed). Mapped stores are invisible to the daemon
  until an explicit writeback trigger; `syncfs` and per-file `fsync` both deliver
  them; the per-file path fails for open-unlinked inodes; `syncfs` covers them.
  Command and verbatim output in `evidence/v1-probe/README.md`.
- validity: current. No product source is exercised by this probe; it isolates the
  kernel/adapter contract. It is evidence about the platform, not about a candidate.

### L2 — Phase-1 benchmark catalog enumeration

- identity: `fs-benchmark-pro infra-list` (registry enumeration only; no case
  executed), plus `shared/runner.py --family historical_access --list`.
- source: `target/release/fs-benchmark-pro` built from
  `LAYERFS_SOURCE_COMMIT=536aaf9ded4e09a9ccfcf2b251aa4db995b383e6`; raw output
  `evidence/phase1-catalog/infra-list.jsonl`; derived ledger
  `evidence/phase1-catalog/scope-ledger.json`; exclusion manifest
  `docs/roadmap/0.1/0.1.6/benchmark-exclusions-issue122.json`.
- environment: macOS host, no container started, no measurement lock taken.
- result: pass. 260 scenarios / 21 families / 258 supported / 29 proof-only;
  #122 exclusion manifest declares 36 cases, 28 present in the registry
  (`E ∩ U`) and 8 still unregistered; included registry scenarios 232;
  `historical_access` adds 11 family-specific cases.
- validity: current as a *planning* ledger; explicitly provisional for Phase 7,
  which must re-enumerate from the sealed candidate before executing anything.

### L3 — Product source untouched (no invalidation of prior passes)

- identity: whole-repository scope check over the working tree.
- source: `git status`/`git diff` against
  `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b`.
- environment: macOS host, Cargo 1.96.0 / rustc 1.96.0 available; no build run.
- result: pass. The only changes are documentation and evidence; no
  `crates/`, `benchmark/` (harness), fixture, or configuration file is modified.
- validity: current, and it is the reason every previously recorded product pass
  remains reusable: the exercised product source is byte-identical to the commit
  those passes were collected from. Re-running them now would produce no new
  information and would violate the reuse rule.

### L4 — Adversarial self-review of the new V2/V4 contracts

- identity: static review pass over [contract resolution](overlay-snapshot-contract-resolution.md)
  §2 and §4 against the rule's failure/ownership clauses and the existing Store
  transaction code.
- source: contract resolution at `a7b565640…` and the follow-up commit that applies
  the two corrections below.
- environment: none (no execution).
- result: two defects found and corrected before any implementation depends on them.
  (a) V4 keyed the publication receipt by an in-memory attempt identity, which cannot
  resolve uncertainty after a process restart; the key is now the deterministic
  `(workspace_id, candidate_root, expected_head, new_base_layer)` tuple, which also
  reproduces `CommitId::derive`. (b) V2 lacked the bounded-substitution obligation,
  so live ranges referencing temporary bytes that had already become canonical would
  pin the temporary spool indefinitely; §2.10 now specifies per-range exact-equivalence
  substitution with canonical ownership retained.
- validity: current. This is review evidence about contract text, not about a
  candidate; it does not qualify any product behavior.

### L5 — V1 boundary-ordering precision

- identity: review of the V1 option-1 wording against the observed flush behaviour.
- source: contract resolution §1 (mechanism table M1/M2 and option 1), probe evidence
  `evidence/v1-probe/run.log`.
- environment: same probe environment as L1.
- result: recorded that neither `syncfs` nor a per-file `fsync` yields an observable
  cut: a store that completes before the flush returns can still miss the kernel's
  internal collection point. Option 1 is therefore presented as "boundary = the
  kernel's collection point inside the flush", not as a clean per-file boundary.
- validity: current; refinement of L1, which it does not invalidate.


- identity: whole-repository scope check over the working tree.
- source: `git status`/`git diff` against
  `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b`.
- environment: macOS host, Cargo 1.96.0 / rustc 1.96.0 available; no build run.
- result: pass. The only changes are documentation and evidence; no
  `crates/`, `benchmark/` (harness), fixture, or configuration file is modified.
- validity: current, and it is the reason every previously recorded product pass
  remains reusable: the exercised product source is byte-identical to the commit
  those passes were collected from. Re-running them now would produce no new
  information and would violate the reuse rule.

### L6 — Non-Commit consumer audit (Phase 5 prerequisite)

- identity: static call-site audit of the helpers Phase 5 would delete, over the
  frozen product source.
- source: contract resolution §5.1; call sites in
  `crates/layerfs-fuse/src/live_owner.rs`, `crates/layerfs-workspace/src/lifecycle.rs`,
  `live_backing.rs`, `cow_tree.rs`, `execution.rs`, `file_io.rs`,
  `layerfs-workspace-core/src/{checkpoint.rs,file_edit.rs}`, `session.rs`,
  `registry.rs`, `reconcile.rs`, `projection.rs`.
- environment: none (no execution).
- result: pass, with three findings that change the removal plan.
  (a) The cut gate is shared with the ordinary SDK splice path
  (`live_owner.rs:2956-2961`), `prepare_shutdown` and a direct `gate.cache_flush()`
  call, so Commit must stop using the complete-dirty-prefix flavour rather than the
  cut being deleted. (b) The container has its own checkpoint installation
  (`live_owner.rs:363-380`, `INSTALL_BEGIN`/`INSTALL_END`) independent of the host
  path, so removing the host Commit call does not remove that machinery.
  (c) The `syncfs` call is also the product's kernel writeback-error channel, so a
  Commit that stops calling it changes error-reporting semantics unless the
  replacement states where those errors are observed.
- validity: current. Call-site analysis of unchanged source.

## Retained passes from earlier work (not re-run by this ledger)

| Pass family | Recorded by | Why still valid |
| --- | --- | --- |
| v0.1.6 M1 fixture/mixed/alias/boundary verification and performance rows | commits `d6ce37b5b`, `536aaf9de`, `3f38e82ca`, `12c047ca1`, `dcd8ed591` | product source unchanged since those runs (L3); no fixture, oracle, image or harness input changed |
| v0.1.5 #120 terminal campaign (227 registered selections) | `docs/roadmap/0.1/0.1.3/…`, #120 reports | same reason; also outside this campaign's scope and custody |
| `tools/test-fast.sh` regression screen | `tools/test-fast-timings.json` | same reason |

Unchanged rows are *not* evidence for the new design: they describe the current
frozen product, whose Commit path still contains the freeze this work must remove.
They are retained as the regression baseline that the future candidate must not
break, not as qualification of the new behavior.

### L7 — V1 retrieval mechanism correction and ownership counterexample

- identity: V1-H1 dirty mmap retrieval, V1-H2 mutation after notification before
  daemon read, V1-H3 open-unlinked retrieval by inode ID. Single changed-mechanism
  probe; original no-trigger experiment is reused, not repeated.
- source: `evidence/v1-investigation/retrieve_probe.c` includes the retained
  `evidence/v1-probe/probe.c`; exact source/image/dependency hashes and command in
  `run-01.json`, `SHA256SUMS`, and pinned primary-source register `sources.json`.
- environment: deployed `6.12.76-linuxkit`, Linux aarch64, GCC 12.2.0, image
  `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`;
  short standalone probe under the existing measurement lock, released afterward.
- result: byte-retrieval assertions PASS; **snapshot mechanism REFUTED**.
  Retrieved 4096 bytes without WRITE callbacks, also after unlink. H2 returned B
  after A was current at notification and a later mapped B completed before read.
  Full supported-surface V1 acceptance remains OPEN. No benchmark was executed.
- validity: current. Original L1 observations remain valid; invalidates only the
  predecessor's claim that the kernel has no buffer-retrieval facility and that
  forced writeback is the sole path to those bytes. It does not qualify product
  snapshot isolation. See `evidence/v1-investigation/README.md` for scope and
  remaining kernel/adapter capability, and independent work that continues.

### L8 — First dependency-ready overlay and cursor implementation

- identity: seven new root/index/replay tests, one owned cursor test, five affected
  existing regressions; exact selectors/counts in `evidence/phase2-components/README.md`.
- source: staged implementation recorded by `evidence/phase2-components/source.json`;
  `overlay.rs`, initial `overlay_index.rs` blob `7680a5811415d98f18fbdb6c27a13bf2f596ffbb`,
  core piece cursor and its wire/host consumers. Test binary SHA-256 in README.
- environment: macOS host, rustc 1.96.0 / Cargo 1.96.0; no measurement sample.
- result: 13 distinct tests PASS. Two initial zero-test selections are retained
  INVALID_EVIDENCE; one initial index compile failure is retained and repaired by
  an explicit closure parameter type. Raw logs and reproduction commands preserved.
- validity: current for this first component implementation. Later graph/payload
  ownership changes require affected index/root checks, not blanket reruns of
  passing wire/core consumers. Does not prove public full snapshot semantics,
  Phase 2 completion, or benchmark custody.

### L9 — Bounded Store publication receipt component

- identity: 2 new receipt tests and 14 affected existing Store tests; source,
  selectors and environment in `evidence/v4-receipts/verification.jsonl`.
- source: explicit `workspace_publications.sql` extension, Store staging/workspace
  publication APIs, exact schema validation and statement registry. Exact hashes
  in `evidence/v4-receipts/final-source.json`.
- environment: native macOS/Cargo; component fault injection and fresh Store reopen.
- result: 16 distinct tests PASS, one compile failure retained and repaired. Last
  test-only addition rejects a Commit appearing solely on another branch; only
  that test was rerun, retaining unaffected earlier passes. No benchmark run.
- validity: current Store component evidence; runtime snapshot/coverage/End/Discard
  integration remains pending. Receipt insertion atomically retains Created and
  UpToDate, but this is not a claim that ordinary Workspace Commit uses it yet.
