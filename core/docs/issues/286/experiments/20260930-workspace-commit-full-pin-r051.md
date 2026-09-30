# #286 r051: full SDK retained pin-byte functional oracle

**Functional oracle PASS**, separately from performance. The same-Workspace dirty G1 lease remained held across its prelude Commit and the one-edit successor Commit. After both, the public SDK read **all 10,485,760 bytes** in explicitly declared ≤31,744-byte requests and returned SHA-256 `31c45fc43c108fdfe65faa79d3568d4b64a3496cc75f29c2d441f98120bbfff5`, exactly the independent 4,097-disperse oracle. Checked lease release, unmount and Sandbox deletion passed. The complete functional command was **18,956,367,917 ns <60 s**; this is not a 15 s performance row or a speedup. The complete canonical old/new verifier ran separately once in **99,113,500 ns <9 s** and passed, without replaying the functional command.

Measured clean source `9d55fa41f7abe071f6284bbaadde5beed43f5824`, with the same SDK/verifier/daemon and image seals as [r050](20260930-workspace-commit-one-edit-r050.md). [Exact compact evidence](20260930-workspace-commit-full-pin-r051-receipts.json) preserves the original driver/control/proof objects, hashes and identity set. The image was reused with build wall0, the master independently byte-copied/hash-checked, and one construction worker was used. The full pin read is a distinct functional selection under the prospectively frozen [v3 contract](../WORKSPACE-COMMIT-FAST-PATH-V3-20260930.md); it does not replace the earlier failed v2 performance command.

| Functional case | Complete ns / bound | Prelude SDK Exec / Commit ns | Final SDK Exec / Commit ns | Full pin bytes / request ceiling | Separate verifier ns / bound | Custody / bytes / canonical / cleanup |
| --- | ---: | --- | --- | --- | ---: | --- |
| `workspace-commit-full-pin-retained-writes-4097-read-31kib-v1` | 18,956,367,917 / 60 s | 8,204,887,250 /555,992,750 | 5,291,417 /16,502,000 | 10,485,760 /31,744 B | 99,113,500 /9 s | PASS /PASS /PASS /PASS |

This establishes the complete G1 pin oracle at this product source across a later live mutation and known Commit. The earlier clean/one-edit fast control receipts remain individually source-pinned and canonical-verified; reuse of this functional pin coverage is explicit. No 32 KiB or 128 KiB view request is claimed. Full-command numeric cache admission remains INELIGIBLE. No earlier-family benchmark, especially Family 2, reran.

Commands:

```text
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-commit-full-pin-retained-writes-4097-read-31kib-v1 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-full-pin-r051
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-commit-full-pin-r051 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-full-pin-proof-r051
```

Raw performance/proof manifest SHA-256s are `d365bf7eeadaf3138c2508dcd9a6bb721484dcacf3052b923b4d330263293cb6` / `85dfd6f550d5716736d84691b3b1c1011da0fe073acc52e99d9cb5305ad48172`. There was one complete functional invocation, one separately bounded verifier, no retries, and no selected failure. Remaining Family 4 lowering/headroom/live/failure/stopping work is unrun and cannot be called PASS from this receipt. Next is the relevant native functional group at a separately frozen identity, reusing prepared setup and unaffected earlier evidence. Production LOC is unchanged: reference65,417, Core70,219, combined135,636 (delta+0).
