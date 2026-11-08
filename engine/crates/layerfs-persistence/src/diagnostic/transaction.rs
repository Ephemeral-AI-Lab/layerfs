//! Borrowed database mechanics: inventory and decoding share this exact view.
use crate::backend::{
    records::{BackendError, Param, Record},
    HistoryStatements, Mechanics,
};

pub(super) trait View: Mechanics + HistoryStatements {
    fn has_table(&self, name: &str) -> Result<bool, BackendError>;
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError>;
}

impl View for crate::backend::Transaction<'_> {
    fn has_table(&self, name: &str) -> Result<bool, BackendError> {
        Ok(self
            .query(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
                vec![Param::Text(name.to_owned())],
            )?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<i64>(0)?
            != 0)
    }
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        self.query(&adapt(sql, true), params)
    }
}

#[cfg(feature = "postgresql")]
impl View for crate::backend::postgresql::session::transaction::Transaction<'_> {
    fn has_table(&self, name: &str) -> Result<bool, BackendError> {
        Ok(self.query("SELECT count(*) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relname=$1 AND c.relkind IN ('r','p')",vec![Param::Text(name.to_owned())])?.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?!=0)
    }
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        // Diagnostic statements use numbered SQLite parameters. PostgreSQL's
        // schema is checked by Settings and set locally in this transaction.
        let mut sql = sql.to_owned();
        for index in (1..=16).rev() {
            sql = sql.replace(&format!("?{index}"), &format!("${index}"));
        }
        self.query(&adapt(&sql, false), params)
    }
}

// These markers deliberately distinguish portable BLOB widths from text bytes.
// SQLite length(TEXT) stops at NUL and counts characters; never use it as a
// pre-extraction memory guard. PostgreSQL bytea already enforces storage class.
fn adapt(sql: &str, sqlite: bool) -> String {
    let mut sql = sql.to_owned();
    for (marker, blob) in [("blob_length(", true), ("byte_length(", false)] {
        while let Some(start) = sql.find(marker) {
            let from = start + marker.len();
            let end = sql[from..]
                .find(')')
                .map(|i| from + i)
                .expect("diagnostic scalar column marker");
            let field = &sql[from..end];
            let value = if sqlite {
                if blob {
                    format!("(CASE WHEN {field} IS NULL THEN NULL WHEN typeof({field})='blob' THEN length(CAST({field} AS BLOB)) ELSE -1 END)")
                } else {
                    format!("length(CAST({field} AS BLOB))")
                }
            } else {
                format!("octet_length({field})")
            };
            sql.replace_range(start..=end, &value);
        }
    }
    sql
}
