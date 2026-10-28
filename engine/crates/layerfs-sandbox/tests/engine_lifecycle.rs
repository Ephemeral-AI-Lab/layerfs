//! Actual public lifecycle adapter through a bounded external Unix Engine peer.
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
fn request() -> SandboxRequest {
    SandboxRequest {
        image: IMAGE.into(),
        store_volume: "lfs-test-borrowed".into(),
        port: 30421,
        labels: vec![],
    }
}
fn volume() -> Vec<u8> {
    fixed(200,b"{\"Name\":\"lfs-test-borrowed\",\"Driver\":\"local\",\"Scope\":\"local\",\"Options\":null}")
}
fn created() -> Vec<u8> {
    fixed(
        201,
        format!("{{\"Id\":\"{CID}\",\"Warnings\":[]}}").as_bytes(),
    )
}
fn logs(payload: &[u8]) -> Vec<u8> {
    let mut r=format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\n\r\n",payload.len()).into_bytes();
    r.extend_from_slice(payload);
    r
}
fn binary(label: &str) -> (std::path::PathBuf, File) {
    let path = std::env::temp_dir().join(format!("lfs-deployment-{label}-{}", std::process::id()));
    let mut file = File::create(&path).unwrap();
    file.write_all(&vec![0x5a; 256 * 1024 + 7]).unwrap();
    drop(file);
    (path.clone(), File::open(path).unwrap())
}
/// The container's system-call filter as the Engine carries it inside a JSON
/// string: user-namespace creation denied, everything else allowed.
const SECCOMP: &str = r#"seccomp={\"defaultAction\":\"SCMP_ACT_ALLOW\",\"syscalls\":[{\"names\":[\"unshare\",\"clone\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":1,\"args\":[{\"index\":0,\"value\":268435456,\"valueTwo\":268435456,\"op\":\"SCMP_CMP_MASKED_EQ\"}]},{\"names\":[\"clone3\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":38}]}"#;
fn endpoint() -> Vec<u8> {
    fixed(200, inspect().as_bytes())
}
/// The Engine's inspection of the running Sandbox container CID.
fn inspect() -> String {
    format!(
        r#"{{"Id":"{CID}","Image":"{IMAGE}","State":{{"Running":true,"Paused":false,"Restarting":false,"Dead":false}},"Config":{{"User":"0:0","Tty":false,"Entrypoint":["/usr/local/bin/layerfs-daemon"],"Cmd":["--config","/layerfs-local/config/daemon.setup"]}},"HostConfig":{{"Init":true,"Privileged":false,"CapAdd":["CAP_SYS_ADMIN"],"Devices":[{{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}}],"SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"],"RestartPolicy":{{"Name":"no"}},"LogConfig":{{"Type":"json-file"}}}},"Mounts":[{{"Type":"volume","Name":"lfs-test-borrowed","Destination":"/layerfs-store","RW":true,"Driver":"local"}}],"NetworkSettings":{{"Ports":{{"30421/tcp":[{{"HostIp":"127.0.0.1","HostPort":"54321"}}]}}}}}}"#
    )
}
#[test]
fn streamed_private_archive_exact_marker_endpoint_and_borrowed_cleanup() {
    let mut payload = frame(1, &vec![b'x'; 1024 * 1024]);
    payload.extend(frame(1, b"\n"));
    payload.extend(frame(2, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n"));
    payload.extend(frame(
        1,
        b"prefix LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\nLAYERFS_DAEMON_",
    ));
    payload.extend(frame(2, b"stderr separator\n"));
    payload.extend(frame(1, b"LISTEN 0.0.0.0:30421\n"));
    let (docker, peer) = Peer::new(
        vec![
            volume(),
            created(),
            fixed(200, b""),
            fixed(204, b""),
            logs(&payload),
            endpoint(),
            fixed(204, b""),
            fixed(204, b""),
        ],
        8192,
    );
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("success");
    let work = sandbox
        .upload(&mut file, b"private config key bytes")
        .unwrap();
    assert_eq!(work.executable_read, 256 * 1024 + 7);
    assert_eq!(work.tar_encoded, work.tar_bytes);
    sandbox.start().unwrap();
    let seen = sandbox.wait_listener(Duration::from_secs(2)).unwrap();
    assert!(seen.marker_seen && seen.stdout > 1024 * 1024 && seen.stderr > 20);
    assert_eq!(sandbox.endpoint().unwrap().to_string(), "127.0.0.1:54321");
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    assert!(sandbox.disposition().deleted);
    let calls = peer.finish();
    assert_eq!(calls.len(), 8);
    let archive = &calls[2];
    let body = archive.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let tar = &archive[body..];
    assert_eq!(
        &tar[512 * 4..512 * 4 + b"layerfs-local/config/daemon.setup".len()],
        b"layerfs-local/config/daemon.setup"
    );
    assert_eq!(&tar[512 * 4 + 100..512 * 4 + 108], b"0000600\0");
    assert_eq!(
        &tar[512 * 4 + 512..512 * 4 + 512 + 24],
        b"private config key bytes"
    );
    assert_eq!(&tar[tar.len() - 1024..], &[0; 1024]);
    let mut cursor = 0;
    while tar[cursor] != 0 {
        let name = &tar[cursor..cursor + 100];
        assert!(!name.starts_with(b"layerfs-store"));
        let header = &tar[cursor..cursor + 512];
        let checksum =
            u32::from_str_radix(std::str::from_utf8(&header[148..154]).unwrap(), 8).unwrap();
        let mut oracle = header.to_vec();
        oracle[148..156].fill(b' ');
        assert_eq!(checksum, oracle.iter().map(|b| u32::from(*b)).sum());
        let size = u64::from_str_radix(std::str::from_utf8(&header[124..135]).unwrap(), 8).unwrap();
        cursor += 512 + (size.div_ceil(512) * 512) as usize;
    }
    assert!(std::str::from_utf8(&calls[7])
        .unwrap()
        .contains("force=false&v=false"));
    // Kernel mounting is granted by exactly one capability and one device;
    // the container itself stays unprivileged with no-new-privileges.
    let create = String::from_utf8_lossy(&calls[1]);
    for field in [
        r#""Privileged":false"#,
        r#""CapAdd":["CAP_SYS_ADMIN"]"#,
        r#""Devices":[{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}]"#,
        r#""User":"0:0""#,
    ] {
        assert!(create.contains(field), "{field}");
    }
    // Nothing in the container may create a user namespace, so no command can
    // hold a private copy of a Workspace mount.
    let security =
        format!(r#""SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"]"#);
    assert!(create.contains(&security), "{security}");
    assert_eq!(create.matches("seccomp=").count(), 1);
    assert_eq!(create.matches("CAP_").count(), 1);
    assert_eq!(create.matches("PathOnHost").count(), 1);
    fs::remove_file(path).unwrap();
}
#[test]
fn widened_or_missing_kernel_access_is_never_reported_as_the_endpoint() {
    let exact = String::from_utf8(endpoint()).unwrap();
    for (from, to) in [
        (r#""Privileged":false"#, r#""Privileged":true"#),
        // Without the Engine's init, orphans of commands stay zombies.
        (r#""Init":true,"#, ""),
        (r#""Init":true"#, r#""Init":false"#),
        (
            r#""CapAdd":["CAP_SYS_ADMIN"]"#,
            r#""CapAdd":["CAP_SYS_ADMIN","CAP_SYS_PTRACE"]"#,
        ),
        (r#""CapAdd":["CAP_SYS_ADMIN"],"#, ""),
        (r#""PathOnHost":"/dev/fuse""#, r#""PathOnHost":"/dev/kmsg""#),
        (
            r#""CgroupPermissions":"rwm"}]"#,
            r#""CgroupPermissions":"rwm"},{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse2","CgroupPermissions":"rwm"}]"#,
        ),
        (
            r#""no-new-privileges=true","apparmor=unconfined""#,
            r#""apparmor=unconfined""#,
        ),
        // The system-call filter not named as one, and with a weaker answer.
        (r#"","seccomp="#, r#"","#),
        (r#"\"errnoRet\":1,"#, r#"\"errnoRet\":0,"#),
    ] {
        assert!(exact.contains(from), "{from}");
        let altered = exact.replacen(from, to, 1);
        // Keep the fixed response framing consistent with the altered body.
        let body = altered.split_once("\r\n\r\n").unwrap().1;
        let (docker, peer) = Peer::new(
            vec![
                volume(),
                created(),
                fixed(200, b""),
                fixed(204, b""),
                logs(&frame(1, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n")),
                fixed(200, body.as_bytes()),
            ],
            8192,
        );
        let mut sandbox = docker.create_sandbox(request()).unwrap();
        let (path, mut file) = binary(&format!("topology-{}", to.len()));
        sandbox.upload(&mut file, b"private").unwrap();
        sandbox.start().unwrap();
        sandbox.wait_listener(Duration::from_secs(2)).unwrap();
        assert!(sandbox.endpoint().is_err(), "{to}");
        drop(peer.finish());
        fs::remove_file(path).unwrap();
    }
}
#[test]
fn partial_create_identity_and_nonlocal_volume_are_not_adopted() {
    let (docker, peer) = Peer::new(
        vec![
            volume(),
            fixed(
                201,
                format!("{{\"Id\":\"{CID}\",\"Id\":\"{CID}\"}}").as_bytes(),
            ),
        ],
        7,
    );
    let f = docker.create_sandbox(request()).unwrap_err();
    assert_eq!(f.phase, CreatePhase::Create);
    assert_eq!(f.transfer.observed_container.unwrap().as_str(), CID);
    peer.finish();
    let (docker,peer)=Peer::new(vec![fixed(200,b"{\"Name\":\"lfs-test-borrowed\",\"Driver\":\"local\",\"Scope\":\"local\",\"Options\":{\"device\":\"/host/path\"}}")],7);
    let f = docker.create_sandbox(request()).unwrap_err();
    assert_eq!(f.phase, CreatePhase::Volume);
    assert!(matches!(f.transfer.cause, RuntimeError::Protocol(_)));
    assert_eq!(peer.finish().len(), 1);
}
#[test]
fn lost_upload_and_start_keep_original_owner_and_refuse_replay() {
    let (docker, peer) = Peer::new(vec![volume(), created(), Vec::new()], 8192);
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("lost-upload");
    let f = sandbox.upload(&mut file, b"secret").unwrap_err();
    assert!(f.transfer.attempted);
    assert_eq!(f.transfer.requested_container.unwrap().as_str(), CID);
    assert_eq!(f.work.executable_read, f.work.executable_bytes);
    assert!(f.transfer.pending_request.bytes().is_empty());
    assert!(
        !sandbox
            .upload(&mut file, b"secret")
            .unwrap_err()
            .transfer
            .attempted
    );
    assert!(!sandbox.start().unwrap_err().attempted);
    assert_eq!(peer.finish().len(), 3);
    fs::remove_file(path).unwrap();
    let (docker, peer) = Peer::new(vec![volume(), created(), fixed(200, b""), Vec::new()], 8192);
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("lost-start");
    sandbox.upload(&mut file, b"secret").unwrap();
    assert!(sandbox.start().unwrap_err().attempted);
    assert!(!sandbox.start().unwrap_err().attempted);
    assert!(!sandbox.delete().unwrap_err().attempted);
    assert!(!sandbox.disposition().started);
    assert_eq!(peer.finish().len(), 4);
    fs::remove_file(path).unwrap();
}
#[test]
fn stderr_and_substring_markers_never_satisfy_readiness() {
    let mut payload = frame(2, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n");
    payload.extend(frame(1, b"prefix LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n"));
    let (docker, peer) = Peer::new(
        vec![
            volume(),
            created(),
            fixed(200, b""),
            fixed(204, b""),
            logs(&payload),
        ],
        2,
    );
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("no-marker");
    sandbox.upload(&mut file, b"secret").unwrap();
    sandbox.start().unwrap();
    let f = sandbox.wait_listener(Duration::from_secs(1)).unwrap_err();
    assert!(!f.observation.marker_seen && f.observation.stderr > 0);
    assert!(
        !sandbox
            .wait_listener(Duration::from_secs(1))
            .unwrap_err()
            .transfer
            .attempted
    );
    peer.finish();
    fs::remove_file(path).unwrap();
}

#[test]
fn exited_daemon_is_stopped_by_the_engines_not_modified_then_deleted() {
    let (docker, peer) = Peer::new(
        vec![
            volume(),
            created(),
            fixed(200, b""),
            fixed(204, b""),
            logs(&frame(2, b"refused the Store\n")),
            fixed(304, b""),
            fixed(204, b""),
        ],
        2,
    );
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("exited");
    sandbox.upload(&mut file, b"secret").unwrap();
    sandbox.start().unwrap();
    assert!(
        !sandbox
            .wait_listener(Duration::from_secs(1))
            .unwrap_err()
            .observation
            .marker_seen
    );
    sandbox.stop(1).unwrap();
    assert!(sandbox.disposition().stopped);
    sandbox.delete().unwrap();
    assert!(sandbox.disposition().deleted);
    let calls = peer.finish();
    assert!(String::from_utf8_lossy(&calls[5]).contains("/stop?t=1"));
    fs::remove_file(path).unwrap();
}

#[test]
fn marker_inside_truncated_frame_keeps_evidence_and_fails_readiness() {
    let mut payload = frame(1, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n");
    let declared = u32::from_be_bytes(payload[4..8].try_into().unwrap()) + 1024;
    payload[4..8].copy_from_slice(&declared.to_be_bytes());
    let (docker, peer) = Peer::new(
        vec![
            volume(),
            created(),
            fixed(200, b""),
            fixed(204, b""),
            logs(&payload),
        ],
        7,
    );
    let mut sandbox = docker.create_sandbox(request()).unwrap();
    let (path, mut file) = binary("truncated-marker");
    sandbox.upload(&mut file, b"secret").unwrap();
    sandbox.start().unwrap();
    let failure = sandbox.wait_listener(Duration::from_secs(1)).unwrap_err();
    assert!(failure.observation.marker_seen);
    assert!(matches!(failure.transfer.cause, RuntimeError::Io(_)));
    assert!(failure.transfer.request_header_bytes > 0);
    assert_eq!(failure.transfer.sent_body_bytes, 0);
    peer.finish();
    fs::remove_file(path).unwrap();
}

/// Reopening a saved Sandbox whose container the Engine answers for with `status` and `body`.
fn reopened(status: u16, body: &[u8]) -> bool {
    let (docker, peer) = Peer::new(vec![fixed(status, body)], 64);
    let id = layerfs_sandbox::ContainerId::parse(CID).unwrap();
    let Err(failure) = layerfs_sandbox::backend::docker::Sandbox::reopen(docker, id, request())
    else {
        panic!("reopen of an unanswered container fails");
    };
    let calls = peer.finish();
    assert!(String::from_utf8_lossy(&calls[0]).contains(&format!("/containers/{CID}/json")));
    failure.container_gone()
}

#[test]
fn only_the_engines_no_such_container_answer_is_a_gone_container() {
    let gone = format!(r#"{{"message":"No such container: {CID}"}}"#);
    assert!(reopened(404, gone.as_bytes()));
    assert!(!reopened(404, br#"{"message":"page not found"}"#));
    assert!(!reopened(500, gone.as_bytes()));
}

#[test]
fn a_stop_the_engine_answers_with_no_such_container_is_a_gone_container() {
    let gone = format!(r#"{{"message":"No such container: {CID}"}}"#);
    let (docker, peer) = Peer::new(vec![endpoint(), fixed(404, gone.as_bytes())], 64);
    let id = layerfs_sandbox::ContainerId::parse(CID).unwrap();
    let mut sandbox =
        layerfs_sandbox::backend::docker::Sandbox::reopen(docker, id, request()).unwrap();
    let failure = sandbox.stop(1).unwrap_err();
    assert!(failure.container_gone());
    assert!(!sandbox.disposition().stopped);
    let calls = peer.finish();
    assert!(String::from_utf8_lossy(&calls[1]).contains("/stop?t=1"));
}

/// CID as the Engine shows it after its process exited (Docker restarted):
/// intact, not running, its port no longer published.
fn exited() -> String {
    inspect()
        .replace(r#""Running":true"#, r#""Running":false"#)
        .replace(
            r#""30421/tcp":[{"HostIp":"127.0.0.1","HostPort":"54321"}]"#,
            "",
        )
}

/// Reopening a saved Sandbox whose container the Engine inspects as `body`.
fn reopened_from(body: &str) -> Box<layerfs_sandbox::WireFailure> {
    let (docker, peer) = Peer::new(vec![fixed(200, body.as_bytes())], 64);
    let id = layerfs_sandbox::ContainerId::parse(CID).unwrap();
    let Err(failure) = layerfs_sandbox::backend::docker::Sandbox::reopen(docker, id, request())
    else {
        panic!("reopen of a container that does not run fails");
    };
    peer.finish();
    failure
}

#[test]
fn only_the_exact_intact_container_not_running_is_a_stopped_container() {
    let stopped = reopened_from(&exited());
    assert!(stopped.container_stopped());
    assert!(!stopped.container_gone());
    let other = "c".repeat(64);
    assert!(!reopened_from(&exited().replace(CID, &other)).container_stopped());
    let dead = exited().replace(r#""Dead":false"#, r#""Dead":true"#);
    assert!(!reopened_from(&dead).container_stopped());
    let image = exited().replace(IMAGE, &format!("sha256:{}", "d".repeat(64)));
    assert!(!reopened_from(&image).container_stopped());
}

#[test]
fn a_stopped_container_is_deleted_once_and_never_forced() {
    let id = layerfs_sandbox::ContainerId::parse(CID).unwrap();
    let (docker, peer) = Peer::new(vec![fixed(204, b"")], 64);
    let mut sandbox = layerfs_sandbox::backend::docker::Sandbox::stopped(docker, id, request());
    assert!(
        sandbox.endpoint().is_err(),
        "a stopped Sandbox has no endpoint"
    );
    sandbox.delete().unwrap();
    assert!(sandbox.disposition().deleted);
    let calls = peer.finish();
    assert_eq!(calls.len(), 1);
    assert!(String::from_utf8_lossy(&calls[0]).starts_with(&format!(
        "DELETE /v1.54/containers/{CID}?force=false&v=false"
    )));
    // running again meanwhile: the Engine refuses the unforced Delete
    let running = br#"{"message":"cannot remove container: container is running"}"#;
    let (docker, peer) = Peer::new(vec![fixed(409, running)], 64);
    let mut sandbox = layerfs_sandbox::backend::docker::Sandbox::stopped(docker, id, request());
    assert!(sandbox.delete().is_err());
    assert!(!sandbox.disposition().deleted);
    peer.finish();
}
