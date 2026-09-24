# #241 10 MiB position Exec liveness diagnostic v2

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Frozen before the first attempt on 2026-09-25.

This is one prospective **diagnostic-only** selection of the existing 66
frozen 10 MiB positions. The manifest and edit semantics stay unchanged.
The source is one fresh independent writable byte copy of the qualified,
closed 10 MiB v3 master. Each position retains its case receipt and first
attempt; the harness stops on the first failure and records all remaining
positions as NOT_RUN. Historical position evidence is never replaced or
pooled with this diagnostic. A successful selection is still not a Phase 3
qualification or a performance admission result.

The added observation records the elapsed host wall and typed result of
each baseline `printf baseline > .position-baseline` SDK Exec before the
existing Commit and range EDIT sequence. An explicit numeric diagnostic run
identity enables existing host and daemon LFT1 telemetry. On a failure,
read-only Docker inspect/top/logs are captured before Sandbox deletion,
followed by a fixed six-second observation window and a second top/logs
snapshot. There is no retry of Exec, unmount, EDIT, Commit or a failed case.
The observation window is outside any operation result and does not convert
an uncertain outcome to success. The 30-second Exec deadline, five-second
native progress rule, FUSE budget, one construction worker, and cache policy
are unchanged.

Before execution, record a clean source commit/tree, host test binary hash,
immutable image ID and compilation seal, manifest hash, master receipt and
independent-copy hashes, and a fresh output path. Cache state is uncontrolled
and `admission_eligible=false`; all elapsed values are diagnostic context.
The purpose is to locate a recurrence of the small baseline Exec failure,
not to re-sample the historical failed row for a better result. If no
failure occurs, report the attempt as inconclusive for the original cause.
