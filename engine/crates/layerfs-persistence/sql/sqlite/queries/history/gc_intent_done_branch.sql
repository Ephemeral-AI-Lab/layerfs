-- One keyset page of the retained `done` audit rows of one Branch, oldest
-- first by (updated_ms, op_id): the rows strictly after the last row the
-- page before ended at ($3, $4; the first page passes the smallest time and
-- an empty op, which every row follows). A release between two reads
-- deletes only rows older than the newest it keeps and retains its own row
-- after every other one, so a page neither skips nor repeats a row. The
-- caller asks one row more than it shows to know whether a page follows.
-- The operator audit names the project and Branch it reads, so one Branch's
-- retained rows never leak into another's page, and a removed Branch's rows
-- stay readable by the names they recorded. Live admission queries filter
-- `state <> 'done'`. Shared by SQLite and PostgreSQL.
SELECT op_id, kind, scope_kind, scope_id, project, branch, machine, sandbox, phase, evidence, updated_ms
FROM gc_intent
WHERE state = 'done' AND project = $1 AND branch = $2
  AND (updated_ms > $3 OR (updated_ms = $3 AND op_id > $4))
ORDER BY updated_ms, op_id
LIMIT $5;
