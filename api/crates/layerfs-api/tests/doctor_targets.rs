//! Explicit doctor options stay outside normal home migration dispatch.
use layerfs_api::{doctor_duration, doctor_request, doctor_with_options, json, DoctorTarget};

#[test]
fn options_preserve_legacy_and_select_all_six_commands() {
    assert!(doctor_request(&json::parse("{}").unwrap())
        .unwrap()
        .is_none());
    for target in [
        "host",
        "machine",
        "sandbox",
        "store.integrity",
        "store.connection",
        "backup.connection",
    ] {
        let name = if target == "host" {
            ""
        } else {
            ",\"name\":\"example\""
        };
        let request =
            doctor_request(&json::parse(&format!("{{\"target\":\"{target}\"{name}}}")).unwrap())
                .unwrap()
                .unwrap();
        if target == "host" {
            assert_eq!(request.target, DoctorTarget::Host);
        }
        if target == "store.integrity" {
            assert!(request.timeout.is_none());
        } else {
            assert!(request.timeout.is_some());
        }
    }
}

#[test]
fn invalid_options_are_refused_without_initializing_the_home() {
    let home = std::env::temp_dir().join(format!("layerfs-doctor-options-{}", std::process::id()));
    assert!(!home.exists());
    for options in [
        r#"{"target":"machine"}"#,
        r#"{"target":"host","name":"example"}"#,
        r#"{"target":"host","quick":true}"#,
        r#"{"target":"store.connection","name":"s","write_test":true}"#,
        r#"{"target":"host","timeout":"0s"}"#,
        r#"{"target":"host","timeout":"18446744073709551615h"}"#,
        r#"{"target":"host","timeout":10}"#,
        r#"{"target":"sandbox","name":"../escape"}"#,
        r#"{"target":"host","surprise":true}"#,
    ] {
        let refusal = doctor_with_options(&home, &json::parse(options).unwrap()).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "{options}");
        assert!(!home.exists(), "invalid request made the home");
    }
}

#[test]
fn duration_is_explicit_and_checked() {
    assert_eq!(doctor_duration("500ms").unwrap().as_millis(), 500);
    assert_eq!(doctor_duration("2m").unwrap().as_secs(), 120);
    for invalid in [
        "0ms",
        "-1s",
        "1",
        "1.5s",
        "1day",
        " 1s",
        "18446744073709551615h",
    ] {
        assert!(doctor_duration(invalid).is_none(), "{invalid}");
    }
}
