-- Bounds the retained `done` audit of one scope (a mount's Branch): after a
-- release retains a row ($3), the oldest done rows of that scope beyond the
-- newest $4 are deleted. The row just retained is never deleted, whatever
-- its time. No new index, table or column: the scope's rows are found
-- through the existing UNIQUE (scope_kind, scope_id, owner_boot) index, and a
-- live or uncertain intent is never matched (`state = 'done'`). Portable to
-- PostgreSQL, as the other intent statements are.
DELETE FROM gc_intent
WHERE state = 'done' AND scope_kind = $1 AND scope_id = $2
  AND op_id <> $3
  AND op_id NOT IN (
    SELECT op_id FROM gc_intent
    WHERE state = 'done' AND scope_kind = $1 AND scope_id = $2
    ORDER BY updated_ms DESC, op_id DESC
    LIMIT $4
  );
