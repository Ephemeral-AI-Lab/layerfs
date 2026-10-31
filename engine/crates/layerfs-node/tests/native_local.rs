//! External tests of the built Node binding's native Sandbox records
//! (LAYERFS_SANDBOX=native). No private Rust source is included: every case
//! writes a Sandbox record as a previous call would have, then calls the
//! addon's public `runtime.sandbox.reclaim` and observes the reply and the
//! files it left. Neither Docker, FUSE, nor a Store is needed.
#![cfg(target_os = "linux")]
mod support;
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use support::*;

/// This boot's id as a record keeps it: 32 lowercase hex digits.
fn boot() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .unwrap()
        .trim()
        .replace('-', "")
}

/// Sandbox 1 of `proj` recorded as a `runtime` (native, or the former local) Sandbox
/// in `directory` named `name`, its daemon long gone (no process has id
/// u32::MAX). `edit` changes the fields before the record is written.
fn record(
    home: &Home,
    runtime: &str,
    directory: &str,
    name: &str,
    edit: impl FnOnce(&mut Vec<(String, SdkValue)>),
) {
    let bytes = |n: u8| SdkValue::Bytes(vec![n; 32]);
    let mut fields: Vec<(String, SdkValue)> = vec![
        ("id".into(), SdkValue::text("1")),
        ("project".into(), SdkValue::text("proj")),
        ("name".into(), SdkValue::text(name)),
        ("container".into(), SdkValue::text(CID)),
        ("runtime".into(), SdkValue::text(runtime)),
        ("directory".into(), SdkValue::text(directory)),
        ("pid".into(), SdkValue::Int(i64::from(u32::MAX))),
        ("start".into(), SdkValue::text("1")),
        ("boot".into(), SdkValue::text(boot())),
        ("port".into(), SdkValue::Int(40404)),
        ("image".into(), SdkValue::text("local")),
        ("secret".into(), bytes(1)),
        ("peer".into(), bytes(2)),
        ("instance".into(), bytes(3)),
    ];
    edit(&mut fields);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(home.0.join("proj/sandboxes/1/sandbox.record"))
        .unwrap();
    file.write_all(
        &Answer {
            id: 1,
            reply: Reply::Sdk(SdkValue::Map(fields)),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
}

/// Replaces (or with `None` removes) one field of a record.
fn set(key: &'static str, value: Option<SdkValue>) -> impl FnOnce(&mut Vec<(String, SdkValue)>) {
    move |fields| {
        fields.retain(|(k, _)| k != key);
        if let Some(value) = value {
            fields.push((key.into(), value));
        }
    }
}

/// `LAYERFS_LOCAL_DIR` of these tests, created like a deployment's.
fn root(home: &Home) -> PathBuf {
    let root = home.0.join("local");
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// Where a `root` deployment puts Sandbox 1 of `proj`, created with a file.
fn own(root: &Path) -> PathBuf {
    let own = root.join("layerfs-proj-1");
    std::fs::create_dir_all(own.join("tmp")).unwrap();
    std::fs::write(own.join("daemon.log"), b"log").unwrap();
    own
}

/// The public reclaim of a `sandbox` (native or docker) deployment, with
/// `localDir` when given, followed by `then` (JavaScript, `r` is the reply).
fn reclaim(home: &Home, sandbox: &str, local_dir: Option<&str>, then: &str) {
    let deploy = match local_dir {
        Some(dir) => format!("{{sandbox:{sandbox:?}, localDir:{dir:?}}}"),
        None => format!("{{sandbox:{sandbox:?}}}"),
    };
    home.node(
        &format!(
            "const r = await native.backendCall('runtime.sandbox.reclaim', {{...args, deploy:{deploy}}});\n{then}"
        ),
        None,
        Duration::from_secs(10),
    );
}

const NEITHER: &str = "assert.equal(r.refused?.code, 'InvalidInput', JSON.stringify(r));
    assert.match(r.refused.message, /neither used nor deleted/);";
const RECLAIMED: &str = "assert.deepEqual(r, {value:{kind:'ok',reclaimed:['1']}});";

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn a_native_record_naming_another_directory_is_neither_used_nor_deleted() {
    let home = Home::new();
    let root = root(&home);
    // a data directory a damaged or edited record names by its own name
    let data = home.0.join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("keep"), b"x").unwrap();
    let old = home.0.join("old/layerfs-proj-1");
    std::fs::create_dir_all(&old).unwrap();
    let r = text(&root);
    for (directory, name) in [
        (text(&data).to_owned(), "data"),
        (format!("{r}/data"), "data"),
        // another LAYERFS_LOCAL_DIR, another Sandbox, another name
        (text(&old).to_owned(), "layerfs-proj-1"),
        (format!("{r}/layerfs-proj-2"), "layerfs-proj-2"),
        (format!("{r}/layerfs-proj-1"), "layerfs-proj-2"),
        (format!("{r}/layerfs-proj-1/sub"), "sub"),
        ("relative/layerfs-proj-1".to_owned(), "layerfs-proj-1"),
        (format!("{r}/../etc/layerfs-proj-1"), "layerfs-proj-1"),
        ("/".to_owned(), ""),
    ] {
        record(&home, "native", &directory, name, |_| {});
        reclaim(&home, "native", Some(r), NEITHER);
        assert!(
            home.0.join("proj/sandboxes/1/sandbox.record").exists(),
            "the record of {directory} was removed"
        );
    }
    assert!(
        data.join("keep").exists(),
        "a delete reached {}",
        data.display()
    );
    assert!(old.exists(), "a delete reached {}", old.display());
}

#[test]
fn a_native_record_is_used_only_with_its_deployment_s_absolute_local_dir() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    let refused = "assert.equal(r.refused?.code, 'InvalidInput', JSON.stringify(r));";
    reclaim(&home, "native", None, refused);
    reclaim(&home, "native", Some("relative"), refused);
    reclaim(
        &home,
        "native",
        Some(&format!("{}/../local", text(&root))),
        refused,
    );
    assert!(own.join("daemon.log").exists());
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

#[test]
fn a_stopped_native_sandbox_where_its_deployment_puts_it_is_deleted_and_reclaimed() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    reclaim(&home, "native", Some(text(&root)), RECLAIMED);
    assert!(!own.exists(), "the stopped Sandbox's directory is deleted");
    assert!(
        !home.0.join("proj/sandboxes/1").exists(),
        "its number is free again"
    );
    assert!(root.exists(), "LAYERFS_LOCAL_DIR itself stays");
}

#[test]
fn a_record_from_before_boot_ids_is_read_without_one() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(
        &home,
        "native",
        text(&own),
        "layerfs-proj-1",
        set("boot", None),
    );
    reclaim(&home, "native", Some(text(&root)), RECLAIMED);
    assert!(!home.0.join("proj/sandboxes/1").exists());
}

#[test]
fn a_damaged_boot_or_port_is_refused_and_removes_nothing() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    for (key, value, code) in [
        ("boot", SdkValue::text("not-hex"), "InvalidInput"),
        ("port", SdkValue::Int(70000), "Failed"),
    ] {
        record(
            &home,
            "native",
            text(&own),
            "layerfs-proj-1",
            set(key, Some(value)),
        );
        reclaim(
            &home,
            "native",
            Some(text(&root)),
            &format!("assert.equal(r.refused?.code, {code:?}, JSON.stringify(r));"),
        );
        assert!(own.join("daemon.log").exists(), "{key}");
        assert!(
            home.0.join("proj/sandboxes/1/sandbox.record").exists(),
            "{key}"
        );
    }
}

#[test]
fn a_docker_record_stays_docker_under_a_native_deployment() {
    // the default record of Home is a Docker one: with no Engine at its
    // socket it is neither reclaimed nor read as a native Sandbox
    let home = Home::new();
    let root = root(&home);
    reclaim(
        &home,
        "native",
        Some(text(&root)),
        "assert.equal(r.refused?.code, 'Failed', JSON.stringify(r));",
    );
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

/// `sandbox.close` of Sandbox 1 under a `deploy` (JavaScript object text):
/// it reopens the record, so another backend's is refused (#527 step 4).
fn close_refused_as_mixed(home: &Home, deploy: &str) {
    home.node(
        &format!(
            "const r = await native.call('sandbox.close', {{...args, deploy:{deploy}}});
            {NEITHER}
            assert.match(r.refused.message, /never mixed/);"
        ),
        None,
        Duration::from_secs(10),
    );
}

/// Reclaim under `deploy` leaves another backend's record out: nothing is
/// reclaimed, nothing is refused (#527 step 12).
const LEFT_OUT: &str = "assert.deepEqual(r, {value:{kind:'ok',reclaimed:[]}});";

#[test]
fn a_record_of_the_former_local_runtime_is_never_used_nor_deleted() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    let native = format!("{{sandbox:'native', localDir:{:?}}}", text(&root));
    // a record of the former `local` runtime is used by no deployment: a
    // reclaim leaves it out, a reopen refuses it
    record(&home, "local", text(&own), "layerfs-proj-1", |_| {});
    reclaim(&home, "native", Some(text(&root)), LEFT_OUT);
    reclaim(&home, "docker", Some(text(&root)), LEFT_OUT);
    close_refused_as_mixed(&home, &native);
    // nor is a native record by any deployment but a native one
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    reclaim(&home, "docker", Some(text(&root)), LEFT_OUT);
    close_refused_as_mixed(
        &home,
        &format!("{{sandbox:'docker', localDir:{:?}}}", text(&root)),
    );
    assert!(own.join("daemon.log").exists());
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
    // a native deployment uses its own record: stopped, it is reclaimed
    reclaim(&home, "native", Some(text(&root)), RECLAIMED);
    assert!(!own.exists());
    assert!(!home.0.join("proj/sandboxes/1").exists());
}

#[test]
fn a_native_record_is_not_reopened_by_a_namespaces_deployment() {
    // #527 step 4: reopen requires the deployment's own backend both ways; a
    // namespaces deployment does not take a native record as its own, and
    // since step 12 its reclaim leaves the record out instead of failing
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    reclaim(&home, "namespaces", Some(text(&root)), LEFT_OUT);
    close_refused_as_mixed(
        &home,
        &format!("{{sandbox:'namespaces', localDir:{:?}}}", text(&root)),
    );
    assert!(own.join("daemon.log").exists());
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
    // the same record under a native deployment is its own: reclaimed
    reclaim(&home, "native", Some(text(&root)), RECLAIMED);
    assert!(!own.exists());
}

#[test]
fn every_backend_s_record_is_listed_with_its_runtime_and_state() {
    // #527 step 12: runtime.sandbox.records lists a record of any backend
    // without matching the deployment's; list_sandboxes and reclaim of a
    // Docker deployment leave a native record out instead of failing
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    let records =
        "await native.backendCall('runtime.sandbox.records', {...args, deploy:{sandbox:'docker'}})";
    let listed =
        "await native.call('project.list_sandboxes', {...args, deploy:{sandbox:'docker'}})";
    // a native daemon long gone: stopped
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    home.node(
        &format!(
            "assert.deepEqual({records}, {{value:{{kind:'ok', items:[{{sandbox:'1', name:'layerfs-proj-1', runtime:'native', image:'local', workspaces:0, state:'stopped'}}]}}}});
            assert.deepEqual({listed}, {{value:{{kind:'ok', items:[]}}}});"
        ),
        None,
        Duration::from_secs(10),
    );
    reclaim(&home, "docker", None, LEFT_OUT);
    // a namespaces record whose process runs (this test's own): running
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let start = stat[stat.rfind(')').unwrap() + 2..]
        .split(' ')
        .nth(19)
        .unwrap()
        .to_owned();
    let digest = format!("sha256:{}", "a".repeat(64));
    record(
        &home,
        "namespaces",
        text(&own),
        "layerfs-proj-1",
        |fields| {
            set("pid", Some(SdkValue::Int(i64::from(std::process::id()))))(fields);
            set("start", Some(SdkValue::text(&start)))(fields);
            set("image", Some(SdkValue::text(&digest)))(fields);
        },
    );
    home.node(
        &format!(
            "const r = {records};
            assert.deepEqual(r.value.items.map((s) => [s.runtime, s.state, s.image]), [['namespaces', 'running', {digest:?}]]);
            assert.deepEqual({listed}, {{value:{{kind:'ok', items:[]}}}});"
        ),
        None,
        Duration::from_secs(10),
    );
    // a record of the former local runtime is listed and never read
    record(&home, "local", text(&own), "layerfs-proj-1", |_| {});
    home.node(
        &format!(
            "const [item] = ({records}).value.items;
            assert.deepEqual([item.runtime, item.state], ['local', 'unknown']);
            assert.match(item.message, /never used/);"
        ),
        None,
        Duration::from_secs(10),
    );
    assert!(own.join("daemon.log").exists(), "listing deletes nothing");
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

#[test]
fn one_sandbox_setting_maps_the_former_pair_and_refuses_conflicts() {
    // #527 step 11 (D3): LAYERFS_SANDBOX=docker|native|namespaces is the one
    // setting; the binding validates it and maps the former pair. A
    // namespaces record makes the call read its deployment's backend.
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    let digest = format!("sha256:{}", "a".repeat(64));
    record(
        &home,
        "namespaces",
        text(&own),
        "layerfs-proj-1",
        set("image", Some(SdkValue::text(&digest))),
    );
    for (deploy, message) in [
        (
            "sandbox:'namespaces', sandboxBackend:'native'",
            r"deploy\.sandbox \(LAYERFS_SANDBOX\) namespaces conflicts with deploy\.sandboxBackend \(LAYERFS_SANDBOX_BACKEND\) native",
        ),
        (
            "sandbox:'docker', sandboxBackend:'namespaces'",
            r"\(LAYERFS_SANDBOX\) docker conflicts with .*\(LAYERFS_SANDBOX_BACKEND\) namespaces",
        ),
        (
            "sandboxBackend:'namespaces'",
            r"\(LAYERFS_SANDBOX\) unset \(docker\) conflicts",
        ),
        (
            "sandbox:'native', sandboxBackend:'podman'",
            r"\(LAYERFS_SANDBOX_BACKEND\) is native or namespaces, not podman",
        ),
        (
            "sandbox:'podman'",
            r"\(LAYERFS_SANDBOX\) is docker, native \(Linux or macOS\) or namespaces \(Linux\), not podman",
        ),
    ] {
        home.node(
            &format!(
                "const r = await native.backendCall('runtime.sandbox.reclaim', {{...args, deploy:{{{deploy}, localDir:{:?}}}}});
                assert.equal(r.refused?.code, 'InvalidInput', JSON.stringify(r));
                assert.match(r.refused.message, /{message}/);",
                text(&root)
            ),
            None,
            Duration::from_secs(10),
        );
    }
    assert!(own.join("daemon.log").exists());
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
    // the former pair with the native backend named is native: a stopped
    // native record is its own and is reclaimed
    record(&home, "native", text(&own), "layerfs-proj-1", |_| {});
    home.node(
        &format!(
            "const r = await native.backendCall('runtime.sandbox.reclaim', {{...args, deploy:{{sandbox:'native', sandboxBackend:'native', localDir:{:?}}}}});
            {RECLAIMED}",
            text(&root)
        ),
        None,
        Duration::from_secs(10),
    );
    assert!(!own.exists());
}
