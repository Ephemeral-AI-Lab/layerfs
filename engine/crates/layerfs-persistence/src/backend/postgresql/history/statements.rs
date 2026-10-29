//! History statement texts on PostgreSQL. The shipped SQLite statements are
//! portable SQL (positional `$n` parameters, `RETURNING`, `ON CONFLICT`) and
//! run unchanged; only those using SQLite-only syntax are replaced here.
//! Array forms bind whole columns, so many rows cost one round trip.
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

/// The array form of a per-row statement, where one exists.
pub(crate) fn arrays(key: &str) -> Option<&'static str> {
    match key {
        "hint_insert_bindings" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/hint_insert_bindings.sql"
        )),
        "digest_insert_rows" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/digest_insert_rows.sql"
        )),
        "digest_roots" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/digest_roots.sql"
        )),
        _ => None,
    }
}
