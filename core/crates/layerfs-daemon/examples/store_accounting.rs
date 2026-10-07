//! Whole Store-half Commit over one writable clone of the retained wide preparation.
#[allow(dead_code)]
#[path = "../tests/support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "e2_startup/records.rs"]
mod engine_records;
#[allow(dead_code)]
#[path = "../tests/support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "e2_startup/json.rs"]
mod json;
#[path = "store_accounting/produce.rs"]
mod produce;
#[path = "store_accounting/records.rs"]
mod records;
#[allow(dead_code)]
#[path = "../tests/support/native_install.rs"]
mod support;
use layerfs_content::filesystem::{FilesystemRootId, PathName};
use layerfs_daemon::{store::BindRequest, Command, Owner, OwnerConfig, Response};
use layerfs_history::{CommitStagedOutcome, WorkspaceId};
use std::{
    fs::OpenOptions,
    io::{BufRead, Write},
    path::Path,
    time::{Duration, Instant},
};
fn phase(name: &str) {
    println!("ENGINE_PHASE {name} pid={}", std::process::id());
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    assert!(std::io::stdin().lock().read_line(&mut line).unwrap() > 0);
    assert_eq!(line, "continue\n");
}
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        4,
        "store_accounting FRESH_DIRECTORY FRESH_RECEIPTS PREPARED_DIRECTORY"
    );
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(90));
        std::process::exit(124);
    });
    // The clone fixture selects a native temporary directory; the observer's
    // declared artifact directory is an explicit alias recorded before work.
    let requested = Path::new(&args[1]);
    std::fs::create_dir(requested).unwrap();
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])
        .unwrap();
    phase("clone");
    let f = fixture::Fixture::prepared_wide(Path::new(&args[3]));
    println!("STORE_ARTIFACT_DIRECTORY {}", f.directory.display());
    phase("startup");
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let store = &f.opened.store;
    let branch = layerfs_history::BranchId::from_slice(&f.manifest.branch).unwrap();
    phase("bind");
    let bound = store
        .bind(
            owner.client(),
            BindRequest {
                branch,
                workspace: WorkspaceId::from_authority([47; 32]).unwrap(),
            },
        )
        .unwrap();
    drop(bound.open);
    let bound = bound.workspace;
    phase("write");
    produce::mutate(&bound);
    for (kind, changed) in [("Committed", true), ("UpToDate", false)] {
        phase(if changed { "commit" } else { "up_to_date" });
        let before = f.opened.diagnostics().unwrap();
        let local = owner.client().diagnostics().unwrap();
        let result = bound
            .commit(|save, _, snapshot| {
                if changed {
                    produce::construct(save, snapshot, store.policy().construction())
                } else {
                    Ok(FilesystemRootId(snapshot.effective_root))
                }
            })
            .unwrap();
        assert!(matches!(
            (&result.history, changed),
            (CommitStagedOutcome::Committed(_), true)
                | (CommitStagedOutcome::UpToDate { .. }, false)
        ));
        let after = f.opened.diagnostics().unwrap();
        let local_after = owner.client().diagnostics().unwrap();
        let mut exact = result.captured.work().sql.expanded();
        exact.accumulate(result.installed.work().sql.expanded());
        let whole = local_after.sql_foreground.since(&local.sql_foreground);
        let writes = after.writer.write_commits - before.writer.write_commits;
        let scans = after.writer.fullscan_steps - before.writer.fullscan_steps;
        records::commit(
            &mut output,
            kind,
            &result,
            before,
            after,
            whole,
            (local, local_after),
        );
        assert_eq!(whole, exact);
        assert_eq!(local_after.admitted - local.admitted, 2);
        assert_eq!(writes, result.storage.reserve + result.storage.publish + 1);
        assert_eq!(result.storage.initial_reservations, 1);
        assert_eq!(result.storage.reservation_refills, 0);
        // The prepared Store has a saturated, schema-bounded signature ring.
        // Its one load scans8192 slots; no namespace-dependent scan is allowed.
        // Paired EXPLAIN/runtime cause: F14 receipts41/42.
        assert_eq!(result.storage.signatures, u64::from(changed));
        assert_eq!(scans, result.storage.signatures * 8191);
        println!("STORE_WHOLE_COMMIT outcome={kind} owner_jobs=2 writes={} initial={} refills={} publication={} history=1 namespace_files=100000",result.storage.reserve+result.storage.publish+1,result.storage.initial_reservations,result.storage.reservation_refills,result.storage.publish);
        drop(result);
        assert_eq!(owner.client().diagnostics().unwrap().outstanding, 0);
    }
    phase("verify");
    let op = bound.operation().unwrap();
    let base = op.workspace().base().unwrap();
    for (name, expected) in [
        ("entry-000000", produce::changed_bytes()),
        ("entry-099999", fixture::body(99999)),
    ] {
        let file = base
            .child(
                base.root().root_inode().serial(),
                &PathName::new(name).unwrap(),
            )
            .unwrap();
        let mut actual = Vec::new();
        base.plan_read(file.serial, 0, 128)
            .unwrap()
            .emit(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
    }
    drop((base, op));
    phase("release");
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
    phase("idle");
    let until = Instant::now() + Duration::from_secs(2);
    while owner.client().diagnostics().unwrap().closed_namespaces < 1 {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(owner.client().maintenance_failure().unwrap().is_none());
    phase("stop");
    owner.stop().unwrap();
    drop(f.opened);
    println!(
        "STORE_RETAINED directory={} physical_cleanup=gone original_results=released",
        f.directory.display()
    );
    phase("finished");
}
