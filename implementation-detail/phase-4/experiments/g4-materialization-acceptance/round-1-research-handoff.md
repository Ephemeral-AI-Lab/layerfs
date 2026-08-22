# G4 Round-1 reconstruction and materialization research handoff

Copy the prompt below into the next Codex task.

---

```text
/goal Complete the read-only/disposable-experiment Round 1 that precedes Phase
4 G4. Research reconstruction and native materialization together, challenge
the current architecture from the full CAS + CDC + COW + canonical identity +
filesystem storage + VFS/projection perspective, and deliver an evidence-backed
G4 research decision package. Do not implement or promote a production
candidate, run G4 acceptance measurements, or start G5.

AUTONOMY AND RESEARCH POSTURE

Work autonomously through the complete Round-1 research package. Do not ask the
user to choose research questions that can be answered from the repository,
retained evidence, primary technical sources, or short disposable experiments.

You must launch three independent subagents after freezing custody. The lead
agent must also read the governing documents and inspect the code itself; do
not outsource understanding to subagents.

The user explicitly encourages bold, architecturally disruptive proposals. You
may propose replacing or redesigning data structures, object layout, mapping,
authentication proofs, read APIs, storage organization, verified-seed policy,
native publication, or VFS boundaries if evidence supports the change. This is
permission for disruptive architectural research—not permission for destructive
filesystem or Git actions.

Do not assume the present SQLite/CAS shape, chunk profile, mapping topology, or
benchmark-private G3 mechanism is optimal merely because it is current. At the
same time, do not call novelty evidence. Every proposal must be derived from
actual code paths, exact retained counters, a defensible complexity/resource
model, primary sources, and, where useful, a short falsifying experiment.

REPOSITORY / CUSTODY

Work only in:

  /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty

Branch:

  codex/empty-worktree

Starting checkpoint:

  5c342f0ae24ecc69f2bfc03da1c05d1074fe956a

Expected starting worktree: clean.

Never touch the sibling `layerfs` repository. Do not reset, clean, rewrite
history, amend, delete retained evidence, chmod sealed evidence, or commit.
Never start G4 measured acceptance, G5, WP5, or production integration.

Before work, freeze and record:

- pwd, branch, HEAD, status, tracked diff, untracked set;
- compiler/toolchain/OS/filesystem and CPU/memory environment;
- source hashes for every file inspected or experimentally copied;
- G0/G1/G2/G3 manifest, terminal, executable, source-set, and static-closure
  hashes;
- the exact reusable 1/10/100-MiB fixture/base hashes;
- whether target benchmark locks or other measured campaigns are active.

Preserve sealed and historical G3 attempts byte-for-byte. Use the controlling
v13 evidence, not historical v11.

CONTROLLING G3 / CURRENT STATE

G3 is complete, committed, and sealed:

- disposition: `G3 PASS / G4 READY`;
- G4: planning-only and UNSTARTED;
- Phase 4: incomplete;
- production integration: false;
- controlling commit:
  `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`;
- controlling result root:
  `target/phase4-g3-incremental-materialization-20260822-v13/results-v13`;
- G3 source set:
  `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`;
- G3 executable:
  `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`;
- raw JSONL:
  `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c`;
- static closure:
  `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531`;
- 67-entry payload manifest:
  `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49`;
- terminal:
  `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e`;
- terminal verification:
  `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6`.

G3 retained a benchmark-private same-open protected verified seed:

  full-authenticate parent once
    -> exact-verify private seed
    -> reopen read-only/no-follow
    -> unlink while retaining descriptor
    -> bind single-use authority permit
    -> APFS fclonefileat into private temp
    -> authenticate canonical changed-range proof
    -> patch exact range
    -> data/metadata sync
    -> no-follow rename
    -> directory sync
    -> exact old/new reconciliation and cleanup

Attempt A—trusting an ordinary user-editable destination from receipts,
metadata, inode/timestamps, or watcher hints—remains a static NO-GO. Attempt B
does not establish persistent cross-process authority or production integration.

CURRENT PERFORMANCE AUTHORITY

Treat the following as retained observations, not immutable targets:

| Operation | Current retained result |
|---|---:|
| 100-MiB durable full create | 308.884052 ms / 323.746 MiB/s |
| Full-create writer maximum RSS | 12.48 MiB |
| SQLite writer cache snapshot maximum | 8.35 MiB |
| Same-open same-count edit | approximately 5-7 ms |
| 100-MiB +1 early/middle edit | 5.108 / 4.576 ms |
| First edit after reopen | approximately 154.019 ms |
| Warm 100-MiB complete authenticated reconstruction | 338.776 ms / 295.180 MiB/s |
| Fresh-process 100-MiB reconstruction | 366.357 ms / 272.958 MiB/s; source cache warm-or-unknown |
| Authenticated returned 1-MiB range | 2.279 ms / 438.749 MiB/s |
| G3 10-MiB qualified seed no-op/clone | 0.993791 ms |
| G3 100-MiB one-byte incremental materialization | 3.414166 ms |
| G3 10-MiB 1-MiB incremental materialization | 2.926167 ms |
| First complete native materialization | Unavailable |
| Controlled-cold reconstruction/materialization | Unavailable |
| Trusted-seed full 100-MiB reconstruction/read | Unavailable |

G2 decomposed the 100-MiB complete logical reconstruction into these median
work families:

- canonical authentication: 94.817 ms;
- closure commitment: 88.483 ms;
- source/output fingerprint: 87.890 ms;
- SQLite BLOB acquisition: 59.404 ms;
- secondary byte decode: 0.141 ms.

Only the 0.141-ms secondary decode was directly removable under the then-current
authority. Challenge whether a different authenticated representation or root
contract can safely change that conclusion, but do not silently delete required
proofs.

READ FIRST — FULLY, BEFORE PROPOSING A DESIGN

The lead agent and relevant subagents must read the actual files, not summaries
alone:

1. `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md`;
2. `implementation-detail/phase-4/README.md`;
3. `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md`;
4. `implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md`;
5. `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md`;
6. G3 v13 preregistration, counter dictionary, runner, both analyzers,
   finalizer, raw evidence, analyses, cleanup, static closure, manifest,
   terminal, and terminal verification;
7. G3 v11 post-seal reaudit and v12/v13 repair contracts, so the research does
   not reintroduce already-found authority/Q/cleanup/range-proof defects;
8. sealed G2-v5 terminal, analyses, raw evidence, and decomposition ledger;
9. `research/phase-4/handoffs/hot-cold-materialization.md`;
10. `research/phase-4/assurance/verification-security-resources.md`;
11. `research/phase-4/foundations/invariant-matrix.md`;
12. `research/phase-4/foundations/benchmark-and-evidence.md`;
13. `research/phase-4/foundations/hypothesis-ledger.md`;
14. `research/phase-4/decision-map.md`;
15. `research/phase-4/core/canonical/` reports;
16. `research/phase-4/core/pipeline/` reports;
17. `research/phase-4/storage/` reports, including compression and SQLite;
18. Phase-4 algorithm spec, lifecycle, tests, and complexity analysis;
19. Phase-3 delta/COW contracts and relevant evaluation requirements;
20. every current caller and implementation involved in:
    SQLite object access, mapping traversal, canonical authentication,
    reconstruction, range reads, seed creation, clone/patch, native output,
    publication/reconciliation, capture/edit, and VFS/OS boundaries.

Explicitly trace at minimum:

- `crates/layerfs-engine/src/lib.rs`;
- `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`;
- `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs`;
- `crates/layerfs-core/src/canonical_v2.rs`;
- CAS, content, mapping, COW, delta, validation, identity, object codec, CDC,
  OS, VFS, and SDK modules;
- Cargo feature/dependency topology and the actual SQLite/rusqlite APIs used.

Do not treat this prompt's candidate examples, thresholds, or architecture
questions as evidence. Verify or reject them.

ROUND-1 SUBAGENTS — REQUIRED

Launch exactly three initial read-only research subagents in parallel. Give each
clear ownership and tell all of them they are not alone, must not edit shared
source/docs, must inspect code/evidence before searching externally, and must
return citations for external claims.

### Subagent A — reconstruction architecture and authenticated read path

Mission:

- trace DB/CAS -> mappings -> canonical authentication -> closure/proof ->
  logical output and ranges;
- identify every complete or partial byte pass, hash, BLOB open, allocation,
  copy, query, and proof fold;
- distinguish required authority from benchmark-only or redundant evidence;
- examine trusted-seed full reads as distinct from clone materialization;
- propose format-preserving and disruptive representations that could improve
  cold, warm, range, scrub, and full native materialization together;
- model asymptotic and constant-factor effects on create/edit/reopen/range;
- design the smallest under-two-minute falsifying experiments.

Required report:

  `research/phase-4/g4-round-1/reconstruction/report.md`

### Subagent B — native materialization, cold/warm state, durability and OS path

Mission:

- trace logical bytes/verified seed -> native file/directory -> metadata ->
  data/metadata sync -> atomic publication -> directory sync -> reconciliation;
- distinguish first/full, empty destination, warm source, fresh process,
  controlled cold, protected seed, incremental, and fallback;
- determine whether an honest controlled-cold method is available on the
  current macOS/APFS environment; never infer cold from process restart;
- examine clone/reflink, copy, sparse/preallocation, descriptor-relative path
  safety, cross-volume fallback, stable-media limits, and VFS integration;
- identify storage/history/cache growth and concurrency consequences;
- propose portable and platform-specific alternatives with explicit fallbacks;
- design the smallest under-two-minute falsifying experiments.

Required report:

  `research/phase-4/g4-round-1/materialization/report.md`

### Subagent C — holistic disruptive core architecture

Mission:

- analyze CAS + CDC + COW + canonical object identity + mapping + filesystem
  storage + SQLite catalog + native seed/cache + VFS/projection as one system;
- ask whether a different authenticated extent/index/object layout could
  improve reconstruction, first materialization, warm reads, incremental
  materialization, edits, ranges, and reopen together;
- consider bold alternatives such as authenticated extent maps, different
  Merkle/proof granularity, profile-bound native representations, bounded
  content-addressed seed caches, reflink-first storage, append/value-log plus
  catalog, direct VFS streaming, history-independent mapping, chunk-profile
  changes, or a new versioned profile—but only after inspecting prior carrier,
  pack, compression, F3, mapping, and canonical-v2 evidence;
- identify migration, compatibility, garbage collection, crash ordering,
  cross-platform, concurrency, and security costs;
- separate a truly shared architectural improvement from a benchmark-specific
  trick;
- design small simulations or disposable probes that can falsify the highest
  upside claims in under two minutes each.

Required report:

  `research/phase-4/g4-round-1/core-architecture/report.md`

The lead agent owns benchmark/evidence methodology, cross-review, experiments,
and final synthesis. After initial reports, send each subagent at least one
cross-review question based on another lane's findings. Require corrections
before accepting the synthesis.

Each subagent may run its own small disposable side experiments inside its
assigned topic. Do not force every experiment through the lead agent, and do
not restrict subagents to prose-only research. Each subagent must:

- use a disjoint temporary/versioned namespace;
- record the hypothesis, commands, start/end wall, custody, raw result, and
  cleanup in its report and the shared experiment ledger;
- keep each complete experiment within the two-minute hard ceiling below;
- restore any experimental source bytes it owns before returning;
- coordinate timing-sensitive work through the benchmark lock so two
  performance probes never contaminate one another;
- avoid touching another subagent's files or active operands.

Static analysis, simulations, code reading, and internet research should run in
parallel. Only timing-sensitive performance rows are serialized.

BROAD EXTERNAL RESEARCH

After local evidence/code inspection, search broadly. Prefer primary sources:

- official SQLite/rusqlite documentation and SQLite source/design notes;
- Apple/XNU/APFS clonefile, copyfile, fcntl, fsync, rename, directory and cache
  semantics;
- filesystems/storage engine papers and official implementations;
- Git, Xet, Nix, content-addressed stores, Merkle DAG/proof designs, databases,
  reflink/clone caches, VFS/projection systems, and relevant research papers;
- official benchmarks only when methods and hardware are stated.

Use secondary sources only to discover primary material. Cite direct URLs near
claims. Do not copy a remote design without showing how its workload,
authority, durability, identity, and local filesystem assumptions map to
LayerFS.

BOLD ARCHITECTURE QUESTIONS

Investigate, without presupposing an answer:

1. Can the canonical root or mapping carry an authenticated ordered extent
   commitment that makes full closure/source fingerprint passes redundant?
2. Can one bounded pass authenticate canonical objects and emit logical/native
   bytes without duplicate BLOB opens, hashes, decodes, or output verification?
3. Should small canonical chunks remain the durable storage unit, or should a
   versioned large authenticated extent/segment representation coexist for
   sequential reads and native materialization?
4. Can a content-addressed verified native representation serve both hot reads
   and clone materialization with bounded eviction and without full logical
   duplication per revision?
5. Can SQLite remain the catalog while payloads use a different immutable
   local layout without recreating the previously rejected carrier's two-file
   crash/GC/authority problems?
6. Would a different mapping tree, chunk profile, or proof granularity improve
   read/materialization enough to justify migration while protecting already
   fast edits and ranges?
7. Can VFS read paths consume authenticated extents directly, avoiding an
   intermediate full reconstruction API while retaining platform-neutral
   fallback?
8. Can same-open seed authority become a bounded cross-process authority
   without replay, rollback, watcher-gap, or malicious same-UID weaknesses?
9. Which operations fundamentally remain Theta(S), and which can become
   O(changed bytes), O(selected range), O(extents), or O(namespace entries)?
10. Is the optimal answer one shared structure, or explicitly two
    representations—a canonical durable truth plus a bounded derived native
    acceleration cache?

These are prompts for investigation, not desired conclusions.

RESOURCE / CROSS-OPERATION BOUNDARIES

The goal is ultra-fast reconstruction and materialization without moving the
bottleneck into CPU, memory, persistent storage, history growth, or another
operation.

For every candidate, report:

- user/system CPU, instructions/cycles where supported, core count and span;
- application RSS, SQLite cache, exact Q, buffers, queues, and peak overlap;
- logical/apparent/allocated persistent bytes and transient bytes;
- steady-state growth across 10/100/1,000 revisions;
- cache/seed eviction, corruption recovery, garbage collection, and rebuild;
- source and destination bytes read/written, cloned, patched, or compared;
- SQLite queries/rows/BLOBs and filesystem syscalls;
- sync/publication boundaries and crash recovery;
- identity/schema/profile/migration and downgrade behavior;
- cross-platform fallback and VFS/API consequences.

Starting resource objectives to challenge and refine:

- no full-file application buffer;
- reconstruction/materialization process RSS target <=20 MiB;
- bounded queues/buffers only;
- no per-revision full native duplicate;
- no unbounded seed, decoded-object, or page cache;
- persistent metadata overhead target <=5% unless a much larger measured gain
  justifies an optional bounded cache;
- any optional native cache must have explicit capacity, eviction, corruption
  validation, and allocated-byte accounting;
- parallel designs must disclose CPU/core cost and deterministic error/
  cancellation behavior; do not assume more cores are free.

PROTECT EXISTING WINS

Research may recommend a future version/profile change, but it must quantify
impact on all protected operations:

- 100-MiB durable full create: 308.884052 ms;
- writer RSS: 12.48 MiB;
- same-open same-count and count-changing edits: approximately 5-7 ms at
  100 MiB;
- one-byte incremental materialization: 3.414166 ms at 100 MiB;
- 1-MiB authenticated range: 2.279 ms;
- reopen/head: approximately 2.088 ms;
- exact identities, typed errors, one transaction/COMMIT, FULL+DELETE,
  atomic visible-head and native publication, bounded Q, cleanup, and
  reconciliation.

Default protection rule:

- <=5% degradation is the normal maximum for create, edits, ranges, and
  retained G3 qualified paths;
- up to 10% may be recommended only if an independently measured read or first
  materialization improvement is at least 2x, resource use remains bounded,
  and the tradeoff is presented explicitly for user approval;
- any identity/format/schema/migration/durability change requires a new
  versioned profile and cannot be silently folded into G4;
- no correctness, authority, exact-error, crash, or terminal-Q degradation is
  acceptable.

Do not reject a bold research candidate merely because it changes format. Rank
it by benefit, migration and risk. But do reject or defer candidates whose
claimed speed comes from unbounded memory, extra full-file passes, per-revision
payload duplication, weaker authentication, cache-state relabeling, omitted
durability, or benchmark-only preparation outside the claimed work.

PERFORMANCE OBJECTIVES — HYPOTHESES, NOT EVIDENCE

Use these as starting objectives and refine them prospectively from evidence:

| 100-MiB path | Acceptance objective | Stretch objective |
|---|---:|---:|
| Warm complete authenticated reconstruction | <=333 ms / >=300 MiB/s | <=300 ms / >=333 MiB/s |
| Fresh-process reconstruction | <=400 ms / >=250 MiB/s | <=350 ms / >=286 MiB/s |
| Controlled-cold reconstruction | <=400 ms / >=250 MiB/s | <=333 ms / >=300 MiB/s |
| Trusted-seed full read | <=50 ms / >=2,000 MiB/s | <=35 ms / >=2,857 MiB/s |
| First full native materialization, warm source | <=400 ms / >=250 MiB/s | <=333 ms / >=300 MiB/s |
| First full native materialization, controlled cold | <=500 ms / >=200 MiB/s | <=400 ms / >=250 MiB/s |
| Protected-seed same-root clone materialization | <=10 ms | <=5 ms |
| One-byte incremental materialization | <=10 ms | <=5 ms |
| 1-MiB incremental materialization | <=20 ms | <=10 ms |

Do not invent throughput for clone/reflink operations that share extents rather
than transferring the logical length. Report wall, syscalls, logical/apparent/
allocated bytes, and supported physical observations separately.

FAST DISPOSABLE EXPERIMENTS

This is research-only, but short experiments are authorized.

Hard timing and task-latency rule:

- every individual side experiment, including its preflight, optional build,
  preparation, measured rows, analysis, and cleanup, must complete in <=120
  seconds total wall. The two-minute limit applies independently to each
  subagent's experiment; it is not a shared two-minute budget for all research;
- aim for <20 seconds of measured operation time;
- use 1-MiB or 10-MiB screens first;
- use at most one 100-MiB primary row per mechanism when the small screen shows
  a direct-counter signal;
- no 500-MiB work;
- no long full-workspace suite for disposable research candidates;
- no repeated campaign to rescue noise; change the hypothesis instead.

The overall Round-1 task must not be dragged out by benchmark construction.
Each subagent should normally choose at most two decisive side experiments. A
third is allowed only when the first result exposes a materially different,
prospectively stated hypothesis. Do not create broad matrices, five-pair timing
campaigns, endurance runs, toolchain rebuilds, or chains of nearly identical
microbenchmarks. Stop experimenting as soon as the candidate can be ranked.

Prefer existing frozen binaries, fixtures, scripts, APIs, and small harnesses.
Build at most once per distinct side candidate. If build plus probe cannot fit
inside 120 seconds, reduce the probe or return a modeled hypothesis for G4
rather than extending the experiment. A timeout is a rejection of that
experiment design; do not increase the ceiling.

Use one repository-level `BENCHMARK_LOCK` (or the existing equivalent) for
timing-sensitive probes. Lock acquisition is fail-fast: do useful read-only
research while another probe runs rather than waiting and blocking the task.
Non-timing simulations and static/codegen inspection may remain parallel.

Experiments may include:

- read-only tracing/counter probes;
- simulators and complexity/storage models;
- copied-source benchmark-private prototypes;
- isolated disposable binaries;
- codegen/profiling of exact hot paths;
- native filesystem probes in fresh temporary namespaces;
- source/cache-state qualification probes;
- direct SQLite/API microbenchmarks using frozen fixture copies.

Prefer `/tmp` or fresh versioned `target/phase4-g4-round1-*` namespaces. Reuse
existing immutable fixtures by hash; do not retain redundant 100-MiB copies.
Keep retained experiment evidence <=50 MiB and transient experiment storage
<=512 MiB unless a smaller predeclared filesystem-layout probe specifically
needs more and cleans it completely.

If an experiment changes tracked source, make the diff narrow and
benchmark-private, record pre/post hashes, and restore the exact committed
source with `apply_patch` before the research turn ends. Do not use destructive
Git commands. Final product source must equal checkpoint `5c342f0` unless only
research documents were added. No experiment result authorizes promotion.

Every experiment must preregister:

- hypothesis and one variable;
- exact input/source/binary custody;
- timer boundaries and direct counters;
- cache/destination state labels;
- CPU/RSS/Q/storage equations;
- protected-operation screen where relevant;
- retain/reject rule;
- unsupported observations and limitations.

An experiment is a side investigation, not an acceptance campaign. It may
falsify or support a mechanism and estimate a ceiling; it may not promote a
profile, rewrite the G3 baseline, or claim G4 acceptance.

Save an append-only experiment ledger at:

  `research/phase-4/g4-round-1/experiments/ledger.md`

Retain only compact raw/statistical artifacts needed to support decisions.

ANALYSIS FRAMEWORK

For every candidate, complete a row with:

| Field | Required content |
|---|---|
| Mechanism | Exact change and removed/overlapped work |
| Target paths | Reconstruction, materialization, range, create, edit, reopen, VFS |
| Complexity | Before/after work and span |
| Measured ceiling | Exact retained wall/counter budget it can affect |
| Predicted gain | Equation and assumptions |
| CPU | Extra/reduced passes, cycles, cores |
| Memory/Q | Peak simultaneous owned state |
| Storage | Logical/apparent/allocated steady-state and history scaling |
| Authority | What authenticates bytes, roots, extents, seeds, and publication |
| Durability | Sync/crash/publication/reconciliation changes |
| Identity/format | Compatibility, migration, version/profile impact |
| Cross-operation effect | Create/edit/range/reopen/G3 regressions or gains |
| Experiment | Fastest <=120-second falsification |
| Evidence | Local files, raw counters, primary-source citations |
| Disposition | Do now, G4 repair only, later profile, defer, or reject |

Separate:

- observed facts;
- derived values with equations;
- external-source facts;
- hypotheses;
- unavailable observations;
- speculative upper bounds.

Do not sum overlapping gross timing ceilings. Do not infer physical I/O from
logical bytes, allocation, pager bytes, RSS, Q, or wall time.

REQUIRED RESEARCH OUTPUT TREE

Create only organized research documents under:

  `research/phase-4/g4-round-1/`

Required layout:

```text
research/phase-4/g4-round-1/
├── README.md
├── reconstruction/
│   └── report.md
├── materialization/
│   └── report.md
├── core-architecture/
│   └── report.md
├── experiments/
│   └── ledger.md
├── benchmark-contract/
│   └── proposed-g4-contract.md
├── decision/
│   ├── candidate-matrix.md
│   └── final-synthesis.md
└── roadmap/
    └── post-g4-dependency-map.md
```

Each specialist owns only its topic report. The lead agent owns README,
experiment ledger, benchmark contract, candidate matrix, synthesis, and
dependency map. Agents are not alone in the repository and must not overwrite
another report.

FINAL SYNTHESIS REQUIREMENTS

The final synthesis must answer:

1. What should G4 measure first before any new optimization?
2. What is the honest controlled-cold procedure, or why is it unavailable?
3. What is the best format-preserving reconstruction improvement?
4. What is the best format-preserving first-materialization improvement?
5. What is the best disruptive architecture with cross-operation upside?
6. Which current full-byte/proof passes are necessary, redundant, or uncertain?
7. Can trusted-seed full reads reach the 2-3 GiB/s objective without excessive
   CPU, RSS, or persistent storage?
8. Can first/cold native materialization reach 200-300 MiB/s, and what exact
   component blocks it if not?
9. Which candidates preserve or improve full create, edits, ranges, partial
   materialization, and reconstruction together?
10. Which changes belong in G4 repair, a later version/profile milestone, G5,
    or post-Phase-4 integration?

Rank no more than:

- three `DO NOW / G4` candidates;
- three `LATER PROFILE / ARCHITECTURE` candidates;
- three rejected/deferred candidates.

For each `DO NOW` candidate, give one exact <=120-second experiment and a kill
rule. Avoid a research backlog with dozens of unranked ideas.

The proposed G4 contract must include:

- separate reconstruction and materialization scoreboards;
- 1/10/100-MiB row matrix;
- warm, fresh-process, controlled-cold, warm-or-unknown, empty-destination,
  protected-seed, and fallback classifications;
- direct work/resource/durability counters;
- exact performance objectives and protected-operation gates;
- total measured campaign budget <=120 seconds;
- independent analysis and append-only repair protocol;
- explicit boundary between G4 acceptance and production integration.

The post-G4 dependency map must re-evaluate, but not execute:

- G5 reopen authority;
- count-changing locality/mapping;
- concurrency/endurance/history;
- residual create/SQLite work;
- VFS/projection/application integration;
- final Phase-4 closure.

ROUND-1 CLOSE RULE

Round 1 is complete only when:

- all three subagent reports exist and were cross-reviewed;
- local code/evidence citations are exact;
- external technical claims cite primary sources;
- every experiment finished within 120 seconds and is in the ledger;
- experimental source is restored exactly;
- G3 sealed evidence and commit are unchanged;
- `git diff --check` passes for the research documents;
- the candidate matrix and synthesis select concrete next actions;
- the proposed G4 contract is ready for a separate preregistration/execution
  agent but does not itself authorize measurement.

Do not implement a production candidate. Do not run G4 acceptance. Do not
commit. Do not start G5.

FINAL RESPONSE

Return:

- Round-1 disposition;
- files created;
- subagents and cross-review conclusions;
- experiments, total wall, hashes, and results;
- top three G4 candidates and top disruptive architecture candidate;
- predicted reconstruction/materialization gains with equations;
- CPU/RSS/Q/storage and cross-operation tradeoffs;
- controlled-cold disposition;
- proposed G4 matrix and performance targets;
- G4 preregistration readiness;
- exact source/G3 custody proof;
- unresolved blockers and post-G4 dependency map.
```
