//! The one support table: `control/SUPPORT.json` is its rendering, and every
//! refusal the check gives is the row's reason. `LAYERFS_BLESS_SUPPORT=1`
//! writes the file from the table instead of comparing.
#![cfg(unix)]
use layerfs_platform::{
    platforms, sandbox_allowed, support, support_json, Backend, CallSite, HostOs, KernelRoute,
    Platform, StoreKind,
};
use layerfs_sandbox_backend::platform_policy::disabled_reason;
use std::path::PathBuf;

fn committed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SUPPORT.json")
}

#[test]
fn support_json_is_the_rendered_table() {
    let rendered = support_json();
    if std::env::var_os("LAYERFS_BLESS_SUPPORT").is_some_and(|v| v == "1") {
        std::fs::write(committed(), &rendered).unwrap();
        return;
    }
    let file = std::fs::read_to_string(committed()).unwrap();
    assert!(
        file == rendered,
        "control/SUPPORT.json differs from support.rs; rerun this test with LAYERFS_BLESS_SUPPORT=1"
    );
}

#[test]
fn every_refusal_is_the_rows_reason() {
    for host in HostOs::NAMED.into_iter().chain([HostOs::Other]) {
        for row in platforms(host) {
            let postgresql = sandbox_allowed(row.platform, host, StoreKind::Postgresql);
            assert_eq!(row.postgresql.sandbox, postgresql.is_ok(), "{row:?}");
            assert_eq!(row.postgresql.reason, postgresql.err(), "{row:?}");
            if let Err(reason) = sandbox_allowed(row.platform, host, StoreKind::Sqlite) {
                assert_eq!(Some(reason), row.postgresql.reason, "{row:?}");
                assert!(!row.supports().sqlite);
            }
            assert_eq!(row.postgresql.store_calls, CallSite::InProcess);
            assert_eq!(row.supports().reason, row.postgresql.reason);
        }
    }
}

/// #535 §7.3: PostgreSQL Sandboxes run on native (macOS, and Linux
/// namespaces with no bind; phase 5a), docker, ssh, and since the G7 live
/// receipts on lima, apple-container, microsandbox and firecracker, on every
/// host each is on.
#[test]
fn postgresql_sandboxes_run_on_every_platform_with_a_backend() {
    let allowed = |p, h| sandbox_allowed(p, h, StoreKind::Postgresql);
    for host in [HostOs::Macos, HostOs::Linux] {
        assert_eq!(allowed(Platform::Native, host), Ok(()));
        assert_eq!(allowed(Platform::Docker, host), Ok(()));
        assert_eq!(allowed(Platform::Microsandbox, host), Ok(()), "{host:?}");
    }
    for host in HostOs::NAMED {
        assert_eq!(
            allowed(Platform::Ssh, host),
            Err(disabled_reason("ssh").unwrap()),
            "{host:?}"
        );
    }
    for platform in [
        Platform::Lima,
        Platform::AppleContainer,
        Platform::Firecracker,
    ] {
        assert_eq!(
            allowed(platform, HostOs::Macos),
            disabled_reason(platform.name()).map_or(Ok(()), Err),
            "{platform:?}"
        );
    }
    assert_eq!(
        allowed(Platform::Native, HostOs::Windows),
        Err("no Sandbox backend on Windows (O5)")
    );
    assert_eq!(
        allowed(Platform::Docker, HostOs::Windows),
        Err("docker through WSL2 is not designed yet")
    );
    assert_eq!(
        allowed(Platform::Lima, HostOs::Linux),
        Err("lima runs on macOS hosts only")
    );
    for platform in Platform::ALL {
        let sqlite = sandbox_allowed(platform, HostOs::Macos, StoreKind::Sqlite);
        assert_eq!(
            sqlite,
            disabled_reason(platform.name()).map_or(Ok(()), Err),
            "{platform:?}"
        );
    }
}

#[test]
fn rows_carry_the_profile_on_hosts_it_is_on() {
    let lima = support(Platform::Lima, HostOs::Macos);
    assert_eq!(lima.backend, Some(Backend::Namespaces));
    assert_eq!(lima.kernel, Some(KernelRoute::Fuse));
    assert_eq!(lima.sqlite, Some(CallSite::Worker));
    assert_eq!(lima.options, Platform::Lima.profile().options);
    assert!(lima.description.starts_with("a Lima VM (vz); "));
    let native = support(Platform::Native, HostOs::Macos);
    assert_eq!(native.backend.map(Backend::stored_name), Some("native"));
    assert_eq!(native.kernel, Some(KernelRoute::Nfs));
    assert_eq!(native.sqlite, Some(CallSite::InProcess));
    let windows = support(Platform::Native, HostOs::Windows);
    assert_eq!(
        (windows.backend, windows.kernel, windows.sqlite),
        (None, None, Some(CallSite::Worker))
    );
    for host in HostOs::NAMED {
        assert_eq!(
            support(Platform::Docker, host).sqlite.is_some(),
            host != HostOs::Windows
        );
    }
    let away = support(Platform::Lima, HostOs::Linux);
    assert_eq!((away.backend, away.kernel, away.sqlite), (None, None, None));
    let names: Vec<&str> = platforms(HostOs::Linux)
        .iter()
        .map(|r| r.platform.name())
        .collect();
    assert_eq!(names, Platform::ALL.map(Platform::name));
}
