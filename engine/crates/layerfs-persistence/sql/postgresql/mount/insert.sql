-- A mount's intent, inside its admission transaction after its keys (#768):
-- inserted only when the attempt token $18 is this transaction's, the Branch
-- $2 exists and is not retired (the first placeholder below:
-- `branch_branch_by_id`, its $1 read as $2) and no live operation conflicts
-- in its scope (the second: `gc_intent_ops_branch` with $1 the project, $2
-- the Branch, $3 the op), the very texts the statement-by-statement
-- registration runs. $3..$17 are the
-- row's values as `gc_intent_insert` binds them. A row the insert cannot
-- make (any of these, or an existing op or owner) fails the statement with
-- 22012, so the transaction aborts and its COMMIT writes nothing.
WITH ins AS (
    INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine,
                           project, branch, owner_runtime, owner_boot, state, uncertain, phase,
                           generation, artifact, high_water, evidence, updated_ms)
    SELECT $3::bytea, $4::text, $5::text, $2::bytea, $6::text, $7::text, $8::text,
           $9::text, $10::text, $11::text, $12::text, $13::text, ($14::bigint <> 0), $15::text,
           NULL, NULL, NULL, $16::text, $17::bigint
     WHERE current_setting('layerfs.attempt', true) IS NOT DISTINCT FROM $18::text
       AND EXISTS (SELECT 1 FROM ({branch}
           ) b)
       AND NOT EXISTS (SELECT 1 FROM ({conflicts}
           ) c)
    ON CONFLICT DO NOTHING
    RETURNING 1
)
SELECT 1 / (SELECT count(*) FROM ins)::int;
