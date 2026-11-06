//! Each platform's adapter refuses what its platform does not do, before
//! any effect and with the same text the calls gave before the adapters.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_machine::{self as machine, Code, MachineError, Platform};
use layerfs_sandbox::platform_policy::disabled_reason;
use standin::Scratch;

fn refusal<T: std::fmt::Debug>(outcome: Result<T, MachineError>) -> String {
    let error = outcome.unwrap_err();
    assert_eq!(error.code, Code::InvalidInput, "{error:?}");
    assert!(!error.vm_exists);
    error.message
}

#[test]
fn built_in_machines_are_never_created_or_removed() {
    let scratch = Scratch::new("adapter-built-in");
    let bundle = scratch.bundle();
    for kind in [Platform::Native, Platform::Docker] {
        let spec = scratch.spec(kind);
        let name = kind.name();
        assert_eq!(
            refusal(machine::create(&spec, &bundle)),
            format!("the {name} machine dev is built in and never created")
        );
        assert_eq!(
            refusal(machine::remove(&spec)),
            format!("the {name} machine dev is built in and never removed")
        );
        assert_eq!(
            refusal(machine::connect(&spec, &bundle)),
            format!("the {name} machine dev is created, not connected")
        );
    }
    assert_eq!(
        refusal(machine::copy_out(
            &scratch.spec(Platform::Native),
            "/x",
            &scratch.path("x")
        )),
        "the native machine dev runs in-process; its files are the host's"
    );
    assert!(scratch.log().is_empty());
}

#[test]
fn ssh_is_connected_and_only_ssh_has_a_host_identity() {
    let scratch = Scratch::new("adapter-ssh");
    let bundle = scratch.bundle();
    let error = machine::create(&scratch.spec(Platform::Ssh), &bundle).unwrap_err();
    assert_eq!(error.code, Code::Unsupported);
    assert_eq!(error.message, disabled_reason("ssh").unwrap());
    for kind in Platform::ALL.into_iter().filter(|k| *k != Platform::Ssh) {
        let mut spec = scratch.spec(kind);
        if kind == Platform::Firecracker {
            let mut host = scratch.spec(Platform::Lima);
            host.nested = true;
            spec.host = Some(Box::new(host));
        }
        assert_eq!(
            refusal(machine::host_identity(&spec)),
            format!("the {} machine dev has no ssh host", kind.name())
        );
        if let Some(reason) = disabled_reason(kind.name()) {
            let error = machine::connect(&spec, &bundle).unwrap_err();
            assert_eq!(error.code, Code::Unsupported);
            assert_eq!(error.message, reason);
        } else if kind.created() {
            assert_eq!(
                refusal(machine::connect(&spec, &bundle)),
                format!("the {} machine dev is created, not connected", kind.name())
            );
        }
    }
    assert!(scratch.log().is_empty());
}
