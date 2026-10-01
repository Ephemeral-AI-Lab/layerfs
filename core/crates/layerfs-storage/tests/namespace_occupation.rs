//! Actual shared profile7 row ceilings at Sites/Alias/Graph/Roots boundaries.
//! Private finite component fixtures; no canonical/base authentication or physical RAM claim.
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
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_content::ObjectId;
    use layerfs_storage::construction_state::{ScratchAuthority, ScratchSession};
    fn scopes(session: &ScratchSession) -> (FactScope, FactScope) {
        let actual = FactSubject::new(
            session.graph_subject().unwrap().clone(),
            Some(InodeTable {
                root: ObjectId::from_bytes(&[0x73; 32]).unwrap(),
                root_serial: 1,
            }),
        )
        .unwrap();
        (
            FactScope::new(session.selection().clone(), actual.clone()).unwrap(),
            FactScope::parents(session.selection().clone(), actual).unwrap(),
        )
    }
    fn aliases_and_sites(session: &mut ScratchSession, scopes: &GraphConstructionScopes) {
        let m = session
            .site_close_membership(&SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal())
            .unwrap();
        session.alias_begin(&m, None).unwrap();
        let a = session.alias_finish(&m).unwrap();
        session.alias_retire(&a).unwrap();
        let seal = session.site_final_seal(&m).unwrap();
        session.site_retire(&seal).unwrap();
    }
    #[test]
    fn sites_and_existing_base_share_single_aggregate_before_insert() {
        with_source(1, true, |source, header| {
            let temp = TempDir::new("occupation_sites");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let subject = subject(source, true, DEFAULT);
            let mut session = authority
                .begin_namespace(
                    [0x81; 32],
                    0,
                    1,
                    subject,
                    AliasCapacity::new(10, DEFAULT, 1).unwrap(),
                    FactCapacity::new(10, 0, FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES + 105)
                        .unwrap(),
                )
                .unwrap();
            let selected = selected(&session);
            let (base, _) = scopes(&session);
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
            let birth = SiteRecord::birth(
                selected.sites(),
                2,
                layerfs_content::filesystem::rows::BindingPoint::new(&header, 0).unwrap(),
                true,
            )
            .unwrap();
            assert!(session
                .site_insert_batch(selected.sites(), &[birth])
                .is_err());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM binding_sites"), 0);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM base_facts"), 1);
            drop(db);
            session.release().unwrap();
        });
    }
    #[test]
    fn alias_planning_checks_base_overlap_before_actual_fact_and_job_writes() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("occupation_alias");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let subject = subject(source, true, DEFAULT);
            let mut session = authority
                .begin_namespace(
                    [0x82; 32],
                    0,
                    0,
                    subject,
                    AliasCapacity::new(10, DEFAULT, 0).unwrap(),
                    FactCapacity::new(10, 0, FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES + 184)
                        .unwrap(),
                )
                .unwrap();
            let selected = selected(&session);
            let (base, _) = scopes(&session);
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
            let m = session
                .site_close_membership(
                    &SiteBirthLedger::new(selected.sites().clone())
                        .unwrap()
                        .seal(),
                )
                .unwrap();
            session.alias_begin(&m, Some(1)).unwrap();
            session.alias_take(&m).unwrap().unwrap();
            assert!(session.alias_enqueue(&m, &[3]).is_err());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM alias_facts"), 1);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM alias_jobs"), 0);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM base_facts"), 1);
            drop(db);
            session.release().unwrap();
        });
    }
    #[test]
    fn graph_growth_counts_live_parent_rows_before_graph_commit() {
        with_source(1, true, |source, _| {
            let temp = TempDir::new("occupation_graph");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let subject = subject(source, true, DEFAULT);
            let mut session = authority
                .begin_namespace(
                    [0x83; 32],
                    0,
                    1,
                    subject,
                    AliasCapacity::new(10, DEFAULT, 1).unwrap(),
                    FactCapacity::new(10, 10, FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES + 91)
                        .unwrap(),
                )
                .unwrap();
            let selected = selected(&session);
            let (base, parent) = scopes(&session);
            session.fact_bind(&base).unwrap();
            session.parent_bind(&parent).unwrap();
            session.parent_insert(&parent, &[3]).unwrap();
            aliases_and_sites(&mut session, &selected);
            let one = GraphNodeKey::new(selected.graph(), 1).unwrap();
            // One60-byte node fits91 in isolation; the already live32-byte
            // parent makes this exact same prospective insert exceed91.
            assert!(session.graph_seed_batch(selected.graph(), &[one]).is_err());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM graph_nodes"), 0);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM parent_eligibility"), 1);
            drop(db);
            session.release().unwrap();
        });
    }
    #[test]
    fn roots_growth_includes_live_sealed_parent_rows_before_root_insert() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("occupation_roots");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let subject = subject(source, true, DEFAULT);
            let mut session = authority
                .begin_namespace(
                    [0x84; 32],
                    1,
                    0,
                    subject,
                    AliasCapacity::new(10, DEFAULT, 0).unwrap(),
                    FactCapacity::new(10, 10, FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES + 94)
                        .unwrap(),
                )
                .unwrap();
            let selected = selected(&session);
            let (base, parent) = scopes(&session);
            session.fact_bind(&base).unwrap();
            session.parent_bind(&parent).unwrap();
            session.parent_insert(&parent, &[3]).unwrap();
            session.parent_close_declarations(&parent).unwrap();
            let parentseal = session.parent_seal(&parent).unwrap();
            aliases_and_sites(&mut session, &selected);
            assert!(session
                .graph_unexpanded(selected.graph())
                .unwrap()
                .is_none());
            let adjacency = session.graph_seal(selected.graph()).unwrap();
            solve_effective_graph(
                &mut session.adapter(),
                &adjacency,
                &mut ValidationGraphWork::default(),
            )
            .unwrap();
            let proof = session.graph_finish(&adjacency).unwrap();
            session.graph_retire(&proof).unwrap();
            assert!(session
                .append(selected.roots(), &[root(selected.roots(), 1)])
                .is_err());
            let path = status(&authority, session.selection().token()).path;
            let db = external(&path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM directory_roots"), 0);
            assert_eq!(
                scalar(&db, "SELECT COUNT(*) FROM parent_eligibility"),
                parentseal.facts.records
            );
            drop(db);
            session.release().unwrap();
        });
    }
}
