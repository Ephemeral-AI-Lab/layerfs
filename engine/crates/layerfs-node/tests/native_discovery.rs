//! External tests of the built Node binding. No private Rust source is included.
#![cfg(target_os = "linux")]
mod support;
use std::{path::Path, process::Command, time::Duration};
use support::{engine::*, *};

#[test]
fn a_gone_container_is_reclaimed_by_removing_its_records() {
    let home = Home::new();
    let engine = engine(
        &home.0,
        404,
        &format!(r#"{{"message":"No such container: {CID}"}}"#),
    );
    home.node(
        "assert.deepEqual(await reclaim(), {value:{kind:'ok',reclaimed:['1']}});",
        None,
        Duration::from_secs(5),
    );
    assert!(engine
        .join()
        .unwrap()
        .starts_with(&format!("GET /v1.54/containers/{CID}/json")));
    assert!(
        !home.0.join("proj/sandboxes/1").exists(),
        "its number is free again"
    );
}

#[test]
fn another_404_is_a_failure_that_removes_nothing() {
    let home = Home::new();
    let engine = engine(&home.0, 404, r#"{"message":"page not found"}"#);
    home.node(
        "assert.equal((await reclaim()).refused.code, 'Failed');",
        None,
        Duration::from_secs(5),
    );
    engine.join().unwrap();
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

#[test]
fn a_number_started_again_with_another_container_is_left_alone() {
    let home = Home::new();
    let record = home.0.join("proj/sandboxes/1/sandbox.record");
    let engine = engines_before(
        &home.0,
        vec![(404, format!(r#"{{"message":"No such container: {CID}"}}"#))],
        move || {
            // The public call read its original record before inspecting it.
            let mut bytes = std::fs::read(&record).unwrap();
            let offset = bytes
                .windows(64)
                .position(|window| window == CID.as_bytes())
                .unwrap();
            bytes[offset..offset + 64].fill(b'c');
            std::fs::write(record, bytes).unwrap();
        },
    );
    home.node(
        "assert.deepEqual(await reclaim(), {value:{kind:'ok',reclaimed:[]}});",
        None,
        Duration::from_secs(5),
    );
    engine.join().unwrap();
    let record = std::fs::read(home.0.join("proj/sandboxes/1/sandbox.record")).unwrap();
    assert!(record.windows(64).any(|window| window == [b'c'; 64]));
}

#[test]
fn a_stopped_container_is_refused_as_gone_then_reclaimed_by_deleting_it() {
    let home = Home::new();
    let engine = engines(
        &home.0,
        vec![(200, exited()), (200, exited()), (204, String::new())],
    );
    home.node(
        r#"
        const first = await native.call('sandbox.list_workspaces', args);
        assert.equal(first.refused.code, 'NoSuchSandbox');
        assert.equal(first.refused.detail.containerGone, true);
        assert.equal(first.refused.detail.containerStopped, true);
        assert.equal(first.refused.detail.container, 'b'.repeat(64));
        assert.deepEqual(await reclaim(), {value:{kind:'ok',reclaimed:['1']}});
    "#,
        None,
        Duration::from_secs(5),
    );
    let requests = engine.join().unwrap();
    assert!(requests[0].starts_with(&format!("GET /v1.54/containers/{CID}/json")));
    assert!(requests[1].starts_with(&format!("GET /v1.54/containers/{CID}/json")));
    assert!(requests[2].starts_with(&format!(
        "DELETE /v1.54/containers/{CID}?force=false&v=false"
    )));
    assert!(
        !home.0.join("proj/sandboxes/1").exists(),
        "its number is free again"
    );
}

#[test]
fn a_stopped_container_the_engine_will_not_delete_keeps_its_records() {
    let home = Home::new();
    let engine = engines(
        &home.0,
        vec![
            (200, exited()),
            (
                409,
                r#"{"message":"cannot remove container: container is running"}"#.into(),
            ),
        ],
    );
    home.node(
        "assert.equal((await reclaim()).refused.code, 'Failed');",
        None,
        Duration::from_secs(5),
    );
    assert_eq!(engine.join().unwrap().len(), 2);
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

#[test]
fn a_dropped_guard_releases_the_lock_while_a_forked_child_holds_a_copy() {
    let home = Home::new();
    let preload = home.0.join("fork-lock.so");
    assert!(Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Werror"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fork_lock.c"))
        .args(["-ldl", "-o"])
        .arg(&preload)
        .status()
        .unwrap()
        .success());
    std::fs::remove_file(home.0.join("proj/sandboxes/1/sandbox.record")).unwrap();
    // sandbox.close takes the project guard, then refuses the absent Sandbox.
    let call =
        "assert.equal((await native.call('sandbox.close',args)).refused.code, 'NoSuchSandbox');";
    home.node(call, Some(&preload), Duration::from_secs(5));
    let child = std::fs::read_to_string(home.0.join("fork.pid")).unwrap();
    let lock = home.0.join("proj/ownership.lock");
    let copied = std::fs::read_dir(format!("/proc/{child}/fd"))
        .unwrap()
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .any(|path| path == lock);
    assert!(copied, "child {child} holds no copy of {}", lock.display());
    home.node(call, None, Duration::from_secs(5));
}
