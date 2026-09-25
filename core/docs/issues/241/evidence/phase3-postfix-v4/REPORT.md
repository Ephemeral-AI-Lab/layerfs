# #241 Phase 3 position selection after baseline Service session correction

> **Status:** Dated planning checkpoint; functional evidence, not release or latency admission.

The [prospective v4 contract](CONTRACT.md) and [plan](plan.json), SHA-256 `db2996e758637b00f9c2f1b5c31a3264ee96b03a50e81a7c128b57b31de519c9`, were frozen before the first mounted case. Source was `8aaf623835cf1384fbf5558e868b8d1b945830bd`, tree `f38980b37d2aaa3fc2b92c4b096ce1df7bce8f76`; the unchanged 264-position manifest SHA-256 was `e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`. The locked release position binary SHA-256 was `78f3c9efe266e2b25beef94473dd2cfc36bdc2d4103592c8df76a36c9588fdb2`, the release SDK Init binary SHA-256 was `1f1f5670755445ed181aee873872b91c73a01773d683b3334d5597528d38ce8e`, and the immutable Linux image was `sha256:c7a8378bb5fbdde23dabe65b77e7f8492696aa2a040544a37d81d6e95ad64858`. The [image receipt](image.json), [master inventory](masters.json) and [four master manifests](master-manifests/) retain the remaining input and product seals.

The four closed masters were prepared once with that exact SDK Init binary from validated copies of the original fixture bytes. Each size took one independent writable Store/history byte copy. Cache state was uncontrolled, and `LAYERFS_CONSTRUCTION_WORKERS=1` was exported for every run. Each size ran its 66 registered positions once, in order, with fresh Sandboxes. No case was retried or replaced.

| Size | PASS | FAIL | NOT_RUN | Complete 66-case command wall | Cleanup | Host acceptor |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 MiB | 66 | 0 | 0 | 226.239 s | PASS | 264 admitted/reaped; peak live 2; 0 drops |
| 10 MiB | 66 | 0 | 0 | 234.478 s | PASS | 264 admitted/reaped; peak live 2; 0 drops |
| 100 MiB | 66 | 0 | 0 | 290.493 s | PASS | 264 admitted/reaped; peak live 2; 0 drops |
| Capped 500 MiB | 66 | 0 | 0 | 514.502 s | PASS | 264 admitted/reaped; peak live 2; 0 drops |

**Prospective Phase 3 functional selection: 264 PASS / 0 FAIL / 0 NOT_RUN.** All four test commands exited 0. Every size's summary says `attempted=66`, `cleanup=true`, `sandbox_list=Ok([])`, and `failures=[]`; its run receipt says PASS. An independent receipt audit matched all 264 case IDs to the frozen manifest, found one terminal PASS per case, confirmed exit-0 baseline Exec and the four edit/fresh/old/source Sandbox deletion and absence paths for every case. The test checks the public SDK Exec/FUSE splice route, callback counts, exact stored bytes and digest, retained old Commit, Branch head, mounted read windows and fresh-mount readback. The host acceptor summaries each show capacity 4 and terminal error `None`. The complete [raw receipts and output](raw/) have a [SHA-256 inventory](RAW-SHA256.txt); private writable Store/history copies remain at the local paths pinned by the plan.

The frozen [v3 failure](../phase3-postfix-v3/REPORT.md) remains **229 PASS / 1 FAIL / 34 NOT_RUN** at its original source; none of those receipts was relabelled. V4 is a new-source functional result, not a latency result or proof that every future Docker connection will succeed. The original v3 five-second span cannot be separated retrospectively into TCP and Noise. The four registered release Edit→Commit cases, exact portable-metadata/canonical expectations for their independent verifier, and release telemetry admission remain separate open gates. The #232 parent still owns its 56-case gate.
