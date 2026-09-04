# v0.1.3 benchmark infrastructure optimization

Status: infrastructure migration completed with bounded representative smoke coverage;
closure evidence and explicit exclusions are recorded in section 15. No full-matrix
performance qualification or release readiness is claimed.

Tracking issue: [#45](https://github.com/Ephemeral-AI-Lab/layerfs/issues/45).

Parent roadmap: [#21](https://github.com/Ephemeral-AI-Lab/layerfs/issues/21).
Local specification: `docs/roadmap/0.1/0.1.3/benchmark-infrastructure-optimization-spec.md`.
Assessment baseline: `810bb3a589ac58d103483df34bb58ecfe0f0ddf4`.

## Latest scope amendment: genuinely compact lower tiers

The user's subsequent 2026-09-05 instruction explicitly changes the low-tier
fixtures: do not carry a 500 MiB / 100,000-file background into a small case.
Interpret the requested reduction as at least 10 times less aggregate data and
100 times fewer files: low-tier initial/final fixtures must fit 50 MiB and
1,000 regular paths (hard-link aliases count). Prefer tier-proportional data
below those ceilings. This overrides the earlier requirement below to preserve
all low-tier recipes unchanged; preserve higher-tier recipes and historical
results, but give changed lower-tier cases/profile aliases new versioned identities.

- Ordinary tier-1/tier-10 cases use compact-v2 layouts and IDs; do not retain
  hidden 500 MiB sources, 100,000-file backgrounds or target schedules pointing
  outside the reduced tree. Update preparation, operations, expected-state
  recipes, offsets and independent verification together.
- Workspace-reuse tier 1 uses one 1 MiB base file, tier 10 ten such files;
  tier-100/tier-500 bases stay unchanged. Low final states are 2/20 MiB.
- Namespace's lower profiles become 5 MB / 100 files and 20 MB / 1,000 files,
  with per-profile reduced anchors. Higher 300/500 MB profiles stay unchanged.
- Store-footprint gains explicitly identified compact controls; original
  large controls remain large. Existing SDK 1/10 MiB and small cross-file,
  CDC and history fixtures remain where already compliant.
- Simple reliability proofs use a versioned 1 MiB / 10-regular-path fixture
  instead of 32 MiB / 1,002 paths. Referenced sentinels, links and proof
  operations remain; the 600-second proof is still explicitly unsupported.
- Fixed 500 MiB capped-result replacements have no equivalent small case;
  preserve their meaning, exclude them from automatic smoke selection and
  report them as large-only rather than silently running or shrinking them.
- `--smoke` checks actual fixture bytes and file count, not just the numeric
  tier or alphabetical case name. It also rejects operation tiers above 10
  even when their initial fixture is empty/small (large create and history
  workloads are not smokes). Namespace and footprint use their explicit
  compact profiles rather than treating a file-count label as a MiB tier.
  Metadata/descriptor-only checks cover every
  changed lower-tier initial/final state before bounded live smoke checks.
- Keep verification under the existing deadline and use small independent
  checks. Do not add an expensive large-background certification system merely
  to preserve the oversized setup the user has now rejected.

Observed reason for this amendment: the old `directory-construct-1` smoke
spent about 80 seconds in first-time preparation for its 500 MiB background;
its selected product calls were milliseconds. Its old full verifier timed out
at approximately 41 seconds. These are retained historical setup/verification
findings, not a performance result for the new compact case.

## 1. Scope and precedence

The user's 2026-09-05 instructions authorize this infrastructure design:

- Full Docker/Linux product execution and preparation; real LayerFS daemon/FUSE/Exec for filesystem workloads; host orchestration only.
- No Docker bind mounts, named/anonymous data volumes, or Docker socket mounts.
- Reusable prepared projects; independent fresh or cloned starting state as appropriate to the measured operation.
- Three modes: default one-sample performance, explicit repeated performance, and selected verification only.
- Family-local `mod.rs`, `setup.sh`, `perf.sh`, and `verify.sh`, with shared implementations.
- Compact timer/resource logs and automatic disposal of bulky sample data instead of artifact-heavy reports.
- Refactor families one by one with an explicit checklist.

These instructions supersede earlier host-product/APFS setup, per-sample bulk-evidence retention, and the literal prohibition on family-local verification scripts. Thin `verify.sh` launchers are now allowed; there remains exactly one verification companion implementation and no copied oracle logic.

This is a separate harness workstream. Do not implement or claim the product optimizations tracked by #38–#44, close those issues, alter their performance targets, or claim release readiness. Refactoring their benchmark adapters does not qualify their product fixes. Do not restart the withdrawn Phase 1 suite; [phase-1-verification-withdrawal.md](phase-1-verification-withdrawal.md) remains a design constraint. This specification authorizes bounded checks during future implementation, not an exhaustive campaign.

## 2. Problem and expected result

The current harness splits the coordinator/Store on macOS from Linux daemon/FUSE work, uses APFS sample copies, permits reference/exchange mounts, and duplicates setup/command/environment/resource receipts. Some supporting performance routes unconditionally reopen. Pure import routes can start an unused Docker runtime. The current image omits the benchmark coordinator. Repeated retained fixtures and Stores can dominate disk usage; merely removing Markdown reports will not solve that problem.

Deliver one coherent execution contract, family-oriented source layout, bounded reusable preparation, and minimal permanent output. Reuse existing family recipes, SDK operations, daemon/FUSE transport, canonical builders, and independent oracles. No new crate, universal executor framework, remote-filesystem abstraction, cache service, or backend plugin system.

## 3. Source organization

Target layout (move existing code incrementally; names denote responsibilities, not permission to create duplicate implementations):

```text
benchmark/fs-bench-pro/
  Cargo.toml
  Dockerfile.layerfs
  verify-selected.py                 # sole verification implementation
  shared/
    runner.py                        # selected modes and orchestration
    runtime.py                       # Docker lifecycle and prepared-state reuse
    daemon-entrypoint.sh              # image bootstrap; not a family runner
    test_runner.py
    test_runtime.py
    test_layout.py
  src/
    main.rs                          # one Linux coordinator binary
    ...                              # existing shared Rust helpers/oracles
  families/
    <existing-family-id>/
      mod.rs                         # definitions and family-specific behavior
      setup.sh                       # thin preparation launcher
      perf.sh                        # thin performance launcher
      verify.sh                      # thin verify-selected.py launcher
```

Preserve existing canonical family/case IDs; directory moves must not rename inputs or alter recipes. A family module is not a separate executable or crate. Existing shared Rust oracles may remain shared. Shell launchers only bind the family and forward arguments/exit status; they contain no Docker, timing, clone, oracle, deadline, or cleanup implementation. Proof-only `perf.sh` returns an unsupported-selection error without starting work.

The user subsequently authorized removal of the root shell entrypoints and obsolete compatibility pipeline. Family-local scripts are now the only supported shell interface. Remove superseded callers and imports together; historical commands/reports are reproduced from their frozen Git revision, not an archive directory or compatibility launcher in the current tree. Update build COPY paths, Rust module paths, source seals, documentation, and all shell/Python callers with each move. Never reinterpret historical evidence.

## 4. Modes and selection

| Interface | Contract |
| --- | --- |
| `--perf-fast` | Default mode; exactly one performance sample of the selected case/input. |
| `--perf-samples N` | Positive integer N; sequential independent samples of the same selected input identity. |
| `--verification` | Exactly one explicitly identified verification selection; no performance samples. |
| `--setup fresh` | Build the selected post-initialization starting Store anew for each sample, outside the operation timer. |
| `--setup clone` | Default where valid: independent writable copy of an authenticated, closed, initialized master Store. |
| `setup.sh` | Optional prewarming; performance automatically acquires compatible preparation or builds it when missing. |

Modes are mutually exclusive. `perf.sh` accepts performance modes only; `verify.sh` implies verification and rejects performance flags. Missing/invalid case selection fails before expensive setup. Performance may use a documented registry default seed/repetition, recorded explicitly; verification requires the exact seed or inherited repetition, source/input/setup identity. A matrix is an explicit caller selection, never implicit expansion by a mode. Verification rejects `--all`, ranges, and missing identities.

Sample ordinal is not seed, inherited repetition, or workload depth. For example, N=10 repeats the selected permitted seed ten times; it does not invent seeds 4–10, consume the old admission schedule, or truncate a history tier. Existing matched admission schedules remain separately identified and are not silently replaced by this convenience interface. `--perf-fast` does not mean qualification PASS.

Keep the user-selected `--perf-fast` name, but help text and the run header must state `full_workload=true`: fast refers only to the default sample count, never reduced workload or the existing fast-verification profile.

Initialization families have no selectable post-init strategy: reuse a prepared source fixture but require a fresh/absent output Store before measured initialization. Reject explicit `--setup clone`; accept omitted setup or explicit fresh as equivalent and record `fresh-output`. This includes namespace, cross-file CAS import, CDC import/boundary checks, and initialization-measuring Store-footprint controls. Use registry operation semantics, not a fragile list of filename suffixes.

Performance contains no correctness-only reopen, full digest, or history replay. Preserve real operations intrinsic to the family: measured reads remain reads; history retains Commit-and-continue and all selected Commit steps. Keep required operation outcome/resource checks; performance completion is not verification success.

## 5. Docker execution and isolated samples

Build and seal the Linux `fs-benchmark-pro` coordinator alongside daemon, proxy FUSE helper, and workload binary. Run fixture generation, independent reference construction, Store initialization, selected coordinator, and sample copies inside Docker/Linux. The host may build/start/supervise containers, transfer small certificates/results, and print summaries; it may not run the measured Store/SDK process or supply an APFS-cloned benchmark input.

Retain `WorkspacePlacement::Container` with existing authenticated daemon transport targeting the same sample container. Use container-local loopback endpoints instead of `host.docker.internal`; no nested Docker CLI/socket is required for product Exec/FUSE. Replace benchmark-side `docker exec` resource helpers with local cgroup reads. Git preparation calls existing local Git preparation/reference functions directly inside the preparation container instead of launching another mounted container. Same-container FUSE/Exec readiness and cleanup require a focused live proof before acceptance; source inspection alone is insufficient.

Validate requested options, image-declared volumes, and actual runtime mounts. Reject any Docker data bind/volume, including ambient environment-injected mounts. `/dev/fuse` device access and the actual product FUSE projection are allowed; Docker's standard internal mounts are not project data volumes. BuildKit compilation-cache mounts are build infrastructure, not runtime sample storage. Do not publish a daemon host port when loopback inside the container suffices.

Enforce literal runtime invariants: `docker inspect .Mounts == []`, empty image `Config.Volumes`, empty `HostConfig.Binds`, no `--mount`/`-v` or Docker socket, and `/dev/fuse` as the only requested benchmark device. Docker-managed hosts/hostname/resolver files require no `.Mounts` allowlist. The daemon listener itself must bind `127.0.0.1:41273`, not `0.0.0.0`; no published port.

Preferred isolation:

```text
Docker-only preparation -> closed, sealed prepared state/image
                         -> fresh sample container per sample
                         -> local Store/coordinator/daemon/FUSE/workload
                         -> compact result extraction -> owned container removal
```

Do not keep N samples in one running container and report its lifetime memory peak as each sample's peak. A reusable running-container alternative requires separately proved sample resource isolation and cleanup; it is not the initial implementation. Prepared images are data reuse, not a persistent cache service. Never seal live SQLite connections, WAL-dependent incomplete state, credentials, capabilities, active mounts, spool, or failed mutable sample output into a master.

Build prepared data from allowlisted fixture/closed-Store/reference paths only (for example, a data-only scratch image), not a raw committed running container. Bind producer runtime, prepared-data artifact and sample runtime identities separately; reuse must not accidentally execute a producer arm's binary as the candidate. Preserve exact membership, bytes, modes, controlled mtimes, symlink targets and hard-link equivalence through image construction and sample copying. Validate these semantics in the final sample environment. A SQLite master must be quiescent and self-contained without required WAL/SHM/journal sidecars.

Each sample binds prepared-data digest, master manifest/compatibility key, runtime/product image digest and composed/sample image digest where applicable. A prepared-data hit does not authorize reuse of a composed image containing another source arm's binaries.

Suggested internal paths are `/var/lib/fs-bench/prepared`, `/var/lib/fs-bench/sample`, and `/workspace/project` (actual FUSE projection). Only the sample Store is mutable. No benchmark database or full fixture is copied to host results by default. Small certificate input and compact output may use `docker cp`/archive streams outside performance timing, with identity checks and bounded transfer cleanup.

## 6. Preparation, cloning, storage, and timing

Prepared source reuse is allowed in both strategies. Fresh means rerun the required Store initialization, not regenerate every deterministic source byte unnecessarily. Clone means independently writable initialized state, never an alias/hard link to a mutable master. Verify selected master identities once per acquisition, then the required sample-copy isolation/identity checks outside timing; do not repeat full-file verification per performance sample.

Do not conflate Docker image writable layers with Linux file reflinks. Record actual preparation/copy strategy. Start with ordinary Linux copying where needed; use filesystem COW only when supported and authenticated. Docker copy-up of a prepared SQLite file may charge a large first write to the measured operation. Establish the sample Store in its intended writable state before timing and report the setup cost; any intentionally measured copy-up gets a distinct timing/storage profile. Never silently remove real product I/O or assert a cold-cache state from container freshness.

Byte-copy fallback is explicitly valid. If trying a reflink, require an explicit success result (for example, `--reflink=always`) and identify fallback separately; `--reflink=auto` is not proof of the copy method. Close/synchronize sample setup before product measurement. Default cache policy is prepared/warm with no host cache flush; matched comparisons use identical preparation order and identify sample order, not an invented cold-cache claim.

All construction, clone, authentication, container startup, cleanup, and output-transfer wall remain observable but outside the product operation timer. Initialization-family initialization remains inside its timer. A fresh sample container isolates prior container lifetime peaks, but its peak still includes its own startup/setup; label it `sample_container_lifetime_peak`, not operation-only peak. CPU/I/O deltas use declared product boundaries. Do not double-count per-process CPU on top of inclusive cgroup CPU. Sampled RSS is not exact cgroup peak. Unavailable required precision is INCOMPLETE, not zero.

Keep preparation/data-image resources, sample setup/container-lifetime resources, and product-window resources separate in the compact records. Never merge their peaks or charge their combined work to the operation timer.

Enforce no more than eight aggregate participating product-owner CPUs, preserve stricter family limits, and serialize builds, preparation, verification, and performance measurements that would compete for resources. Record Docker/Linux platform, filesystem/storage driver, resource allocation, cache policy and operation timing profile. Old macOS or split-owner measurements are historical, not a matched Docker-only baseline. Any comparison must rerun the relevant baseline under matching conditions; this issue does not require a full matrix.

The aggregate cap also applies to benchmark-owned image builds, composition and setup; cap Cargo jobs/build workers and participating containers, not just the timed workload. Unrelated host activity must be disclosed or measurements postponed when it invalidates isolation.

Prepared cache is explicitly disposable and finite. Initial default: at most 10 GiB of benchmark-owned prepared data and at most eight inactive prepared entries; record effective limits, account unique/shared layer storage honestly, and permit explicit adjustment. If one selected entry exceeds the limit, use one-shot preparation and remove it after use rather than silently exceeding retained-cache capacity. Evict inactive owned entries deterministically before admitting new entries. Protect active/in-use entries; never broad `docker system prune`, delete foreign images, or recursively target unrelated workspace roots. Shared base/runtime/build images have separate bounded ownership/retention; do not misreport them as free. Remove one-use preparation images and completed sample containers. Cancellation must also release owned resources.

## 7. Minimal permanent output

```text
<output>/<run-id>/perf.jsonl          # performance only
<output>/<run-id>/verification.json   # verification only
<output>/<run-id>/failure.log         # optional, failures only; bounded
```

No automatic Markdown/HTML/CSV reports, per-sample directories, separate environment/command/clone/cleanup receipts, raw sampler streams, full Docker-inspection dumps, per-path catalogs, Store copies, or input trees. Print summaries to the terminal. Reports are generated from compact logs only when explicitly requested. Debug retention is opt-in, scoped, size-bounded and disclosed; never enabled silently on failure.

`perf.jsonl` contains one header, one record per attempted sample, and one final summary. Header: schema/run identity; source revision/dirty-content seal; product/harness/image/platform identity; family/case/seed-or-repetition/input/setup identity; requested N; limits; timing/cache profile. Sample: ordinal; selected phase walls; total CPU and accurately named memory/I/O observations; relevant family work counters; outcome; cleanup; real preparation/clone/transfer wall. Summary: requested/attempted/valid/failed counts and distributions over valid samples only. A partial N-sample run cannot claim all requested samples passed. Performance status and verification status are distinct.

Create a unique run path exclusively. Stream records for interruption recovery; finalize once and never append a later run or overwrite a completed result. A missing summary is interrupted/INCOMPLETE. A reader must not infer PASS from a truncated log. Cap unexpected child output in memory; drain it without allowing disk/memory growth or child pipe deadlock. `failure.log` defaults to at most 1 MiB per invocation with a truncation marker, sanitized bounded error tail, and no credentials. Preserve failed attempt identity/status without retaining every working byte.

Stream bounded coordinator records through stdout to the host supervisor; incrementally persist completed records outside timed calls so OOM/removal does not destroy all evidence. Do not wait for a final `docker cp` to recover the only result. The host owns final status after container cleanup. For verification, collect bounded streamed observations, clean up, then publish the one authoritative receipt on the host; a pre-cleanup container receipt is not final.

The compact verification receipt embeds the bounded observations, digests and certificate material needed to support its checks and proof reuse. Existing compatible receipts can serve as identified proof inputs; do not create a new per-sample certificate directory. Do not claim reusable verification after deleting evidence required to establish that proof. If proof integrity cannot be preserved by the compact receipt and its exact retained proof identities within the deadline, return INCOMPLETE/unsupported. External debug artifacts are explicit, scoped, capped and non-authoritative; they are not automatic output or a substitute for the required receipt.

## 8. Selected verification and cleanup

Use one `verify-selected.py` implementation, invoked directly or through family `verify.sh`. Dispatch from authoritative family/case metadata, not `endswith('-proof')`. Preserve inherited capped repetition semantics and fixed proof identities. Reuse `workspace_verify.rs`, `dedup_verify.rs`, existing SDK/native oracles and structural proof logic; no duplicate oracle in shell/Python adapters.

The hard 59-second end-to-end clock begins before preparation/authentication and includes checking, cleanup, transfer, and final receipt publication. Reserve cleanup time; do not start more checks when it is exhausted. At or beyond 59 seconds the invocation cannot PASS. Exercise delayed hashing, output transfer and receipt publication, not only worker timeout. Use PASS/FAIL/TIMEOUT/INCOMPLETE, never unchanged automatic retries. A retained partial receipt/cleanup failure is nonpassing. The companion owns exact container IDs/labels and must terminate actual container processes, not just its host `docker exec` client.

Initially reserve at least five seconds for owned cleanup/publication and end check work by elapsed54s; reduce the work allowance further when setup consumed it. A late receipt must report its actual elapsed time and deadline failure, never a fabricated below59s wall. If publication itself fails, the external nonzero result is INCOMPLETE and absence of the required receipt cannot satisfy acceptance. Deadline fault injection must include final publication; no final PASS may survive an overrun.

One verification receipt includes identities, checks, sampled paths/ranges, reused proof identities, omissions, monotonic wall, resource precision, cleanup and evidence location. Large fresh preparation is inside the deadline if requested; an exact compatible prepared hit may avoid it. Long semantic proofs such as sustained-600 are explicitly unsupported/INCOMPLETE under this interface, not shortened or passed. Preserve their definitions without running the withdrawn exhaustive suite. A family's adapter migration can be complete with an explicit unsupported long-proof disposition, but that proof remains unverified.

Performance and verification remain separate invocations and never concurrent. During migration, run the companion after the first successful selected performance sample on a changed route, after an actual verification correction, and once for a materially distinct stable route; reuse exact matching PASS without repeating it for N timings. Final verification is at most one selected invocation per family, each below59s and at most600s aggregate; stop with explicit incomplete coverage when a budget cannot accommodate more. Performance scripts do not secretly perform reopen/proof work.

## 9. Implementation checklist: shared foundation first

- [ ] Inventory current entrypoints, registry IDs, timers, oracle routes and callers; pin source and preserve dirty/unrelated work.
- [ ] Build/seal Linux coordinator in runtime image; prove same-container daemon FUSE/Exec with no data mounts or socket.
- [ ] Centralize modes, explicit selection, N/seed separation, proof-only rejection, and fresh-output rules.
- [ ] Implement sealed Docker-local preparation, independent writable sample state, finite cache and fresh sample container lifecycle.
- [ ] Remove Git nested preparation, mount-based references/exchange, and host-side product/resource assumptions through shared callers.
- [ ] Implement compact logs, bounded child output, exact cleanup ownership, failure deadline and retention policy.
- [ ] Check copy-up/timing/resource boundaries and image/platform identities before collecting a new matched timing.
- [ ] Prove two tiny routes serially: fresh initialization and post-init clone with real FUSE/Exec, including failure cleanup and immutable-master isolation.

## 10. Family-by-family migration checklist

For EACH active row, complete these gates in order before marking its top-level box:

1. Transfer family code/launchers and update every real caller while preserving canonical cases, sizes, seeds, limits, and operation timers.
2. Check all three mode outcomes and setup applicability with parser/dispatch checks; unsupported modes fail before runtime startup.
3. Exercise one smallest representative performance route (unless proof-only), then one bounded selected verification when eligible. Do not rerun passing evidence after unrelated changes or execute an entire matrix to check a wrapper.
4. Confirm no data mounts/host product work, no performance-only reopen, compact output only, correct resource names and complete sample cleanup; remove superseded code only after transfer.
5. Record source-bound result and remaining unsupported cases in the issue; infrastructure migration is not product performance qualification. Final changed-route verification follows section8 budgets.

Sequential queue (18 non-archival family IDs, then three historical-entrypoint dispositions):

- [ ] **01 `init_namespace`** — four released profiles. Source-only reuse; fresh output Store; no synthetic Workspace/reopen/FUSE phase in initializer timing. Thin setup/perf/verify adapters; preserve exact released byte/file definitions.
- [ ] **02 `edit_length_preserving`** — 12 SDK cases. Fresh/clone starting Store; preserve singular SDK edit/Commit timer and separate End; real projection where declared.
- [ ] **03 `edit_length_changing`** — 32 historical definitions. Transfer supported cases; preserve five oversized growth originals as history-only under the cap, not silently repaired inputs.
- [ ] **04 `edit_canonical_chunk_count`** — 12 SDK cases. Preserve canonical outcome/control membership; fresh/clone and shared SDK operation path.
- [ ] **05 `edit_length_changing_capped`** — five versioned replacements. Fix companion route/repetition dispatch; fixed-input verification uses repetition1, not synthetic seed expansion.
- [ ] **06 `store_footprint`** — three controls. Initialization-measuring route requires fresh output; move correctness-only reconnect out of performance, preserve declared footprint census and version changed complete timing boundaries.
- [ ] **07 `payload_create_read`** — eight timed cases. Host-independent Docker preparation; preserve authentic write/read operations, byte counts and inherited-anchor identity.
- [ ] **08 `directory_construction_traversal`** — 12 cases. Preserve create/read/scan distinctions and real FUSE routes; no setup substitution for measured construction.
- [ ] **09 `tiny_file_churn`** — 20 cases. Preserve create/stat/unlink/bulk distinctions and complete selected workload; do not use fast mode to shrink operations.
- [ ] **10 `namespace_mutation`** — four cases. Preserve populated background/subtree fixture and authentic rename/delete; fresh/clone setup only outside operation timer.
- [ ] **11 `workspace_change_locality`** — 16 cases. Preserve clean/fixed/dense/distributed paths; distributed SDK edits remain SDK, not rewritten as filesystem writes.
- [ ] **12 `git_tool_workflow`** — four cases. Direct Docker-local fixture/reference preparation; local authenticated normalization input and real Git Exec; zero reference/exchange binds.
- [ ] **13 `mixed_load_bearing`** — four cases. Preserve complete episodes, hard-link relationships, intermediate logical limits and actual tool operations.
- [ ] **14 `dedup_cross_file`** — ten cases. Measured import requires fresh output; qualify input transcript/intersections before timed admission using bounded reusable preparation proof; no product CAS optimization.
- [ ] **15 `dedup_cdc_locality`** — 20 cases plus boundary proof. Fresh output; correct qualification order and explicit selected boundary dispatch; no product CDC optimization or implicit cohort expansion beyond the proof's declared semantics.
- [ ] **16 `dedup_workspace_reuse`** — 12 cases. Fresh/clone genesis; seal observed pre-operation payload attribution identity outside timing; reuse independent final oracle, no product scaling fix.
- [ ] **17 `dedup_branch_history`** — 20 cases. Clone/fresh genesis, execute every Commit in selected tier, preserve Commit-and-continue; no correctness replay in performance and explicit long-verification omissions.
- [ ] **18 `workspace_reliability`** — 28 named subcases/12 recipe groups. Verification-only; reject perf modes. Distinguish same-root collision from same-Branch isolation and no-history from required no-op staging; retain long proofs unsupported under59s rather than running them.
- [x] **19 Registered payload `run.sh`** — retired from the current tree with the user's cleanup authorization. Its five-operation campaign and frozen anchors remain historical Git evidence, not an additional active lane.
- [x] **20 Archival `edit_same_count`** — root runner removed; historical reproduction requires its frozen revision. No benchmark rerun or active-family PASS is claimed.
- [x] **21 Archival `edit_count_changing`** — same explicit historical disposition; no modern-mode compatibility is advertised.

Do not mark an active row complete from compilation or code reuse alone. If a real product failure blocks a representative check, preserve the compact failure, diagnose whether it is adapter or product, and leave the row blocked. Do not broaden this issue into a later-family optimization.

## 11. Acceptance and non-goals

- [ ] Shared checks cover exclusive modes, repeated same-input N, invalid setup, proof/inherited dispatch and no implicit expansion.
- [ ] Every active family has local setup/perf/verify launchers backed by shared implementations, with all sequential gates complete or explicit unverified blockers that keep the issue open.
- [ ] Runtime inspection rejects injected binds, named/anonymous volumes and image-declared volumes; normal product route works without Docker socket or nested Docker.
- [ ] Master/sample isolation, cache hit/miss/tamper/capacity/active-entry protection, copy-up policy, and scoped cancellation cleanup pass focused checks.
- [ ] Artifact census proves successful perf produces only `perf.jsonl`; selected verification produces only `verification.json`; failure log respects the 1 MiB cap. No retained sample database/container or one-use image remains after successful cleanup.
- [ ] JSONL interruption/summary math and fake delayed59s verification/receipt/cleanup tests cannot produce a false PASS; foreign Docker objects remain untouched.
- [ ] Every historical entrypoint has a transfer or archival disposition; documentation stops claiming unsupported universal modes or inherited proof coverage.
- [ ] Final report in this issue is a short migration checklist/status and compact log links, not a full performance matrix or a new generated-report tree.

Completion means benchmark infrastructure is migrated and bounded representative checks passed; it does not mean all family cases are verified, all performance targets passed, Phase1 verification resumed, or v0.1.3 is release-ready. Preserve those distinctions in every summary.

## 12. Implementation checkpoint (2026-09-05)

At the `db4703274` checkpoint, the compact fixture changes and Docker-only
adapters were implemented. This was not a terminal all-family PASS: the product
Monitor mismatch below kept the selected checks nonpassing. Builds/measurements
were serialized; that checkpoint changed no product crate. See the scoped
Monitor repair below for subsequent results.

- 73 versioned compact cases/proofs are present: 34 ordinary, six Workspace
  reuse, two namespace, three Store-footprint and 28 reliability definitions.
  Registry and descriptor self-checks confirm the compact byte/file bounds.
- Higher operation tiers are excluded from automatic smoke selection even
  when their initial state is empty/small. Fixed 500 MiB capped replacements
  are explicitly large-only. Higher-tier workloads are not rerun here.
- Linux image builds and 19 focused Python checks passed. The last tested
  image is `layerfs-bench-infra:504f4c047fe1d9dc`; earlier retained compact
  smoke receipts identify their actual preceding image/source/harness seals.
- Logs live locally under
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-infra-smoke-20260905`.
  They contain compact performance/verification receipts and failure logs,
  not copied inputs/Stores. No owned sample container remains after checks.

| Selected compact route | Performance | Verification end-to-end |
| --- | --- | --- |
| Namespace 5 MB / 100 files | PASS; initialization 18.51 ms | PASS, 3.27 s |
| Namespace 20 MB / 1,000 files | PASS; initialization 53.51 ms | PASS, 3.43 s |
| Directory construction 1 MiB / 50 background files | PASS; product-call sum 22.51 ms | PASS, 8.71 s |
| Store-footprint compact metadata control | PASS | PASS, 3.08 s |
| Payload creation tier 1 | PASS | PASS, 2.43 s |
| Namespace mutation tier 1 | PASS | PASS, 2.56 s |
| Workspace clean Commit tier 1 | PASS | PASS, 2.82 s |
| Git workflow tier 1 | PASS | PASS, 3.80 s |
| Mixed episode tier 1 | PASS | PASS, 2.76 s |
| Workspace exact reuse tier 1 | PASS | PASS, 2.74 s |
| Reliability dirty-net-zero, 1 MiB / 10 regular paths | Proof-only | PASS, 9.03 s |
| Tiny bulk-create tier 1 | FAIL: Monitor candidate validation | Not claimed |
| Directory construction tier 10 | FAIL: same Monitor validation | Not claimed |

Previously passed 1 MiB SDK edit, cross-file, CDC and one-Commit history
smokes retain their original image identities; their already-small fixtures
were not expanded or gratuitously rerun. `fresh` setup and two independent
same-input performance samples were also exercised. These are selected
smoke observations, not matched performance distributions or scaling claims.

Retained failures and corrections:

- Old directory construction tier 1 required a 500 MiB/100,000-file background:
  first setup was 79.87 s and full verification timed out at 41.19 s. The new
  compact case's observed first setup was 8.54 s. Different fixtures and cache
  histories mean this is a setup observation, not a product speedup claim.
- Setup migration failures (raw image-ID FROM, wrong Unix-socket readiness,
  daemon-as-PID1 resource collection, prepared manifest allowlist, Git image
  identity and reliability's host Docker helper/master path) were retained,
  diagnosed and corrected. Corrected selected checks passed.
- Tiny bulk-create and directory construction tier 10 reach a stale product
  telemetry gate: Store admission/receipt validation accepts up to 8,191
  objects, while `layerfs-monitor/src/operation.rs` still requires Workspace
  Commit transactions to contain fewer than 128. The SDK may report this
  integrity error after publication; it is not evidence of rollback. Keep
  these outcomes failed until the product contract is repaired and rechecked.
  This checkpoint does not silently weaken the gate or claim those cases pass.

Every retained verification invocation, including failures, completed below
59 seconds; compact successful invocations above took 2.43–9.03 seconds.
Long endurance proofs remain unsupported under the bounded interface.

## 13. Authorized root-entrypoint cleanup

The user approved the cleanup after reviewing the dependency inventory.
All 24 former root-level shell files were addressed: 21 run scripts and two
shell libraries removed, and the unchanged daemon entrypoint moved into
`shared/`. The obsolete `workspace-runner.py`, `sdk-edit-custody.py`, and
their two mutually dependent report generators were removed together so
no executable is left importing deleted modules. Historical versions remain
recoverable from Git; no archive folder or replacement wrapper was added.

The Dockerfile's COPY source was updated; the installed entrypoint path and
script bytes remain unchanged. A focused layout test checks the absence of
root shell scripts/retired modules, all 18 families' 54 shell entrypoints,
their dispatch targets and executable bits, and the Docker COPY source.
All three public script styles also support `--help` without requiring a
runtime or selected input; the verifier's pre-parse seed guard was corrected
with a failing-then-passing regression check. Twenty-two focused Python
checks passed. The relocated entrypoint is byte-identical; its Docker COPY
input was validated without rerunning a benchmark matrix or rebuilding the
full image.
The active README now documents only current commands. Historical roadmap
and results documents retain their historical references.

This is structural cleanup, not a performance qualification or a product
repair. Product changes already present in the worktree are outside this
cleanup and are not included in its commit.

## 14. Selected Monitor admission-limit repair (2026-09-05)

The two failed selections above shared a stale product validation limit, not an
oversized-fixture or slow-operation failure. Store admitted up to 8,191 objects
per transaction, but `CandidateStats::validate_for` still required non-initializer
operations to contain fewer than 128. SDK observation happens after the operation,
so this Monitor error could mask a successful publication; it did not imply rollback.

The follow-up local patch uses Store's `ADMISSION_BATCH_COUNT` and existing
`OBJECT_PAGE_BYTES` in Monitor, removes the duplicate family-specific limit, and
makes the initializer's count constant an alias of the same Store constant. All
accounting equations and the strict <4 MiB byte bound remain enforced. The
`validate_for` signature remains compatible.

The new regression check failed before the fix and passed afterward. It covers
127/128/512/8,191 objects for Commit, Resolve, Initialize and Add; rejects 8,192
objects, a 4 MiB batch and inconsistent accounting; and exercises default validation.
Both tests in the Monitor integration-test file passed, including passive snapshot
and dedup analysis. The Linux benchmark image rebuilt successfully.

These are one-sample selected checks, not matched speedup/scaling results:

| Case / seed 1 | Declared product time | Exec | Commit | Maximum transaction objects | Selected verification |
| --- | ---: | ---: | ---: | ---: | --- |
| `tiny-bulk-create-1-compact-v2` | 145.450958 ms | 98.164250 ms | 39.180625 ms | 652 | PASS, 2.846490 s |
| `directory-construct-10-compact-v2` | 36.095375 ms | 24.485042 ms | 6.315708 ms | 158 | PASS, 3.384589 s |

All four invocations report cleanup PASS, zero OOM kills and zero current swap;
both verifiers finished below the hard 59-second deadline. No owned sample
container remained after these runs. Both transaction counts exceed the obsolete
127-object bound and remain below Store's real limit.

Evidence-producing source is `db4703274` plus the four-file local product/test
patch, not a new committed revision. Source seal:
`1a53315a4811c4319b23942f5e98745b0436718b6e49c84eba6b91c237e2fc95`.
Image: `layerfs-bench-infra:1a53315a4811c431`, digest
`sha256:52eac7c40ffbff87680597c1a203057b0dacf41dfebc4c47ef1d6b22cb530256`.
The image digest is recorded authoritatively in the compact receipts below.

Compact evidence root:
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-infra-smoke-20260905/monitor-admission-fix-1a53315a4811c431`.
Each `tiny-perf` / `directory-perf` contains `perf.jsonl`; each `tiny-verify` /
`directory-verify` contains the identity-matched `verification.json`. Original
failed attempts remain untouched. This fixes those two selected blockers only;
it does not close #45, qualify every family or run the large-only cases.

## 15. Issue #45 closure verification (2026-09-05)

The user requested independent verification of the other task's repair and closure
of #45. Both previously failed cases were rerun serially, each followed by its
separate identity-matched companion. No full matrix or large-background case was run.

| Case, seed 1, clone setup | Product-call sum | Container command CPU | Container lifetime peak | Verification wall | Outcome |
| --- | ---: | ---: | ---: | ---: | --- |
| `tiny-bulk-create-1-compact-v2` | 159.349877 ms | 165.599 ms | 15,511,552 B | 2.883782 s | perf / verification / cleanup PASS |
| `directory-construct-10-compact-v2` | 50.014250 ms | 74.726 ms | 22,495,232 B | 3.057962 s | perf / verification / cleanup PASS |

These are single-sample smoke results, not speedup estimates. Command CPU is
inclusive container command-window CPU, not product-call-only CPU; lifetime
memory includes setup. Both performance receipts report zero benchmark reopen
and verifier calls. Maximum transaction objects were 652 and 158, respectively:
both exercise the repaired gate. All four invocations report cleanup PASS,
zero OOM kills and zero current swap. The sample Stores were independent
Docker-local byte copies with equal master/sample hashes and distinct inodes.

The current host harness is `cab5793e8cd2949c58c4ead41ecb12ba826bfee7c54b67f4406c9192c2aa9549`.
The reused image/source identities are those in section 14. Its product seal
`1bf45522946c68d57effbc4334a93100af208b79f21f3f237a45927d596cd84e`
matches the current product files. The current whole-source seal differs
(`1127c5640bdf106f6fab9b6f6b90625e7e2f91826cd42731e10a0a087406b37c`)
because root-entrypoint cleanup followed the image build; do not describe the
image as built from cleanup commit `3bcc31bdadca689eba2a5d6a2ffb73ca9b83e174`.
The four-file product/test repair remains an uncommitted change owned by the
repair task; this closure does not overwrite or include it in a documentation commit.

Current closure disposition for the sequential checklist (sections 9–11 retain
the original detailed design gates; this table records the observed migration
coverage, not an assertion that every fault permutation was tested):

| Checklist rows | Closure disposition |
| --- | --- |
| 01–04, 06–17: 16 small-performance families | Family-local adapters transferred; selected performance and separate verification PASS receipts retained. Unrelated already-passing routes were not rerun. |
| 05: capped length-changing family | Interface transferred; fixed 500 MiB cases explicitly large-only, excluded from smoke by the latest user scope. No live PASS claimed. |
| 18: reliability | Verification-only interface; compact dirty-net-zero proof PASS. Long endurance proofs remain unsupported/unverified under 59 seconds. |
| 19–21: historical entrypoints | Retired with explicit user authorization; frozen Git history preserves reproduction. |
| Shared harness and layout | 22 focused Python tests PASS on the current harness; 18 family directories / 54 launchers; no root shell wrappers. |

Retained evidence covers 17 families with selected verification PASS (16
performance-capable small families plus the proof-only family). The complete
local receipt census contains 26 verification attempts, including original
failures and these user-requested rechecks: 134.317113 s combined, maximum
41.188612 s. Every recorded invocation is below 59 s; historical FAIL/TIMEOUT
receipts remain unchanged. This census is not a new final-family campaign.

Closure evidence root:
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-infra-smoke-20260905/issue45-closure-recheck`.
Only four files are retained (97,436 bytes total):

| Relative evidence path | SHA-256 |
| --- | --- |
| `tiny-perf/perf.jsonl` | `444303230c6803f4f7e57a1caeca6efe16e65e3ddc11dc0f32937ffa554c3fa7` |
| `tiny-verify/verification.json` | `c69a5351ca31a0e1c725a0a3f3ad912f4e9e1260293f539fc72bffcf20816734` |
| `directory-perf/perf.jsonl` | `9899eb21a20ea29928d7395d885195b43f3e6142b406999da8670d3619ac0c86` |
| `directory-verify/verification.json` | `901b34a204d90c720188cb4e81ecbd9c4dda5332b4d7224c8da2e3f22f7d6509` |

#45 is complete for the user-authorized infrastructure migration and smoke
scope. The large-only capped cases, remaining reliability proofs, higher tiers,
unselected cases and performance targets are not qualified by this closure.
No merge, push, release, Phase 1 replay or later-family optimization is included.
