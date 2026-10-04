# Retained-history performance budget ruling, 2026-10-04

> **Status:** Current planning checklist; no release candidate exists.

The owner explicitly answered: **“Restore the original60/170/170-second history
limits.”** This session direction overrides the generic15s/25s performance-command
rule for these three retained-history selections only. It is prospective and
applies to the supported-profile campaign, not historical receipt relabeling.

| Selection | Retained states | Complete performance command bound |
| --- | ---: | ---: |
| history-stride10 | 17 | 60,000,000,000ns |
| history-stride3 | 53 | 170,000,000,000ns |
| history-stride1 | 157 | 170,000,000,000ns |

The separate verifier remains9,500,000,000ns for each row. The owner restored
performance limits, not the original10/20/30s verifier limits. Integer10% time
margin, storage ceilings, corpus/state selection, workers/buffers, cache and
identity requirements remain. Both arms of each supported Disposable or Durable
pair use the same restored bound. Init retains15s/9.5s.

The earlier full-reference count diagnostic remains FAIL_COMMAND_BUDGET at
25,014,771,375ns,15published states and no final root vector/cleanup proof. Do not
rewrite it as completed or within this later limit. No unchanged old arm is
retried or promoted. New profile-aware vehicles require their own prospectively
frozen source/harness/observer/corpus identity and one sample per declared arm.
Registry rule identity: owner-2026-10-04-restored-original-60-170-170-performance-only-v1.
History driver/proof/cold integration remains necessary before admission; a
budget ruling alone does not qualify a row.

## Prospective stride1 extension, owner follow-up 2026-10-04

Owner asks to increase stride1's limit a bit after the frozen Stage1 reference
times out at170,014,270,917ns after143/157states. Set the next stride1 complete
performance envelope to **190s** (+20s, +11.76%), for both reference/candidate
and Disposable/Durable. This applies only to new `history-stride1-v3` case IDs.
Stride10/3 retain60/170s, every separate proof retains9.5s, Init retains15/9.5s.
Storage, workload/state selection, workers, buffers, cache and time-margin gates
remain unchanged. Historical v1/v2 records and case definitions retain170s and
their original failures; old stride1-v2 selection refuses another sample.
The next changed implementation/harness needs new matched v3 evidence.
Registry rule: `owner-2026-10-04-60-170-190-performance-only-v2`.

## Explicit 300-second stride1 budget, owner follow-up 2026-10-04

The owner quotes the reference timeout at190.012s and directs: **“give them
300s budget to finish”**. New `history-stride1-v4` cases therefore use a
300,000,000,000ns complete performance-command limit for both reference and
candidate, in both supported profiles. Stride10/3 remain60/170s. The separate
verification limit remains9.5s; this instruction does not change proof coverage,
storage limits, workers, cache state or the10% comparison margin.

Preserve historical v3/190s failures with their original identities and limits;
v3 selections are retired. The new version receives one prospectively declared
sample per arm, with an unchanged reference product and newly matched harness.
Registry rule: `owner-2026-10-04-60-170-300-performance-only-v3`.


## Approved fast content proof, owner follow-up2026-10-04

After stride1-v4 completes performance in211.235051708s, its existing proof times
out at9.505829958s. The owner explicitly selects **“Use bounded content proof
with a12s limit”**. New case IDs are stride10-v3, stride3-v3, stride1-v5 for both
profiles. Their complete separate proof budget is12,000,000,000ns. Prior case
IDs retain9.5s and original content scope and are retired, never relabeled.
Performance budgets stay60/170/300s; all other gates stay unchanged.

Proof policy `all-state-structure-five-anchor-bounded-content-v1` keeps closed
C2/C5 schema/integrity/census/custody and every selected state's namespace path,
kind and declared size checks. At initial/quarter/midpoint/three-quarter/final
states it compares up to8 whole files<=64KiB and2 symlinks<=4KiB; up to4 larger
chunked files compare16KiB logical ranges at start/middle/end against actual
prepared-source bytes read inside proof. Unselected whole-file sizes are metadata
commitments, not claims that their payload bytes were decoded. Large whole-file
payloads are outside this sample selection. Logical content bytes<=8MiB; actual
physical acquisition during file-root/content phases, including dependencies and
conservatively included mapping metadata,<=32MiB. The calibrated sealed observer
counts returned whole-pack SQL bodies and successful BLOB read bytes in both
arms; missing counters or overflow fails closed. Read-unit size and object/depth
bounds remain product limits. Optional whole-pack audit stays a separate scope.
