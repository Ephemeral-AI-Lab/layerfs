//! The control-capacity read model (#540): the typed DTO round-trips the
//! daemon's answer, keeps a missing field legible, and drops a malformed
//! retained record instead of inventing one.
use layerfs_bridge::control::{AdmissionModel, CapacityModel, RetainedFailureModel, RetainedModel};

fn failure(slot: i64) -> RetainedFailureModel {
    RetainedFailureModel {
        slot,
        class: "pool".into(),
        generation: 4,
        owner: "a1b2c3d4:812:layerfs-control-0".into(),
        received_bytes: Some(24),
        request: Some("control".into()),
        reply_sent: false,
        cause: "kernel request".into(),
        fence_error: None,
        diagnostic_error: Some("closed".into()),
        age_ms: 12,
        expires_in_ms: 3_600_000,
    }
}

#[test]
fn the_typed_model_round_trips_its_wire_record() {
    let model = CapacityModel {
        admission: AdmissionModel {
            pool_vacant: 2,
            pool_running: 6,
            reserved_vacant: 1,
            reserved_running: 0,
        },
        retained: RetainedModel {
            limit: 64,
            ttl_ms: 3_600_000,
            count: 1,
            dropped: 2,
            recorded: 3,
            failures: vec![failure(3)],
        },
    };
    let wire = model.to_sdk();
    assert_eq!(CapacityModel::parse(&wire), model);
    assert_eq!(
        wire.get("retained").and_then(|v| v.get_int("count")),
        Some(1)
    );
}

#[test]
fn a_missing_or_malformed_record_reads_as_absent_not_invented() {
    let value = layerfs_bridge::control::SdkValue::map([
        ("admission", layerfs_bridge::control::SdkValue::Null),
        (
            "retained",
            layerfs_bridge::control::SdkValue::map([
                ("limit", layerfs_bridge::control::SdkValue::Int(64)),
                (
                    "failures",
                    layerfs_bridge::control::SdkValue::List(vec![
                        layerfs_bridge::control::SdkValue::map([(
                            "slot",
                            layerfs_bridge::control::SdkValue::Int(1),
                        )]),
                    ]),
                ),
            ]),
        ),
    ]);
    let model = CapacityModel::parse(&value);
    assert_eq!(model.admission, AdmissionModel::default());
    assert_eq!(model.retained.limit, 64);
    assert_eq!(model.retained.count, 0);
    assert!(model.retained.failures.is_empty());
}
