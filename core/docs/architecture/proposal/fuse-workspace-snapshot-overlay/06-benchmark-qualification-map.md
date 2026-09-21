# Benchmark evidence and Pair 1 qualification map

> **Status: Research; informative and not a product contract.**
> Read-only evidence review, 2026-09-21. Historical values below retain their
> original source, mode, gate and limitations. No benchmark, setup, build,
> verifier, Docker workload or product test was run for this review. The future
> qualification rows are proposed work, not a campaign registration or a result.

This paper connects the existing `benchmark/fs-bench-pro` coverage and the exact
v0.1.6 release record to the [Pair 1 implementation plan](04-implementation-and-verification.md).
It covers the complete family inventory relevant to the runtime, including
SDK edits, ordinary FUSE operations, metadata, history, storage and optional
proofs. A family is not dropped merely because the current core cannot execute
its operation or its declared size.

## 1. Sources, custody and precedence

| Evidence layer | Identity / scope | How it is used here |
| --- | --- | --- |
| Exact reference release | Peeled `v0.1.6` commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` | Reference source, family definitions and immutable release documents |
| #154 final measured product | Recorded source `823f556ca301e261b71d41b09ef619897b48e5f9`; product seal `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` | 33 regular plus 3 extended release rows; not relabelled as measurements made at the tag commit |
| #154 harness | `8a6d76dd2f2e639fb356ac74551a4aae2634df1eaf2b3603ebd458cafd90dff5` | Recorded on the final matrix and inspected raw receipts |
| #154 image | `sha256:2bd697b90894749592cf62e6470a0d0a9cf54a0416cbf3cd903dbbcc3d9954df` | Recorded runtime artifact; a tag string alone is not an image identity |
| Earlier #151/#152 comparisons | Multiple explicitly recorded candidate revisions versus v0.1.5; banked B1/B2/B3 separately identified | Historical mechanism, resource failures and owner dispositions, not one final-tag measurement set |
| Replacement foundation | `152b9c3a2e8ec2536a1d63601b681e1f7ef34455` | Current shared-operation limits and Pair 2/3 integration prerequisites |
| Current operating rules | User/repository owner directions, including per-worktree isolation, one sample and one construction producer | Govern any future run, even where an older guide or receipt has a wider allowance |

The v0.1.6 release [applicability record][release-verification] states that the
measured product and harness seals reproduce against the tag, while the complete
source seal differs because of the version bump and explicitly identified
collector declarations. The measured source was marked dirty for foreign
uncommitted research material outside the product/source seal scope described
there. Preserve that record; do not silently report a clean tag build as the
original measured artifact. The source-only release used existing campaign
evidence rather than rerunning it. [Machine-readable release evidence][release-evidence]

This review read the family/registry/runner/verifier sources and existing JSON
receipts without invoking `infra-list`, setup, a self-check or a verifier.
The checked-out family modules, `workload/workspace_registry.rs` and
`src/infra.rs` have no diff from the exact release tag. Current runner/isolation
changes are read as current mechanics, not substituted into old receipt identity.

### 1.1 Current rules do not inherit old execution exceptions

Read the [measurement contract](../../../../../docs/general/benchmark_rules.md),
[benchmark rules](../../../../../benchmark/AGENTS.md),
[runner guide](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[release policy](../../../../../docs/general/release-policy.md) and
[documentation policy](../../../../../docs/general/documentation-policy.md).
The following differences are material:

- Current owner rules require one sample per case/arm, fresh output, independent
  verification, fixed workers and full selected membership. Older n3/median
  examples do not authorize repetitions now.
- Current measurement isolation is [per worktree](../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md).
  Targets/outputs belong to that worktree; concurrent work on the same host is
  recorded as interference. Old global-lock examples are historical, and state
  isolation does not establish a quiet CPU, disk or page cache.
- Current ordinary complete-command budgets are at most 15 seconds, with the
  declared small exception mechanism up to 25 seconds; verification is normally
  under 15 seconds within its 60-second hard budget. Old 60/120/300/600-second
  execution allowances are not permission to widen a new selection.
- `tools/preflight.sh` is permanently retired and there is no CI or replacement
  aggregate gate. Its PASS in a release-preparation record is history, not a
  command to run now. Current implementation uses the owning workspace's locked
  Rust 1.85.1 checks and appropriate external proofs.
- The permanent benchmark hosting rule keeps the coordinator, C1 construction,
  Store/SQLite and default physical spool on the macOS host. The
  `v016-local-snapshot-experiment-v1` exception explicitly admitted execution-side
  mutable state/backing for that treatment. It does **not** automatically admit
  the new Pair 1 daemon backing topology. A v0.1.7 campaign needs its own explicit
  operation/topology/acknowledgement/cache declaration. A product deployment
  diagram is not permission for undeclared data-sharing mounts or Docker-owned
  SQLite/coordinators.

No future claim in this paper borrows a historical waiver or changes an old
receipt's eligibility. Actual worker configuration must be pinned on both new
arms. The exact v0.1.6 source still has an available-parallelism construction
default and the small-content worker constant; a process-wide construction gate
is not proof of a single internal producer. Release/experiment prose records the
single-worker collection regime, but default source behavior and the actual
sample configuration are distinct. Do not infer either a new one-worker proof
or historical invalidity merely from the other. [Reference construction][worker-source]
[Reference small-content construction][small-workers]

## 2. What the harness actually selects

```text
 family perf.sh                         family verify.sh
      |                                       |
      v                                       v
 shared/runner.py                     verify-selected.py
      |                                       |
 exact source/image/input/case          independent identity-bound proof
      |                                       |
 host public SDK/coordinator  <----->  Linux daemon/FUSE/workload
      |
 host canonical construction + Store

 retained perf.jsonl and verification.json
      |
 #154 committed matrices -> release CSVs / closeout
```

The shell scripts are thin launchers. The public call route and fixture/oracle
owners are in Rust family/workload modules and the selected host adapter.
`shared/runner.py` maps workspace rows to `pure_call_sum_ns`, SDK rows to
`edit_commit_ns`, namespace rows to `layerstack_init_ns`, and Store-footprint
rows to `product_call_sum_ns`. Complete wrapper wall, selected child wall,
preparation, cleanup and resource windows are separate fields. The word
`performance` on a wrapper does not make all these durations interchangeable.
[Selection adapter][infra] [Workspace registry][registry] [Runner][runner]

At the tag, the 16 Workspace-family definitions declare **169 IDs**. That count
includes the six new verify-only historical-access IDs and the three extended
IDs; two extended IDs are also verify-only. The registry's old phrase "timed
IDs" must not be used to claim 169 performance samples. The other main
definitions are **56 SDK edit IDs**, **4 namespace Init IDs** and **6 Store
footprint IDs**: 235 primary defined selections by source arithmetic, not a
frozen campaign count. There are separately **29 proof IDs**: 28 reliability
definitions and one CDC boundary proof. One reliability definition is the
optional sustained 600-second proof. Do not count extensions or inherited
duplicates twice.

The host-family allowlist has 22 names. Repository-history profiles, the
exploratory small-file-delta wrapper and five old capped-edit IDs have separate
status below. Four structured-text namespace variants are opt-in source variants,
not additions to the four mandatory pseudorandom Init cases. Static inventory
here is not an execution or cardinality qualification of a new campaign.

### 2.1 Complete main-family coverage map

Counts below are definitions at the exact tag, with performance/proof roles
stated separately. `Prerequisite` means the family cannot yet run against Pair 1
until the named real operation exists. `Separate owner` means a C1/C2/C5 or
import claim is not itself evidence for FUSE; it does not remove that family from
any future whole-product campaign that declares it mandatory.

| Family | Defined membership / surface | Pair 1 requirement and current disposition |
| --- | --- | --- |
| `payload_create_read` | 8: create and random-read at 1/10/100/500 tiers | Reads need real lazy FUSE range/EOF behavior; creation needs new-inode/metadata operations and disk backing. Prerequisite for writable rows; large reads must be measured through actual cache behavior |
| `tiny_file_churn` | 20: create/stat/unlink/bulk-create/bulk-delete at four tiers | Essential npm analogue: many names plus mixed large/tiny payload. Requires bounded runtime metadata/FD/index state, create/unlink and complete shared input beyond 128 names; not a payload-buffer-only test |
| `directory_construction_traversal` | 12: construction, metadata scan and content scan at four tiers | Read subsets need paged List/Stat and bounded cookies; construction needs writable namespace/bulk input. Keep metadata-only traversal separate from content-reading work |
| `git_tool_workflow` | 4 ordinary Git/tool workflows | Requires the real tool's create/truncate/write/metadata/rename/flush semantics and full Git oracle. SDK edits cannot stand in for Git's POSIX operations; fsync/mmap/locking differences must be dispositioned |
| `namespace_mutation` | 4 subtree-relocate/delete selections | Requires pre-visibility type/emptiness/cycle/alias validation and open-handle lifetime. The current 4,096-work bound can block large namespaces; immutable mount roots do not waive descendant rename |
| `workspace_change_locality` | 16: clean Commit, dense rewrite, distributed SDK edit and fixed move at four tiers | Covers no-op work, large disk writes, SDK visibility and namespace locality. Dense-rewrite kernel-cache failure must remain visible. Different subroutes must not be pooled as one edit API |
| `mixed_load_bearing` | 10: four earlier agent-episode rows, four regular v0.1.6 M1 rows and two exhaustive verify-only extensions | Full combined POSIX helpers, SDK edits, metadata and repeated Commit. Requires both callable surfaces, full namespace input and exact G/G+1 reconciliation; the exhaustive rows have no performance timer |
| `dedup_cross_file` | 10 initialization/import cases: one anchor plus unique/identical/mixed at remaining tiers | Primarily C1/C2 import/storage reuse, not a mounted edit. A replacement public import/init path is a prerequisite; native import must not be replaced with deferred FUSE creation and called identical |
| `dedup_cdc_locality` | 20 initialization/variant cases plus one separate boundary proof | Primarily canonical construction/dedup with authenticated readback. The fixture's overwrite/insert/delete names do not make these SDK or FUSE mutations. Separate C1/C2 owner and real import prerequisites |
| `dedup_workspace_reuse` | 14: exact/local/unique additions over four tiers plus two explicit fixed-base controls | Same Workspace and retained base with new files; needs disk-backed additions, new-inode metadata, clean-file/root reuse and complete final bindings. Compact and fixed-base controls are different cases |
| `dedup_branch_history` | 26: five inherited history patterns at four depths plus six K10/K100 v0.1.6 profiles | Repeated public edits/Commits, retained roots and ancestry; added namespace-inode profile mixes POSIX and SDK. Requires stable generations, history integration and exact callback/SDK route counts |
| `file_size_transition` | 7: five fixed-size controls, four-step size roundtrip, five-step alias roundtrip | Six plans are SDK-shaped; alias roundtrip uses POSIX names/replacement. Requires shared inode identity, truncation, whole/chunk boundary behavior and independent retained-state readback; never pool SDK-only and alias/POSIX ratios |
| `multi_workspace_development` | 5: four two-Workspace rows and the extended four-Workspace control | Requires actual multi-Workspace consumer topology and recorded overlap. New one-frozen-submission-per-consumer policy may refuse overlap that the reference admits; decision/proof needed, not implicit extra concurrency |
| `branch_development` | 6: four M1 trunk/child rows plus two compact graph controls | Requires Fork/GetBranch, repeated Commit and branch/ancestry identity. K is local child depth, not necessarily total Commits. A branch family name is not evidence of simultaneous C2 saves |
| `historical_access` | 6 new v0.1.6 **verify-only** retained-state selections | Requires read-only historical root mounting, alias/absence checks and exact producer identity. No deferred-read latency result exists in these six rows; performance is N/A |
| `local_snapshot` | 1 lifecycle: create 25,000 one-byte files, change 256 selected files, restore them, three distinct Commits | Direct metadata/cardinality and generation stress despite tiny payload bytes. Needs live creation, scalable backed dirty/completion indexes and input beyond current 128-record limits. Three Commits are not three samples |
| `edit_length_preserving` | 12 SDK cases: head/middle/tail 4 KiB overwrite across four sizes | Requires public SDK/control mutation into Workspace, no edit-caused FUSE WRITE and separate mounted visibility proof. Direct service EditFile alone is not equivalent |
| `edit_length_changing` | 32 SDK cases: eight operations across four sizes; five active large-result cases have explicit capped-v2 identities | Same SDK/control requirement plus current-result edits, append/truncate/zero semantics. Preserve exact already-versioned fixture algebra; do not newly shrink a selection |
| `edit_canonical_chunk_count` | 12 SDK cases: preserve/increase/decrease with fixed 64 KiB replacement across four sizes | Requires exact canonical oracle, unchanged file length, SDK route and measured mechanism counters; a POSIX rewrite cannot satisfy the same claim |
| `init_namespace` | 4 pseudorandom tiers: 100/1,000/10,000/100,000 files; 4 text variants opt-in only | Native public initialization/import plus independent mounted readback. Current 128-entry bootstrap is insufficient at larger tiers. Multi-worker exception and 2.7-second cold 100k target stay separate from Commit |
| `store_footprint` | 6 controls: unique/metadata-cardinality/large-object at high and low sizes | C1/C2/import/storage-accounting owner. There is no FUSE-write latency to invent in an Init/footprint-only metric; full declared import and authentication still need the real replacement path |
| `workspace_reliability` | 28 proof definitions, 27 verification-supported routine cases and one optional 600-second sustained case; no performance distribution | Required behavioral review for lifetime, quota/failure, links, attrs, parallel operations and publication. Old retry/instrumentation expectations must be mapped explicitly to current no-replay/source-purity rules, not copied or silently omitted |

Count provenance is [the tag registry][registry], [the selection adapter][infra]
and the [family definitions][families]. Every main family above has a mapped
owner or prerequisite. A later case manifest must preserve its exact IDs and
profile versions, not turn this table into a new family generator.

### 2.2 Separate optional, replaced and exploratory surfaces

| Surface | Recorded membership / status | Disposition |
| --- | --- | --- |
| Inherited `historical_access` v2 | 11 performance cases plus their 11 proofs, using a separately sealed full157 schema-9 Store | #152 recorded NOT_RUN because that exact Store was unavailable. These are not the six new v0.1.6 IDs; do not rebuild a different Store and reuse their identity |
| `repository_history` | 3 optional profiles: stride-1/3/10, retaining 157/53/17 states including the final checkpoint | Long public importer/history work, explicitly selected; otherwise NOT_RUN_OPTIONAL. Core history construction alone is not mounted-history or matched Git evidence |
| `edit_length_changing_capped` v1 | 5 inherited duplicate replacements, outside the active host-family selection | Superseded by the active capped-v2 rows in `edit_length_changing`; retain historical NOT_RUN_OPTIONAL rather than count them as new missing active SDK operations |
| `small_file_delta_smoke` | 1 exploratory case, ten files and 30 ordinary Exec/FUSE saves plus initial retained state | Separate wrapper/storage-smoke route, no numerical gate. Its historical 600-second watchdog does not authorize a new long run or make it an SDK edit |
| Sustained reliability | One 600-second definition | Optional/unrun in the cited release evidence. Routine parallel/read-write and repeated-publication smoke is not that endurance proof |
| Structured-text namespace variants | 4 explicitly selected text-mode variants | Different input identity from the mandatory pseudorandom set; no substitution into its cold target |

Sources: [old historical-access wrapper][old-access],
[repository-history registry][repository-history],
[small-file smoke][small-smoke], and [release acceptance][acceptance].

## 3. What the release record does and does not establish

### 3.1 The final #154 set

The release [closeout][closeout], [performance CSV][perf-csv],
[verification CSV][verify-csv] and two committed [regular][matrix] /
[extended][matrix-ext] matrices contain:

| Item | Recorded result |
| --- | ---: |
| Total selected rows | 36: 33 regular plus 3 extended |
| Actual performance receipts | 28 |
| Performance gates | 25 PASS, 3 EXCEPTION |
| Verify-only rows | 8: six historical-access and two exhaustive mixed |
| Independent verification receipts | 36, status PASS |
| Verification gates | 33 PASS, 3 EXCEPTION |
| Cleanup outcomes | 64 PASS: 28 performance plus 36 verification |
| Reused verification identities | 0 |
| Mode-level results over the 15-second family target | 8 |

The 36 rows divide into 7 file-size transition, 6 branch development, 6
dedup/history, 6 mixed (including two verify-only), 5 multi-Workspace (including
the four-Workspace extension), and 6 historical-access rows. This set is not the
entire historical harness inventory or the earlier #152 comparison.

Each row used seed 1 and one sample per mode. A row containing 100 or 400
different Commit operations is still one workload sample. The release recorded
a busy 14-CPU host with load-1m 6.73-8.83. It does not provide a quiet-host result
or a universal latency distribution.

Historical allowances were 60 seconds for regular performance invocation,
25 seconds for regular verification with one exact 30-second exception, and
60/120/300-second extended watchdogs. A raw operation status PASS under an
execution allowance and a closeout gate EXCEPTION above the 15-second family
target are different fields. Preserve both. Selected summaries also retain
their recorded `admission_eligible=false`; the owner-authorized release reuse
does not turn them into new v0.1.7 admission receipts.

All 28 performance `perf.jsonl` and 36 verification `verification.json` files
linked from the two tag matrices were present when read for this review. Their
observed product and harness identities each formed one matching set shown in
§1. This is a file/identity inspection, not an independent rerun of their oracles,
timers, binary custody or environment.

### 3.2 Representative measured rows, with original scope

These examples select challenges for the plan. The complete tables remain in
the authoritative release artifacts; they are not reproduced as a new campaign.

| Original source / case | Exact recorded value | Original status and inference limit |
| --- | --- | --- |
| #154 `v016-history-boundary-cycle-k10-v1`, raw `phase=create` | 11,142,792 ns | Case status/gate PASS. This is the mount/create phase of an already prepared history workload, not payload ingestion or first-read cost |
| #154 `v016-history-boundary-cycle-k100-v1`, raw `phase=create` | 9,139,167 ns | Case status/gate PASS. Two similar millisecond mount measurements do not prove free subsequent reads or a universal scaling law |
| #154 `v016-branch-mixed-500mb-30000-k100-v1` | Driver complete wall 16.492 s; selected child `command_wall_ns=12,843,518,667`; `pure_call_sum_ns=23,079,619,758`; 210 Commit records | Performance gate EXCEPTION; verifier driver 23.100 s with gate EXCEPTION. These are different timer scopes, not alternate estimates of one latency |
| Same branch row, per-Commit workload records | Sum 3,934,840,341 ns; median 16,811,958 ns; 210 different Commit operations | Within-workload statistics, not 210 independent benchmark samples and not the installation's complete cost |
| #154 `v016-workspace-four-100mb-5000-k100-v1` | Driver performance 7.573 s; verifier 9.882 s; 400 Commit records; workload-reported overlap 54,533,292 ns over 314 rounds | PASS under its declared extended policy. Demonstrates that recorded multi-Workspace schedule only; not same-Workspace G/D1 proof or #210's C2 reverse-failure schedule |
| #154 `v016-mixed-development-500mb-30000-k100-v1` | Driver performance 18.312 s; verifier 22.121 s | Both closeout gates EXCEPTION above the family target, not erased by successful completion |
| #154 `v016-mixed-exhaustive-500mb-30000-k100-v1` | Verifier driver 59.326 s | Performance N/A; verification gate PASS under the historical 300-second watchdog. Not a read-throughput or fast-validation claim |
| #154 six `historical_access` rows | Performance N/A in all six; verifier driver walls 1.593-1.964 s | Proof of selected retained-state reads under their coverage, not deferred-read latency measurements |

The two create phases and the two larger raw performance examples were read
directly from [K10][raw-k10], [K100][raw-k100], [branch L500 K100][raw-branch]
and [four Workspaces][raw-four]. The local raw files are append-only evidence,
not tracked source. Their read-time hashes are recorded in §7 for exact location.
The derived driver wall/gate values come from the committed matrix/CSV, not from
renaming an inner raw field.

Nominal case names do not give exact fixture bytes: the inspected L500 branch
row records **499,934,464 bytes / 29,984 regular-or-hardlink paths**; the L100
four-Workspace row records **99,934,464 bytes / 4,984 such paths**. Parent dirs and
other kinds have their own counts. Preserve the fixture/input identity instead
of rounding a name into a new oracle.

The branch L500 K100 raw row reports 4,032 regular creates, 4,536 regular unlinks,
336 hard-link creates, 84 populated-directory moves, 672 explicit mtime calls and
84 SDK edit calls. That concrete operation mix cannot be exercised by only
ReadFile/EditFile or by a 128-record existing-inode stage interface. Its
`concurrency_claim` is explicitly `not-a-concurrent-topology` even though it has
an overlap observation; do not promote that field into a stronger concurrency
claim. Summed public-call durations can differ from elapsed command wall; use
their declared event/aggregation scopes. [M1 timer/operation source][m1-source]

### 3.3 Earlier comparisons retain failures and waivers

The [#152 final report][report152] records 196 freshly collected performance
cells, 189 PASS, six material FAIL rows and one owner-waived cold-Init target
miss. Three B1/B2/B3 controls were reused by citation, not freshly sampled in
that count. Eleven old historical-access performance rows plus eleven proofs
were NOT_RUN for the missing sealed Store; optional repository-history and
sustained selections remained unrun. Candidate identities changed during fixes
and remain attached to their groups. The table's banked rows must not be counted
again as fresh collections.

| Retained observation | Source identity and original disposition | Requirement for the new plan |
| --- | --- | --- |
| Cold `namespace-100000`: candidate 4.986 s versus v0.1.5 4.398 s | #152 G1 candidate `8b5e0955e`, product `31a42c95197a21c5…`; target miss owner-waived, independent verification PASS | Preserve the 2.7-second cold Init target and its multi-worker exception; a v0.1.6 waiver is not a future pass |
| `dedup-cdc-scattered-500`: 2.290 s versus 1.426 s, 1.61x | #152 G6 candidate `29835f44d`, product `dc2b3a14f45a4eb7…`; material FAIL, later owner-accepted as recorded with the single-worker diagnosis | Do not add construction workers to hide the cost; compare matched producer settings and keep failure status |
| B3 25k lifecycle: workflow 10,381,383,418 ns; Commit steps 521,839,917 / 76,904,333 / 67,380,500 ns; temporary backing 25,000 B | Ledger L20, `9104bcb4f`, product `31a42c95…`, `perf-CAND-25k-r5`; accepted with the recorded time/host-state qualifications | Test metadata/cardinality and exact repeated-generation behavior; small payload does not imply small metadata or free capture |
| Dense-rewrite 500 MiB: candidate cgroup file peak 529,182,720 B, control 525,750,272 B; physical spool 524,288,000 B | #152 resource diagnostic, source group identities retained in its report; named cache-amplification guardrail FAIL, not excused by control parity | Separate ordinary existing-file writable-open/kernel-cache behavior from create-path spool buffers and SDK edits |
| B2's bounded-window transfer versus cache-served reading | Ledger L18/L20 records the cache-credit defect, its repair and the remaining unavailable sandbox-process memory gate | The published bounded-residency observation is not a hard generic RSS cap; Commit must pay required backing reads under the declared cache state |

These earlier numerical observations are carried from their named published
report/ledger, not claimed revalidated from all their original untracked run
directories in this review. [Exact ledger][ledger151] gives their original paths,
full identities and arithmetic. Do not combine them with the single final #154
product identity.

Two particularly important qualifications in #152 remain useful:

1. Its 56 SDK-edit cells' small file-cache observations do not qualify ordinary
   POSIX dense rewrites. On the latter route the report names payload-scaled
   kernel file cache as a guardrail failure. Removing daemon copy-up alone
   therefore cannot establish bounded container memory.
2. Six reliability fault proofs were later repaired to 27/27 routine PASS on
   product `970964e9…`; the repair includes legacy test-instrumentation hooks
   and re-driving a failed generation. Current core prohibits those product
   test hooks and automatic mutation replay. Preserve the old evidence, then
   define equivalent required failure visibility and the explicit current
   refusal/retention semantics; do not copy the old retry behavior or call it N/A
   without disposition. [Reliability repair][reliability-fix]

## 4. Gaps the benchmark evidence exposes

### 4.1 Lazy mount, cached read and physical work are separate

The millisecond create phases above attach an existing immutable namespace; they
do not pay to ingest and construct every file. C1/C2 construction timings from
a mount-free core cannot be placed opposite those mount values as if they
measured the same operation.

The exact v0.1.6 tag also has a **process-shared 32 MiB immutable range cache**.
Its 8 KiB immutable-file acquisition/prefetch cutoff does not imply that every
larger content read is uncached. The earlier mechanism reading was from another
source state and is not the exact-release cache authority. Measure actual
userspace range-cache hit/miss/eviction, kernel callbacks and bridge operations;
do not assume a repeated executable above 8 KiB necessarily misses or that an
eager candidate must win. [Reference range cache][range-cache]

The six new historical-access cases are explicitly verify-only in both the
family and host execution source. They mount one selected retained state and
check its declared bytes/alias behavior; they create no Commits. Verification
wall is not deferred-read latency. The older eleven-case cold/warm performance
definition was a different sealed fixture and was not run in #152. These facts
leave a real #207 measured-read gap; they do not establish zero read cost.
[Access family][access-family] [Access implementation][access-source]

### 4.2 SDK/control and POSIX workflow both need real routes

The SDK edit invariant is public
`Client::edit_workspace_file_range`/`edit_workspace_file_ranges`. Its timer starts
at that call and its receipts require observed topology/counter evidence, with
zero edit-caused FUSE WRITE. The edit must reach the live Workspace and any
necessary kernel invalidation; calling service EditFile and obtaining an
unattached immutable root does not satisfy it. Independent mounted visibility,
byte/root and reconnect checks belong to the separate verifier, with its actual
coverage stated. The exact-tag fast SDK verifier checks canonical roots,
unchanged extents and a bounded boundary range through the existing mount, then
reopens the Store/client; it explicitly emits `fresh_fuse_reopen=false` and
`full_file_bytes_verified=false`. Do not promote that into a fresh-remount or
whole-file byte proof for the new candidate. [SDK proof][sdk-proof]

Conversely, npm/Git/ordinary namespace families must perform their actual
POSIX/FUSE mutations. Rewriting temp-file/rename saves into SDK edits changes
the operation. Some v0.1.6 mixed families intentionally combine both surfaces;
their declared call algebra, not the family name alone, decides the mapping.
The required callable host SDK/control-to-daemon boundary is a prerequisite in
[04](04-implementation-and-verification.md), not supplied by a daemon-local Rust
API or by the host-facing C5 service endpoint.

### 4.3 Payload scale and metadata scale are independent

The existing tiny-bulk 500 profile contains 5,000 affected files and 500 MiB of
content: three 100 MiB files, 4,000 4 KiB files and 997 medium files. The 25k
local-snapshot case has only one byte per file, then edits 256 chosen names and
restores them. Both violate the current shared existing-inode 128-record
envelope for their full semantics, for different reasons. [Tiny profile][tiny-profile]
[25k recipe][snapshot-family]

A bounded payload window cannot replace backed namespace/inode/piece/dirty-index
and completion-map state. Disk backing requires quota, immutable G/D1 extents,
bounded FD/index/page residency and physical-release evidence. A failed append's
reserved/dead range is not reclaimed merely because no name references it;
the legacy repair report explicitly records such a retained-allocation difference.
The new 8 MiB consumer working-allocation proposal is neither the reference's
spool quota nor an observed process/cgroup maximum.

The replacement interface still lacks general live new-inode/metadata/symlink
attachment and a complete larger-generation input path. Current 128 names /
128 inodes / 128 directory records, 32 KiB metadata and 8 MiB edit-replacement
limits remain independent. Do not make the old workloads fit by committing
every 128 names, shrinking the graph, widening a timeout or silently using an
unregistered import route. [Replacement request contract][core-request]

### 4.4 Several different overlap questions

| Question | Relevant existing evidence | What remains required |
| --- | --- | --- |
| Does one Workspace accept live successor work while G is genuinely saving? | Reference non-pausing source and carried snapshot/reliability evidence | Pair 1 S-11: causal local write/read progress before the real service save finishes; held terminal alone is insufficient |
| Can distinct Workspaces have operations overlap? | Two/four-Workspace M1 schedules with recorded overlap | Exact consumer mapping and new single-frozen-per-consumer policy must be reconciled; separate mounts do not prove it |
| Can separate C2 saves overlap with reverse success/failure and stage preservation? | Pair 2 source/review is separate | #210 H04/H06/H08 schedules remain open; M1 workload overlap is not that proof |
| Does caller state remain correct after unknown publication or a stale exact token? | Old reliability and retained-history behavior expose the need | Current exact StageObservation/G association, no replay and no root-equality inference must be verified through the new real route |

In the reference, capture can occur before the process-shared construction gate;
serializing builders does not imply only one retained snapshot across Workspaces.
The new one-frozen-generation-per-consumer proposal is an explicit isolation/
admission restriction. Preserve it until a separate owner decision; report an
unmatched multi-Workspace case rather than adding hidden snapshot slots or workers.

### 4.5 Memory, authentication and unsupported counters

Raw #154 samples declare host-process CPU/RSS/I/O separately from container
lifetime peak and command CPU. The inspected examples have `cache_contract=null`;
their closed byte-copy setup and a PASS status cannot establish a new cold-source
claim or a phase-reset cgroup peak. Do not retroactively relabel their original
results; constrain the new claim to what they actually recorded.

Current families require authentication, exact logical bytes, metadata/identity,
mode separation, cleanup and custody, not only latency. Where an old route's
host spool-write counter is dead, zero is not proof that sandbox backing did no
work. Where a verifier is sampled, say what it covered. All 36 final verifications
declare omission of exhaustive Phase 1 replay. The two exhaustive mixed rows add
their own stated deep oracle; their names do not erase other recorded omissions.
FUSE, SDK and storage authentication paths must remain enabled on both future arms.

## 5. Coverage classification for the implementation plan

Use these labels per **case and metric**, not to hide whole families:

| Label | Correct use |
| --- | --- |
| Prerequisite / NOT_RUN | The operation, control route, backing, metadata/input capacity, platform or fixture is missing; retain the full intended selection |
| Separate-owner qualification | Import/C1/C2/C5 behavior has its own authentic operation and evidence; it cannot by itself prove a mounted Workspace, but remains required if selected by the whole-product campaign |
| Truly not applicable | A schema-declared metric has no operation: performance on the six verify-only access rows, FUSE-write latency for a native Init-only timer, or no payload upload for a metadata-only command |
| Optional not selected | Explicit long repository-history, sustained or exploratory profile not chosen; do not present routine smoke as completion |
| Superseded historical definition | The five capped-v1 duplicates or a replaced fixture profile, retained with its old status; not silently counted as active or mixed with replacement IDs |
| Historical FAIL / waiver / EXCEPTION | Preserve the numeric result and exact owner scope. Neither owner acceptance nor control parity converts the raw failed condition into PASS |

The proposed delivery dependency order is:

```text
 public SDK/control contract + authenticated daemon route
                     |
 readable historical/explicit-root mount
                     |
 disk payload + bounded metadata/index + required live metadata operations
                     |
 exact G/D1 capture, bounded lowering, one own-result tree/Commit path
                     |
 new-inode/symlink/namespace + larger complete-generation input
                     |
 full tiny/bulk/25k/Git/npm/repeated/multi-Workspace workload membership
                     |
 separate exact-source performance + independent verification declaration
```

A missing prerequisite can move earlier in its own operation round; this graph
does not authorize bundling the whole implementation. The accepted input set,
chosen npm graph and benchmark case registry must be explicit before claiming
that the selected workload is covered. A README table alone is not admission.

## 6. Future matched collection, not permission to measure now

Before implementing a new/changed family, commit its operation-specific contract
and link its issue. Use existing family/runner/verifier ownership; do not create
an aggregate preflight or a second benchmark framework. Declare:

1. **Exact arms and treatment.** Reference tag/product build, candidate source,
   lockfile/compilation/image/worker identities, common harness/oracle, allowed
   topology/route differences and the precise claim they permit. Missing SDK
   compatibility or changed acknowledgement cannot be hidden in a speed ratio.
2. **Full membership.** Every case, size/count/depth, input digest, operation
   sequence, script requirement and expected call/Commit/root count. Ordinary
   POSIX installation and SDK-edit families remain separate.
3. **Timing.** Mount, first required read, declared execution sequence, local
   write/backing work, source transfer, file/tree construction, stage/Commit,
   publication and cleanup retain their own boundaries. Count sum-of-calls and
   wall independently; no work moves outside its rightful measured phase.
4. **Cache and resource state.** Equal declared cold/warm treatment where
   applicable; account reference range cache, kernel cache, backing pages,
   daemon working allocations, service memory, host and container scopes.
   Unsupported cold acquisition is INELIGIBLE, not an invented label.
5. **Reuse and samples.** Prepared immutable inputs acquired once; independent
   writable copies via the qualified clone contract after initialization; fresh
   outputs; one sample per case/arm. No pre-touching, mutated sample reuse,
   selected warm reruns or quietest-run selection.
6. **Verification and limitations.** Bind separate proof to exact sample
   identities; preserve oracle coverage/omissions, final bytes/roots, expected
   failures and cleanup. A qualifying reused proof must be explicitly named.
   Missing counters, wrong route or unavailable mandatory memory proof cannot
   be converted to zero or PASS.
7. **Budgets and locks.** Current per-worktree locks and owned targets, declared
   interference, current 15/25/60-second policy and single construction producer;
   namespace Init keeps its exception. Preserve every failure and whole unrun
   selection when it cannot fit; never borrow the old extended watchdogs.

An executable above 8 KiB remains a useful possible read/exec case, **not an
uncached-by-construction case**. Compare the exact same executable, interpreter/
library placement and predefined repetitions on both real mounts; count actual
cache hits/misses/evictions, callbacks, logical operations and required reads.
Do not derive a transport-latency or prefetch threshold from a verify-only row.
Any cache-policy experiment is a separate treatment after the first matched
mount/read qualification, with an explicit owner-approved budget.

## 7. Read-only evidence inventory and limits of this review

All links in this table point to existing evidence; none was regenerated.
Hashes identify the raw files read during the review, not a new evidence seal
or a reinterpretation of their historical admission status.

| Existing local receipt | SHA-256 observed on read |
| --- | --- |
| [Boundary-cycle K10 performance][raw-k10] | `2ca72d3401623f92d1bd73c3393e8e94274533104b9fe354de0760d9b1d2a567` |
| [Boundary-cycle K100 performance][raw-k100] | `735b8c38dec4e1b61f99bd5a3c7a6d43b3a8d1e63891a89864b243114684eb45` |
| [Branch L500 K100 performance][raw-branch] | `91e2f8ed78e05a45fc64cfe75f7b8ad2c4871d6bc0860c79a9096e8225923d44` |
| [Four-Workspace performance][raw-four] | `c20b57ea029d499d7eb9687423c8c3ac696f230bb73b574e36287e11cb2d803d` |
| [Boundary-after historical verification][raw-access] | `17a4aa565870f97a8c1e9cc8e4da3ea2d03363aa183ec65d7b51b36907098e1c` |

Raw receipt links are local untracked artifacts and may not exist in another
clone. The committed matrix, CSV and release record remain the portable index;
if raw evidence is unavailable to a later reviewer, report that availability
gap rather than reconstructing or promoting a substitute file.

Review performed: policy/source reads, exact-tag peel, static family arithmetic,
matrix/CSV/receipt parsing, existence and identity checks for the 64 final raw
files, selected raw hashes, and document consistency/link checks. Not performed:
compilation, image launch, fixture preparation, benchmark collection, verifier
execution, oracle recomputation, cache invalidation, worker/runtime qualification,
binary resealing, any new latency/memory measurement, commit or issue update.

[release-verification]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/verification.md
[release-evidence]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/release-evidence.json
[acceptance]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/acceptance.md
[closeout]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/benchmark-closeout.md
[perf-csv]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/benchmark-performance.csv
[verify-csv]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/release-notes/0.1.6/benchmark-verification.csv
[matrix]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json
[matrix-ext]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-extended-matrix.json
[registry]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/workload/workspace_registry.rs
[families]: https://github.com/Ephemeral-AI-Lab/layerfs/tree/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/families
[infra]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/src/infra.rs
[runner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/shared/runner.py
[sdk-proof]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/src/sdk_edit_verify.rs
[old-access]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/shared/historical_access.py
[repository-history]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/shared/repository_history.py
[small-smoke]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/families/small_file_delta_smoke/README.md
[tiny-profile]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/families/tiny_file_churn/README.md
[snapshot-family]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/families/local_snapshot/mod.rs
[access-family]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/families/historical_access/mod.rs
[access-source]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/src/v016_access.rs
[m1-source]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/benchmark/fs-bench-pro/src/v016_mixed.rs
[report152]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md
[ledger151]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md
[reliability-fix]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/docs/roadmap/0.1/0.1.6/evidence/issue152-reliability-fix-report.md
[range-cache]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/immutable_read_cache.rs
[worker-source]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/changes.rs
[small-workers]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-layerstack-store/src/objects.rs
[core-request]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[raw-k10]: ../../../../../benchmark-results/v016/final3-seed1/performance/v016-history-boundary-cycle-k10-v1/1/candidate/run1/perf.jsonl
[raw-k100]: ../../../../../benchmark-results/v016/final3-seed1/performance/v016-history-boundary-cycle-k100-v1/1/candidate/run1/perf.jsonl
[raw-branch]: ../../../../../benchmark-results/v016/final3-seed1/performance/v016-branch-mixed-500mb-30000-k100-v1/1/candidate/run1/perf.jsonl
[raw-four]: ../../../../../benchmark-results/v016/final3-ext/performance/v016-workspace-four-100mb-5000-k100-v1/1/candidate/run1/perf.jsonl
[raw-access]: ../../../../../benchmark-results/v016/final3-seed1/verification/v016-access-boundary-after-v1/1/candidate/run1/verification.json
