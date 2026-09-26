# #241 position selection after Sandbox host-port allocation correction

> **Status:** Prospective selection, frozen before its first mounted case.
> Keep [v1](../phase3-postfix-v1/REPORT.md) at 0 PASS / 4 FAIL / 260 NOT_RUN
> and [v2](../phase3-postfix-v2/REPORT.md) at 227 PASS / 1 FAIL / 36 NOT_RUN.
> Neither history is relabelled or repeated at its source identity.

Run the same 264-case
[position manifest](../../position-manifest-v1.tsv), SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`:
66 cases each at 1 MiB, 10 MiB, 100 MiB and capped 500 MiB. The prospective
product correction lets Docker allocate a loopback host port, reads the
assigned mapping, and makes the Sandbox route available only after readiness.
Do not retry a failed Sandbox create or change the selected cases, source
fixture, one-construction-worker setting, cache policy, or timeouts.

Before execution, record the new clean source commit/tree, locked release
test and SDK Init binary hashes, immutable Linux image ID, exact sealed
master keys and hashes, fixture hashes, telemetry identities, and four fresh
output paths in a new append-only plan. Reuse a closed master on an exact
fixture/Init-binary/lockfile key match and record its original producer
commit; otherwise prepare it once. Each size uses an independent writable
byte copy of its master, then one Branch and fresh Sandboxes per position.

A size stops after its first failure, records remaining positions `NOT_RUN`,
and retains failed Sandbox diagnostics and all cleanup outcomes. Continue
the other declared sizes once. Phase 3 passes only with **264 PASS, zero
FAIL/NOT_RUN**, exact bytes/history/mounted-view checks and confirmed Sandbox
deletion. This functional selection has uncontrolled cache state and makes no
latency or cold-cache admission claim. The separate release Edit→Commit cases
and identity-matched independent verifiers remain subsequent gates.
