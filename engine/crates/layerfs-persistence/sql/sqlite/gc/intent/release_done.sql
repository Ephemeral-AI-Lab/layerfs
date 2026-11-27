-- The durable 0-table operator audit: release keeps the intent row, clears
-- the owner columns so its unique key cannot block a later mount, and stores
-- the action/operator/op evidence. Admission queries filter `state <> 'done'`;
-- `gc_intent_done` is the only read that sees the retained row. A row already
-- retained is not matched again, so a second release reports it gone instead
-- of rewriting the audit line. It answers the row's scope, which
-- `gc_intent_done_cap` then bounds.
UPDATE gc_intent
SET state = 'done',
    uncertain = FALSE,
    owner_runtime = NULL,
    owner_boot = NULL,
    evidence = $1,
    updated_ms = $2
WHERE op_id = $3 AND state <> 'done'
RETURNING scope_kind, scope_id;
