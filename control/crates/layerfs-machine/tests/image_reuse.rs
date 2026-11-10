//! Concurrent homes and binary-only rebuilds share one import on each Engine.
#![cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home;
use layerfs_machine::{docker_default, ensure_built_ins, record::MachineRecord};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn concurrent_first_use_runtime_updates_pruning_and_engine_separation() {
    let root = std::env::temp_dir().join(format!("lfs-image-reuse-{}", std::process::id()));
    fs::create_dir_all(root.join("bin")).unwrap();
    let bundle = root.join("bundle");
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("sandbox-image.tar"), b"archive").unwrap();
    let script = format!(
        r#"#!/bin/sh
root='{root}'
engine=${{2##*/}}
shift 2
case "$1 $2" in
  'image inspect')
    for last do :; done
    key=${{last##*:}}
    [ -f "$root/$engine-$key" ] && {{ printf 'sha256:%064d\n' 7; exit 0; }}
    echo 'No such image' >&2; exit 1 ;;
  'load -i')
    echo "$engine" >> "$root/imports"
    touch "$root/$engine-$(printf '%064d' 7)"
    sleep .05
    printf 'Loaded image ID: sha256:%064d\n' 7 ;;
  'image tag')
    key=${{4##*:}}
    touch "$root/$engine-$key" ;;
esac
"#,
        root = root.display()
    );
    let docker = root.join("bin/docker");
    fs::write(&docker, script).unwrap();
    fs::set_permissions(docker, fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var_os("PATH").unwrap();
    let old_cache = std::env::var_os("XDG_CACHE_HOME");
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            root.join("bin").display(),
            old_path.to_string_lossy()
        ),
    );
    std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let write = |runtime: &str, image: &str| {
        fs::write(
            bundle.join("bundle.json"),
            format!(r#"{{"hash":"{runtime}","protocol":3,"image":"sha256:{image}"}}"#),
        )
        .unwrap()
    };
    let make = |name: &str, socket: &str| {
        let path = root.join(name);
        let id = home::ensure(&path).unwrap().id;
        ensure_built_ins(&path, &id).unwrap();
        MachineRecord::of(&path, "docker")
            .unwrap()
            .with("docker", SdkValue::text(socket))
            .write(&path)
            .unwrap();
        path
    };
    write(&"a".repeat(64), &"b".repeat(64));
    let one = make("one", "/fake/first.sock");
    let two = make("two", "/fake/first.sock");
    let first = std::thread::spawn(move || docker_default(&one).unwrap());
    let second = std::thread::spawn(move || docker_default(&two).unwrap());
    assert_eq!(first.join().unwrap(), second.join().unwrap());
    let count = || {
        fs::read_to_string(root.join("imports"))
            .unwrap()
            .lines()
            .count()
    };
    assert_eq!(
        count(),
        1,
        "same Engine/content imports once even concurrently"
    );
    write(&"c".repeat(64), &"b".repeat(64));
    let three = make("three", "/fake/first.sock");
    docker_default(&three).unwrap();
    assert_eq!(count(), 1, "runtime-only change must not import");
    write(&"d".repeat(64), &"e".repeat(64));
    docker_default(&three).unwrap();
    assert_eq!(count(), 2, "image-only change imports once");
    fs::remove_file(root.join(format!("first.sock-{}", "e".repeat(64)))).unwrap();
    fs::remove_file(root.join(format!("first.sock-{:064}", 7))).unwrap();
    docker_default(&three).unwrap();
    assert_eq!(count(), 3, "missing image must not reuse stale record");
    let four = make("four", "/fake/second.sock");
    docker_default(&four).unwrap();
    assert_eq!(count(), 4, "another Engine owns its own mapping");
    fs::write(
        bundle.join("bundle.json"),
        format!(r#"{{"hash":"{}"}}"#, "d".repeat(64)),
    )
    .unwrap();
    docker_default(&three).unwrap();
    assert_eq!(
        count(),
        4,
        "legacy metadata can reuse a present matching recorded ID"
    );
    std::env::set_var("PATH", old_path);
    std::env::remove_var("LAYERFS_BUNDLE");
    match old_cache {
        Some(value) => std::env::set_var("XDG_CACHE_HOME", value),
        None => std::env::remove_var("XDG_CACHE_HOME"),
    }
    fs::remove_dir_all(root).unwrap();
}
