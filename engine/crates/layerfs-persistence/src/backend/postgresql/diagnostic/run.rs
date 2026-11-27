//! PostgreSQL coherent logical audit and supported online checks.
use crate::backend::error::BackendError;
use crate::backend::seam::Param;
use crate::diagnostic::{check::store, emit, Observation, Options, Outcome};
use crate::PostgresHandles;
use layerfs_storage::port::PersistenceError;

pub(crate) fn with_cancellation<T>(
    source: Option<&std::sync::atomic::AtomicBool>,
    target: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    body: impl FnOnce() -> T,
) -> Result<T, PersistenceError> {
    let Some((source, target)) = source.zip(target) else {
        return Ok(body());
    };
    std::thread::scope(|scope| {
        let (done, ended) = std::sync::mpsc::channel::<()>();
        struct Stop(Option<std::sync::mpsc::Sender<()>>);
        impl Drop for Stop {
            fn drop(&mut self) {
                if let Some(done) = self.0.take() {
                    let _ = done.send(());
                }
            }
        }
        let watcher = scope.spawn(move || loop {
            if source.load(std::sync::atomic::Ordering::Relaxed) {
                target.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            match ended.recv_timeout(std::time::Duration::from_millis(20)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        });
        let stop = Stop(Some(done));
        let result = body();
        drop(stop);
        if watcher.join().is_err() {
            return Err(PersistenceError::Uncertain);
        }
        Ok(result)
    })
}

impl PostgresHandles {
    /// Audits one project's data ([`super::project`]) in one REPEATABLE READ
    /// READ ONLY view, with nothing written. Problems stream to `observer`;
    /// `progress` hears `(done, total)` trees.
    pub fn diagnose_project(
        &self,
        stack: layerfs_history::LayerStackId,
        options: Options,
        observer: &mut dyn FnMut(Observation),
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<crate::diagnostic::ProjectCoverage, PersistenceError> {
        let cancellation = self
            .storage
            .session
            .state
            .lock()
            .map_err(|_| PersistenceError::Uncertain)?
            .client
            .cancellation();
        with_cancellation(options.cancelled, cancellation, || {
            self.storage
                .session
                .run_observed::<_, BackendError>(false, |tx| {
                    options.check()?;
                    crate::backend::postgresql::schema::diagnostic_guard(tx)?;
                    let rows = tx.query("SELECT COALESCE(MAX(pack_id),0) FROM pack", vec![])?;
                    let ceiling = rows.first().ok_or(BackendError::Integrity)?.get(0)?;
                    crate::diagnostic::check::project::run(
                        tx, ceiling, stack, options, observer, progress,
                    )
                })
                .map_err(PersistenceError::from)
        })?
    }

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
        with_cancellation(options.cancelled, cancellation, || {
            self.diagnose_inner(options, observer)
        })?
    }

    fn diagnose_inner(
        &self,
        options: Options,
        observer: &mut dyn FnMut(Observation),
    ) -> Result<(), PersistenceError> {
        let logical=self.storage.session.run_observed::<_,BackendError>(false,|tx|{
            options.check()?;
            if let Some(deadline)=options.deadline{
                let millis=deadline.saturating_duration_since(std::time::Instant::now()).as_millis().max(1).min(i32::MAX as u128);
                tx.batch(&format!("SET LOCAL statement_timeout={millis}; SET LOCAL lock_timeout={millis}"))?;
            }
            if let Err(error)=crate::backend::postgresql::schema::diagnostic_guard(tx){
                emit(observer,"store.schema",if error==BackendError::Integrity{Outcome::Fail}else{Outcome::Unknown},"pinned PostgreSQL schema admission changed or is unavailable; remaining logical checks blocked; no content-corruption inference");
                return Err(error);
            }
            let identity=tx.query("SELECT md5(txid_current_snapshot()::text),catalog_incarnation,CASE WHEN octet_length(catalog_id)=32 THEN catalog_id END,CASE WHEN octet_length(binding_key) BETWEEN 1 AND 128 THEN binding_key END FROM history_meta WHERE id=1",vec![])?;
            let row=identity.first().ok_or(BackendError::Integrity)?;
            let snapshot:String=row.get(0)?;let incarnation:i64=row.get(1)?;
            let catalog=row.get::<Vec<u8>>(2).ok();let binding=row.get::<Vec<u8>>(3).ok();
            if incarnation!=self.history.incarnation as i64 || catalog.as_deref()!=Some(self.history.catalog_id.as_slice()) || binding.as_deref().and_then(|bytes|layerfs_history::CatalogId::derive(bytes).ok()).is_none_or(|derived|Some(derived.as_slice())!=catalog.as_deref()){
                emit(observer,"store.identity",Outcome::Fail,"Store catalog/binding/incarnation changed after admission; pinned logical authority rejected; no content-corruption inference");
                return Err(BackendError::Integrity);
            }
            emit(observer,"store.snapshot",Outcome::Pass,format!("PostgreSQL REPEATABLE READ READ ONLY snapshot fingerprint md5 {snapshot} (observation label, not authority); catalog incarnation {incarnation}; long scans retain MVCC versions"));
            super::contract::check(tx,options,observer)?;
            let rows=tx.query("SELECT COALESCE(MAX(pack_id),0) FROM pack",vec![])?;
            let ceiling=rows.first().ok_or(BackendError::Integrity)?.get(0)?;
            store::run(tx,ceiling,options,observer)
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
            let relations=self.storage.session.run_observed::<_,BackendError>(false,|tx|tx.query("SELECT c.oid::bigint,c.relkind::text,COALESCE(a.amname,'')::text,COALESCE(i.indisvalid AND i.indisready,true)::int::bigint FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_catalog.pg_am a ON a.oid=c.relam LEFT JOIN pg_catalog.pg_index i ON i.indexrelid=c.oid WHERE n.nspname=current_schema() AND c.oid::bigint>$1 AND c.relkind IN ('r','i','m','p','I','f','S') ORDER BY c.oid LIMIT 64",vec![Param::I64(after)]))?;
            if relations.is_empty() {
                break;
            }
            for relation in relations {
                options.check()?;
                after = relation.get(0)?;
                let kind: String = relation.get(1)?;
                let access: String = relation.get(2)?;
                let ready: i64 = relation.get(3)?;
                if !matches!(kind.as_str(), "r" | "m" | "S" | "i")
                    || (kind == "i" && (access != "btree" || ready != 1))
                {
                    incomplete = true;
                    emit(observer,"store.database.relations",Outcome::Unknown,format!("relation OID {after}: unsupported relation/access method or index not ready/valid; no engine check invoked"));
                    continue;
                }
                let started = std::time::Instant::now();
                let mut corrupt = false;
                let mut denied = false;
                let result=self.storage.session.run_observed::<_,BackendError>(false,|tx|{
                    let functions=tx.query("SELECT n.nspname,p.proname,has_function_privilege(p.oid,'EXECUTE')::int::bigint FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace JOIN pg_catalog.pg_depend d ON d.objid=p.oid AND d.classid='pg_proc'::regclass AND d.deptype='e' JOIN pg_catalog.pg_extension e ON e.oid=d.refobjid WHERE e.extname='amcheck' AND p.proname=$1 AND ((p.proname='bt_index_check' AND p.proargtypes::text='2205 16') OR (p.proname='verify_heapam' AND p.proargtypes[0]=2205 AND p.proargtypes[1]=16 AND p.proargtypes[2]=16 AND p.proargtypes[3]=25 AND p.pronargs-p.pronargdefaults<=4)) ORDER BY p.oid LIMIT 1",vec![Param::Text(if kind=="i"{"bt_index_check"}else{"verify_heapam"}.to_owned())])?;
                    let Some(function)=functions.first()else{return Ok(None);};let schema:String=function.get(0)?;let name:String=function.get(1)?;if function.get::<i64>(2)?!=1{denied=true;return Ok(None);}
                    let quote=|s:&str|format!("\"{}\"",s.replace('"',"\"\""));
                    let sql=if kind=="i"{format!("SELECT 1 FROM {}.{}($1::bigint::oid::regclass,{})",quote(&schema),quote(&name),!options.quick)}else{format!("SELECT msg FROM {}.{}($1::bigint::oid::regclass,false,true,'none') LIMIT 100",quote(&schema),quote(&name))};
                    tx.engine_check(&sql,after,&mut corrupt).map(Some)
                });
                match result {
                    Ok(Some(rows)) => {
                        checked += 1;
                        if kind != "i" && !rows.is_empty() {
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
                        } else {
                            emit(observer,"store.database.relations",Outcome::Pass,format!("relation OID {after}: {} completed with no reported inconsistency in {} ms; separate online observation interval",if kind=="i"{"bt_index_check(heapallindexed=true)"}else{"verify_heapam(check_toast=true,skip=none)"},started.elapsed().as_millis()));
                        }
                    }
                    Ok(None) => {
                        incomplete = true;
                        emit(
                            observer,
                            "store.database.relations",
                            Outcome::Unknown,
                            if denied {
                                format!("relation OID {after}: supported amcheck function exists but EXECUTE privilege is denied")
                            } else {
                                format!("relation OID {after}: supported amcheck function unavailable; no extension installed automatically")
                            },
                        );
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
