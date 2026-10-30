//! The namespace backend's formats and records: the init stage's spec, the
//! recorded facts a reopen reads, and the refusals that keep one runtime's
//! records from another. Nothing here starts a process.
use layerfs_sandbox::{
    backend::HelperSpec,
    provider::{DockerRecord, LocalRecord, SandboxRecord},
};
use std::path::PathBuf;

fn spec() -> HelperSpec {
    HelperSpec {
        root: PathBuf::from("/var/lib/layerfs/local/sandbox-1.root"),
        sandbox: PathBuf::from("/var/lib/layerfs/local/sandbox-1"),
        store: PathBuf::from("/layerfs-store/project"),
        daemon: PathBuf::from("/var/lib/layerfs/local/sandbox-1/bin/layerfs-daemon"),
        config: PathBuf::from("/var/lib/layerfs/local/sandbox-1/config/daemon.setup"),
        cgroup: PathBuf::from("/sys/fs/cgroup/layerfs/0000000000000000000000000000000000000000000000000000000000000001"),
    }
}

#[test]
fn the_init_spec_roundtrips() {
    let spec = spec();
    let encoded = spec.encode().unwrap();
    assert_eq!(HelperSpec::parse(&encoded).unwrap(), spec);
    // one line per field, key first
    let text = String::from_utf8(encoded).unwrap();
    assert_eq!(text.lines().count(), 6);
    assert!(text.starts_with("root=/var/lib/layerfs/local/sandbox-1.root\n"));
}

#[test]
fn a_value_may_hold_an_equals_sign() {
    let mut spec = spec();
    spec.store = PathBuf::from("/layerfs-store/a=b");
    let encoded = spec.encode().unwrap();
    assert_eq!(HelperSpec::parse(&encoded).unwrap().store, spec.store);
}

#[test]
fn a_path_the_init_stage_cannot_mount_is_refused() {
    let relative = HelperSpec {
        root: PathBuf::from("relative/root"),
        ..spec()
    };
    assert!(relative.encode().is_err());
    let parent = HelperSpec {
        sandbox: PathBuf::from("/var/lib/../sandbox"),
        ..spec()
    };
    assert!(parent.encode().is_err());
    let newline = HelperSpec {
        config: PathBuf::from("/var/lib/a\nb"),
        ..spec()
    };
    assert!(newline.encode().is_err());
    let bytes = newline.encode().unwrap_err();
    assert_eq!(format!("{bytes:?}"), r#"Protocol("helper spec path")"#);
}

#[test]
fn a_spec_is_refused_unless_every_field_appears_once() {
    let encoded = spec().encode().unwrap();
    let text = String::from_utf8(encoded).unwrap();
    // one field missing
    let missing = text
        .lines()
        .filter(|line| !line.starts_with("cgroup="))
        .collect::<Vec<_>>();
    assert!(HelperSpec::parse(missing.join("\n").as_bytes()).is_err());
    // one field twice
    let twice = format!("{text}cgroup=/sys/fs/cgroup/layerfs/2\n");
    assert!(HelperSpec::parse(twice.as_bytes()).is_err());
    // an unknown field
    let unknown = format!("{text}kernel=nfs\n");
    assert!(HelperSpec::parse(unknown.as_bytes()).is_err());
    // a line without a key
    assert!(HelperSpec::parse(format!("{text}/\n").as_bytes()).is_err());
    // the whole file is bounded
    let huge = vec![b'a'; 8193];
    assert!(HelperSpec::parse(&huge).is_err());
}

#[test]
fn a_namespaces_record_is_its_own_variant_and_keeps_the_local_facts() {
    let local = LocalRecord {
        name: "layerfs-project-1".to_owned(),
        pid: 4242,
        start_time: 7,
        boot: Some(1),
        port: 41000,
        labels: vec![("layerfs.role".to_owned(), "sandbox".to_owned())],
    };
    let record = SandboxRecord::Namespaces(local.clone());
    assert_eq!(record.namespaces(), Some(&local));
    assert!(record.local().is_none());
    assert!(record.docker().is_none());
    // the native record of the same facts stays a native record
    let native = SandboxRecord::Local(local);
    assert!(native.namespaces().is_none());
    // a Docker record written before this backend existed still reads as Docker
    let docker = SandboxRecord::Docker(DockerRecord {
        image: "sha256:0".to_owned(),
        store_volume: "volume".to_owned(),
        port: 30421,
        labels: Vec::new(),
    });
    assert!(docker.namespaces().is_none());
    assert!(docker.local().is_none());
}
