# Logical reference acquisition treatment

Status: Count-based mechanism proven; new competitive speed not yet measured.
Based on896e84c3b. Existing closure contract finalized-object-handoff.md requires
references in the same atomic write, earlier acknowledged batches or protected
storage. Save admission and registration enforce locator membership; exact reuse
and selected physical chains enforce authenticated reconstruction separately.

The former wave prefetch treats every located logical child as a physical chain
root. A new valid128-entry extent leaf referencing persisted32KiB chunks triggers
2 batched acquisitions and36 individual payload acquisitions totaling12275774B.
Its children are4194304 raw B. Reproducer sqlite_reference_acquisition first
failed (target/phase7-agent/logical-reference-before.log). This is a count-driven
mechanism diagnostic, not a speed arm or admission proof.

Membership query still includes all offered IDs, references and advisory
predecessors. Only chain-prefetch roots omit logical-only children. Offered IDs
and physical predecessors remain; dynamically chosen candidates still acquire
and check chains in the selector. Admission and publication closure unchanged.
Missing children still fail, exact reuse still acquires and compares bytes, and
authenticated readback of leaf/all128 chunks passes. No cache, transaction,
worker, pack/framing, codec, durability or failure policy bound changes.

Frozen verification:14 tests across sqlite_reference_acquisition,
sqlite_publication, sqlite_transaction_units, project init_sqlite (full100/1000
namespace oracle); scoped storage/persistence/project all-target Clippy-Dwarnings;
core fmt, boundary guard and23 guard self-tests PASS. Full-workspace suites not
repeated; no CI/preflight claim. Retained failure stays on disk.

Prospective next measurement: one release/locked matched10000-file Init-v2 pair,
existing cold-content contract, fresh DB creation/checkpoint/close in product
clock, complete performance<=15s, separate mandatory proof<=9.5s, allocation
<=matched final reference. Old FAIL receipts unchanged. Other sizes/histories
unrun at this treatment; all-seven goal remains active.


## Matched10000-file result atb32c2a8eb

Product baseline1646280917/candidate2450084791ns, ratio1.488254383,
timeFAIL. Exact10*current=24500847910 >11*baseline=18109090087.
Roots match; sampled independent proof, cold-content-page attestation and cleanup
PASS. Final allocation312508416/307597312B passes this matched allocation gate.
Performance envelopes2648136875/5121216333ns; verifier615745708/978408792ns,
all under15s/9.5s. No unchanged identity resample. Earlier failed rows retained.

Current SQL13677 statements/2580946 executedVM,789 transactions,136 write
commits, cumulativecommit1226820153ns. C2read_packs1/4911B, payload_reads0,
reserve23,publish110. Previous retained10000 mechanism read126603056B; this
new treatment removes logical-only dependency acquisition. Current namespace
byte/identity proof matches its reference. Do not use cross-window timing as a
controlled speed delta or claim every formerly acquired byte was unnecessary.
Nested SQL/commit/publication/parallel spans overlap, and physical sync syscall
counts remain unavailable.

Raw receipts: benchmark-results/fs-bench-pro/issue302-logical10000-{baseline,
candidate}-treatment1; comparison JSON issue302-logical10000-comparison-treatment1.
Commands: python3 core/benchmark/fs-bench-pro/runner.py run --case
phase7-sqlite-init-10000-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-output>, then --arm candidate
with independent output. Read benchmark_agent_report.md before each command.
Source/compilation/dependency/harness/workload seals and cache numbers in receipts.

ProductionLOC codecommit137502->137505(+3), reference65417/core72085->72088,
active28041->28044/inactive44044, old191/new7631->7634/rest64263.
Exact snapshot counter/method recorded in codecommit; evidence-only followup+0.
No push/PR/merge. All-seven goal active; other treatment sizes/history NOT_RUN.
Next: bounded durable publication boundary composition, cold-preconditioning
cause diagnostic and actual history driver/proof/budget binding. This row does
not establish Phase4.5+10% parity.
