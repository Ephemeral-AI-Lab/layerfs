# Family6 SDK r069 —3 passing profiles; dirty-discard cleanup FAIL

Clean measured sourcee2429875d. Four one-shot public SDK commands under15s; numeric cache INELIGIBLE. Full old/new independent proof and cleanup pass for the first3; the original failed-shell v1 is FAIL, not promoted.

| SDK selection | Complete ns /15s | Exec ns | Commit ns | Separate verifier ns /<9s | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| mixed ordinary |3,018,102,791 |77,423,583 |35,059,916 |2,436,158,042 |PASS: full modes/bytes/targets/alias/mtime, cleanup |
| retained G1/G2 |1,004,097,417 |28,052,584 |25,586,917 |37,677,209 |PASS: full old/new, parentage, G1 pin10B, refcount2→1, selected mtimes, cleanup |
| known POSIX refusals |892,824,959 |35,872,334 |21,213,167 |28,218,625 |PASS: no forbidden names, full tree, known Commit, cleanup |
| exit7/no Commit v1 |5,816,837,209 |16,446,041 |N/A: not called |NOT_RUN: cleanup failed |FAIL: exact exit7/no Commit/command PASS, shutdown Busy despite SDK delete success |

The retained prelude has Exec82,999,000ns and Commit34,190,125ns. G1 generation1 reads all10B abXYZf+four zeros, digest4e23f74205ccc841a459771482bb2c61049d36603061070cb4da0edc9750f1de, checked release. Independent proof checks alias reference count2 before removal and1 after, fixed old/new UTC mtimes and symlink target file. New paths13/12/13, complete regular file bytes49/39/41 respectively. Symlink targets are verified separately from regular-byte totals.

Original v1 driver reports exit_status7, commit_called=false, unmount_ok=true and sandbox_delete_ok=true. The captured daemon diagnostic nevertheless says `sandbox shutdown retained: Busy`; cleanup takes5,481,638,875ns. The evaluator correctly refuses cleanup PASS. No independent proof of that failed row was collected, and no clean/rollback claim follows.

Root cause is an incorrect benchmark assumption: unmount detaches FUSE, while Workspace.close_clean_until explicitly refuses state.dirty_inodes>0 (runtime/lifecycle.rs:73–91). Daemon shutdown preserves a dirty owner's Commit/Status endpoint after Busy. Sandbox deletion confirms Docker resource removal, not successful Workspace graceful close, and can force-stop after grace. The current public SDK exposes no dirty-discard operation. The spec's expectation that exit7 private edits can be silently discarded by clean shutdown was not a supported contract. Do not remove the guard or accept force removal as clean custody.

Register a distinct v2 known-error recovery workflow before collecting it: confirm the canonical head unchanged after exit7 through public C5 query, Pin/read the accepted7B private state, unmount and observe open retained Workspace, explicitly Commit those accepted edits as the benchmark owner's declared recovery, verify parent/new tree and full held bytes, release and check clean shutdown. This adds work and a deliberate Commit after the failure observation; it is not automatic product behavior or a relabel of v1. v1 remains visible historical FAIL, with no replay. Existing3 passing SDK profiles and all3 native profiles are reused with original source/artifact labels.

[Compact receipts](20260930-workspace-mutations-sdk-r069-receipts.json) retain all source/harness/build/image/fixture/proof/command identities and failure diagnostics. Both raw paths stay immutable. Next collect only new v2 profile r070 and separate full proof<9s at same15s complete bound. No worker, quota/Budget, deadline, codec or product change. Earlier families and passing unit/benchmark arms are not swept. Exact per-commit production LOC135638→135638(delta+0), reference65417/Core70221.270-depth and10240 architecture work remain deferred#276; full numeric/admission is open.
