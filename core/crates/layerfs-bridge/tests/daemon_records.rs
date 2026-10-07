//! Additive startup/session records and private deployment configuration.
use layerfs_bridge::{
    control::{Answer, Call, DaemonPhase, DaemonStatus, HelloRequest, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    initial_record::InitialRecord,
};
fn setup() -> DaemonSetup {
    DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: [17; 32],
        control_peer: [18; 32],
        store: "/shared/private/store.sqlite".into(),
        overlay: "/local/private/overlay.sqlite".into(),
        mounts: "/mounts".into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 2,
            read_handles: 2,
            handshake_ms: 3000,
            cache_bytes: 4096,
            owner_bytes: 16 * 1024 * 1024,
            lifecycle_reserve: 1024 * 1024,
            namespaces: 2,
            ordinary_jobs: 2,
            lifecycle_jobs: 2,
            pager_kib: 1024,
        },
        existing_store: None,
    }
}
#[test]
fn private_setup_is_exact_bounded_and_redacts_secret() {
    let value = setup();
    let bytes = value.encode().unwrap();
    assert_eq!(DaemonSetup::decode(&bytes).unwrap(), value);
    for n in 0..bytes.len() {
        assert!(DaemonSetup::decode(&bytes[..n]).is_err());
    }
    let mut extra = bytes;
    extra.push(0);
    assert!(DaemonSetup::decode(&extra).is_err());
    let diagnostic = format!("{value:?}");
    assert!(diagnostic.contains("[redacted]"));
    assert!(!diagnostic.contains("17, 17"));
    let mut bad = value.clone();
    bad.limits.handshake_ms = 0;
    assert!(bad.encode().is_err());
    bad = value.clone();
    bad.command_uid = 0;
    assert!(bad.encode().is_err());
    bad = value;
    bad.store = "/shared/../store".into();
    assert!(bad.encode().is_err());
}
#[test]
fn startup_and_session_end_preserve_correlation_and_require_actual_ready_facts() {
    for request in [
        Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }),
        Request::Hello(HelloRequest {
            expected_instance: Some([3; 32]),
            wait_for_store: true,
        }),
        Request::EndSession,
    ] {
        let call = Call { id: 42, request };
        let bytes = call.encode().unwrap();
        assert_eq!(Call::decode(&bytes).unwrap(), call);
        assert!(
            matches!(InitialRecord::decode(&bytes).unwrap(), InitialRecord::Control(value) if value == call)
        );
        for n in 0..bytes.len() {
            assert!(InitialRecord::decode(&bytes[..n]).is_err());
        }
    }
    let status = DaemonStatus {
        instance: [3; 32],
        phase: DaemonPhase::ControlReady,
        overlay_sqlite: Some("3.51.0".into()),
        store_sqlite: Some("3.51.0".into()),
    };
    let answer = Answer {
        id: 42,
        reply: Reply::Hello(status.clone()),
    };
    let bytes = answer.encode().unwrap();
    assert_eq!(Answer::decode(&bytes).unwrap(), answer);
    for n in 0..bytes.len() {
        assert!(Answer::decode(&bytes[..n]).is_err());
    }
    let mut missing = status;
    missing.store_sqlite = None;
    assert!(Answer {
        id: 42,
        reply: Reply::Hello(missing)
    }
    .encode()
    .is_err());
    let ended = Answer {
        id: 42,
        reply: Reply::SessionEnded,
    };
    assert_eq!(Answer::decode(&ended.encode().unwrap()).unwrap(), ended);
    assert!(InitialRecord::decode(b"LFSC\x02").is_err());
}
