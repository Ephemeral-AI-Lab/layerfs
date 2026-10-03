//! Closed statement selection and measured SQLite execution.
use super::{connection::SqlWork, rows};
use crate::backend::records::{BackendError, Param, Record};
use rusqlite::{Connection, StatementStatus};
use std::{cell::RefCell, time::Instant};
pub(crate) fn history(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "allocation_advance_scope" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/allocation_advance_scope.sql"
        )),
        "allocation_insert_scope" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/allocation_insert_scope.sql"
        )),
        "allocation_scope_by_id" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/allocation_scope_by_id.sql"
        )),
        "branch_branch_by_id" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/branch_branch_by_id.sql"
        )),
        "branch_branch_first_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/branch_branch_first_page.sql"
        )),
        "branch_branch_name_taken" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/branch_branch_name_taken.sql"
        )),
        "branch_branch_next_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/branch_branch_next_page.sql"
        )),
        "branch_insert_branch" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/branch_insert_branch.sql"
        )),
        "commit_advance_branch" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/commit_advance_branch.sql"
        )),
        "commit_branch_head" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/commit_branch_head.sql"
        )),
        "commit_commit_by_id" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/commit_commit_by_id.sql"
        )),
        "commit_delete_stage" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/commit_delete_stage.sql"
        )),
        "commit_insert_commit" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/commit_insert_commit.sql"
        )),
        "layerstack_advance_stack" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_advance_stack.sql"
        )),
        "layerstack_insert_layer" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_insert_layer.sql"
        )),
        "layerstack_insert_stack" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_insert_stack.sql"
        )),
        "layerstack_layer_by_id" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_layer_by_id.sql"
        )),
        "layerstack_layer_by_source" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_layer_by_source.sql"
        )),
        "layerstack_layer_root" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_layer_root.sql"
        )),
        "layerstack_stack_by_id" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_stack_by_id.sql"
        )),
        "layerstack_stack_first_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_stack_first_page.sql"
        )),
        "layerstack_stack_head" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_stack_head.sql"
        )),
        "layerstack_stack_name_taken" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_stack_name_taken.sql"
        )),
        "layerstack_stack_next_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/layerstack_stack_next_page.sql"
        )),
        "staging_bump_token" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_bump_token.sql"
        )),
        "staging_delete_stage" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_delete_stage.sql"
        )),
        "staging_insert_stage" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_insert_stage.sql"
        )),
        "staging_next_token" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_next_token.sql"
        )),
        "staging_stage_by_workspace" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_stage_by_workspace.sql"
        )),
        "staging_stage_first_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_stage_first_page.sql"
        )),
        "staging_stage_next_page" => Ok(include_str!(
            "../../../sql/sqlite/queries/history/staging_stage_next_page.sql"
        )),
        _ => Err(BackendError::Integrity),
    }
}
pub(crate) fn run(
    connection: &Connection,
    sql: &str,
    params: Vec<Param>,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    let values: Vec<_> = params.into_iter().map(rows::bind).collect();
    let bytes = values
        .iter()
        .map(|v| match v {
            rusqlite::types::Value::Blob(v) => v.len(),
            rusqlite::types::Value::Text(v) => v.len(),
            _ => 8,
        })
        .sum::<usize>() as u64;
    let bindings: Vec<&dyn rusqlite::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    borrowed(connection, sql, &bindings, bytes, work)
}
pub(crate) fn borrowed(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes: u64,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    let start = Instant::now();
    let result = (|| {
        let mut statement = connection.prepare_cached(sql).map_err(rows::error)?;
        let count = statement.column_count();
        let result = (|| {
            let mut cursor = statement.query(values).map_err(rows::error)?;
            let mut result = Vec::new();
            while let Some(row) = cursor.next().map_err(rows::error)? {
                result.push(rows::record(row, count)?);
            }
            Ok(result)
        })();
        let steps = statement.get_status(StatementStatus::VmStep).max(0) as u64;
        statement.reset_status(StatementStatus::VmStep);
        let mut w = work.borrow_mut();
        w.statements += 1;
        w.vm_steps += steps;
        w.bound_bytes += bytes;
        result
    })();
    let elapsed = start.elapsed().as_nanos() as u64;
    work.borrow_mut().statement_ns += elapsed;
    if sql == "COMMIT" {
        work.borrow_mut().commit_ns += elapsed;
    }
    result
}

pub(crate) fn batch(
    connection: &Connection,
    sql: &str,
    work: &RefCell<SqlWork>,
) -> Result<(), BackendError> {
    use rusqlite::fallible_iterator::FallibleIterator;
    let start = Instant::now();
    let result = (|| {
        let mut batch = rusqlite::Batch::new(connection, sql);
        while let Some(mut statement) = batch.next().map_err(rows::error)? {
            let result = statement.execute([]).map_err(rows::error);
            let mut w = work.borrow_mut();
            w.statements += 1;
            w.vm_steps += statement.get_status(StatementStatus::VmStep).max(0) as u64;
            result?;
        }
        Ok(())
    })();
    work.borrow_mut().statement_ns += start.elapsed().as_nanos() as u64;
    result
}
