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

### L10 — Required CI and retained failures

- identity: PR #126 CI runs `34780076545` and `34780263723`, original failed
  heads/steps, pure format repair and targeted Store Clippy repair.
- source: exact heads `5996231830eb05c986f7f9dcbb4b24cc52288281` and
  `2728aebd38ac5d3b2a8a1379b3259684512284b0`; subsequent receipt boxing/shorthand
  source in the associated repair commit. `evidence/ci/README.md` names commands.
- environment: GitHub Ubuntu CI, Rust 1.85.1 tests / 1.96.0 fmt+Clippy; targeted
  Store repair uses local Rust 1.96.0. No performance measurement.
- result: first CI FAIL_FMT; second fmt/Python/full-native-tests PASS then
  FAIL_CLIPPY (redundant field + large enum). Store repair Clippy PASS and one
  exact affected publication regression PASS. Full raw failures retained.
- validity: current for recorded heads only. No lint/oracle gate disabled; no
  #122 benchmark executed. New snapshot components/ownership changes still need
  affected verification and final CI; these passes do not seal a candidate.

### L11 — Disk ownership, physical relocation and uncertain I/O repairs

- identity: typed index set/remove/floor/scan, external payload and linked roots,
  retained snapshots, exact quota, ambiguous header write/readback, every reclaim
  write boundary, physical relocation/corruption/truncate retry, read-only ticket
  retirement and cleanup readiness.
- source: exact final Index blob `da9eff967602acbdee497614f58b2d721e52a47a`, SHA-256
  `023b463b31c3ab29347123dd2da01e4247efd4352f008f0291c718b6d4c8dcd4`;
  `evidence/phase2-components/index-relocation-manifest.json` and preceding
  `index-ownership-recovery-manifest.json` bind each command/source/binary.
- environment: native macOS arm64, rustc 1.96.0; scoped Cargo/standalone component
  checks serialized separately from benchmark measurement.
- result: 13 compatible index PASS checks plus retained admission pass. Original
  failures (three leaked pages after ambiguous increment, read-only ticket
  exhaustion, premature cleanup readiness) remain raw evidence with targeted
  repairs. Data arena reclaims physically while held snapshots stay readable;
  bounded header/reverse catalog highwater remains separately charged.
- validity: current component evidence. Supersedes earlier index passes only
  where leaf ownership/physical layout/retirement paths changed; untouched
  wire/core cursor passes remain retained. No public-path or phase-completion claim.

### L12 — Payload + range integration

- identity: exact 64-MiB source/one-byte retention; held physical reader;
  disk metadata owner; quota/short-I/O; repeated source append; range split/merge,
  unequal edits, truncation/lineage, 1024-piece localized cursor; cleanup.
- source/environment/commands: `evidence/phase2-payload/README.md` and
  `integration-review.md`, `evidence/phase2-ranges/README.md`, their append-only
  ledgers and per-attempt JSON bind all product/dependency/binary identities.
- result: four distinct range checks PASS; payload targeted checks PASS with
  valid pass reuse. Measured payload `67,108,864 -> 4,096 -> 0` allocated bytes;
  only 4,096 bytes relocated. Repeated append retains one source-location record.
  Three real shared failures were preserved and repaired: old-reader retirement
  incorrectly blocked unrelated allocation; truncated append consumed input;
  cleanup status omitted queued/disk release work. Unrelated compilation blocks
  remain NOT_RUN, never PASS.
- validity: current for the recorded exact dependency identities; earlier passes
  explicitly invalidated for their changed source/ownership paths. These are
  development component measurements, not benchmark rows or candidate custody.

### L13 — Metadata-only canonical predecessor correspondence

- identity: reviewed nonzero-anchor/zero-gap plans, shifted/overlapping ranges,
  fresh/equivalent lineage, malformed provenance, journal/index bounds and 180
  fragmented intervals across cursor batches; huge-zero metadata case.
- source/environment/commands: `evidence/v3-correspondence/README.md`,
  `verification.jsonl` and attempt source manifests. No payload encoder replaced.
- result: four focused checks PASS; the huge-zero case reuses `1 PiB + 1` logical
  bytes with one anchor/plan fragment and 49 journal bytes, with zero payload reads.
  This is an algorithm fixture, not a claim to bypass existing public file limits.
- validity: current component evidence after recorded shared-index revalidation.
  Actual shared builder/public C1/C2 integration remains required.

### L14 — Owned Snapshot content and bounded metadata installation

- identity: eight targeted overlay tests including complete-root races, preserved
  binding masks, fixed inode metadata/linked ownership, canonical aliases,
  no dirty history on immutable cache acquisition, FIFO mutation admission and
  snapshot independence, exact captured payload after live replacement/owner drop.
- source: current Overlay/Snapshot and final Index/Payload/Ranges; exact source
  hashes recorded in `evidence/phase2-components/snapshot-integration-source.json`.
- environment: macOS native rustc 1.96.0; exact command in
  `snapshot-content-attempt01.log`; no FUSE benchmark or kernel qualification.
- result: 8 PASS. Replay check was deliberately skipped and its unaffected L8
  pass reused. Snapshot reads `captured`, current reads `live`; independent range
  reads and EOF behavior stay correct after dropping the live owner.
- validity: current host-installed-state proof only. Generic-FUSE V1 remains
  OPEN, as do host operation/candidate/coordinator/public adapter integration,
  complete resource admission, final correctness/capacity and full Phase 7.


### L15 — Overlay policy, ownership lifetime and host operations

- identity: one exact/+1 core overlay policy check; four host aggregate/index/FD,
  operation reservation and detached-owner lifetime checks; three host operation
  checks covering cold canonical reads, retained snapshots, aliases, unequal SDK
  splices, atomic failed batches, concurrent rename, open-unlinked identity and
  tombstone-only directory pagination.
- source/environment: `evidence/phase2-components/overlay-budget-manifest.json`
  and `evidence/phase3-host-operations/attempt-02.json` bind exact source hashes,
  native macOS arm64 Rust 1.96.0, commands and raw logs.
- result: policy 1 PASS; budget 4 PASS; host operations 3 PASS. Host attempt-01
  failed all three checks during root initialization: non-root `acquire_inode`
  incorrectly rejected the namespace root's valid zero incoming references.
  Host attempt-02 reuses existing canonical root resolution and passes, including
  29,049 snapshots observed during concurrent namespace changes.
- validity: recorded component identities only. Later shared range-summary/read
  changes and cumulative quota wiring require the directly affected checks;
  separate replay/index/Store passes remain reusable where dependencies did not
  change. These results do not prove generic-FUSE V1 or integrated Commit.

### L16 — Retained CI integration failure

- identity/source: GitHub CI `34781059906` at `6d907bc43` and `34782823313` at
  `c95588f30760e08235d56b513e48b92a9defd9b7`; remote Rust 1.96.0 workspace Clippy.
- result: FAIL; raw logs preserved in `evidence/ci/`. New private overlay and
  correspondence components have no production callers yet, so `-D warnings`
  correctly rejects dead code. The earlier index scan arity issue was repaired.
- validity: failures remain current evidence of incomplete product integration.
  No `allow(dead_code)`, public API exposure solely for lints, or test-only hiding
  is accepted as the repair. Complete ordinary-operation/candidate/lifecycle
  integration before claiming CI success.


### L17 — Shared reader prerequisites, logical quotas, kernel references and host dispatch

- identity/source: range summary/zero limit and root-protected streaming reads
  (7 checks), persisted metadata description (1), affected Snapshot reader (1),
  bound in `evidence/phase4-candidate/prereq-01-source.json`. Host operation,
  exact logical quotas, kernel reference/detach, wire, installation generation
  and request-digest checks bind source in `evidence/phase3-host-operations/`.
- environment: native macOS arm64 Rust 1.96.0; exact commands and raw results in
  each attempt manifest; no benchmarks or Linux FUSE workload executed.
- result: prerequisites 9 PASS; host 5 distinct PASS (4 from attempt03 retained,
  1 targeted attempt04); wire 2 PASS; exact root-generation and replay-digest
  checks 1 PASS each. Host attempt03's remaining failure was a test expecting
  128 visible names despite two scanned tombstones; the repaired oracle follows
  continuation and verifies all 131 live entries, without changing production code.
- further integration: `dispatch-cookie-attempt01.json` records two new exact
  tests from one compiled binary: stable numeric directory cookies across 301
  cold names, aliases, deletion/cursor resume and repeated create/remove (PASS,
  3.40s), and exact host dispatch/reply replay including kernel-owned orphan and
  retained snapshot reads (PASS, 0.02s). These are test wall times, not benchmark
  measurements. Replay binds the entire request digest; mismatched, stale and
  unknown-session identities never reapply an operation.
- validity: the shared reader/aggregate and binding ownership changes explicitly
  invalidated the three earlier host checks; their directly affected reruns now
  pass. Pure test-oracle repair did not invalidate the other four host passes.
  Original failures remain preserved. Generic-FUSE mmap, actual client transport,
  shared candidate/C1/C2/public lifecycle and final capacity/campaign remain open.


### L18 — Reviewed replay acknowledgment correction

- identity: the L17 host-dispatch check extended with a sequence-2 retry carrying
  the identical mutation while cumulative acknowledgment advances from 0 to 1.
- source/environment/commands: `evidence/phase3-host-operations/replay-ack-attempt01.json`
  and `replay-ack-attempt02.json`, native Rust 1.96.0; raw logs retained alongside.
- result: attempt01 FAIL (Invalid); attempt02 PASS after hashing operation bytes
  separately from the session/sequence key and acknowledgment metadata. Known
  failures, lost results, changed-body rejection, stale/unknown requests and
  kernel-orphan snapshot reads remain in the same exact check.
- validity: this is the current host-dispatch pass. The older full-frame test
  missed a valid acknowledgment update. Numeric cookie and unchanged wire codec
  passes are retained because their exercised paths did not change.

### L19 — owner CI budget update; historical failure retained

- identity: full native CI, Rust 1.85.1, bounded jobs=4, run34789404326.
- source: product/test source `7a4a920acfc40a456508f27ef28c526068de7f92`; original `tools/test-fast.sh` ceiling120s.
- environment: GitHub Actions runner and toolchain recorded in raw log.
- result: all native selections passed; enclosing CI step FAIL at125s under its original120s ceiling. Raw: `evidence/ci/run-34789404326-failed.log`. Preserve FAIL; do not relabel as a historical PASS.
- validity: current evidence of that historical attempt. Owner subsequently authorized150s for CI; `tools/test-fast.sh` and current development guidance updated accordingly. No benchmark/correctness contract changed; no timing optimization or repeated full suite requested by this change. Shell syntax checked with `bash -n tools/test-fast.sh`.

### L20 — explicit host mount transport integration

- identity: Linux FUSE helper and daemon mount startup with host-owned authority selected before connection; strict daemon Mount codec.
- source: exact per-file SHA256 manifest `evidence/phase3-host-operations/mount-authority-linux-attempt01-source.json`; before/after unchanged. Linux helperSHA `ed690820d4437aad700e4d7a2b1974206c2ccd69dc6e1c5deca7be6b77c23e2c`, daemonSHA `6fb684d896132ed841269496898b8bf79f6c5f085f4f4b0ee0312e1c10eb5614`.
- environment: native macOS cross-build, existing cargo-zigbuild/Zig, aarch64-unknown-linux-musl debug. No container/workload/benchmark execution.
- result: helper+daemon build PASS (27.23s command wall; not a performance claim), exact commands/log in `mount-authority-linux-attempt01-result.json` and `.log`. Native `protocol::tests::fragmented_and_coalesced_frames_and_exec_bounds` PASS1, old Mount byte encoding retained, affirmative host selection roundtrip and invalid/trailing flag rejection; `mount-authority-codec-attempt01-{source,result}.json`/`.log`, source unchanged.
- validity: concrete mount entrypoints compile; codec test current. This does not establish mounted-kernel, complete public lifecycle or V1 acceptance. New ignored mounted host-owner component check is written and remains NOT_RUN until explicitly executed.

### L21 — CI after owner150second update

- identity/source: run34791110435 at `f35e0039ba8f11fb97feb608b2bc179fc0e9bc04`; native Rust1.85.1 suite using new150s ceiling.
- result: native test-fast step SUCCESS per GitHub Actions job/step receipt; overall CI FAIL in strictClippy due unwired new component paths and remaining lint findings. Raw failed step: `evidence/ci/run-34791110435-failed.log`. The old125s/120s run remains FAIL separately atL19.
- validity: historical source-specific CI result; ongoing integration remains the repair. No repeated full native suite for a timing optimization, no numerical benchmark contract change and no lint suppression.

### L22 — integrated SDK readers, attempt retries and shared host runtime

- source/environment: native macOS arm64 debug, exact `evidence/phase4-candidate/capacity-compile-attempt03-source.json`; no source drift, workspace binarySHA `0869ef5dcda8f36013d124129753dd125bd1d3a25101f13d51c9477cebcfb4a7`.
- result: SDK host4 + exact installed-snapshot1 PASS (`phase3-host-operations/sdk-host-attempt01.json`, `sdk-installed-snapshot-attempt01.json`); direct owned-file reader2 PASS (`sdk-file-lease-attempt02.json`, `sdk-reader-reservation-attempt01.json`); coordinator held construction/C1/C2 + exact lost stage/Created/UpToDate retries + definite branch-conflict abandonment3 PASS (`phase4-candidate/coordinator-c1-c2-attempt02.json`, `coordinator-retry-attempt02.json`, `coordinator-conflict-attempt01.json`); local/TCP HostRuntime factory/coverage1 PASS (`runtime-local-tcp-attempt01.json`). Each JSON carries exact command/raw log.
- invalidation: reader creation changed to pre-admission+activation, requiring the affected old reader check; canonical candidate now uses charged scratch/private admission ownership, requiring the affected C1/C2 and retry checks. New lifecycle/runtime checks had no predecessor pass. Unchanged unrelated suites were not rerun. These are component results, not publicV1 acceptance or benchmark measurements.

### L23 — shared scratch and bounded maintenance integration

- source: same exact attempt03 workspace binary; Store binarySHA `89b16632c6734f15792b8e9050077e353111dc7efea485362366fb0773910dfc`.
- result: actual Store scratch/SQLite/data/order tests5 PASS in `phase4-candidate/scratch-integrated-attempt01.json`. Existing unlinked-SQLite probe remains ignored after its original recorded FAIL; it is not counted as a pass. New capacity-owner fixture FAIL (`capacity-owner-attempt01.json`): repetitive2MiB data deduplicated below the actual spill threshold, so its required spill assertion did not trigger. Correct that unpassed fixture to deterministic varied bytes; preserve the assertion and failed attempt.
- maintenance:2 PASS and1 test-oracle FAIL in new maintenance evidence. Idle pre-stage construction error correctly clears its attempt; test incorrectly expected retention. Repair only the failing test using the existing real retained-stage fault; retain bounded-node/key and C2/CAS passes. Those isolated correctness processes may have overlapped SDK tests; no benchmark measurement occurred.
- remaining:96MiB per-construction reserve is explicitly provisional and admits at most one default builder under shared128MiB; trace/reduce actual scoped working-set bounds and check independent builders. Account ordinary host SnapshotReader cache. Default8192 persisted payload token limit needs separate capacity investigation; none of these is a capacity PASS claim.

### L24 — first actual mounted host-owner check, cleanup failure retained

- identity: `host_runtime::tests::mounted_host_owner_preserves_sdk_mapping_handles_and_owned_commit_input`, explicitly ignored in resource-free native runs and manually executed under existing measurement lock.
- source: native0869ef5d from L22; Linux helpered690820 from L20; immutableimage `sha256:b9d3d2c3090596364d2d70ee304a5b7316b8dc51b9d672b8a0b3857e1de940f3`, kernel6.12.76-linuxkit, Python3.11.2. macOS owns Store/SDK/host runtime/spool; Docker owns only helper/FUSE/workload. Actual2CPU/2GiB/no-swap/256PID/nonprivileged/no-host-bind checks passed.
- result: overall FAIL in `phase3-host-operations/mounted-host-attempt01.json`/`.log`. All prior assertions passed: SDK byte visible through retained mapping and descriptor, unrelated dirty mmap byte retained, inode/hardlink/rename/open-unlinked lifetime, owned C1 excluding later ordinary write and C2 including it. Normal `projection.end()` then failed with EBUSY because HostClient retained its preopened mount-root descriptor. Preserve `mounted-host-attempt01-fuse.stderr` and fallback logs; do not count this row as a passing mounted check.
- cleanup: owner removed the test container. Original runner mistakenly matched capitalized error text; append-only `mounted-host-attempt01-cleanup-followup.json` verifies actual absence with Docker's lowercase message. This correction does not turn failed product unmount into success. Raw fixture Store remains in its original temporary directory.
- next: release root descriptor before unmount, add explicit local SHUTDOWN parity and focused ownership check, rebuild only affected helper/native code, rerun the failed mounted case. Generic Commit dirty-mmap acquisition remains unproven; this concrete test uses an explicitly supplied owned input and is not a Phase6/final benchmark result.

### L25 — bounded three-fix checkpoint delivered source

- source: `f8ed6bbbb3cc86eea23b82d3efa96c70b0519d2a`, formattedsource seal `evidence/step1-publication/source-before.json`; finalnativeRust1.85.1 binarySHA1f84e1f0..., LinuxhelperSHA29f56dcd....
- mounted repair: `evidence/phase3-host-operations/mounted-host-attempt02.json`/`.log` PASS1 including normalunmount and verifiedcontainerabsence. FirstEBUSYFAIL, rawstderr/fallbackrecords andincorrectcase-sensitivecontainercheckfollowup remainretained. Exactrunnerpublishedbesidereceipt. Explicitownedinput only; no genericV1/fullpublicCommitclaim.
- otherboundedfixes: actualvariedspillfixturePASS (`capacity-owner-attempt02.json`); exactretained-stagemaintenancetestPASS (`maintenance-attempt02.json`) with2unaffectedpassesretained. Independentreviewfoundnonewscopedblocker.
- finalpublicationverification: buildPASS; format/whitespacePASS; runner7PASS; native637selections/37binaries/79batches reports611passes26ignored andnoexecutedtestfailure, butgateFAIL171s>150s. StrictClippyFAIL; preserveexactlogs. Named1.96toolchainLinuxstdmissingFAIL repairedbyexistingconfiguredstabletoolchain, finalhelperPASS. No CI timeoptimization/waiver/lintsuppression.
- ownerstoppinginstruction: stopafterthreefixes, remote-mainpublication anddurablehandoff. No furtherproductmigration/V1/capacity/benchmarkexecutionauthorizedinthisrun. [Successor handoff](overlay-snapshot-step1-handoff.md) recordsremainingworkandcustody. Issues124/125stayOPEN; no#122scenario/release/tag.

### L26 — owner180second CI budget

Owner directly accepted171s and requested180s before checkpoint publication. Updated the actual test-fast ceiling and current development guidance; `bash -n tools/test-fast.sh` PASS. L25's historical171s/150s gateFAIL remains unchanged, as doesL19's125s/120s failure. No passing suite rerun, benchmark gate change or lint suppression. CurrentCIbudget180s; strictClippy stillOPEN.

### L27 — owner-approved balanced Clippy policy

Owner approved advisory style/complexity/unused-code warnings; CI still denies clippy::correctness, clippy::suspicious and unused_must_use. Initial balanced command FAILed on a stale doc comment documenting the wrong benchmark helper. Comment-only removal repaired it; balanced workspace Clippy PASS. Equivalent State Default derivation passes the existing shutdown descriptor/local-control check (1exacttest); fmt/whitespace PASS. Commands, raw failure/pass logs and source hashes: evidence/ci/balanced-clippy/. No full native rerun locally for these equivalent/comment-only changes; publication's Linux CI will exercise its existing complete gate. Current180s budget, V1/product acceptance and36 #122 exclusions unchanged.

### L37 — minimal-overhead review published; a pre-existing CI flake identified

Identity: PR#133 (docs only, 31 files under `evidence/minimal-overhead-review/`), merged `f9b7d324de06dce14ed07857b260110a27368659`; CI run [34805024890](https://github.com/Ephemeral-AI-Lab/layerfs/actions/runs/34805024890).
Result: published the review evidence #130 depends on but which had never been in the repository, captured at directory hash `71ae8487731459e0904e28109dd99f1782f7ea55f744103500d6804594fc13f1` after a 75-second quiescence check (another session was writing to that directory while this campaign was stopped). The audit's own verdict — "Do not approve this as a fully specified implementation yet" — is preserved unaltered; none of it is implementation or qualification evidence.
**Incidental finding — a pre-existing flaky CI test:** the first CI attempt on this docs-only PR failed at `layerfs-fuse::host_client::tests::cancelled_entry_and_partial_page_keep_pre_admitted_cleanup` (`crates/layerfs-fuse/src/host_client.rs:1248`, a cleanup-slot permit assertion). The PR changes no source file (`git diff --name-only origin/main...HEAD` lists only `docs/`). The identical commit passed 6/6 locally and **passed on rerun of the same run id with no code change**, so the failure is a flake, not a regression.
Validity: current, and recorded because it matters beyond this row: the file was last modified by the predecessor's `f8ed6bbbb`, not by this run, so the flake predates this work; a flaky gate can mask a real regression during the eventual #125 campaign and must not be mistaken for one, nor silently retried as if green. No fix has been attempted and no retry-until-green policy is adopted. Not a benchmark, gate or waiver claim.

### L38 — #130 promotion, scaling review and two-second implementation plan

- Identity: owner-requested architecture/feasibility review, with two bounded
  subagent reviews of tiny churn and the current write/Index/payload/transport
  path; coordination/contract review by the root agent.
- Source: main `695c482e7aa8471710ad7e869334084de0bf2fa0`; plan on
  `codex/promote-workspace-overhead`. Interrupted four-file Step 5 edits remain
  unverified and excluded from this documentation publication. Historical 25k
  source/log is `1ffd63688eaabca7b8a4b756a342cde4514011d2` on the trajectory branch.
- Environment/result: read-only source/receipt analysis on macOS; **REVIEW ONLY**,
  no new product PASS, benchmark run or measured speedup. Two-second feasibility
  is not established. Found a source-level cumulative quadratic payload evacuation
  counterexample, fixed per-file allocation amplification and serialized RPC cost.
  The old diagnostic itself also performs cumulative quadratic censuses; retain
  its timing as diagnostic wall, not production throughput or a million-file forecast.
- Validity: scheduling and the explicit <=2-second #130 objective supersede the
  prior after-closure prerequisite. All correctness/benchmark criteria and #122
  exclusions remain. New plan selects bounded local reclamation, dense existing
  Index, compact common ranges and packed arena allocation; it is not implementation.
- Evidence correction: L37's same-source fail/pass establishes intermittent
  behavior, but file blame alone does not establish the root cause or introduction
  point. Preserve its original failure and passes; no blanket unrelated-flake waiver.
  Likewise trajectory-branch L36's assertion that a suite failure is "void" due
  to concurrent source edits is unsupported: a compiled test binary does not
  normally read Rust source. Retain the failure and investigate source custody/
  actual cause; do not count it as a pass or erase it.

### L39 — owner switches #130 evaluation to existing tiny-churn cases

- Identity/source: owner update after L38: evaluate close performance to existing
  tiny-* benchmarks and defer 25,000-file and million-file cases. The two existing
  subagents rechecked active registry/criteria and minimal implementation order.
- Result: **REVIEW ONLY**; no benchmark/build/test/probe. Exactly 20 active tiny-*
  identities are in `tiny_file_churn`, none intersect the 36 #122 exclusions.
  Keep their current fixtures/modes/seeds, original public-call timer and existing
  stronger criteria. The plan prospectively adopts the existing #118 ordinary
  paired regression screen for comparable per-case results; no invented universal
  two-second threshold or per-file dilution of a whole-workflow delta.
- Validity: L38's promotion and no-quadratic requirement remain; its active
  25k/two-second objective and new-case registration task are superseded/deferred.
  Million-file qualification is deferred, not passed or waived; any broader
  terminal requirement for it remains OPEN. Preserve all old evidence unchanged.
- Next: current-route tiny-case diagnostics and focused quadratic-reclamation
  repair, then the smallest measured improvement. The full compact-storage bundle
  is no longer required before this evaluation; no new scale matrix is introduced.
- Subsequent owner refinement: quick iteration targets exactly
  `tiny-create-500-mixed-v4`, `tiny-stat-500-mixed-v4`,
  `tiny-unlink-500-mixed-v4`, `tiny-bulk-create-500-mixed-v3`, and
  `tiny-bulk-delete-500-mixed-v3`. Bulk tier500 means 5,000 files/500 MiB.
  Retain valid passes and rerun only affected checks; full 20-case qualification
  follows when stable, not after every fix. This is scheduling, not a test result.
