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
