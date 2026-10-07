//! Frozen resource-growth selections over original product operations.
#[path = "resource_growth/allocation.rs"]
mod allocation;
#[allow(dead_code)]
#[path = "../tests/support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "../tests/support/complete_fixture.rs"]
mod fixture;
#[path = "resource_growth/observe.rs"]
mod observe;
#[path = "resource_growth/operations.rs"]
mod operations;
#[allow(dead_code)]
#[path = "store_accounting/produce.rs"]
mod produce;
#[allow(dead_code)]
#[path = "../tests/support/native_install.rs"]
mod support;
#[global_allocator]
static ALLOCATOR: allocation::Observed = allocation::Observed;
use layerfs_daemon::{store::BindRequest, Command, Owner, OwnerConfig, Response};
use layerfs_history::{BranchId, WorkspaceId};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        7,
        "resource_growth FRESH_DIRECTORY FRESH_RECEIPTS PREPARED MODE N AMOUNT"
    );
    let mode = args[4].as_str();
    let names: usize = args[5].parse().unwrap();
    let amount: u64 = args[6].parse().unwrap();
    assert!(matches!(
        (mode, names, amount),
        ("commit", 1000 | 10000 | 100000, 1)
            | ("repeat", 100000, 64)
            | ("save", 1000, 4194304 | 33554432 | 268435456)
    ));
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(90));
        std::process::exit(124);
    });
    fs::create_dir(&args[1]).unwrap();
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])
        .unwrap();
    writeln!(out,"{{\"schema\":\"pre-s8-growth-start-v1\",\"mode\":\"{mode}\",\"files\":{names},\"amount\":{amount},\"profile\":\"sqlite-wal-off-v2\",\"allocation_scope\":\"all-process Rust requested live bytes; excludes C, slack, stack and kernel\"}}").unwrap();
    observe::phase("clone");
    let f = fixture::Fixture::prepared_wide(Path::new(&args[3]));
    println!("STORE_ARTIFACT_DIRECTORY {}", f.directory.display());
    observe::phase("startup");
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    observe::phase("bind");
    let bound = f
        .opened
        .store
        .bind(
            owner.client(),
            BindRequest {
                branch: BranchId::from_slice(&f.manifest.branch).unwrap(),
                workspace: WorkspaceId::from_authority([47; 32]).unwrap(),
            },
        )
        .unwrap();
    drop(bound.open);
    let bound = bound.workspace;
    operations::verify_name(&bound, "entry-000000", &fixture::body(0));
    operations::verify_name(
        &bound,
        &format!("entry-{:06}", names - 1),
        &fixture::body(names - 1),
    );
    observe::point(&mut out, "bound", 0, &f.opened, &owner);
    if mode == "save" {
        observe::phase("save");
        operations::save(&mut out, &f.opened, &bound, amount);
        observe::point(&mut out, "released", 1, &f.opened, &owner);
    } else {
        for index in 0..amount as usize {
            observe::phase(&format!("cycle-{index:03}"));
            operations::commit(&mut out, &f.opened, &owner, &bound, index);
            observe::phase(&format!("released-{index:03}"));
            observe::point(&mut out, "released", index + 1, &f.opened, &owner);
        }
    }
    observe::phase("close");
    assert!(matches!(
        owner
            .client()
            .try_submit(Some(bound.route()), Command::Close)
            .unwrap()
            .wait()
            .unwrap()
            .result(),
        Ok(Response::Done)
    ));
    drop(bound);
    observe::phase("drain");
    let until = Instant::now() + Duration::from_secs(2);
    while owner.client().diagnostics().unwrap().closed_namespaces < 1 {
        assert!(Instant::now() < until, "terminal cleanup deadline");
        std::thread::yield_now();
    }
    assert!(owner.client().maintenance_failure().unwrap().is_none());
    observe::point(&mut out, "drained", amount as usize, &f.opened, &owner);
    observe::phase("stop");
    owner.stop().unwrap();
    drop(f.opened);
    observe::terminal(&mut out);
    println!(
        "GROWTH_COMPLETE mode={mode} files={names} amount={amount} retained={}",
        f.directory.display()
    );
    observe::phase("finished");
}
