# Retained-history producer and independent verifier port

> Status: first-state functional diagnostic only. All three history gates NOT_RUN.
> Product identity: dabd0aa8e plus untracked benchmark vehicles, explicitly dirty.
> No production algorithm/profile changed; no speed sample taken in this round.

The new release/locked `layerfs-project` examples exercise C1 construction,
C2 physical publication and real C5 initialize/fork/stage/commit/add-layer
operations on the shared durable SQLite authority. They retain the original
v4 single-producer selection: same-path chunk predecessors and advisory roots
on; full/ordered/similarity producer options off; predecessor depth limit255.
The corpus parser, digest, Git-OID validation, allocator/read instrumentation and
canonical collector reuse first-party original harness modules unchanged.
Filesystem chain and retained C5 adapters follow the original public operations.
There is no second database, synthetic SQL-root row or repeated-Init substitute.

A fresh Store was constructed for ONLY the first selected stride10 state. This
is not the seventeen-state selection and has no declared cold input contract.
The probe records DIAGNOSTIC, selected_states17, executed_states1 and admission
NOT_RUN. A separate process opens that Store read-only, queries C5 custody,
walks the entire declared tree through public C1/C2 reads, checks every kind and
logical size, and authenticates every tenth content path plus the final path.
Stored role/length metadata is fetched through bounded public persistence locate
calls (512 distinct IDs), not through an added SQL dependency. Verifier digest
and length maps reuse immutable identities within that proof invocation only;
they do not warm a later speed arm.

| Diagnostic | Complete command ns / bound | Result / scope |
| --- | ---: | --- |
| first-state producer | 1,008,797,500 /25,000,000,000 | exit0, one state, one custody state; root ff484f8d92f57413ee83e7201fb04c8512ffd7bed3083471713db4e8e17c70d3 |
| initial verifier | 870,505,875 /9,500,000,000 | exit1: receipt adapter expected child rather than recorded run.child; no Store read |
| corrected verifier | 544,587,084 /9,500,000,000 | exit0, complete359paths;29sampled content paths;306,296authenticated logicalB; no independent root-pin check |

The producer's raw lifecycle operation clock is190,773,417ns. Its explanatory
stages are acquisition52,285,875ns, changed content/predecessors6,496,416ns,
filesystem1,372,292ns, save/C562,775,334ns. These are **not a matched speed
comparison**, neither additive total attribution nor full-history projections.
The clock also includes corpus opening, Store creation, custody query, checkpoint,
close and logging; the future comparison must prospectively match that scope or
replace the vehicle before either gate arm. No historical receipt is relabeled.
Wait4 lifetime child RSS is40,468,480B producer /15,941,632B corrected verifier;
it is not phase-only or a whole-importer bound. The producer's per-state
canonical collector, correspondence maps and original corpus owners remain
explicit harness scope; no file-size-independent memory claim follows.

All three append-only raw folders are retained under
`benchmark-results/fs-bench-pro/issue302-history-first-state-{probe1,verifier-probe1,verifier-probe2}`.
Compact prospective/receipt/manifest/error copies are under this issue's checks
folder. Binaries are immutable SHA256 archive entries named in those receipts.
The failed verifier remains visible. Correcting the schema changed the verifier
binary and reused the first-state Store only for a separate correctness probe;
no unchanged producer or speed arm was rerun.

Reproduction vehicle (fresh Store/scratch required, diagnostics only):

```
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 \
 LAYERFS_HISTORY_CHUNK_PREDECESSORS=1 LAYERFS_HISTORY_FULL_PRODUCER=0 \
 LAYERFS_HISTORY_ORDERED_PREDECESSORS=0 LAYERFS_HISTORY_SIMILARITY_CANDIDATES=0 \
 LAYERFS_HISTORY_DEPTH_LIMIT=255 core/target/release/examples/benchmark_history \
 /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data <fresh-store> <fresh-scratch> history-stride10 probe
core/target/release/examples/verify_history \
 /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data <retained-store> <recorded-receipt> history-stride10 probe
```

These are driver arguments, not permission to bypass the runner or sample again.
The actual diagnostic invoked the archived executables through the bounded
phase7_sqlite.invoke vehicle after reading benchmark_agent_report.md. Diagnostic
producer25s exception was recorded before invocation; verifier9.5s unchanged.

Checks: release/locked producer and verifier builds PASS after adapter repairs;
scoped warning-denying Clippy for both examples PASS after fixing local formatting
and non-Drop lifetime cleanup. Four explicitly scoped lint allowances apply only
to unchanged first-party borrowed reference modules, not active product code.
Core fmt --all --check PASS; boundary438 production files PASS; guard self-tests23
PASS. The initial virtual-workspace fmt command without --all found no targets,
so it did not verify formatting. No production edit or full workspace test rerun
was needed for this tooling port; no full-history performance/proof was run.

Remaining before history admission: baseline producer vehicle on unmodified
7edddbdb8; prospectively sealed independent roots and O3 inventories; both-arm
cold contract that prevents an earlier state/write from crediting a measured
phase; matched complete-product scope; and history command budgets reconciled
with current15s/declared25s policy. Original60/170s limits are not silently
adopted. The registered17/53/157-state rows remain NOT_RUN and the all-seven goal
remains ACTIVE. Current Init time failures remain unchanged.

Production LOC for this tooling/evidence commit:137677->137677(delta0), root
reference65417, core72260(active28216/inactive reference44044), migration old191/
new7806/rest-core64263. Exact parent/staged-tree counter confirmation is required
before commit; examples/docs are excluded by the existing reproducible method.
