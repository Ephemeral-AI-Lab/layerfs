//! Actual profile7 SQL ownership and bounded fact pages; no physical/global RAM claim.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::support::TempDir;
    use layerfs_content::filesystem::inode::read::InodeTable;
    use layerfs_content::filesystem::rows::BindingAuthority;
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::{scope_for_seed, FilesystemRootId};
    use layerfs_content::ObjectId;
    use layerfs_storage::construction_state::{
        ScratchAuthority, ScratchDisposition, ScratchSession,
    };
    use layerfs_storage::StorageError;
    use rusqlite::Connection;
    fn begin(
        authority: &ScratchAuthority,
        capacity: FactCapacity,
    ) -> (ScratchSession, FactScope, FactScope) {
        let source = BindingAuthority::new().unwrap();
        let subject = GraphSubject::new(
            source.source_id(),
            scope_for_seed([0x65; 32]),
            Some(FilesystemRootId(ObjectId::from_bytes(&[0x41; 32]).unwrap())),
            1,
            GraphCapacity::default(),
        )
        .unwrap();
        let session = authority
            .begin_namespace(
                [0x78; 32],
                0,
                0,
                subject.clone(),
                AliasCapacity::new(512, subject.capacity().scratch_bytes(), 0).unwrap(),
                capacity,
            )
            .unwrap();
        let actual = FactSubject::new(
            subject,
            Some(InodeTable {
                root: ObjectId::from_bytes(&[0x73; 32]).unwrap(),
                root_serial: 1,
            }),
        )
        .unwrap();
        let base = FactScope::new(session.selection().clone(), actual.clone()).unwrap();
        let parent = FactScope::parents(session.selection().clone(), actual).unwrap();
        (session, base, parent)
    }
    #[test]
    fn selected_presence_absence_parent_bound_counts_literal_keys_and_cross128_pages() {
        let temp = TempDir::new("namespace_fact_pages");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, base, parent) =
            begin(&authority, FactCapacity::new(512, 512, 128 * 1024).unwrap());
        assert_eq!(session.profile().unwrap().version, 7);
        session.fact_bind(&base).unwrap();
        session.parent_bind(&parent).unwrap();
        assert_eq!(session.fact_get(&base, 2).unwrap(), None);
        for serials in (2..=386).collect::<Vec<_>>().chunks(128) {
            let rows: Vec<_> = serials
                .iter()
                .map(|s| BaseFact {
                    serial: *s,
                    value: None,
                })
                .collect();
            session.fact_insert(&base, &rows).unwrap();
            session.parent_insert(&parent, serials).unwrap();
        }
        session.parent_close_declarations(&parent).unwrap();
        for serials in (2..=386)
            .filter(|s| s % 2 == 0)
            .collect::<Vec<_>>()
            .chunks(128)
        {
            session.parent_mark_bound(&parent, serials).unwrap();
        }
        session.parent_mark_bound(&parent, &[9999, 2, 2]).unwrap();
        assert_eq!(
            session.fact_get(&base, 2).unwrap(),
            Some(BaseFact {
                serial: 2,
                value: None
            })
        );
        let path = authority.status().unwrap()[0].path.clone();
        let inspect = Connection::open(&path).unwrap();
        let mut literal = [0u8; 25];
        literal[..8].copy_from_slice(&session.selection().token().to_be_bytes());
        literal[8..16].copy_from_slice(&4u64.to_be_bytes());
        literal[16] = 17;
        literal[17..].copy_from_slice(&2u64.to_be_bytes());
        let value: Vec<u8> = inspect
            .query_row(
                "SELECT value FROM base_facts WHERE key=?1",
                [literal.as_slice()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, vec![0; 74]);
        let plan: String = inspect
            .query_row(
                "EXPLAIN QUERY PLAN SELECT value FROM base_facts WHERE key=?1",
                [literal.as_slice()],
                |r| r.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("SEARCH") && plan.contains("PRIMARY KEY"),
            "{plan}"
        );
        drop(inspect);
        let parent_seal = session.parent_seal(&parent).unwrap();
        assert_eq!(
            (
                parent_seal.facts.records,
                parent_seal.bound,
                parent_seal.excluded().unwrap()
            ),
            (385, 193, 192)
        );
        assert_eq!(
            session.parent_get(&parent_seal, 3).unwrap(),
            Some(ParentFact {
                serial: 3,
                bound: false
            })
        );
        assert_eq!(session.parent_get(&parent_seal, 9999).unwrap(), None);
        let seal = session.fact_seal(&base).unwrap();
        assert_eq!(
            (seal.records, seal.bytes, seal.maximum),
            (385, 40425, Some(386))
        );
        let memory = session.fact_memory(&base).unwrap();
        let before = memory.reserved_bytes();
        let held = session.fact_page(&seal, None, 128, 65536).unwrap();
        assert_eq!(held.records().len(), 128);
        assert!(!held.eof);
        assert!(memory.reserved_bytes() > before);
        drop(held);
        assert_eq!(memory.reserved_bytes(), before);
        let mut after = None;
        let mut seen = vec![];
        let mut pages = 0;
        loop {
            let page = session.fact_page(&seal, after, 128, 65536).unwrap();
            seen.extend(page.records().iter().map(|r| r.serial));
            after = page.last;
            pages += 1;
            if page.eof {
                break;
            }
        }
        assert_eq!(seen, (2..=386).collect::<Vec<_>>());
        assert_eq!(pages, 4);
        let end = session.fact_page(&seal, after, 1, 301).unwrap();
        assert!(end.records().is_empty() && end.eof);
        drop(end);
        session.fact_retire(&seal).unwrap();
        session.parent_retire(&parent_seal).unwrap();
        let inspect = Connection::open(&path).unwrap();
        assert_eq!(inspect.query_row("SELECT (SELECT COUNT(*) FROM base_facts)+(SELECT COUNT(*) FROM parent_eligibility)",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        drop(inspect);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        assert!(!path.exists());
    }
    #[test]
    fn combined_fact_parent_limit_refuses_before_second_insert_and_preserves_prior_metadata() {
        let temp = TempDir::new("namespace_fact_capacity");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, base, parent) = begin(
            &authority,
            FactCapacity::new(8, 8, 1609 + 105 + 32).unwrap(),
        );
        session.fact_bind(&base).unwrap();
        session.parent_bind(&parent).unwrap();
        session
            .fact_insert(
                &base,
                &[BaseFact {
                    serial: 2,
                    value: None,
                }],
            )
            .unwrap();
        session.parent_insert(&parent, &[2]).unwrap();
        assert!(session
            .fact_insert(
                &base,
                &[BaseFact {
                    serial: 3,
                    value: None
                }]
            )
            .is_err());
        let path = authority.status().unwrap()[0].path.clone();
        let inspect = Connection::open(&path).unwrap();
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM base_facts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM parent_eligibility", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(inspect);
        session.release().unwrap();
    }
    #[test]
    fn foreign_actual_table_refusal_does_not_consume_original_subject_and_empty_eof_is_header_only()
    {
        let temp = TempDir::new("namespace_fact_scope");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, base, parent) =
            begin(&authority, FactCapacity::new(8, 8, 64 * 1024).unwrap());
        session.fact_bind(&base).unwrap();
        session.parent_bind(&parent).unwrap();
        let altered = FactSubject::new(
            base.subject().selected().clone(),
            Some(InodeTable {
                root: ObjectId::from_bytes(&[0x74; 32]).unwrap(),
                root_serial: 1,
            }),
        )
        .unwrap();
        let foreign = FactScope::new(session.selection().clone(), altered).unwrap();
        assert!(session.fact_get(&foreign, 2).is_err());
        assert_eq!(session.fact_get(&base, 2).unwrap(), None);
        session.parent_close_declarations(&parent).unwrap();
        let parents = session.parent_seal(&parent).unwrap();
        let parent_eof = session.parent_page(&parents, None, 1, 301).unwrap();
        assert!(parent_eof.eof && parent_eof.records().is_empty());
        drop(parent_eof);
        let seal = session.fact_seal(&base).unwrap();
        let eof = session.fact_page(&seal, None, 1, 301).unwrap();
        assert!(eof.eof && eof.records().is_empty());
        drop(eof);
        session.fact_retire(&seal).unwrap();
        session.parent_retire(&parents).unwrap();
        session.release().unwrap();
    }
    #[test]
    fn real_sql_commit_unknown_retains_proposed_exact_base_facts_and_denies_cleanup() {
        let temp = TempDir::new("namespace_fact_unknown");
        let parent_path = temp.path().to_path_buf();
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, base, _) = begin(&authority, FactCapacity::new(8, 8, 64 * 1024).unwrap());
        session.fact_bind(&base).unwrap();
        session
            .fact_insert(
                &base,
                &[BaseFact {
                    serial: 2,
                    value: None,
                }],
            )
            .unwrap();
        let token = session.selection().token();
        let path = authority.status().unwrap()[0].path.clone();
        let held = Connection::open(&path).unwrap();
        held.execute_batch("BEGIN").unwrap();
        assert_eq!(
            held.query_row(
                "SELECT records FROM fact_owner WHERE table_id=17",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let error = session
            .fact_insert(
                &base,
                &[
                    BaseFact {
                        serial: 3,
                        value: None,
                    },
                    BaseFact {
                        serial: 4,
                        value: None,
                    },
                ],
            )
            .unwrap_err();
        assert!(
            matches!(error, StorageError::UnknownOutcome { .. }),
            "{error:?}"
        );
        assert!(session.release().is_err());
        let status = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|s| s.token == token)
            .unwrap();
        assert_eq!(status.disposition, ScratchDisposition::Unknown);
        let failure = status.failure.unwrap();
        assert!(
            failure.contains("fact attempt insert")
                && failure.contains("serial: 3")
                && failure.contains("serial: 4"),
            "{failure}"
        );
        held.execute_batch("ROLLBACK").unwrap();
        drop(held);
        drop(session);
        assert!(authority.release_retained(token).is_err());
        assert!(path.exists());
        std::mem::forget(temp);
        println!(
            "profile7 Unknown fixture retained at {}",
            parent_path.display()
        );
    }
}
