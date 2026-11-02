//! The kind table: each kind's name, backend, route and worker bind per host.
#![cfg(all(unix, feature = "machine"))]
use layerfs_sandbox::machine::{Backend, HostOs, Kind, Route};
use std::net::Ipv4Addr;

#[test]
fn every_kind_parses_back_from_its_name() {
    let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
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
    for kind in Kind::ALL {
        assert_eq!(Kind::parse(kind.name()), Some(kind));
    }
    assert_eq!(Kind::parse("vm:lima"), None);
    assert_eq!(Kind::parse("Lima"), None);
}

#[test]
fn backends_follow_the_kind_and_host() {
    assert_eq!(Kind::Native.backend(HostOs::Macos), Some(Backend::Native));
    assert_eq!(
        Kind::Native.backend(HostOs::Linux),
        Some(Backend::Namespaces)
    );
    assert_eq!(Kind::Native.backend(HostOs::Windows), None);
    for host in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
        assert_eq!(Kind::Docker.backend(host), Some(Backend::Docker));
        for kind in [
            Kind::AppleContainer,
            Kind::Microsandbox,
            Kind::Lima,
            Kind::Firecracker,
            Kind::Ssh,
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
fn routes_are_in_process_only_for_native_and_linux_docker() {
    assert_eq!(Kind::Native.route(HostOs::Linux), Route::InProcess);
    assert_eq!(Kind::Native.route(HostOs::Macos), Route::InProcess);
    assert_eq!(Kind::Native.route(HostOs::Windows), Route::Worker);
    assert_eq!(Kind::Docker.route(HostOs::Linux), Route::InProcess);
    assert_eq!(Kind::Docker.route(HostOs::Macos), Route::Worker);
    assert_eq!(Kind::Docker.route(HostOs::Windows), Route::Worker);
    for host in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
        for kind in [
            Kind::AppleContainer,
            Kind::Microsandbox,
            Kind::Lima,
            Kind::Firecracker,
            Kind::Ssh,
        ] {
            assert_eq!(kind.route(host), Route::Worker, "{kind:?} on {host:?}");
        }
    }
}

#[test]
fn workers_bind_loopback_where_the_forward_reaches_it() {
    assert_eq!(Kind::Lima.worker_bind(), Ipv4Addr::LOCALHOST);
    assert_eq!(Kind::Ssh.worker_bind(), Ipv4Addr::LOCALHOST);
    for kind in [
        Kind::AppleContainer,
        Kind::Microsandbox,
        Kind::Firecracker,
        Kind::Docker,
    ] {
        assert_eq!(kind.worker_bind(), Ipv4Addr::UNSPECIFIED, "{kind:?}");
    }
}

#[test]
fn built_in_created_and_tools() {
    let built_in: Vec<Kind> = Kind::ALL.into_iter().filter(|k| k.built_in()).collect();
    assert_eq!(built_in, [Kind::Native, Kind::Docker]);
    let created: Vec<Kind> = Kind::ALL.into_iter().filter(|k| k.created()).collect();
    assert_eq!(
        created,
        [
            Kind::AppleContainer,
            Kind::Microsandbox,
            Kind::Lima,
            Kind::Firecracker
        ]
    );
    assert_eq!(Kind::Native.tool(), None);
    assert_eq!(Kind::Firecracker.tool(), Some("limactl"));
    assert_eq!(Kind::AppleContainer.tool(), Some("container"));
    assert_eq!(Kind::Microsandbox.tool(), Some("msb"));
}

#[test]
fn an_idle_worker_resets_a_microvm_and_powers_other_vms_off() {
    // #533 review P0-6: a Firecracker VMM ignores an ACPI power-off
    assert_eq!(Kind::Firecracker.guest_stop(), "reboot");
    assert_eq!(Kind::Ssh.guest_stop(), "0");
    for kind in [Kind::Lima, Kind::AppleContainer, Kind::Microsandbox] {
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
