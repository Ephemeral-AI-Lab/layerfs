//! #373: a Sandbox container carries its labels (`layerfs.role`,
//! `layerfs.project`, `layerfs.sandbox`), sent once at create and required on
//! every reopen, through a bounded external Unix Engine peer. Its only mount
//! is the Store volume, read-write at `/layerfs-store`.
#[allow(dead_code)]
mod support {
    pub mod engine;
}
use layerfs_sandbox::{
    backend::docker::{CreatePhase, SandboxRequest},
    RuntimeError,
};
use std::{
    fs::{self, File},
    io::Write,
    time::Duration,
};
use support::engine::{fixed, frame, Peer};
const CID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECCOMP: &str = r#"seccomp={\"defaultAction\":\"SCMP_ACT_ALLOW\",\"syscalls\":[{\"names\":[\"unshare\",\"clone\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":1,\"args\":[{\"index\":0,\"value\":268435456,\"valueTwo\":268435456,\"op\":\"SCMP_CMP_MASKED_EQ\"}]},{\"names\":[\"clone3\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":38}]}"#;
const STORE: &str = r#"{"Type":"volume","Name":"lfs-test-borrowed","Destination":"/layerfs-store","RW":true,"Driver":"local"}"#;
const OTHER: &str = r#"{"Type":"volume","Name":"lfs-pnpm-store","Destination":"/pnpm-store","RW":false,"Driver":"local"}"#;
const LABELS: [(&str, &str); 3] = [
    ("layerfs.role", "sandbox"),
    ("layerfs.project", "shop"),
    ("layerfs.sandbox", "2"),
];

fn request(labels: &[(&str, &str)]) -> SandboxRequest {
    SandboxRequest {
        image: IMAGE.into(),
        store_volume: "lfs-test-borrowed".into(),
        port: 30421,
        labels: (labels.iter())
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
    }
}
fn volume(name: &str) -> Vec<u8> {
    fixed(
        200,
        format!(r#"{{"Name":"{name}","Driver":"local","Scope":"local","Options":null}}"#)
            .as_bytes(),
    )
}
fn created() -> Vec<u8> {
    fixed(201, format!(r#"{{"Id":"{CID}","Warnings":[]}}"#).as_bytes())
}
fn logs(payload: &[u8]) -> Vec<u8> {
    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\n\r\n", payload.len()).into_bytes();
    r.extend_from_slice(payload);
    r
}
/// The inspection the runtime accepts, with the given `Labels` object text
/// (`null` for none) and `Mounts` entries.
fn inspection(labels: &str, mounts: &[&str]) -> Vec<u8> {
    let mounts = mounts.join(",");
    fixed(200, format!(r#"{{"Id":"{CID}","Image":"{IMAGE}","State":{{"Running":true,"Paused":false,"Restarting":false,"Dead":false}},"Config":{{"User":"0:0","Labels":{labels},"Tty":false,"Entrypoint":["/usr/local/bin/layerfs-daemon"],"Cmd":["--config","/layerfs-local/config/daemon.setup"]}},"HostConfig":{{"Init":true,"Privileged":false,"CapAdd":["CAP_SYS_ADMIN"],"Devices":[{{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}}],"SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"],"RestartPolicy":{{"Name":"no"}},"LogConfig":{{"Type":"json-file"}}}},"Mounts":[{mounts}],"NetworkSettings":{{"Ports":{{"30421/tcp":[{{"HostIp":"127.0.0.1","HostPort":"54321"}}]}}}}}}"#).as_bytes())
}
/// Creates, uploads, starts and inspects one Sandbox requesting `labels`;
/// returns whether the inspection (`observed` labels, `mounts`) was accepted
/// and every Engine request.
fn inspect(
    labels: &[(&str, &str)],
    observed: &str,
    mounts: &[&str],
    tag: &str,
) -> (bool, Vec<Vec<u8>>) {
    let responses = vec![
        volume("lfs-test-borrowed"),
        created(),
        fixed(200, b""),
        fixed(204, b""),
        logs(&frame(1, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n")),
        inspection(observed, mounts),
    ];
    let (docker, peer) = Peer::new(responses, 8192);
    let mut sandbox = docker.create_sandbox(request(labels)).unwrap();
    let path = std::env::temp_dir().join(format!("lfs-labels-{tag}-{}", std::process::id()));
    File::create(&path)
        .unwrap()
        .write_all(&[0x5a; 4096])
        .unwrap();
    sandbox
        .upload(&mut File::open(&path).unwrap(), b"private")
        .unwrap();
    sandbox.start().unwrap();
    sandbox.wait_listener(Duration::from_secs(2)).unwrap();
    let accepted = sandbox.endpoint().is_ok();
    fs::remove_file(path).unwrap();
    (accepted, peer.finish())
}
const ALL: &str = r#"{"org.opencontainers.image.title":"layerfs-runtime","layerfs.sandbox":"2","layerfs.project":"shop","layerfs.role":"sandbox"}"#;

#[test]
fn labels_are_sent_at_create_and_found_in_any_order_beside_the_image_s() {
    let (accepted, requests) = inspect(&LABELS, ALL, &[STORE], "accept");
    assert!(accepted, "every requested label, beside an image label");
    let create = String::from_utf8_lossy(&requests[1]).into_owned();
    assert!(
        create.starts_with("POST /v1.54/containers/create"),
        "{create}"
    );
    assert!(create.contains(r#""Labels":{"layerfs.role":"sandbox","layerfs.project":"shop","layerfs.sandbox":"2"},"User":"0:0""#), "{create}");
    assert!(create.contains(r#""Mounts":[{"Type":"volume","Source":"lfs-test-borrowed","Target":"/layerfs-store","ReadOnly":false,"VolumeOptions":{"NoCopy":true}}],"#), "{create}");
    // #423: the daemon's log is capped at two 10 MB files
    assert!(
        create.contains(
            r#""LogConfig":{"Type":"json-file","Config":{"max-size":"10m","max-file":"2"}}"#
        ),
        "{create}"
    );
    // a request without labels sends none and accepts whatever the image has
    let (accepted, requests) = inspect(&[], "null", &[STORE], "none");
    assert!(accepted);
    assert!(String::from_utf8_lossy(&requests[1]).contains(r#""Labels":{},"User""#));
}

#[test]
fn a_missing_or_different_label_or_another_mount_is_a_topology_mismatch() {
    let other_project = ALL.replace(r#""shop""#, r#""cafe""#);
    let missing = r#"{"layerfs.role":"sandbox","layerfs.project":"shop"}"#;
    let store_read_only = STORE.replace(r#""RW":true"#, r#""RW":false"#);
    for (tag, observed, mounts) in [
        ("other-project", other_project.as_str(), vec![STORE]),
        ("missing", missing, vec![STORE]),
        ("none-mismatch", "null", vec![STORE]),
        ("second-mount", ALL, vec![STORE, OTHER]),
        ("repeated", ALL, vec![STORE, STORE]),
        ("store-ro", ALL, vec![store_read_only.as_str()]),
    ] {
        assert!(!inspect(&LABELS, observed, &mounts, tag).0, "{tag}");
    }
}

#[test]
fn bad_labels_are_refused_before_any_engine_request() {
    let long = "a".repeat(129);
    for labels in [
        vec![("", "x")],
        vec![("layerfs.role", "")],
        vec![("layerfs role", "x")],
        vec![("layerfs.role", "a\"b")],
        vec![("layerfs.role", long.as_str())],
        vec![("layerfs.role", "a"), ("layerfs.role", "b")],
    ] {
        let (docker, peer) = Peer::new(vec![], 7);
        let f = docker.create_sandbox(request(&labels)).unwrap_err();
        assert_eq!(f.phase, CreatePhase::Validate, "{labels:?}");
        assert!(matches!(f.transfer.cause, RuntimeError::Protocol(_)));
        assert!(peer.finish().is_empty(), "{labels:?}");
    }
}
