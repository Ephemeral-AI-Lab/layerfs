# Issue 271: extended 4,097-write count diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The four-hop source `933b3457c916519f458137e4c4f19662c8e9228e` has one
retained public 4,097 gate FAIL at the fixed 25 s complete-command limit. Its
daemon later reported a 26.117643 s Exec, but the host received no driver
receipt, callback count, Commit root or verifier result. The gate is not retried
or relabeled. The user requested a separate extended run to see where cost
grows; this diagnostic may use a **60 s complete-command timeout** solely to
collect that cause evidence. It is never eligible for the 25 s gate or a
cache-qualified latency PASS, even if the command finishes.

Add one named `diagnostic4097` selection to the existing public runner. Use the
same closed, independently verified 8,194-byte master, independent writable
byte-copy clone, sealed release SDK driver and verifier, static writer and
four-hop product binary. One Mount → one generic one-process/one-fd Exec → one
explicit Commit issues exactly 4,097 one-byte positional writes at even
offsets. Keep the one construction worker, uncontrolled cache contract and
full independent old/new-head oracle. Change no product algorithm, worker,
cache treatment, writer payload or verifier bound.

Enable the existing FUSE/ledger count instrument every **512 accepted WRITEs**,
at 512, 1,024, ..., 4,096. The 4,097th write has no ledger checkpoint; report
that gap explicitly. Capture cumulative 4 KiB ledger reads/writes, metadata
reads, child/Local/sponsor edges when emitted, page counts, writer quarter
progress, callback count, SDK Exec/Commit/complete wall, full verifier and
cleanup. Derive each 512-write increment and per-write cost from the retained
checkpoints; compare the first 512 writes with the prior 100/512 source rows
only with the different instrumentation/cache status stated. Missing samples
remain missing. A completed long run diagnoses finite-range scaling; three
count points alone cannot prove an asymptotic complexity class.

The 60 s limit and under-10 s independent verifier are fixed before this sole
extended diagnostic attempt. Keep fresh append-only output, source/product/
harness/image/workload hashes, the separate original gate FAIL and all cleanup
outcomes. A timed-out diagnostic remains FAIL; no same-identity retry follows.
