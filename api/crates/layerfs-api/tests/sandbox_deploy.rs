//! Where a Sandbox's daemon and the sealed Linux bundle are looked for
//! (#533 review P0-4, O16): `<cache>/layerfs/bundle/<version>/<os>-<arch>/`,
//! the cache `XDG_CACHE_HOME` or `~/.cache`. A `native` Sandbox with no
//! daemon recorded names this host's bundle folder; a machine that needs
//! the Linux bundle names `linux-<arch>` and the tool that places it. One
//! test, because it sets the process's environment.
#![cfg(feature = "machine")]
use layerfs_api::calls;
use layerfs_api::home;
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-deploy-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

#[test]
fn the_daemon_and_the_linux_bundle_come_from_the_versioned_cache() {
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");
    let version = env!("CARGO_PKG_VERSION");
    let arch = std::env::consts::ARCH;

    // a native Sandbox with no daemon recorded: this host's bundle folder
    let home = tempdir("home");
    let folder = tempdir("src");
    home::open(&home).unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    call(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t("t")),
            ("machine", t("native")),
        ],
    )
    .unwrap_err();
    let expected = cache
        .join("layerfs/bundle")
        .join(version)
        .join(format!("{}-{arch}", std::env::consts::OS))
        .join("layerfs-daemon");
    assert_eq!(code, "Unavailable", "{message}");
    assert!(
        message.contains(&expected.display().to_string()),
        "{message}"
    );
    // nothing was claimed
    assert_eq!(
        call(&home, "sandbox.list", &[]).unwrap().get("items"),
        Some(&SdkValue::List(Vec::new()))
    );

    // a VM needs the Linux bundle: refused naming linux-<arch> (or, on a
    // host without the VM's tool, refused for the tool first)
    let (code, message) = call(
        &home,
        "machine.create",
        &[("name", t("dev")), ("platform", t("lima"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    if message.contains("bundle") {
        let linux = cache
            .join("layerfs/bundle")
            .join(version)
            .join(format!("linux-{arch}"));
        assert!(message.contains(&linux.display().to_string()), "{message}");
        assert!(message.contains("tools/linux_bundle.py"), "{message}");
    } else {
        assert!(
            message.contains("cannot be created on this host"),
            "{message}"
        );
    }
    for dir in [home, folder, cache] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}
