# F8 atomic history foundation

> **Status:** Atomic history implementation and Disposable host/Linux proofs;
> F8 Save/capture/local-install composition remains pending.

## Deepest-file plan

- Add required HistoryCatalog::stage_and_commit(StageRequest), with the existing
  typed CommitStagedOutcome. It is one conditional transaction, not a default
  composition of separately acknowledged port calls.
- Implement in Persistence history/catalog.rs by reusing staging::stage_changes
  and commit::commit_staged inside one existing write closure. No nested
  transaction, retry or local state mutation. On definite failure the newly
  inserted stage and token advance roll back. Existing stage ownership remains
  untouched. Unknown returns UnknownOutcome with the original request retained
  by the caller, never an invented acknowledged-stage receipt.
- Extend the Init-only external MemoryHistory fixture with explicit Unsupported
  for this operation. It is not used as atomic-publication proof.
- Add external Disposable tests/atomic_publication.rs: committed, UpToDate,
  exact HeadMoved, no stage/token leak, preserve preexisting stage, and a real
  process-held writer yields Busy/no effect followed by a new explicit operation.
  Reuse bounded process ownership helper code as external test support only.
- Update History/Persistence architecture and API docs, preserve the older
  two-call API/receipts until its consumers retire. No changed Save allocation
  behavior or F8 daemon/local-install completion claim in this checkpoint.

Host first, then pinned Linux targeted build/run, 100s stops and append-only
receipts. Count exactly one write transaction per successful/conflicting
publication and zero successful write transactions on Busy. No timing admission;
Durable builds but execution stays owner-deferred. Final scoped checks and LOC.

## Retained outcomes

[Receipts01–09](checks/pre-s8-atomic-history-20261007/) record targeted builds,
source hashes, host/Linux proofs and final checks. All three bodies pass on
both platforms. Committed and UpToDate each use one successful write transaction.
Exact HeadMoved uses one write transaction and one rollback, returns exact
expected/actual head and base, and consumes no stage token. No newly created
stage remains on any definite result. An existing separately acknowledged stage
is preserved when the combined attempt refuses it.

The second-process proof records one failed BEGIN statement, zero successful
write transactions, zero rollback and no new stage or changed head on Busy.
Reads still finish while it holds the writer. After explicit release/join, a
new original stage-and-commit succeeds. No automatic retry or discard is used.
Test database directories are removed on successful completion; no failed or
unknown operation arose. Unknown handling is preserved by the existing provider
quarantine and the method returns no separately acknowledged stage claim; no
new injected-unknown execution is claimed.

Host binary SHA256 `c7bc45fef8bd2e27ba6c4e88297b97ccf68f7ea255154918207369dc009bf4e1`;
Linux `e68ae7d40dfb447e1c835691d34b61958124187e83f509cc31a2e78a03bd4de9`.
Pinned image `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`;
Cargo1.85.1 locked/test-optimized, repository ARM64 config,
`LAYERFS_CONSTRUCTION_WORKERS=1`, profile Disposable WAL/OFF. Linux fixtures are
container-local `/tmp`. Every invocation has100s wall stop; observed bodies are
below0.1s, diagnostic only. Cache state is uncontrolled; no timing credit.
Durable compiled; execution NOT_RUN — owner-deferred.

Scoped all-target Clippy covers History, Persistence, Project, SDK and Daemon.
Formatting,750-file boundary and43 guard self-tests pass. This checkpoint tests
C5 semantic transitions with real persisted history records; content savedness,
Save transaction counts and local Workspace install are explicitly not proved
by these synthetic root identities. F8 closes only after that integrated proof.

The owner-approved [reservation decision](SAVE-RESERVATION-DECISION-20261007.md)
revises the next checkpoint's count equation to initial reservation plus counted
refills plus publication batches plus one stage-and-commit. It does not change
this history implementation or require a timing resample.

Production LOC:170951 ->170965 (delta+14). Core105534 ->105548;
active62369 ->62383; reference65417, excluded predecessors36325 and excluded
integration6840 unchanged. Receipt10 records exact parent/staged counting.
