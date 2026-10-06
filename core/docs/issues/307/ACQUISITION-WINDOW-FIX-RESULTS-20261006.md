# Acquisition execution correction: qualification results

> **Status:** Archived; retained for historical evidence only.
> 2026-10-06. The execution correction is implemented; the full speed issue remains open.

Source4c03b41bf5fcc684df3a707d223ad2b9166396f8, tree6f8770c98ba9648b1af6a2e1e0bfafb5a57d2c8a, implements
statement leases per entry/place unit and fixed32-row indexed root updates. It
keeps the global SQLite owner, three acquisition tables, versions4–6, public
ports, row/byte windows, alias/evidence ordering, transaction boundaries and
persistence settings. Missing, repeated and invalid positions preserve first
failure and full-unit rollback; unknown execution retains the quarantine fence.

Root completion for1000 files is1004→36 SQL statements. Complete acquisition is
3178→2200 statements,390830→394041 VM steps, with8 write commits unchanged and
1023 constant-input scan steps explicitly included. This improves execution
overhead; it does not remove necessary row updates or durability. A32-root SQL
input has5 statements/2176 VM/31 constant-input steps at both2000 and20000 stored
rows,0 sorts/autoindex/reprepares, and indexed target SEARCH. RETURNING order is
not assumed. The two constant-input scans are distinct from stored-population
scans; all other acquisition plan constraints remain enforced.

[Source proof and diagnostic receipts](checks/init-acquisition-fix-20261006/identity.json),
[retained failures](checks/init-acquisition-fix-20261006/FAILURES.md) and
[count attribution](checks/init-acquisition-fix-20261006/count-attribution.json)
contain the exact source hashes, commands, clocks, limits and scope. Host
Persistence100/Project76/Storage55/SDK26 bodies, final8 acquisition bodies,40
tooling tests, host/Linux Clippy, workspace/Linux no-run builds, guard and fmt
pass. Broad provider proofs precede the final first-invalid-position refinement;
final affected acquisition/Clippy checks cover it. Linux global Store runtime
is unsupported; compilation is not a claim of provider bodies there.

## Changed-source performance checkpoint

The [prospective affected selection](../../../../docs/roadmap/0.1/0.1.7/namespace-init-acquisition-window-fix-20261006.md)
uses the unchanged acquisition-v2 cases and30s complete performance/19s proof/
30s build caps. One fresh candidate per case; no replay or replacement sample.
The four unchanged reference receipts at45e2b09e8/7edddbdb8 are reused as earlier
evidence after verifying relevant source/product/SQL/compilation/dependency/
root-config/driver/binary/helper/case/fixture/oracle identities. Harness seal,
case names, cache and gates are identical. Candidate seals/binaries changed
with its source. Original candidate outcomes and reference observations remain
unchanged. [Exact ledger](checks/init-acquisition-window-results-20261006/ledger.json).

| Case | Reference complete product ns | Previous candidate ns | Fixed-source candidate ns | Fixed/reference | Observed fixed/previous delta | Joint gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Durable100 | 41992917 | 97931000 | 94632292 | 2.253530× | -3.368% | FAIL |
| Disposable100 | 39485000 | 40476833 | 41976291 | 1.063095× | +3.704% | PASS |
| Durable1000 | 119535750 | 225763292 | 239892625 | 2.006869× | +6.258% | FAIL |
| Disposable1000 | 124912209 | 152242291 | 145382875 | 1.163880× | -4.506% | FAIL |

Gate arithmetic stays `10*candidate_product_ns <= 11*reference_product_ns`.
All storage gates pass. Disposable1000 improves by an observed4.506%, but remains
16.388% slower than the reference and needs7979445.1 ns less complete product
time to reach the unchanged10% margin. Durable1000 is an observed6.258% slower
than the earlier candidate; do not call the whole correction a universal speedup.
Single observations are not medians, distributions or repeatability evidence.

| Case | Complete performance ns /30 s | Separate proof ns /19 s | Reference → candidate allocated B | Driver CPU ns | Driver lifetime RSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Durable100 | 1056548167 | 541847208 | 7372800 → 5304320 | 67954000 | 39731200 |
| Disposable100 | 72078500 | 17828083 | 7372800 → 5275648 | 56214000 | 39780352 |
| Durable1000 | 327949958 | 31534125 | 23101440 → 20938752 | 262102000 | 55312384 |
| Disposable1000 | 246394625 | 32028541 | 23101440 → 20893696 | 215070000 | 54820864 |

All four operations, builds, cold-content attestations, independent sampled
verifiers, equal pair roots and cleanup pass. No timeout/refusal/uncertain outcome
occurs in the qualification selection. Every input regular-file page attests
resident_after=0 anew. Metadata residency remains unobserved. Final allocation
is not a disk/journal peak, and Darwin wait4 driver lifetime RSS is not phase
or whole-system residency. The verifier inventories102/1011 paths and checks
53 files/3354003 B or70 files/6430827 B from the5/20 MB roots, not every byte.

Reference remains MEMORY/OFF split Storage/history. The candidate Durable profile
remains WAL/FULL/fullfsync; Disposable is MEMORY/OFF. The publication/durability
difference stays explicit rather than changing a profile or waiving the gate.

## Remaining cause and next work

The retained revised-source traces narrow the remaining work: Disposable1000
has63654236 ns of observed SQL components, including25800874 ns of COMMIT, and
51255044 ns of inclusive Save metadata registration. Durable1000 has147968414 ns
of SQL components, including105016915 ns of COMMIT, and100029709 ns of Save metadata
registration. These spans overlap; never sum them into a manufactured breakdown.
The statement metric now charges prepare/execution/counters/lease return, with
prepare/return once per shared lease; transaction/product clocks retain all
surrounding work. Diagnostic times are uncontrolled/instrumented and ineligible.

Source inspection after these samples confirms Monolithic immutable-body INSERTs
already borrow pack bodies in bounded pages (`sqlite/publish.rs`); there is no
extra first-party20 MB body clone in that path to remove. Safe SQLite binding
does copy into the engine, and the metadata/publication path requires further
statement/physical-I/O attribution before another correction. No speculative
publication rewrite or unchanged-arm rerun was performed. The execution overhead
is corrected, while the three required relative-speed failures remain open.

Concrete next-ready work: registered publication/reservation/COMMIT/copy/page
attribution for the retained1000-file cases, then a source-backed correction
under unchanged authority, durability, capacity and one-attempt contracts.
Independent R1 (host supervisor/consumer attachment) and E1 (complete S7/S9
observer/workload registration) remain ready. R2–R4/E2–E4/Q1/C1 remain open;
P3/P6/P7/P13/P14 are later Commit prerequisites. S7/S8/S9 remain unchecked.

All10000/100000 profile cases remain NOT_RUN; v1 limits/evidence and all historical
failures remain unchanged. The original full raw outputs/SQLite stores and sealed
binaries are retained in the attached measurement checkout and independently
copied to primary `benchmark-results/fs-bench-pro/`. [Closed-copy hashes](checks/init-acquisition-window-results-20261006/closed-copy-manifest.json)
verify every retained sample file. [Setup/reference reuse](checks/init-acquisition-window-results-20261006/setup-reuse.json)
and [commands](checks/init-acquisition-window-results-20261006/commands/) retain exact
methods/seals and120s outer stops. Four unrelated containers, both untracked owner
notes and the closed S5/S6 record remain untouched. No push/release/deployment
or reference retirement occurred.

Production LOC for4c03b41bf: core93991→94185 (+194), reference65417→65417 (+0),
combined159408→159602 (+194), exact first-parent/staged/committed tree verified
in [source receipt](checks/init-acquisition-window-results-20261006/source-committed-loc.json).
The result/audit receipt commit retains core94185/reference65417/combined159602,
delta+0. Same unchanged `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, exact
first-parent/final staged archives, core/crates+crates including shipped SQL,
production exclusions for tests/inline tests/examples/harnesses/docs/tools/
manifests/builds/third-party. Commit message and tracker record final tree verification.
