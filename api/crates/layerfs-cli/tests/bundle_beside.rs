//! The `layerfs` command finds the Linux bundle of its own build beside
//! its executable (UI pass 2, LF4 item e): with no `LAYERFS_BUNDLE`, an
//! installed runtime's `<runtime>/bundle/linux-<arch>/` next to
//! `<runtime>/bin/layerfs` (reached through a symlink, as `~/.local/bin`
//! links it) comes before the user cache, but only when its seal names this
//! build (version, worker protocol, source commit and dirty flag).
//! `LAYERFS_BUNDLE` still wins. `home get --detail` says which it uses.
mod common;
use common::{run, tempdir, text, LAYERFS};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::path::Path;
use std::process::Command;

/// `home get --detail`'s `bundle`, run through `program` with the cache in
/// `scratch`, and `LAYERFS_BUNDLE` only when `env` names it.
fn bundle(program: &Path, scratch: &Path, env: Option<&Path>) -> SdkValue {
    let mut command = Command::new(program);
    command
        .args(["home", "get", "--detail"])
        .env("LAYERFS_HOME", scratch.join("home"))
        .env("XDG_CACHE_HOME", scratch.join("cache"))
        .env_remove("LAYERFS_BUNDLE");
    if let Some(dir) = env {
        command.env("LAYERFS_BUNDLE", dir);
    }
    let out = run(&mut command);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let answer = json::parse(&text(&out.stdout)).unwrap();
    answer
        .get("bundle")
        .cloned()
        .unwrap_or_else(|| panic!("{answer:?}"))
}

#[test]
fn a_bundle_of_this_build_beside_the_executable_comes_before_the_cache() {
    let scratch = tempdir("beside");
    // this build's identity, as the command reports it
    let out = run(Command::new(LAYERFS).args(["--version", "--json"]));
    let version = json::parse(&text(&out.stdout)).unwrap();
    let commit = version.get_text("commit").unwrap().to_owned();
    let dirty = version.get("dirty") == Some(&SdkValue::Bool(true));

    // an installed runtime: bin/layerfs and bundle/linux-<arch>/
    let runtime = scratch.join("runtime");
    std::fs::create_dir_all(runtime.join("bin")).unwrap();
    let program = runtime.join("bin/layerfs");
    std::fs::copy(LAYERFS, &program).unwrap();
    let sealed = runtime
        .join("bundle")
        .join(format!("linux-{}", std::env::consts::ARCH));
    std::fs::create_dir_all(&sealed).unwrap();
    let seal = |head: &str| {
        let manifest = format!(
            r#"{{"hash":"beside","version":"{}","protocol":{},"platform":"linux-{}","source":{{"head":"{head}","dirty":{dirty},"sha256":"00"}}}}"#,
            env!("CARGO_PKG_VERSION"),
            layerfs_api::worker::PROTOCOL,
            std::env::consts::ARCH
        );
        std::fs::write(sealed.join("bundle.json"), manifest).unwrap();
    };
    // reached through a symlink elsewhere
    std::fs::create_dir_all(scratch.join("link")).unwrap();
    let link = scratch.join("link/layerfs");
    std::os::unix::fs::symlink(&program, &link).unwrap();
    let real = sealed.canonicalize().unwrap();

    seal(&commit);
    let found = bundle(&link, &scratch, None);
    assert_eq!(found.get_text("source"), Some("beside"), "{found:?}");
    assert_eq!(found.get_text("path"), real.to_str(), "{found:?}");
    assert_eq!(found.get("present"), Some(&SdkValue::Bool(true)));

    // LAYERFS_BUNDLE still names another folder
    let other = scratch.join("other");
    let found = bundle(&link, &scratch, Some(&other));
    assert_eq!(found.get_text("source"), Some("env:LAYERFS_BUNDLE"));
    assert_eq!(found.get_text("path"), other.to_str());

    // a seal of another build is not used: the cache is
    seal(&"0".repeat(40));
    let found = bundle(&link, &scratch, None);
    assert_eq!(found.get_text("source"), Some("cache"), "{found:?}");
    let cache = found.get_text("path").unwrap();
    assert!(
        cache.starts_with(scratch.join("cache").to_str().unwrap()),
        "{cache}"
    );

    // no bundle beside it: the cache
    std::fs::remove_dir_all(runtime.join("bundle")).unwrap();
    let found = bundle(&link, &scratch, None);
    assert_eq!(found.get_text("source"), Some("cache"), "{found:?}");
    std::fs::remove_dir_all(scratch).unwrap();
}
