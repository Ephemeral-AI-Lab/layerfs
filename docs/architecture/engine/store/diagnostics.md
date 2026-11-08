# Observational Store diagnostics

The [table coverage map](diagnostic-coverage.md) records authority, keys, validators
and quick/full policy for every applicable Store category.

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
| Independent lineage/overwrite/run/absorption/reach | Yes | Yes |
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
joins its scoped watcher. PostgreSQL also joins a scoped cancellation watcher;
its owned cancellation flag interrupts in-flight queries without an overall deadline.
Multi-host admission resolves/connects each configured endpoint in sequence,
including Unix/TCP fallback; randomized multi-host admission is explicitly
unsupported instead of changing the saved selection semantics. Remote diagnostic frames are bounded and calls are
not replayed after uncertain transport loss.

Findings distinguish authoritative inconsistencies from unavailable coverage.
Advisory hint/signature/digest failures become warnings in Control. Current
explicit limits: exhaustive adversarial merge/force-history, file-mapping,
attribute and symlink corruption qualification remains pending. Unsupported
SQLite versions and unsupported connection configurations are incomplete,
not evidence of corruption. Engine error output is capped at 100 messages per
SQLite check or PostgreSQL heap and truncation is reported.

### Exact Layer overwrite reconstruction

Before each publication at height `h`, the audit derives the source Commit's
newest Layer height `n` from immutable edges. It finds the greatest earlier
publication height whose source is either a local Commit ancestor of that source
(first and Commit-second parents only), or a Commit first absorbed at or below
`n`. Together with `n`, capped at `h - 1`, this is the covered height; the
expected overwrite count is `(h - 1) - covered`. The second case includes late
publication of a Commit already represented through Layer ancestry.

This is the graph equation behind `history/lineage.rs::covered` and
`history/layerstack.rs::add_layer`, evaluated before adding this publication to
the reconstructed absorption map. The oracle reads neither saved reach answers
nor saved run labels. It audits every retained publication, across separate
stacks, even when the legacy `layer.overwrote` blob is empty (as it is for modern
writes). Invalid authoritative graphs block reconstruction explicitly.

A scoped, joined interruption watcher covers scratch SQL as well as source SQL;
deadline or cancellation interrupts recursive queries and leaves coverage
incomplete rather than declaring corruption. Graph closure and prior publication
lookups use private disk-backed scratch; runtime can grow with history size.

### Structural damage before LayerFS admission

If read-only LayerFS handle admission or the logical audit fails, diagnosis may
make one separate read-only physical observation of the original SQLite file.
This supplemental view is **not** the logical inventory's pinned snapshot: it
does not restart or replay the logical audit, validate its earlier inventory,
or clear previous failures. Logical access/coverage remains Unknown. An expired
deadline or cancellation skips supplemental work.

Only SQLite `SQLITE_CORRUPT`/`SQLITE_NOTADB` or non-`ok` integrity/quick-check rows
confirm a structural failure. Busy/locked, permission/I/O, missing file, unknown
application schema or binding mismatch alone do not establish corruption. A
clean supplemental structural check says nothing about supported application
schema, authority, canonical objects or history integrity.
