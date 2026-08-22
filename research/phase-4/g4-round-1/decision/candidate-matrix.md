# G4 Round-1 candidate matrix

Status: **research ranking only — no implementation or promotion**

The matrix limits the decision set to three `DO NOW / G4`, three later
architecture/profile candidates, and three rejected/deferred directions. Local
facts are bound to checkpoint `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
Historical timings are ceilings and hypotheses, not adjacent evidence.

## Ranking summary

| Rank | ID | Candidate | Disposition | Decisive next action |
|---:|---|---|---|---|
| 1 | G4-M0 | Unchanged G3 `M0-control`, then batched proof-preserving `M0-candidate` | **DO NOW / G4** | Measure immutable control first; candidate cannot overwrite or relabel it |
| 2 | G4-S1 | Same-open protected-seed full read plus retained clone/patch/fallback | **DO NOW / G4** | Score logical seed read separately from clone; qualify v13 fast/fault routes |
| 3 | G4-R1 | Prove and A/B one redundant closure product in shared verified stream | **DO NOW / G4** | Keep raw oracle; remove only closure after formal authority/error proof |
| 4 | L-A1 | Bounded content-root verified native cache under stronger custody | **LATER ARCHITECTURE** | Specify authority, cap, eviction, rebuild, corruption, and restart first |
| 5 | L-P1 | Bounded SQLite-resident authenticated extent BLOBs first; external immutable segments only after Stage-A insufficiency | **LATER PHYSICAL PROFILE** | Run the one-file SQLite extent lower bound first; external value-plane work remains conditional and never a second durable truth |
| 6 | L-I1 | Direct VFS range/sequential streaming over shared engine boundary | **LATER INTEGRATION** | Define handles/backpressure/errors after G4 freezes the engine primitive |
| 7 | X-A1 | Trust mutable destination/seed metadata or receipts | **REJECT** | No performance experiment is admissible |
| 8 | X-C1 | Call restart, reopen, `F_NOCACHE`, or shared-host state cold | **REJECT** | Retain `Unavailable`; use exclusive-host purge approximation only |
| 9 | X-F1 | New CDC/mapping/Merkle/pack/compression/loose-file format in G4 | **DEFER/REJECT FOR G4** | Reopen only from a measured post-G4 bottleneck and new versioned profile |

## DO NOW / G4

### G4-M0 — unchanged native control, then batched proof-preserving candidate

| Field | Decision record |
|---|---|
| Mechanism | First measure the unchanged frozen G3 complete fallback as `M0-control`. It is immutable and diagnostic: one native stream/publisher, derived ~5,371-query S1-100 shape, and omitted closure/occurrence outputs. Then, as `M0-candidate`, reuse accepted `verify_file_inner`/`stream_file` and at-most-64-reference leaf batching, add a bounded `Write` sink after complete chunk authentication, and preserve root/topology/closure/occurrence/output/error semantics. Publish through the same descriptor-relative private-temp old-or-new protocol. The candidate never overwrites or relabels the control. |
| Target paths | One-file engine qualification only: first/full empty-destination native materialization and clone/cross-volume/count-change/invalid-authority fallback. No directory/workspace/VFS/SDK acceptance. Logical reconstruction is separate R0; create, edit, range, and reopen are protected. |
| Complexity | `Theta(A + S + N + M)` authenticated traversal and `S` native writes, single-thread span; bounded `O(max_chunk + mapping/batch + writer_buffer)` application state. No second full userspace reconstruction buffer/pass. |
| Measured ceiling | Warm accepted logical reconstruction is 338.775916 ms; fresh process 366.356667 ms with OS cache warm-or-unknown. First/full 100-MiB native wall is unavailable. G3's complete fallback is diagnostic only: derived roughly 5,371 queries for S1-100 and omitted accepted proof products, versus the accepted 170-query/5,371-row path. |
| Predicted gain | This is not a logical-read speed claim. It supplies the missing native operation without a second full userspace pass. For warm S1-100, `T_native = 338.775916 ms + native_write/sync/publish - overlap`; the 400-ms acceptance objective permits at most 61.224084 ms net native overhead. The actual overhead is unavailable until G4. |
| CPU | Retains canonical, closure, occurrence, and output folds; adds write/syscall/system CPU. No worker pool. Record user/system, core span, context switches, and instructions/cycles only if directly supported. |
| Memory/Q | No full-file buffer. Writer buffer at most 1 MiB; exact Q includes it and terminal Q is zero. Process RSS `<=20 MiB`; SQLite cache reported separately. |
| Storage | One private temp of apparent logical length `S`, renamed to the final path. No seed and no per-revision native duplicate. Record temp/final logical, apparent, allocated high-water and zero residue; physical bytes remain unavailable. |
| Authority | Expected root/head/profile plus complete Canonical-v2 graph/object authentication and all accepted evidence folds. Private prefix writes never authorize publication; any later failure discards the temp. Mutable destination is never a source. |
| Durability | Explicit file data/metadata sync policy, no-follow descriptor-relative atomic rename, parent-directory sync, and exact target/prior reconciliation on ambiguous acknowledgement. Stable-media completion beyond OS contracts remains unavailable. |
| Identity/format | No identity, mapping, schema, receipt, or storage-profile change. Shared narrow engine stream plus benchmark-private OS publisher; production VFS/SDK remains false. |
| Cross-operation effect | Expected zero direct create/edit/range/reopen change; nevertheless enforce adjacent <=5% gates and exact counter/error parity because traversal code is shared. G3 clone/patch remains a separate route. |
| Experiment | First use the exact global lock `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/target/BENCHMARK_LOCK` for one separate <=120-s candidate build plus focused 1/10-MiB semantic/resource screen, analysis, cleanup, and source restoration; freeze a passing candidate identity. Then reacquire it for the no-build main campaign: once-only 1/10/100-MiB `M0-control` slots precede once-only 1/10/100-MiB `M0-candidate` slots in the exact 30-row chronology. Count SQL/rows/BLOBs/auth/folds/output/sync/rename/reconcile/RSS/Q/storage. Post-publication reread stays separate and identical. Full measured campaign, prep through terminal cleanup, <=120 s. |
| Evidence | Accepted walker at `phase4_create_edit_benchmark.rs:8238-8368,9038-9177`; native publication at `phase4_g3_materialization.rs:1954-2250`; specialist materialization report sections 4 and 8; reconstruction cross-review correction. |
| Disposition | **DO NOW / G4 rank 1.** Kill on lost batching/proof/error semantics, unauthenticated final exposure, RSS >20 MiB, terminal Q/residue, any protection failure, or 100-MiB warm wall >400 ms. |

### G4-S1 — protected-seed read, clone, patch, and fallback

| Field | Decision record |
|---|---|
| Mechanism | Keep G3-v13's exact read-only/no-follow unlinked seed descriptor, same-open authority binding, single-use permit, whole-file clone, authenticated selected-range patch, two-sync publication, fallbacks, reconciliation, and cleanup. Add a distinct full logical seed-read row that sequentially consumes bytes from the already-qualified descriptor; never substitute clone wall for returned-byte wall. |
| Target paths | Trusted same-open full read, same-root clone/no-op, one-byte and 1-MiB same-size materialization, invalid authority/external mutation/count change/clone miss, and focused publication faults. No persistent/fresh-process trusted-seed claim. |
| Complexity | Full seed read `Theta(S)` returned bytes and `O(B)` memory. Qualified clone is filesystem metadata/span plus `O(changed authenticated bytes)` patch and publication; fallback is G4-M0 `Theta(A+S+N+M)`. |
| Measured ceiling | Retained once-only screens: 10-MiB no-op 0.993791 ms; 100-MiB one-byte 3.414166 ms; 10-MiB 1-MiB patch 2.926167 ms. Trusted-seed 100-MiB full-read wall and seed-fill/rebuild wall are unavailable. |
| Predicted gain | Exact bandwidth arithmetic: 100 MiB / 50 ms = 2,000 MiB/s; /35 ms = 2,857.14 MiB/s. A cache-hot same-open descriptor read without an in-timer digest may reach the target, but this is unmeasured. Clone/no-op and one-byte objectives are <=10 ms, stretch <=5 ms; never report clone logical throughput. |
| CPU | Full read copies `S`; optional digest is a separate `Theta(S)` CPU fold. Qualified clone/patch hashes only authority and selected canonical chunks. Full reconciliation may become `Theta(S)` and is charged. |
| Memory/Q | Fixed <=1-MiB read/patch buffer; RSS <=20 MiB; G3 exact reconciliation buffer stays charged; terminal Q zero. Seed bytes are persistent/cache storage, not application Q. |
| Storage | Operation-local seed plus clone temp/final have apparent length `S`; APFS sharing and physical bytes are unavailable. No history cache. Seed preparation/full reconstruction/write/readback, qualified hit, post-operation verification, and maintenance/rebuild are separate ledgers and never hidden. The retained 100-MiB one-byte raw row held a 100-MiB seed and 100-MiB temp and read 100 MiB for verification outside its 3.414166-ms operation timer. |
| Authority | Valid only while the authenticated unlinked descriptor remains alive in the same authority lifetime. Root/receipt selects the key but does not authenticate reopened bytes. A descriptor can be passed to another live process, but broker restart loses this capability and requires full reauthentication/rebuild. |
| Durability | Exact v13 data sync, metadata sync, no-follow rename, parent sync, ambiguity reconciliation, error precedence, permit consumption, and cleanup. Full reads are read-only. |
| Identity/format | Canonical-v2 unchanged; macOS/APFS acceleration with complete portable fallback. No persistent cache/profile. |
| Cross-operation effect | Preserves fast partial range behavior and current create/edit paths. The retained operation-local 100-MiB row was 4.24 s external real, 3.23 s user, and 0.91 s system for the whole child despite a 3.414166-ms hit. Fill/qualification, hit, and revalidation/eviction/repair/rebuild CPU/RSS/storage are separate. Count changes remain complete fallback. |
| Experiment | One 10-MiB descriptor-read screen and at most one 100-MiB full-read primary, exact consuming sink and independent oracle; then the compact v13 10/100-MiB clone/no-op/one-byte/1-MiB rows plus focused 1-MiB faults. Kill full-read claim above 50 ms, clone/one-byte above 10 ms, any v13 counter/error drift, hidden seed preparation, RSS >20 MiB, or residue. Total <=120 s. |
| Evidence | `phase4_g3_materialization.rs:1303-1383,1632-2250`; G3-v13 raw/report/baseline; reconstruction and materialization specialist reports. |
| Disposition | **DO NOW / G4 rank 2.** Qualify only the exact live-descriptor scope; persistent authority remains later architecture. |

### G4-R1 — verified-stream authority/evidence separation

| Field | Decision record |
|---|---|
| Mechanism | Formalize a shared narrow engine primitive conceptually `materialize_verified_file(expected_root, path, sink) -> VerifiedFileSummary`. Pin one catalog generation; authenticate every mapping/chunk object and all Canonical-v2 topology. Prove whether the separate closure commitment is derivable from the authoritative root plus per-object authentication. In the first A/B, keep the raw output digest and remove only closure work after the proof; preserve negative-case and identity-first error semantics. Native publication is a separate consumer of a successful summary. |
| Target paths | Warm/fresh logical reconstruction, scrub, and the authenticated traversal used by G4-M0. Later VFS may consume it; range routing stays current. |
| Complexity | Current logical work is `Theta(A auth + A closure + S output digest/delivery)`; candidate stays `Theta(A+S)` but removes a constant full canonical closure fold. No sublinear full-read claim. |
| Measured ceiling | G2 closure family median 88.483070 ms; current warm row 338.775916 ms. Occurrence commitment is 0.408711 ms and secondary decode 0.141476 ms—too small to lead. Component medians are candidate ceilings, not additive predictions. |
| Predicted gain | Plausible 20–88 ms if formal equivalence closes. Impossible full-ceiling subtraction: `338.775916 - 88.483070 = 250.292846 ms` (396.37 MiB/s); this is an upper bound, not a result. Acceptance goal is <=333 ms, stretch <=300 ms. |
| CPU | Removes one full closure hasher/update domain only; full canonical Object-ID authentication and raw output digest/delivery remain. No parallel worker assumption. |
| Memory/Q | Same bounded rows, mapping frames, and hash/sink state; fewer hasher fields. Exact Q, RSS <=20 MiB, and terminal zero. |
| Storage | None; compact external oracle artifacts only. |
| Authority | Expected root and every expected Object ID authenticate the ordered graph. Removal is allowed only after a formal proof and adversarial corpus show closure adds no independent binding. If receipt/API output includes closure, version that result rather than silently changing it. |
| Durability | Read-only primitive; G4-M0 publisher retains all native durability. |
| Identity/format | Canonical-v2/object/store layout unchanged. Verified-summary/evidence schema may need an explicit version. |
| Cross-operation effect | May improve reconstruction/scrub and M0 CPU. Create/edit/range/reopen should be byte-identical; protect each within 5%. No direct seed-hit gain. |
| Experiment | Adjacent position-balanced A/B at 1/10/100 MiB under the lock, one variable: closure on/off, raw digest retained. Exact outputs/root/topology/auth/SQL/rows/errors/Q/RSS/storage. Include missing/corrupt/wrong-role/length/order/cycle cases and independent authority review. Kill below 5% wall win, changed error/authority/result semantics, or any protected regression. <=120 s. |
| Evidence | Canonical-v2 mapping/root at `canonical_v2.rs:72-255`; current folds at `phase4_create_edit_benchmark.rs:2150-2169,8238-8368,9058-9177`; G2-v5 decomposition; core-architecture C1. |
| Disposition | **DO NOW / G4 rank 3 as contract proof + bounded A/B.** It is not permission to edit benchmark evidence after the fact. |

## LATER PROFILE / ARCHITECTURE

### L-A1 — bounded content-root verified native cache

| Field | Decision record |
|---|---|
| Mechanism | Keep Canonical-v2 durable truth. Admit a fully authenticated raw native seed keyed by `(store/profile, file-content/root identity)` into a derived cache with atomic insertion, allocated-byte and descriptor/entry caps, deterministic eviction, corruption quarantine/rebuild, and descriptor leases. Stronger custody is a separate-UID/service-owned namespace or equivalent; clients receive read-only descriptor capabilities. |
| Target paths | Repeated hot reads, same-root clone, incremental clone/patch, and future VFS. It does not establish canonical mapping/edit authority and therefore does not currently improve first edit after reopen. Miss/corruption/restart falls back to G4-M0 and rebuilds. |
| Complexity | Fill/rebuild `Theta(A+S)` plus native write/readback; read hit `Theta(S)`; clone hit metadata/span + changed bytes; index bounded by entries/cap. |
| Measured ceiling | G3 proves only live same-open clone/patch walls. Persistent lookup, broker, restart, fill, eviction, and trusted full-read costs are unavailable. |
| Predicted gain | Supported same-volume clone hits plausibly remain <10 ms; live cache-hot full reads may target <=50 ms. After broker restart, current architecture must fully reauthenticate/rebuild, so persistent <50 ms is not a present claim. |
| CPU | Hits avoid SQLite/mapping/canonical hashes only while custody is valid; fill/rebuild pays all auth and verification. Foreground miss/revalidation and maintenance eviction/repair/rebuild each report wall, user/system CPU, RSS/Q, I/O counters, and storage; none is free background work. |
| Memory/Q | Bounded streaming buffer/index/request concurrency; application RSS <=20 MiB. No decoded/full-file memory cache. |
| Storage | Hard allocated-byte cap `K` plus bounded index/temps. Example `K=2 GiB`: 10 distinct 100-MiB roots may use about 1 GiB; 100/1,000 roots plateau at 2 GiB via eviction. Never `revisions*S`. APFS clone sharing remains unsupported physical evidence. |
| Authority | No current primitive survives true broker restart with exact byte authority. Named 0600 files, flags, inode/timestamps, receipts, root IDs, or clone provenance are insufficient against same-UID rollback/substitution/mutation. Restart discards or fully revalidates. |
| Durability | Cache is rebuildable, but hit eligibility follows complete auth -> sync -> atomic cache rename -> cache-dir sync. Destination publication stays separate. |
| Identity/format | Canonical format unchanged; derived cache index disposable. A content-only seed identity distinct from file root would require authenticated versioned state. |
| Cross-operation effect | Hot reads/materialization improve; first miss/create may regress due admission. Protect create/edit/range/reopen; disclose hit rate and churn. |
| Experiment | First simulate 10/100/1,000-root capacity/eviction/corruption traces. After authority design, a <=120-s miss/hit/restart/corruption screen with explicit cap. Kill if hit rehashes `Theta(S)`, cap/high-water fails, restart trusts mutable bytes, or miss/create regresses >5%. |
| Evidence | G3 seed authority and retained rows; specialist cross-review consensus; Apple clone semantics; Nix verification precedent. |
| Disposition | **LATER ARCHITECTURE rank 1 / top disruptive candidate.** It is the best cross-operation upside, conditional on a real protection domain. |

### L-P1 — SQLite-resident extent BLOB first, external segment only later

| Field | Decision record |
|---|---|
| Mechanism | First aggregate unchanged canonical frames into bounded authenticated extent BLOB rows inside SQLite, keeping SQLite/canonical CAS as the sole durable authority. Catalog object-to-extent offsets remain transactional. Only if this direct-counter lower bound is insufficient may a later external immutable segment/value plane be modeled, with SQLite still the sole catalog/head authority and no second logical truth. |
| Target paths | Create, reconstruction, first/fallback materialization, ranges, scrub, reopen, compaction/GC, and VFS. |
| Complexity | Full read `Theta(S)` frame/auth work plus batched catalog and `K` coalesced reads; create appends new canonical bytes; GC copies selected live segment bytes off foreground. |
| Measured ceiling | Entire current SQLite/BLOB acquisition family is 59.403771 ms, only a gross ceiling; accepted path already uses 83 leaf batches for 5,284 references. Historical custom carrier had ~10.3 index reads/lookup and ~4.02x reopen reread, a negative control. |
| Predicted gain | Plausible 20–55 ms full-read gain only if spans coalesce and catalog stays batched; impossible zero-cost acquisition floor `338.775916 - 59.403771 = 279.372145 ms`. First-create benefit is unknown and can be negative after segment sync + SQLite commit. |
| CPU | Less SQLite row/BLOB overhead; same canonical hash/grammar; small frame/checksum parsing. |
| Memory/Q | One max frame plus <=1-MiB read window and mapping/catalog batch; RSS <=20 MiB; no full segment/file buffer. |
| Storage | Canonical payload once plus frame and locator metadata, <=5% steady-state apparent/allocated regression. Orphan tail and GC generation high-water explicit across 10/100/1,000 revisions. |
| Authority | Head/root in SQLite; locators are untrusted until frame canonical length/kind/Object ID validate. Frame checksum is diagnostic only. |
| Durability | SQLite-resident extent arm retains one `FULL` SQLite transaction/COMMIT and existing rollback-journal authority. An external segment arm, if ever reached, requires complete frames -> segment sync -> SQLite catalog/head commit, orphan recovery, ambiguity reconciliation, GC generations, and reader leases. |
| Identity/format | SQLite extent arm is a new physical/schema profile with canonical IDs unchanged. External segment is a later, larger physical profile. Both require explicit dual-generation migration/downgrade; neither is G4 repair. |
| Cross-operation effect | May improve sequential reconstruction/materialization/scrub/VFS; could improve append create or regress sync/storage/range. Exact <=5% guards. |
| Experiment | Read-only 1/10/100-MiB lower bound: current per-object BLOBs versus bounded SQLite-resident extent BLOBs, identical auth/folds/output and direct query/row/extent/byte/CPU/RSS/Q/storage counters. Require a focused direct signal and no >5% range/storage projection. Only if insufficient and the external-layout incremental upside remains material may a separate segment `pread` lower bound run. |
| Evidence | Core-architecture C3 after monitor correction; G2 acquisition; rejected carrier; SQLite atomic-commit docs. WiscKey/Git are later design precedents, not permission for a second durable truth. |
| Disposition | **LATER PHYSICAL PROFILE rank 2.** SQLite-resident extent first; external value log conditional; neither DO-NOW G4. |

### L-I1 — direct VFS streaming on the current mapping

| Field | Decision record |
|---|---|
| Mechanism | Build versioned VFS handles over the shared verified engine primitive and current Canonical-v2 cumulative extents. Pin head/root/catalog generation at open; route reads to selected authenticated chunks; optionally use a valid native seed descriptor. Backpressure, cancellation, and typed errors are explicit. |
| Target paths | Lazy projection, partial/range and sequential reads, mmap policy where supportable, cache hints; full native checkout remains G4-M0. |
| Complexity | Open `O(head/path)`; range `O(H+J+R)` under whole-chunk IDs; full sequential remains `Theta(S)`. No intermediate complete file for a range read. |
| Measured ceiling | Current 1-MiB range 2.279209 ms; VFS implementation/wall is absent. |
| Predicted gain | Large avoided work for applications reading only selected ranges; no claim that a first full read becomes sublinear or <50 ms. |
| CPU | Current selected canonical auth plus copy; avoid full reconstruction/native publication for partial reads. |
| Memory/Q | Bounded per-handle read buffers and a global request/queue cap; aggregate RSS/Q budget explicit. |
| Storage | No mandatory full native projection; optional bounded native cache L-A1 only. |
| Authority | Pinned expected root/generation and complete selected-object authentication. Handles do not silently follow a moving head. Seed use requires L-A1 authority. |
| Durability | Read-only VFS; writes/captures and native publication remain separate APIs. |
| Identity/format | Canonical unchanged; VFS/SDK API version new. |
| Cross-operation effect | Range/partial UX improves and shares the same validator; create/edit untouched. Concurrency/cancellation becomes new G5/integration work. |
| Experiment | After G4 freezes the primitive, 1/10/100-MiB trace replay with sparse and full sequential workloads, exact selected/auth/returned bytes and handle generation. Kill on >5% current range regression, unbounded buffers/queue, or hidden full reconstruction. |
| Evidence | Current `read_file_range`; VFS/SDK five-line stubs; core-architecture C5. |
| Disposition | **LATER INTEGRATION rank 3.** Product-shaped, but not a G4 acceptance change. |

## REJECTED / DEFERRED

### X-A1 — trust mutable destination or persistent seed metadata

| Field | Decision record |
|---|---|
| Mechanism | Skip byte authentication because receipt/root/path/inode/mode/mtime/watcher/clone lineage matches. |
| Target paths | No-op, reopen, incremental, trusted seed. |
| Complexity | Apparent `O(1)` only by deleting required authority; honest verification remains `Theta(S)` without exclusive custody. |
| Measured ceiling | 94.816564-ms canonical-auth family, but it is required current-store work; G3 Attempt A is static NO-GO. |
| Predicted gain | Invalid. |
| CPU | Lower only by omitting security/correctness. |
| Memory/Q | Irrelevant. |
| Storage | Replayable sidecars add state without byte authority. |
| Authority | Fails out-of-band mutation, writable-fd mutation, same-UID substitution, watcher gaps, rollback, and replay. |
| Durability | Cannot settle lost acknowledgement or corruption. |
| Identity/format | Silent semantic downgrade. |
| Cross-operation effect | Reintroduces closed v11/Attempt-A defects. |
| Experiment | None. Static mutate/substitute/rollback while preserving hints already falsifies it. |
| Evidence | Validation receipt fields; G3 report/v11-v13 contracts; specialist consensus. |
| Disposition | **REJECT.** No performance result can redeem missing authority. |

### X-C1 — cold-state relabeling

| Field | Decision record |
|---|---|
| Mechanism | Claim process restart/reopen is cold, or use `F_NOCACHE` as ordinary-path eviction proof, or run global purge on a shared host and omit limitations. |
| Target paths | Reconstruction and first native. |
| Complexity | No mechanism change; changes only the label. |
| Measured ceiling | Fresh-process 366.356667 ms is explicitly warm-or-unknown. True device/controller cache state is unavailable. |
| Predicted gain | None; invalid evidence. |
| CPU | Unchanged. |
| Memory/Q | Unchanged. |
| Storage | Unchanged. |
| Authority | Cache labels are evidence custody, not byte authority. |
| Durability | Unchanged. |
| Identity/format | None. |
| Cross-operation effect | Makes results incomparable/misleading. |
| Experiment | Prospective exclusive host only: finish preflight, close operands, successful `/usr/sbin/purge`, immediate no-warmup row; label `controlled-host-buffer-cold-approximation`, device cache unavailable. If preflight fails, retain `Unavailable`. |
| Evidence | Local/Apple `purge(8)`, Apple `fcntl(2)`, current benchmark labels, materialization report experiment. |
| Disposition | **REJECT relabeling.** Permit only the explicitly limited future approximation. |

### X-F1 — format churn without a measured bottleneck

| Field | Decision record |
|---|---|
| Mechanism | During G4, add larger chunks/new CDC, prolly mapping, a second Merkle/outboard tree, macrosegment canonical IDs, thousands of loose reflink files, foreground compression/deltas, or Git-style packs. |
| Target paths | Claims broad read/materialization benefit but changes create/edit/range/migration/GC. |
| Complexity | Full reads remain `Theta(S)`; new metadata/build/GC/migration work. Larger chunks reduce constants but increase selected-range/edit amplification. Loose files add `Theta(N)` syscalls/inodes. |
| Measured ceiling | Range is already 2.279209 ms; BLOB acquisition is only 59.403771 ms; compression saved 4.1453% while costing ~147.8 ms encode, and local Git delta made the pack 844 bytes larger. |
| Predicted gain | Insufficient or unavailable. No 2x full-read evidence; high risk to protected wins. |
| CPU | Extra proof/codec/boundary/metadata work. |
| Memory/Q | New windows/queues/tree builders; boundedness unproven. |
| Storage | New proof/index/pack/inode/GC state; migration high-water. |
| Authority | Any new root/proof/CDC contract needs a versioned profile and adversarial proof. |
| Durability | New crash/compaction/repack protocols. |
| Identity/format | Breaking or new physical profile; never silent G4 repair. |
| Cross-operation effect | Risks create/edit/range/reopen/G3 wins to address smaller/unproven ceilings. |
| Experiment | Defer. Reopen one mechanism only after post-G4 counters identify its exact bottleneck, with a <=120-s 10-MiB falsifier and <=5% protected gates. |
| Evidence | Canonical-v2/CDC/mapping/storage research; core-architecture C6-C10; local compression/carrier results. |
| Disposition | **DEFER new Merkle/prolly/CDC profiles; REJECT loose-file reflink and foreground compression/pack for current workload.** |

## Decision discipline

The three G4 items are not one stacked candidate. G4-M0 establishes the native
boundary, G4-S1 qualifies the retained protected-seed route, and G4-R1 is a
separate one-variable authority/evidence A/B. If the 120-second total cannot
hold them with exact preparation and independent closure, G4-R1 moves to a
separate repair campaign; already passing rows are not rerun merely to rescue
noise.
