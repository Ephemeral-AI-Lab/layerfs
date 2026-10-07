//! Multiple actual daemon processes share one installed Store, with no write retry.
#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod fixture;
#[path = "support/shared_process.rs"]
mod process;
#[path = "support/shared_role.rs"]
mod role;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};
use layerfs_daemon::{install::receive_install, install_types::StoreSettings};
use layerfs_history::{
    BranchId, CommitStagedOutcome, ForkRequest, ForkSource, HistoryName, WorkspaceId,
};
use layerfs_sdk::control::Control;
use std::{fs, path::PathBuf, thread};
fn identity(tag: u8) -> WorkspaceId {
    WorkspaceId::from_authority([tag; 32]).unwrap()
}
fn mount(client: &mut Control, tag: u8, branch: BranchId) -> WorkspaceToken {
    match client
        .call(Request::Mount {
            workspace: identity(tag),
            branch,
        })
        .unwrap()
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    }
}
fn commit(reply: Reply) -> layerfs_history::CommitRecord {
    match reply {
        Reply::Committed(CommitStagedOutcome::Committed(value)) => value,
        other => panic!("{other:?}"),
    }
}
fn close(client: &mut Control, token: WorkspaceToken) {
    assert_eq!(
        client.call(Request::Unmount(token)).unwrap(),
        Reply::Unmounted(token)
    );
}
#[test]
#[ignore = "explicit owned daemon subprocess role"]
fn daemon_process_role() {
    role::run();
}
#[test]
fn overlapping_saves_ordered_overwrites_and_killed_daemon_preserve_the_shared_store() {
    let base = if cfg!(target_os = "linux") {
        PathBuf::from(
            std::env::var_os("LAYERFS_SHARED_STORE_ROOT").expect("explicit named-volume placement"),
        )
    } else {
        std::env::temp_dir()
    };
    let shared = base.join(format!("layerfs-sharing-{}", std::process::id()));
    fs::create_dir(&shared).unwrap();
    let database = shared.join("store.sqlite");
    let source = support::Fixture::new("shared-processes", Some(database.to_str().unwrap()));
    let destination = database.clone();
    let (mut channel, installer) = support::pair(move |connection| {
        receive_install(connection, &destination, StoreSettings::default())
    });
    let installed = layerfs_sdk::install(&source.project, &mut channel).unwrap();
    drop(installer.join().unwrap());
    drop(channel);
    let manifest = shared.join("manifest.bin");
    fs::write(&manifest, installed.manifest.encode().unwrap()).unwrap();
    fs::remove_file(&source.project.store.path).unwrap();
    let main = source.project.branch.branch.id;
    let fork = BranchId::from_authority([57; 16]);
    let (mut a, mut ca) =
        process::Daemon::start("A", &manifest, &source.directory.join("A-overlay.sqlite"));
    assert!(matches!(
        ca.call(Request::Fork(ForkRequest {
            stack: source.project.branch.branch.stack,
            branch: fork,
            name: HistoryName::new("parallel-b").unwrap(),
            source: ForkSource::Layer(source.project.branch.branch.base_layer)
        }))
        .unwrap(),
        Reply::Forked(_)
    ));
    let (mut b, mut cb) =
        process::Daemon::start("B", &manifest, &source.directory.join("B-overlay.sqlite"));
    let ta = mount(&mut ca, 11, main);
    let tb = mount(&mut cb, 12, fork);
    let first_a = thread::spawn(move || {
        let result = ca.call(Request::Commit(ta));
        (ca, result)
    });
    a.event("SHARING_SAVE_ENTER role=A attempt=1");
    let first_b = thread::spawn(move || {
        let result = cb.call(Request::Commit(tb));
        (cb, result)
    });
    b.event("SHARING_SAVE_ENTER role=B attempt=1");
    println!(
        "SHARING_OVERLAP both_Save_lifetimes_active=true initial_reservations_acknowledged=true"
    );
    a.release();
    let (mut ca, result) = first_a.join().unwrap();
    let first = commit(result.unwrap());
    a.event("SHARING_ROOT_ORACLE");
    b.release();
    let (mut cb, result) = first_b.join().unwrap();
    let separate = commit(result.unwrap());
    b.event("SHARING_ROOT_ORACLE");
    assert_ne!(first.root, separate.root);
    let race_b = mount(&mut cb, 13, main);
    let next_a = thread::spawn(move || {
        let result = ca.call(Request::Commit(ta));
        (ca, result)
    });
    a.event("SHARING_SAVE_ENTER role=A attempt=2");
    let next_b = thread::spawn(move || {
        let result = cb.call(Request::Commit(race_b));
        (cb, result)
    });
    b.event("SHARING_SAVE_ENTER role=B attempt=2");
    a.release();
    let (mut ca, result) = next_a.join().unwrap();
    let earlier = commit(result.unwrap());
    a.event("SHARING_ROOT_ORACLE");
    b.release();
    let (mut cb, result) = next_b.join().unwrap();
    let last = commit(result.unwrap());
    assert_eq!(earlier.parent, Some(first.id));
    assert_eq!(last.parent, Some(first.id));
    assert_ne!(earlier.root, last.root);
    b.event("SHARING_ROOT_ORACLE");
    let crashing = thread::spawn(move || ca.call(Request::Commit(ta)));
    a.event("SHARING_SAVE_ENTER role=A attempt=3");
    a.event("SHARING_PREFIX_ACK");
    a.kill();
    let unknown = crashing.join().unwrap().unwrap_err();
    assert!(unknown.attempted && unknown.received.is_none());
    println!("SHARING_ORIGINAL_CRASH_CLIENT {unknown:?}");
    close(&mut cb, race_b);
    close(&mut cb, tb);
    let end = mount(&mut cb, 99, main);
    b.event("SHARING_ROOT_ORACLE");
    close(&mut cb, end);
    b.finish();
    drop(cb);
    // All prior daemon Store handles are now gone. This is a fresh opener.
    let (mut fresh, mut client) =
        process::Daemon::start("C", &manifest, &source.directory.join("C-overlay.sqlite"));
    let check_main = mount(&mut client, 21, main);
    fresh.event("SHARING_ROOT_ORACLE");
    let check_fork = mount(&mut client, 22, fork);
    fresh.event("SHARING_ROOT_ORACLE");
    close(&mut client, check_main);
    close(&mut client, check_fork);
    let end = mount(&mut client, 99, main);
    fresh.event("SHARING_ROOT_ORACLE");
    close(&mut client, end);
    fresh.finish();
    println!("SHARING_RESULT separate_commits=2 same_branch_publications=2 last_head={} captured_parents_preserved=true killed_daemon=true fresh_open_full_oracles=true original_client_unknown_preserved=true store={}",last.id,database.display());
    drop(unknown);
    source.cleanup();
    fs::remove_dir_all(shared).unwrap();
}
