# Observational Store diagnostics

`layerfs-persistence::diagnostic` exposes streamed observations over a dedicated
read-only session. Control resolves the saved Store and its authoritative
placement. SQLite can run locally only in the file's kernel; remote execution
uses the fixed-root `store.diagnose` worker operation. PostgreSQL uses the saved
connection and password reference. Diagnosis never migrates, checkpoints,
repairs, acquires a source staging lease, or enters the publication lane.

SQLite pins one transaction on the original database including committed WAL.
PostgreSQL pins one REPEATABLE READ READ ONLY transaction for logical inventory
and all object readers. Concurrent publications do not enter this view. Long
audits retain WAL frames or MVCC versions. PostgreSQL online physical checks
run afterward and explicitly have a different observation interval.

| Coverage | Full | Quick |
| --- | --- | --- |
| SQLite original engine | integrity_check plus foreign_key_check | quick_check plus foreign_key_check |
| Schema and typed history rows/identities/graph | Yes | Yes |
| Independent lineage/run/absorption/reach | Yes | Yes |
| Ordinal blocks and advisory hint/signature syntax | Yes | Yes |
| Every pack and winning stored object, including retained orphans | Yes | Skipped |
| Contextual historical namespace qualification | Yes | Skipped |
| File payload streaming and digest hints | Yes | Skipped |
| PostgreSQL supported amcheck heap/TOAST/B-tree functions | Yes, if installed and permitted | Skipped |

Quick checks can still take O(N) time. SQLite quick_check excludes uniqueness
and index/table equivalence. PostgreSQL amcheck is never installed automatically;
missing functions, privileges or unsupported relations produce unknown coverage.
No ordinary diagnostic uses bt_index_parent_check or claims offline checksums.

Inventories use actual row keys rather than contiguous-ID assumptions. Duplicate
physical representations, first-wins forward dependency locators, allocator
gaps and retained unreachable data are legitimate. Canonical authentication
uses existing pooled/delta readers; all physical pack record framing is checked
separately. Existing content validators check locally decidable child semantics,
while qualify_root runs once per historical root/scope/profile context. Indexed
scratch lives in a private SQLite temporary file outside the source and is
removed when its owned connection closes. File bytes flow into a bounded hash
sink rather than being retained as complete files.

A supplied absolute deadline covers SQL, decoding boundaries, resolver helper,
TCP, TLS and authentication. PostgreSQL without an overall audit deadline still
has a ten-second connection admission limit. Cancellation may take up to one
additional second for server cancellation/ROLLBACK acknowledgment; uncertain
release is reported explicitly. SQLite interrupts the actual connection and
joins its scoped watcher. Remote diagnostic frames are bounded and calls are
not replayed after uncertain transport loss.

Findings distinguish authoritative inconsistencies from unavailable coverage.
Advisory hint/signature/digest failures become warnings in Control. Current
explicit limits: exact Layer overwrite reconstruction is not implemented
(range checks only); exhaustive adversarial merge/force-history, file-mapping,
attribute and symlink corruption qualification remains pending. Unsupported
SQLite versions and unsupported connection configurations are incomplete,
not evidence of corruption. Engine error output is capped at 100 messages per
SQLite check or PostgreSQL heap and truncation is reported.
