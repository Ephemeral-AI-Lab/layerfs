//! Real control Service with process helpers, no kernel NFS mount required.
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use layerfs_bridge::control::{MountPhase, Reply, Request, UnmountControl};
use layerfs_daemon::control::{ExportServing, ExportServingConfig, Helper, Mounter, HELPER_WAIT};
use layerfs_history::WorkspaceId;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{mpsc, Arc},
    time::Duration,
};

fn base(label: &str) -> PathBuf {
    let root = std::env::var_os("H07_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join(format!("helper-{}-{label}", std::process::id()))
}
fn run_case(label: &str, helper: Helper, before: impl FnOnce()) -> (String, MountPhase) {
    let base = base(label);
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("mounts")).unwrap();
    fs::write(base.join("src/file"), b"fixture").unwrap();
    let rig = rig::Rig::new(
        &base.join("src"),
        &base.join("work"),
        Duration::from_secs(5),
    );
    let service = rig.service(Arc::new(ExportServing::start(ExportServingConfig::new(
        base.join("mounts"),
        501,
        20,
        Mounter {
            mount: Helper::words("true").unwrap(),
            unmount: helper,
            force: Helper::words("true").unwrap(),
            flush: None,
        },
    ))));
    let token = match service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([33; 32]).unwrap(),
            branch: rig.branch,
        })
        .unwrap()
        .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    service.execute_control(&Request::Attach(token)).unwrap();
    before();
    let reply = format!(
        "{:?}",
        service.execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true
        }))
    );
    let phase = match service.execute_control(&Request::Status(token)) {
        Ok(done) => match done.reply {
            Reply::Status(status) => status.kernel.unwrap().phase,
            other => panic!("{other:?}"),
        },
        Err(_) => MountPhase::Unattached,
    };
    if phase == MountPhase::Retained {
        let repeated = format!(
            "{:?}",
            service.execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true
            }))
        );
        assert_eq!(
            reply, repeated,
            "retained request must not launch another helper"
        );
    }
    // No actual kernel mount exists in this fixture. Service owns retained
    // export custody until drop.
    drop(service);
    drop(rig);
    fs::remove_dir_all(base).unwrap();
    (reply, phase)
}
#[test]
fn exited_helper_does_not_wait_for_a_descendants_pipe_eof() {
    let root = base("pipe-holder");
    fs::create_dir_all(&root).unwrap();
    let pidfile = root.join("descendant.pid");
    let script = format!("sleep 20 & echo $! > {}; exit 0", pidfile.display());
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run_case(
            "pipes",
            Helper(vec!["sh".into(), "-c".into(), script]),
            || {},
        );
        let _ = send.send(result);
    });
    let result = receive.recv_timeout(Duration::from_secs(3));
    // Always release the descendant, including on failure, before asserting.
    if let Ok(pid) = fs::read_to_string(&pidfile) {
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
    }
    worker.join().unwrap();
    fs::remove_dir_all(root).unwrap();
    let (reply, _) = result.expect("exited helper blocked on descendant's open pipe");
    assert!(reply.contains("Unmounted"), "{reply}");
}
#[test]
fn timed_out_helper_returns_retained_and_does_not_replay() {
    // STOP makes the helper ignore TERM; KILL must reap it without wait().
    // A normal process cannot ignore SIGKILL: this tests the actual deadline,
    // not the kernel's uninterruptible U state.
    let (reply, phase) = run_case(
        "timeout",
        Helper(vec!["sh".into(), "-c".into(), "kill -STOP $$".into()]),
        || {},
    );
    assert!(
        reply.contains("Retained") && reply.contains("60s"),
        "{reply}"
    );
    assert_eq!(phase, MountPhase::Retained);
    assert_eq!(HELPER_WAIT, Duration::from_secs(60));
}

#[test]
fn verbose_helper_preserves_busy_refusal_without_pipe_deadlock() {
    let (reply, phase) = run_case(
        "verbose",
        Helper(vec!["sh".into(), "-c".into(), "echo busy >&2; i=0; while [ $i -lt 5000 ]; do echo verbose-helper-output; i=$((i+1)); done; exit 32".into()]),
        || {},
    );
    assert!(reply.to_ascii_lowercase().contains("busy"), "{reply}");
    assert_eq!(phase, MountPhase::Ready);
}
