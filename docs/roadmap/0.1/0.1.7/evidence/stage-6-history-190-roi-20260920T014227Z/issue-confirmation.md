Stride3 confirms the one-helper result: operation **64,870,176,420→59,039,478,665 ns (8.988256355662305% reduction)**; complete command88,637,897,000→82,042,953,250 ns. All53 roots/save decisions/Store bytes match. Stride10 remains a5.58% operation improvement with a642,043,167 ns complete-wall regression; do not hide that distinction.

Candidate sampled verification is5,489,793,375 /18,912,983,625 ns, meeting10/20 s. Baseline stride3 verification22,686,258,458 ns remains TARGET_MISS. Independent review reproduced the raw arithmetic and actual hashes of both binaries and all five Stores, including the separately profiled Store.

**Decision: retain the catalogue prepared-statement reuse and stop this optimization round.** Net production−1 LOC, existing16-entry cache, no new data cache/telemetry/policy. Whole-lane prepare counts and exact SQL CPU remain unmeasured; the authorizer fixture proves actual repeated preparation falls5→1 with fresh results and preserved errors. Full workspace checks are running before publication. Cache/O3/historical timing gaps remain open.

Preflight deferrals and an independent-hash lock refusal remain in evidence. The empty private lock was preserved after both global locks were acquired and no active owner/descriptor was found; its origin is UNKNOWN. No performance sample was repeated and no other owner's process was interrupted.
