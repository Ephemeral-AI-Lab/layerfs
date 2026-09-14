# V4 Store publication receipt component evidence

Status: **Store component PASS; runtime integration and full V4 acceptance remain open.**
No benchmark workloads or #122 scenarios were executed. Source hashes, commands,
environment and retained attempts are in `verification.jsonl`, the two initial
source manifests and `final-source.json`. These are development checks against
uncommitted working source, not a sealed benchmark candidate.

## Implemented contract

- `WorkspacePublicationAttempt` captures Workspace, branch/LayerStack, expected
  head/root/base, exact canonical candidate, new base and covered snapshot sequence.
  The key derives the specified `(workspace, candidate, expected head, new base)`
  tuple. Every receipt lookup additionally validates the entire captured context.
  Two successive `UpToDate` captures with the same key cannot reuse stale coverage;
  the earlier receipt must first be applied and acknowledged.
- `commit_workspace_candidate_retained` shares the existing admission, exact stage,
  source validation, canonical Commit derivation and conditional branch transaction.
  Receipt insertion and exact stage retirement are inside that same transaction,
  including the no-op path. It does **not** acknowledge runtime coverage.
- `publish_workspace_stage` retries the exact retained canonical stage without
  construction or admission. Existing receipts return their exact original outcome.
- `resolve_workspace_publication` first reads the exact receipt. Without it, exact
  unchanged branch context is known not published; a verified deterministic Commit
  in this branch's bounded ancestry is a Created witness. An unrelated branch,
  missing stage, moved no-op branch, or exhausted ancestry bound never implies PASS.
- `acknowledge_workspace_publication` removes only the exact acknowledged context.
  It is idempotent and rollback-safe; a cleanup error leaves known success and its
  receipt intact. Runtime must apply published coverage **before** this call.
- Retention is one fixed-width row per Workspace and at most 1024 rows per Store;
  a full receipt table rolls the publication transaction back while preserving the
  exact stage. There is no automatic eviction or outcome history. Ancestry lookup
  visits at most 256 Commit rows and otherwise returns explicit Unknown. These are
  implementation resource bounds, not new numerical performance acceptance gates.

## Explicit schema compatibility decision

`sql/schema/workspace_publications.sql` is an optional runtime metadata extension,
created only by receipt-enabled publication. Canonical Store format versions 6–10
are unchanged. Ordinary connects do not create or migrate it. Schema validation
accepts the exact extension SQL alongside the exact supported base schema and
continues rejecting unexpected columns, indexes, triggers and other schema objects
before writer configuration. Existing non-receipt API behavior and schema fixtures
remain available during coordinator integration. The table is an explicit schema
change authorized by V4, not a copied Workspace checkpoint or canonical format.

## Results and retained failure

| Evidence | Result | Coverage |
| --- | --- | --- |
| `attempt-01-publication.log` | FAIL_COMPILE; retained | `rusqlite` count type and `MutexGuard` dereference type errors; no tests ran |
| `attempt-02-publication.log` | 5 passed | 2 new receipt tests plus 3 existing publication-related tests |
| `attempt-03-schema.log` | 9 passed | Existing strict-schema, legacy/nonpromotion, connection and durability checks |
| `attempt-04-manifest.log` | 1 passed | Exact SQL registry/preparation against the legacy schema |
| `attempt-05-legacy-staging.log` | 1 passed | Existing v6 stage retention, failures, retry and publication |
| `attempt-06-other-branch.log` | 1 passed | Final form of the stage/capacity test, extended with other-branch witness rejection |

The two receipt tests cover post-transaction reply loss for Created and UpToDate;
fresh Store reopen; later authorized branch advance; deterministic exact retry;
sequence and context mismatches; cleanup failure; receipt capacity rollback; retained
stage conflicts; missing-stage uncertainty; bounded ancestry; malformed extension
rejection without mutation; and the exact candidate Commit existing only on another
branch. Attempt 06 changed only test assertions, so previously passing unrelated
checks are retained. The existing `unused_mut` warning in admission metadata tests
was preserved outside this task's ownership.

## Remaining integration

The runtime's ordinary `Workspace::commit` and workspace reconciliation still use
legacy `commit_workspace_candidate`/`commit_workspace_reconciliation`. Existing
`pending_stage`/`pending_publication` behavior has not been silently changed. The
attempt coordinator must retain the new immutable attempt alongside the owned
snapshot and canonical correspondence, use the retained API, resolve unknown
outcomes without recapture, apply coverage/context, and only then acknowledge.
End/Discard must resolve any in-flight publication and keep snapshot/cleanup owners
until safe release. Public Created/UpToDate results can be derived from the receipt;
replayed receipts intentionally do not fabricate admission/timing counters.
