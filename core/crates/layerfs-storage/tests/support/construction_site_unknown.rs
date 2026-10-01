//! Each actual SQLite COMMIT phase has a fresh child/native owner and SHARED barrier.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use layerfs_content::filesystem::rows::BindingRows;
use layerfs_content::filesystem::state::{BindingSiteState, SiteKey, SiteObservation};
use layerfs_content::ContentError;
use layerfs_storage::construction_state::{ScratchAuthority, ScratchDisposition};
use layerfs_storage::StorageError;

use super::fixture::*;
use super::{oracle, process, support::TempDir};

#[test]
fn real_shared_reader_unknown_birth_close_facts_final_and_retire_keep_exact_owner() {
    for phase in ["birth", "close", "facts", "final", "retire"] {
        let temp = TempDir::new("sites_unknown_process");
        let output = process::writer(temp.path(), phase);
        assert!(
            output.status.success(),
            "phase={phase} stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout)
            .contains(&format!("LFCS3 exact Unknown retained: {phase}")));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let private: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(private.len(), 1);
        assert_eq!(
            std::fs::read_dir(&private[0]).unwrap().count(),
            1,
            "{phase} Unknown owner was not unlinked"
        );
    }
}

#[test]
fn writer_child() {
    let Some(base) = std::env::var_os("LAYERFS_SITES_UNKNOWN_BASE") else {
        return;
    };
    with_source(2, |source, header| {
        let authority = ScratchAuthority::new(Path::new(&base), 1).unwrap();
        let mut session = authority
            .begin_sites([0x60; 32], 1, 2, source.binding_source_id().unwrap())
            .unwrap();
        let selected = scopes(&session, source);
        let owner = status(&authority, session.selection().token());
        let phase = std::env::var("LAYERFS_SITES_UNKNOWN_PHASE").unwrap();
        let births = [
            birth(selected.sites(), header, 0, true),
            birth(selected.sites(), header, 1, true),
        ];
        let prior_records = if phase == "birth" {
            0
        } else {
            insert(&mut session, selected.sites(), header, 2, |_| true);
            2
        };
        let expected = expected_birth(selected.sites(), header, 2, |_| true);
        let members = if matches!(phase.as_str(), "facts" | "final" | "retire") {
            Some(session.site_close_membership(&expected).unwrap())
        } else {
            None
        };
        let observations = [
            SiteObservation::new(births[0].key(), true),
            SiteObservation::new(births[1].key(), false),
        ];
        if matches!(phase.as_str(), "final" | "retire") {
            session
                .site_observe_base_batch(members.as_ref().unwrap(), &observations)
                .unwrap();
        }
        let known_seal = if phase == "retire" {
            Some(session.site_final_seal(members.as_ref().unwrap()).unwrap())
        } else {
            None
        };
        let prior_stage = match phase.as_str() {
            "birth" | "close" => 0,
            "facts" | "final" => 1,
            "retire" => 2,
            _ => panic!("unregistered phase {phase}"),
        };
        assert_eq!(rows(&owner.path, "sites"), prior_records);
        assert_eq!(rows(&owner.path, "roots"), 0);
        let mut reader = process::HeldReader::new(&owner.path, prior_records, prior_stage);
        let outcome = match phase.as_str() {
            "birth" => session
                .adapter()
                .site_insert_batch(selected.sites(), &births)
                .map(|_| ()),
            "close" => session
                .adapter()
                .site_close_membership(&expected)
                .map(|_| ()),
            "facts" => session
                .adapter()
                .site_observe_base_batch(members.as_ref().unwrap(), &observations),
            "final" => session
                .adapter()
                .site_final_seal(members.as_ref().unwrap())
                .map(|_| ()),
            "retire" => session.adapter().site_retire(known_seal.as_ref().unwrap()),
            _ => unreachable!(),
        };
        assert!(
            matches!(
                outcome,
                Err(ContentError::ProviderFailure {
                    what: "construction scratch state"
                })
            ),
            "{phase}: {outcome:?}"
        );
        let original = session.take_failure().expect("original typed C2 failure");
        let StorageError::UnknownOutcome { original } = original else {
            panic!("expected COMMIT Unknown: {original:?}")
        };
        let StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) = *original else {
            panic!("actual SQLite failure expected: {original:?}")
        };
        assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
        assert!(session.is_quarantined());
        let before = status(&authority, owner.token);
        let before_metadata = std::fs::metadata(&owner.path).unwrap();
        session.site_abandon(selected.sites()).unwrap();
        session.site_abandon(selected.sites()).unwrap();
        assert!(session.take_failure().is_none());
        let quarantined = status(&authority, owner.token);
        assert_eq!(quarantined.disposition, ScratchDisposition::Unknown);
        let failure = quarantined.failure.as_ref().unwrap();
        assert!(failure.contains(&format!("scope={:?}", scope_bytes(selected.sites()))));
        assert!(failure.contains(&format!("records={prior_records}")));
        let kind = match phase.as_str() {
            "birth" => "Birth",
            "close" => "CloseMembership",
            "facts" => "Observe",
            "final" => "FinalSeal",
            "retire" => "Retire",
            _ => unreachable!(),
        };
        assert!(
            failure.contains(&format!("site attempt {kind}:")),
            "{failure}"
        );
        match phase.as_str() {
            "birth" => {
                assert!(failure.contains("before=0, proposed=2"));
                assert!(
                    failure.contains(&format!("records={:?}", [Some(births[0]), Some(births[1])]))
                );
                assert!(failure.contains("proposed_flags=[1, 1]"));
            }
            "close" => {
                assert!(failure.contains(&format!("expected birth={:?}", expected.encode())));
                assert!(failure.contains(&format!(
                    "proposed membership={:?}",
                    oracle::membership(
                        &expected.encode(),
                        Some(oracle::key(&scope_bytes(selected.sites()), 3))
                    )
                )));
            }
            "facts" => {
                assert!(
                    failure.contains(&format!("records={:?}", [Some(births[0]), Some(births[1])]))
                );
                assert!(failure.contains("proposed_flags=[7, 3]"));
                assert!(failure.contains(&format!(
                    "observations={:?}",
                    [Some(observations[0]), Some(observations[1])]
                )));
            }
            "final" => {
                let mut expected_final =
                    oracle::Transcript::new(scope_bytes(selected.sites()), true);
                expected_final.append(2, 3, point_bytes(selected.sites(), header, 1));
                expected_final.append(3, 7, point_bytes(selected.sites(), header, 0));
                assert!(failure.contains(&format!("proposed final={:?}", expected_final.seal())));
                assert!(failure.contains("maximum=Some(SiteKey("));
            }
            "retire" => {
                let expected_records = [
                    Some(births[1].observe(false).unwrap()),
                    Some(births[0].observe(true).unwrap()),
                ];
                assert!(failure.contains("before=2, proposed=0"));
                assert!(failure.contains(&format!("records={expected_records:?}")));
                assert!(failure.contains("prior_after=None"));
                assert!(failure.contains(&format!(
                    "proposed_after={:?}",
                    Some(SiteKey::new(selected.sites(), 3).unwrap())
                )));
                assert!(failure.contains(&format!(
                    "final=Some({:?})",
                    known_seal.as_ref().unwrap().encode()
                )));
            }
            _ => unreachable!(),
        }
        assert_eq!(quarantined.failure, before.failure);
        assert_eq!(quarantined.file, owner.file);
        assert_eq!(quarantined.path, owner.path);
        assert_eq!(quarantined.allocated_bytes, Some(CLASS));
        assert!(!quarantined.release_attempted);
        let after_metadata = std::fs::metadata(&owner.path).unwrap();
        assert_eq!(identity(&before_metadata), identity(&after_metadata));
        assert_eq!(before_metadata.len(), after_metadata.len());
        assert_eq!(before_metadata.blocks(), after_metadata.blocks());
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        let output = reader.release_and_reap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        // Explicit negative access attempts never resend the failed operation or
        // inspect an Unknown SQL result. Taking the typed error did not refund.
        assert!(session.capacity(selected.roots()).is_err());
        assert!(session
            .append(selected.roots(), &[root(selected.roots(), 1)])
            .is_err());
        assert!(session.release().is_err());
        drop(session);
        let retained = status(&authority, owner.token);
        assert_eq!(retained.disposition, ScratchDisposition::Unknown);
        assert!(retained.retained && retained.quarantined);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        assert!(authority.release_retained(owner.token).is_err());
        assert!(owner.path.exists());
        assert_eq!(retained.file, owner.file);
        assert_eq!(retained.allocated_bytes, Some(CLASS));
        assert_eq!(retained.failure, quarantined.failure);
        println!("LFCS3 exact Unknown retained: {phase}");
    });
}

#[test]
fn reader_child() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    let Some(path) = std::env::var_os("LAYERFS_SITES_READ_DB") else {
        return;
    };
    let socket = std::env::var_os("LAYERFS_SITES_READ_SOCKET").unwrap();
    let expected_records: i64 = std::env::var("LAYERFS_SITES_READ_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    let expected_stage: i64 = std::env::var("LAYERFS_SITES_READ_STAGE")
        .unwrap()
        .parse()
        .unwrap();
    let mut control = UnixStream::connect(Path::new(&socket)).unwrap();
    control
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    control
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let reader = rusqlite::Connection::open_with_flags(
        Path::new(&path),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    reader.busy_timeout(Duration::ZERO).unwrap();
    let source: String = reader
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    eprintln!(
        "LFCS3 actual reader: sqlite={} source={source}; READ_ONLY|NOFOLLOW/busy0, executable={}",
        rusqlite::version(),
        std::env::current_exe().unwrap().display()
    );
    reader.execute_batch("BEGIN").unwrap();
    let facts: (i64, i64) = reader
        .query_row("SELECT records,stage FROM site_owner", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(facts, (expected_records, expected_stage));
    eprintln!("LFCS3 actual BEGIN+SELECT SHARED barrier acknowledged: records={expected_records},stage={expected_stage}");
    control.write_all(&[1]).unwrap();
    let mut command = [0];
    control.read_exact(&mut command).unwrap();
    assert_eq!(command, [2]);
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    control.write_all(&[3]).unwrap();
}
