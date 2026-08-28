//! Legacy Full product-generation integrity verification.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use rusqlite::Connection;

pub(crate) fn verify_product_integrity(connection: &Connection) -> EngineResult<u64> {
    let mut statements = 0_u64;
    let foreign_key_failure = {
        statements = statements
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        let mut statement = connection
            .prepare("PRAGMA foreign_key_check")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        rows.next().map_err(map_sqlite_error)?.is_some()
    };
    if foreign_key_failure {
        return Err(EngineError::InvalidRecord("product foreign key integrity"));
    }
    let checks = [
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branches b
            LEFT JOIN layerfs_branches p ON p.branch_id = b.immediate_parent_branch_id
            LEFT JOIN layerfs_operation_versions v
              ON v.branch_id = b.immediate_parent_branch_id
             AND v.operation_version_id = b.fork_operation_version_id
            WHERE (b.depth = 0 AND NOT EXISTS(
                       SELECT 1 FROM layerfs_layers l
                       WHERE l.layer_stack_id = b.origin_layer_stack_id
                         AND l.layer_id = b.origin_layer_id
                         AND l.root_id = b.fork_root_id))
               OR (b.depth > 0 AND (
                    p.branch_id IS NULL OR p.depth + 1 != b.depth
                    OR p.origin_layer_stack_id != b.origin_layer_stack_id
                    OR p.origin_layer_id != b.origin_layer_id
                    OR v.root_id != b.fork_root_id
                    OR v.created_by_kind != 'operation'
                    OR v.created_by_operation_id != b.fork_operation_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branches b
            WHERE (b.generation = 0 AND b.head_operation_version_id IS NOT NULL)
               OR (b.generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branch_transitions t
                    WHERE t.branch_id = b.branch_id
                      AND t.after_generation = b.generation
                      AND t.after_operation_version_id = b.head_operation_version_id))
               OR (SELECT COUNT(*) FROM layerfs_branch_transitions t
                   WHERE t.branch_id = b.branch_id) != b.generation
               OR (SELECT COUNT(DISTINCT after_generation)
                   FROM layerfs_branch_transitions t
                   WHERE t.branch_id = b.branch_id) != b.generation)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branch_transitions t
            LEFT JOIN layerfs_branch_transitions p
              ON p.branch_id = t.branch_id
             AND p.after_generation = t.before_generation
            WHERE (t.before_generation = 0 AND t.before_operation_version_id IS NOT NULL)
               OR (t.before_generation > 0 AND (
                    p.transition_id IS NULL
                    OR p.after_operation_version_id IS NOT t.before_operation_version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layer_stacks s
            WHERE NOT EXISTS(
                    SELECT 1 FROM layerfs_layers g
                    WHERE g.layer_stack_id = s.layer_stack_id
                      AND g.creation_kind = 'genesis'
                      AND g.accepted_generation = 0)
               OR (s.generation = 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers g
                    WHERE g.layer_stack_id = s.layer_stack_id
                      AND g.layer_id = s.head_layer_id
                      AND g.creation_kind = 'genesis'))
               OR (s.generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layer_stack_transitions t
                    WHERE t.layer_stack_id = s.layer_stack_id
                      AND t.after_generation = s.generation
                      AND t.after_layer_id = s.head_layer_id))
               OR (SELECT COUNT(*) FROM layerfs_layer_stack_transitions t
                   WHERE t.layer_stack_id = s.layer_stack_id) != s.generation
               OR (SELECT COUNT(DISTINCT after_generation)
                   FROM layerfs_layer_stack_transitions t
                   WHERE t.layer_stack_id = s.layer_stack_id) != s.generation)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layer_stack_transitions t
            LEFT JOIN layerfs_layer_stack_transitions p
              ON p.layer_stack_id = t.layer_stack_id
             AND p.after_generation = t.before_generation
            LEFT JOIN layerfs_layers g
              ON g.layer_stack_id = t.layer_stack_id
             AND g.creation_kind = 'genesis'
            WHERE (t.before_generation = 0 AND t.before_layer_id != g.layer_id)
               OR (t.before_generation > 0 AND (
                    p.transition_id IS NULL OR p.after_layer_id != t.before_layer_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layers l
            LEFT JOIN layerfs_branches b ON b.branch_id = l.source_branch_id
            LEFT JOIN layerfs_branch_transitions t
              ON t.branch_id = l.source_branch_id
             AND t.after_generation = l.source_branch_generation
             AND t.after_operation_version_id = l.source_branch_head_operation_version_id
            WHERE l.creation_kind = 'candidate' AND (
                b.branch_id IS NULL OR b.depth != l.source_branch_depth
                OR b.origin_layer_stack_id != l.layer_stack_id
                OR t.transition_id IS NULL))
            OR EXISTS(
                SELECT 1 FROM layerfs_branch_deltas d
                LEFT JOIN layerfs_branch_transitions t
                  ON t.branch_id = d.source_branch_id
                 AND t.after_generation = d.source_branch_generation
                 AND t.after_operation_version_id = d.source_branch_operation_version_id
                LEFT JOIN layerfs_operation_versions v
                  ON v.branch_id = d.source_branch_id
                 AND v.operation_version_id = d.source_branch_operation_version_id
                WHERE t.transition_id IS NULL OR v.root_id != d.source_root)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_version_leases x
            WHERE (x.target_kind = 'layer' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers l WHERE l.layer_id = x.target_id))
               OR (x.target_kind = 'operation_version' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_operation_versions v
                    WHERE v.operation_version_id = x.target_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_released_versions r
            LEFT JOIN layerfs_operation_versions v
              ON r.target_kind = 'operation_version'
             AND v.branch_id = r.owner_id AND v.operation_version_id = r.version_id
            LEFT JOIN layerfs_branch_transitions t
              ON t.branch_id = r.owner_id AND t.request_id = r.request_id
             AND t.action_kind = 'branch_rollback'
            LEFT JOIN layerfs_operation_versions target
              ON target.branch_id = t.branch_id
             AND target.operation_version_id = t.after_operation_version_id
            LEFT JOIN layerfs_operation_versions before_v
              ON before_v.branch_id = t.branch_id
             AND before_v.operation_version_id = t.before_operation_version_id
            LEFT JOIN layerfs_branches b ON b.branch_id = r.owner_id
            WHERE r.target_kind = 'operation_version' AND (
                v.operation_version_id IS NULL OR v.root_id != r.root_id
                OR (t.transition_id IS NULL AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'branch' AND f.target_id = r.owner_id
                      AND r.release_generation > f.staged_generation))
                OR t.after_generation != r.release_generation
                OR v.sequence <= target.sequence OR v.sequence > before_v.sequence
                OR (b.head_operation_version_id = r.version_id AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'branch' AND f.target_id = b.branch_id))
                OR EXISTS(SELECT 1 FROM layerfs_version_leases x
                    WHERE x.target_kind = 'operation_version'
                      AND x.target_id = r.version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_released_versions r
            LEFT JOIN layerfs_layers l
              ON r.target_kind = 'layer'
             AND l.layer_stack_id = r.owner_id AND l.layer_id = r.version_id
            LEFT JOIN layerfs_layer_stack_transitions t
              ON t.layer_stack_id = r.owner_id AND t.request_id = r.request_id
             AND t.action_kind = 'layer_stack_rollback'
            LEFT JOIN layerfs_layers target
              ON target.layer_stack_id = t.layer_stack_id
             AND target.layer_id = t.after_layer_id
            LEFT JOIN layerfs_layers before_l
              ON before_l.layer_stack_id = t.layer_stack_id
             AND before_l.layer_id = t.before_layer_id
            LEFT JOIN layerfs_layer_stacks s ON s.layer_stack_id = r.owner_id
            WHERE r.target_kind = 'layer' AND (
                l.layer_id IS NULL OR l.root_id != r.root_id
                OR (t.transition_id IS NULL AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'layer_stack' AND f.target_id = r.owner_id
                      AND r.release_generation > f.staged_generation))
                OR t.after_generation != r.release_generation
                OR l.accepted_generation <= target.accepted_generation
                OR l.accepted_generation > before_l.accepted_generation
                OR (s.head_layer_id = r.version_id AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'layer_stack'
                      AND f.target_id = s.layer_stack_id))
                OR EXISTS(SELECT 1 FROM layerfs_version_leases x
                    WHERE x.target_kind = 'layer' AND x.target_id = r.version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_durable_tracking_refs r
            WHERE (r.target_kind = 'branch' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branches b
                    WHERE b.branch_id = r.target_id AND (
                        (r.generation = 0 AND b.fork_root_id = r.root_id)
                        OR EXISTS(
                            SELECT 1 FROM layerfs_branch_transitions t
                            JOIN layerfs_operation_versions v
                              ON v.branch_id = t.branch_id
                             AND v.operation_version_id = t.after_operation_version_id
                            WHERE t.branch_id = b.branch_id
                              AND t.after_generation = r.generation
                              AND t.after_operation_version_id = r.target_version_id
                              AND v.root_id = r.root_id))))
               OR (r.target_kind = 'layer' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers l
                    WHERE l.layer_id = r.target_id AND l.root_id = r.root_id))
               OR (r.target_kind = 'operation_version' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_operation_versions v
                    WHERE v.operation_version_id = r.target_id
                      AND v.root_id = r.root_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_push_outbox o
            LEFT JOIN layerfs_branches b ON b.branch_id = o.branch_id
            WHERE (o.accepted_generation = 0 AND (
                       o.operation_version_id IS NOT NULL
                       OR b.fork_root_id != o.accepted_root_id))
               OR (o.accepted_generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branch_transitions t
                    JOIN layerfs_operation_versions v
                      ON v.branch_id = t.branch_id
                     AND v.operation_version_id = t.after_operation_version_id
                    WHERE t.branch_id = o.branch_id
                      AND t.after_generation = o.accepted_generation
                      AND t.after_operation_version_id = o.operation_version_id
                      AND v.root_id = o.accepted_root_id)))",
    ];
    for (index, sql) in checks.into_iter().enumerate() {
        statements = statements
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        let invalid = connection
            .query_row(sql, [], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?;
        if invalid {
            return Err(EngineError::InvalidRecord(match index {
                0 => "product Branch ancestry",
                1 => "product Branch head",
                2 => "product Branch transition chain",
                3 => "product LayerStack head",
                4 => "product LayerStack transition chain",
                5 => "product Layer source ancestry",
                6 => "product lease target",
                7 => "product released OperationVersion",
                8 => "product released Layer",
                9 => "product tracking target",
                _ => "product outbox target",
            }));
        }
    }
    Ok(statements)
}

pub(crate) fn verify_full_product_integrity(connection: &Connection) -> EngineResult<bool> {
    let empty = connection
        .query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM layerfs_objects)
                AND NOT EXISTS(SELECT 1 FROM layerfs_deltas)
                AND NOT EXISTS(SELECT 1 FROM layerfs_retained_roots)
                AND NOT EXISTS(SELECT 1 FROM layerfs_layers)
                AND NOT EXISTS(SELECT 1 FROM layerfs_layer_stacks)
                AND NOT EXISTS(SELECT 1 FROM layerfs_branches)
                AND NOT EXISTS(SELECT 1 FROM layerfs_operations)
                AND NOT EXISTS(SELECT 1 FROM layerfs_operation_versions)
                AND NOT EXISTS(SELECT 1 FROM layerfs_branch_deltas)
                AND NOT EXISTS(SELECT 1 FROM layerfs_branch_transitions)
                AND NOT EXISTS(SELECT 1 FROM layerfs_layer_stack_transitions)
                AND NOT EXISTS(SELECT 1 FROM layerfs_version_leases)
                AND NOT EXISTS(SELECT 1 FROM layerfs_released_versions)
                AND NOT EXISTS(SELECT 1 FROM layerfs_durable_tracking_refs)
                AND NOT EXISTS(SELECT 1 FROM layerfs_transfer_state)
                AND NOT EXISTS(SELECT 1 FROM layerfs_branch_push_pages)
                AND NOT EXISTS(SELECT 1 FROM layerfs_sync_object_pins)
                AND NOT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts)
                AND NOT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items)
                AND NOT EXISTS(SELECT 1 FROM layerfs_sync_receipts)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    if empty {
        return Ok(true);
    }
    let checks = [
        (
            "Full Branch ancestry/head",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_branches b
                LEFT JOIN layerfs_layers l
                  ON l.layer_stack_id = b.origin_layer_stack_id
                 AND l.layer_id = b.origin_layer_id
                LEFT JOIN layerfs_branches p ON p.branch_id = b.immediate_parent_branch_id
                LEFT JOIN layerfs_operation_versions f
                  ON f.branch_id = b.immediate_parent_branch_id
                 AND f.operation_version_id = b.fork_operation_version_id
                WHERE l.layer_id IS NULL OR l.result_root_id != b.fork_root_id
                   OR (b.depth > 0 AND (p.branch_id IS NULL OR p.depth + 1 != b.depth
                       OR f.operation_version_id IS NULL
                       OR f.result_root_id != b.fork_root_id))
                   OR (b.generation = 0 AND b.head_operation_version_id IS NOT NULL)
                   OR (b.generation > 0 AND NOT EXISTS(
                        SELECT 1 FROM layerfs_branch_transitions t
                        WHERE t.branch_id = b.branch_id
                          AND t.after_generation = b.generation
                          AND t.after_operation_version_id = b.head_operation_version_id))
                   OR (SELECT count(*) FROM layerfs_branch_transitions t
                       WHERE t.branch_id = b.branch_id) != b.generation)",
        ),
        (
            "Full Branch transition chain",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_branch_transitions t
                LEFT JOIN layerfs_branch_transitions p
                  ON p.branch_id = t.branch_id
                 AND p.after_generation = t.before_generation
                WHERE (t.before_generation = 0
                         AND t.before_operation_version_id IS NOT NULL)
                   OR (t.before_generation > 0 AND
                         (p.transition_id IS NULL OR
                          p.after_operation_version_id IS NOT t.before_operation_version_id))
                   OR (t.after_generation > 0 AND t.after_operation_version_id IS NULL))",
        ),
        (
            "Full OperationVersion fold",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_operation_versions v
                LEFT JOIN layerfs_operations o ON o.operation_id = v.operation_id
                LEFT JOIN layerfs_branch_deltas b ON b.branch_delta_id = v.branch_delta_id
                LEFT JOIN layerfs_deltas d ON d.delta_id = v.transition_delta_id
                WHERE d.delta_id IS NULL OR d.parent_root_id IS NOT v.base_root_id
                   OR d.result_root_id != v.result_root_id
                   OR (v.created_by_kind = 'operation' AND
                        (o.operation_id IS NULL OR o.branch_id != v.branch_id
                         OR o.result_operation_version_id != v.operation_version_id
                         OR o.base_root_id != v.base_root_id
                         OR o.candidate_root_id != v.result_root_id
                         OR o.state != 'durably_accepted'))
                   OR (v.created_by_kind = 'child_merge' AND
                        (b.branch_delta_id IS NULL OR b.purpose != 'child_merge'
                         OR b.source_branch_id != v.child_branch_id
                         OR b.applied_delta_id != v.transition_delta_id
                         OR b.destination_root_id != v.base_root_id
                         OR b.result_root_id != v.result_root_id)))",
        ),
        (
            "Full LayerStack head/chain",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_layer_stacks s
                WHERE (SELECT count(*) FROM layerfs_layers g
                       WHERE g.layer_stack_id = s.layer_stack_id
                         AND g.creation_kind = 'genesis' AND g.state = 'accepted'
                         AND g.accepted_generation = 0) != 1
                   OR (SELECT count(*) FROM layerfs_layer_stack_transitions t
                       WHERE t.layer_stack_id = s.layer_stack_id) != s.generation
                   OR (s.generation = 0 AND NOT EXISTS(SELECT 1 FROM layerfs_layers h
                       WHERE h.layer_stack_id = s.layer_stack_id
                         AND h.layer_id = s.head_layer_id
                         AND h.creation_kind = 'genesis' AND h.state = 'accepted'))
                   OR (s.generation > 0 AND NOT EXISTS(
                       SELECT 1 FROM layerfs_layer_stack_transitions h
                       WHERE h.layer_stack_id = s.layer_stack_id
                         AND h.after_generation = s.generation
                         AND h.after_layer_id = s.head_layer_id)))
             OR EXISTS(
                SELECT 1 FROM layerfs_layer_stack_transitions t
                LEFT JOIN layerfs_layer_stack_transitions p
                  ON p.layer_stack_id = t.layer_stack_id
                 AND p.after_generation = t.before_generation
                LEFT JOIN layerfs_layers g
                  ON g.layer_stack_id = t.layer_stack_id
                 AND g.creation_kind = 'genesis'
                WHERE (t.before_generation = 0 AND t.before_layer_id != g.layer_id)
                   OR (t.before_generation > 0 AND
                         (p.transition_id IS NULL OR p.after_layer_id != t.before_layer_id)))",
        ),
        (
            "Full Layer fold/source",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_layers l
                LEFT JOIN layerfs_layers p
                  ON p.layer_stack_id = l.layer_stack_id AND p.layer_id = l.parent_layer_id
                LEFT JOIN layerfs_branches b ON b.branch_id = l.source_branch_id
                LEFT JOIN layerfs_branch_transitions t
                  ON t.branch_id = l.source_branch_id
                 AND t.after_generation = l.source_branch_generation
                 AND t.after_operation_version_id = l.source_operation_version_id
                LEFT JOIN layerfs_branch_deltas x
                  ON x.branch_delta_id = l.source_branch_delta_id
                LEFT JOIN layerfs_deltas d ON d.delta_id = l.transition_delta_id
                WHERE l.creation_kind = 'candidate' AND
                      (p.layer_id IS NULL OR b.branch_id IS NULL OR b.depth != l.source_branch_depth
                       OR t.transition_id IS NULL OR x.branch_delta_id IS NULL
                       OR x.purpose != 'layer_stack_merge'
                       OR x.source_branch_id != l.source_branch_id
                       OR x.source_operation_version_id != l.source_operation_version_id
                       OR x.destination_root_id != l.parent_root_id
                       OR x.result_root_id != l.result_root_id
                       OR d.delta_id IS NULL OR d.parent_root_id != l.parent_root_id
                       OR d.result_root_id != l.result_root_id))",
        ),
        (
            "Full released version",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_released_versions r
                LEFT JOIN layerfs_layers l
                  ON r.target_kind = 'layer' AND l.layer_stack_id = r.layer_stack_id
                 AND l.layer_id = r.layer_id
                LEFT JOIN layerfs_operation_versions v
                  ON r.target_kind = 'operation_version' AND v.branch_id = r.branch_id
                 AND v.operation_version_id = r.operation_version_id
                WHERE (r.target_kind = 'layer' AND
                         (l.layer_id IS NULL OR l.result_root_id != r.root_id OR NOT EXISTS(
                          SELECT 1 FROM layerfs_layer_stack_transitions t
                          WHERE t.layer_stack_id = r.layer_stack_id
                            AND t.request_id = r.request_id
                            AND t.after_generation = r.release_generation
                            AND t.action_kind = 'layer_stack_rollback')))
                   OR (r.target_kind = 'operation_version' AND
                         (v.operation_version_id IS NULL OR v.result_root_id != r.root_id
                          OR NOT EXISTS(SELECT 1 FROM layerfs_branch_transitions t
                          WHERE t.branch_id = r.branch_id AND t.request_id = r.request_id
                            AND t.after_generation = r.release_generation
                            AND t.action_kind = 'branch_rollback'))))",
        ),
        (
            "Full tracking target",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_durable_tracking_refs r
                WHERE r.status != 'verified_complete'
                   OR (r.target_kind = 'layer' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_layers l WHERE l.layer_id = r.target_id
                          AND l.result_root_id = r.root_id))
                   OR (r.target_kind = 'operation_version' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_operation_versions v
                        WHERE v.operation_version_id = r.target_id
                          AND v.result_root_id = r.root_id))
                   OR (r.target_kind = 'branch' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_branches b WHERE b.branch_id = r.target_id
                          AND ((r.generation = 0 AND r.target_version_id IS NULL
                                AND b.fork_root_id = r.root_id)
                            OR EXISTS(SELECT 1 FROM layerfs_branch_transitions t
                                JOIN layerfs_operation_versions v
                                  ON v.branch_id = t.branch_id
                                 AND v.operation_version_id = t.after_operation_version_id
                                WHERE t.branch_id = b.branch_id
                                  AND t.after_generation = r.generation
                                  AND t.after_operation_version_id = r.target_version_id
                                  AND v.result_root_id = r.root_id)))))",
        ),
        (
            "Full accepted receipt",
            "SELECT EXISTS(
                SELECT 1 FROM layerfs_sync_receipts r
                WHERE r.result IN ('fetched', 'durably_accepted', 'verified_complete')
                  AND ((r.candidate_kind = 'branch' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_branches b WHERE b.branch_id = r.candidate_id
                          AND ((r.decided_generation = 0 AND r.decided_head_id IS NULL
                                AND b.fork_root_id = r.decided_root_id)
                            OR EXISTS(SELECT 1 FROM layerfs_branch_transitions t
                                JOIN layerfs_operation_versions v
                                  ON v.branch_id = t.branch_id
                                 AND v.operation_version_id = t.after_operation_version_id
                                WHERE t.branch_id = b.branch_id
                                  AND t.after_generation = r.decided_generation
                                  AND t.after_operation_version_id = r.decided_head_id
                                  AND v.result_root_id = r.decided_root_id))))
                    OR (r.candidate_kind = 'layer' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_layers l WHERE l.layer_id = r.decided_head_id
                          AND l.result_root_id = r.decided_root_id))
                    OR (r.candidate_kind = 'operation_version' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_operation_versions v
                        WHERE v.operation_version_id = r.decided_head_id
                          AND v.result_root_id = r.decided_root_id))))",
        ),
    ];
    for (name, sql) in checks {
        if connection
            .query_row(sql, [], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?
        {
            return Err(EngineError::InvalidRecord(name));
        }
    }
    Ok(false)
}
