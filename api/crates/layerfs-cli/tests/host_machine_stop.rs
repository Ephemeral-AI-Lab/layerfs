#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use fixture::Fixture;
use layerfs_home::names::HOST_MACHINE;
use layerfs_protocol::provision::install::SdkValue;
use proof::Cli;

fn cli(home: &Fixture, args: &[&str]) -> Cli {
    Cli::spawn(&home.paths.home, args)
}

fn live(home: &Fixture) -> SdkValue {
    cli(
        home,
        &["machine", "get", HOST_MACHINE, "--with", "live", "--detail"],
    )
    .ok()
    .get("live")
    .unwrap()
    .clone()
}

fn state(home: &Fixture, machine: &str, worker: &str) {
    let live = live(home);
    assert_eq!(live.get_text("machine_state"), Some(machine), "{live:?}");
    assert_eq!(
        live.get("worker").unwrap().get_text("health"),
        Some(worker),
        "{live:?}"
    );
}

#[test]
fn stop_fences_warm_sdk_and_new_cli_calls_until_explicit_start() {
    // SDK power management discovers its matching installation through PATH.
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_layerfs"))
        .parent()
        .unwrap();
    let previous = std::env::var_os("PATH").unwrap_or_default();
    let search = std::iter::once(bin.to_owned()).chain(std::env::split_paths(&previous));
    std::env::set_var("PATH", std::env::join_paths(search).unwrap());
    let home = proof::create();
    let start = ["machine", "start", HOST_MACHINE];
    let stop = ["machine", "stop", HOST_MACHINE];
    let read = ["file", "read", "shop/api", "main", "/hello"];
    state(&home, "enabled", "absent");
    cli(&home, &start).ok();
    state(&home, "enabled", "ready");
    let gen = home.ping().get("generation").unwrap().clone();
    cli(&home, &start).ok();
    assert_eq!(home.ping().get("generation"), Some(&gen));
    let expected = cli(&home, &read).ok();
    let history = proof::commits(&home);
    let sdk_read = || {
        layerfs_api::call(
            Some(&home.paths.home),
            "file.read",
            SdkValue::map([
                ("project", SdkValue::text("shop/api")),
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/hello")),
            ]),
            &mut |_| {},
        )
    };
    assert_eq!(sdk_read().unwrap(), expected);
    let pid = std::process::id();
    assert!(proof::lease_file(&home, i64::from(pid)).exists());

    // another process's warm lease (this SDK's) holds the stopped host
    let refused = cli(&home, &stop).refused("InUse");
    let msg = refused.get_text("message").unwrap();
    assert!(
        msg.contains("shop") && msg.contains(&pid.to_string()),
        "{refused:?}"
    );
    proof::wait_for("drained worker", || {
        (!home.paths.socket.exists()).then_some(())
    });
    state(&home, "stopped", "absent");
    cli(&home, &["machine", "doctor", HOST_MACHINE]).ok();
    assert_eq!(sdk_read().unwrap_err().code, "Unavailable");
    cli(&home, &read).refused("Unavailable");
    cli(&home, &["file", "mkdir", "shop/api", "main", "/after-stop"]).refused("Unavailable");
    cli(&home, &["machine", "stop", HOST_MACHINE, "--force"]).refused("InUse");
    assert!(proof::lease_file(&home, i64::from(pid)).exists());
    let other = proof::create();
    proof::assert_reads(proof::spawn_reads(&other, 1, false));
    other.drain();

    // the SDK's own stop closes its own idle copy first: only other
    // processes and a call running here hold the host
    layerfs_api::call(
        Some(&home.paths.home),
        "machine.stop",
        SdkValue::map([("machine", SdkValue::text(HOST_MACHINE))]),
        &mut |_| {},
    )
    .expect("an SDK process's own idle Store lease does not hold its stop");
    assert!(!proof::lease_file(&home, i64::from(pid)).exists());
    let record = layerfs_api::home::paths::store_record(&home.paths.home, "shop");
    let before_compact = std::fs::read(&record).unwrap();
    cli(&home, &["store", "compact", "shop"]).refused("Unavailable");
    assert_eq!(
        std::fs::read(&record).unwrap(),
        before_compact,
        "refused compact changed the Store record"
    );
    assert!(std::fs::read_dir(layerfs_api::home::paths::lease_dir(
        &home.paths.home,
        "shop"
    ))
    .unwrap()
    .next()
    .is_none());
    assert!(cli(&home, &stop).ok().get_bool("unchanged"));
    state(&home, "stopped", "absent");
    cli(&home, &start).ok();
    state(&home, "enabled", "ready");
    assert_ne!(home.ping().get("generation"), Some(&gen));
    assert_eq!(cli(&home, &read).ok(), expected);
    assert_eq!(proof::commits(&home), history);
    cli(&home, &stop).ok();
}

#[test]
fn force_reconciles_only_the_gone_owners_uncertain_stop() {
    use layerfs_api::home::{paths, read_record, with_field, write_record};
    let home = proof::create();
    let path = paths::machine_record(&home.paths.home, HOST_MACHINE);
    cli(&home, &["machine", "stop", HOST_MACHINE]).ok();
    let stopped = read_record(&path).unwrap().unwrap();

    for operation in ["machine.update", "machine.stop"] {
        let intent = SdkValue::map([
            ("id", SdkValue::text("op-1111222233334444")),
            ("name", SdkValue::text(operation)),
            ("phase", SdkValue::text("stop")),
            (
                "owner",
                SdkValue::map([("pid", SdkValue::Int(2_000_000_000))]),
            ),
            ("uncertain", SdkValue::Bool(true)),
        ]);
        let pending = with_field(stopped.clone(), "op", intent);
        write_record(&path, &pending).unwrap();
        cli(&home, &["machine", "start", HOST_MACHINE]).refused("Unresolved");
        cli(&home, &["machine", "stop", HOST_MACHINE]).refused("Unresolved");
        assert_eq!(read_record(&path).unwrap().unwrap(), pending);
        let mut force = cli(&home, &["machine", "stop", HOST_MACHINE, "--force"]);
        if operation == "machine.stop" {
            force.ok();
            let resolved = read_record(&path).unwrap().unwrap();
            assert_eq!(resolved.get_text("state"), Some("stopped"));
            assert!(resolved.get("op").is_none());
        } else {
            force.refused("Unresolved");
            assert_eq!(read_record(&path).unwrap().unwrap(), pending);
        }
    }
}
