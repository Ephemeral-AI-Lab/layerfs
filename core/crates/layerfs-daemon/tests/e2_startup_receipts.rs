//! Public startup functional proofs; outputs/artifacts persist for root receipts.
#[path = "../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]
#[allow(dead_code, clippy::needless_range_loop)]
mod digest;
#[path = "../examples/e2_startup/driver.rs"]
mod driver;
#[path = "../examples/e2_startup/json.rs"]
mod json;
#[path = "../examples/e2_startup/records.rs"]
mod records;
#[path = "../examples/e2_startup/streams.rs"]
mod streams;
use layerfs_daemon::{OwnerConfig, OwnerError};
use layerfs_overlay::{OverlayError, ProfileConfig};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn inputs() -> (PathBuf, PathBuf, PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "layerfs-e2-startup-proof-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).expect("fresh external proof root");
    let identity = root.join("identity-input.json");
    // Deliberately incomplete diagnostic inputs: the independent Python
    // validator must not qualify them. These proofs test the original API,
    // refusal custody and recorder, not sealed-source/topology admission.
    std::fs::write(
        &identity,
        b"{\"schema\":\"cluster-two-e2-identity-v1\",\"receipt_id\":\"external-functional-proof\"}",
    )
    .unwrap();
    (
        root.join("database.sqlite"),
        root.join("diagnostic"),
        identity,
    )
}

#[test]
fn actual_default_startup_records_then_stops_without_creating_a_route() {
    let (db, out, identity) = inputs();
    driver::collect(&db, &out, &identity).unwrap_or_else(|failure| panic!("{failure:?}"));
    let startup = std::fs::read_to_string(out.join("startup.jsonl")).unwrap();
    let probes = std::fs::read_to_string(out.join("probes.jsonl")).unwrap();
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let outcomes = std::fs::read_to_string(out.join("outcomes.json")).unwrap();
    assert_eq!(startup.lines().count(), 1);
    assert!(startup.contains("\"creation_reported\":true"));
    assert!(startup.contains("\"status\":\"READY\""));
    assert!(startup.contains("\"cleanup_headroom_bytes\":134217728"));
    assert!(startup.contains("daemon-startup-lifetime-observed-allocation"));
    assert_eq!(probes.lines().count(), 2);
    assert!(probes.contains("\"admitted\":0"));
    assert_eq!(std::fs::metadata(out.join("jobs.jsonl")).unwrap().len(), 0);
    assert!(manifest.contains("UNRUN-route_unavailable"));
    assert!(manifest.contains("\"qualification_status\":\"NOT_EVALUATED\""));
    assert!(manifest.contains("\"global_persistence\":\"NOT_IN_SCOPE\""));
    assert!(manifest.contains("\"e1_sample_status\":\"NOT_RUN\""));
    assert!(outcomes.contains("\"stop_status\":\"STOPPED\""));
    assert!(outcomes.contains("\"route\":null"));
    assert!(
        db.is_file(),
        "original backing artifact retained after Stop"
    );
    println!(
        "E2_STARTUP_PROOF output={} database={} diagnostic_only=true",
        out.display(),
        db.display()
    );
}

#[test]
fn pre_creation_refusal_keeps_original_error_and_emits_unavailable_work() {
    let (db, out, identity) = inputs();
    let failure = driver::collect_with(
        &db,
        &out,
        &identity,
        ProfileConfig::default(),
        OwnerConfig {
            bytes: 1,
            ..Default::default()
        },
    )
    .expect_err("original invalid admission must refuse");
    assert!(matches!(
        &failure.original_startup_error,
        Some(OwnerError::InvalidAdmission)
    ));
    assert!(failure.retained_owner.is_none());
    assert!(!db.exists());
    let startup = std::fs::read_to_string(out.join("startup.jsonl")).unwrap();
    assert!(startup.contains("\"creation_reported\":false"));
    assert!(startup.contains("\"status\":\"UNAVAILABLE\",\"work\":null"));
    assert_eq!(
        std::fs::metadata(out.join("probes.jsonl")).unwrap().len(),
        0
    );
    let outcomes = std::fs::read_to_string(out.join("outcomes.json")).unwrap();
    assert!(outcomes.contains("UNRUN-no-ready-owner"));
    println!(
        "E2_STARTUP_ORIGINAL_REFUSAL output={} cause={failure:?}",
        out.display()
    );
}

#[test]
fn failed_schema_attempt_retains_real_sql_and_created_artifact() {
    let (db, out, identity) = inputs();
    let failure = driver::collect_with(
        &db,
        &out,
        &identity,
        ProfileConfig {
            max_pages: Some(32),
            ..Default::default()
        },
        OwnerConfig::default(),
    )
    .expect_err("small owning quota preserves original schema failure");
    assert!(matches!(
        &failure.original_startup_error,
        Some(OwnerError::Overlay(OverlayError::Sql(_)))
    ));
    let work = failure.original_creation.as_ref().unwrap();
    assert_eq!(work.file_create_calls, 1);
    assert!(work.sql.total().attempts > 0);
    assert!(db.is_file());
    let startup = std::fs::read_to_string(out.join("startup.jsonl")).unwrap();
    assert!(startup.contains("\"creation_reported\":true"));
    assert!(startup.contains("\"status\":\"FAILED\""));
    assert!(out.join("manifest.json").is_file());
    println!(
        "E2_STARTUP_ORIGINAL_SCHEMA_FAILURE output={} database={} cause={failure:?}",
        out.display(),
        db.display()
    );
}

#[test]
fn existing_output_is_not_truncated_and_startup_is_unattempted() {
    let (db, out, identity) = inputs();
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("marker"), b"retained earlier output").unwrap();
    let failure = driver::collect(&db, &out, &identity).expect_err("unused output required");
    assert!(failure.original_creation.is_none());
    assert!(!db.exists());
    assert_eq!(
        std::fs::read(out.join("marker")).unwrap(),
        b"retained earlier output"
    );
    assert!(!out.join("startup.jsonl").exists());
}

#[test]
fn external_receipt_identities_exist_before_any_startup_attempt() {
    let (db, out, identity) = inputs();
    let recorder = streams::Recorder::create(&db, &out, &identity).unwrap();
    assert!(!db.exists());
    assert_eq!(recorder.startup_record_id.len(), 64);
    assert_eq!(recorder.attempt_id.len(), 64);
    assert_eq!(recorder.owner_id.len(), 64);
    assert_ne!(recorder.startup_record_id, recorder.attempt_id);
    assert_ne!(recorder.attempt_id, recorder.owner_id);
    assert_eq!(recorder.startup.records, 0);
    assert_eq!(
        recorder.invocation[1],
        db.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(db.file_name().unwrap())
    );
    assert_eq!(
        recorder.invocation[2],
        out.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(out.file_name().unwrap())
    );
    assert_eq!(recorder.invocation[3], identity.canonicalize().unwrap());
}

#[test]
fn bounded_external_encoder_escapes_and_refuses_growth() {
    assert_eq!(
        streams::compact_identity("{\n \"value\": \"space stays \\\" λ\"\n}").unwrap(),
        "{\"value\":\"space stays \\\" λ\"}"
    );
    assert!(streams::compact_identity("{\"raw\":\"bad\nline\"}").is_err());
    let mut value = json::Json::new();
    value.string("quote\"\\\n\u{0000}λ").unwrap();
    assert_eq!(
        value.finish().unwrap(),
        b"\"quote\\\"\\\\\\n\\u0000\xce\xbb\"\n"
    );
    let mut value = json::Json::new();
    value.raw(&"a".repeat(json::WINDOW)).unwrap();
    assert!(value.raw("b").is_err());
}

#[test]
fn reused_external_streaming_hash_matches_independent_standard_vectors() {
    let mut hash = digest::Sha256::new();
    hash.update(b"a");
    hash.update(b"bc");
    assert_eq!(
        digest::hex(&hash.finish()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        digest::hex(&digest::sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
