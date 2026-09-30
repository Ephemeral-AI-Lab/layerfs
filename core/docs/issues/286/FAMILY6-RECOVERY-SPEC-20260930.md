# Family6 known failed-shell recovery — prospective v2

Functional diagnostic extension after immutable r069; not release admission.
The original workspace-mutations-shell-exit7-no-commit-sdk-v1 remains historical
FAIL: exact exit7 and no Commit but dirty shutdown Busy. Original spec and
receipts are not rewritten. Core close deliberately refuses dirty file state;
unmount detaches FUSE and public SDK has no dirty-discard operation. Force
Docker deletion cannot qualify graceful Workspace cleanup. No product change
or guard removal is part of this correction.

New exact ID: workspace-mutations-shell-exit7-retained-explicit-commit-sdk-v2.
Same prepared SDK small master, independent byte copy, arbitrary ordinary
command `mkdir uncommitted; printf private > uncommitted/file; exit7`, executed
with set-e/umask022 and exact expected exit7. Complete launch-to-exit <=15s;
separate independent full verifier <9s, one initial sample at committed source.
Cache uncontrolled/numeric INELIGIBLE, one construction worker, all product
memory/quota/crypto/format/dependency defaults unchanged.

Declared sequence inside the complete command:

1. SDK Exec returns known7; no automatic Commit has been called.
2. Read-only public Service/C5 HistoryQuery uses required HISTORY_PROFILE2 and
   confirms branch head equals the prepared old Commit. This observer and its
   root reads are inside the command, never hidden in setup.
3. Public SDK PinView captures accepted private state; fully read/hash7B private
   through lease before recovery. Unmount FUSE, then public Status must report
   unmounted, open and not stopping: the owner remains available for recovery.
4. Benchmark owner explicitly calls public SDK Commit after observing the
   failure/retention. Require known new Commit with prepared parent. This is
   an explicit test recovery decision, not automatic commit of failed commands.
5. Read/hash all7 held bytes again, check held generation/status and completed
   lease release; checked Status, Sandbox delete/logs/shutdown. Reuse the actual
   earlier successful unmount rather than issue an undeclared retry.

Expected old canonical tree remains9 paths/3 files/30B. Explicit recovery's new
canonical tree has11 paths/4 files/37B: extra directory uncommitted mode0755 and
file mode0644 with exact7B private. All prior paths/modes/bytes and inherited
inode identities remain. Read-only independent full tree/byte/history verifier
requires advancement and correct parent after the explicit recovery; direct
before-recovery head equality and private-view proof establish no implicit Commit.
SDK operation_ns includes observer/Pin/unmount/Status/recovery, final commit_ns
records the explicit Commit. The original sdk.shell_package nested span covers
initial Exec; do not mistake it for the whole recovery or subtract overlapping
spans to invent Server/transport attribution.

Current default Family6 SDK selection is3 successful original mixed/retained/
refusal profiles plus this v2. Old v1 stays in registry/report as historical FAIL,
not sampled or silently dropped. Collect only new v2 asr070; reuse r069's3
passing rows/full proofs, r067 live andr068 failure/resume profiles, and unchanged
r058 stopping/r054 authority proof scopes with original identities. Future
explicit discard/graceful-shutdown-ack capability is outside this experiment.

```sh
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-mutations-shell-exit7-retained-explicit-commit-sdk-v2 --out benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-recovery-r070
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-recovery-r070 --out benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-recovery-proof-r070
```

All source/artifact/fixture/oracle/commands/verdicts are append-only; no relaxed
bound, codec work, product branch, new API or unchanged performance rerun.
Production LOC stays135638 (reference65417/Core70221), per exact snapshot counter.
