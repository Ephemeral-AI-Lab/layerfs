//! Log suffix semantics through the public Sandbox API and a bounded Engine peer.
#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/engine.rs"]
mod peer;
use layerfs_sandbox::{
    backend::docker::{Sandbox, SandboxRequest},
    ContainerId, RuntimeError,
};
use peer::{frame, Peer};

fn read_tail(wire: Vec<u8>, cap: usize, split: usize) -> Vec<u8> {
    let (docker, server) = Peer::new(vec![wire], split);
    let sandbox = Sandbox::stopped(
        docker,
        ContainerId::parse(&"b".repeat(64)).unwrap(),
        SandboxRequest {
            image: format!("sha256:{}", "a".repeat(64)),
            store_volume: "log-tail-store".into(),
            port: 30421,
            labels: vec![],
        },
    );
    let before = sandbox.disposition();
    let tail = sandbox.log_tail(20, cap).unwrap();
    assert_eq!(sandbox.disposition(), before);
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(request.contains("/logs?stdout=true&stderr=true&follow=false&tail=20"));
    tail
}

fn logs(frames: &[u8]) -> Vec<u8> {
    let mut wire = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\n\r\n",
        frames.len()
    ).into_bytes();
    wire.extend_from_slice(frames);
    wire
}

#[test]
fn mixed_streams_keep_exact_suffix_across_windows_and_repeated_wraps() {
    let mut frames = Vec::new();
    let mut all = Vec::new();
    for (i, length) in [0, 3, 8191, 8192, 8193, 65537, 137, 65536]
        .into_iter()
        .enumerate()
    {
        let bytes: Vec<u8> = (0..length).map(|n| ((n + i * 17) % 251) as u8).collect();
        frames.extend(frame(if i % 2 == 0 { 1 } else { 2 }, &bytes));
        all.extend(bytes);
    }
    let wire = logs(&frames);
    for cap in [
        0,
        1,
        7,
        2048,
        8191,
        8192,
        8193,
        65536,
        all.len(),
        all.len() + 1,
    ] {
        let tail = read_tail(wire.clone(), cap, 8192);
        assert_eq!(tail, all[all.len().saturating_sub(cap)..], "cap={cap}");
    }
}

#[test]
fn fragmented_frames_preserve_binary_and_utf8_bytes() {
    let mut bytes = "中文日志\n".as_bytes().to_vec();
    bytes.extend(0..=255);
    let mut frames = frame(1, &bytes[..5]);
    frames.extend(frame(2, &bytes[5..]));
    for cap in [0, 1, 7, 17, bytes.len()] {
        let tail = read_tail(logs(&frames), cap, 1);
        assert_eq!(tail, bytes[bytes.len().saturating_sub(cap)..]);
    }
}

#[test]
fn zero_capacity_still_validates_the_entire_response() {
    let container = ContainerId::parse(&"b".repeat(64)).unwrap();
    let mut invalid_tag = frame(1, b"valid prefix");
    invalid_tag.extend(frame(3, b"invalid stream"));
    let mut incomplete = frame(1, b"truncated");
    incomplete.pop();
    for frames in [invalid_tag, incomplete] {
        let (docker, server) = Peer::new(vec![logs(&frames)], 7);
        let sandbox = Sandbox::stopped(
            docker,
            container,
            SandboxRequest {
                image: format!("sha256:{}", "a".repeat(64)),
                store_volume: "log-tail-store".into(),
                port: 30421,
                labels: vec![],
            },
        );
        let failure = sandbox.log_tail(20, 0).unwrap_err();
        assert_eq!(failure.status, Some(200));
        assert_eq!(failure.requested_container, Some(container));
        assert!(matches!(
            failure.cause,
            RuntimeError::Protocol(_) | RuntimeError::Io(_)
        ));
        assert_eq!(server.finish().len(), 1);
    }
}
