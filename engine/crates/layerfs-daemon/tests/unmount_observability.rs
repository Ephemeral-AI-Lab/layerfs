//! The normal unmount's abort path as an observation (issue #2): the one
//! telemetry section a traced reply carries, the one stderr line, and the
//! silence of an unmount that never took that path.
//!
//! The abort path itself is staged by `stalled_unmount`; these tests pin the
//! stable key set both forms render and the absence of any observation when
//! the drain never stalled.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{MountWork, SdkValue, TeardownStage, WorkspaceToken};
use layerfs_daemon::{control::UnmountAbort, telemetry};
use layerfs_fuse::mount::AbortWrite;
use layerfs_history::WorkspaceId;
use nix::errno::Errno;
use rig::Rig;
use std::time::Duration;

/// One observation of a connection that stalled at `Join` and was aborted.
fn observation(abort: AbortWrite) -> UnmountAbort {
    UnmountAbort {
        token: WorkspaceToken {
            workspace: WorkspaceId::from_authority([9; 32]).unwrap(),
            namespace: 3,
        },
        stage: TeardownStage::Join,
        abort,
        detached: true,
        work: MountWork {
            loops_configured: 4,
            loops_entered: 4,
            loops_exited: 4,
            loops_joined: 4,
            active: 0,
            retained: 0,
            completed: 3,
            ..MountWork::default()
        },
        waited: Duration::from_millis(7),
    }
}

/// The section's field names, in the order the daemon writes them.
fn names(section: &SdkValue) -> Vec<String> {
    match section {
        SdkValue::Map(fields) => fields.iter().map(|(name, _)| name.clone()).collect(),
        other => panic!("not a section: {other:?}"),
    }
}

#[test]
fn the_abort_section_keeps_one_key_set_for_every_abort_result() {
    let written = telemetry::unmount_abort(&observation(AbortWrite::Written));
    assert_eq!(
        names(&written),
        [
            "stage",
            "abort",
            "detached",
            "loops_exited",
            "loops_joined",
            "active",
            "retained",
            "waited_ms"
        ]
    );
    assert_eq!(written.get_text("stage"), Some("Join"));
    assert_eq!(written.get_text("abort"), Some("Written"));
    assert!(written.get_bool("detached"));
    assert_eq!(written.get_int("loops_exited"), Some(4));
    assert_eq!(written.get_int("loops_joined"), Some(4));
    assert_eq!(written.get_int("active"), Some(0));
    assert_eq!(written.get_int("retained"), Some(0));
    assert_eq!(written.get_int("waited_ms"), Some(7));
    assert!(written.get("bytes").is_none() && written.get("errno").is_none());

    // A short write names its count, a failed one its errno, and only then.
    let short = telemetry::unmount_abort(&observation(AbortWrite::Short(0)));
    assert_eq!(short.get_text("abort"), Some("Short"));
    assert_eq!(short.get_int("bytes"), Some(0));
    assert!(short.get("errno").is_none());
    let failed = telemetry::unmount_abort(&observation(AbortWrite::Failed(Errno::EIO)));
    assert_eq!(failed.get_text("abort"), Some("Failed"));
    assert_eq!(failed.get_text("errno"), Some("EIO"));
    assert!(failed.get("bytes").is_none());
}

#[test]
fn the_stderr_line_carries_the_same_fields_as_the_section() {
    let line = telemetry::unmount_abort_line(&observation(AbortWrite::Written));
    assert!(
        line.starts_with("LAYERFS_DAEMON_UNMOUNT_ABORT workspace=09090909"),
        "{line}"
    );
    assert!(
        line.ends_with(
            "namespace=3 stage=Join abort=Written detached=true loops_exited=4 loops_joined=4 active=0 retained=0 waited_ms=7"
        ),
        "{line}"
    );
    let failed = telemetry::unmount_abort_line(&observation(AbortWrite::Failed(Errno::EIO)));
    assert!(
        failed.contains(" abort=Failed errno=EIO detached=true "),
        "{failed}"
    );
    let short = telemetry::unmount_abort_line(&observation(AbortWrite::Short(0)));
    assert!(
        short.contains(" abort=Short bytes=0 detached=true "),
        "{short}"
    );
}

#[test]
fn an_unmount_that_never_stalled_reports_no_abort_path() {
    let rig = Rig::new("unmount-observability");
    let ready = rig.mount(1);
    let done = rig.harness.unmount(&ready);
    assert!(done.abort.is_none(), "{:?}", done.abort);
    assert!(
        rig.harness.service.retained_abort(ready.token).is_none(),
        "a completed unmount retains no abort observation"
    );
    rig.finish();
}
