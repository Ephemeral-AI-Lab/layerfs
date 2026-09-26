# #241 full functional position qualification after host fixes

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This selection is frozen before its first mounted run.

Select the existing [264-case manifest](../../position-manifest-v1.tsv),
SHA-256 `e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`:
66 positions each on 1 MiB, 10 MiB, 100 MiB, and capped 500 MiB pristine
inputs. Case IDs, offsets, payload, same-FD range request, public SDK
Exec→Commit route, old/new/pristine checks, mounted windows, full-byte C1
oracle, status counters, and Sandbox cleanup remain as frozen. One size uses
one independent writable byte copy of its own validated, closed v3 master;
every position gets a fresh sibling Branch and mounted Sandbox. Use one
construction worker. Do not resample a position, select a better attempt,
or rewrite the historical 243 PASS / 1 FAIL / 20 NOT_RUN or later diagnostic
receipts.

Before running, freeze one clean source commit/tree, locked release
`benchmark_init` binary and four exact preparation keys, atomically published
master receipt/Store/history hashes, source fixture hashes, test binary,
immutable Linux daemon/tool image, manifest hash, telemetry run identities,
and four fresh output paths. The current-source masters must be prepared
once outside the position test using the SDK Init route and reused by
independent byte copy. Retain each size's first complete selection. A failure
stops later positions of that size with explicit NOT_RUN receipts; run the
other declared sizes once and retain all outcomes. On a failure known before
Sandbox deletion, collect the bounded daemon/container diagnostics. The
snapshot of Docker inspect is limited to runtime State so control keys do
not enter published evidence.

This is functional qualification, not a latency or cold-cache campaign.
Cache state is uncontrolled; no operation wall is a performance PASS. The
Phase 3 position gate passes only if all 264 cases, all stated byte/history/
mounted checks, and all Sandbox cleanup checks pass at this single identity.
The four registered release Edit→Commit samples and separate verifiers remain
independent subsequent gates.
