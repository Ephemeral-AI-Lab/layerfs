//! Exercise real tar streams into a private stand-in guest filesystem.
#![cfg(target_os = "macos")]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Platform};
use standin::Scratch;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn setup(s: &Scratch) {
    s.tool(
        "limactl",
        r#"[ "$1" = list ] && printf 'Running\t%s\n' "$SCRATCH/instance"; exit 0"#,
    );
    s.tool("tar", r#"exec /usr/bin/tar "$@""#);
    let script = format!(
        r##"#!/usr/bin/python3
import os, shlex, subprocess, sys
root = {root:?}
args = shlex.split(" ".join(sys.argv[sys.argv.index("--") + 1:]))[3:]
body = args[2] if len(args) > 2 else ""
if "findmnt" in body: sys.exit(0)
if "max_user_namespaces" in body: print(0); sys.exit(0)
if "setsid nohup" in body: sys.exit(7)
args = [a.replace("/layerfs/", root + "/guest/layerfs/") for a in args]
result = subprocess.run(args, stdout=subprocess.PIPE)
sys.stdout.buffer.write(result.stdout.replace((root + "/guest/layerfs").encode(), b"/layerfs"))
sys.exit(result.returncode)
"##,
        root = s.dir.to_str().unwrap()
    );
    let path = s.bin().join("ssh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn manifest(s: &Scratch, hash: &str, worker_hash: &str) -> machine::Bundle {
    let mut b = s.bundle();
    b.hash = hash.into();
    fs::write(b.dir.join("sandbox-image.tar"), b"same large image").unwrap();
    fs::write(b.dir.join("bundle.json"), format!(r#"{{"hash":"{hash}","files":{{"layerfs":"{worker_hash}","layerfs-daemon":"{stable}","layerfs-sandbox-helper":"{stable}","sandbox-image.tar":"{stable}"}}}}"#, stable="a".repeat(64))).unwrap();
    b
}

fn start(s: &Scratch, b: &machine::Bundle) {
    let error = machine::start(&s.spec(Platform::Lima), b).unwrap_err();
    assert!(error.message.contains("starting the worker"), "{error}");
}

#[test]
fn runtime_update_keeps_image_and_repairs_a_missing_artifact() {
    let s = Scratch::new("guest-artifacts");
    setup(&s);
    let b = manifest(&s, "1111", &"b".repeat(64));
    start(&s, &b);
    fs::remove_file(s.log_file()).unwrap();
    let b = manifest(&s, "2222", &"c".repeat(64));
    start(&s, &b);
    let log = s.log();
    assert!(log.contains("-cf - layerfs"), "worker should update: {log}");
    assert!(
        !log.contains("-cf - sandbox-image.tar"),
        "unchanged image should stay: {log}"
    );
    assert!(!log.contains("-cf - layerfs-daemon"));
    assert_eq!(
        fs::read_to_string(s.path("guest/layerfs/bundle/.sealed"))
            .unwrap()
            .trim(),
        "2222"
    );
    fs::remove_file(s.path("guest/layerfs/bundle/sandbox-image.tar")).unwrap();
    fs::remove_file(s.log_file()).unwrap();
    start(&s, &b);
    assert!(
        s.log().contains("-cf - sandbox-image.tar"),
        "missing image must be repaired"
    );
}

#[test]
fn interrupted_copy_never_seals_the_new_bundle() {
    let s = Scratch::new("guest-copy-failed");
    setup(&s);
    let b = manifest(&s, "1111", &"b".repeat(64));
    start(&s, &b);
    let b = manifest(&s, "2222", &"c".repeat(64));
    s.tool("tar", "exit 1");
    assert!(machine::start(&s.spec(Platform::Lima), &b).is_err());
    assert_eq!(
        fs::read_to_string(s.path("guest/layerfs/bundle/.sealed"))
            .unwrap()
            .trim(),
        "1111"
    );
}

#[test]
fn legacy_copy_failure_keeps_the_previous_complete_bundle() {
    let s = Scratch::new("legacy-copy-failed");
    setup(&s);
    let mut b = s.bundle();
    start(&s, &b);
    let marker = s.path("guest/layerfs/bundle/.sealed");
    assert_eq!(fs::read_to_string(&marker).unwrap().trim(), b.hash);
    b.hash = "abcd5678".into();
    s.tool("tar", "exit 1");
    assert!(machine::start(&s.spec(Platform::Lima), &b).is_err());
    assert_eq!(fs::read_to_string(marker).unwrap().trim(), "0123abcd");
    assert!(s.path("guest/layerfs/bundle/layerfs").is_file());
}
