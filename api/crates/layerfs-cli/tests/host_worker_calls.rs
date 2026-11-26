#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

mod common;
#[path = "common/host_worker.rs"]
mod fixture;

use common::{layerfs, run, text};
use fixture::Fixture;
use layerfs_api::{home::paths, json, worker};
use layerfs_bridge::control::SdkValue;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn prepare(home: &Path) {
    layerfs_api::call(Some(home), "home.init", SdkValue::map([]), &mut |_| {}).unwrap();
    let source = home.join("source");
    std::fs::create_dir_all(source.join("docs")).unwrap();
    std::fs::write(source.join("docs/a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(source.join("docs/b.txt"), "three\n").unwrap();
    layerfs_api::call(
        Some(home),
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    )
    .unwrap();
    layerfs_api::call(
        Some(home),
        "project.init",
        SdkValue::map([
            ("project", t("shop/api")),
            ("folder", t(source.to_str().unwrap())),
        ]),
        &mut |_| {},
    )
    .unwrap();
}

fn lines(file: &Path) -> Vec<SdkValue> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(|line| json::parse(line).unwrap())
        .collect()
}

fn read_cli(home: &Path, words: &[&str]) -> SdkValue {
    let mut command = layerfs(home, words);
    command
        .env("LAYERFS_LOGS", "1")
        .env("LAYERFS_LOGS_TAG", "native-route")
        .env("LAYERFS_TRACE", home.join("route.jsonl"))
        .env("LAYERFS_TRACE_STEP", "1");
    let out = run(&mut command);
    assert!(out.status.success(), "{}", text(&out.stderr));
    json::parse(&text(&out.stdout)).unwrap()
}

fn cleanup(home: &Fixture) {
    home.drain();
    let end = Instant::now() + Duration::from_secs(5);
    while home.paths.socket.exists() {
        assert!(Instant::now() < end, "worker did not drain");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn cold_cli_launches_and_warm_reads_match_the_direct_results() {
    let home = Fixture::new();
    let path = &home.paths.home;
    prepare(path);
    assert!(!home.paths.socket.exists());
    for (method, at) in [
        ("read", "/docs/a.txt"),
        ("stat", "/docs/a.txt"),
        ("list", "/docs"),
    ] {
        let direct = layerfs_api::call(
            Some(path),
            &format!("file.{method}"),
            SdkValue::map([
                ("project", t("shop/api")),
                ("branch", t("main")),
                ("path", t(at)),
            ]),
            &mut |_| {},
        )
        .unwrap();
        let routed = read_cli(path, &["file", method, "shop/api", "main", at]);
        assert_eq!(routed, direct, "{method}");
    }
    let ping = home.ping();
    assert_eq!(ping.get_int("stores"), Some(1));
    let pid = ping.get_int("pid").unwrap().to_string();
    assert!(paths::lease_dir(path, "shop").join(pid).is_file());

    let trace = lines(&path.join("route.jsonl"));
    let connections: Vec<_> = trace
        .iter()
        .filter(|line| line.get_text("event") == Some("worker.connection"))
        .collect();
    assert_eq!(connections.len(), 3);
    assert!(connections
        .iter()
        .all(|line| { line.get_text("op") == Some("store.call") && line.get_bool("sent") }));
    for (op, phase) in [
        ("file.read", "worker.store_open"),
        ("file.stat", "worker.call"),
        ("file.list", "worker.host_dispatch"),
    ] {
        assert!(trace
            .iter()
            .any(|line| line.get_text("op") == Some(op) && line.get_text("step") == Some(phase)));
    }
    assert!(!trace
        .iter()
        .any(|line| line.get_text("step") == Some("lease")));
    let audits = lines(&path.join("trace/calls.jsonl"));
    assert_eq!(
        audits
            .iter()
            .filter(|line| {
                line.get_text("client") == Some("cli")
                    && line.get_text("tag") == Some("native-route")
            })
            .count(),
        3
    );
    cleanup(&home);
}

#[test]
fn non_cli_scope_refusal_and_long_socket_paths_keep_the_direct_boundary() {
    let home = Fixture::new();
    prepare(&home.paths.home);
    let direct = layerfs_api::call_scoped(
        Some(&home.paths.home),
        None,
        "file.read",
        SdkValue::map([
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/docs/a.txt")),
        ]),
        &mut |_| {},
        layerfs_api::Client::Cli,
    )
    .unwrap();
    assert!(!home.paths.socket.exists());
    let denied = run(layerfs(
        &home.paths.home,
        &[
            "file",
            "read",
            "shop/api",
            "main",
            "/docs/b.txt",
            "--scope",
            "/docs/a.txt",
            "--scope-project",
            "shop/api",
        ],
    )
    .env("LAYERFS_TRACE", home.paths.home.join("denied.jsonl")));
    assert_eq!(denied.status.code(), Some(125));
    assert!(!home.paths.socket.exists());
    assert!(!lines(&home.paths.home.join("denied.jsonl"))
        .iter()
        .any(|line| line.get_text("event") == Some("worker.connection")));

    let long = home.paths.home.join("x".repeat(105));
    prepare(&long);
    let paths = worker::HostPaths::new(&long).unwrap();
    assert!(!paths.fits());
    let routed = read_cli(&long, &["file", "read", "shop/api", "main", "/docs/a.txt"]);
    assert_eq!(routed.get_text("text"), direct.get_text("text"));
    assert!(!paths.socket.exists());
    assert!(lines(&long.join("route.jsonl"))
        .iter()
        .any(|line| line.get_text("step") == Some("lease")));
}

#[test]
fn ordinary_endpoint_refuses_methods_and_arguments_outside_the_allowlist() {
    let home = Fixture::new();
    prepare(&home.paths.home);
    let mut worker = home.start();
    home.ping();
    for (method, key, expected) in [
        ("fs.write", "path", "Unsupported"),
        ("fs.stat", "storePath", "InvalidInput"),
        ("fs.list", "page", "InvalidInput"),
    ] {
        let stream = UnixStream::connect(&home.paths.socket).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        let hello = worker::read_host_frame(&stream, worker::MAX_HELLO, Some(end))
            .unwrap()
            .unwrap();
        let args = SdkValue::map([
            ("method", t(method)),
            ("store", t("shop")),
            ("project", t("api")),
            (
                "fields",
                SdkValue::Map(vec![
                    ("branch".to_owned(), t("main")),
                    (key.to_owned(), t("/docs/a.txt")),
                ]),
            ),
        ]);
        let frame = SdkValue::map([
            ("op", t("store.call")),
            ("args", args),
            ("generation", hello.get("generation").unwrap().clone()),
            ("request_id", t("negative-proof")),
        ]);
        worker::write_host_frame(&stream, &frame, worker::HOST_MAX_REQUEST, end).unwrap();
        let answer = worker::read_host_frame(&stream, worker::HOST_MAX_REPLY, Some(end))
            .unwrap()
            .unwrap();
        assert_eq!(
            answer.get("error").unwrap().get_text("code"),
            Some(expected)
        );
    }
    assert_eq!(home.ping().get_int("stores"), Some(0));
    cleanup(&home);
    assert!(worker.wait(Duration::from_secs(5)).success());
}

#[test]
fn malformed_reply_after_the_request_is_unknown_without_a_direct_attempt() {
    let home = Fixture::new();
    prepare(&home.paths.home);
    let mut reference = home.start();
    let mut hello = worker::HostHello::own(&home.paths).unwrap();
    hello.build = home.ping().get_text("build").unwrap().to_owned();
    cleanup(&home);
    assert!(reference.wait(Duration::from_secs(5)).success());

    let guard = home.paths.try_hold().unwrap().unwrap();
    let listener = home.paths.bind(false, &guard).unwrap();
    listener.set_nonblocking(true).unwrap();
    let fixture = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(7);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < end);
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        worker::host_check_peer(&stream).unwrap();
        worker::write_host_frame(&stream, &hello.to_frame(), worker::MAX_HELLO, end).unwrap();
        let request = worker::read_host_frame(&stream, worker::HOST_MAX_REQUEST, Some(end))
            .unwrap()
            .unwrap();
        let args = request.get("args").unwrap();
        assert_eq!(args.get_text("method"), Some("fs.stat"));
        assert_eq!(args.get_text("store"), Some("shop"));
        assert_eq!(args.get_text("project"), Some("api"));
        assert!(args.get("store_record").is_none());
        assert!(args.get("storePath").is_none());
        stream.write_all(&[0, 0, 0, 9, b'{']).unwrap();
        request.get_text("request_id").unwrap().to_owned()
    });

    let out = run(layerfs(
        &home.paths.home,
        &[
            "file",
            "stat",
            "shop/api",
            "main",
            "/docs/a.txt",
            "--detail",
        ],
    )
    .env("LAYERFS_TRACE", home.paths.home.join("route.jsonl")));
    let id = fixture.join().unwrap();
    assert_eq!(out.status.code(), Some(125));
    assert!(out.stdout.is_empty());
    let error = text(&out.stderr);
    assert!(
        error.contains("\"Unknown\"") && error.contains(&id),
        "{error}"
    );
    let trace = lines(&home.paths.home.join("route.jsonl"));
    assert_eq!(trace.len(), 1, "{trace:?}");
    assert_eq!(trace[0].get_text("event"), Some("worker.connection"));
    assert!(trace[0].get_bool("sent"));
    home.paths.remove_endpoints(&guard).unwrap();
}

#[test]
fn real_warm_worker_releases_its_lease_before_store_remove() {
    let home = Fixture::new();
    let path = &home.paths.home;
    prepare(path);
    let _ = read_cli(path, &["file", "read", "shop/api", "main", "/docs/a.txt"]);
    let before = home.ping();
    assert_eq!(before.get_int("stores"), Some(1));
    let out = run(&mut layerfs(path, &["store", "remove", "shop"]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!paths::store_record(path, "shop").exists());
    let after = home.ping();
    assert_eq!(after.get_int("stores"), Some(0));
    assert_eq!(before.get_text("generation"), after.get_text("generation"));
    let close = after.get("last_close").unwrap();
    assert_eq!(close.get_text("store"), Some("shop"));
    assert!(close.get_bool("closed"));
    cleanup(&home);
}
