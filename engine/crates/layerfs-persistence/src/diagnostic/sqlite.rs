//! SQLite engine checks on the original file, including its committed WAL.
use super::{emit, scan, Observation, Options, Outcome};
use crate::{
    backend::records::{BackendError, Param},
    SqliteHandles,
};
use layerfs_storage::port::PersistenceError;

impl SqliteHandles {
    /// Audits the original Store in one read transaction. No source migration,
    /// checkpoint, publication, repair or staging operation is performed.
    /// Observations stream to the caller; expiration returns incomplete work.
    pub fn diagnose(
        &self,
        options: Options,
        observer: &mut dyn FnMut(Observation),
    ) -> Result<(), PersistenceError> {
        let interrupt = self
            .storage
            .session
            .state
            .lock()
            .map_err(|_| PersistenceError::Uncertain)?
            .connection
            .get_interrupt_handle();
        let (done, ended) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let watcher = (options.deadline.is_some() || options.cancelled.is_some()).then(|| {
                scope.spawn(move || loop {
                    if options.check().is_err() {
                        interrupt.interrupt();
                        break;
                    }
                    let wait =
                        options
                            .deadline
                            .map_or(std::time::Duration::from_millis(20), |deadline| {
                                deadline
                                    .saturating_duration_since(std::time::Instant::now())
                                    .min(std::time::Duration::from_millis(20))
                            });
                    match ended.recv_timeout(wait) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                })
            });
            let result=self.storage.session.run::<_,BackendError>(false, |tx| {
            options.check()?;
            // First data read pins the view before inventory and decoders run.
            let identity = tx.query("SELECT catalog_incarnation FROM history_meta WHERE id=1", vec![])?;
            let incarnation:i64 = identity.first().ok_or(BackendError::Integrity)?.get(0)?;
            emit(observer, "store.snapshot", Outcome::Pass, format!("SQLite read transaction pinned; catalog incarnation {incarnation}; committed WAL included; long scans retain WAL frames"));
            if let Err(error) = crate::backend::sqlite::schema_diagnostic::check(tx,self.storage.session.acquisition) {
                emit(observer,"store.schema",if error==BackendError::Integrity {Outcome::Fail}else{Outcome::Unknown},"SQLite schema/authority claims could not be verified against shipped definitions; no untrusted schema payload extracted; this does not establish content corruption");
                return Err(error);
            }
            let authority = tx.query("SELECT catalog_id,binding_key FROM history_meta WHERE id=1",vec![])?;
            let authority = authority.first().ok_or(BackendError::Integrity)?;
            let catalog:Vec<u8> = authority.get(0)?;
            let binding:Vec<u8> = authority.get(1)?;
            if catalog!=self.history.catalog_id.as_slice() || layerfs_history::CatalogId::derive(&binding).map_err(|_|BackendError::Integrity)?!=self.history.catalog_id || incarnation<=0 || incarnation as u64!=self.history.incarnation {
                emit(observer,"store.identity",Outcome::Fail,"pinned catalog identity/incarnation differs from admitted Store authority; dependent logical checks blocked; this does not establish content corruption");
                return Err(BackendError::Integrity);
            }
            emit(observer,"store.identity",Outcome::Pass,"pinned catalog identity, binding and incarnation match admitted Store authority");
            crate::backend::sqlite::schema::check(tx.connection,tx.work,(tx.layout(),self.storage.session.acquisition))?;
            let schema=tx.query("SELECT schema_source,schema_definition FROM history_meta WHERE id=1",vec![])?;
            let schema=schema.first().ok_or(BackendError::Integrity)?;
            if schema.get::<String>(0)?!=crate::backend::sqlite::schema::source((tx.layout(),self.storage.session.acquisition))||schema.get::<String>(1)?!=crate::backend::sqlite::schema::definition(tx.connection,tx.work)?{return Err(BackendError::Integrity);}
            emit(observer,"store.schema",Outcome::Pass,format!("SQLite application ID, schema version {}, table set and frozen schema definition checked in audit view",tx.schema_version()));
            let pragma = if options.quick { "PRAGMA quick_check(100)" } else { "PRAGMA integrity_check(100)" };
            let rows = tx.query(pragma, vec![])?;
            let mut errors = 0;
            for row in rows {
                let message:String = row.get(0)?;
                if message != "ok" { errors+=1; emit(observer,"store.database.structure",Outcome::Fail,message); }
            }
            if errors == 0 { emit(observer,"store.database.structure",Outcome::Pass,if options.quick { "SQLite quick_check completed; uniqueness and index/table equivalence excluded" } else { "SQLite integrity_check completed" }); }
            else if errors >=100 { emit(observer,"store.database.structure.coverage",Outcome::Unknown,"SQLite engine error output reached limit 100; additional errors may exist"); }
            options.check()?;
            // Stream rather than collecting every foreign-key violation.
            let mut statement=tx.connection.prepare("PRAGMA foreign_key_check").map_err(crate::backend::sqlite::session::rows::error)?;
            let mut rows=statement.query([]).map_err(crate::backend::sqlite::session::rows::error)?;
            let mut violations=0u64;
            while let Some(row)=rows.next().map_err(crate::backend::sqlite::session::rows::error)? {
                options.check()?;
                violations+=1;
                let table:String=row.get(0).map_err(crate::backend::sqlite::session::rows::error)?;
                let parent:String=row.get(2).map_err(crate::backend::sqlite::session::rows::error)?;
                emit(observer,"store.database.foreign_keys",Outcome::Fail,format!("table {table} references absent row in {parent}"));
            }
            if violations==0 { emit(observer,"store.database.foreign_keys",Outcome::Pass,"SQLite foreign_key_check completed"); }
            drop(rows); drop(statement);
            let ceiling=tx.query("SELECT COALESCE(MAX(pack_id),0) FROM pack",Vec::<Param>::new())?.first().ok_or(BackendError::Integrity)?.get(0)?;
            scan::run(tx, ceiling, options, observer)
        });
            let _ = done.send(());
            if watcher.is_some_and(|thread| thread.join().is_err()) {
                return Err(PersistenceError::Uncertain);
            }
            result.map_err(Into::into)
        })
    }
}
