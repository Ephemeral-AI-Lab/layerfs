-- One keyset page of the retained `done` audit rows of one Branch, oldest
-- first by (updated_ms, op_id): the rows strictly after the last row the
-- page before ended at ($3, $4; the first page passes the smallest time and
-- an empty op, which every row follows). The caller asks one row more than
-- it shows to know whether a page follows.
--
-- The guarantee, exactly. No row is ever returned twice: a retained row's
-- key never changes, and a release only deletes rows (the cap) or adds one.
-- A row the cap deletes before the reader reaches it is not returned. A row
-- of the Branch identity the release belongs to is never skipped: releases
-- are serialized (SQLite's single writer, PostgreSQL's Store lane row) and
-- each is stamped after that identity's newest done row, so it commits with
-- a key above every row of it a reader can have seen. Across identities it
-- can be: the names select the rows of every Branch that bore them, but the
-- stamp floor is per identity, so after a Branch (or its project) is removed
-- and created again under the same names, a release of the new Branch is
-- stamped at or below the earlier Branch's rows when those were stamped at
-- or after the releasing host's clock (that host's clock is behind, or the
-- earlier Branch retained more than one row a millisecond). Such a row sorts
-- among the earlier rows, and a reader whose cursor is already past it does
-- not get it; a read from the first page returns it.
--
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
