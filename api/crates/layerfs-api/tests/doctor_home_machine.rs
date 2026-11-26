//! `home doctor` and `machine doctor` (call surface 2.0) on temporary homes,
//! with a stand-in `docker` whose state a file sets and a stand-in worker,
//! so this host's own Docker is never asked: a clean home is `{"ok":true}`
//! alone; a stopped docker machine is checked by its record, operation,
//! configuration, provider and state, and blocks what is on it; a Docker
//! Engine that does not answer is `engine_down` with the fix that starts
//! Docker; an outdated worker with a Sandbox on its machine gives the forced
//! stop's fix and counts the Sandbox; a broken record is the first check's
//! error. One progress event goes out per finished thing.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, read_record, with_field, write_entity, write_record};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `docker version` answers unless the state is `down`; `docker inspect`
/// answers the state (no container when it is `missing`).
const STAND_IN: &str = r#"#!/bin/sh
state=$(cat "${0%/*}/state")
case "$*" in
  *version*) if [ "$state" = down ]; then echo 'Cannot connect to the Docker daemon' >&2; exit 1; fi; echo 29.0 ;;
  *inspect*) if [ "$state" = missing ]; then echo 'Error: No such object' >&2; exit 1; fi; printf '%s\tmachine-worker\n' "$state" ;;
  *) exit 71 ;;
esac
"#;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-dhm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// One call's answer (short, or long with `detail`) and its progress events.
fn call(home: &Path, name: &str, pairs: &[(&str, &str)], detail: bool) -> (SdkValue, usize) {
    let mut args: Vec<(String, SdkValue)> = (pairs.iter())
        .map(|(key, value)| ((*key).to_owned(), t(value)))
        .collect();
    if detail {
        args.push(("detail".to_owned(), SdkValue::Bool(true)));
    }
    let mut events = 0;
    let answer = layerfs_api::call(Some(home), name, SdkValue::Map(args), &mut |_| events += 1)
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message));
    (answer, events)
}

fn short(home: &Path, name: &str, pairs: &[(&str, &str)]) -> String {
    json::print(&call(home, name, pairs, false).0)
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// The ids of a node's check rows, in order.
fn checks(node: &SdkValue) -> Vec<&str> {
    (list(node, "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect()
}

/// A home with its native daemon recorded as an executable file.
fn initialized(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    call(&home, "home.init", &[], false);
    let daemon = home.join("layerfs-daemon");
    std::fs::write(&daemon, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = paths::machine_record(&home, layerfs_home::names::HOST_MACHINE);
    let native = read_record(&path).unwrap().unwrap_or_else(|| {
        SdkValue::map([
            ("name", t(layerfs_home::names::HOST_MACHINE)),
            ("platform", t("host")),
        ])
    });
    let native = with_field(native, "daemon", t(&daemon.display().to_string()));
    write_entity(&path, &native).unwrap();
    home
}

/// A sqlite Store `cache` with project `site`, and a Sandbox `sb` of it,
/// on the built-in machine `docker`.
fn on_docker(home: &Path) {
    write_entity(
        &paths::store_record(home, "cache"),
        &SdkValue::map([
            ("name", t("cache")),
            ("kind", t("sqlite")),
            ("machine", t("docker")),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::project_record(home, "cache", "site"),
        &SdkValue::map([("name", t("site"))]),
    )
    .unwrap();
    write_entity(
        &paths::sandbox_record(home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("project", t("cache/site")),
            ("machine", t("docker")),
            ("id", t("sb")),
        ]),
    )
    .unwrap();
}

/// A stand-in worker for `docker` that answers its hello as a worker of
/// another build (no protocol) and is sent nothing more.
fn old_worker(home: &Path) -> std::thread::JoinHandle<bool> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_record(
        &paths::worker_record(home, "docker"),
        &SdkValue::map([
            ("address", t(&listener.local_addr().unwrap().to_string())),
            ("secret", t("5e")),
        ]),
    )
    .unwrap();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut length = [0u8; 4];
        if stream.read_exact(&mut length).is_err() {
            return false;
        }
        let mut hello = vec![0; u32::from_be_bytes(length) as usize];
        let _ = stream.read_exact(&mut hello);
        let ready: &[u8] = br#"{"ready":true}"#;
        let _ = stream.write_all(&(ready.len() as u32).to_be_bytes());
        let _ = stream.write_all(ready);
        // no request follows a hello from another build
        let mut more = [0u8; 1];
        matches!(stream.read(&mut more), Ok(0))
    })
}

// One test owns PATH: every case asks the stand-in `docker`.
#[test]
fn home_and_machine_doctors_answer_from_their_frozen_checks() {
    let tools = tempdir("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let docker = tools.join("docker");
    std::fs::write(&docker, STAND_IN).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let state = |word: &str| std::fs::write(tools.join("state"), word).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![tools.clone()];
    dirs.extend(std::env::split_paths(&path));
    std::env::set_var("PATH", std::env::join_paths(dirs).unwrap());
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");

    // a clean home: docker never started (no container) is stopped
    state("missing");
    let home = initialized("clean");
    let (answer, events) = call(&home, "home.doctor", &[], false);
    let (long, _) = call(&home, "home.doctor", &[], true);
    // this build's own environment decides two of the home's checks
    let environment = |finding: &SdkValue| {
        matches!(
            finding.get_text("check"),
            Some("home.prerequisites" | "home.bundle")
        )
    };
    if cfg!(target_os = "macos") {
        assert_eq!(json::print(&answer), r#"{"ok":true}"#, "{long:?}");
    } else {
        assert!(list(&long, "findings").iter().all(environment), "{long:?}");
    }
    assert!(list(&long, "not_checked").is_empty(), "{long:?}");
    let tree = long.get("tree").unwrap();
    let order = [
        "home.build",
        "home.prerequisites",
        "home.layout",
        "home.records",
        "home.bundle",
        "home.host_daemon",
        "home.artifacts",
        "home.capacity",
    ];
    let ran = checks(tree);
    let at: Vec<usize> = (ran.iter())
        .map(|id| order.iter().position(|o| o == id).unwrap())
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{ran:?}");
    assert!(ran.contains(&"home.layout") && ran.contains(&"home.records"));
    let machines = list(tree, "machines");
    let names: Vec<&str> = (machines.iter())
        .filter_map(|m| m.get_text("machine"))
        .collect();
    assert_eq!(names, ["docker", layerfs_home::names::HOST_MACHINE]);
    assert_eq!(
        checks(&machines[0]),
        [
            "machine.record",
            "machine.operation",
            "machine.configuration",
            "machine.provider",
            "machine.state"
        ]
    );
    // the host machine adds its worker's endpoint: none is published here
    assert_eq!(
        checks(&machines[1]),
        [
            "machine.record",
            "machine.operation",
            "machine.state",
            "machine.worker.endpoint"
        ]
    );
    assert_eq!(events, 3, "the home, then each machine");

    // a stopped docker machine blocks what is on it without trying it
    on_docker(&home);
    let stopped = r#"{"machine":"docker","reason":"stopped"}"#;
    assert_eq!(
        short(&home, "machine.doctor", &[("machine", "docker")]),
        format!(
            r#"{{"not_checked":[{{"store":"cache","blocked_by":{stopped}}},{{"sandbox":"sb","blocked_by":{stopped}}}]}}"#
        )
    );
    let (long, events) = call(&home, "machine.doctor", &[("machine", "docker")], true);
    assert_eq!(events, 3, "the machine, its Store, its Sandbox");
    let tree = long.get("tree").unwrap();
    assert_eq!(tree.get_text("status"), Some("ok"), "{long:?}");
    assert_eq!(list(tree, "stores")[0].get_text("coverage"), Some("none"));
    assert_eq!(long.get_text("status"), Some("not_checked"));
    for (name, word, thing) in [
        ("store.doctor", "store", "cache"),
        ("project.doctor", "project", "cache/site"),
        ("sandbox.doctor", "sandbox", "sb"),
    ] {
        let answer = short(&home, name, &[(word, thing)]);
        assert!(answer.contains(stopped), "{name}: {answer}");
    }

    // Docker is not running: engine_down, and the fix starts it
    state("down");
    let (answer, _) = call(&home, "machine.doctor", &[("machine", "docker")], false);
    let found = &list(&answer, "findings")[0];
    assert_eq!(found.get_text("machine"), Some("docker"));
    assert_eq!(found.get_text("check"), Some("machine.provider"));
    assert_eq!(found.get_text("severity"), Some("error"));
    let start = match cfg!(target_os = "macos") {
        true => "open -a Docker",
        false => "sudo systemctl start docker",
    };
    assert_eq!(found.get_text("fix"), Some(start));
    let down = r#"{"machine":"docker","reason":"engine_down"}"#;
    assert!(json::print(&answer).contains(down), "{answer:?}");

    // an outdated worker with a Sandbox on its machine
    state("running");
    let record = paths::machine_record(&home, "docker");
    let docker = read_record(&record)
        .unwrap()
        .unwrap_or_else(|| SdkValue::map([("name", t("docker")), ("platform", t("docker"))]));
    write_entity(&record, &with_field(docker, "state", t("running"))).unwrap();
    let worker = old_worker(&home);
    let answer = short(&home, "machine.doctor", &[("machine", "docker")]);
    assert!(worker.join().unwrap(), "a request followed the hello");
    let outdated = r#"{"machine":"docker","reason":"outdated"}"#;
    assert_eq!(
        answer,
        format!(
            r#"{{"findings":[{{"machine":"docker","check":"machine.worker.compatibility","severity":"error","message":"worker is from another build; 1 Sandbox is on it","fix":"layerfs machine stop docker --force && layerfs machine start docker"}}],"not_checked":[{{"store":"cache","blocked_by":{outdated}}},{{"sandbox":"sb","blocked_by":{outdated}}}]}}"#
        )
    );

    // a broken record is its first check's error, with nothing to run
    write_entity(
        &paths::machine_record(&home, "vm"),
        &SdkValue::map([
            ("name", t("vm")),
            ("platform", t("lima")),
            ("cpus", SdkValue::Int(-1)),
        ]),
    )
    .unwrap();
    let (answer, _) = call(&home, "machine.doctor", &[("machine", "vm")], false);
    let found = &list(&answer, "findings")[0];
    assert_eq!(found.get_text("check"), Some("machine.record"));
    assert_eq!(found.get_text("severity"), Some("error"));
    assert!(found.get("fix").is_none(), "{answer:?}");

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(tools);
}
