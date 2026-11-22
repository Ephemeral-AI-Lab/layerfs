//! Sandbox route URL rewriting on Linux (`docker` and `native`):
//! `localhost` becomes `127.0.0.1` so daemons in a rootfs without `/etc/hosts`
//! can dial it, while `::1`, `127.0.0.1`, and `sslmode=verify-full` rules stay
//! intact.
#![cfg(all(feature = "machine", target_os = "linux"))]

use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, secrets, write_entity};
use layerfs_store::record::StoreRecord;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SECRET: &str = "s3cr3t-pg-pw";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-sb-reach-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn write_store(home: &std::path::Path, url: &str) -> StoreRecord {
    let value = SdkValue::map([
        ("v", SdkValue::Int(1)),
        ("name", SdkValue::text("shop")),
        ("kind", SdkValue::text("postgresql")),
        ("url", SdkValue::text(url)),
        ("auth", SdkValue::text("pg")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
    ]);
    write_entity(&paths::store_record(home, "shop"), &value).unwrap();
    StoreRecord::of(home, "shop").unwrap()
}

#[test]
fn linux_docker_and_native_rewrite_localhost_to_127_0_0_1_and_keep_v6_and_verify_full() {
    let home = tempdir("home");
    let opened = home::open(&home).unwrap();
    layerfs_machine::ensure_built_ins(&home, &opened.id).unwrap();
    secrets::save_secret(
        &home,
        "auth",
        "pg",
        &SdkValue::map([
            ("user", SdkValue::text("app")),
            ("password", SdkValue::text(SECRET)),
        ]),
    )
    .unwrap();
    for machine in ["native", "docker"] {
        let r = write_store(&home, "postgresql://localhost:55432/shop?sslmode=disable");
        let (sel, _) = layerfs_sandbox::route::reached(&home, machine, &r).unwrap();
        assert_eq!(
            sel.url.as_deref(),
            Some("postgresql://app:s3cr3t-pg-pw@127.0.0.1:55432/shop?sslmode=disable"),
            "{machine}"
        );

        let v6 = write_store(&home, "postgresql://[::1]:55432/shop?sslmode=verify-full");
        let (sel_v6, _) = layerfs_sandbox::route::reached(&home, machine, &v6).unwrap();
        assert_eq!(
            sel_v6.url.as_deref(),
            Some("postgresql://app:s3cr3t-pg-pw@[::1]:55432/shop?sslmode=verify-full"),
            "{machine}"
        );

        let v4 = write_store(
            &home,
            "postgresql://127.0.0.1:55432/shop?sslmode=verify-full",
        );
        let (sel_v4, _) = layerfs_sandbox::route::reached(&home, machine, &v4).unwrap();
        assert_eq!(
            sel_v4.url.as_deref(),
            Some("postgresql://app:s3cr3t-pg-pw@127.0.0.1:55432/shop?sslmode=verify-full"),
            "{machine}"
        );

        let verify_localhost = write_store(
            &home,
            "postgresql://localhost:55432/shop?sslmode=verify-full",
        );
        let Err(err) = layerfs_sandbox::route::reached(&home, machine, &verify_localhost) else {
            panic!("{machine}: expected Unsupported for localhost with verify-full");
        };
        assert_eq!(err.code, "Unsupported", "{machine}: {err:?}");
        assert!(!err.message.contains(SECRET), "{err:?}");
    }
    let _ = std::fs::remove_dir_all(&home);
}
