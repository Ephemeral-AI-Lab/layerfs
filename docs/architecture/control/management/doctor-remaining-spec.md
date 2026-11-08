# Doctor remaining backend work and closure specification

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Specification for remaining backend implementation and qualification. It does
not claim that the cases below have passed.

Tracking: [parent #116](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/116).
Implementation baseline: `ff06da3a71e946bc75121ce81dc8f16e37109224` on
`doctor-implementation-116`, based on `layerfs_v2_0_0` at
`716197c4999fb087aa0fb17bac5ceb48b0b556ae`. Recheck the target at integration.

## 1. Scope and existing evidence

Finish the backend diagnostics and demonstrate enough correctness, safety and
supported-runtime coverage to close #116 and land on `layerfs_v2_0_0`.

Included: Core validators, Engine diagnostic sessions/inventories, Control
orchestration, worker/daemon diagnostic transport, provider observations,
PostgreSQL connection/online checks, S3 probes, backend tests and operator docs.

Excluded by owner direction:

- Frontend work: CLI parsing/presentation, Node/Python SDKs, public frontend
  parity, package installation/publishing, MCP and UI work. Reuse existing
  frontend wiring and evidence; do not create a new frontend workstream.
- Garbage collection, compaction, historical deletion and issue #41.
- New database formats, new machine platforms, automatic repair, and general
  filesystem or backup performance campaigns.
- Durable execution. Fixtures explicitly use Disposable; retained Durable code
  may compile but is not executed or counted as qualified.

Tests may invoke the internal diagnostic entry points directly. Preserve their
existing caller contract; a necessary backend correction must not silently
change selectors, defaults or report semantics.

Existing evidence to reuse, with its original pins and limitations:

| Evidence | Established baseline |
| --- | --- |
| Combined Doctor suite | 99 passing tests, including five actual PostgreSQL17 cases; receipt `cargo-macos-5cf15f78999d484ba423a3cfa163ee62` |
| Final affected S3/Backup suite | 24 passing tests, including ordinary connection reuse; receipt `cargo-macos-bfeaafcb3a9d4366b79746b30f47d564`; overlaps the combined suite |
| Exact overwrite validation | Graph-derived oracle accepts late publication/merge/fork histories and rejects seven altered counts |
| SQLite structure | Actual bad header and damaged B-tree; unsupported schema, wrong binding, Busy and expired-budget distinctions |
| Cancellation | Source/scratch interruption, real worker cancellation, PostgreSQL query cancellation and independently observed snapshot release |
| Source checks | All-target Clippy, changed-file formatting, dependency boundary guard and tooling tests passed |

The existing Node/Python results remain historical evidence outside this spec.
Do not repeat unchanged passing suites merely to increase test totals. Rebuild
and rerun affected cases when source, features, fixtures or dependencies change.

## 2. Required diagnostic behavior

| Observation | Required result |
| --- | --- |
| Confirmed malformed authoritative bytes, impossible references or derived-index disagreement | Failed integrity check with an identifiable object/row/context and error severity |
| Wrong saved Store binding | Authority/configuration failure; block affected logical checks; do not call it content corruption |
| Unsupported schema, relation, daemon operation or connection selection | Explicit unsupported/unknown coverage, with the affected checks named |
| Denied access, authentication or unavailable transport | Distinguish the observed access failure from corruption; dependent coverage remains blocked/unknown |
| Deadline, cancellation or lost reply | Preserve earlier findings, mark unfinished coverage incomplete, and do not replay the operation |
| Damaged advisory hints/signatures/digests | Identify advisory damage separately; do not infer namespace loss from the advisory alone |
| Quick-mode exclusion | Explicit skipped check, with the omitted guarantee named |
| Legitimate retained, sparse or duplicate data | No invented corruption finding |

`complete` describes coverage of the selected mode, not health. A complete audit
can report confirmed failures. Quick can complete its reduced scope while
explicitly skipping full-only checks. Requested applicable work that is
unavailable, interrupted or truncated prevents complete coverage. A full audit
must not silently omit an applicable table or object category.

Reports and traces must not disclose passwords, credential-bearing URLs,
authorization headers, signed URLs, worker secrets or daemon control keys.
Retained findings/evidence remain bounded; counts survive truncation. Evidence
from a dropped repeated check ID must never attach to an earlier object.

## 3. Remaining work packages

### B1. Exhaustive inventory and acquisition coverage

Owner layers: Engine and Control. Issues: #121, #124, #125.

Produce a coverage map from the actual supported schema/profile definitions.
For every table/category, record its authoritative/advisory role, inventory
key, snapshot source, validator, quick/full policy and exclusion reason, if any.
Cover catalog/history, packs and physical groups/records, winning locations,
ordinal blocks, hints, signatures, digests and every applicable acquisition or
auxiliary table. Inspect real keys; allocator ceilings are not row inventories.

Close any uncovered applicable category with a bounded validator, or report
explicitly unverified coverage. A documented omission alone does not satisfy a
full-integrity guarantee that requires that category.

Acceptance:

- Healthy empty Stores and supported layout/acquisition combinations pass.
- Active acquisition and interrupted-but-valid retained state are distinguished
  from malformed acquisition metadata and dangling authoritative references.
- Windows are bounded and disk-backed scratch is used where total state grows.
- Actual BLOB lengths are checked before materializing malformed bodies. Use
  format/profile limits, not invented total Store/file limits.
- Progress events keep long worker calls observable without consuming retained
  finding entries or changing their counts.

### B2. Content, dependency and namespace adversarial qualification

Owner layers: Core validators, Engine inventory/reader and Control outcomes.
Issues: #122, #123, #124, #127.

Use real persisted data and the existing canonical encoders/readers. Where a
fixture needs valid hashes around invalid semantics, recompute the enclosing
hashes so the intended semantic validator is exercised.

| Case | Mutation or setup | Required evidence/control |
| --- | --- | --- |
| C01 Pack/storage framing | Truncated pack, changed digest/length, malformed group/record or oversized body | Identify structural failure without allocating the declared oversized body |
| C02 Canonical identity | Change object bytes and reseal the pack | Object authentication still fails |
| C03 Locators | Wrong role, offset, length, group or missing winning location | Name the failing binding; unrelated valid objects remain independently checkable |
| C04 Dependencies | Missing base, actual dependency cycle, over-depth and a fully replacing delta with a corrupt required base | Detect the actual violation; valid forward dependencies pass |
| C05 Ordinals/blocks | Missing ordinal, malformed block, checksum damage and unused damaged block | Inspect applicable stored rows even when current heads do not reference them |
| C06 Mappings/summaries | Invalid mapping, logical length or child summary with otherwise valid identity | Semantic failure; valid sparse files pass |
| C07 Attributes/symlinks | Malformed portable/non-portable attributes and symlinks | Applicable syntax/context fails; valid dangling symlinks pass |
| C08 Namespace context | Aliases, islands, invalid counts or incompatible root/scope/profile context | Contextual failure survives canonical-object deduplication |
| C09 Retained data | Corruption only in historical roots or unreachable winning objects | Full detects applicable damage; quick names the skipped coverage |

For each corruption fixture include a valid negative control. Duplicate physical
representations, reservation gaps, removed-project retained data and first-wins
forward dependencies remain valid. Check retained objects' locally decidable
semantics without inventing a namespace context that no longer survives.
Distinguish winning locators from valid losing physical representations; do not
impose a mixed winner/loser dependency graph on a losing representation.

Existing core proof can cover a validator's unchanged semantics. Add Doctor-level
proof for its inventory reachability, error propagation and outcome classification;
do not duplicate every existing core test without a specific missing guarantee.

### B3. Authoritative history and derived-index qualification

Owner layers: Core and Engine. Issues: #123, #127.

Retain the implemented independent height/depth/newest/run/absorption/reach and
overwrite checks. Expected answers must come from authoritative graph facts,
not ordinary ancestry APIs that trust the indexes under examination.

Acceptance:

- Missing/cross-stack heads, bases and parents, cycles, identity mismatch and
  altered Layer provenance produce distinct attributable findings.
- Independently damage each derived-index family and verify it is detected.
- Valid merge, fork, forced history, publication and retained history after
  removal pass under current semantics.
- Qualify applicable historical roots, including roots absent from current heads.
- Preserve the exact overwrite oracle's pre-publication semantics, including
  late publication already represented through Layer ancestry and modern empty
  legacy overwrite blobs.
- Old namespace/context damage is not hidden by a valid current root or an
  earlier successful qualification of the same canonical object.

### B4. Snapshot coherence, no mutation and cancellation

Owner layers: Engine and Control. Issues: #117, #121, #124, #125, #127.

| Scenario | Required proof |
| --- | --- |
| Publication, removal or pack insertion during paging | One logical snapshot; no late-row mixing or false missing references |
| Committed uncheckpointed SQLite WAL | Audit includes committed WAL content without doctor requesting a checkpoint |
| Remote SQLite placement | Audit runs through the authoritative same-kernel worker, not a copied database or daemon Store-call route |
| SQL/lock wait | Actual query interruption and snapshot release, observed independently |
| Hash/object traversal and scratch SQL | Cooperative/actual interruption at bounded work boundaries; watchers join and scratch is released |
| Worker/daemon/provider/DNS/TLS/S3 delay or disconnect | Owned sockets/processes terminate or explicitly uncertain release is reported; no abandoned resolver/watchdog or mutation replay |

Use independent connections/processes and lifecycle/write counters, not just
elapsed time or a returned cancellation error. Capture before/after logical
state and lifecycle effects. Doctor must not create/convert a home, migrate or
backfill a Store, acquire source lease/lock records, refresh placement, start or
deploy a runtime, create a forward, repair, sweep, checkpoint or run GC.

SQLite's supplemental raw structural probe remains a separately labeled physical
view after failed logical admission/audit. It does not replay the logical audit
or validate its original snapshot, and is skipped after deadline/cancellation.
Likewise, PostgreSQL online physical checks have a separate observation interval.
Backend physical bookkeeping is distinct from prohibited LayerFS logical writes.
In concurrent fixtures, attribute the deliberate writer's changes separately
from diagnostic effects. Byte-for-byte unchanged assertions apply to quiescent
fixtures; they do not prohibit legitimate concurrent publication or SQLite's
physical shared-memory bookkeeping.

Retain the documented PostgreSQL cancellation cleanup grace of at most one
additional second. Local synchronous filesystem/CPU observations check budgets
at their defined boundaries; report those boundaries rather than promising
universal hard preemption of kernel calls.

### B5. PostgreSQL access and physical-check matrix

Owner layers: Engine and Control. Issues: #125, #127.

Qualify correct/wrong binding, absent schema, denied schema/table access, wrong
password, unreachable endpoint, stalled authentication, configured TLS failure,
and connection loss during an audit. Include denial while reading advisory data
and while qualifying namespace content. Preserve typed access/provider errors.

Exercise `amcheck` present, absent and denied; supported relation/function
signatures and unsupported relations; and replica behavior. A supported case
must work; an unsupported case must explicitly limit coverage. Diagnosis never
installs an extension, invokes disruptive strong-lock checks or claims offline
server-file checksum verification.

Retain lazy configured host-order admission, Unix/TCP fallback and TLS hostname
identity. Randomized multi-host diagnostic admission may remain explicitly
unsupported; this spec does not add that capability. Qualify bounded Linux
resolution and verify oversized physical payloads are rejected before decoding.

Physical results identify actual checked/skipped/unavailable relations. Logical
results identify their pinned snapshot. Permission denial, replica limitations
and physical-check unavailability must not become corruption or healthy coverage.

### B6. Real Host, Machine, Sandbox and backup environments

Owner layer: Control, with Engine's narrow registry inspector.
Issues: #118, #119, #120, #126, #127.

| Environment | Required qualification |
| --- | --- |
| Host | Default/configured artifacts and manifest compatibility, local capacity/identity, current/absent/previous/malformed home, live/dead owners and observable record races; unused optional tools do not suppress independent checks |
| Machine | Supported native route, Docker and one supported VM route; provider running/stopped/missing, missing tool, active intent, endpoint/secret/version/ping failure and timeout; no lifecycle effects |
| Sandbox | Local, actual worker and forwarded routes; stale/restarted daemon, lost runtime, reused carrier PID, missing worker record, denied authentication, Busy, changed/interrupted pages and cancellation |
| Linux runtime | Actual worker/daemon diagnostic exchange and bounded `getent` resolver execution, including helper cleanup |
| Versioned MinIO | Saved-prefix listing and owned-version PUT/GET/DELETE using actual deployment headers and permissions; existing versions and unrelated backups unchanged |
| R2 | Configured TLS/connection/listing; optional write test remains unsupported with no PUT when safe immutable-version ownership cannot be established |

Keep existing S3 fault-peer tests for collision, uncertain mutation replies,
wrong bytes, replacement preservation and denied/lost cleanup. Do not repeat a
real mutation to resolve uncertainty. Report only the exact possible retained
probe key/version; do not authorize deletion of an unowned object.

Sandbox control readiness/registry enumeration does not prove FUSE/NFS serving,
mutable-overlay integrity or Store health. Authentication-close ambiguity remains
unknown unless a typed protocol response establishes the cause. These boundaries
are intentional and must not be turned into false failures or unsupported claims.

## 4. Execution and evidence rules

Each case records: requirement ID, source commit, feature flags, binary/bundle
identities, OS/provider/database version and configuration, explicitly Disposable
fixture identity, setup/mutation, diagnostic invocation, expected check/outcome,
observed findings/coverage, mutation/release evidence and final verdict.

- `PASS`: the specified expectation and safety assertions were demonstrated.
- `FAIL`: wrong result, hang, unsafe effect, leaked secret or resource-release failure.
- `NOT_RUN`: unperformed qualification; it is not a pass.
- `BLOCKED`: a named prerequisite prevents the case; retain the owning issue.
- An expected unsupported result is a passing contract test only when this spec
  permits that capability to be unsupported; it does not qualify the capability.

Use isolated disposable databases, homes, buckets/prefixes and runtime resources.
Never damage live user data. Keep operator credentials outside reports and source.
Use registered private worktree resources; build tests first and bound each test
invocation to at most 120 seconds. Preserve failures and diagnose before rerunning.
Do not invent a performance campaign or treat memory hints as bounded-state proof.

Product fixes/docs and crate-local regression tests belong in `layerfs`.
Reusable cross-environment harnesses/qualification ledgers belong in
`layerfs-benchmark`; link exact receipts from #127. Keep algorithms in Core,
database mechanics in Engine and network/provider orchestration in Control.
Do not relax dependency guards to make a diagnostic implementation fit.

Execution order: reconcile evidence; finish B1 coverage; complete B2/B3 fixtures;
close B4/B5 gaps; run B6 real environments. Independent environments may run in
parallel with separate writable resources. A discovered defect returns to its
owning layer and requires affected-case verification before completion.

## 5. Close #116 and merge

- [ ] B1–B6 have implementation/qualification evidence for every applicable
  requirement; any remaining omission is explicitly outside the accepted scope.
- [ ] No known Doctor correctness, false-health, unsafe mutation, secret leakage,
  unbounded extraction or cancellation/resource-release defect remains.
- [ ] Update operator docs with actual coverage, full/quick differences,
  long-read effects, unsupported cases and safe recovery suggestions. Doctor
  suggests actions but performs no repair or GC.
- [ ] Reconcile #117–#127 checklists against evidence. Remove stale pending
  statements for already delivered overwrite, SQLite structure and integration
  work. Frontend additions/qualification are excluded from this remaining scope;
  retain prior evidence and move any separately desired frontend work out of it.
- [ ] Review the exact integrated candidate against the current
  `layerfs_v2_0_0`; resolve conflicts and verify affected behavior if it changes.
  Record source/binary identities and known independent baseline failures.
- [ ] Pass scoped local builds, affected backend tests, formatting, Clippy and
  dependency guards. Record production LOC and required attribution per commit.
  CI reports are supplementary: repository policy has no required CI gate and
  no wait-for-CI/pre-push wrapper is introduced.
- [ ] Publish the PR and final backend coverage report, merge the reviewed
  candidate, link the actual merged commit/receipts, then close #116 and the
  fulfilled backend subissues. Do not close on a local branch alone.

GC is not a dependency. An unrelated baseline failure, including #115, is
tracked separately and does not become Doctor scope by association; independently
reproduce and record it, and leave any affected qualification case unfinished
until it has valid evidence. A green unrelated run cannot replace this proof.

## Related implementation contracts

- [Doctor overview](doctor.md)
- [Store diagnostics and snapshot/coverage boundaries](../../engine/store/diagnostics.md)
- [Machine diagnostics](doctor-machine.md)
- [Sandbox diagnostics](doctor-sandbox.md)
- [Backup connection and temporary-write safety](doctor-backup.md)
