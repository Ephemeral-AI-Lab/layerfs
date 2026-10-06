# Window execution fix: retained failures and limits

> **Status:** Archived; retained for historical evidence only.

- `02-regression-before-build`: exit101, no bodies. The new test’s integer range was ambiguous for `to_be_bytes` (E0689); explicitly typed u64, then `03` builds. No product fault or timeout.
- `04-regression-before`: expected exit101 at old source: a200-entry window paid404 prepare/checkouts versus the declared at-most16 regression bound. Later new statements tests pass. No failing performance arm was replayed.
- `06-batching-and-plans`: new3 batching bodies pass; existing plan assertion fails on `SCAN 32 CONSTANT ROWS`. It prohibited every SCAN, including the bounded VALUES input. The updated assertion permits only those two named constant-input scans in the two root statements, requires indexed target SEARCH, and rejects every stored-population scan. Final actual32-root cost is5 statements/2176 VM/31 constant-input steps at both2000 and20000 rows,0 sorts/autoindex/reprepares. Every other acquisition query retains its earlier constraints.

No command reached its explicit wall ceiling. Original benchmark speed FAILs and A3’s Linux whole-Persistence/environment failures retain their old receipts. Linux global Store runtime is unsupported; this checkpoint uses owning macOS provider proofs and Linux compilation/Clippy, without retargeting the Store. Capacity and real uncertain/quarantined Init still are not induced.

Diagnostics are count attribution, not performance arms. Their cache is uncontrolled, instrumentation overhead is included, and after-durable overlapped a readonly fmt check. The statement wall metric is now prepare+execution/counters+lease return; complete operation/transaction clocks retain surrounding work. No diagnostic timing/RSS claim is eligible. New qualified candidate samples remain pending at this source checkpoint.
