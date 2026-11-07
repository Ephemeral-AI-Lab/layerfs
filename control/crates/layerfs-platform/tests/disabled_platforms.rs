//! Product policy applies before any tool or bundle access; adapters remain compiled.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, HostOs, Platform, StoreKind};
use layerfs_sandbox_backend::platform_policy::disabled_reason;
use standin::Scratch;

#[test]
fn disabled_on_every_host_and_store_kind_without_probing() {
    for platform in [Platform::Firecracker, Platform::Ssh] {
        let reason = disabled_reason(platform.name()).unwrap();
        for host in HostOs::NAMED.into_iter().chain([HostOs::Other]) {
            let row = machine::support(platform, host);
            assert_eq!((row.backend, row.kernel, row.sqlite), (None, None, None));
            assert!(row.options.is_empty());
            assert_eq!(row.description, reason);
            for store in [StoreKind::Sqlite, StoreKind::Postgresql] {
                assert_eq!(machine::sandbox_allowed(platform, host, store), Err(reason));
            }
        }
        let scratch = Scratch::new("disabled-probe");
        for tool in ["ssh", "limactl"] {
            scratch.tool(tool, "exit 99");
        }
        let probe = machine::probe(platform, &scratch.tools());
        assert!(!probe.ready);
        assert_eq!(probe.missing, [reason]);
        assert_eq!(probe.version, None);
        assert!(scratch.log().is_empty());
    }
}

#[test]
fn new_work_cannot_reach_retained_adapters() {
    for platform in [Platform::Firecracker, Platform::Ssh] {
        let scratch = Scratch::new("disabled-work");
        let spec = scratch.spec(platform); // deliberately lacks old platform-specific setup
        let bundle = scratch.bundle();
        let outcomes = [
            machine::create(&spec, &bundle).map(drop),
            machine::connect(&spec, &bundle).map(drop),
            machine::start(&spec, &bundle).map(drop),
            machine::forward(&spec, 1234).map(drop),
            machine::copy_in(&spec, &scratch.dir, None, "/data"),
            machine::copy_out(&spec, "/data/store", &scratch.path("out")),
        ];
        for outcome in outcomes {
            let error = outcome.unwrap_err();
            assert_eq!(error.code, Code::Unsupported);
            assert_eq!(error.message, disabled_reason(platform.name()).unwrap());
            assert!(!error.vm_exists);
        }
        assert!(scratch.log().is_empty());
        assert!(!scratch.path("out").exists());
    }
}
