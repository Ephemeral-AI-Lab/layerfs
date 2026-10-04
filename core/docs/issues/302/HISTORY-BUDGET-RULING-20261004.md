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
