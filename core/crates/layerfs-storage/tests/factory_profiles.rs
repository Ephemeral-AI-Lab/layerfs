//! New private5/6 exact captured headers and real5 successful reset.
//! Logical/native component scope only; no memory/speed qualification.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
    use layerfs_content::file::edit::DraftCapacity;
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_storage::construction_state::ScratchAuthority;
    fn header(path: &std::path::Path) -> Vec<u8> {
        external(path)
            .query_row("SELECT header FROM session_owner WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap()
    }
    fn check_binding(bytes: &[u8], domain: &[u8]) {
        let mut hash = blake3::Hasher::new();
        hash.update(domain);
        hash.update(&bytes[88..120]);
        hash.update(&bytes[120..192]);
        hash.update(&bytes[24..56]);
        hash.update(&bytes[16..24]);
        hash.update(&bytes[192..]);
        assert_eq!(&bytes[56..88], hash.finalize().as_bytes());
    }
    #[test]
    fn profile5_exact_header_and_fifth_known_clean_row() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("factory5_reset");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let graph_subject = subject(source, true, DEFAULT);
            let capacity = AliasCapacity::new(128, 8192, 0).unwrap();
            let mut session = authority
                .begin_alias_graph([0x65; 32], 129, 0, graph_subject.clone(), capacity)
                .unwrap();
            let scopes = selected(&session);
            let owner = status(&authority, session.selection().token());
            let bytes = header(&owner.path);
            assert_eq!(bytes.len(), 322);
            assert_eq!(&bytes[..8], b"LFCSOWN5");
            assert_eq!(&bytes[8..10], &5u16.to_be_bytes());
            assert_eq!(&bytes[192..298], &graph_subject.encode());
            assert_eq!(&bytes[298..306], &128u64.to_be_bytes());
            assert_eq!(&bytes[306..314], &8192u64.to_be_bytes());
            assert_eq!(&bytes[314..], &0u64.to_be_bytes());
            check_binding(&bytes, b"layerfs/construction-state/native/v5\0");
            let membership = session
                .site_close_membership(
                    &SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal(),
                )
                .unwrap();
            session.alias_begin(&membership, None).unwrap();
            let aliases = session.alias_finish(&membership).unwrap();
            session.alias_retire(&aliases).unwrap();
            let sites = session.site_final_seal(&membership).unwrap();
            session.site_retire(&sites).unwrap();
            assert!(session.graph_unexpanded(scopes.graph()).unwrap().is_none());
            let adjacency = session.graph_seal(scopes.graph()).unwrap();
            solve_effective_graph(
                &mut session.adapter(),
                &adjacency,
                &mut ValidationGraphWork::default(),
            )
            .unwrap();
            let proof = session.graph_finish(&adjacency).unwrap();
            session.graph_retire(&proof).unwrap();
            for start in [1u64, 129] {
                let batch = (start..=(start + 127).min(129))
                    .map(|serial| root(scopes.roots(), serial))
                    .collect::<Vec<_>>();
                session.append(scopes.roots(), &batch).unwrap();
            }
            let seal = session.seal(scopes.roots()).unwrap();
            session.complete_phase(scopes.roots()).unwrap();
            let clean = status(&authority, owner.token);
            assert!(clean.known_clean);
            assert_eq!(clean.root_retirement.unwrap().seal, seal);
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            let db = external(&owner.path);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM alias_facts"), 0);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM alias_jobs"), 0);
            assert_eq!(
                scalar(
                    &db,
                    "SELECT stage+sequence+facts+jobs+expanded+remaining FROM alias_owner"
                ),
                0
            );
            let progress: Vec<u8> = db
                .query_row("SELECT progress FROM alias_owner", [], |row| row.get(0))
                .unwrap();
            assert_eq!(progress, [0u8; 264]);
            drop(db);
            assert!(session.alias_capacity(&membership).is_err());
            assert!(session.graph_select(scopes.graph()).is_err());
            assert!(session
                .get(&seal, StateKey::directory_root(scopes.roots(), 1).unwrap())
                .is_err());
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }
    #[test]
    fn profile6_exact_captured_header_scope_and_version() {
        let temp = TempDir::new("factory6_header");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let capacity = DraftCapacity::default();
        let mut session = authority.begin_drafts([0x66; 32], capacity).unwrap();
        let owner = status(&authority, session.selection().token());
        let bytes = header(&owner.path);
        assert_eq!(bytes.len(), 216);
        assert_eq!(&bytes[..8], b"LFCSOWN6");
        assert_eq!(&bytes[8..10], &6u16.to_be_bytes());
        assert_eq!(&bytes[192..200], &65536u64.to_be_bytes());
        assert_eq!(&bytes[200..208], &8388607u64.to_be_bytes());
        assert_eq!(&bytes[208..], &DEFAULT.to_be_bytes());
        check_binding(&bytes, b"layerfs/construction-state/native/v6\0");
        assert_eq!(session.profile().unwrap().version, 6);
        assert_eq!(session.draft_scope().unwrap().capacity(), capacity);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn profile7_exact_capacity_suffix_and_seven_row_reset_after_final_consumers() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("factory7_reset");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let graph_subject = subject(source, true, DEFAULT);
            let mut session = authority
                .begin_namespace(
                    [0x67; 32],
                    129,
                    0,
                    graph_subject.clone(),
                    AliasCapacity::new(128, DEFAULT, 0).unwrap(),
                    FactCapacity::new(128, 128, DEFAULT).unwrap(),
                )
                .unwrap();
            let scopes = selected(&session);
            let owner = status(&authority, session.selection().token());
            let bytes = header(&owner.path);
            assert_eq!(bytes.len(), 346);
            assert_eq!(&bytes[..8], b"LFCSOWN7");
            assert_eq!(&bytes[8..10], &7u16.to_be_bytes());
            assert_eq!(&bytes[192..298], &graph_subject.encode());
            assert_eq!(&bytes[322..330], &128u64.to_be_bytes());
            assert_eq!(&bytes[330..338], &128u64.to_be_bytes());
            assert_eq!(&bytes[338..], &DEFAULT.to_be_bytes());
            check_binding(&bytes, b"layerfs/construction-state/native/v7\0");
            let actual = FactSubject::new(
                graph_subject,
                Some(layerfs_content::filesystem::inode::read::InodeTable {
                    root: layerfs_content::ObjectId::from_bytes(&[0x73; 32]).unwrap(),
                    root_serial: 1,
                }),
            )
            .unwrap();
            let base = FactScope::new(session.selection().clone(), actual.clone()).unwrap();
            let parent = FactScope::parents(session.selection().clone(), actual).unwrap();
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
            session.parent_bind(&parent).unwrap();
            session.parent_insert(&parent, &[3]).unwrap();
            session.parent_close_declarations(&parent).unwrap();
            let parents = session.parent_seal(&parent).unwrap();
            let membership = session
                .site_close_membership(
                    &SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal(),
                )
                .unwrap();
            session.alias_begin(&membership, None).unwrap();
            let aliases = session.alias_finish(&membership).unwrap();
            session.alias_retire(&aliases).unwrap();
            let sites = session.site_final_seal(&membership).unwrap();
            session.site_retire(&sites).unwrap();
            assert!(session.graph_unexpanded(scopes.graph()).unwrap().is_none());
            let adjacency = session.graph_seal(scopes.graph()).unwrap();
            solve_effective_graph(
                &mut session.adapter(),
                &adjacency,
                &mut ValidationGraphWork::default(),
            )
            .unwrap();
            let proof = session.graph_finish(&adjacency).unwrap();
            session.graph_retire(&proof).unwrap();
            let base_seal = session.fact_seal(&base).unwrap();
            session.fact_retire(&base_seal).unwrap();
            for start in [1u64, 129] {
                let batch = (start..=(start + 127).min(129))
                    .map(|serial| root(scopes.roots(), serial))
                    .collect::<Vec<_>>();
                session.append(scopes.roots(), &batch).unwrap();
            }
            let seal = session.seal(scopes.roots()).unwrap();
            assert!(session.parent_get(&parents, 3).unwrap().is_some());
            session.parent_retire(&parents).unwrap();
            session.complete_phase(scopes.roots()).unwrap();
            let clean = status(&authority, owner.token);
            assert!(clean.known_clean);
            assert_eq!(clean.root_retirement.unwrap().seal, seal);
            assert_eq!(session.profile().unwrap().version, 7);
            let db = external(&owner.path);
            for table in [
                "base_facts",
                "parent_eligibility",
                "directory_roots",
                "alias_facts",
                "alias_jobs",
            ] {
                assert_eq!(scalar(&db, &format!("SELECT COUNT(*) FROM {table}")), 0);
            }
            assert_eq!(scalar(&db,"SELECT COUNT(*) FROM fact_owner WHERE stage=0 AND scope IS NULL AND records=0 AND bound=0 AND record_bytes=0 AND remaining=0 AND maximum IS NULL AND after_serial IS NULL AND digest IS NULL"),2);
            drop(db);
            assert!(session.fact_get(&base, 2).is_err());
            assert!(session.parent_get(&parents, 3).is_err());
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }
}
