# Independent entry execution: Init qualification

> **Status:** Archived; retained for historical evidence only.
> 2026-10-06. Implemented component correction; complete parity remains unqualified.

The bounded entry correction is implemented and independently verified. Both
selected Disposable cases now satisfy the unchanged competitive speed/storage
gate. Disposable1000 completes in132742708 ns, down from the earlier corrected
candidate's145382875 ns, and is6.269% above the124912209 ns reference. Durable100
and Durable1000 retain FAIL. This checkpoint does not establish universal speedup,
repeatability or matched-profile parity with the older run-backed Project.

## Source and mechanism

Local-only source `a6bd6860c5061e03c4803c8962d847eb348d6069`, tree
`91ba0a4867426cab042e0e4fdf5bb681e16c09e0`, first parent
`aed249da2fb2950edc14f516f8754096040027a5`. Implementation is in Persistence's
[entry inputs](../../../crates/layerfs-persistence/src/backend/sqlite/acquisition/entry_windows.rs)
and three shipped SQL statements. [Architecture](../../architecture/44-acquisition-backing.md#independent-entry-execution-inputs-2026-10-06)
records ownership, complexity and resource obligations. Project/Storage/public APIs,
Store/schema4–6, profiles, indexes and public row/byte windows are unchanged.

A fixed32-entry input proves independence using indexed existing entry/directory/
native key checks and bounded distinct-input sets. Known aliases, key/position
collisions and malformed shapes end the prefix before any attempted mutation;
the boundary executes once in the original native-upsert/entry-insert order.
Independent new rows use two INSERT executions without RETURNING materialization.
The same original short transaction encloses all inputs, ordered boundary rows
and the single exact charge update. Full-unit rollback, canonical first identity,
byte-equal evidence, aliases, original errors, epoch/owner/abandoned checks and
unknown Session quarantine remain. There is no failed-operation replay or alternate
Project algorithm. No extra database, writer, constructor, dependency, profile
change, total-root transaction or input-sized resident mirror is introduced.

For K input entries and N stored indexed rows the bound remains O(K logN), plus
real key/BLOB/index/page work. SQL scratch and four borrowed-key sets each contain
at most32 rows; repeated dependencies may inspect32 candidates each, a fixed
factor. Names/paths/roots stay borrowed and only60-byte native facts are packed.
This is source reasoning, not measured whole-system memory or physical I/O.

## Eligible affected checkpoint

[Prospective registration](../../../../docs/roadmap/0.1/0.1.7/namespace-init-entry-window-20261006.md)
selects the unchanged acquisition-v2 cases with30s complete performance/19s
separate proof/30s build caps. Four candidate samples ran once in registered order.
The sole unchanged runner drives public Project Init, fresh Store creation,
checkpoint/allocation release and final close. Init is nested inside complete
product time; no lifecycle or cleanup term is subtracted.

The four original qualifying reference receipts are reused after verifying the
actual unchanged7edddbdb8 source, product/shipped-SQL/compilation/dependency/root
Cargo config, wrapper, binaries, helper, case, fixture and oracle identities.
[Reuse receipt](checks/init-entry-window-results-20261006/setup-reference-reuse.json).
Harness seal remains890afa94138e347672c9b0d5ed3ddc09c90718ba2b9f9d77b9058ccc092bd7f6.
Candidate source/binaries changed; original reference/cache/wall/proof observations
and all previous PASS/FAIL/NOT_RUN outcomes retain their original identities.

| Case | Reused reference product ns | Previous candidate4c03b41bf ns | Candidatea6bd6860c ns | Candidate/reference | Observed candidate/previous delta | Joint gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Durable100 | 41992917 | 94632292 | 107346666 | 2.556304× | +13.436% | FAIL |
| Disposable100 | 39485000 | 41976291 | 39661541 | 1.004471× | -5.514% | PASS |
| Durable1000 | 119535750 | 239892625 | 228736916 | 1.913544× | -4.650% | FAIL |
| Disposable1000 | 124912209 | 145382875 | 132742708 | 1.062688× | -8.694% | PASS |

Speed arithmetic is exactly `10*candidate_product_ns <= 11*reference_product_ns`.
Disposable1000 operands are1327427080 <=1374034299; its earlier FAIL remains a
FAIL at its original identity. Durable100's slower observation remains retained.
One observation per source/case is not a distribution or repeatability claim.

| Case | Complete performance ns /30s | Independent proof ns /19s | Reference → candidate final allocated B | Driver CPU ns | Driver lifetime RSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Durable100 | 1952624375 | 704989833 | 7372800 →5304320 | 86509000 | 40337408 |
| Disposable100 | 67961458 | 19028583 | 7372800 →5275648 | 54836000 | 40255488 |
| Durable1000 | 340332250 | 31609167 | 23101440 →20934656 | 241616000 | 54083584 |
| Disposable1000 | 230087959 | 31559500 | 23101440 →20910080 | 201632000 | 51249152 |

All builds, operations, cold-content attestations, independent sampled verifiers,
root comparisons, cleanup and absolute caps PASS. No timeout, refusal, unknown
outcome or replay occurred in this selection. All storage gates PASS. Source
regular-file pages attest resident_after=0 anew for each arm; metadata residency
remains unobserved. Final allocation is not a disk/journal peak; wait4 RSS is the
driver lifetime, not phase or system residency. Constructor count remains4 with
LAYERFS_CONSTRUCTION_WORKERS=1 and ARM64 flags verified in Cargo fingerprints.
The four unrelated running containers remain declared interference and untouched.

The oracle inventories102/1011 paths and checks every kind/directory metadata,
then53 files/3354003 B or70 files/6430827 B from5/20 MB roots. It is a sampled
payload oracle, not every payload byte or full S9 acceptance. Pair roots equal
ca1c20f806277e3fb0447ef526a5ca8d298753d01667f501e8b58b6350cd86f1 and
a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c respectively.
Server/daemon/FUSE/transport scopes are N/A on this direct component route.

Reference is Phase4.5 MEMORY/OFF split Storage/history. Candidate Durable remains
WAL/FULL/fullfsync and Monolithic schema4; Disposable remains MEMORY/OFF. The
profile/layout difference is inherited and disclosed. The old Projectabdb322f42
comparison used matched profiles but uncontrolled instrumented clocks: it remains
[diagnostic/ineligible](PROJECT-OLD-CURRENT-DIAGNOSTIC-20261006.md), not a speed arm.
No eligible matched-profile older/current performance selection ran here. Neither
the two new PASSs nor the diagnostics establish overall older-Project parity.

## Counts, proof and remaining work

The final public point-window diagnostic uses7 executions/7828 VM at both2000
and20000 stored rows, against the retained baseline68/5353. All stored-state
classifier accesses are indexed SEARCH; constant-input scans are explicit. This
removes89.706% of executions while increasing VM46.236%. Both the plan and actual
runtime work, rather than elapsed time alone, establish the changed mechanism.

One separately registered public `acquisition_profile` Disposable1000 operation
attributes the full pipeline. Its cache is uncontrolled and instrumentation is
enabled, so clocks are ineligible. [Selection and raw counts](checks/init-entry-window-results-20261006/diagnostic-selection.json).

| Acquisition scope | Previous executions | New executions | Previous VM | New VM | Write commits |
| --- | ---: | ---: | ---: | ---: | ---: |
| Entry/native insertion,3 units | 2023 | 114 | 165261 | 246848 | 3 unchanged |
| Whole acquisition,38 calls | 2200 | 291 | 394041 | 475628 | 8 unchanged |
| Discard,1 unit | 7 | 7 | 117946 | 117946 | 1 unchanged |

Native path lengths and logical working-byte charges differ between checkout
locations; they are not a comparable memory/storage claim. The increased preflight VM is retained: full acquisition rises20.705% while
execution count falls86.773%. Complete Init in this diagnostic has569 statements/
677951 VM/26 writes. Only8 writes belong to acquisition; changed surrounding
Save packing/physical publication counts are not silently attributed to it.
The earlier whole-Init25-write observation remains unchanged. Fixed root inputs
retain1023 fullscan steps; new entry inputs measure0 fullscan steps. The changed
path returns40 entry-unit rows against1006 before; actual journal/device/copy/page
costs remain unavailable. Root and1011 entries match the compact oracle.

Final source checks:102 Persistence and157 Project/Storage/SDK bodies PASS, locked
no-run builds before execution, host/Linux changed-scope warning-denying Clippy,
Linux no-run, fmt,652-file boundary and40 tooling tests PASS. Functional walls are
20.854244750/32.291691792s under110s stops; no timeout. Failure receipts retain an
incorrect zero-body exact filter, the expected baseline count failure and the
repaired Rust compile failure. Provider binary seals are post-run only; consumer
pre/post hashes match. Linux global Store runtime remains unsupported. Actual
acquisition capacity/unknown fault cases were not induced; shared Session failure
proofs retain their own scope. [Exact source/check/custody receipt](checks/init-entry-window-20261006/identity.json).

Next source-backed candidate is budget-safe final discard: it still owns117946
VM and materializes2011 deleted row charges. Any change must preserve actual table/
charge consistency, row budget, exact byte credit, stale/abandoned custody and
unknown quarantine. Publication/reservation/COMMIT/page/copy attribution is still
required before changing immutable-body publication, which already borrows pack
bodies. Durable1000 retains99260376 ns inclusive COMMIT and136446882 ns SQL;
Disposable1000 retains23219585/54417932 ns. These overlap and are not additive.
Extra source-qualified preflight work also needs review for alias-heavy shapes.

The owner goal of overall parity remains unmet/unqualified. Larger10000/100000
Durable/Disposable tiers remain NOT_RUN; v1 remains unsampled at15/9.5s. S7/S8/S9
remain incomplete; R1–R4/E1–E4/Q1/C1 and later Commit P3/P6/P7/P13/P14 stay open.
No later runtime lane, push/release/deployment, third-party edit or legacy retirement
was undertaken. The selected bounded entry correction and its affected qualification
are delivered, without treating partial gate success as full acceptance.

## Retention and exact LOC

[Ledger](checks/init-entry-window-results-20261006/ledger.json),
[commands](checks/init-entry-window-results-20261006/commands/) and
[closed-copy hashes](checks/init-entry-window-results-20261006/closed-copy-manifest.json)
retain exact source/product/SQL/compilation/dependency/binary/helper/harness/fixture/
cache/profile identities, raw ns/B and gate operands. Original full outputs remain
in this chat's managed `/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs`.
Independent complete raw outputs, closed databases and binary copies remain under
primary `benchmark-results/fs-bench-pro/init-entry-window-retained-20261006/`.
Compact text receipts are committed; database/binary files stay outside Git.
Two predecessor diagnostic worktrees, observer sources, owner notes, side documents,
closed S5/S6 handoff, root reference and four unrelated containers are preserved.

Source commit production LOC: core94185→94495 (+310), reference65417→65417 (+0),
combined159602→159912 (+310). [Exact source staged/committed-tree receipt](checks/init-entry-window-20261006/source-committed-loc.json).
Same unchanged counter SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
git archive exact first-parent/final staged core/crates+crates including runtime
SQL and application adapters/excluded predecessors, production exclusions and
inline-test handling. Committed tree matches the prepared tree. This is additional
provider logic, not relocation or reference retirement. The result/audit commit is
docs/receipts only and retains core94495/reference65417/combined159912, delta+0;
its exact final staged/committed-tree verification is recorded separately.
