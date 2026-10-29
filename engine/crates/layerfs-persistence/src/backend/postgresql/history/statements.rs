//! History statement texts on PostgreSQL. The shipped SQLite statements are
//! portable SQL (positional `$n` parameters, `RETURNING`, `ON CONFLICT`) and
//! run unchanged; only those using SQLite-only syntax are replaced here.
use crate::backend::records::BackendError;

pub(crate) fn statement(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "hint_insert_binding" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/hint_insert_binding.sql"
        )),
        "statistics_totals" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/statistics_totals.sql"
        )),
        other => crate::backend::sqlite::session::query::history(other),
    }
}
