# #286 r050: one-edit retained fast control

**Completed fast control and separate canonical proof PASS.** The previously refused one-edit case had one new-identity attempt after the bounded-label fix. The complete command was **11,825,536,125 ns <15 s**, final SDK Exec **4,712,000 ns**, final Commit **17,603,083 ns** with a known new head parented by the prelude Commit, 4,098 expected writes, held G1 lease/release and checked unmount/deletion PASS. The complete canonical old/new proof ran separately once in **99,190,083 ns <9 s** with no performance replay. Full pin bytes were SKIPPED in the fast row; numeric latency remains INELIGIBLE.

Measured clean source `9d55fa41f7abe071f6284bbaadde5beed43f5824`. The [compact performance/proof evidence](20260930-workspace-commit-one-edit-r050-receipts.json) pins exact source/tree/seals and release artifacts: SDK `9e68d6f53ac07980ae3afad480f7e8d1c558b5c49cbf4880bf180e0739502e15`, independent verifier `abfbe9f75efa3c0d397a81c4e6eeaf5a9f317737acd068e405eee4b543a60985`, daemon `5d82fadd7e0c5a4c7c053fb577a32f57d141a438c22012cc91b23098f52ab51b`. The exact daemon image was reused with build wall0, and the closed master was independently byte-copied and hash-checked before launch. One construction worker. Prelude Exec **8,114,831,709 ns** and prelude Commit **521,168,083 ns** were inside the complete command.

The separately launched verifier checked both complete 2-path/10,485,760-byte trees, modes, the prelude parent's identity and final new Commit parentage. Both performance and verifier scopes have checked cleanup. The SDK custody observation confirms the same generation-1 lease held after both Commits, but does not replace the full pinned-byte oracle. The current command/canonical control result is combined with the unchanged successful [r049 clean control](20260930-workspace-commit-fast-r049.md): only the shared benchmark label/zero-resource failure classification changed, with no production or independent-verifier change. This exact relevant-scope reuse follows the owner's fast-path direction; the clean control and earlier Family 1/2/3 benchmarks were not replayed.

Commands:

```text
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-commit-one-edit-retained-writes-4097-v3 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-one-edit-r050
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-commit-one-edit-r050 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-one-edit-proof-r050
```

The raw performance/proof manifests are respectively `0cc0611590647d6f917b58f34638e80e66500da905b2d58751fc5925c2e4d19c` / `6cd7ce3ae53cff563edfdf867ab7a47f8fddb2e4938990a60dad462fe81fd0bd`. There is one selected attempt, no failed/unrun selected case, verification separate and no numeric speed claim. Remaining Family 4 selections stay NOT_RUN. Next is the distinct 60 s full pin-byte functional oracle, then lowering/headroom/live/failure/stopping. Production LOC remains reference65,417, Core70,219, combined135,636 (delta+0).
