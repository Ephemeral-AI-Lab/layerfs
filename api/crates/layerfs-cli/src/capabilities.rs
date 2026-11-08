//! Machine-readable CLI discovery; these commands never open a home.
use crate::output;
use layerfs_api::{json, Kind};
use layerfs_bridge::control::SdkValue;
use std::process::ExitCode;

fn strings(values: impl IntoIterator<Item = impl AsRef<str>>) -> SdkValue {
    SdkValue::List(
        values
            .into_iter()
            .map(|s| SdkValue::text(s.as_ref()))
            .collect(),
    )
}

pub fn version() -> ExitCode {
    let features = [
        ("machine", cfg!(feature = "machine")),
        ("postgresql", cfg!(feature = "postgresql")),
        ("tls", cfg!(feature = "tls")),
    ];
    output::result(
        &SdkValue::map([
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("commit", SdkValue::text(env!("LAYERFS_BUILD_COMMIT"))),
            (
                "dirty",
                SdkValue::Bool(env!("LAYERFS_BUILD_DIRTY") == "true"),
            ),
            (
                "features",
                strings(
                    features
                        .into_iter()
                        .filter(|(_, on)| *on)
                        .map(|(name, _)| name),
                ),
            ),
        ]),
        false,
    )
}

pub fn help() -> ExitCode {
    let calls = layerfs_api::calls();
    let all = || calls.iter().map(|c| c.name()).collect::<Vec<_>>();
    let selected = |predicate: fn(&layerfs_api::Call) -> bool| {
        calls
            .iter()
            .filter(|c| predicate(c))
            .map(|c| c.name())
            .collect::<Vec<_>>()
    };
    let switch = |name: &str, applies: Vec<String>| {
        SdkValue::map([("name", SdkValue::text(name)), ("calls", strings(applies))])
    };
    let mut switches = vec![
        switch("--text", all()),
        switch("-h", all()),
        switch("--help", all()),
        switch("--", all()),
        switch("--progress", selected(|c| c.arg("on_progress").is_some())),
        switch("--stream", selected(|c| c.arg("stream").is_some())),
        switch("--events", selected(|c| c.arg("stdout").is_some())),
        switch("--scope", all()),
        switch("--scope-project", all()),
        switch("--scope-branch", all()),
        switch(
            "--from",
            selected(|c| {
                c.args
                    .iter()
                    .any(|a| matches!(a.kind, Kind::Content | Kind::Json))
            }),
        ),
        switch(
            "-m",
            selected(|c| c.options.iter().any(|a| a.name == "message")),
        ),
    ];
    let mut from = std::collections::BTreeMap::<String, Vec<String>>::new();
    for call in &calls {
        for arg in call.args.iter().chain(call.options) {
            if arg.optional && arg.kind == Kind::Text {
                from.entry(format!("--{}-from", arg.name.replace('_', "-")))
                    .or_default()
                    .push(call.name());
            }
        }
    }
    for (name, calls) in from {
        switches.push(switch(&name, calls));
    }
    let command = |name, args: &[&str], options: &[&str]| {
        SdkValue::map([
            ("name", SdkValue::text(name)),
            ("args", strings(args)),
            ("options", strings(options)),
        ])
    };
    let commands = vec![
        command("doctor", &[], &["--text"]),
        command("doctor host", &[], &["--timeout", "--text"]),
        command("doctor machine", &["machine"], &["--timeout", "--text"]),
        command("doctor sandbox", &["sandbox"], &["--timeout", "--text"]),
        command(
            "doctor store integrity",
            &["store"],
            &["--quick", "--timeout", "--text"],
        ),
        command(
            "doctor store connection",
            &["store"],
            &["--timeout", "--text"],
        ),
        command(
            "doctor backup connection",
            &["bucket"],
            &["--write-test", "--timeout", "--text"],
        ),
        command("import-env", &[], &[]),
        command("worker", &[], &[]),
        command("help", &["[family]", "[call]"], &["--json"]),
        command("--version", &[], &["--json"]),
    ];
    let table = json::parse(&layerfs_api::table_json()).expect("the API table is valid JSON");
    output::result(
        &SdkValue::map([
            ("calls", table),
            ("commands", SdkValue::List(commands)),
            ("switches", SdkValue::List(switches)),
        ]),
        false,
    )
}
