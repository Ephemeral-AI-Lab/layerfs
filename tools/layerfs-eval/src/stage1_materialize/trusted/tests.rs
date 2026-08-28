use super::contract::{trusted_schedule_json, TRUSTED_SCHEDULE};
use crate::legacy_full::IntegrityMode;
use crate::stage1_materialize::attribution::equations::trust_equation;
use crate::stage1_materialize::row::contract::EngineDelta;

#[test]
fn trusted_schedule_and_counter_equation_are_explicit() {
    assert_eq!(TRUSTED_SCHEDULE, [0, 24, 96]);
    let schedule = trusted_schedule_json();
    assert!(schedule.contains("\"integrity_mode\":\"TrustedLocalDev\""));
    assert!(schedule.contains("\"warmups\":3"));
    assert!(schedule.contains("\"measured\":9"));

    let trusted = EngineDelta {
        fetched_rows: 7,
        role_decode_passes: 7,
        ..EngineDelta::default()
    };
    assert!(trust_equation(IntegrityMode::TrustedLocalDev, &trusted));
    assert!(!trust_equation(IntegrityMode::Verified, &trusted));
    let verified = EngineDelta {
        authentication_passes: 7,
        identity_authentication_ns: 1,
        ..trusted
    };
    assert!(trust_equation(IntegrityMode::Verified, &verified));
    assert!(!trust_equation(IntegrityMode::TrustedLocalDev, &verified));
}
