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
            serial_low_water: 0,
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
/// The explicit serial low-water is one `u64` after the other limits, and the
/// record magic advanced with it: a record of the previous layout is refused,
/// never read with shifted fields.
#[test]
fn serial_low_water_round_trips_and_the_previous_record_is_refused() {
    const LOW: u64 = 0x0102_0304_0506_0708;
    let mut value = setup();
    assert_eq!(value.limits.serial_low_water, 0);
    let zero = value.encode().unwrap();
    assert_eq!(DaemonSetup::decode(&zero).unwrap(), value);
    value.limits.serial_low_water = LOW;
    let bytes = value.encode().unwrap();
    let decoded = DaemonSetup::decode(&bytes).unwrap();
    assert_eq!(decoded.limits.serial_low_water, LOW);
    assert_eq!(decoded, value);

    // Exact placement: the last limit, before the existing-Store flag.
    assert_eq!(&bytes[..5], b"LFSD\x02");
    let at = bytes.len() - 9;
    assert_eq!(bytes[at..at + 8], LOW.to_be_bytes());
    assert_eq!(bytes[at - 4..at], value.limits.pager_kib.to_be_bytes());
    assert_eq!(bytes[at + 8], 0, "no existing Store");
    assert_eq!(zero.len(), bytes.len());
    assert_eq!(zero[at..at + 8], [0; 8]);
    assert_eq!((&zero[..at], zero[at + 8]), (&bytes[..at], bytes[at + 8]));

    // The previous record: magic 1 and no low-water field.
    let mut previous = bytes.clone();
    previous.drain(at..at + 8);
    previous[4] = 1;
    assert_eq!(&previous[..5], b"LFSD\x01");
    assert!(DaemonSetup::decode(&previous).is_err());
    // Neither half alone is accepted: the previous magic over the current
    // layout, or the current magic over the previous layout.
    let mut old_magic = bytes.clone();
    old_magic[4] = 1;
    assert!(DaemonSetup::decode(&old_magic).is_err());
    previous[4] = 2;
    assert!(DaemonSetup::decode(&previous).is_err());
    for magic in [0, 3, 0xff] {
        let mut other = bytes.clone();
        other[4] = magic;
        assert!(DaemonSetup::decode(&other).is_err(), "magic {magic}");
    }
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
