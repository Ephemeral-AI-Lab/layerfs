//! `store upgrade` on PostgreSQL (#804 §5.2), engine level, on the frozen
//! schema-3 fixture restored into a fresh database per test: the golden
//! upgrade (A) with its counts (J), a working Store and the doctor's
//! admission afterwards (B, D), refusals that change nothing (F), cancel
//! (H) and the incarnation bump (I). NOT_RUN without
//! `LAYERFS_TEST_POSTGRES_URL`.
use crate::support;

use layerfs_history::records::{CommitMeta, CommitOutcome, CommitRequest, CommitVia};
use layerfs_history::{
    CommitAttempt, CommitId, HistoryCatalog, HistoryName, LayerId, NamedSnapshot,
};
use layerfs_persistence::upgrade::{
    postgresql_format, postgresql_format_evidence, upgrade_postgresql, FormatStatus, UpgradeError,
    UpgradeOutcome, UpgradeReport, UpgradeRequest, UpgradeWork, POSTGRESQL_PHASES,
};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use support::pg::fixture::{FixtureDb, SCHEMA};
use support::upgrade::{fixtures, hex, manifest_branches, unhex};

fn run(
    db: &FixtureDb,
    cancel: &AtomicBool,
) -> (Result<UpgradeReport, UpgradeError>, Vec<&'static str>) {
    let mut phases = Vec::new();
    let request = UpgradeRequest {
        cancel,
        failpoint: None,
    };
    let answer = upgrade_postgresql(&db.url, SCHEMA, &request, &mut |p| phases.push(p));
    (answer, phases)
}

fn binding(db: &FixtureDb) -> Vec<u8> {
    db.client()
        .query_one("SELECT binding_key FROM history_meta", &[])
        .unwrap()
        .get(0)
}

fn incarnation(db: &FixtureDb) -> i64 {
    db.client()
        .query_one("SELECT catalog_incarnation FROM history_meta", &[])
        .unwrap()
        .get(0)
}

fn ids(db: &FixtureDb) -> Vec<Vec<u8>> {
    db.client()
        .query(
            "SELECT commit_id FROM \"commit\" UNION ALL SELECT layer_id FROM layer ORDER BY 1",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect()
}

fn selection(db: &FixtureDb) -> PostgresSelection {
    PostgresSelection::new(&db.url, SCHEMA)
}

#[test]
fn golden_upgrade_keeps_every_head_and_commit_in_two_round_trips() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let db = FixtureDb::restore(&admin, "golden");
    let format = postgresql_format(&db.url, SCHEMA).unwrap();
    assert_eq!(
        (format.version, format.status),
        (Some(3), FormatStatus::Outdated)
    );
    let evidence = postgresql_format_evidence(&db.url, SCHEMA).unwrap();
    assert_eq!(
        (evidence.raw, evidence.variant.as_deref()),
        (3, Some("25c261b205"))
    );
    let before_ids = ids(&db);
    let g = incarnation(&db);
    let started = Instant::now();
    let (answer, phases) = run(&db, &AtomicBool::new(false));
    let report = answer.expect("upgraded");
    eprintln!("upgrade took {:?}", started.elapsed());
    assert_eq!(report.outcome, UpgradeOutcome::Upgraded);
    assert_eq!(report.engine, "postgresql");
    assert_eq!((report.format_before, report.format_after), (3, 4));
    assert_eq!(report.steps, vec!["3->4".to_owned()]);
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.work,
        UpgradeWork {
            connects: 1,
            round_trips: 2,
            write_transactions: 1
        }
    );
    assert_eq!(phases, POSTGRESQL_PHASES.to_vec());
    assert_eq!(ids(&db), before_ids);
    // I: every session opened before the upgrade fences on G and fails Stale.
    assert_eq!(incarnation(&db), g + 1);
    let format = postgresql_format(&db.url, SCHEMA).unwrap();
    assert_eq!(
        (format.version, format.status),
        (Some(4), FormatStatus::Current)
    );

    let handles = PostgresHandles::open_read_only(&selection(&db), &binding(&db), [1; 32]).unwrap();
    let manifest = manifest_branches(&fixtures().join("postgresql/v3/manifest.json"));
    assert!(!manifest.is_empty());
    for (project, branch, head, commits) in manifest {
        let NamedSnapshot::Found(snapshot) = handles
            .history
            .branch_snapshot_named(
                &HistoryName::new(&project).unwrap(),
                &HistoryName::new(&branch).unwrap(),
            )
            .unwrap()
        else {
            panic!("{project}/{branch} missing");
        };
        let actual = match (head.starts_with("32"), snapshot.branch.head_commit) {
            (false, Some(id)) => hex(id.as_slice()),
            _ => hex(handles
                .history
                .layer_stack_named(&HistoryName::new(&project).unwrap())
                .unwrap()
                .unwrap()
                .head_layer
                .as_slice()),
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

    let digest = db.digest();
    let (again, phases) = run(&db, &AtomicBool::new(false));
    let again = again.unwrap();
    assert_eq!(again.outcome, UpgradeOutcome::Current);
    assert_eq!(
        again.work,
        UpgradeWork {
            connects: 1,
            round_trips: 1,
            write_transactions: 0
        }
    );
    assert_eq!(phases, vec!["check"]);
    assert_eq!(db.digest(), digest);
}

#[test]
fn upgraded_store_has_the_frozen_schema_takes_commits_and_passes_the_doctor() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let db = FixtureDb::restore(&admin, "works");
    run(&db, &AtomicBool::new(false)).0.unwrap();
    let mut client = db.client();
    let row = client
        .query_one(
            "SELECT schema_source, schema_definition FROM history_meta",
            &[],
        )
        .unwrap();
    let (source, definition): (String, String) = (row.get(0), row.get(1));
    let (target, current) =
        layerfs_persistence::internals::postgresql_target_and_current_source(true);
    assert_eq!(source, target);
    assert_eq!(source, current);
    assert_eq!(
        definition,
        layerfs_persistence::internals::postgresql_step_columns(true).1
    );
    assert!(definition.contains("commit history_serial bigint NO\ncommit attempt bytea YES\n"));

    let binding = binding(&db);
    let mut resolver =
        |host: &str, port: u16, _: Instant| -> std::io::Result<Vec<std::net::SocketAddr>> {
            (host, port).to_socket_addrs().map(|found| found.collect())
        };
    PostgresHandles::open_diagnostic(
        &selection(&db),
        &binding,
        [1; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .expect("the doctor's open admits the upgraded column order")
    .diagnose(
        layerfs_persistence::diagnostic::Options {
            quick: false,
            deadline: None,
            cancelled: None,
        },
        &mut |o| {
            assert!(
                o.outcome != layerfs_persistence::diagnostic::Outcome::Fail,
                "{}: {}",
                o.id,
                o.message
            )
        },
    )
    .unwrap();

    let handles = PostgresHandles::open_writable(&selection(&db), &binding, [1; 32]).unwrap();
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
    let attempt = CommitAttempt::from_bytes([9; 16]);
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

/// Applies `edit` (SQL on the Store schema) to a fresh restore and expects
/// the upgrade to be refused as `matches` says with every row unchanged.
fn refused(
    admin: &str,
    tag: &str,
    edit: &str,
    matches: impl Fn(&UpgradeError) -> bool,
) -> UpgradeError {
    let db = FixtureDb::restore(admin, tag);
    if !edit.is_empty() {
        db.client().batch_execute(edit).unwrap();
    }
    let digest = db.digest();
    let error = run(&db, &AtomicBool::new(false)).0.expect_err("refused");
    assert!(matches(&error), "{tag}: {error:?}");
    assert_eq!(db.digest(), digest, "{tag}: the refusal changed the Store");
    error
}

#[test]
fn refusals_change_nothing() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let unsupported = |e: &UpgradeError| matches!(e, UpgradeError::Unsupported(_));
    refused(
        &admin,
        "source",
        "UPDATE history_meta SET schema_source = schema_source || ' '",
        unsupported,
    );
    refused(
        &admin,
        "columns",
        "ALTER TABLE gc_dead ADD COLUMN extra bigint",
        unsupported,
    );
    let set_version = |v: i64| {
        format!(
            "ALTER TABLE history_meta DROP CONSTRAINT history_meta_schema_version_check; \
             ALTER TABLE store_policy DROP CONSTRAINT store_policy_schema_version_check; \
             UPDATE history_meta SET schema_version = {v}; UPDATE store_policy SET schema_version = {v}"
        )
    };
    for (tag, version, word) in [
        ("v2", 2, "schema version 2"),
        ("v1", 1, "schema version 1"),
        ("newer", 5, "newer than this build"),
    ] {
        let UpgradeError::Unsupported(text) =
            refused(&admin, tag, &set_version(version), unsupported)
        else {
            unreachable!()
        };
        assert!(
            text.contains(word) && !text.contains("outdated"),
            "{tag}: {text}"
        );
    }
    // A schema-2 history_meta that lacks a later column still gets its
    // named refusal, and the doctor's evidence keeps its real version.
    let v2_missing = format!(
        "{}; ALTER TABLE history_meta DROP COLUMN next_history_serial, \
         DROP COLUMN schema_definition",
        set_version(2)
    );
    let UpgradeError::Unsupported(text) = refused(&admin, "v2cols", &v2_missing, unsupported)
    else {
        unreachable!()
    };
    assert!(
        text.starts_with("PostgreSQL Store schema version 2"),
        "{text}"
    );
    {
        let db = FixtureDb::restore(&admin, "v2evidence");
        db.client().batch_execute(&v2_missing).unwrap();
        let evidence = postgresql_format_evidence(&db.url, SCHEMA).unwrap();
        assert_eq!((evidence.raw, evidence.format.version), (2, Some(2)));
        assert_eq!(evidence.format.status, FormatStatus::Unsupported);
    }
    // A Store without its lane row is refused, not converted unguarded.
    refused(
        &admin,
        "nolane",
        "DELETE FROM store_lane",
        |e| matches!(e, UpgradeError::Failed(text) if text.contains("store_lane")),
    );
    refused(
        &admin,
        "gcop",
        "INSERT INTO gc_op(id,generation,phase,started_ms) VALUES (1,1,'mark',0)",
        |e| matches!(e, UpgradeError::Busy(_)),
    );
    let missing = refused(
        &admin,
        "noschema",
        "DROP SCHEMA layerfs CASCADE; CREATE SCHEMA layerfs",
        unsupported,
    );
    assert!(matches!(missing, UpgradeError::Unsupported(_)));
}

/// An idle session of an older build holding a lock the conversion needs
/// (a row of `store_lane`, a table lock, the Store-wide GC key) refuses
/// message 2 at once (`NOWAIT`), `Busy`, with every row unchanged.
#[test]
fn an_idle_holder_refuses_message_two_busy() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    for (tag, hold) in [
        (
            "lane",
            "BEGIN; SELECT 1 FROM store_lane WHERE id=1 FOR UPDATE",
        ),
        ("table", "BEGIN; SELECT count(*) FROM \"commit\""),
        ("gckey", "SELECT pg_advisory_lock_shared(KEY)"),
    ] {
        let db = FixtureDb::restore(&admin, &format!("idle_{tag}"));
        let mut holder = db.client();
        holder
            .batch_execute(&hold.replace("KEY", &store_key(&db).to_string()))
            .unwrap();
        let digest = db.digest();
        let started = Instant::now();
        let error = run(&db, &AtomicBool::new(false)).0.unwrap_err();
        assert!(matches!(error, UpgradeError::Busy(_)), "{tag}: {error:?}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{tag}: {:?}",
            started.elapsed()
        );
        assert_eq!(db.digest(), digest, "{tag}");
        drop(holder);
    }
}

/// The Store-wide GC advisory key (`history/admission.rs`).
fn store_key(db: &FixtureDb) -> i64 {
    let catalog: Vec<u8> = db
        .client()
        .query_one("SELECT catalog_id FROM history_meta", &[])
        .unwrap()
        .get(0);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(&catalog);
    hasher.update(&[1]);
    hasher.update(&catalog);
    i64::from_be_bytes(hasher.finalize().as_bytes()[..8].try_into().unwrap())
}

#[test]
fn a_cancel_before_message_two_or_while_it_waits_changes_nothing() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    // Set in `check`: message 2 is never sent.
    let db = FixtureDb::restore(&admin, "cancel_check");
    let digest = db.digest();
    let cancel = AtomicBool::new(false);
    let request = UpgradeRequest {
        cancel: &cancel,
        failpoint: None,
    };
    let answer = upgrade_postgresql(&db.url, SCHEMA, &request, &mut |p| {
        if p == "check" {
            cancel.store(true, Ordering::SeqCst);
        }
    });
    assert_eq!(answer.unwrap_err(), UpgradeError::Cancelled);
    assert_eq!(db.digest(), digest);
    drop(db);

    // Set in `convert` while message 2 waits on a lock (an ACCESS EXCLUSIVE
    // holder of `branch`, which the assert reads): the cancel request stops
    // it, 57014, and the server rolled back.
    let db = FixtureDb::restore(&admin, "cancel_wait");
    let mut holder = db.client();
    holder
        .batch_execute("BEGIN; LOCK TABLE branch IN ACCESS EXCLUSIVE MODE")
        .unwrap();
    let digest = {
        // Read the digest without `branch` (locked); compare the version after.
        db.client()
            .query_one("SELECT schema_version FROM history_meta", &[])
            .unwrap()
            .get::<_, i64>(0)
    };
    let cancel = AtomicBool::new(false);
    let request = UpgradeRequest {
        cancel: &cancel,
        failpoint: None,
    };
    let started = Instant::now();
    let answer = upgrade_postgresql(&db.url, SCHEMA, &request, &mut |p| {
        if p == "convert" {
            cancel.store(true, Ordering::SeqCst);
        }
    });
    // The cancel request normally lands within milliseconds; on a slow host
    // the 2 s lock timeout may fire first. Either way nothing changed.
    let error = answer.unwrap_err();
    assert!(
        matches!(error, UpgradeError::Cancelled | UpgradeError::Busy(_)),
        "{error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    holder.batch_execute("ROLLBACK").unwrap();
    let after: i64 = db
        .client()
        .query_one("SELECT schema_version FROM history_meta", &[])
        .unwrap()
        .get(0);
    assert_eq!((digest, after), (3, 3));
}

/// A cancel racing COMMIT: whatever the server answered is the answer, and
/// the Store is exactly what the answer says.
#[test]
fn a_cancel_racing_commit_follows_the_server() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let mut seen = (0, 0);
    for round in 0..4 {
        let db = FixtureDb::restore(&admin, &format!("race{round}"));
        let digest = db.digest();
        let cancel = AtomicBool::new(false);
        let request = UpgradeRequest {
            cancel: &cancel,
            failpoint: None,
        };
        let answer = upgrade_postgresql(&db.url, SCHEMA, &request, &mut |p| {
            if p == "convert" {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        let version: i64 = db
            .client()
            .query_one("SELECT schema_version FROM history_meta", &[])
            .unwrap()
            .get(0);
        match answer {
            Ok(report) => {
                assert_eq!((report.outcome, version), (UpgradeOutcome::Upgraded, 4));
                seen.0 += 1;
            }
            Err(UpgradeError::Cancelled) => {
                assert_eq!(db.digest(), digest);
                seen.1 += 1;
            }
            Err(other) => panic!("round {round}: {other:?}"),
        }
    }
    eprintln!("upgraded {} cancelled {}", seen.0, seen.1);
}
