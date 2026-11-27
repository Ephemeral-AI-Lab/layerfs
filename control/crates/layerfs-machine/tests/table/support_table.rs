//! The one support table: `control/SUPPORT.json` is its rendering, and every
//! refusal the check gives is the row's reason. `LAYERFS_BLESS_SUPPORT=1`
//! writes the file from the table instead of comparing.
#![cfg(unix)]
use {layerfs_machine::{platform::{self, sandbox_allowed, support_json, Backend, CallSite, HostOs, Platform}, table::support}, layerfs_protocol::{self, self}};
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
            let postgresql = sandbox_allowed(row.platform, host, layerfs_protocol::Postgresql);
            assert_eq!(row.postgresql.sandbox, postgresql.is_ok(), "{row:?}");
            assert_eq!(row.postgresql.reason, postgresql.err(), "{row:?}");
            if let Err(reason) = sandbox_allowed(row.platform, host, layerfs_protocol::Sqlite) {
                assert_eq!(Some(reason), row.postgresql.reason, "{row:?}");
                assert!(!row.supports().sqlite);
            }
            assert_eq!(row.postgresql.store_calls, CallSite::InProcess);
            assert_eq!(row.supports().reason, row.postgresql.reason);
        }
    }
}

/// A machine's `supports.log` (layerfs-project#178): true where its calls
/// go to a worker this build starts, so a worker log exists to read. Never
/// on `native`, Windows included (its WSL2 worker is not designed), and
/// never for a platform that is not on the host.
#[test]
fn a_worker_log_is_supported_where_this_build_starts_the_worker() {
    let log = |platform, host| support(platform, host).supports().log;
    for host in HostOs::NAMED.into_iter().chain([HostOs::Other]) {
        assert!(!log(Platform::Host, host), "native on {host:?}");
        for row in platforms(host) {
            let worker = row.sqlite == Some(CallSite::Worker);
            let allowed = sandbox_allowed(row.platform, host, layerfs_protocol::Sqlite).is_ok();
            assert_eq!(row.supports().log, worker && allowed, "{row:?}");
        }
    }
    for platform in [Platform::Docker, Platform::Lima, Platform::AppleContainer] {
        assert!(log(platform, HostOs::Macos), "{platform:?} on macOS");
        assert!(!log(platform, HostOs::Windows), "{platform:?} on Windows");
        assert!(!log(platform, HostOs::Other), "{platform:?} elsewhere");
    }
    assert!(log(Platform::Docker, HostOs::Linux));
    assert!(!log(Platform::Lima, HostOs::Linux));
    assert!(!log(Platform::AppleContainer, HostOs::Linux));
    // Windows `native` names a worker as its call site, and still has none
    let windows = support(Platform::Host, HostOs::Windows);
    assert_eq!(windows.sqlite, Some(CallSite::Worker));
    assert!(!windows.supports().log);
}

/// #535 §7.3: PostgreSQL Sandboxes run on native (macOS, and Linux
/// namespaces with no bind; phase 5a), docker, and since the G7 live
/// receipts on lima and apple-container, on every host each is on.
#[test]
fn postgresql_sandboxes_run_on_every_platform_with_a_backend() {
    let allowed = |p, h| sandbox_allowed(p, h, layerfs_protocol::Postgresql);
    for host in [HostOs::Macos, HostOs::Linux] {
        assert_eq!(allowed(Platform::Host, host), Ok(()));
        assert_eq!(allowed(Platform::Docker, host), Ok(()));
    }
    for platform in [Platform::Lima, Platform::AppleContainer] {
        assert_eq!(allowed(platform, HostOs::Macos), Ok(()), "{platform:?}");
    }
    assert_eq!(
        allowed(Platform::Host, HostOs::Windows),
        Err("no Sandbox backend on Windows")
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
        let sqlite = sandbox_allowed(platform, HostOs::Macos, layerfs_protocol::Sqlite);
        assert_eq!(sqlite, Ok(()), "{platform:?}");
    }
}

#[test]
fn rows_carry_the_profile_on_hosts_it_is_on() {
    let lima = support(Platform::Lima, HostOs::Macos);
    assert_eq!(lima.backend, Some(Backend::Namespaces));
    assert_eq!(lima.kernel, Some(layerfs_protocol::Fuse));
    assert_eq!(lima.sqlite, Some(CallSite::Worker));
    assert_eq!(lima.options, Platform::Lima.profile().options);
    assert!(lima.description.starts_with("a Lima VM (vz); "));
    let native = support(Platform::Host, HostOs::Macos);
    assert_eq!(native.backend.map(Backend::stored_name), Some("process"));
    assert_eq!(native.kernel, Some(layerfs_protocol::Nfs));
    assert_eq!(native.sqlite, Some(CallSite::InProcess));
    let windows = support(Platform::Host, HostOs::Windows);
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
