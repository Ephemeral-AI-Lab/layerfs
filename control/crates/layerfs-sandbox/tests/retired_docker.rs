//! A Sandbox recorded under the retired docker backend (D-8,
//! layerfs-project#111) on the side that runs Sandboxes (a machine's
//! worker): reopening it is refused `Unsupported` with how to recreate it,
//! and `sandbox.close` forgets it, answering its container's name. No
//! Docker is used; nothing it names is touched.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, read_record, write_record};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-retired-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// A worker's record of Sandbox `old` as the docker backend wrote it: no
/// `runtime`, its container's name and id, the Engine socket.
fn docker_record(root: &Path, runtime: Option<&str>) -> SdkValue {
    let mut daemon = vec![
        ("platform", t("docker")),
        ("name", t("lfs-0123abcd-old")),
        ("container", t(&"c".repeat(64))),
        ("docker", t("/var/run/docker.sock")),
        ("image", t(&format!("sha256:{}", "d".repeat(64)))),
    ];
    if let Some(runtime) = runtime {
        daemon.push(("runtime", t(runtime)));
    }
    let daemon = SdkValue::Map(daemon.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
    let dir = paths::sandbox_dir(root, "old");
    std::fs::create_dir_all(&dir).unwrap();
    write_record(
        &dir.join("record.json"),
        &SdkValue::map([("name", t("old")), ("daemon", daemon.clone())]),
    )
    .unwrap();
    daemon
}

fn args(root: &Path) -> SdkValue {
    SdkValue::map([
        ("home", t(&root.display().to_string())),
        ("sandbox", t("old")),
        ("project", t("shop/api")),
    ])
}

#[test]
fn a_retired_docker_record_is_refused_with_how_to_recreate_it() {
    for runtime in [None, Some("docker")] {
        let root = root(&format!("refused-{}", runtime.unwrap_or("none")));
        let daemon = docker_record(&root, runtime);
        let refusal = match layerfs_sandbox::recorded::recorded(
            &args(&root),
            "old",
            &layerfs_sandbox::record::DaemonRecord::from_record(&daemon),
        ) {
            Ok(_) => panic!("a retired docker record was reopened"),
            Err(refusal) => refusal,
        };
        assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
        for part in [
            "docker Sandbox backend",
            "`layerfs sandbox close old` forgets it",
            "`docker rm -f lfs-0123abcd-old`",
            "`layerfs sandbox create`",
        ] {
            assert!(
                refusal.message.contains(part),
                "{part}: {}",
                refusal.message
            );
        }
        // the record is left as it was
        assert!(paths::sandbox_record(&root, "old").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn close_forgets_a_retired_docker_sandbox_and_names_its_container() {
    let root = root("close");
    docker_record(&root, None);
    let answer = layerfs_sandbox::lifecycle::close(&args(&root)).unwrap();
    assert_eq!(answer.get_text("kind"), Some("Forgotten"), "{answer:?}");
    assert_eq!(answer.get_text("container"), Some("lfs-0123abcd-old"));
    assert_eq!(
        answer.get_text("remove"),
        Some("docker rm -f lfs-0123abcd-old")
    );
    assert!(!paths::sandbox_dir(&root, "old").exists());
    // closed again: nothing is recorded any more
    let refusal = layerfs_sandbox::lifecycle::close(&args(&root)).unwrap_err();
    assert_eq!(refusal.code, "NoSuchSandbox", "{}", refusal.message);
    assert!(read_record(&paths::sandbox_record(&root, "old"))
        .unwrap()
        .is_none());
    std::fs::remove_dir_all(root).unwrap();
}

/// A create of the same name on the worker forgets the retired record, as
/// a close does, instead of answering `Exists` (D-8: a home whose close
/// could not reach the worker must not leave the name stuck). The create
/// then fails on its own inputs (no `deploy.localDir`), after the record
/// is gone; nothing is launched.
#[test]
fn a_create_of_the_same_name_forgets_a_retired_docker_record() {
    let root = root("create");
    docker_record(&root, None);
    write_record(
        &paths::store_record(&root, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("schema", t("shop")),
            ("synchronous_commit", t("off")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
        ]),
    )
    .unwrap();
    let backend = if cfg!(target_os = "macos") {
        "native"
    } else {
        "namespaces"
    };
    let args = SdkValue::map([
        ("home", t(&root.display().to_string())),
        ("sandbox", t("old")),
        ("project", t("api")),
        ("store", t("pg")),
        (
            "postgresql",
            SdkValue::map([("url", t("postgresql://127.0.0.1:1/none"))]),
        ),
        ("deploy", SdkValue::map([("sandbox", t(backend))])),
    ]);
    let refusal = layerfs_sandbox::lifecycle::create(&args).unwrap_err();
    assert_ne!(refusal.code, "Exists", "{}", refusal.message);
    assert!(
        matches!(refusal.code.as_str(), "InvalidInput" | "Unsupported"),
        "{}: {}",
        refusal.code,
        refusal.message
    );
    assert!(!paths::sandbox_record(&root, "old").exists());
    std::fs::remove_dir_all(root).unwrap();
}
