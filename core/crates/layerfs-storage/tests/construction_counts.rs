//! Actual profile8 component custody and indexed row arithmetic; no native/global RAM claim.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_oracle.rs"]
#[allow(dead_code)]
mod graph_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
#[allow(dead_code)]
mod roots_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::{fixture::*, support::TempDir};
    use layerfs_content::filesystem::inode::read::InodeTable;
    use layerfs_content::filesystem::references::record::Row;
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_content::ObjectId;
    use layerfs_storage::construction_state::{
        ScratchAuthority, ScratchDisposition, ScratchSession,
    };
    fn begin(
        authority: &ScratchAuthority,
        source: &impl layerfs_content::filesystem::rows::BindingRows,
        update: bool,
        capacity: CanonicalCapacity,
    ) -> (
        ScratchSession,
        GraphConstructionScopes,
        FactScope,
        ParentSeal,
        CanonicalScope,
    ) {
        let subject = subject(source, update, DEFAULT);
        let mut session = authority
            .begin_canonical(
                [0x91; 32],
                0,
                0,
                subject.clone(),
                AliasCapacity::new(1024, DEFAULT, 0).unwrap(),
                FactCapacity::new(1024, 1024, DEFAULT).unwrap(),
                capacity,
            )
            .unwrap();
        let selected = selected(&session);
        let actual = FactSubject::new(
            subject,
            update.then_some(InodeTable {
                root: ObjectId::from_bytes(&[0x72; 32]).unwrap(),
                root_serial: 1,
            }),
        )
        .unwrap();
        let base = FactScope::new(session.selection().clone(), actual.clone()).unwrap();
        let parent = FactScope::parents(session.selection().clone(), actual.clone()).unwrap();
        session.fact_bind(&base).unwrap();
        session.parent_bind(&parent).unwrap();
        session.parent_close_declarations(&parent).unwrap();
        let parents = session.parent_seal(&parent).unwrap();
        let members = session
            .site_close_membership(
                &SiteBirthLedger::new(selected.sites().clone())
                    .unwrap()
                    .seal(),
            )
            .unwrap();
        session.alias_begin(&members, None).unwrap();
        let aliases = session.alias_finish(&members).unwrap();
        session.alias_retire(&aliases).unwrap();
        let sites = session.site_final_seal(&members).unwrap();
        session.site_retire(&sites).unwrap();
        if !update {
            session.graph_root(selected.graph()).unwrap();
            let root = session.graph_unexpanded(selected.graph()).unwrap().unwrap();
            assert_eq!(root.key().serial(), 1);
            session.graph_expanded(selected.graph(), &root).unwrap();
        }
        assert!(session
            .graph_unexpanded(selected.graph())
            .unwrap()
            .is_none());
        let adjacency = session.graph_seal(selected.graph()).unwrap();
        if update {
            if let Err(mapped) = solve_effective_graph(
                &mut session.adapter(),
                &adjacency,
                &mut ValidationGraphWork::default(),
            ) {
                panic!(
                    "Update SCC mapped={mapped:?}; original={:?}",
                    session.take_failure()
                );
            }
        }
        let proof = session.graph_finish(&adjacency).unwrap();
        session.graph_retire(&proof).unwrap();
        let counts = CanonicalScope::counts(session.selection().clone(), actual).unwrap();
        session.count_memory(&counts).unwrap();
        session.count_begin(&counts).unwrap();
        (session, selected, base, parents, counts)
    }
    fn capacity() -> CanonicalCapacity {
        CanonicalCapacity::new(1024, 1024, 1024, 1024, DEFAULT).unwrap()
    }
    fn finish(
        session: &mut ScratchSession,
        base: &FactScope,
        parents: &ParentSeal,
        final_seal: &CountSeal,
        zeros: Option<&ZeroSeal>,
    ) {
        let facts = session.fact_seal(base).unwrap();
        session.fact_retire(&facts).unwrap();
        session.count_retire(final_seal, zeros).unwrap();
        session.parent_retire(parents).unwrap();
    }
    #[test]
    fn fresh_declared_membership_exact_rows_pages_lastowner_and_bounded_retirement() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("canonical_counts_fresh");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, _selected, base, parents, scope) =
                begin(&authority, source, false, capacity());
            let mut expected = CountLedger::new(scope.clone(), CountEpoch::Final).unwrap();
            for serial in 2..=258 {
                let row = CountRecord {
                    row: Row::Count {
                        serial,
                        value: None,
                        count: 0,
                    },
                    touched: false,
                };
                session.count_cas(&scope, None, &row).unwrap();
                expected.append(&[row]).unwrap();
            }
            let seal = session.count_seal(&scope, CountEpoch::Final).unwrap();
            assert_eq!(seal, expected.seal().unwrap());
            assert_eq!((seal.records, seal.touched), (257, 0));
            let memory = session.count_memory(&scope).unwrap();
            let before = memory.reserved_bytes();
            let held = session.count_page(&seal, None, 128, 65536).unwrap();
            assert_eq!(held.records().len(), 128);
            assert_eq!(memory.reserved_bytes(), before + held.working_bytes());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            let mut key = [0; 25];
            key[..8].copy_from_slice(&session.selection().token().to_be_bytes());
            key[8..16].copy_from_slice(&6u64.to_be_bytes());
            key[16] = 19;
            key[17..].copy_from_slice(&2u64.to_be_bytes());
            let value: Vec<u8> = db
                .query_row(
                    "SELECT value FROM canonical_counts WHERE key=?1",
                    [key.as_slice()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(value.len(), 97);
            assert_eq!(value[96], 0);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM canonical_counts"), 257);
            drop(db);
            drop(held);
            assert_eq!(memory.reserved_bytes(), before);
            let mut ledger = CountLedger::new(scope.clone(), CountEpoch::Final).unwrap();
            let mut after = None;
            let mut pages = 0;
            loop {
                let page = session.count_page(&seal, after, 128, 65536).unwrap();
                ledger.append(page.records()).unwrap();
                after = page.last;
                pages += 1;
                if page.eof {
                    break;
                }
            }
            assert_eq!(pages, 3);
            assert_eq!(ledger.seal().unwrap(), seal);
            let eof = session
                .count_page(&seal, after, 1, COUNT_PAGE_HEADER_BYTES)
                .unwrap();
            assert!(eof.eof && eof.records().is_empty());
            drop(eof);
            finish(&mut session, &base, &parents, &seal, None);
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM canonical_counts"), 0);
            assert_eq!(scalar(&db, "SELECT stage FROM count_owner"), 6);
            drop(db);
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }
    #[test]
    fn update_seed_transcript_fifo_current_retention_external_depth_and_full_names() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("canonical_release_frontier");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, _selected, base, parents, scope) =
                begin(&authority, source, true, capacity());
            let facts: Vec<_> = (2..=258)
                .map(|serial| BaseFact {
                    serial,
                    value: None,
                })
                .collect();
            for rows in facts.chunks(128) {
                session.fact_insert(&base, rows).unwrap();
            }
            for fact in &facts {
                session
                    .count_cas(
                        &scope,
                        None,
                        &CountRecord {
                            row: Row::Effect {
                                serial: fact.serial,
                                value: None,
                                delta: 0,
                            },
                            touched: true,
                        },
                    )
                    .unwrap();
            }
            let effects = session.count_seal(&scope, CountEpoch::Effects).unwrap();
            for rows in facts.chunks(128) {
                session.zero_append(&effects, rows).unwrap();
            }
            let zeros = session.zero_seal(&effects).unwrap();
            session.count_resume(&zeros).unwrap();
            let jobs = scope.jobs().unwrap();
            session.release_begin(&jobs, &zeros).unwrap();
            for rows in facts.chunks(128) {
                session.release_seed(&jobs, rows).unwrap();
            }
            session.release_close_seeds(&jobs).unwrap();
            let path = status(&authority, session.selection().token()).path;
            for serial in 2..=258 {
                let job = session.release_take(&jobs).unwrap().unwrap();
                assert_eq!(job.serial, serial);
                if serial == 2 {
                    let db = external(&path);
                    assert_eq!(scalar(&db, "SELECT pending FROM release_owner"), 256);
                    assert_eq!(
                        scalar(&db, "SELECT current_key IS NOT NULL FROM release_owner"),
                        1
                    );
                    drop(db);
                }
                session
                    .release_complete_job(
                        &jobs,
                        &job,
                        Some(ObjectId::from_bytes(&[0x63; 32]).unwrap()),
                    )
                    .unwrap();
            }
            assert_eq!(session.release_take(&jobs).unwrap(), None);
            let name = layerfs_content::filesystem::PathName::new(&"x".repeat(255)).unwrap();
            for depth in (1..=257).rev() {
                let frame = session.release_frame(&jobs).unwrap().unwrap();
                assert_eq!(frame.depth, depth);
                let next = ReleaseFrame {
                    after: Some(ReleaseName::from_path_name(&name)),
                    finished: false,
                    ..frame
                };
                session.release_advance(&jobs, &frame, &next, &[]).unwrap();
                let eof = ReleaseFrame {
                    after: None,
                    finished: true,
                    ..next
                };
                session.release_advance(&jobs, &next, &eof, &[]).unwrap();
                session.release_pop(&jobs, &eof).unwrap();
            }
            assert_eq!(session.release_frame(&jobs).unwrap(), None);
            let release = session.release_seal(&jobs).unwrap();
            assert_eq!(
                (release.jobs, release.directories, release.maximum_depth),
                (257, 257, 257)
            );
            session.release_retire(&release).unwrap();
            let final_seal = session.count_seal(&scope, CountEpoch::Final).unwrap();
            finish(&mut session, &base, &parents, &final_seal, Some(&zeros));
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }
    #[test]
    fn count_record_limit_refuses_before_sql_and_keeps_original_capsule() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("canonical_count_capacity");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, _, _, _, scope) = begin(
                &authority,
                source,
                false,
                CanonicalCapacity::new(1, 0, 0, 0, DEFAULT).unwrap(),
            );
            let row = CountRecord {
                row: Row::Count {
                    serial: 2,
                    value: None,
                    count: 0,
                },
                touched: false,
            };
            session.count_cas(&scope, None, &row).unwrap();
            let second = CountRecord {
                row: Row::Count {
                    serial: 3,
                    value: None,
                    count: 0,
                },
                touched: false,
            };
            assert!(session.count_cas(&scope, None, &second).is_err());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM canonical_counts"), 1);
            assert_eq!(scalar(&db, "SELECT records FROM count_owner"), 1);
            drop(db);
            session.release().unwrap();
        });
    }
    #[test]
    fn real_count_commit_unknown_retains_before_proposed_and_refuses_cleanup() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("canonical_count_unknown");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, _, _, _, scope) = begin(&authority, source, false, capacity());
            let row = CountRecord {
                row: Row::Count {
                    serial: 2,
                    value: None,
                    count: 0,
                },
                touched: false,
            };
            session.count_cas(&scope, None, &row).unwrap();
            let token = session.selection().token();
            let path = status(&authority, token).path;
            let reader = external(&path);
            reader.execute_batch("BEGIN").unwrap();
            assert_eq!(scalar(&reader, "SELECT records FROM count_owner"), 1);
            let proposed = CountRecord {
                row: Row::Count {
                    serial: 2,
                    value: None,
                    count: 1,
                },
                touched: true,
            };
            let error = session.count_cas(&scope, Some(row), &proposed).unwrap_err();
            assert!(
                matches!(error, layerfs_storage::StorageError::UnknownOutcome { .. }),
                "{error:?}"
            );
            assert!(session.release().is_err());
            let unknown = status(&authority, token);
            assert_eq!(unknown.disposition, ScratchDisposition::Unknown);
            assert!(unknown.failure.as_deref().unwrap().contains("CAS"));
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            reader.execute_batch("ROLLBACK").unwrap();
            drop(reader);
            drop(session);
            assert!(path.exists());
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert!(authority.release_retained(token).is_err());
            std::mem::forget(temp);
        });
    }
}
