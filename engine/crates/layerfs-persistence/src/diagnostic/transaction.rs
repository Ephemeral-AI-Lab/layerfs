//! Borrowed database mechanics: inventory and decoding share this exact view.
use crate::backend::{
    records::{BackendError, Param, Record},
    HistoryStatements, Mechanics,
};

pub(super) trait View: Mechanics + HistoryStatements {
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError>;
}

impl View for crate::backend::Transaction<'_> {
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        self.query(sql, params)
    }
}

#[cfg(feature = "postgresql")]
impl View for crate::backend::postgresql::session::transaction::Transaction<'_> {
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        // Diagnostic statements use numbered SQLite parameters. PostgreSQL's
        // schema is checked by Settings and set locally in this transaction.
        let sql = sql.replace("?1", "$1").replace("?2", "$2");
        self.query(&sql, params)
    }
}
