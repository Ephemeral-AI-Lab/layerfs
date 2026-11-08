//! Each platform's adapter refuses what its platform does not do, before
//! any effect and with the same text the calls gave before the adapters.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, MachineError, Platform};
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
