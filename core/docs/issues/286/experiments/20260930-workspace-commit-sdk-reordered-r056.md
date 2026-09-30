# #286 r056: public SDK retained reordered-copy PASS

**Functional/command/full-pin/separate canonical proof/checked cleanup PASS**. Complete functional command19,204,189,375 ns <60 s; final SDK Exec16,710,334 ns and known Commit31,669,167 ns. All4099 expected write callbacks (4097 prelude +two8 KiB swapped ranges), exact parented new Commit, full retained dirty-G1 pin10,485,760 bytes at31 KiB maximum reads, checked release/unmount/Sandbox deletion PASS. Separate full canonical G1/new2-path/10 MiB trees and prelude-parent/new-parent checks PASS in100,304,875 ns <9 s.

Measured sourcec2d2634b9. [Compact receipts](20260930-workspace-commit-sdk-reordered-r056-receipts.json) pin exact release SDKa60c0b44..., verifierabfbe9f7..., daemon5d82fadd..., image97d1df65..., source/tree/seals, master copy/hash method and raw manifests. One construction worker; image and closed master reused. Prelude Exec8,342,068,334 ns and all construction/pin/Commit/read/cleanup work were inside the complete command. Public ordinary POSIX copy semantics and retained/live isolation proved; no internal Base-normalization counter claim. Native origin controls fromr053/r054 are explicitly reused for that scope. Cache INELIGIBLE, performance_claim=false; this functional verifier is not a speed arm.

Commands:

```text
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-commit-sdk-reordered-base-copy-8kib-v1 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-sdk-reordered-r056
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-commit-sdk-reordered-r056 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-sdk-reordered-proof-r056
```

No unchanged successful current/earlier case was rerun. The original failed stopping fixture remainsr055FAIL; next is its corrected newline handshake. One historical proof JSON retains the initialized reason string despite statusPASS and a completed child verifier; this reporting defect is corrected prospectively only, and its immutable raw evidence remains unchanged. Production LOC reference65,417/Core70,219/combined135,636, delta0.
