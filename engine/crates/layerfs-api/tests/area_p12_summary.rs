//! P12 cheap Store summary over real disposable Stores and public SDK values.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::{
    control::{SdkCall, SdkRefusal, SdkValue},
    sdk_values::hex,
};
use layerfs_content::ObjectId;
use layerfs_history::{
    BranchId, CommitMeta, CommitOutcome, CommitRecord, CommitRequest, CommitVia,
    HistoryCatalogConfig,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use layerfs_store_service::operator;
use std::fs;
use support::{write, Fixture};

fn try_summary(fixture: &Fixture, args: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let call = SdkCall {
        method: "backend.store.project.get_summary".into(),
        args,
    };
    operator::dispatch(&fixture.service, &call)
}

fn summary(fixture: &Fixture) -> SdkValue {
    try_summary(
        fixture,
        SdkValue::map([("project", SdkValue::text("installed"))]),
    )
    .unwrap_or_else(|refusal| panic!("get_summary refused: {refusal:?}"))
}

fn empty_fixture() -> Fixture {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-installed-{}-p12-empty",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let sealed = directory.join("sealed.sqlite");
    let config = PersistenceConfig::sqlite(&sealed)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
    let handles = Handles::create(
        config.clone(),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: installed::BINDING.to_vec(),
            cursor_key: installed::CURSOR,
            incarnation: 1,
        },
    )
    .unwrap();
    let sealed = handles.seal().unwrap();
    let installed_path = directory.join("store.sqlite");
    fs::copy(sealed.path, &installed_path).unwrap();
    fs::remove_file(&config.path).unwrap();
    Fixture::open(installed::Fixture {
        directory,
        config: PersistenceConfig {
            path: installed_path,
            ..config
        },
        branch: BranchId::from_authority([24; 16]),
        count: 0,
    })
}

fn main_commit(fixture: &Fixture, label: &[u8]) -> CommitRecord {
    let branch = fixture.host.branch;
    let snapshot = fixture
        .service
        .store
        .history()
        .branch_snapshot(branch)
        .unwrap()
        .unwrap();
    match fixture
        .service
        .store
        .history()
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            candidate_root: ObjectId::for_bytes(label),
            profile: snapshot.profile,
            scope: snapshot.scope,
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Workspace),
            force: false,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("main Commit outcome: {other:?}"),
    }
}

fn assert_exact_keys(value: &SdkValue) {
    let SdkValue::Map(fields) = value else {
        panic!("summary must be a map: {value:?}");
    };
    let mut names = fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        ["branches", "commits", "lastCommit", "logical", "mainLayer"]
    );
}

#[test]
fn empty_store_has_zero_counts_and_no_main_head() {
    let fixture = empty_fixture();
    let value = summary(&fixture);
    assert_exact_keys(&value);
    assert_eq!(value.get_int("logical"), Some(0));
    assert_eq!(value.get_int("branches"), Some(0));
    assert_eq!(value.get_int("commits"), Some(0));
    assert_eq!(value.get("mainLayer"), Some(&SdkValue::Null));
    assert_eq!(value.get("lastCommit"), Some(&SdkValue::Null));
    fixture.cleanup();
}

#[test]
fn summary_matches_independent_branch_commit_and_main_head_facts() {
    let fixture = Fixture::new("p12-small", |source| {
        fs::write(source.join("base.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("empty");
    main_commit(&fixture, b"P12 main one");
    fixture.fork("feature");
    fixture.apply("feature", vec![write("/feature.txt", "feature\n")]);
    let record = main_commit(&fixture, b"P12 main two");

    let value = summary(&fixture);
    assert_exact_keys(&value);
    let storage = operator::dispatch(
        &fixture.service,
        &SdkCall {
            method: "backend.store.project.get_storage".into(),
            args: SdkValue::map([("project", SdkValue::text("installed"))]),
        },
    )
    .unwrap();
    assert_eq!(value.get_int("logical"), storage.get_int("logical"));
    assert_eq!(value.get_int("branches"), Some(3));
    assert_eq!(value.get_int("commits"), Some(3));
    assert_eq!(
        value.get_text("mainLayer"),
        Some(hex(record.base_layer.as_slice()).as_str())
    );
    let last = value.get("lastCommit").expect("main head Commit");
    assert_eq!(last.get_text("id"), Some(record.id.to_string().as_str()));
    assert_eq!(last.get_int("createdMs"), Some(record.meta.time_ms));
    fixture.cleanup();
}

#[test]
fn summary_statement_and_row_work_is_constant_at_two_store_sizes() {
    let fixture = Fixture::new("p12-work", |_source| 0);
    main_commit(&fixture, b"P12 work main");
    let before = fixture.work();
    let small_value = summary(&fixture);
    let small = fixture.since(before);
    assert_eq!(small_value.get_int("branches"), Some(1));
    assert_eq!(small_value.get_int("commits"), Some(1));

    for branch in 0..4 {
        let name = format!("branch-{branch}");
        fixture.fork(&name);
        for commit in 0..3 {
            fixture.apply(
                &name,
                vec![write(&format!("/{name}-{commit}.txt"), "growth\n")],
            );
        }
    }
    let before = fixture.work();
    let large_value = summary(&fixture);
    let large = fixture.since(before);
    assert_eq!(large_value.get_int("branches"), Some(5));
    assert_eq!(large_value.get_int("commits"), Some(13));
    println!(
        "P12_SUMMARY_WORK small_statements={} large_statements={} small_rows={} large_rows={}",
        small.statements, large.statements, small.rows, large.rows
    );
    assert_eq!(
        small.statements, 7,
        "SummaryBound statement count (5 queries plus snapshot BEGIN and COMMIT)"
    );
    assert_eq!(small.rows, 5, "SummaryBound returned-row count");
    assert_eq!(large.statements, small.statements);
    assert_eq!(large.rows, small.rows);
    fixture.cleanup();
}

#[test]
fn summary_refuses_unknown_options_and_non_object_arguments() {
    let fixture = Fixture::new("p12-refusals", |_source| 0);
    let unknown = try_summary(
        &fixture,
        SdkValue::map([
            ("project", SdkValue::text("installed")),
            ("unknown", SdkValue::Bool(true)),
        ]),
    )
    .expect_err("unknown option must be refused");
    assert_eq!(unknown.code, "InvalidInput");
    assert!(unknown.message.contains("unknown option"));

    for args in [SdkValue::Int(1), SdkValue::List(Vec::new())] {
        let refusal = try_summary(&fixture, args).expect_err("non-map must be refused");
        assert_eq!(refusal.code, "InvalidInput");
        assert!(refusal.message.contains("options must be a map"));
    }
    fixture.cleanup();
}
