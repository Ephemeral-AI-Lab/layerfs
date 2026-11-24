-- The newest retained `done` time of the scope op $1 was admitted at, or
-- NULL when the scope has none. A release stamps its retained row after it,
-- so a host whose clock is behind still ranks its own row newest: the cap
-- keeps it and the keyset audit returns it last. The read runs inside the
-- release's write transaction, after SQLite's writer lock or PostgreSQL's
-- Store lane row is taken, so it sees every earlier release of the scope.
-- The scope is one Branch identity, not the names the audit reads (no index
-- covers those). One bounded read: the op by
-- its key, then its scope's rows through the existing UNIQUE (scope_kind,
-- scope_id, owner_boot) index prefix (the cap keeps at most 100 done rows a
-- scope). Shared by SQLite and PostgreSQL.
SELECT MAX(d.updated_ms)
FROM gc_intent d
WHERE d.state = 'done'
  AND d.scope_kind = (SELECT o.scope_kind FROM gc_intent o WHERE o.op_id = $1)
  AND d.scope_id = (SELECT o.scope_id FROM gc_intent o WHERE o.op_id = $1);
