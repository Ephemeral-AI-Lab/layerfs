# G5-0 parallel audit synthesis

Date: 2026-08-23

The correctness/trust, performance/resource, and evidence/custody lanes independently read the H11 source and raw v1/v2 authority. All three agree that v2 is diagnostic only and that v3 is the next unused attempt.

Accepted recommendations are frozen in [PREREGISTRATION-v3.md](PREREGISTRATION-v3.md): separate whole-harness RAII Q, 64-byte frozen reachability-entry rule, authenticated stored/reachable reads, exact historical tuples, split reopen timers with incomplete SQL labeled honestly, standalone independent recomputation, zero-row dry-run, retained lock descriptor plus inode/token attestation, and full fsync/manifest closure.

Rejected recommendations: changing the LayerFS algorithm, schema/profile/receipt, reusing product operation Q for cross-operation ownership, claiming the unused operation log as authority, calling RSS a substitute for Q, or manufacturing complete SQL counters from the old 3-query/3-row/8-BLOB subset.

The lanes also identified one issue beyond the prior final-Q summary: H11 storage/reachability counting read canonical blobs directly. V3 routes every counted object through requested-ObjectId validation before classification. Unreachable-table rows remain a diagnostic all-row authentication/counting pass, not a new GC or product API.

