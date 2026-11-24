-- A heartbeat only ever extends: a delayed renewal carrying an older deadline
-- must not shorten a lease a later one already set. The fence is checked in the
-- same write transaction before this statement runs.
UPDATE gc_intent
SET lease_expires_ms = $1, updated_ms = $2
WHERE op_id = $3 AND state <> 'done'
  AND (lease_expires_ms IS NULL OR lease_expires_ms < $1)
RETURNING op_id;
