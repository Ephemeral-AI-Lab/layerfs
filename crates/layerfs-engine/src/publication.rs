use super::refs::{read_ref_on_connection, validate_ref_name, RefState};
use super::*;
use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::InodeId;
use layerfs_core::namespace_codec::decode_namespace_root;

pub struct Publication<'a> {
    engine: &'a Engine,
    connection: ConnectionGuard<'a>,
    name: String,
    expected: Option<RefState>,
    verified_retained_root: Option<ObjectId>,
    active: bool,
}

impl Engine {
    pub fn begin_publication<'a>(
        &'a self,
        expected: Option<&RefState>,
        name: &str,
    ) -> EngineResult<Publication<'a>> {
        validate_ref_name(name)?;
        if expected.is_some_and(|state| state.name != name) {
            return Err(EngineError::InvalidRecord("expected ref name"));
        }
        let mut connection = self.lock_write_connection()?;
        self.mark_statement()?;
        if !connection.transaction {
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
        }
        self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;
        let actual = read_ref_on_connection(&connection, name)?;
        if actual.as_ref() != expected {
            let store_id = store_id(&connection)?;
            let discarded = finalize_rollback(self, &mut connection);
            if discarded {
                let observed = super::read_ref_reconcile_readonly(&self.path, name, store_id);
                if observed.as_ref().ok() != Some(&actual) {
                    return Err(EngineError::AmbiguousDurability);
                }
                restore_primary(self, &mut connection, store_id, name, &actual)?;
            }
            return Err(EngineError::PublicationConflict);
        }
        Ok(Publication {
            engine: self,
            connection,
            name: name.to_owned(),
            expected: expected.cloned(),
            verified_retained_root: None,
            active: true,
        })
    }
}

impl Publication<'_> {
    pub fn allocate_inode_id(&mut self) -> EngineResult<InodeId> {
        self.ensure_active()?;
        self.engine.mark_statement()?;
        let (store_id, serial) = self
            .connection
            .query_row(
                "SELECT store_id, next_inode_serial FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(map_sqlite_error)?;
        let serial =
            u64::try_from(serial).map_err(|_| EngineError::InvalidRecord("inode serial"))?;
        let next = serial.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        self.connection
            .execute(
                "UPDATE layerfs_authority SET next_inode_serial = ?1 WHERE authority_id = 1",
                params![i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?],
            )
            .map_err(map_sqlite_error)?;
        Ok(InodeId::allocate(
            ObjectId::from_bytes(&store_id)?.to_bytes(),
            serial,
        ))
    }

    pub fn put_object(&mut self, canonical: &[u8]) -> EngineResult<ObjectId> {
        self.ensure_active()?;
        let id = ObjectId::for_bytes(canonical);
        put_object_on_connection(self.engine, &self.connection, id, canonical)?;
        Ok(id)
    }

    pub fn publish_namespace(mut self, canonical: &[u8]) -> EngineResult<RefState> {
        decode_namespace_root(canonical).map_err(EngineError::Core)?;
        let root = self.put_object(canonical)?;
        self.commit_ref(root)
    }

    pub(crate) fn retain_existing_root(&mut self, root: ObjectId) -> EngineResult<()> {
        self.ensure_active()?;
        let retained = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_retained_roots WHERE root_id = ?1)",
                params![root.as_bytes().as_slice()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if !retained {
            return Err(EngineError::MissingRoot(root));
        }
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            root,
            |_, bytes| {
                decode_namespace_root(bytes)
                    .map(|_| ())
                    .map_err(EngineError::Core)
            },
        )?;
        self.verified_retained_root = Some(root);
        Ok(())
    }

    pub(crate) fn commit_ref(mut self, root: ObjectId) -> EngineResult<RefState> {
        self.ensure_active()?;
        if let Some(expected) = &self.expected {
            if expected.root == root {
                let expected = expected.clone();
                let store_id = store_id(&self.connection)?;
                let discarded = finalize_rollback(self.engine, &mut self.connection);
                self.active = false;
                if discarded {
                    let observed =
                        super::read_ref_reconcile_readonly(&self.engine.path, &self.name, store_id);
                    if observed.as_ref().ok() != Some(&Some(expected.clone())) {
                        return Err(EngineError::AmbiguousDurability);
                    }
                    restore_primary(
                        self.engine,
                        &mut self.connection,
                        store_id,
                        &self.name,
                        &Some(expected.clone()),
                    )?;
                }
                return Ok(expected);
            }
        }
        if self.engine.mode == super::integrity::IntegrityMode::Verified
            && self.verified_retained_root != Some(root)
        {
            let observation =
                super::integrity::verify_root(&self.connection, &self.engine.path, root)?;
            self.engine.bump(|counters| {
                checked_add(&mut counters.root_verifications, 1)?;
                checked_add(&mut counters.root_verification_objects, observation.objects)?;
                checked_add(&mut counters.root_verification_bytes, observation.bytes)
            })?;
        }
        let generation = self.expected.as_ref().map_or(Ok(0), |state| {
            state
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)
        })?;
        self.engine.mark_statement()?;
        self.connection.execute("INSERT INTO layerfs_retained_roots (root_id) VALUES (?1) ON CONFLICT(root_id) DO NOTHING", params![root.as_bytes().as_slice()]).map_err(map_sqlite_error)?;
        if self.expected.is_some() {
            self.connection
                .execute(
                    "UPDATE layerfs_refs SET generation = ?1, root_id = ?2 WHERE name = ?3",
                    params![
                        i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                        root.as_bytes().as_slice(),
                        &self.name
                    ],
                )
                .map_err(map_sqlite_error)?;
        } else {
            self.connection
                .execute(
                    "INSERT INTO layerfs_refs (name, generation, root_id) VALUES (?1, 0, ?2)",
                    params![&self.name, root.as_bytes().as_slice()],
                )
                .map_err(map_sqlite_error)?;
        }
        self.engine.mark_statement()?;
        if self.engine.mode == super::integrity::IntegrityMode::TrustedLocalDev {
            self.connection
                .execute(
                    "UPDATE layerfs_authority SET trusted_history = 1 WHERE authority_id = 1",
                    [],
                )
                .map_err(map_sqlite_error)?;
        }
        let store_id = store_id(&self.connection)?;
        match self.engine.commit_dispatch.commit(&self.connection) {
            Ok(()) => {
                self.active = false;
                self.connection.transaction = false;
                self.engine
                    .bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(RefState {
                    name: self.name.clone(),
                    generation,
                    root,
                })
            }
            Err(error) => {
                let _ = self.engine.note_sqlite_error(&error);
                finalize_rollback(self.engine, &mut self.connection);
                self.connection.guard.take();
                self.active = false;
                let observed =
                    super::read_ref_reconcile_readonly(&self.engine.path, &self.name, store_id);
                match observed {
                    Ok(Some(state)) if state.generation == generation && state.root == root => {
                        restore_primary(
                            self.engine,
                            &mut self.connection,
                            store_id,
                            &self.name,
                            &Some(state.clone()),
                        )?;
                        Ok(state)
                    }
                    Ok(observed) if observed == self.expected => {
                        restore_primary(
                            self.engine,
                            &mut self.connection,
                            store_id,
                            &self.name,
                            &observed,
                        )?;
                        Err(map_sqlite_error(error))
                    }
                    _ => Err(EngineError::AmbiguousDurability),
                }
            }
        }
    }

    fn ensure_active(&self) -> EngineResult<()> {
        if self.active {
            Ok(())
        } else {
            Err(EngineError::InvalidTransaction)
        }
    }
}

impl ObjectStore for Publication<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        with_authenticated_canonical_on_connection(self.engine, &self.connection, id, |_, bytes| {
            Ok(bytes.to_vec())
        })
        .map_err(core_store_error)
    }

    fn put(&mut self, canonical: &[u8]) -> Result<ObjectId, CoreError> {
        self.put_object(canonical).map_err(core_store_error)
    }
}

impl Drop for Publication<'_> {
    fn drop(&mut self) {
        if self.active {
            let store_id = store_id(&self.connection).ok();
            let discarded = finalize_rollback(self.engine, &mut self.connection);
            if discarded {
                if let Some(store_id) = store_id {
                    let observed =
                        super::read_ref_reconcile_readonly(&self.engine.path, &self.name, store_id);
                    if let Ok(observed) = observed {
                        if observed == self.expected {
                            let _ = restore_primary(
                                self.engine,
                                &mut self.connection,
                                store_id,
                                &self.name,
                                &observed,
                            );
                        }
                    }
                }
            }
            self.active = false;
        }
    }
}

fn store_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(map_sqlite_error)?
        .try_into()
        .map_err(|_| EngineError::InvalidRecord("StoreId"))
}

fn finalize_rollback(engine: &Engine, connection: &mut ConnectionGuard<'_>) -> bool {
    let active = !connection.is_autocommit();
    let failed = active && connection.execute_batch("ROLLBACK").is_err();
    connection.transaction = false;
    if failed {
        connection.guard.take();
    } else if active {
        engine.bump_best_effort(|counters| checked_add(&mut counters.transactions_rolled_back, 1));
    }
    failed
}

fn restore_primary(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    store_id: [u8; 32],
    ref_name: &str,
    expected_ref: &Option<RefState>,
) -> EngineResult<()> {
    let reopened = super::reopen_store_primary(&engine.path, store_id, ref_name, expected_ref)
        .map_err(|_| EngineError::AmbiguousDurability)?;
    *connection.guard = Some(reopened);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::namespace::NamespaceRootV1;
    use layerfs_core::namespace_codec::{encode_namespace_root, profile_id};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    const CHILD_TEST: &str = "publication::tests::publication_crash_child";

    struct LostCommitAcknowledgement;
    impl CommitDispatch for LostCommitAcknowledgement {
        fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
            connection.execute_batch("COMMIT")?;
            Err(rusqlite::Error::InvalidQuery)
        }
    }

    fn root(label: &str) -> Vec<u8> {
        encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([0x71; 32], 0),
            inode_table_root: ObjectId::for_bytes(label.as_bytes()),
        })
        .unwrap()
    }

    #[test]
    fn publication_crash_child() {
        let Ok(cut) = std::env::var("LAYERFS_PUBLICATION_CRASH_CUT") else {
            return;
        };
        let path = PathBuf::from(std::env::var_os("LAYERFS_PUBLICATION_CRASH_STORE").unwrap());
        let mut engine =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        if cut == "T6-requested" {
            engine.commit_dispatch = std::sync::Arc::new(LostCommitAcknowledgement);
        }
        let prior = engine.read_ref("main").unwrap();
        if cut == "T6-different" {
            let replacement_path = path.with_extension("t6-replacement");
            let replacement = Engine::open_with_mode(
                &replacement_path,
                integrity::IntegrityMode::TrustedLocalDev,
            )
            .unwrap();
            replacement
                .begin_publication(None, "main")
                .unwrap()
                .publish_namespace(&root("replacement"))
                .unwrap();
        }
        if cut == "T0" {
            std::process::exit(80);
        }
        let publication = engine.begin_publication(prior.as_ref(), "main").unwrap();
        if cut == "T1" {
            std::process::exit(81);
        }
        if matches!(cut.as_str(), "T2" | "T3" | "T4") {
            let wanted = match cut.as_str() {
                "T2" => "layerfs_objects",
                "T3" => "layerfs_retained_roots",
                "T4" => "layerfs_refs",
                _ => unreachable!(),
            };
            publication
                .connection
                .update_hook(Some(
                    move |_: rusqlite::hooks::Action, _: &str, table: &str, _: i64| {
                        if table == wanted {
                            std::process::exit(82);
                        }
                    },
                ))
                .unwrap();
        }
        if cut == "T6-prior" {
            publication.connection.commit_hook(Some(|| true)).unwrap();
            assert!(publication.publish_namespace(&root(&cut)).is_err());
            return;
        }
        if matches!(cut.as_str(), "T6-different" | "T6-missing") {
            let hook_path = path.clone();
            let different = cut == "T6-different";
            publication
                .connection
                .commit_hook(Some(move || {
                    fs::rename(&hook_path, hook_path.with_extension("t6-old")).unwrap();
                    if different {
                        fs::rename(hook_path.with_extension("t6-replacement"), &hook_path).unwrap();
                    }
                    true
                }))
                .unwrap();
            assert!(matches!(
                publication.publish_namespace(&root(&cut)),
                Err(EngineError::AmbiguousDurability)
            ));
            assert!(matches!(
                engine.read_ref("main"),
                Err(EngineError::AmbiguousDurability)
            ));
            return;
        }
        publication.publish_namespace(&root(&cut)).unwrap();
    }

    #[test]
    fn publication_t0_through_t6_survive_real_process_exit_and_reopen() {
        for cut in [
            "T0",
            "T1",
            "T2",
            "T3",
            "T4",
            "T5",
            "T6-prior",
            "T6-requested",
            "T6-different",
            "T6-missing",
        ] {
            let path = std::env::temp_dir().join(format!(
                "layerfs-publication-crash-{cut}-{}-{}.sqlite",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let engine =
                Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
            let prior = engine
                .begin_publication(None, "main")
                .unwrap()
                .publish_namespace(&root("prior"))
                .unwrap();
            let store_id = engine.store_id().unwrap();
            drop(engine);

            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CHILD_TEST])
                .env("LAYERFS_PUBLICATION_CRASH_CUT", cut)
                .env("LAYERFS_PUBLICATION_CRASH_STORE", &path)
                .status()
                .unwrap();
            assert_eq!(
                status.success(),
                matches!(
                    cut,
                    "T5" | "T6-prior" | "T6-requested" | "T6-different" | "T6-missing"
                )
            );

            if cut == "T6-different" {
                let replacement =
                    Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev)
                        .unwrap();
                assert_ne!(replacement.store_id().unwrap(), store_id);
                let state = replacement.read_ref("main").unwrap().unwrap();
                decode_namespace_root(
                    &replacement.load_object(state.root).unwrap().canonical_bytes,
                )
                .unwrap();
                drop(replacement);
                fs::remove_file(&path).unwrap();
                fs::remove_file(path.with_extension("t6-old")).unwrap();
                continue;
            }
            if cut == "T6-missing" {
                assert!(!path.exists());
                fs::rename(path.with_extension("t6-old"), &path).unwrap();
            }

            let reopened =
                Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
            let observed = reopened.read_ref("main").unwrap().unwrap();
            decode_namespace_root(&reopened.load_object(observed.root).unwrap().canonical_bytes)
                .unwrap();
            if matches!(cut, "T5" | "T6-requested") {
                assert_ne!(observed.root, prior.root, "{cut} lost requested root");
                assert_eq!(observed.generation, prior.generation + 1);
            } else {
                assert_eq!(observed, prior, "{cut} exposed partial publication");
            }
            drop(reopened);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn restore_primary_rejects_same_store_id_stale_path_replacement() {
        let path = std::env::temp_dir().join(format!(
            "layerfs-publication-stale-reopen-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        let prior = engine
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&root("prior"))
            .unwrap();
        drop(engine);
        let stale = path.with_extension("stale");
        fs::copy(&path, &stale).unwrap();

        let engine =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        let requested = engine
            .begin_publication(Some(&prior), "main")
            .unwrap()
            .publish_namespace(&root("requested"))
            .unwrap();
        let store_id = engine.store_id().unwrap();
        let saved = path.with_extension("requested");
        fs::rename(&path, &saved).unwrap();
        fs::rename(&stale, &path).unwrap();

        let mut connection = engine.lock_connection().unwrap();
        connection.guard.take();
        assert!(matches!(
            restore_primary(&engine, &mut connection, store_id, "main", &Some(requested)),
            Err(EngineError::AmbiguousDurability)
        ));
        assert!(connection.guard.is_none());
        drop(connection);
        assert!(matches!(
            engine.read_ref("main"),
            Err(EngineError::AmbiguousDurability)
        ));
        drop(engine);
        fs::remove_file(path).unwrap();
        fs::remove_file(saved).unwrap();
    }
}
