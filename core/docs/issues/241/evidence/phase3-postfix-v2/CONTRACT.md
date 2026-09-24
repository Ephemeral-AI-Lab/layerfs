# #241 position selection after daemon session-custody correction

> **Status:** Prospective selection, frozen before its first mounted case.
> The preceding [v1 selection](../phase3-postfix-v1/REPORT.md) remains
> 0 PASS / 4 FAIL / 260 NOT_RUN and is never rewritten or resampled.

Use the unchanged 264-case
[position manifest](../../position-manifest-v1.tsv), SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`:
66 positions each at 1 MiB, 10 MiB, 100 MiB and capped 500 MiB. The
prospective product change reduces retained upstream daemon sessions while
preserving request IDs, the host session limit, deadlines and the actual
Exec→Commit route. Its new clean source commit/tree, locked release binary
hashes, immutable daemon/tool image, prepared master keys and hashes, fixture
hashes, telemetry identities and four fresh output paths must be written to a
new append-only plan before execution. Do not promote a v1 result to v2.

Prepare each size's closed master once via release SDK Init at the new source
identity. The functional test makes one independent writable byte copy per
size. Keep one construction worker. Each position uses its own Branch and
Sandboxes. A size stops after its first failure with explicit `NOT_RUN`
receipts; run the other declared sizes once. Retain failures, fresh-mount
daemon diagnostics, cleanup outcomes and all passing receipts. A diagnostic
for socket counts must name its own purpose and cannot replace a position
case or a release performance sample.

The position gate requires **264 PASS, zero FAIL/NOT_RUN, exact byte/history/
mounted checks and successful Sandbox deletion**. This is functional
evidence with uncontrolled cache state, not latency or cold-cache admission.
The separate four-case release Edit→Commit selection and identity-matched
verifier remain open gates.
