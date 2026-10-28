//! Real known SQLite capacity exhaustion through ordinary mounted writes.
//! A transparent observer records actual kernel owner outcomes; it injects no
//! fault, changes no input and retains no response or owner credit.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace},
    Owner, OwnerClient, OwnerConfig,
};
use layerfs_fuse::{
    attributes::Identity,
    mount::AbortWrite,
    ports::{Fence, MountServices, RequestServices, ServiceError, ServiceFuture, ServiceReply},
    session::{Detach, KernelSession, SessionConfig},
    MountRequests,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{KernelCookieOffer, KernelMount, OverlayError, ProfileConfig, Publication};
use layerfs_storage::ReservationBlocks;
use layerfs_workspace::{
    BaseView, KernelDirectoryWindow, KernelMutationOutcome, KernelReadOperation, KernelReadOutcome,
    KernelVisitRequest, KernelWindow, VisitFacts, WorkspaceError, WriteData,
};
use std::{
    fs,
    os::unix::process::CommandExt,
    process::{Command as Process, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const COMMAND: u32 = 65534;
struct ObserveMount {
    bound: Arc<BoundWorkspace>,
    full: Arc<AtomicU64>,
    io_full: bool,
}
struct ObserveRequest {
    inner: Arc<dyn RequestServices>,
    full: Arc<AtomicU64>,
    io_full: bool,
}
impl MountServices for ObserveMount {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Ok(Arc::new(ObserveRequest {
            inner: layerfs_daemon::mount_request(&self.bound, fence)?,
            full: self.full.clone(),
            io_full: self.io_full,
        }))
    }
}
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
        fn $name(&self, $($arg:$ty),*) -> ServiceFuture<'_, $out> {
            self.inner.$name($($arg),*)
        }
    )*};
}
impl RequestServices for ObserveRequest {
    forward! {
        directory_visit(mount:KernelMount,serial:u64,handle:u64,offset:u64,after:Option<Vec<u8>>) -> ServiceReply<Arc<KernelDirectoryWindow>>;
        publish_cookies(offer:KernelCookieOffer,names:Vec<Vec<u8>>) -> ServiceReply<()>;
        close_directory(mount:KernelMount,serial:u64,handle:u64) -> ServiceReply<()>;
        read_visit(mount:KernelMount,serial:u64,handle:Option<u64>,offset:u64,length:u32) -> ServiceReply<Arc<KernelWindow>>;
        close_file(mount:KernelMount,serial:u64,handle:u64) -> ServiceReply<()>;
        observe_visit(mount:KernelMount,request:u64,serial:u64,handle:Option<u64>,operation:KernelReadOperation,facts:Arc<VisitFacts>) -> ServiceReply<Arc<KernelReadOutcome>>;
        base() -> BaseView;
        recognize(serial:u64,offset:u64,data:WriteData) -> WriteData;
        forget(mount:KernelMount,serial:u64,count:u64) -> ServiceReply<()>;
        reply_attempted(publication:Publication) -> ServiceReply<()>;
    }
    fn provider_failure(&self) -> Result<Option<ServiceError>, ServiceError> {
        self.inner.provider_failure()
    }
    fn failed_base_read(&self, step: ServiceError) -> ServiceError {
        self.inner.failed_base_read(step)
    }
    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError> {
        self.inner.reserve_serial()
    }
    fn mutate_visit(
        &self,
        request: KernelVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelMutationOutcome>>> {
        Box::pin(async move {
            let reply = self.inner.mutate_visit(request).await?;
            if let Err(WorkspaceError::Overlay(error)) = &reply.get().result {
                // Count only the exact actual owning result selected by this
                // proof: direct Full, or a known rollback retaining IOERR_WRITE.
                let original = match error {
                    OverlayError::Sql(error) if !self.io_full => Some((error, 13)),
                    OverlayError::NoSpace { cause } if self.io_full => match cause.as_ref() {
                        OverlayError::Sql(error) => Some((error, 778)),
                        _ => None,
                    },
                    _ => None,
                };
                if original.is_some_and(|(error, expected)| {
                    error
                        .sqlite_error()
                        .is_some_and(|code| code.extended_code == expected)
                }) {
                    self.full.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(reply)
        })
    }
}
fn job<T>(
    client: &OwnerClient,
    route: layerfs_workspace::Route,
    action: impl FnOnce(&OwnerClient, layerfs_workspace::Route) -> Result<T, layerfs_daemon::OwnerError>,
) -> T {
    action(client, route).unwrap()
}
fn state(client: &OwnerClient, route: layerfs_workspace::Route) -> layerfs_overlay::WorkspaceState {
    job(
        client,
        route,
        move |owner: &layerfs_daemon::OwnerClient, route| owner.state(route),
    )
}
fn resources(client: &OwnerClient, route: layerfs_workspace::Route) -> layerfs_overlay::Resources {
    job(
        client,
        route,
        move |owner: &layerfs_daemon::OwnerClient, route| {
            owner.workspace_client(route)?.resources()
        },
    )
}
fn run(forced: bool, device: bool) {
    fusectl::mount();
    let fixture = installed::Fixture::built(
        if forced {
            "sqlite-full-forced"
        } else {
            "sqlite-full-normal"
        },
        |source| {
            fs::write(source.join("prior.dat"), vec![17; 4096]).unwrap();
            fs::write(source.join("fail.dat"), []).unwrap();
            2
        },
    );
    let store = open_store(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        2,
        0,
        ReservationBlocks::default(),
    )
    .unwrap();
    let before_history = store
        .history()
        .branch_snapshot(fixture.branch)
        .unwrap()
        .unwrap();
    let device_root = device.then(|| {
        std::path::PathBuf::from(
            std::env::var_os("LAYERFS_PROOF_DEVICE_ROOT").expect("owned device fixture"),
        )
    });
    let io_full = device && std::env::var_os("LAYERFS_PROOF_TRANSLATE_ENOSPC").is_some();
    let overlay = device_root
        .as_ref()
        .unwrap_or(&fixture.directory)
        .join("overlay.sqlite");
    let quota = if device {
        None
    } else {
        let path = fixture.directory.join("quota-schema.sqlite");
        let probe = layerfs_overlay::Overlay::create(&path, ProfileConfig::default()).unwrap();
        let pages = probe.pages().unwrap().0;
        probe.close_database().unwrap();
        fs::remove_file(path).unwrap();
        // Enough room for setup, less than the 32 payload pages the request
        // needs. Derive from schema size so table removal preserves the fault.
        Some(u32::try_from(pages + 8).unwrap())
    };
    let owner = Owner::start(
        &overlay,
        ProfileConfig {
            pager_kib: 4,
            max_pages: quota,
        },
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let bound = store
        .bind(
            client.clone(),
            BindRequest {
                branch: fixture.branch,
                workspace: WorkspaceId::from_authority([if forced { 219 } else { 218 }; 32])
                    .unwrap(),
            },
        )
        .unwrap()
        .workspace;
    let bound = Arc::new(bound);
    let route = bound.route();
    let root = bound
        .operation()
        .unwrap()
        .workspace()
        .base()
        .unwrap()
        .root()
        .root_inode()
        .serial();
    let done = job(
        &client,
        route,
        move |owner: &layerfs_daemon::OwnerClient, route| owner.kernel_mount(route, root),
    );
    let mount = done;
    let queue = MountRequests::new(mount);
    let full = Arc::new(AtomicU64::new(0));
    let mut session = KernelSession::attach(
        SessionConfig {
            directory: fixture.directory.join("mounted"),
            owner_uid: 0,
            owner_gid: 0,
            ready_wait: Duration::from_secs(3),
            drain_wait: Duration::from_secs(3),
            control: layerfs_fuse::mount::CONTROL.into(),
        },
        queue.clone(),
        Arc::new(ObserveMount {
            bound: bound.clone(),
            full: full.clone(),
            io_full,
        }),
        Identity {
            root,
            uid: COMMAND,
            gid: COMMAND,
        },
    )
    .unwrap();
    let _filler = device_root.as_ref().map(|root| {
        let filler = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("kernel-fill"))
            .unwrap();
        let filesystem = nix::sys::statvfs::statvfs(root.as_path()).unwrap();
        let bytes = filesystem.blocks_free() * filesystem.fragment_size();
        let fill = nix::fcntl::fallocate(
            &filler,
            nix::fcntl::FallocateFlags::empty(),
            0,
            i64::try_from(bytes).unwrap(),
        );
        if let Err(error) = fill {
            assert_eq!(error, nix::errno::Errno::ENOSPC);
        }
        let full = nix::sys::statvfs::statvfs(root.as_path()).unwrap();
        assert_eq!(full.blocks_available(), 0, "actual owned device exhaustion");
        println!(
            "SQLITE_FULL_DEVICE fill={fill:?} free_blocks={} available_blocks={} io_full={io_full}",
            full.blocks_free(),
            full.blocks_available()
        );
        filler
    });
    let initial = state(&client, route);
    let initial_resources = resources(&client, route);
    let before = client.diagnostics().unwrap();
    let script = r#"
import os
assert open('prior.dat','rb').read() == bytes([17])*4096
fd = os.open('fail.dat',os.O_WRONLY)
try:
    try:
        n = os.write(fd,bytes([73])*131072)
        print('acknowledged_bytes='+str(n))
        print('observed_errno=0')
    except OSError as error:
        print('acknowledged_bytes=0')
        print('observed_errno='+str(error.errno))
finally:
    os.close(fd)
assert os.stat('fail.dat').st_size == 0
assert open('prior.dat','rb').read() == bytes([17])*4096
"#;
    let mut child = Process::new("python3")
        .arg("-c")
        .arg(script)
        .current_dir(session.directory())
        .uid(COMMAND)
        .gid(COMMAND)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    let mut timed_out = false;
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= until {
            timed_out = true;
            let _ = session.abort();
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    println!(
        "SQLITE_FULL_KERNEL forced={forced} output={} stderr={}",
        text.trim(),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = client.diagnostics().unwrap();
    let final_state = state(&client, route);
    let final_resources = resources(&client, route);
    let known_full = full.load(Ordering::Relaxed);
    let reservation_refusals =
        after.allocation_foreground.refusals - before.allocation_foreground.refusals;
    let work = after.sql_foreground.since(&before.sql_foreground).total();
    let history = store
        .history()
        .branch_snapshot(fixture.branch)
        .unwrap()
        .unwrap();
    let remaining = queue.work().unwrap();
    println!("SQLITE_FULL_KERNEL device={device} io_full={io_full} known_full={known_full} reservation_refusals={reservation_refusals} sql={work:?} initial={initial:?} final={final_state:?} requests={remaining:?}");
    // On the old implementation the real failed completion is retained.
    // Inspect it without taking, releasing, or replacing its owner.
    for slot in 0..remaining.retained {
        if let Ok(detail) = queue.inspect_failure(slot, |failure| match failure {
            layerfs_fuse::FailureView::Request(error) => format!("{error:?}"),
            layerfs_fuse::FailureView::Panic(_) => "panic".to_owned(),
        }) {
            println!("SQLITE_FULL_KERNEL retained={detail}");
        }
    }
    let drained = if remaining.retained != 0 || forced || timed_out {
        if !timed_out {
            assert_eq!(session.abort().unwrap(), AbortWrite::Written);
        }
        session.force_drain()
    } else {
        assert_eq!(session.detach().unwrap(), Detach::Detached);
        session.drain()
    };
    // A red keeps its exact stopped owner until this proof-only container
    // exits. It is never called a clean drain or released through Drop.
    let clean = match drained {
        Ok(drained) => {
            println!("SQLITE_FULL_KERNEL drained={drained:?}");
            true
        }
        Err(kept) => {
            println!("SQLITE_FULL_KERNEL retained_drain={kept:?}");
            false
        }
    };
    if clean {
        job(&client, route, move |owner, route| {
            owner.kernel_revoke(route, mount)
        });
        bound.close().unwrap();
        drop(queue);
        drop(bound);
        drop(client);
        owner.stop().unwrap();
        drop(store);
        fixture.cleanup();
    }
    assert!(!timed_out, "ordinary process completed within bounded wait");
    assert_eq!(
        known_full, 1,
        "actual known SQLiteFull owning result exactly once"
    );
    assert_eq!(
        reservation_refusals, 0,
        "SQL exhaustion is not pre-BEGIN physical admission"
    );
    assert!(
        work.rows_changed > 0,
        "real partial atomic DML reached before failure"
    );
    assert_eq!(history, before_history, "publication authority unchanged");
    assert_eq!(
        final_state, initial,
        "known rollback preserves Workspace state"
    );
    assert_eq!(
        final_resources.counts.payload_bytes, initial_resources.counts.payload_bytes,
        "known rollback preserves stored payload bytes"
    );
    assert_eq!(
        final_resources.counts.payload_cells, initial_resources.counts.payload_cells,
        "known rollback preserves stored payload cells"
    );
    assert!(text.contains("acknowledged_bytes=0"));
    assert!(
        text.contains("observed_errno=28"),
        "known rollback must answer ENOSPC: {text}"
    );
    assert!(output.status.success(), "ordinary syscall assertions");
    assert_eq!(
        remaining.retained, 0,
        "known no-effect request released after reply"
    );
    assert!(
        clean,
        "kernel drain completed without retained request custody"
    );
}
#[test]
fn known_sqlite_full_is_enospc_and_normal_drain_completes() {
    run(false, false);
}
#[test]
fn known_sqlite_full_is_enospc_and_forced_drain_completes() {
    run(true, false);
}

#[test]
#[ignore = "requires exclusively owned ext4 device; device_fixture.sh mounted"]
fn real_device_full_reports_enospc_and_normal_drain_completes() {
    run(false, true);
}
#[test]
#[ignore = "requires exclusively owned ext4 device; device_fixture.sh mounted"]
fn real_device_full_reports_enospc_and_forced_drain_completes() {
    run(true, true);
}
