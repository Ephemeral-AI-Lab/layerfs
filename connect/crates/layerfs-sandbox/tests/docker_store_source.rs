//! #533 step 12 (deferral closed): a Sandbox container's Store folder comes
//! from a whole volume (1.1.0), a folder of the docker machine's one data
//! volume, or a host folder, written as Docker's `-v` source. Each is sent
//! at create and found again at inspection, through a bounded external Unix
//! Engine peer; a malformed source is refused before any Engine request. A
//! loopback PostgreSQL URL is renamed for a container under Docker Desktop.
#[allow(dead_code)]
mod support {
    pub mod engine;
}
use layerfs_sandbox::docker::{desktop_url, store_locator, SandboxRequest};
use std::{
    fs::{self, File},
    io::Write,
    time::Duration,
};
use support::engine::{fixed, frame, Peer};
const CID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECCOMP: &str = r#"seccomp={\"defaultAction\":\"SCMP_ACT_ALLOW\",\"syscalls\":[{\"names\":[\"unshare\",\"clone\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":1,\"args\":[{\"index\":0,\"value\":268435456,\"valueTwo\":268435456,\"op\":\"SCMP_CMP_MASKED_EQ\"}]},{\"names\":[\"clone3\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":38}]}"#;
const FOLDER: &str = "lfs-test-docker-data/store/shop";
const HOST: &str = "/srv/layerfs home/store/shop";

fn request(source: &str) -> SandboxRequest {
    SandboxRequest {
        image: IMAGE.into(),
        store_volume: Some(source.into()),
        port: 30421,
        labels: vec![("layerfs.role".into(), "sandbox".into())],
    }
}
fn volume(name: &str) -> Vec<u8> {
    fixed(
        200,
        format!(r#"{{"Name":"{name}","Driver":"local","Scope":"local","Options":null}}"#)
            .as_bytes(),
    )
}
fn logs(payload: &[u8]) -> Vec<u8> {
    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\n\r\n", payload.len()).into_bytes();
    r.extend_from_slice(payload);
    r
}
fn inspection(mounts: &str) -> Vec<u8> {
    fixed(200, format!(r#"{{"Id":"{CID}","Image":"{IMAGE}","State":{{"Running":true,"Paused":false,"Restarting":false,"Dead":false}},"Config":{{"User":"0:0","Labels":{{"layerfs.role":"sandbox"}},"Tty":false,"Entrypoint":["/usr/local/bin/layerfs-daemon"],"Cmd":["--config","/layerfs-local/config/daemon.setup"]}},"HostConfig":{{"Init":true,"Privileged":false,"CapAdd":["CAP_SYS_ADMIN"],"Devices":[{{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}}],"SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"],"RestartPolicy":{{"Name":"no"}},"LogConfig":{{"Type":"json-file"}}}},"Mounts":[{mounts}],"NetworkSettings":{{"Ports":{{"30421/tcp":[{{"HostIp":"127.0.0.1","HostPort":"54321"}}]}}}}}}"#).as_bytes())
}

/// Creates, uploads, starts and inspects one Sandbox whose Store comes from
/// `source`, the Engine answering `mounts`; whether the inspection was
/// accepted, and every Engine request.
fn launch(source: &str, volume_read: bool, mounts: &str, tag: &str) -> (bool, Vec<Vec<u8>>) {
    let mut responses = Vec::new();
    if volume_read {
        responses.push(volume("lfs-test-docker-data"));
    }
    responses.extend([
        fixed(201, format!(r#"{{"Id":"{CID}","Warnings":[]}}"#).as_bytes()),
        fixed(200, b""),
        fixed(204, b""),
        logs(&frame(1, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n")),
        inspection(mounts),
    ]);
    let (docker, peer) = Peer::new(responses, 8192);
    let mut sandbox = docker.create_sandbox(request(source)).unwrap();
    let path = std::env::temp_dir().join(format!("lfs-source-{tag}-{}", std::process::id()));
    File::create(&path).unwrap().write_all(&[0x5a; 64]).unwrap();
    sandbox
        .upload(&mut File::open(&path).unwrap(), b"private")
        .unwrap();
    sandbox.start().unwrap();
    sandbox.wait_listener(Duration::from_secs(2)).unwrap();
    let accepted = sandbox.endpoint().is_ok();
    fs::remove_file(path).unwrap();
    (accepted, peer.finish())
}

#[test]
fn each_source_names_where_the_store_is_in_the_container() {
    assert_eq!(
        store_locator("lfs-store-shop"),
        Some("/layerfs-store/global/store.sqlite")
    );
    assert_eq!(store_locator(FOLDER), Some("/layerfs-store/store.sqlite"));
    assert_eq!(store_locator(HOST), Some("/layerfs-store/store.sqlite"));
    for bad in [
        "",
        "/",
        "-vol",
        "vol/",
        "vol/../x",
        "vol/.hidden",
        "/a/../b",
        "/a//b",
        "/a/b\n",
    ] {
        assert_eq!(store_locator(bad), None, "{bad:?}");
    }
}

#[test]
fn a_volume_folder_is_mounted_by_its_subpath_after_its_volume_is_read() {
    let mount = r#"{"Type":"volume","Name":"lfs-test-docker-data","Source":"/var/lib/docker/volumes/lfs-test-docker-data/_data","Destination":"/layerfs-store","RW":true,"Driver":"local"}"#;
    let (accepted, requests) = launch(FOLDER, true, mount, "folder");
    assert!(accepted);
    let read = String::from_utf8_lossy(&requests[0]);
    assert!(
        read.starts_with("GET /v1.54/volumes/lfs-test-docker-data "),
        "{read}"
    );
    let create = String::from_utf8_lossy(&requests[1]);
    assert!(create.contains(r#""Mounts":[{"Type":"volume","Source":"lfs-test-docker-data","Target":"/layerfs-store","ReadOnly":false,"VolumeOptions":{"NoCopy":true,"Subpath":"store/shop"}}]"#), "{create}");
}

#[test]
fn a_host_folder_is_bind_mounted_with_no_volume_read() {
    let mount = format!(
        r#"{{"Type":"bind","Source":"{HOST}","Destination":"/layerfs-store","Mode":"","RW":true,"Propagation":"rprivate"}}"#
    );
    let (accepted, requests) = launch(HOST, false, &mount, "bind");
    assert!(accepted);
    let create = String::from_utf8_lossy(&requests[0]);
    assert!(
        create.starts_with("POST /v1.54/containers/create"),
        "{create}"
    );
    assert!(create.contains(&format!(
        r#""Mounts":[{{"Type":"bind","Source":"{HOST}","Target":"/layerfs-store","ReadOnly":false}}]"#
    )));
    // the inspection must show that folder, bound read-write, and only it
    for (tag, wrong) in [
        ("other", r#"{"Type":"bind","Source":"/srv/other","Destination":"/layerfs-store","RW":true}"#.to_owned()),
        ("readonly", format!(r#"{{"Type":"bind","Source":"{HOST}","Destination":"/layerfs-store","RW":false}}"#)),
        ("volume", r#"{"Type":"volume","Name":"lfs-test-docker-data","Destination":"/layerfs-store","RW":true,"Driver":"local"}"#.to_owned()),
    ] {
        assert!(!launch(HOST, false, &wrong, tag).0, "{tag}");
    }
}

#[test]
fn a_malformed_source_is_refused_before_any_engine_request() {
    let (docker, peer) = Peer::new(Vec::new(), 8192);
    assert!(docker.create_sandbox(request("vol/../x")).is_err());
    assert!(peer.finish().is_empty());
}

#[test]
fn a_loopback_postgresql_url_names_the_desktop_host() {
    assert_eq!(
        desktop_url("postgres://u:p@127.0.0.1:5432/db?sslmode=disable").as_deref(),
        Some("postgres://u:p@host.docker.internal:5432/db?sslmode=disable")
    );
    assert_eq!(
        desktop_url("postgresql://localhost/db").as_deref(),
        Some("postgresql://host.docker.internal/db")
    );
    assert_eq!(
        desktop_url("postgres://u@[::1]:6543/db").as_deref(),
        Some("postgres://u@host.docker.internal:6543/db")
    );
    for other in [
        "postgres://u@db.example.com:5432/db",
        "postgres://u@127.0.0.2/db",
        "not a url",
        // a port that is not 1 to 5 digits (here a password's unescaped `/`
        // cut the authority) is not read
        "postgres://u:pw/x@localhost/db",
        "postgres://u@localhost:/db",
        "postgres://u@[::1]x/db",
    ] {
        assert_eq!(desktop_url(other), None, "{other}");
    }
}
