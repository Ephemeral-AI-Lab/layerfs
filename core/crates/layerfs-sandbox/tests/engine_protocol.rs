//! Structural selected metadata and standard stream ownership through public Docker APIs.
#![cfg(unix)]
#[path = "support/engine.rs"]
mod peer;
use layerfs_sandbox::{
    backend::docker::{Docker, ExecRequest},
    CommandIdentity, ContainerId, ExecId, RuntimeError,
};
use std::io::{self, Write};
fn container() -> ContainerId {
    ContainerId::parse(&"b".repeat(64)).unwrap()
}
fn exec() -> ExecId {
    ExecId::parse(&"a".repeat(64)).unwrap()
}
fn request() -> ExecRequest {
    ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec!["/bin/bash".into(), "-c".into(), "printf ordinary".into()],
        environment: vec!["X=a\"b\\c\n".into()],
        directory: "/tmp".into(),
        stdin: false,
    }
}
fn created() -> Vec<u8> {
    peer::fixed(201, format!("{{\"Id\":\"{}\"}}", exec()).as_bytes())
}
fn inspect(extra: &str) -> String {
    format!("{{\"ID\":\"{}\",\"ContainerID\":\"{}\",\"Running\":false,\"ExitCode\":null,\"Pid\":0{extra}}}",exec(),container())
}
#[test]
fn structural_inspection_ignores_giant_arguments_and_retains_null_exit() {
    let nested = format!(
        ",\"ProcessConfig\":{{\"Arguments\":[\"{}\"],\"Running\":true,\"ExitCode\":0}}",
        "x".repeat(1024 * 1024)
    );
    let body = inspect(&nested);
    let (docker, server) = peer::Peer::new(vec![peer::fixed(200, body.as_bytes())], 8192);
    let value = docker.inspect_exec(container(), exec()).unwrap();
    assert!(!value.running);
    assert_eq!(value.exit_code, None);
    assert_eq!(value.known_root_exit(), None);
    assert_eq!(value.pid, Some(0));
    server.finish();
}
#[test]
fn escaped_deciding_duplicates_wrong_identity_and_truncated_json_refuse() {
    let cases = [
        inspect(",\"\\u0052unning\":true"),
        inspect(",\"ExitCode\":0"),
        inspect("").replace(&"a".repeat(64), &"c".repeat(64)),
        inspect("").trim_end_matches('}').into(),
        inspect("").replace("null", "2147483648"),
        inspect("").replace("null", "1e0"),
        inspect(",\"other\":\"\\uD800\""),
    ];
    for body in cases {
        let (docker, server) = peer::Peer::new(vec![peer::fixed(200, body.as_bytes())], 1);
        let failure = docker.inspect_exec(container(), exec()).unwrap_err();
        assert!(failure.attempted);
        assert_eq!(failure.status, Some(200));
        assert_eq!(failure.requested_exec, Some(exec()));
        assert!(failure.observed_exec.is_some());
        server.finish();
    }
}
#[test]
fn chunked_metadata_accepts_split_unicode_and_rejects_conflicting_framing() {
    let body = inspect(",\"ignored\":\"\\uD83D\\uDE00中文\"");
    let mut chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    for chunk in body.as_bytes().chunks(3) {
        chunked.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        chunked.extend_from_slice(chunk);
        chunked.extend_from_slice(b"\r\n");
    }
    chunked.extend_from_slice(b"0\r\nX-End: yes\r\n\r\n");
    let (docker, server) = peer::Peer::new(vec![chunked], 1);
    assert_eq!(
        docker.inspect_exec(container(), exec()).unwrap().exit_code,
        None
    );
    server.finish();
    let invalid =
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}".to_vec();
    let (docker, server) = peer::Peer::new(vec![invalid], 8192);
    let failure = docker.inspect_exec(container(), exec()).unwrap_err();
    assert_eq!(failure.status, Some(200));
    assert!(matches!(failure.cause, RuntimeError::Protocol(_)));
    server.finish();
}
#[test]
fn upgrade_read_ahead_preserves_frames_and_request_identity() {
    let mut frames = peer::frame(1, b"ordinary");
    frames.extend_from_slice(&peer::frame(2, b"error"));
    let (docker, server) = peer::Peer::new(vec![created(), peer::upgrade(&frames)], 8192);
    let owner = docker.create_exec(container(), request()).unwrap();
    assert_eq!(owner.identity(), (container(), exec()));
    let (input, output, handle) = owner.start().unwrap().into_parts().unwrap();
    drop(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let done = output.copy_to(&mut stdout, &mut stderr).unwrap();
    assert_eq!(stdout, b"ordinary");
    assert_eq!(stderr, b"error");
    assert_eq!(done.stdout, 8);
    assert_eq!(done.stderr, 5);
    assert!(matches!(handle.cancel(), Err(RuntimeError::Unsupported(_))));
    let requests = server.finish();
    let first = String::from_utf8(requests[0].clone()).unwrap();
    assert!(first.contains("\"User\":\"501:20\""));
    assert!(first.contains("\"Privileged\":false"));
    assert!(first.contains("a\\\"b\\\\c\\n"));
}
struct FailedSink {
    written: usize,
}
impl Write for FailedSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.written == 2 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = (2 - self.written).min(bytes.len());
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn positive_delivery_and_maximal_truncated_frame_keep_original_pending_bytes() {
    let (docker, server) = peer::Peer::new(
        vec![created(), peer::upgrade(&peer::frame(1, b"hello"))],
        8192,
    );
    let (_, output, _) = docker
        .create_exec(container(), request())
        .unwrap()
        .start()
        .unwrap()
        .into_parts()
        .unwrap();
    let mut sink = FailedSink { written: 0 };
    let failure = output.copy_to(&mut sink, &mut io::sink()).unwrap_err();
    assert_eq!(failure.output.pending_payload(), b"llo");
    assert_eq!(failure.output.framing_bytes().len(), 8);
    assert_eq!(sink.written, 2);
    assert_eq!(failure.output.progress().stdout, 2);
    assert_eq!(failure.output.progress().wire_payload, 5);
    server.finish();
    let mut partial = vec![1, 0, 0, 0];
    partial.extend_from_slice(&u32::MAX.to_be_bytes());
    partial.extend_from_slice(b"abc");
    let (docker, server) = peer::Peer::new(vec![created(), peer::upgrade(&partial)], 8192);
    let (_, output, _) = docker
        .create_exec(container(), request())
        .unwrap()
        .start()
        .unwrap()
        .into_parts()
        .unwrap();
    let failure = output
        .copy_to(&mut io::sink(), &mut io::sink())
        .unwrap_err();
    assert_eq!(failure.output.pending_payload(), b"abc");
    assert!(matches!(failure.cause, RuntimeError::Io(_)));
    server.finish();
}
#[test]
fn partial_creation_id_and_lost_start_are_retained_without_replay() {
    let broken = format!("{{\"Id\":\"{}\",", exec());
    let (docker, server) = peer::Peer::new(vec![peer::fixed(201, broken.as_bytes())], 8192);
    let failure = docker.create_exec(container(), request()).unwrap_err();
    assert_eq!(failure.transfer.observed_exec, Some(exec()));
    server.finish();
    let (docker, server) = peer::Peer::new(vec![created(), Vec::new()], 8192);
    let failure = docker
        .create_exec(container(), request())
        .unwrap()
        .start()
        .unwrap_err();
    assert!(failure.transfer.attempted);
    assert_eq!(failure.transfer.requested_exec, Some(exec()));
    assert_eq!(failure.transfer.observed_exec, None);
    let again = failure.created.start().unwrap_err();
    assert!(!again.transfer.attempted);
    server.finish();
}
#[test]
fn before_effect_identity_refusal_has_no_engine_connection() {
    let docker = Docker::new(
        "/nonexistent-layerfs-engine.sock",
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let mut input = request();
    input.identity.uid = 0;
    let failure = docker.create_exec(container(), input).unwrap_err();
    assert!(!failure.transfer.attempted);
    assert_eq!(failure.transfer.sent_bytes, 0);
    assert!(ContainerId::parse("b").is_err());
    assert!(ExecId::parse(&"0".repeat(64)).is_err());
}

#[test]
fn actual_mismatched_inspection_and_http_error_body_remain_distinct_from_selector() {
    let actual = ExecId::parse(&"c".repeat(64)).unwrap();
    let body = inspect("").replace(&exec().to_string(), &actual.to_string());
    let (docker, server) = peer::Peer::new(vec![peer::fixed(200, body.as_bytes())], 8192);
    let failed = docker.inspect_exec(container(), exec()).unwrap_err();
    assert_eq!(failed.requested_exec, Some(exec()));
    assert_eq!(failed.observed_exec, Some(actual));
    assert_eq!(failed.observed_container, Some(container()));
    server.finish();
    let body = b"{\"message\":\"original remote error\"}";
    let (docker, server) = peer::Peer::new(vec![peer::fixed(500, body)], 8192);
    let failed = docker.inspect_exec(container(), exec()).unwrap_err();
    assert_eq!(failed.observed_exec, None);
    assert_eq!(failed.requested_exec, Some(exec()));
    let diagnostic = failed.error_body.unwrap();
    assert!(diagnostic.complete);
    assert_eq!(diagnostic.prefix, body);
    server.finish();
}
#[test]
fn wrong_upgrade_media_and_malformed_status_trailers_extensions_refuse() {
    let valid = String::from_utf8(peer::upgrade(&[])).unwrap();
    for wrong in [
        valid.replace("application/vnd.docker.multiplexed-stream", "text/html"),
        valid.replace(
            "Content-Type: application/vnd.docker.multiplexed-stream\r\n",
            "",
        ),
        valid.replace(
            "Connection: Upgrade",
            "Content-Type: application/vnd.docker.multiplexed-stream\r\nConnection: Upgrade",
        ),
    ] {
        let (docker, server) = peer::Peer::new(vec![created(), wrong.into_bytes()], 8192);
        let failed = docker
            .create_exec(container(), request())
            .unwrap()
            .start()
            .unwrap_err();
        assert_eq!(failed.transfer.status, Some(101));
        assert!(matches!(failed.transfer.cause, RuntimeError::Protocol(_)));
        server.finish();
    }
    for tail in [
        b"0\r\n: invalid\r\n\r\n".as_slice(),
        b"0\r\nContent-Length : 1\r\n\r\n",
        b"0\r\nX-Test: \x00\r\n\r\n",
        b"0;\r\n\r\n",
    ] {
        let body = inspect("");
        let mut wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        wire.extend_from_slice(format!("{:x}\r\n", body.len()).as_bytes());
        wire.extend_from_slice(body.as_bytes());
        wire.extend_from_slice(b"\r\n");
        wire.extend_from_slice(tail);
        let (docker, server) = peer::Peer::new(vec![wire], 8192);
        assert!(docker.inspect_exec(container(), exec()).is_err());
        server.finish();
    }
    for wire in [
        b"HTTP/1.1 200\r\nContent-Length: 0\r\n\r\n".as_slice(),
        b"HTTP/1.1 200 bad\x00reason\r\nContent-Length: 0\r\n\r\n",
    ] {
        let (docker, server) = peer::Peer::new(vec![wire.to_vec()], 8192);
        assert!(docker.inspect_exec(container(), exec()).is_err());
        server.finish();
    }
}
