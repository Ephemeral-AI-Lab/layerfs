//! Behaviour asks a machine's platform, not its name: the machine whose
//! calls run in this process, and the built-in machines' state without a
//! record.
#![cfg(feature = "machine")]
use layerfs_home::reply::state::State;
use layerfs_machine::status::state::unrecorded;
use layerfs_machine::{in_process, provided};

#[test]
fn native_runs_in_this_process_on_macos_and_linux() {
    let expected = cfg!(any(target_os = "macos", target_os = "linux"))
        .then_some(layerfs_home::names::HOST_MACHINE);
    assert_eq!(in_process(), expected);
}

#[test]
fn a_provider_holds_every_platform_but_native() {
    assert!(!provided("host"));
    for platform in ["docker", "lima", "apple-container", "firecracker"] {
        assert!(provided(platform), "{platform}");
    }
}

#[test]
fn built_in_machines_without_a_record_follow_their_platform() {
    assert!(matches!(
        unrecorded(layerfs_home::names::HOST_MACHINE),
        State::Ready
    ));
    assert!(matches!(unrecorded("docker"), State::Stopped));
    assert!(matches!(unrecorded("dev"), State::Broken { .. }));
}
