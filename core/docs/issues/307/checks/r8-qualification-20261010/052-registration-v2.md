# R8 prospective registration after preparation defect correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

[Registration v2](../../../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v2.json)
freezes source3654bebc26fa422c5b956b057f0ff294aff6c1b9 before one new full-proof
invocation. Original registrationv1/commit3748e37ce and FAIL047 remain unchanged.
The full-byte oracle was never invoked in047: startup refused the backing mode
before Ready or Mount. [Diagnosis049](049-startup-diagnosis.md) and
[read-only metadata051](051-original-copy-metadata/stdout.txt) identify0644/root
on the original own copy; the product's existing protected-file check was correct.

New preparation050 makes an independent byte copy with umask077 and records exact
0600/root ownership and sealed digest before startup. The original failed volume
and container remain retained. No failed operation is replayed. Captured root,
Store digest, workload, comparator, configured uid/gid, product binaries, cache
profile, budgets and verdict vocabulary are identical to v1. Only controller
preparation and its source/harness identity change. Product tree9a760... is unchanged.

The full100s/inner85s exception and the four9s/inner8s precondition stops remain
exactly as registered. All90 functional and282 timing IDs remain visible; timing
still has zero authorized attempts because G01-G09 prerequisites are unmet.
V2 carries all inherited owner questions. No full scope, cold, timing or release
claim follows from preparation or tool tests. The declared default CLI/SDK socket
continues to match receipt046. No alternative endpoint or third-party change.
