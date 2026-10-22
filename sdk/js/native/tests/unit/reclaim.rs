//! #373: a Sandbox whose container the Engine no longer has, or has
//! stopped, is reclaimed (a stopped container deleted once, then its
//! records) so its number is free again; nothing else counts as gone.
//! Included by sandbox.rs.
use super::reclaim;
use layerfs_bridge::control::SdkValue;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ID: &str = "1";
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A private LayerFS home with Sandbox 1 of project `proj` recorded as
/// running in container CID, behind the Engine socket `docker.sock`.
fn home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "layerfs-reclaim-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let private = |path: &Path| std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
    private(&home);
    private(&home.join("proj"));
    private(&home.join("proj").join("sandboxes"));
    private(&dir(&home));
    let bytes = |n: u8| SdkValue::Bytes(vec![n; 32]);
    crate::discovery::save(
        &dir(&home).join("sandbox.record"),
        SdkValue::map([
            ("id", SdkValue::text(ID)),
            ("project", SdkValue::text("proj")),
            ("container", SdkValue::text(CID)),
            (
                "docker",
                SdkValue::text(home.join("docker.sock").to_str().unwrap()),
            ),
            (
                "image",
                SdkValue::text(format!("sha256:{}", "a".repeat(64))),
            ),
            ("volume", SdkValue::text("lfs-test-borrowed")),
            ("secret", bytes(1)),
            ("peer", bytes(2)),
            ("instance", bytes(3)),
        ]),
    )
    .unwrap();
    home
}

/// Sandbox 1's directory.
fn dir(home: &Path) -> PathBuf {
    home.join("proj").join("sandboxes").join(ID)
}

/// The Engine at `home/docker.sock`, answering one request with `status` and `body`.
fn engine(home: &Path, status: u16, body: &str) -> JoinHandle<String> {
    let engine = engines(home, vec![(status, body.to_owned())]);
    thread::spawn(move || engine.join().unwrap().remove(0))
}

/// The Engine at `home/docker.sock`, answering each request, one connection
/// each, with the next `(status, body)`; it returns their request heads.
fn engines(home: &Path, responses: Vec<(u16, String)>) -> JoinHandle<Vec<String>> {
    let listener = UnixListener::bind(home.join("docker.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut requests = Vec::new();
        for (status, body) in responses {
            let response = format!(
                "HTTP/1.1 {status} test\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("bounded engine admission: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            (&stream).write_all(response.as_bytes()).unwrap();
            requests.push(request);
        }
        requests
    })
}

/// The container's system-call filter as the Engine carries it inside a JSON string.
const SECCOMP: &str = r#"seccomp={\"defaultAction\":\"SCMP_ACT_ALLOW\",\"syscalls\":[{\"names\":[\"unshare\",\"clone\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":1,\"args\":[{\"index\":0,\"value\":268435456,\"valueTwo\":268435456,\"op\":\"SCMP_CMP_MASKED_EQ\"}]},{\"names\":[\"clone3\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":38}]}"#;

/// The Engine's inspection of CID after its process exited (Docker
/// restarted): the recorded deployment, intact, not running, no port published.
fn exited() -> String {
    let image = format!("sha256:{}", "a".repeat(64));
    format!(
        r#"{{"Id":"{CID}","Image":"{image}","State":{{"Running":false,"Paused":false,"Restarting":false,"Dead":false}},"Config":{{"User":"0:0","Tty":false,"Entrypoint":["/usr/local/bin/layerfs-daemon"],"Cmd":["--config","/layerfs-local/config/daemon.setup"]}},"HostConfig":{{"Init":true,"Privileged":false,"CapAdd":["CAP_SYS_ADMIN"],"Devices":[{{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}}],"SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"],"RestartPolicy":{{"Name":"no"}},"LogConfig":{{"Type":"json-file"}}}},"Mounts":[{{"Type":"volume","Name":"lfs-test-borrowed","Destination":"/layerfs-store","RW":true,"Driver":"local"}}],"NetworkSettings":{{"Ports":{{}}}}}}"#
    )
}

fn args(home: &Path) -> SdkValue {
    SdkValue::map([
        ("sandbox", SdkValue::text(ID)),
        ("project", SdkValue::text("proj")),
        ("home", SdkValue::text(home.to_str().unwrap())),
    ])
}

/// Reopens Sandbox 1 and, when it is gone, reclaims it, as placement does.
fn place(home: &Path) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let refusal = crate::discovery::reopen(&args(home))
        .err()
        .expect("not running");
    match crate::discovery::gone_container(&refusal) {
        Some((container, stopped)) => reclaim(&args(home), ID, container, stopped),
        None => Err(refusal),
    }
}

#[test]
fn a_gone_container_is_reclaimed_by_removing_its_records() {
    let home = home();
    let engine = engine(
        &home,
        404,
        &format!(r#"{{"message":"No such container: {CID}"}}"#),
    );
    let closed = place(&home).unwrap();
    assert_eq!(closed.get_text("kind"), Some("Reclaimed"));
    assert_eq!(closed.get_text("id"), Some(ID));
    assert!(engine
        .join()
        .unwrap()
        .starts_with(&format!("GET /v1.54/containers/{CID}/json")));
    assert!(!dir(&home).exists(), "its number is free again");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn another_404_is_a_failure_that_removes_nothing() {
    let home = home();
    let engine = engine(&home, 404, r#"{"message":"page not found"}"#);
    let refusal = place(&home).unwrap_err();
    assert_eq!(refusal.code, "Failed");
    engine.join().unwrap();
    assert!(dir(&home).join("sandbox.record").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_number_started_again_with_another_container_is_left_alone() {
    let home = home();
    let other = "c".repeat(64);
    let refusal = reclaim(&args(&home), ID, &other, false).unwrap_err();
    assert_eq!(refusal.code, "Busy");
    assert!(dir(&home).join("sandbox.record").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stopped_container_is_refused_as_gone_then_reclaimed_by_deleting_it() {
    let home = home();
    let engine = engine(&home, 200, &exited());
    let refusal = crate::discovery::reopen(&args(&home)).err().unwrap();
    engine.join().unwrap();
    assert_eq!(refusal.code, "NoSuchSandbox");
    assert!(matches!(
        refusal.detail.get("containerGone"),
        Some(SdkValue::Bool(true))
    ));
    assert!(matches!(
        refusal.detail.get("containerStopped"),
        Some(SdkValue::Bool(true))
    ));
    assert_eq!(
        crate::discovery::gone_container(&refusal),
        Some((CID, true))
    );

    std::fs::remove_file(home.join("docker.sock")).unwrap();
    let engine = engines(&home, vec![(200, exited()), (204, String::new())]);
    let closed = place(&home).unwrap();
    assert_eq!(closed.get_text("kind"), Some("Reclaimed"));
    let requests = engine.join().unwrap();
    assert!(requests[0].starts_with(&format!("GET /v1.54/containers/{CID}/json")));
    assert!(requests[1].starts_with(&format!(
        "DELETE /v1.54/containers/{CID}?force=false&v=false"
    )));
    assert!(!dir(&home).exists(), "its number is free again");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stopped_container_the_engine_will_not_delete_keeps_its_records() {
    let home = home();
    let running = r#"{"message":"cannot remove container: container is running"}"#;
    let engine = engines(&home, vec![(200, exited()), (409, running.to_owned())]);
    let refusal = place(&home).unwrap_err();
    assert_eq!(refusal.code, "Failed");
    assert_eq!(engine.join().unwrap().len(), 2);
    assert!(dir(&home).join("sandbox.record").exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// Sandbox 1 recorded as a local Sandbox in `directory` named `name`, its
/// daemon long gone (no process has id u32::MAX).
#[cfg(target_os = "linux")]
fn local_record(home: &Path, directory: &Path, name: &str) {
    let bytes = |n: u8| SdkValue::Bytes(vec![n; 32]);
    crate::discovery::save(
        &dir(home).join("sandbox.record"),
        SdkValue::map([
            ("id", SdkValue::text(ID)),
            ("project", SdkValue::text("proj")),
            ("name", SdkValue::text(name)),
            ("container", SdkValue::text(CID)),
            ("runtime", SdkValue::text("local")),
            ("directory", SdkValue::text(directory.to_str().unwrap())),
            ("pid", SdkValue::Int(i64::from(u32::MAX))),
            ("start", SdkValue::text("1")),
            ("port", SdkValue::Int(40404)),
            ("image", SdkValue::text("local")),
            ("secret", bytes(1)),
            ("peer", bytes(2)),
            ("instance", bytes(3)),
        ]),
    )
    .unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_local_record_outside_its_configured_directory_is_never_deleted() {
    let home = home();
    let root = home.join("local");
    std::fs::create_dir(&root).unwrap();
    let local = || {
        let mut args = args(&home);
        if let SdkValue::Map(fields) = &mut args {
            fields.push((
                "deploy".into(),
                SdkValue::map([
                    ("sandbox", SdkValue::text("local")),
                    ("localDir", SdkValue::text(root.to_str().unwrap())),
                ]),
            ));
        }
        args
    };
    // an edited or damaged record naming a data directory by its own name
    let data = home.join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("keep"), b"x").unwrap();
    local_record(&home, &data, "data");
    let refusal = reclaim(&local(), ID, CID, true).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert!(
        refusal.message.contains("neither used nor deleted"),
        "{}",
        refusal.message
    );
    assert!(
        data.join("keep").exists(),
        "a delete reached {}",
        data.display()
    );
    assert!(dir(&home).join("sandbox.record").exists());
    // the same from a record of another LAYERFS_LOCAL_DIR
    let old = home.join("old");
    std::fs::create_dir_all(old.join("layerfs-proj-1")).unwrap();
    local_record(&home, &old.join("layerfs-proj-1"), "layerfs-proj-1");
    assert_eq!(
        reclaim(&local(), ID, CID, true).unwrap_err().code,
        "InvalidInput"
    );
    assert!(old.join("layerfs-proj-1").exists());
    // where this deployment puts it, the stopped Sandbox is deleted and reclaimed
    let own = root.join("layerfs-proj-1");
    std::fs::create_dir_all(own.join("tmp")).unwrap();
    local_record(&home, &own, "layerfs-proj-1");
    let reclaimed = reclaim(&local(), ID, CID, true).unwrap();
    assert_eq!(reclaimed.get_text("kind"), Some("Reclaimed"));
    assert!(!own.exists());
    assert!(!dir(&home).exists(), "its number is free again");
    std::fs::remove_dir_all(home).unwrap();
}
