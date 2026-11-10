//! A cancel that comes before the command runs (layerfs-project#113, the
//! start race): its record has no stop handle yet, so the cancel is
//! answered `cancelled` (SIGKILL) and only marked; the owner, told the
//! handle once the command runs, is refused as interrupted and ends it.
//! Never `ended` for a command that goes on to run.
use layerfs_bridge::process::LocalProcess;
use layerfs_sandbox::exec::cancel::{cancel, unreached, Outcome};
use layerfs_sandbox::exec::id::ExecId;
use layerfs_sandbox::exec::record::{self, ExecRecord, Route, Running};
use layerfs_sandbox_backend::Stop;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

const WORKSPACE: &str = "ab12";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-start-race-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn handles() -> [Stop; 2] {
    [
        Stop::Cgroup {
            path: PathBuf::from("/sys/fs/cgroup/layerfs/exec-e1"),
        },
        Stop::Group {
            pgid: 4242,
            leader: LocalProcess {
                pid: 4242,
                start: 7,
                boot: None,
            },
        },
    ]
}

#[test]
fn a_cancel_before_the_command_runs_is_answered_cancelled_and_its_owner_ends_it() {
    for (route, handle) in [Route::Local, Route::Machine("lima".into())]
        .into_iter()
        .zip(handles())
    {
        let dir = folder(&format!("{route:?}").replace(['(', ')', '"'], ""));
        let id = ExecId::parse("e1").unwrap();
        let mut made = ExecRecord::new(&id, WORKSPACE, ("/w/1", None));
        made.route = route.clone();
        let running = Running::start(&dir, "s1", &made, &mut unreached).unwrap();
        let file = record::file(&dir, WORKSPACE, "e1");
        // the window: recorded, not yet running, so no stop handle
        assert_eq!(record::read(&file).unwrap().unwrap().stop, None);
        let mut asked = 0;
        let mut remote = |_: &ExecRecord, _: Duration| {
            asked += 1;
            Outcome::Unknown
        };
        let answer = cancel(&dir, "s1", &file, Duration::from_secs(5), &mut remote).unwrap();
        assert_eq!(answer, Outcome::Cancelled("SIGKILL"), "{route:?}");
        assert_eq!(asked, 0, "{route:?}: nothing runs yet to signal");
        let marked = record::read(&file).unwrap().unwrap();
        assert!(marked.cancel.is_some(), "{route:?}: the cancel is marked");
        // the command now runs: its owner is refused the handle and ends it
        let refused = running.spawned(&handle).unwrap_err();
        assert_eq!(refused.kind(), ErrorKind::Interrupted, "{route:?}");
        assert_eq!(record::read(&file).unwrap().unwrap().stop, None);
        assert!(running.end().unwrap(), "{route:?}: ends as cancelled");
        assert!(record::read(&file).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn a_command_told_its_handle_first_is_cancelled_through_it() {
    let dir = folder("told");
    let id = ExecId::parse("e2").unwrap();
    let mut made = ExecRecord::new(&id, WORKSPACE, ("/w/1", None));
    made.route = Route::Machine("lima".into());
    let running = Running::start(&dir, "s1", &made, &mut unreached).unwrap();
    let [handle, _] = handles();
    running.spawned(&handle).unwrap();
    let file = record::file(&dir, WORKSPACE, "e2");
    assert_eq!(record::read(&file).unwrap().unwrap().stop, Some(handle));
    let mut asked = Vec::new();
    let mut remote = |found: &ExecRecord, grace: Duration| {
        asked.push((found.stop.clone(), grace));
        Outcome::Cancelled("SIGTERM")
    };
    let answer = cancel(&dir, "s1", &file, Duration::from_secs(3), &mut remote).unwrap();
    assert_eq!(answer, Outcome::Cancelled("SIGTERM"));
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].1, Duration::from_secs(3));
    assert!(running.end().unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cancel_before_registration_is_consumed_by_start_on_each_record_route() {
    for route in [Route::Local, Route::Machine("lima".into())] {
        let dir = folder(&format!("unregistered-{route:?}").replace(['(', ')', '"'], ""));
        let id = ExecId::parse("early").unwrap();
        let file = record::file(&dir, WORKSPACE, id.as_str());
        let answer = cancel(&dir, "s1", &file, Duration::ZERO, &mut unreached).unwrap();
        assert_eq!(answer, Outcome::Ended);
        let mut made = ExecRecord::new(&id, WORKSPACE, ("/w/1", None));
        made.route = route;
        let running = Running::start(&dir, "s1", &made, &mut unreached).unwrap();
        assert_eq!(
            running.spawned(&handles()[0]).unwrap_err().kind(),
            ErrorKind::Interrupted
        );
        assert!(running.end().unwrap());
        // The mark is consumed, so immediate reuse is an independent command.
        let running = Running::start(&dir, "s1", &made, &mut unreached).unwrap();
        running.spawned(&handles()[0]).unwrap();
        assert!(!running.end().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn expired_pending_cancel_does_not_affect_id_reuse_or_other_workspaces() {
    let dir = folder("expired");
    let id = ExecId::parse("reuse").unwrap();
    let file = record::file(&dir, WORKSPACE, id.as_str());
    cancel(&dir, "s1", &file, Duration::ZERO, &mut unreached).unwrap();
    let other = ExecRecord::new(&id, "other", ("/w/2", None));
    let running = Running::start(&dir, "s1", &other, &mut unreached).unwrap();
    running.spawned(&handles()[0]).unwrap();
    assert!(!running.end().unwrap());
    layerfs_home::home::write_entity(
        &file.with_extension("cancel"),
        &layerfs_bridge::control::SdkValue::map([(
            "expires",
            layerfs_bridge::control::SdkValue::Int(1),
        )]),
    )
    .unwrap();
    let made = ExecRecord::new(&id, WORKSPACE, ("/w/1", None));
    let running = Running::start(&dir, "s1", &made, &mut unreached).unwrap();
    running.spawned(&handles()[0]).unwrap();
    assert!(!running.end().unwrap());
    assert!(!file.with_extension("cancel").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
