//! Discovery works without initializing a home and reflects the API table.
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;

#[test]
fn version_identifies_source_and_features_without_a_home() {
    let home = tempdir("identity").join("missing");
    let plain = run(&mut layerfs(&home, &["--version"]));
    assert!(plain.status.success());
    assert_eq!(
        text(&plain.stdout),
        format!("layerfs {}\n", env!("CARGO_PKG_VERSION"))
    );
    let out = run(&mut layerfs(&home, &["--version", "--json"]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let value = json::parse(&text(&out.stdout)).unwrap();
    assert_eq!(value.get_text("version"), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(value.get_text("commit"), Some(env!("LAYERFS_BUILD_COMMIT")));
    assert_eq!(value.get_text("commit").unwrap().len(), 40);
    assert_eq!(
        value.get("dirty"),
        Some(&SdkValue::Bool(env!("LAYERFS_BUILD_DIRTY") == "true"))
    );
    let Some(SdkValue::List(features)) = value.get("features") else {
        panic!("features list");
    };
    assert_eq!(
        features.contains(&SdkValue::text("postgresql")),
        cfg!(feature = "postgresql")
    );
    assert_eq!(
        features.contains(&SdkValue::text("machine")),
        cfg!(feature = "machine")
    );
    assert_eq!(
        features.contains(&SdkValue::text("sandbox-runtime")),
        layerfs_bridge::sandbox_runtime::LOADER.is_some()
    );
    assert!(!home.exists());
}

#[test]
fn capabilities_include_the_table_and_cli_only_commands() {
    let home = tempdir("capabilities").join("missing");
    let out = run(&mut layerfs(&home, &["help", "--json"]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let value = json::parse(&text(&out.stdout)).unwrap();
    assert_eq!(
        value.get("calls"),
        Some(&json::parse(&layerfs_api::table_json()).unwrap())
    );
    let Some(SdkValue::List(commands)) = value.get("commands") else {
        panic!("commands list");
    };
    for (name, option) in [
        ("doctor store integrity", "--quick"),
        ("doctor backup connection", "--write-test"),
        ("doctor host", "--timeout"),
    ] {
        let command = commands
            .iter()
            .find(|c| c.get_text("name") == Some(name))
            .unwrap();
        let Some(SdkValue::List(options)) = command.get("options") else {
            panic!("options list");
        };
        assert!(options.contains(&SdkValue::text(option)));
    }
    for name in ["worker", "import-env"] {
        assert!(commands.iter().any(|c| c.get_text("name") == Some(name)));
    }
    let Some(SdkValue::List(switches)) = value.get("switches") else {
        panic!("switches list");
    };
    let stream = switches
        .iter()
        .find(|c| c.get_text("name") == Some("--stream"))
        .unwrap();
    assert_eq!(
        stream.get("calls"),
        Some(&SdkValue::List(vec![SdkValue::text("workspace.exec")]))
    );
    for flag in ["-h", "--help", "--", "--message-from"] {
        assert!(
            switches.iter().any(|s| s.get_text("name") == Some(flag)),
            "missing {flag}"
        );
    }
    assert!(!home.exists());
}

#[test]
fn invalid_discovery_options_are_usage_errors() {
    let home = tempdir("capabilities-usage").join("missing");
    for args in [["--version", "--bogus"], ["help", "--bogus"]] {
        let out = run(&mut layerfs(&home, &args));
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(text(&out.stderr).starts_with("layerfs: "));
    }
    assert!(!home.exists());
}

#[test]
fn explicit_force_options_are_discoverable_alongside_existing_integration_calls() {
    let home = tempdir("force-capabilities").join("missing");
    let out = run(&mut layerfs(&home, &["help", "--json"]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let value = json::parse(&text(&out.stdout)).unwrap();
    let Some(SdkValue::List(calls)) = value.get("calls") else {
        panic!("calls list");
    };
    for (family, name) in [("machine", "stop"), ("sandbox", "close")] {
        let call = calls
            .iter()
            .find(|call| {
                call.get_text("family") == Some(family) && call.get_text("call") == Some(name)
            })
            .unwrap();
        let Some(SdkValue::List(options)) = call.get("options") else {
            panic!("call options");
        };
        let force = options
            .iter()
            .find(|option| option.get_text("name") == Some("force"))
            .unwrap();
        assert_eq!(force.get_text("kind"), Some("bool"));
        assert_eq!(force.get("optional"), Some(&SdkValue::Bool(true)));
    }
    assert!(calls
        .iter()
        .any(|call| call.get_text("family") == Some("project")
            && call.get_text("call") == Some("export")));
    assert!(!home.exists());
}
