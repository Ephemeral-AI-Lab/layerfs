# #241 Phase 1B finish diagnostic: retained decision

**Outcome:** functional and independent-verifier PASS for both declared routes;
latency **INELIGIBLE** for a cold claim. The observed 500 MiB versus 1 MiB
`EditFile` `service.finish` increase is 31.308500 ms, of which
`storage.finish.drain` accounts for 30.495542 ms (97.4%). This locates the
growing span at batch drain. The two samples do not distinguish SQLite
presence lookups, validation, physical reads or writes *within* drain. No
safe, narrow product optimization is justified by these counts. Phase 1B
closes with that explicit limit; the unchanged four v4 arms are not resampled.

## Identity and method

The [contract](CONTRACT.md), [macOS counter amendment](HOST_IO_AMENDMENT.md)
and [pre-run seal](PRE_RUN.json) preceded collection. Both diagnostics executed
source `09a2084175d2f9c6ad2afc18b49a418743ef6627`, product seal
`a419adeba1c3b7c0cf3ea8fb8cab8d8725f9e90316ddd8c9d938040aec7edbfa`,
harness seal `aa6f92edc1124fbbfc9b0f73d489947e3c623d4cc5322c5ee8294b4196d3d55b`
and image `sha256:078d691e57d6b02c9f7033fea0a1f857536559b444031a9f610cb3ddf7f795e9`.
The v4 registry digest is
`9decc295b688083db7b1f6899ebbc315eeeddfa28d0fafe68537507d12394fbb`.
Locked release binary hashes, prepared-master hashes, commands and fresh
output paths are in `PRE_RUN.json`; each run used an independent writable byte
copy. `LAYERFS_CONSTRUCTION_WORKERS=1` and `LAYERFS_FINISH_DIAGNOSTIC=1` were
set in both runs. The first parent of the evidence commit is the sampled
source; this appended report does not mutate either raw receipt.

Each row is one public SDK Exec, a checked mounted 4 KiB inline range ioctl,
and one public SDK Commit. The tool observed two STATE callbacks, one EDIT
callback, 4,096 accepted literal bytes and zero shifted suffix bytes.
The complete command includes process and container lifecycle and cleanup;
the independent verifier is outside the operation timer. LFT1 alone supplied
operation and substep elapsed time. [derive.py](derive.py) checks the frozen
identities, route counts, child cardinality, budgets, verifier, cleanup and
cache classification and reproduces [derived.json](derived.json) from the
[1 MiB](attempt-01/1mib/receipt.json) and
[capped 500 MiB](attempt-01/500mib/receipt.json) raw receipts. Both attempt
folders retain LFT1, daemon output, driver, verifier, build/image manifest,
post-run observations and per-file `SHA256SUMS`. The Store/history database
bytes and private cursor key remain at the ignored local run paths stated in
`POST_RUN.json`; only their sizes and hashes were copied here.

## One sample per declared shape

| Pristine file | Raw Exec→Commit | `EditFile` finish | Drain | Owner | Complete command | Verifier | Cache |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1 MiB | 42.870500 ms | 1.358292 ms | 0.532458 ms | 0.825000 ms | 2.633530 s PASS | 0.604983 s PASS | INELIGIBLE |
| capped 500 MiB | 80.625459 ms | 32.666792 ms | 31.028000 ms | 1.637500 ms | 0.910685 s PASS | 3.073079 s PASS | INELIGIBLE |
| 500−1 MiB difference | 37.754959 ms | **31.308500 ms** | **30.495542 ms** | 0.812500 ms | diagnostic only | — | — |

The complete commands met 15 s and separate, identity-matched verifiers met
10 s. Both verifiers checked full result bytes and digest, canonical root,
published metadata, historical old Commit and Branch behavior. Unmount and
public Sandbox Delete passed; the receipts report no failed selection.
Neither raw Exec→Commit number is a latency PASS: the Linux FUSE backing
domain could serve Commit from Edit's resident pages. The separate macOS Store
check saw zero resident pages before each timed run. No historical v4 receipt
or gate was relabelled. The legacy G2 numeric target field in these runner
receipts is not a matched target for this diagnostic.

The LFT1 finish immediate children account for all but 834 ns and 1,292 ns
of their parent spans at 1 and 500 MiB. Owner children account for all but
3,959 ns and 3,543 ns. Owner pack seal grew 0.821583 ms; SQLite commit was
19,000/18,125 ns, candidate flush 333/250 ns, pool clone 916/541 ns and
candidate clone 78,833/63,583 ns. Those postcommit clones therefore do not
explain the 30.496 ms drain increase. The `EditFile` Service LFT1 scopes had
one and four resource samples, respectively; CPU samples and sampled RSS do
not cover the scope boundaries and are **not** exact phase CPU or peak RSS.

| `EditFile` finish count | 1 MiB | capped 500 MiB |
| --- | ---: | ---: |
| Inserted/full/reused/prefix objects | 3 / 3 / 0 / 0 | 9 / 9 / 0 / 0 |
| Batch objects / canonical bytes | 3 / 6,507 | 9 / 30,243 |
| Packs / packed bytes | 2 / 6,336 | 2 / 26,309 |
| `objects` INSERT statements / transactions | 2 / 4 | 2 / 4 |
| Pool entries/live bytes; candidate entries/live bytes | 0 / 0; 0 / 688,128 | 0 / 0; 0 / 688,128 |
| Chain objects / encoded bytes | 1 / 20,880 | 1 / 20,896 |
| Pooled pack fetches / bytes | 0 / 0 | 0 / 0 |
| Process-wide physical disk-read delta | 806,912 B | 6,115,328 B |
| Post-run Store SQLite page count | 1,041 | 142,554 |

The physical-read counter spans the **whole host process** from before Exec
through Commit; it is not finish-exclusive. The 4,096-byte SQLite page counts
were observed after both performance and verification and describe the whole
Store, not pages visited by finish. Per-finish page and physical-read counts
are `UNAVAILABLE`, as recorded in each `LFS_FINISH_COUNT` line. The larger
batch and process reads make drain a useful target for a later, separately
frozen causal diagnostic, but do not identify which drain operation can safely
be removed or changed. No timeout, worker, cache or buffer policy changed.

## Disposition

The retained causal boundary is **batch drain**, not postcommit index cloning.
Within drain, the mechanism remains open because no bounded child or
finish-exclusive page/physical-read count separates its SQLite lookup,
validation and write work. Accordingly B3 has no product change and B4 has no
changed-source four-case proof to run. Phase 1C may now screen repeated-piece,
C1 locality and cutoff behavior under a distinct identity. A future drain
optimization requires a new prospective count contract and changed-source
proof; these two cache-ineligible diagnostics cannot serve as a cold speed arm.
