//! External tests of the built Node binding's local Sandbox records
//! (LAYERFS_SANDBOX=local). No private Rust source is included: every case
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

/// Sandbox 1 of `proj` recorded as a `runtime` (local or native) Sandbox
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

/// The public reclaim of a `sandbox` (local or native) deployment, with
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
fn a_local_record_naming_another_directory_is_neither_used_nor_deleted() {
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
        record(&home, "local", &directory, name, |_| {});
        reclaim(&home, "local", Some(r), NEITHER);
        assert!(
            home.0.join("proj/sandboxes/1/sandbox.record").exists(),
            "the record of {directory} was removed"
        );
    }
    assert!(data.join("keep").exists(), "a delete reached {}", data.display());
    assert!(old.exists(), "a delete reached {}", old.display());
}

#[test]
fn a_local_record_is_used_only_with_its_deployment_s_absolute_local_dir() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(&home, "local", text(&own), "layerfs-proj-1", |_| {});
    let refused = "assert.equal(r.refused?.code, 'InvalidInput', JSON.stringify(r));";
    reclaim(&home, "local", None, refused);
    reclaim(&home, "local", Some("relative"), refused);
    reclaim(
        &home,
        "local",
        Some(&format!("{}/../local", text(&root))),
        refused,
    );
    assert!(own.join("daemon.log").exists());
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}

#[test]
fn a_stopped_local_sandbox_where_its_deployment_puts_it_is_deleted_and_reclaimed() {
    let home = Home::new();
    let root = root(&home);
    let own = own(&root);
    record(&home, "local", text(&own), "layerfs-proj-1", |_| {});
    reclaim(&home, "local", Some(text(&root)), RECLAIMED);
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
        "local",
        text(&own),
        "layerfs-proj-1",
        set("boot", None),
    );
    reclaim(&home, "local", Some(text(&root)), RECLAIMED);
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
            "local",
            text(&own),
            "layerfs-proj-1",
            set(key, Some(value)),
        );
        reclaim(
            &home,
            "local",
            Some(text(&root)),
            &format!("assert.equal(r.refused?.code, {code:?}, JSON.stringify(r));"),
        );
        assert!(own.join("daemon.log").exists(), "{key}");
        assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists(), "{key}");
    }
}

#[test]
fn a_docker_record_stays_docker_without_a_local_deployment() {
    // the default record of Home is a Docker one: with no Engine at its
    // socket it is neither reclaimed nor read as a local Sandbox
    let home = Home::new();
    let root = root(&home);
    reclaim(
        &home,
        "local",
        Some(text(&root)),
        "assert.equal(r.refused?.code, 'Failed', JSON.stringify(r));",
    );
    assert!(home.0.join("proj/sandboxes/1/sandbox.record").exists());
}
