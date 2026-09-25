# #232 repeated-128 independent verifier repair

**Status: prospective, before a second verifier invocation.** The single
changed-product Exec→ioctl→Commit attempt at source `7ee723ca6` completed its
public route and cleanup, but its first independent verifier failed before
opening Store/history because `verify_edit` recognized only the historical
`-complexity-v1` scenario suffix. The failed verifier receipt is retained.

Register exactly `repeated-128-progress-proof-v1` with the existing complexity
operation contract in the verifier. Do not change product source, payload,
result bytes, canonical-root rules, verifier coverage or any timer/budget.
Build one new locked release verifier and pin its exact hash and source. Run it
once against the **same** copied Store/history and performance `case.txt`,
without rerunning Exec, ioctl, Commit or cleanup. Retain raw stdout/stderr,
exit code, wall time and a fresh verifier receipt next to the original failed
receipt. The verifier must finish under 10 seconds and check full result bytes,
canonical representation, mode/mtime, retained old Commit and reopened Branch.

The original performance receipt remains `FAIL` because its first verifier
failed. A passing second independent verifier may support a separately labelled
changed-product **functional proof** only if the source/product, case, image,
Store/history and head identities match. It cannot produce a cold-latency PASS
or replace the first receipt. Report any second failure plainly; do not retry it.
