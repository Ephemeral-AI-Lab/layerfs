# #286 r049: v3 fast retained Commit controls and separate proof

The **clean control completed under its 15 s wall**, with a known final `UpToDate` head, 4,097 expected writes, the same live G1 lease held across both Commits, checked lease release and cleanup. Its independent complete canonical old/new proof ran separately once and passed in 0.104 s. The full pinned-byte oracle is explicitly **SKIPPED** in this fast row and remains a distinct functional selection. Numeric latency is **INELIGIBLE** because the whole Commit cache domain is unproved.

The one-edit case **FAILS before sandbox creation**: its generated sandbox label exceeded the product's name bound, yielding definite `InvalidInput` with `sandbox: None`. Its outer invocation took 0.123 s and reached no mounted Workspace, Commit or pin. No resource was admitted, as confirmed by the typed CreateError and an empty Docker owner inventory. The original fast receipt's cleanup field is `FAIL` because it lacked a final control receipt; this report does not change that saved field or invent a checked unmount for a nonexistent resource. Its separate canonical proof correctly remained NOT_RUN.

Measured clean source `9bdc11dc1` (full source/tree/seals and release artifact hashes in the [compact receipts](20260930-workspace-commit-fast-r049-receipts.json)). The current release daemon image was reused by exact daemon/writer/Dockerfile identity with 0 image-build wall. Both cases cloned the closed #271 master using independent writable byte copies, with pre/post-copy hash checks. Prelude construction stayed inside each external command; there was one attempt per case and one construction worker. Earlier-family evidence was reused; no Family 1/2/3 benchmark ran.

| Case / public route | End-to-end ns / bound | Final SDK Exec ns | Final SDK Commit ns | Prelude SDK Exec / Commit ns | Separate canonical verifier ns / 9 s | Pin custody / full bytes | Cleanup | Result |
| --- | ---: | ---: | ---: | --- | ---: | --- | --- | --- |
| `workspace-commit-clean-retained-writes-4097-v3` / SDK Mount, Exec, pin, two Commits, checked release | 13,956,690,292 / 15 s | 1,506,500 | 5,883,708 (`UpToDate`) | 8,270,944,542 / 578,922,667 | 103,848,625 | PASS / SKIPPED | PASS | complete fast diagnostic; canonical control PASS; full pin oracle pending; numeric INELIGIBLE |
| `workspace-commit-one-edit-retained-writes-4097-v3` / create refused before Mount | 123,117,792 / 15 s | UNAVAILABLE | UNAVAILABLE | UNAVAILABLE | NOT_RUN | NOT_RUN | no resource admitted; raw field FAIL | FAIL (`InvalidInput` label bound) |

The clean verifier reopened the retained Store/history without mutation, checked both complete 2-path/10,485,760-byte trees and modes against the independently derived 4,097-disperse manifest, the prelude Commit's parent against the sealed master Commit, and the final unchanged head. Performance receipts were manifest-checked before and after proof. The lease observation recorded generation 1 with one held lease after both Commits; it is a custody proof, not a complete pin-byte proof. Complete Host Server/daemon phase totals remain unavailable; nested LFT1 observations are retained and not summed into a headline.

Commands, each with a fresh output path:

```text
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit --out benchmark-results/fs-bench-pro/issue286-workspace-commit-fast-r049
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-commit-fast-r049 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-proof-r049
```

Performance manifest SHA-256 `11afb92bcdca8534a424280ac1e8df26fb9075e76361bcc135458fe00c269bca`; separate-proof manifest `2b46081b108b0f40f8cfbb1ccfbe14ebbdd64915a6df780de48e739400cef9d1`. The compact evidence preserves exact driver/control/proof objects and raw hashes. No performance arm was replayed by `prove`.

**Next:** shorten only the shared benchmark-generated sandbox label to a deterministic bounded digest plus PID, record the zero-owned-resource failure disposition, and take one new-identity attempt for the refused one-edit case only. The successful clean performance/proof receipt stays reused, with unchanged production and verifier scope stated explicitly. Then run the separately registered full pin functional oracle once. Remaining Family 4 lowering/headroom/live/failure/stopping selections stay NOT_RUN; no full-family admission is claimed. This harness/example/report change has production LOC reference 65,417 → 65,417, Core 70,219 → 70,219, combined 135,636 → 135,636 (delta +0).
