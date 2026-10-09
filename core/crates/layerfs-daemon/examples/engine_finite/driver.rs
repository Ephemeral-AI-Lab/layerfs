//! One finite admitted arrival trace, retaining each original completion.
use super::receipt::Recorder;
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, Pending, Response, ServiceClass,
};
use layerfs_overlay::{
    Capture, Inode, InodeKind, OperationOwner, OperationRecord, ProfileConfig, Route,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
const CLASSES: [ServiceClass; 6] = [
    ServiceClass::Read,
    ServiceClass::Mutation,
    ServiceClass::Capture,
    ServiceClass::Lifecycle,
    ServiceClass::OperationRecord,
    ServiceClass::Source,
];
struct Workspace {
    route: Route,
    capture: Capture,
    operation: OperationOwner,
}
fn wait(pending: Pending) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "bounded original job result");
        std::thread::yield_now();
    }
}
fn call(
    client: &OwnerClient,
    rec: &mut Recorder,
    ws: usize,
    route: Option<Route>,
    class: ServiceClass,
    phase: &str,
    command: Command,
) -> Completion {
    let pending = client.try_submit(route, command).unwrap();
    let done = wait(pending);
    rec.job(ws, class, phase, &done);
    done
}
fn inode(wave: u64) -> Inode {
    Inode {
        serial: 2,
        kind: InodeKind::File,
        mode: 0o640,
        mtime_seconds: wave as i64,
        mtime_nanoseconds: 0,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
fn mutation(wave: u64) -> Command {
    Command::Publish {
        inode: inode(wave),
        name: None,
        cell: None,
    }
}
pub fn run(directory: &Path, receipts: &Path, mut phase: impl FnMut(&str)) {
    std::fs::create_dir(directory).unwrap();
    let mut rec = Recorder::new(receipts);
    phase("startup");
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let mut workspaces = Vec::with_capacity(4);
    phase("setup");
    for ws in 0..4 {
        let done = call(
            &client,
            &mut rec,
            ws,
            None,
            ServiceClass::Lifecycle,
            "setup",
            Command::Open {
                incarnation: [ws as u8 + 1; 32],
                base_root: [17; 32],
            },
        );
        let route = match done.result() {
            Ok(Response::Opened(route)) => *route,
            other => panic!("{other:?}"),
        };
        drop(done);
        let done = call(
            &client,
            &mut rec,
            ws,
            Some(route),
            ServiceClass::Mutation,
            "setup",
            mutation(0),
        );
        let publication = match done.result() {
            Ok(Response::Published(value)) => *value,
            other => panic!("{other:?}"),
        };
        drop(done);
        drop(call(
            &client,
            &mut rec,
            ws,
            Some(route),
            ServiceClass::Lifecycle,
            "setup",
            Command::ReplyAttempted(publication),
        ));
        let done = call(
            &client,
            &mut rec,
            ws,
            Some(route),
            ServiceClass::Lifecycle,
            "setup",
            Command::AcquireOperation { request: 1 },
        );
        let operation = match done.result() {
            Ok(Response::Operation(Some(value))) => *value,
            other => panic!("{other:?}"),
        };
        drop(done);
        let done = call(
            &client,
            &mut rec,
            ws,
            Some(route),
            ServiceClass::Capture,
            "setup",
            Command::Capture,
        );
        let capture = match done.result() {
            Ok(Response::Captured(value)) => *value,
            other => panic!("{other:?}"),
        };
        drop(done);
        workspaces.push(Workspace {
            route,
            capture,
            operation,
        });
    }
    phase("arrivals");
    for wave in 0..32 {
        let mut pending = Vec::with_capacity(24);
        for (ws, bound) in workspaces.iter().enumerate() {
            for (index, class) in CLASSES.into_iter().enumerate() {
                let command = match index {
                    0 => Command::Inode(2),
                    1 => mutation(wave + 1),
                    2 => Command::CapturedInodes {
                        capture: bound.capture,
                        after: 0,
                    },
                    3 => Command::State,
                    4 => Command::PutOwnedOperationRecord {
                        owner: bound.operation,
                        record: OperationRecord {
                            kind: 1,
                            key: wave,
                            value: vec![wave as u8; 128],
                        },
                    },
                    5 => Command::AcquireBaseSource { owner: wave + 100 },
                    _ => unreachable!(),
                };
                pending.push((
                    ws,
                    class,
                    client.try_submit(Some(bound.route), command).unwrap(),
                ));
            }
        }
        let mut dependencies = Vec::with_capacity(8);
        for (ws, class, original) in pending {
            let done = wait(original);
            rec.job(ws, class, "wave", &done);
            match done.result() {
                Ok(Response::Published(value)) => {
                    dependencies.push((ws, Command::ReplyAttempted(*value)))
                }
                Ok(Response::BaseSource(value)) => {
                    dependencies.push((ws, Command::ReleaseBaseSource(*value)))
                }
                _ => (),
            }
        }
        for (ws, command) in dependencies {
            drop(call(
                &client,
                &mut rec,
                ws,
                Some(workspaces[ws].route),
                ServiceClass::Lifecycle,
                "dependency",
                command,
            ));
        }
    }
    assert_eq!(client.diagnostics().unwrap().closed_namespaces, 0);
    phase("release");
    for (ws, bound) in workspaces.iter().enumerate() {
        drop(call(
            &client,
            &mut rec,
            ws,
            Some(bound.route),
            ServiceClass::Lifecycle,
            "release",
            Command::ReleaseOperation(bound.operation),
        ));
        drop(call(
            &client,
            &mut rec,
            ws,
            Some(bound.route),
            ServiceClass::Lifecycle,
            "release",
            Command::Close,
        ));
    }
    // Every logical Close still holds its original capture.
    assert_eq!(client.diagnostics().unwrap().closed_namespaces, 0);
    for (ws, bound) in workspaces.iter().enumerate() {
        drop(call(
            &client,
            &mut rec,
            ws,
            Some(bound.route),
            ServiceClass::Lifecycle,
            "release",
            Command::ReleaseClosedCapture(bound.capture),
        ));
    }
    phase("idle");
    // Observation only: no SQL job or wake is used to obtain this progress.
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.diagnostics().unwrap().closed_namespaces < 4 {
        assert!(
            Instant::now() < deadline,
            "terminal namespace cleanup did not finish while idle"
        );
        std::thread::yield_now();
    }
    for (ws, bound) in workspaces.iter().enumerate() {
        let done = call(
            &client,
            &mut rec,
            ws,
            Some(bound.route),
            ServiceClass::Lifecycle,
            "verify_close",
            Command::CleanupState,
        );
        assert!(matches!(
            done.result(),
            Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone))
        ));
        println!("ENGINE_CLOSE workspace={ws} original={:?}", done.result());
    }
    assert!(client.maintenance_failure().unwrap().is_none());
    let work = client.diagnostics().unwrap();
    rec.reconcile(work);
    rec.finish(work);
    println!("ENGINE_FINITE jobs={} wave_counts={:?} peak_queued={} peak_credit={} maintenance_rows={} record_cleanup_bytes={} closed_namespaces={} physical_cleanup=gone",rec.jobs,rec.wave_classes,work.peak_queued,work.peak_credited_bytes,work.maintenance_rows,work.maintenance_data_bytes,work.closed_namespaces);
    phase("stop");
    owner.stop().unwrap();
    phase("finished");
}
