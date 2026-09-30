# #286 r053: full64 MiB lowering PASS; occupied fixture shortfall

**Full lowering PASS**, in18,091,738,750 ns <60 s. Stage113,870,334 ns <10 s, one SaveFileV2 with exactly8 replacement bytes, final67,108,662 bytes. Independent complete old/candidate/new/G2 and old/G1 pinned byte comparisons passed, both known Commits matched their expected roots/parents, and checked release/clean close left zero allocation/reservation. Source8b216d3d1; same binary6971b001... as r052, with corrected clone custody. Both prepared masters were reused, not regenerated.

**Occupied2 MiB Stage fixture FAIL before Stage**, in97,458,916 ns, because its filler left more than the declared4 KiB ordinary headroom; six later cases NOT_RUN. Product cleanup UNKNOWN there, externally archived state and owned container/volume removal PASS. The retained oracle cannot count as an occupied-quota proof.

The external helper dropped its temporary two-byte input after writing the active journal, but computed filler length before reclaiming that now-unowned physical payload. `Workspace::own_payload` runs maintain_backing before the next acquisition; that reclaimed input makes the helper's prior occupancy estimate stale. The fix explicitly reclaims released payloads before taking the allocation/reservation baseline and reports the complete baseline/final counters on a refusal. No quota, required headroom, Budget, deadline, product or oracle change. The successful full64 MiB lowering is reused and will not be replayed.

| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |
| --- | ---: | --- | --- | --- |
| workspace-commit-full-lowering-size-64mib-native-v2 | 18091738750 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-stage-headroom-quota-2mib-native-v2 | 97458916 / 60 s | FAIL | UNKNOWN | INELIGIBLE |
| workspace-commit-headroom-quota-4mib-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-live-g1-g2-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-pin-custody-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-known-unknown-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-local-c5-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-reordered-base-copy-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |

Reproduction: `python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit-native --out benchmark-results/fs-bench-pro/issue286-workspace-commit-native-r053`. [Compact receipts](20260930-workspace-commit-native-r053-receipts.json) include every selected status and successful lowering/failing filler stdout/stderr. Native component route, numeric INELIGIBLE and performance_claim=false. No prior family or passing SDK control was rerun.

Next: corrected occupied Stage case, then previously unrun controls, once each. Production LOC reference65,417 / Core70,219 / combined135,636, delta0.
