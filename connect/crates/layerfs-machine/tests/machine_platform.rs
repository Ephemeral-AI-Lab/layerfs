//! The kind table: each kind's name, backend, route and worker bind per host.
#![cfg(unix)]
use layerfs_machine::{Backend, HostOs, Platform, Route};
use std::net::Ipv4Addr;

#[test]
fn every_kind_parses_back_from_its_name() {
    let names: Vec<&str> = Platform::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "native",
            "docker",
            "apple-container",
            "microsandbox",
            "lima",
            "firecracker",
            "ssh"
        ]
    );
    for kind in Platform::ALL {
        assert_eq!(Platform::parse(kind.name()), Some(kind));
    }
    assert_eq!(Platform::parse("vm:lima"), None);
    assert_eq!(Platform::parse("Lima"), None);
}

#[test]
fn backends_follow_the_kind_and_host() {
    assert_eq!(
        Platform::Native.backend(HostOs::Macos),
        Some(Backend::Native)
    );
    assert_eq!(
        Platform::Native.backend(HostOs::Linux),
        Some(Backend::Namespaces)
    );
    assert_eq!(Platform::Native.backend(HostOs::Windows), None);
    for host in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
        assert_eq!(Platform::Docker.backend(host), Some(Backend::Docker));
        for kind in [
            Platform::AppleContainer,
            Platform::Microsandbox,
            Platform::Lima,
            Platform::Firecracker,
            Platform::Ssh,
        ] {
            assert_eq!(
                kind.backend(host),
                Some(Backend::Namespaces),
                "{kind:?} on {host:?}"
            );
        }
    }
}

#[test]
fn routes_are_in_process_only_for_native() {
    assert_eq!(Platform::Native.route(HostOs::Linux), Route::InProcess);
    assert_eq!(Platform::Native.route(HostOs::Macos), Route::InProcess);
    assert_eq!(Platform::Native.route(HostOs::Windows), Route::Worker);
    // the terminal pass: a Linux docker Store lives in a Docker volume, behind a worker
    assert_eq!(Platform::Docker.route(HostOs::Linux), Route::Worker);
    assert_eq!(Platform::Docker.route(HostOs::Macos), Route::Worker);
    assert_eq!(Platform::Docker.route(HostOs::Windows), Route::Worker);
    for host in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
        for kind in [
            Platform::AppleContainer,
            Platform::Microsandbox,
            Platform::Lima,
            Platform::Firecracker,
            Platform::Ssh,
        ] {
            assert_eq!(kind.route(host), Route::Worker, "{kind:?} on {host:?}");
        }
    }
}

#[test]
fn workers_bind_loopback_where_the_forward_reaches_it() {
    assert_eq!(Platform::Lima.worker_bind(), Ipv4Addr::LOCALHOST);
    assert_eq!(Platform::Ssh.worker_bind(), Ipv4Addr::LOCALHOST);
    for kind in [
        Platform::AppleContainer,
        Platform::Microsandbox,
        Platform::Firecracker,
        Platform::Docker,
    ] {
        assert_eq!(kind.worker_bind(), Ipv4Addr::UNSPECIFIED, "{kind:?}");
    }
}

#[test]
fn built_in_created_and_tools() {
    let built_in: Vec<Platform> = Platform::ALL.into_iter().filter(|k| k.built_in()).collect();
    assert_eq!(built_in, [Platform::Native, Platform::Docker]);
    let created: Vec<Platform> = Platform::ALL.into_iter().filter(|k| k.created()).collect();
    assert_eq!(
        created,
        [
            Platform::AppleContainer,
            Platform::Microsandbox,
            Platform::Lima,
            Platform::Firecracker
        ]
    );
    assert_eq!(Platform::Native.tool(), None);
    assert_eq!(Platform::Firecracker.tool(), Some("limactl"));
    assert_eq!(Platform::AppleContainer.tool(), Some("container"));
    assert_eq!(Platform::Microsandbox.tool(), Some("msb"));
}

#[test]
fn an_idle_worker_resets_a_microvm_and_powers_other_vms_off() {
    // #533 review P0-6: a Firecracker VMM ignores an ACPI power-off
    assert_eq!(Platform::Firecracker.guest_stop(), "reboot");
    assert_eq!(Platform::Ssh.guest_stop(), "0");
    // the terminal pass: a container has no init and no CAP_SYS_BOOT
    assert_eq!(Platform::AppleContainer.guest_stop(), "init");
    for kind in [Platform::Lima, Platform::Microsandbox] {
        assert_eq!(kind.guest_stop(), "1", "{kind:?}");
    }
}

#[test]
fn the_current_host_is_named() {
    let expected = if cfg!(target_os = "linux") {
        HostOs::Linux
    } else if cfg!(target_os = "macos") {
        HostOs::Macos
    } else {
        HostOs::Other
    };
    assert_eq!(HostOs::current(), expected);
}
