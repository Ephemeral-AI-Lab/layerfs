//! Actual profile5 SQLite/native frontier; logical component proofs, no speed/RSS claim.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::support::TempDir;
    use layerfs_content::filesystem::rows::BindingAuthority;
    use layerfs_content::filesystem::scope_for_seed;
    use layerfs_content::filesystem::state::*;
    use layerfs_storage::construction_state::{
        ScratchAuthority, ScratchDisposition, ScratchSession,
    };
    use layerfs_storage::StorageError;
    use rusqlite::Connection;
    fn begin(
        authority: &ScratchAuthority,
        capacity: AliasCapacity,
    ) -> (ScratchSession, GraphConstructionScopes, SiteMembership) {
        let source = BindingAuthority::new().unwrap();
        let subject = GraphSubject::new(
            source.source_id(),
            scope_for_seed([0x82; 32]),
            None,
            1,
            GraphCapacity::default(),
        )
        .unwrap();
        let mut session = authority
            .begin_alias_graph([0x43; 32], 0, 0, subject.clone(), capacity)
            .unwrap();
        let scopes = GraphConstructionScopes::new(session.selection().clone(), subject).unwrap();
        let members = session
            .site_close_membership(&SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal())
            .unwrap();
        (session, scopes, members)
    }
    fn complete(session: &mut ScratchSession, m: &SiteMembership, c: AliasCurrent) {
        let initial = AliasProgress::initial();
        let sites = AliasProgress::base(None);
        let done = AliasProgress::complete();
        session.alias_advance(m, c, &initial, &sites).unwrap();
        session.alias_advance(m, c, &sites, &done).unwrap();
        session.alias_complete(m, c).unwrap();
    }
    #[test]
    fn native_priority_exact_sql_projection_and_cross_128_retirement_with_live_sites() {
        let temp = TempDir::new("aliases_priority");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, _, members) =
            begin(&authority, AliasCapacity::new(600, 128 * 1024, 0).unwrap());
        assert_eq!(session.profile().unwrap().version, 5);
        let path = authority.status().unwrap()[0].path.clone();
        session.alias_begin(&members, Some(1)).unwrap();
        let root = session.alias_take(&members).unwrap().unwrap();
        for children in (2..=385).collect::<Vec<_>>().chunks(128) {
            session.alias_enqueue(&members, children).unwrap();
        }
        session.alias_enqueue(&members, &[2, 3, 2]).unwrap();
        complete(&mut session, &members, root);
        let inspect = Connection::open(&path).unwrap();
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_facts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            385
        );
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            384
        );
        let mut literal_fact_key = [0u8; 25];
        literal_fact_key[..8].copy_from_slice(&session.selection().token().to_be_bytes());
        literal_fact_key[8..16].copy_from_slice(&1u64.to_be_bytes());
        literal_fact_key[16] = 6;
        literal_fact_key[17..].copy_from_slice(&2u64.to_be_bytes());
        let fact: (i64, u8) = inspect
            .query_row(
                "SELECT sequence,status FROM alias_facts WHERE key=?1",
                [literal_fact_key.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(fact, (388, 1));
        let mut literal_job_key = literal_fact_key;
        literal_job_key[16] = 7;
        literal_job_key[17..].copy_from_slice(&388u64.to_be_bytes());
        assert_eq!(
            inspect
                .query_row(
                    "SELECT serial FROM alias_jobs WHERE key=?1",
                    [literal_job_key.as_slice()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        let plan: String = inspect
            .query_row(
                "EXPLAIN QUERY PLAN SELECT sequence,status FROM alias_facts WHERE key=?1",
                [vec![0u8; 25]],
                |r| r.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("SEARCH") && plan.contains("PRIMARY KEY"),
            "{plan}"
        );
        drop(inspect);
        let mut expected = vec![2, 3];
        expected.extend((4..=385).rev());
        let mut actual = vec![];
        while let Some(current) = session.alias_take(&members).unwrap() {
            actual.push(current.serial);
            complete(&mut session, &members, current);
        }
        assert_eq!(actual, expected);
        let seal = session.alias_finish(&members).unwrap();
        assert_eq!((seal.records, seal.maximum), (385, Some(385)));
        session.alias_retire(&seal).unwrap();
        let inspect = Connection::open(&path).unwrap();
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_facts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            inspect
                .query_row("SELECT stage FROM alias_owner", [], |r| r.get::<_, u8>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            inspect
                .query_row("SELECT stage FROM site_owner", [], |r| r.get::<_, u8>(0))
                .unwrap(),
            1
        );
        drop(inspect);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        assert!(!path.exists());
    }
    #[test]
    fn aggregate_exhaustion_keeps_exact_prior_rows_and_refuses_before_insert() {
        let temp = TempDir::new("aliases_capacity");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, _, members) = begin(
            &authority,
            AliasCapacity::new(10, ALIAS_FIXED_BYTES + 79, 0).unwrap(),
        );
        session.alias_begin(&members, Some(1)).unwrap();
        let current = session.alias_take(&members).unwrap().unwrap();
        assert_eq!(current.serial, 1);
        let path = authority.status().unwrap()[0].path.clone();
        assert!(session.alias_enqueue(&members, &[2]).is_err());
        let inspect = Connection::open(&path).unwrap();
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_facts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            inspect
                .query_row("SELECT COUNT(*) FROM alias_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            inspect
                .query_row("SELECT sequence FROM alias_owner", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(inspect);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn actual_shared_read_barrier_makes_alias_commit_unknown_and_retains_exact_attempt() {
        let temp = TempDir::new("aliases_unknown");
        let parent = temp.path().to_path_buf();
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, _, members) =
            begin(&authority, AliasCapacity::new(10, 64 * 1024, 0).unwrap());
        session.alias_begin(&members, Some(1)).unwrap();
        let token = session.selection().token();
        let path = authority.status().unwrap()[0].path.clone();
        let held = Connection::open(&path).unwrap();
        held.execute_batch("BEGIN").unwrap();
        assert_eq!(
            held.query_row("SELECT facts FROM alias_owner", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let error = session.alias_enqueue(&members, &[2, 3]).unwrap_err();
        assert!(
            matches!(error, StorageError::UnknownOutcome { .. }),
            "{error:?}"
        );
        assert!(session.is_quarantined());
        assert!(session.release().is_err());
        let retained = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|s| s.token == token)
            .unwrap();
        assert_eq!(retained.disposition, ScratchDisposition::Unknown);
        let failure = retained.failure.unwrap();
        assert!(failure.contains("alias attempt enqueue"), "{failure}");
        assert!(
            failure.contains("serial: 2") && failure.contains("serial: 3"),
            "{failure}"
        );
        assert_eq!(
            held.query_row("SELECT facts FROM alias_owner", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        held.execute_batch("ROLLBACK").unwrap();
        drop(held);
        drop(session);
        assert!(authority.release_retained(token).is_err());
        assert!(path.exists());
        // The exact Unknown owner is intentionally preserved; no test cleanup guesses SQL custody.
        std::mem::forget(temp);
        println!("profile5 Unknown fixture retained at {}", parent.display());
    }
}
