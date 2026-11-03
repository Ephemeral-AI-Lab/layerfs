//! A docker Sandbox with no image named uses the bundle's Sandbox image
//! (#535 P5): `sandbox-image.tar` is loaded into Docker once, by
//! `machine.start docker` (the only call that starts a machine, D11), its
//! image id recorded on the docker machine and used for the worker's
//! container and the Sandbox; `--image` still overrides; a bundle without
//! the archive is refused naming it. A stand-in `docker` (it loads, but runs no
//! container) and a bundle folder of empty stand-ins; no Docker is used.
//! One test, because it sets the process's environment.
#![cfg(feature = "machine")]
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const ID: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-p5-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn create(home: &Path, name: &str, image: Option<&str>) -> SdkRefusal {
    let mut fields = vec![
        ("project".to_owned(), t("shop/api")),
        ("name".to_owned(), t(name)),
        ("machine".to_owned(), t("docker")),
    ];
    if let Some(image) = image {
        fields.push(("image".to_owned(), t(image)));
    }
    calls::call(
        Some(home),
        "sandbox.create",
        SdkValue::Map(fields),
        &mut |_| {},
    )
    .expect_err("no container runs here")
}

fn start(home: &Path) -> SdkRefusal {
    calls::call(
        Some(home),
        "machine.start",
        SdkValue::map([("name", t("docker"))]),
        &mut |_| {},
    )
    .expect_err("no container runs here")
}

fn log(tools: &Path) -> String {
    std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default()
}

#[test]
fn a_docker_sandbox_with_no_image_named_uses_the_bundle_s_image() {
    // a stand-in docker: `load` answers an id, no container exists or runs
    let tools = tempdir("tools");
    let docker = tools.join("docker");
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> "${{0%/*}}/calls.log"
case "$3" in
  load) echo "Loaded image ID: {ID}" ;;
  inspect) echo "Error: No such object" >&2; exit 1 ;;
  *) echo "the stand-in refuses: $*" >&2; exit 9 ;;
esac
"#
    );
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    // a sealed bundle of empty stand-ins, with no image archive yet
    let bundle = tempdir("bundle");
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        std::fs::write(bundle.join(name), b"").unwrap();
    }
    std::fs::write(bundle.join("bundle.json"), r#"{"hash":"0123abcd"}"#).unwrap();

    let home = tempdir("home");
    home::open(&home).unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("docker")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();

    // no archive in the bundle: refused naming it, nothing loaded
    let refusal = start(&home);
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(
        refusal.message.contains("sandbox-image.tar"),
        "{}",
        refusal.message
    );
    assert!(!log(&tools).contains("load"), "{}", log(&tools));

    // with the archive: loaded once, recorded, and the worker's container
    // is started on it (the stand-in runs none)
    std::fs::write(bundle.join("sandbox-image.tar"), b"").unwrap();
    let refusal = start(&home);
    assert_eq!(refusal.code, "Failed", "{}", refusal.message);
    let record = read_record(&paths::machine_record(&home, "docker"))
        .unwrap()
        .unwrap();
    let loaded = record.get("loaded_image").cloned().unwrap_or_default();
    assert_eq!(loaded.get_text("id"), Some(ID), "{record:?}");
    assert_eq!(loaded.get_text("bundle"), Some("0123abcd"));
    let calls = log(&tools);
    let loads: Vec<&str> = calls.lines().filter(|l| l.contains(" load ")).collect();
    assert_eq!(loads.len(), 1, "{calls}");
    let archive = bundle.join("sandbox-image.tar");
    let wanted = format!("load -i {}", archive.display());
    assert!(loads[0].ends_with(&wanted), "{calls}");
    let run = calls.lines().find(|l| l.contains(" run ")).unwrap_or("");
    assert!(run.contains(&format!("--entrypoint sh {ID} -c")), "{calls}");

    // a create uses the recorded id: no second load; with no worker
    // running it is refused naming `machine start`, and starts nothing
    let refusal = create(&home, "s3", None);
    assert_eq!(refusal.code, "Unavailable", "{}", refusal.message);
    assert!(
        refusal.message.contains("layerfs machine start docker"),
        "{}",
        refusal.message
    );
    let calls = log(&tools);
    assert_eq!(calls.matches(" load ").count(), 1, "{calls}");
    assert_eq!(calls.matches(" run ").count(), 1, "{calls}");

    // a named image overrides it: the worker still runs the loaded one,
    // and nothing is loaded again
    let named = format!("sha256:{}", "2".repeat(64));
    let _ = create(&home, "s4", Some(&named));
    assert_eq!(log(&tools).matches(" load ").count(), 1);

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
    let _ = std::fs::remove_dir_all(bundle);
}
