# #231: bounded-time full SDK Init verifier

> **Historical failed treatment.** The later owner-approved
> [lightweight sampled verifier](SDK-VERIFIER-LITE-20260924.md) supersedes
> the full-content design for new SDK benchmark receipts.

> Frozen before changing the verifier or its watchdog. Owner direction on
> 2026-09-24 allows a verifier limit above 5 seconds and strictly below
> 10 seconds. This selection uses **9.5 seconds**. All prior five-second
> receipts remain immutable and keep their original PASS/TIMEOUT statuses.

The release-only [four-tier cohort](SDK-RELEASE-FOUR-TIER-RESULTS-20260924.md)
completed the 100,000-file public Init in 5.468 s but its separate full
verifier hit the five-second watchdog. The verifier had already listed all
101,001 paths and was reading/hashing file content when killed. A labeled
read-only Store-only diagnostic against that retained Store measured
**1.130 s to finish manifest loading and traversal**. It skipped C5 History
authentication because the original ephemeral cursor key was not retained;
its 9.8-second watchdog also expired during file checks. This diagnostic
does not prove a full oracle result or an admission time. Its raw receipt and
instrument diff stay under
`benchmark-results/fs-bench-pro/issue231-verifier-profile-20260924-01/`.

The selected treatment starts the existing four file-verification workers
before directory traversal and streams each discovered file job to them
through a standard-library channel. The worker count, Store read API,
SHA-256 check, metadata comparison, manifest cardinality, path uniqueness,
C5 root check and C1/C2 reopened reads remain the same. Traversal and
file checking can overlap; the verifier must still wait for all workers
and report PASS only after every path and byte is checked. A worker error,
sender error or traversal error fails the verifier. The queue may hold up
to the whole discovered file set, as the current `VecDeque` already does;
this is an external verifier, not bounded product construction. It may be
bounded separately if verifier memory becomes a gate.

Use locked release binaries from the sole SDK runner. Give the changed
verifier an append-only v4 receipt/schema and a fresh case output at the
new source/build/harness identity. Keep the 15-second complete Init command
and 30-second build limits. The independent verifier watchdog is 9.5 s
for every case; it is never inside the public Init timer. Do not sample,
omit files, skip SHA-256, change worker counts, reuse an earlier PASS,
or extend 9.5 s after a miss. Run the 100k case once at the final source
identity, then the smaller cases once if the treatment succeeds, to prove
the complete four-case verifier route. Retain every failure and incomplete
attempt.

This is a **functional verification** treatment. Source cache remains
`source-cache-uncontrolled-v1`, and no numeric SDK performance target is
frozen; a verifier PASS cannot by itself clear #231's four eligible
performance PASS requirement or authorize a main merge/issue closure.
