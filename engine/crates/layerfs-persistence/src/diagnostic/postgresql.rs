//! PostgreSQL connection, coherent logical audit and supported online checks.
use super::{emit, scan, Observation, Options, Outcome};
use crate::{
    backend::{
        postgresql::session::driver::Driver,
        records::{BackendError, Param},
    },
    PostgresHandles, PostgresSelection,
};
use layerfs_storage::port::PersistenceError;

/// Checks the saved PostgreSQL connection without modifying the database.
/// The diagnostic async driver owns and bounds DNS/TCP/TLS/authentication.
pub fn connection(
    selection: &PostgresSelection,
    binding: &[u8],
    options: Options,
    resolver: &mut super::Resolver<'_>,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), PersistenceError> {
    let mut client = match Driver::diagnostic(selection, options.deadline, resolver) {
        Ok(client) => client,
        Err(error) => {
            if let crate::backend::postgresql::session::driver::Error::Unsupported(reason) = &error
            {
                emit(
                    observer,
                    "store.connection.configuration",
                    Outcome::Unknown,
                    *reason,
                );
                return Ok(());
            }
            let code = error.code().map(|c| c.code()).unwrap_or("unavailable");
            let (id, outcome) = if code.starts_with("28") {
                ("store.connection.authentication", Outcome::Fail)
            } else if code == "3D000" {
                ("store.connection.database", Outcome::Fail)
            } else {
                ("store.connection.transport", Outcome::Unknown)
            };
            emit(observer,id,outcome,format!("PostgreSQL connection not established; SQLSTATE {code}; credentials and driver error text omitted"));
            return Ok(());
        }
    };
    emit(observer,"store.connection.authentication",Outcome::Pass,"PostgreSQL driver established authenticated database session using configured TLS policy; DNS/TCP/TLS were combined by the driver");
    let result=client.query_one("SELECT COALESCE(has_schema_privilege((SELECT oid FROM pg_catalog.pg_namespace WHERE nspname=$1),'USAGE'),false), EXISTS(SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname=$1)",&[&selection.schema]);
    match result {
        Ok(row) => {
            let permitted: bool = row.try_get(0).map_err(|_| PersistenceError::Malformed)?;
            let exists: bool = row.try_get(1).map_err(|_| PersistenceError::Malformed)?;
            emit(
                observer,
                "store.connection.schema",
                if exists && permitted {
                    Outcome::Pass
                } else {
                    Outcome::Fail
                },
                if exists && permitted {
                    "configured schema exists and role has USAGE"
                } else {
                    "configured schema is absent or role lacks USAGE"
                },
            );
        }
        Err(error) => emit(
            observer,
            "store.connection.schema",
            Outcome::Unknown,
            format!(
                "read-only schema query did not complete; SQLSTATE {}",
                error.code().map(|c| c.code()).unwrap_or("unavailable")
            ),
        ),
    }
    let table = format!("\"{}\".history_meta", selection.schema.replace('"', "\"\""));
    match client.query_one(
        &format!("SELECT binding_key,catalog_incarnation FROM {table} WHERE id=1"),
        &[],
    ) {
        Ok(row) => {
            let recorded: Vec<u8> = row.try_get(0).map_err(|_| PersistenceError::Malformed)?;
            emit(
                observer,
                "store.connection.identity",
                if recorded == binding {
                    Outcome::Pass
                } else {
                    Outcome::Fail
                },
                if recorded == binding {
                    "configured Store metadata is readable and binding matches the saved Store"
                } else {
                    "database schema binding does not match the saved Store"
                },
            );
        }
        Err(error) => emit(
            observer,
            "store.connection.read",
            if matches!(error.code().map(|c| c.code()), Some("42501" | "42P01")) {
                Outcome::Fail
            } else {
                Outcome::Unknown
            },
            format!(
                "configured Store metadata read did not complete; SQLSTATE {}",
                error.code().map(|c| c.code()).unwrap_or("unavailable")
            ),
        ),
    }
    // Establish actual negotiated TLS when the server exposes this observation.
    match client.query_one("SELECT ssl FROM pg_catalog.pg_stat_ssl WHERE pid=pg_backend_pid()",&[]){
        Ok(row)=>{let ssl:bool=row.try_get(0).map_err(|_|PersistenceError::Malformed)?;emit(observer,"store.connection.tls",Outcome::Pass,if ssl{"server reports TLS active; verification followed configured driver policy"}else{"server reports no TLS; connection followed configured driver policy"});}
        Err(_)=>emit(observer,"store.connection.tls",Outcome::Unknown,"negotiated TLS observation unavailable; successful connection alone does not establish TLS"),
    }
    if client.close().is_err() {
        emit(
            observer,
            "store.connection.close",
            Outcome::Unknown,
            "database session close was not acknowledged",
        );
    }
    Ok(())
}

impl PostgresHandles {
    /// Audits logical LayerFS data in one REPEATABLE READ READ ONLY view.
    /// Online engine checks follow separately and carry their own observations.
    pub fn diagnose(
        &self,
        options: Options,
        observer: &mut dyn FnMut(Observation),
    ) -> Result<(), PersistenceError> {
        let cancellation = self
            .storage
            .session
            .state
            .lock()
            .map_err(|_| PersistenceError::Uncertain)?
            .client
            .cancellation();
        let (done, ended) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let watcher = options.cancelled.zip(cancellation).map(|(source, target)| {
                scope.spawn(move || loop {
                    if source.load(std::sync::atomic::Ordering::Relaxed) {
                        target.store(true, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    match ended.recv_timeout(std::time::Duration::from_millis(20)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                })
            });
            let result = self.diagnose_inner(options, observer);
            let _ = done.send(());
            if watcher.is_some_and(|thread| thread.join().is_err()) {
                return Err(PersistenceError::Uncertain);
            }
            result
        })
    }
    fn diagnose_inner(
        &self,
        options: Options,
        observer: &mut dyn FnMut(Observation),
    ) -> Result<(), PersistenceError> {
        let logical=self.storage.session.run::<_,BackendError>(false,|tx|{
            options.check()?;
            if let Some(deadline)=options.deadline{
                let millis=deadline.saturating_duration_since(std::time::Instant::now()).as_millis().max(1).min(i32::MAX as u128);
                tx.batch(&format!("SET LOCAL statement_timeout={millis}; SET LOCAL lock_timeout={millis}"))?;
            }
            let identity=tx.query("SELECT txid_current_snapshot()::text,catalog_incarnation FROM history_meta WHERE id=1",vec![])?;
            let row=identity.first().ok_or(BackendError::Integrity)?;
            let snapshot:String=row.get(0)?;let incarnation:i64=row.get(1)?;
            emit(observer,"store.snapshot",Outcome::Pass,format!("PostgreSQL REPEATABLE READ READ ONLY snapshot {snapshot}; catalog incarnation {incarnation}; long scans retain MVCC versions"));
            let rows=tx.query("SELECT COALESCE(MAX(pack_id),0) FROM pack",vec![])?;
            let ceiling=rows.first().ok_or(BackendError::Integrity)?.get(0)?;
            scan::run(tx,ceiling,options,observer)
        });
        if logical.is_err() {
            let released = self
                .storage
                .session
                .state
                .lock()
                .ok()
                .and_then(|state| state.client.released());
            if let Some(released) = released {
                emit(
                    observer,
                    "store.cancellation.release",
                    if released {
                        Outcome::Pass
                    } else {
                        Outcome::Unknown
                    },
                    if released {
                        "server cancellation and ROLLBACK acknowledged; source snapshot released"
                    } else {
                        "source work/snapshot release was not acknowledged before bounded cancellation cleanup ended"
                    },
                );
            }
        }
        logical?;
        if options.quick {
            emit(
                observer,
                "store.database.relations",
                Outcome::Skipped,
                "quick mode excludes exhaustive PostgreSQL heap/TOAST/index scans",
            );
            return Ok(());
        }
        self.diagnose_relations(options, observer)
    }

    fn diagnose_relations(
        &self,
        options: Options,
        observer: &mut dyn FnMut(Observation),
    ) -> Result<(), PersistenceError> {
        let mut after = 0i64;
        let mut checked = 0u64;
        let mut failed = 0u64;
        let mut incomplete = false;
        loop {
            options.check()?;
            let relations=self.storage.session.run::<_,BackendError>(false,|tx|tx.query("SELECT c.oid::bigint,c.relkind::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.oid::bigint>$1 AND c.relkind IN ('r','i') ORDER BY c.oid LIMIT 64",vec![Param::I64(after)]))?;
            if relations.is_empty() {
                break;
            }
            for relation in relations {
                options.check()?;
                after = relation.get(0)?;
                let kind: String = relation.get(1)?;
                let mut corrupt = false;
                let result=self.storage.session.run::<_,BackendError>(false,|tx|{
                    let functions=tx.query("SELECT n.nspname,p.proname FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace JOIN pg_catalog.pg_depend d ON d.objid=p.oid AND d.classid='pg_proc'::regclass AND d.deptype='e' JOIN pg_catalog.pg_extension e ON e.oid=d.refobjid WHERE e.extname='amcheck' AND p.proname=$1 AND ((p.proname='bt_index_check' AND p.proargtypes::text='2205 16') OR (p.proname='verify_heapam' AND p.proargtypes[0]=2205 AND p.proargtypes[1]=16 AND p.proargtypes[2]=16 AND p.proargtypes[3]=25 AND p.pronargs-p.pronargdefaults<=4)) ORDER BY p.oid LIMIT 1",vec![Param::Text(if kind=="i"{"bt_index_check"}else{"verify_heapam"}.to_owned())])?;
                    let Some(function)=functions.first()else{return Ok(None);};let schema:String=function.get(0)?;let name:String=function.get(1)?;
                    let quote=|s:&str|format!("\"{}\"",s.replace('"',"\"\""));
                    let sql=if kind=="i"{format!("SELECT 1 FROM {}.{}($1::bigint::oid::regclass,{})",quote(&schema),quote(&name),!options.quick)}else{format!("SELECT msg FROM {}.{}($1::bigint::oid::regclass,false,true,'none') LIMIT 100",quote(&schema),quote(&name))};
                    tx.engine_check(&sql,after,&mut corrupt).map(Some)
                });
                match result {
                    Ok(Some(rows)) => {
                        checked += 1;
                        if kind == "r" && !rows.is_empty() {
                            failed += 1;
                            if rows.len() >= 100 {
                                incomplete = true;
                            }
                            for row in rows {
                                emit(
                                    observer,
                                    "store.database.relations",
                                    Outcome::Fail,
                                    format!("relation OID {after}: {}", row.get::<String>(0)?),
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        incomplete = true;
                        emit(observer,"store.database.relations",Outcome::Unknown,format!("relation OID {after}: supported amcheck function unavailable; no extension installed automatically"));
                    }
                    Err(_) if corrupt => {
                        failed += 1;
                        emit(observer,"store.database.relations",Outcome::Fail,format!("relation OID {after}: PostgreSQL amcheck reported SQLSTATE XX001/XX002 corruption"));
                    }
                    Err(_) => {
                        incomplete = true;
                        emit(observer,"store.database.relations",Outcome::Unknown,format!("relation OID {after}: engine check unavailable/refused/failed; this alone does not confirm corruption"));
                    }
                }
            }
        }
        emit(observer,"store.database.relations.summary",if failed>0{Outcome::Fail}else if incomplete{Outcome::Unknown}else{Outcome::Pass},format!("{checked} supported online relation checks completed outside the logical inventory snapshot; {failed} relations reported corruption; output capped at 100 messages per heap; server-files/checksum/offline verification excluded"));
        Ok(())
    }
}
