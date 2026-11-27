//! No list, get or status call starts a machine (#535 P4, #536 §10b): with
//! a stopped Lima machine holding a Store, a project and a Sandbox (a
//! stand-in `limactl` that reports it stopped and logs every command),
//! every `*.list`, `*.get` and `*.status` call answers from records and
//! live probes (`machine list` and `machine get` with `--with live` too). `sandbox.get` answers the record
//! with no Workspaces and why; `workspace.status` is refused `Unavailable`.
//! Nothing ran a start: no `start.lock`, no `limactl start`, no
//! `worker.json`. The same holds with a stale `worker.json` that nothing
//! answers. One test, because it sets the process's environment.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, write_record};
use layerfs_protocol::provision::install::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const STAND_IN: &str = r#"#!/bin/sh
echo "$*" >> "${0%/*}/calls.log"
case "$1" in
  --version) echo "limactl version 0.0-stand-in" ;;
  list) printf 'Stopped\t/nonexistent\n' ;;
  *) echo "the stand-in refuses: $*" >&2; exit 9 ;;
esac
"#;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-nostart-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, &str)]) -> Result<SdkValue, (String, String)> {
    let live = |(n, v): &(&str, &str)| match (*n, *v) {
        ("with", section) => ("with".to_owned(), SdkValue::List(vec![t(section)])),
        (n, v) => (n.to_owned(), t(v)),
    };
    let args = SdkValue::Map(args.iter().map(live).collect());
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A home with stopped Lima machine `dev` holding sqlite Store `shop`
/// (project `api`) and Sandbox `s1`.
fn home_with_stopped_vm(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-dev")),
            ("state", t("stopped")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("normal")),
            ("machine", t("dev")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    write_record(
        &paths::sandbox_record(&home, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("dev")),
            ("id", t("s-1")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    home
}

/// Runs every list, get and status call against `home`.
fn every_read(home: &Path) {
    for (name, args) in [
        ("machine.list", vec![]),
        ("machine.list", vec![("with", "live")]),
        ("machine.get", vec![("machine", "dev")]),
        ("machine.get", vec![("machine", "dev"), ("with", "live")]),
        ("store.list", vec![]),
        ("store.get", vec![("store", "shop")]),
        ("store.get", vec![("store", "shop"), ("with", "live")]),
        ("project.list", vec![]),
        ("project.get", vec![("project", "shop/api")]),
        ("sandbox.list", vec![]),
        ("backup.list", vec![]),
    ] {
        call(home, name, &args).unwrap_or_else(|(code, why)| panic!("{name}: {code} {why}"));
    }
    let status = call(home, "store.get", &[("store", "shop"), ("with", "live")]).unwrap();
    assert_eq!(status.get_text("state"), Some("blocked"), "{status:?}");
    // a list item carries the same state, starting nothing (#536 §10b)
    let listed = call(home, "store.list", &[]).unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}")
    };
    let shop = items.iter().find(|i| i.get_text("store") == Some("shop"));
    assert_eq!(
        shop.and_then(|i| i.get_text("state")),
        Some("blocked"),
        "{listed:?}"
    );
    let machine = call(home, "machine.get", &[("machine", "dev")]).unwrap();
    assert_eq!(machine.get_text("state"), Some("stopped"), "{machine:?}");

    // blocked by its stopped machine, from records: no daemon is asked
    let got = call(home, "sandbox.get", &[("sandbox", "s1")]).unwrap();
    assert_eq!(got.get_text("state"), Some("blocked"), "{got:?}");
    let by = got.get("blocked_by").unwrap_or_else(|| panic!("{got:?}"));
    assert_eq!(by.get_text("machine"), Some("dev"), "{got:?}");
    assert_eq!(by.get_text("reason"), Some("stopped"), "{got:?}");

    let (code, message) = call(home, "workspace.status", &[("workspace", "s1:w")]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    // D11: nothing starts a machine; the refusal names the call that does
    assert!(message.contains("layerfs machine start dev"), "{message}");
    // and carries it as the refusal's `fix` (conformance R1 F5)
    let args = SdkValue::map([("workspace", t("s1:w"))]);
    let refusal =
        layerfs_api::call(Some(home), "workspace.status", args, &mut |_| {}).expect_err("refused");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs machine start dev"),
        "{refusal:?}"
    );
}

fn nothing_started(home: &Path, tools: &Path) {
    let dir = paths::machine_dir(home, "dev");
    assert!(!dir.join("start.lock").exists(), "a start ran");
    let log = std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default();
    assert!(
        log.lines()
            .all(|l| l == "--version" || l.starts_with("list ")),
        "{log}"
    );
}

#[test]
fn no_list_get_or_status_call_starts_a_machine() {
    let tools = tempdir("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");

    // no worker recorded
    let home = home_with_stopped_vm("none");
    every_read(&home);
    nothing_started(&home, &tools);
    assert!(!paths::worker_record(&home, "dev").exists());
    std::fs::remove_dir_all(&home).unwrap();

    // a stale worker.json that nothing answers
    let home = home_with_stopped_vm("stale");
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let stale = SdkValue::map([("address", t(&gone.to_string())), ("secret", t("5e"))]);
    write_record(&paths::worker_record(&home, "dev"), &stale).unwrap();
    every_read(&home);
    nothing_started(&home, &tools);
    std::fs::remove_dir_all(&home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
    let _ = std::fs::remove_dir_all(cache);
}
