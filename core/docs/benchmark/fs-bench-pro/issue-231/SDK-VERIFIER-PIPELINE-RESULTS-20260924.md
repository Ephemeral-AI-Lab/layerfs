# #231: verifier-overlap attempt

> **Status: failed.** Preserve the one-shot receipt; do not repeat it or
> raise its 9.5-second bound after the miss. This verifier-only candidate
> overlaps full namespace traversal with four existing file workers and
> retains every path, metadata, byte-count and SHA-256 check.

The prospective [contract](SDK-VERIFIER-PIPELINE-20260924.md) was measured
on clean source `c3c2372183c731f1fcb211ac1fd7288a8fff5734` (tree
`20dc9a26264619f04c61d784287c1c2accc04ea2`), product seal
`cf61d25cdf22c6ec403b7305cce32cc3c60be3b387d99619246fe8d827654b51`,
harness seal `9cb11bf11a65a979c32af6e05eaa9711c6d39e5cded325690dbfde88cc282499`.
Locked release binaries were built or reused by exact seal; the changed
build took 0.389 s. The fixed seed-1 100,000-file/500-MB SDK case had a
fresh Store and History. It made one public call; no arm was repeated.

| Observation | Result |
| --- | ---: |
| Public SDK Init | 5.747495458 s, confirmed root |
| Complete performance command | 5.767703666 s, PASS under 15 s |
| Full separate verifier | **TIMEOUT at 9.506694 s**, fixed budget 9.5 s |
| Verifier traversal | 101,001 paths and 100,000 discovered files, then ongoing file checks |
| Cleanup | PASS |
| Receipt integrity | PASS from read-only `runner.py verify --run` |
| Functional / performance | FAIL / INELIGIBLE |

The verifier's partial stderr shows file workers were still reading and
hashing at the kill. It emitted no complete oracle JSON, so this is **not**
a 100k readback PASS. A separate, labeled Store-only profile against the
earlier retained Store measured 1.130 s through manifest/traversal, then
timed out at its own 9.8-second diagnostic watchdog during file checks.
That profile deliberately skipped C5 authentication because the earlier
ephemeral cursor key was unavailable; it is not an alternate full verifier.
Its exact instrument diff and raw output are retained with the
[one-shot receipt](evidence/verifier-pipeline-20260924/receipt.json) and
[hash inventory](evidence/verifier-pipeline-20260924/SHA256SUMS.json).

The overlap alone did not create enough margin below 10 s. The source
still calls public `read_all` once per file, making roughly 100,000
authenticated root demands. The next distinct treatment will batch those
root demands and preserve the full logical-byte oracle, while removing
this unproven overlap code. This attempt remains a recorded failure;
no slower/faster conclusion is inferred from different cache states.
