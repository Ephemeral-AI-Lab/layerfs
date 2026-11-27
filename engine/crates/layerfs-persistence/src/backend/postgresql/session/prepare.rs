//! Statements prepared ahead of their first use (#768): a session prepares
//! the texts an operation will send in one flight when it opens, so the
//! operation itself sends no prepare.
use super::connection::Session;
use crate::backend::error::BackendError;
use layerfs_telemetry::timer::waits::Wait;

/// The text an autocommit read sends for `sql` on a session whose open set
/// a generation: the statement with the Store's generation joined as its
/// trailing column (`Transaction::fenced`). A shipped statement ends in `;`,
/// which the extended protocol accepts as one statement but a parenthesized
/// subquery does not.
pub(crate) fn fenced(sql: &str, schema: &str) -> String {
    let statement = sql.trim_end();
    let statement = statement.strip_suffix(';').unwrap_or(statement).trim_end();
    format!(
        "SELECT q.*,f.__layerfs_incarnation FROM ({statement}) q CROSS JOIN \
         (SELECT catalog_incarnation AS __layerfs_incarnation FROM {schema}.history_meta WHERE id=1) f"
    )
}

impl Session {
    /// Prepares every one of `texts` not yet prepared on this session's
    /// connection, all in one flight (one round trip, none when all are
    /// cached), each counted in `prepares`. A prepare changes nothing on
    /// the server, so a refused one is the caller's definite failure; one
    /// whose reply was lost quarantines the session, as any lost reply does.
    ///
    /// Texts that name tables without the schema are prepared inside one
    /// read transaction sent in the same flight, whose `SET LOCAL
    /// search_path` resolves them as the transactions that run them do.
    pub(crate) fn prepare(&self, texts: &[String]) -> Result<(), BackendError> {
        let wait = if self.writable {
            Wait::Writer
        } else {
            Wait::Reader
        };
        let turn = self.admission.enter(wait)?;
        let mut state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        self.ready(&mut state, turn.nested)?;
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        let begin = self.settings.read_begin();
        match state.statements_in(&texts, Some(&begin)) {
            Ok(_) => Ok(()),
            Err(e) => {
                let error = e.backend();
                if error == BackendError::Unknown {
                    state.quarantine();
                }
                Err(error)
            }
        }
    }
}
