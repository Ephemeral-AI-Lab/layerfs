//! `store upgrade` on SQLite (#804 §5.1), engine level, on copies of the
//! frozen schema-2 fixture the old build captured: the golden upgrade (A),
//! a working Store afterwards (B), the exact schema (D), refusals that
//! change nothing (F), cancel in each phase (H), the counts (J, in-process
//! part) and the two SQLite behaviours the run rests on (L).
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::records::{CommitMeta, CommitOutcome, CommitRequest, CommitVia};
use layerfs_history::{
    CommitAttempt, CommitId, HistoryCatalog, HistoryName, LayerId, NamedSnapshot,
};
use layerfs_persistence::upgrade::{
    sqlite_format, sqlite_format_evidence, upgrade_sqlite, FormatStatus, UpgradeError,
    UpgradeOutcome, UpgradeRequest, SQLITE_PHASES,
};
use layerfs_persistence::{read_heads, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use support::upgrade::{
    fixtures, manifest_branches, sqlite_fixture_copy, sqlite_v2_copy, store_state, unhex, SQLITE_V2,
};
use support::Temp;

fn run(
    path: &Path,
    cancel: &AtomicBool,
    failpoint: Option<u32>,
) -> (
    Result<layerfs_persistence::upgrade::UpgradeReport, UpgradeError>,
    Vec<&'static str>,
) {
    let mut phases = Vec::new();
    let request = UpgradeRequest { cancel, failpoint };
    let answer = upgrade_sqlite(path, &request, &mut |p| phases.push(p));
    (answer, phases)
}

fn binding(path: &Path) -> Vec<u8> {
    let c = Connection::open(path).unwrap();
    let b = c
        .query_row("SELECT binding_key FROM history_meta", [], |r| r.get(0))
        .unwrap();
    c.close().unwrap();
    b
}

fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path.to_path_buf())
        .with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn raw(path: &Path, sql: &str) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(sql).unwrap();
    c.close().unwrap();
}

/// Every Commit and Layer id the file holds, sorted.
fn ids(path: &Path) -> Vec<Vec<u8>> {
    let c = Connection::open(path).unwrap();
    let mut s = c
        .prepare("SELECT commit_id FROM \"commit\" UNION ALL SELECT layer_id FROM layer ORDER BY 1")
        .unwrap();
    let ids = s
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    drop(s);
    c.close().unwrap();
    ids
}

#[test]
fn golden_upgrade_keeps_every_head_and_commit() {
    for (variant, folder) in SQLITE_V2 {
        golden(variant, folder);
    }
}

fn golden(variant: &str, folder: &str) {
    let temp = Temp::new("upgrade-golden");
    let path = sqlite_fixture_copy(temp.path(), folder);
    let evidence = sqlite_format_evidence(&path).unwrap();
    assert_eq!(evidence.variant.as_deref(), Some(variant));
    assert_eq!(sqlite_format(&path).unwrap().status, FormatStatus::Outdated);
    let heads = read_heads(&path).unwrap();
    let before_ids = ids(&path);
    let started = std::time::Instant::now();
    let (answer, phases) = run(&path, &AtomicBool::new(false), None);
    let report = answer.expect("upgraded");
    eprintln!("upgrade took {:?}", started.elapsed());
    assert_eq!(report.outcome, UpgradeOutcome::Upgraded);
    assert_eq!((report.format_before, report.format_after), (2, 3));
    assert_eq!(report.steps, vec!["2->3".to_owned()]);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.work.write_transactions, 1);
    assert_eq!(phases, SQLITE_PHASES.to_vec());
    assert_eq!(read_heads(&path).unwrap(), heads);
    assert_eq!(ids(&path), before_ids);
    let format = sqlite_format(&path).unwrap();
    assert_eq!(
        (format.version, format.status),
        (Some(3), FormatStatus::Current)
    );
    let raw_version: i64 = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(raw_version, 62);

    // The current build opens it and answers every head and Commit the old
    // build named at capture.
    let handles = SqliteHandles::open_read_only(config(&path), &binding(&path), [1; 32]).unwrap();
    let manifest = manifest_branches(&fixtures().join(folder).join("manifest.json"));
    assert!(!manifest.is_empty());
    for (project, branch, head, commits) in manifest {
        let found = handles
            .history
            .branch_snapshot_named(
                &HistoryName::new(&project).unwrap(),
                &HistoryName::new(&branch).unwrap(),
            )
            .unwrap();
        let NamedSnapshot::Found(snapshot) = found else {
            panic!("{project}/{branch} missing");
        };
        // A Branch's head is its head Commit; a project's published line
        // (`main`) answers the stack's head Layer.
        let actual = match (head.starts_with("32"), snapshot.branch.head_commit) {
            (false, Some(id)) => support::upgrade::hex(id.as_slice()),
            _ => {
                let stack = handles
                    .history
                    .layer_stack_named(&HistoryName::new(&project).unwrap())
                    .unwrap()
                    .unwrap();
                support::upgrade::hex(stack.head_layer.as_slice())
            }
        };
        assert_eq!(actual, head, "{project}/{branch} head");
        for id in commits {
            let bytes = unhex(&id);
            let present = if bytes[0] == 0x12 {
                handles
                    .history
                    .commit(CommitId::from_slice(&bytes).unwrap())
                    .unwrap()
                    .is_some()
            } else {
                handles
                    .history
                    .layer(LayerId::from_slice(&bytes).unwrap())
                    .unwrap()
                    .is_some()
            };
            assert!(present, "{project}/{branch} {id}");
        }
    }
    drop(handles);

    // A second upgrade changes nothing. (The read-only open above left its
    // empty `-wal` and `-shm`, which a last close may remove: the Store file
    // is what must not change.)
    let state = support::upgrade::sha256_file(&path);
    let (again, phases) = run(&path, &AtomicBool::new(false), None);
    let again = again.unwrap();
    assert_eq!(again.outcome, UpgradeOutcome::Current);
    assert_eq!((again.format_before, again.format_after), (3, 3));
    assert!(again.steps.is_empty());
    assert_eq!(again.work.write_transactions, 0);
    assert_eq!(phases, vec!["check"]);
    assert_eq!(support::upgrade::sha256_file(&path), state);
}

#[test]
fn upgraded_store_takes_commits_with_an_attempt_and_passes_the_doctor() {
    for (_, folder) in SQLITE_V2 {
        works(folder);
    }
}

fn works(folder: &str) {
    let temp = Temp::new("upgrade-works");
    let path = sqlite_fixture_copy(temp.path(), folder);
    run(&path, &AtomicBool::new(false), None).0.unwrap();
    let binding = binding(&path);
    let doctor = SqliteHandles::open_diagnostic(&path, &binding, [1; 32])
        .expect("the doctor's open admits it");
    // The whole doctor, every check, passes over the upgraded Store.
    let mut failed = Vec::new();
    doctor
        .diagnose(
            layerfs_persistence::diagnostic::Options {
                quick: false,
                deadline: None,
                cancelled: None,
            },
            &mut |o| {
                if o.outcome == layerfs_persistence::diagnostic::Outcome::Fail {
                    failed.push(format!("{}: {}", o.id, o.message));
                }
            },
        )
        .unwrap();
    assert!(failed.is_empty(), "{failed:#?}");
    drop(doctor);
    let handles = SqliteHandles::open_writable(config(&path), &binding, [1; 32]).unwrap();
    let history = &handles.history;
    let NamedSnapshot::Found(snapshot) = history
        .branch_snapshot_named(
            &HistoryName::new("lib").unwrap(),
            &HistoryName::new("dev").unwrap(),
        )
        .unwrap()
    else {
        panic!("lib/dev");
    };
    let record = snapshot.branch;
    let head = history
        .commit(record.head_commit.unwrap())
        .unwrap()
        .unwrap();
    let attempt = CommitAttempt::from_bytes([7; 16]);
    let outcome = history
        .commit_branch(&CommitRequest {
            branch: record.id,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: head.root,
            candidate_root: support::root(9),
            profile: snapshot.profile,
            scope: snapshot.scope,
            second_parent: None,
            meta: CommitMeta {
                attempt: Some(attempt),
                ..CommitMeta::new(CommitVia::Files)
            },
            force: false,
            required_head: None,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("not committed: {outcome:?}");
    };
    assert_eq!(
        history.commit_by_attempt(attempt).unwrap().unwrap().id,
        record.id
    );
}

#[test]
fn the_upgraded_schema_is_the_frozen_target() {
    for (_, folder) in SQLITE_V2 {
        exact_schema(folder);
    }
}

fn exact_schema(folder: &str) {
    let temp = Temp::new("upgrade-schema");
    let path = sqlite_fixture_copy(temp.path(), folder);
    run(&path, &AtomicBool::new(false), None).0.unwrap();
    let c = Connection::open(&path).unwrap();
    let sql = |name: &str| -> String {
        c.query_row("SELECT sql FROM sqlite_schema WHERE name=?1", [name], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let commit = sql("commit");
    assert!(
        commit.contains(
            "history_serial INTEGER NOT NULL DEFAULT 0, attempt BLOB CHECK(attempt IS NULL OR length(attempt)=16),\n    FOREIGN KEY"
        ),
        "{folder}: {commit}"
    );
    assert_eq!(
        sql("commit_attempt"),
        "CREATE UNIQUE INDEX commit_attempt ON \"commit\"(attempt) WHERE attempt IS NOT NULL"
    );
    for table in ["store_policy", "history_meta"] {
        let text = sql(table);
        assert!(
            text.contains("CHECK(schema_version=62)") && !text.contains("=42)"),
            "{text}"
        );
    }
    let (source, definition): (String, String) = c
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let (target, current) = layerfs_persistence::internals::sqlite_target_and_current_source(62);
    assert_eq!(source, target);
    assert_eq!(source, current);
    let mut live = String::new();
    let mut s = c
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    for row in s
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
    {
        let (t, n, q) = row.unwrap();
        live.push_str(&format!("{t}\n{n}\n{q}\n"));
    }
    assert_eq!(definition, live);
    let (check, policy): (String, i64) = (
        c.query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap(),
        c.query_row("SELECT schema_version FROM store_policy", [], |r| r.get(0))
            .unwrap(),
    );
    assert_eq!((check.as_str(), policy), ("ok", 62));
}

/// Applies `edit` to a fresh copy, then expects `upgrade` to refuse it as
/// `matches` says, leaving the file and sidecars byte-identical.
fn refused(tag: &str, edit: &str, matches: impl Fn(&UpgradeError) -> bool) -> UpgradeError {
    let temp = Temp::new(tag);
    let path = sqlite_v2_copy(temp.path());
    if !edit.is_empty() {
        raw(&path, edit);
    }
    let state = store_state(&path);
    let error = run(&path, &AtomicBool::new(false), None)
        .0
        .expect_err("refused");
    assert!(matches(&error), "{tag}: {error:?}");
    assert_eq!(
        store_state(&path),
        state,
        "{tag}: the refusal changed the Store"
    );
    error
}

#[test]
fn refusals_change_nothing() {
    let unsupported = |e: &UpgradeError| matches!(e, UpgradeError::Unsupported(_));
    refused(
        "upgrade-source",
        "PRAGMA ignore_check_constraints=1; UPDATE history_meta SET schema_source=schema_source||' '",
        unsupported,
    );
    refused("upgrade-def", "CREATE TABLE extra(x INTEGER)", unsupported);
    for (tag, version, word) in [
        ("upgrade-transient", 45, "transient"),
        ("upgrade-schema1", 22, "schema version 1"),
        ("upgrade-retired", 5, "retired"),
        ("upgrade-newer", 82, "newer than this build"),
        ("upgrade-unknown", 50, "not a format"),
    ] {
        let error = refused(tag, &format!("PRAGMA user_version={version}"), unsupported);
        let UpgradeError::Unsupported(text) = error else {
            unreachable!()
        };
        assert!(text.contains(word), "{tag}: {text}");
        assert!(!text.contains("outdated"), "{tag}: {text}");
    }
    refused("upgrade-foreign", "PRAGMA application_id=7", unsupported);
    refused(
        "upgrade-intent",
        "INSERT INTO gc_intent(op_id,kind,scope_kind,scope_id,state,uncertain,updated_ms) \
         SELECT randomblob(16),'mount','store',catalog_id,'live',0,1 FROM history_meta",
        |e| matches!(e, UpgradeError::Busy(_)),
    );
}

#[test]
fn another_open_connection_is_in_use_and_a_held_lane_is_busy() {
    let temp = Temp::new("upgrade-holders");
    let path = sqlite_v2_copy(temp.path());
    let state = store_state(&path);
    {
        let other = Connection::open(&path).unwrap();
        let _: i64 = other
            .query_row("SELECT count(*) FROM \"commit\"", [], |r| r.get(0))
            .unwrap();
        let error = run(&path, &AtomicBool::new(false), None).0.unwrap_err();
        assert!(matches!(error, UpgradeError::InUse(_)), "{error:?}");
        other.close().unwrap();
    }
    {
        let dir = std::fs::File::open(temp.path()).unwrap();
        let lock =
            nix::fcntl::Flock::lock(dir, nix::fcntl::FlockArg::LockExclusiveNonblock).unwrap();
        let error = run(&path, &AtomicBool::new(false), None).0.unwrap_err();
        assert!(matches!(error, UpgradeError::Busy(_)), "{error:?}");
        drop(lock);
    }
    assert_eq!(store_state(&path), state);
    run(&path, &AtomicBool::new(false), None)
        .0
        .expect("free again");
}

#[test]
fn a_cancel_in_each_phase_before_commit_changes_nothing_and_at_commit_is_too_late() {
    for ((_, folder), at) in SQLITE_V2
        .iter()
        .flat_map(|fixture| SQLITE_PHASES.iter().map(move |at| (*fixture, *at)))
    {
        let temp = Temp::new("upgrade-cancel");
        let path = sqlite_fixture_copy(temp.path(), folder);
        let state = store_state(&path);
        let cancel = AtomicBool::new(false);
        let request = UpgradeRequest {
            cancel: &cancel,
            failpoint: None,
        };
        let answer = upgrade_sqlite(&path, &request, &mut |p| {
            if p == at {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        if at == "commit" {
            assert_eq!(answer.unwrap().outcome, UpgradeOutcome::Upgraded);
        } else {
            assert_eq!(answer.unwrap_err(), UpgradeError::Cancelled, "{at}");
            assert_eq!(store_state(&path), state, "{at}");
        }
    }
}

#[test]
fn a_failpoint_after_each_statement_before_commit_rolls_back() {
    // Statements: BEGIN, the step's eleven, COMMIT (13).
    for (n, (_, folder)) in (1..=12).flat_map(|n| SQLITE_V2.map(|f| (n, f))) {
        let temp = Temp::new("upgrade-failpoint");
        let path = sqlite_fixture_copy(temp.path(), folder);
        let state = store_state(&path);
        let error = run(&path, &AtomicBool::new(false), Some(n)).0.unwrap_err();
        assert_eq!(error, UpgradeError::Failed(format!("failpoint {n}")));
        assert_eq!(store_state(&path), state, "failpoint {n}");
    }
    let temp = Temp::new("upgrade-failpoint");
    let path = sqlite_v2_copy(temp.path());
    let error = run(&path, &AtomicBool::new(false), Some(13)).0.unwrap_err();
    assert!(matches!(error, UpgradeError::Unknown(_)), "{error:?}");
    assert_eq!(sqlite_format(&path).unwrap().version, Some(3));
}

#[test]
fn format_evidence_names_the_variant() {
    let temp = Temp::new("upgrade-evidence");
    let path = sqlite_v2_copy(temp.path());
    let evidence = sqlite_format_evidence(&path).unwrap();
    assert_eq!((evidence.raw, evidence.variant.as_deref()), (42, Some("d")));
    assert_eq!(evidence.format.status, FormatStatus::Outdated);
    raw(&path, "PRAGMA user_version=82");
    let newer = sqlite_format(&path).unwrap();
    assert_eq!(
        (newer.version, newer.status),
        (Some(4), FormatStatus::Newer)
    );
    raw(&path, "PRAGMA user_version=200");
    let beyond = sqlite_format(&path).unwrap();
    assert_eq!((beyond.version, beyond.status), (None, FormatStatus::Newer));
    let UpgradeError::Unsupported(text) = run(&path, &AtomicBool::new(false), None).0.unwrap_err()
    else {
        panic!("user_version 200 is refused Unsupported")
    };
    assert!(text.contains(layerfs_persistence::upgrade::NEWER), "{text}");
    raw(&path, "PRAGMA user_version=22");
    assert_eq!(
        sqlite_format(&path).unwrap().status,
        FormatStatus::Unsupported
    );
    std::fs::write(temp.join("empty.sqlite"), b"").unwrap();
    let empty = sqlite_format(&temp.join("empty.sqlite")).unwrap();
    assert_eq!(
        (empty.version, empty.status),
        (None, FormatStatus::Unsupported)
    );
}

/// L: in WAL mode every open connection holds a SHARED lock on the file,
/// so `locking_mode=EXCLUSIVE` fails its first read while one is open.
#[test]
fn sqlite_exclusive_mode_fails_while_another_connection_is_open() {
    let temp = Temp::new("upgrade-l1");
    let path = sqlite_v2_copy(temp.path());
    let other = Connection::open(&path).unwrap();
    let _: i64 = other
        .query_row("SELECT count(*) FROM layer", [], |r| r.get(0))
        .unwrap();
    // Idle: no transaction is open on it now.
    let mine = Connection::open(&path).unwrap();
    mine.busy_timeout(std::time::Duration::ZERO).unwrap();
    mine.execute_batch("PRAGMA locking_mode=EXCLUSIVE").unwrap();
    let error = mine
        .query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))
        .unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy),
        "{error}"
    );
    drop(mine);
    other.close().unwrap();
    let mine = Connection::open(&path).unwrap();
    mine.execute_batch("PRAGMA locking_mode=EXCLUSIVE").unwrap();
    let _: i64 = mine
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .unwrap();
    // And it now keeps every new opener out.
    let late = Connection::open(&path).unwrap();
    late.busy_timeout(std::time::Duration::ZERO).unwrap();
    assert!(late
        .query_row("SELECT 1 FROM layer", [], |r| r.get::<_, i64>(0))
        .is_err());
}

/// L: `quick_check` and `integrity_check` skip CHECK constraints while
/// `ignore_check_constraints` is on, and report them once it is off.
#[test]
fn sqlite_checks_skip_check_constraints_while_they_are_ignored() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(
        "CREATE TABLE t(v INTEGER CHECK(v=1)) STRICT; PRAGMA ignore_check_constraints=ON; \
         INSERT INTO t VALUES(2);",
    )
    .unwrap();
    for pragma in ["quick_check", "integrity_check"] {
        let on: String = c
            .query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(on, "ok", "{pragma} while ignored");
    }
    c.execute_batch("PRAGMA ignore_check_constraints=OFF")
        .unwrap();
    for pragma in ["quick_check", "integrity_check"] {
        let off: String = c
            .query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
            .unwrap();
        assert!(off.contains("CHECK constraint failed"), "{pragma}: {off}");
    }
}

/// The doctor's variant reads what the upgrade would see: a committed WAL
/// frame that changed the recorded source (not yet checkpointed) makes the
/// Store no longer variant (d), though the main file alone still says so.
#[test]
fn format_evidence_reads_through_the_wal() {
    let temp = Temp::new("upgrade-evidence-wal");
    let path = sqlite_v2_copy(temp.path());
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "PRAGMA wal_autocheckpoint=0; \
             UPDATE history_meta SET schema_source=schema_source||' '",
        )
        .unwrap();
    let evidence = sqlite_format_evidence(&path).unwrap();
    assert_eq!((evidence.raw, evidence.variant), (42, None));
    drop(writer);
    let reverted = sqlite_v2_copy(temp.path());
    assert_eq!(
        sqlite_format_evidence(&reverted)
            .unwrap()
            .variant
            .as_deref(),
        Some("d")
    );
}

/// A read-only session cannot remove its sidecars when it closes, so a
/// `-shm` can be beside the Store before an upgrade (#819: a Store call's
/// read-only control session). The run's exclusive connection keeps its
/// WAL index in heap memory and never opens it, so that `-shm` is not the
/// run's leftover and is no warning; a `-wal` left would still be one.
#[test]
fn a_shm_a_read_only_session_left_is_not_the_upgrades_leftover() {
    let temp = Temp::new("upgrade-shm-before");
    let path = sqlite_v2_copy(temp.path());
    let side = |suffix: &str| path.with_file_name(format!("store.sqlite{suffix}"));
    let reader =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let tables: i64 = reader
        .query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))
        .unwrap();
    assert!(tables > 0);
    reader.close().unwrap();
    assert!(side("-shm").exists(), "the read-only close keeps its -shm");
    let (answer, _) = run(&path, &AtomicBool::new(false), None);
    let report = answer.expect("upgraded");
    assert_eq!(report.outcome, UpgradeOutcome::Upgraded);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}
