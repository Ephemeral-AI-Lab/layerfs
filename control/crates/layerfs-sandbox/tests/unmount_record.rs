//! The unmount guard's record file across a forced attempt (layerfs-project#136):
//! an attempt that changed nothing must leave the earlier record as it was.
use layerfs_bridge::control::{
    Request, SdkRefusal, SdkValue, TeardownCustody, TeardownStage, UnmountControl, WorkspaceToken,
};
use layerfs_history::WorkspaceId;
use layerfs_sandbox::unmount_guard::{begin_at, cancel, finish, forceable, repeat};
use layerfs_sandbox::{lifecycle, OperationCause, OperationFailure};
use layerfs_store::project::discovery;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// A Sandbox folder with its `workspace/` subfolder, both private.
fn fixture(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-unmount-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    for path in [&dir, &workspace] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}

fn record_file(dir: &Path) -> PathBuf {
    dir.join("workspace").join("01.unmount")
}

/// A completed record a force may replace: the daemon answered `Retained`
/// with its export still held at `Detach`.
fn completed() -> SdkValue {
    SdkValue::map([
        ("state", SdkValue::text("Retained")),
        ("startedMs", SdkValue::Int(7)),
        ("path", SdkValue::text("/w")),
        ("forced", SdkValue::Bool(false)),
        ("stage", SdkValue::text("Detach")),
        ("code", SdkValue::text("Failed")),
        ("message", SdkValue::text("detach not established")),
        ("detail", SdkValue::Null),
    ])
}

/// The same shape with another daemon stage, or with no stage at all (a send
/// or connect failure, or a refusal before any effect).
fn completed_at(stage: Option<&str>) -> SdkValue {
    let mut fields = Vec::new();
    for (name, value) in [
        ("state", SdkValue::text("Retained")),
        ("startedMs", SdkValue::Int(7)),
        ("path", SdkValue::text("/w")),
        ("forced", SdkValue::Bool(false)),
    ] {
        fields.push((name.to_owned(), value));
    }
    if let Some(stage) = stage {
        fields.push(("stage".to_owned(), SdkValue::text(stage)));
    }
    for (name, value) in [
        ("code", SdkValue::text("Failed")),
        ("message", SdkValue::text("detach not established")),
        ("detail", SdkValue::Null),
    ] {
        fields.push((name.to_owned(), value));
    }
    SdkValue::Map(fields)
}

fn refused(code: &str) -> SdkRefusal {
    SdkRefusal {
        code: code.into(),
        message: code.into(),
        detail: SdkValue::Null,
    }
}

/// The earlier record comes back exactly, and still answers as it did.
fn assert_restored(file: &Path, prior: &SdkValue) {
    let kept = discovery::load(file).unwrap();
    assert_eq!(kept.get_text("state"), prior.get_text("state"));
    assert_eq!(kept.get_text("code"), prior.get_text("code"));
    assert_eq!(kept.get("forced"), prior.get("forced"));
    assert_eq!(kept.get_int("startedMs"), prior.get_int("startedMs"));
    assert!(forceable(&kept), "the earlier record answers as it did");
}

#[test]
fn a_busy_force_keeps_the_record_it_replaced() {
    let dir = fixture("busy");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", true).unwrap();
    assert_eq!(
        discovery::load(&file).unwrap().get_text("state"),
        Some("Unmounting")
    );
    finish(&attempt, "/w", &Err(refused("Busy"))).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_dirty_force_keeps_the_record_it_replaced() {
    let dir = fixture("dirty");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", true).unwrap();
    finish(&attempt, "/w", &Err(refused("Dirty"))).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_call_that_sent_nothing_keeps_the_record_it_replaced() {
    let dir = fixture("cancel");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", false).unwrap();
    cancel(&attempt).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_first_attempt_that_does_not_run_leaves_no_record() {
    let dir = fixture("first");
    let file = record_file(&dir);
    let attempt = begin_at(file.clone(), "/w", false).unwrap();
    finish(&attempt, "/w", &Err(refused("Busy"))).unwrap();
    assert!(!file.exists());
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Only the daemon's own `Retained(Detach)` answer is replaceable. A stage
/// another step stopped at, or a failure that never got a stage, is answered
/// as it stands: the daemon would refuse it, and `force` must not overwrite
/// the earlier answer with an attempt of its own.
#[test]
fn a_record_force_cannot_replace_keeps_its_answer() {
    for (label, stage) in [
        ("join", Some("Join")),
        ("revoke", Some("Revoke")),
        ("close", Some("Close")),
        ("abort", Some("Abort")),
        ("no-stage", None),
    ] {
        let value = completed_at(stage);
        assert!(!forceable(&value), "{label}: {value:?}");
        let answer = repeat(&value);
        let error = answer.expect_err("a retained record answers Retained");
        assert_eq!(error.code, "Failed");
        assert_eq!(error.detail.get_text("state"), Some("Retained"));
    }
}

/// The daemon's own answer carries the stage the record keeps: a `Retained`
/// reply becomes `Failed` with `detail.stage`, while a refusal or a lost
/// exchange has no stage at all.
#[test]
fn a_retained_answer_carries_its_stage_into_the_record() {
    let token = WorkspaceToken {
        workspace: WorkspaceId::from_authority([9; 32]).unwrap(),
        namespace: 3,
    };
    let failure = |stage: TeardownStage| {
        Box::new(OperationFailure {
            request: Request::Unmount(UnmountControl {
                token,
                discard: true,
            }),
            cause: OperationCause::Retained(Box::new(TeardownCustody {
                token,
                stage,
                detached: false,
                work: None,
                detail: "client unmount failed".into(),
                forced: None,
            })),
        })
    };
    let refusal = lifecycle::operation(failure(TeardownStage::Detach));
    assert_eq!(refusal.code, "Failed");
    assert_eq!(refusal.detail.get_text("stage"), Some("Detach"));
    assert!(!refusal.detail.get_bool("detached"));

    // The same record shape the attempt publishes: the stage it keeps is the
    // daemon's, so this one is forceable and a `Join` one is not.
    for (stage, forceable_stage) in [("Detach", true), ("Join", false)] {
        let dir = fixture(&format!("stage-{stage}"));
        let file = record_file(&dir);
        let attempt = begin_at(file.clone(), "/w", false).unwrap();
        finish(&attempt, "/w", &Err(refusal_at(stage))).unwrap();
        let kept = discovery::load(&file).unwrap();
        assert_eq!(kept.get_text("stage"), Some(stage));
        assert_eq!(forceable(&kept), forceable_stage, "{stage}: {kept:?}");
        drop(attempt);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// The refusal shape `lifecycle::operation` builds from a daemon `Retained`
/// answer at `stage`.
fn refusal_at(stage: &str) -> SdkRefusal {
    SdkRefusal {
        code: "Failed".into(),
        message: "retained".into(),
        detail: SdkValue::map([("stage", SdkValue::text(stage))]),
    }
}
