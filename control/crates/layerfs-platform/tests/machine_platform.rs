//! The platform table: each profile's name, backend, kernel route, executor,
//! worker bind, hosts and options per host.
#![cfg(unix)]
use layerfs_platform::{Backend, HostOs, Platform, Route};
use std::net::Ipv4Addr;

#[test]
fn every_kind_parses_back_from_its_name() {
    let names: Vec<&str> = Platform::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(names, ["native", "docker", "apple-container", "lima"]);
    for kind in Platform::ALL {
        assert_eq!(Platform::parse(kind.name()), Some(kind));
    }
    assert_eq!(Platform::parse("vm:lima"), None);
    assert_eq!(Platform::parse("Lima"), None);
    // removed in 2.0: a record may still name one, but no platform parses
    for removed in ["firecracker", "microsandbox", "ssh"] {
        assert_eq!(Platform::parse(removed), None);
    }
}

#[test]
fn backends_follow_the_kind_and_host() {
    assert_eq!(
        Platform::Native.backend(HostOs::Macos),
        Some(Backend::HostProcess)
    );
    assert_eq!(
        Platform::Native.backend(HostOs::Linux),
        Some(Backend::Namespaces)
    );
    assert_eq!(Platform::Native.backend(HostOs::Windows), None);
    for host in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
        for kind in [Platform::Docker, Platform::AppleContainer, Platform::Lima] {
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
        for kind in [Platform::AppleContainer, Platform::Lima] {
            assert_eq!(kind.route(host), Route::Worker, "{kind:?} on {host:?}");
        }
    }
}

#[test]
fn workers_bind_loopback_where_the_forward_reaches_it() {
    assert_eq!(Platform::Lima.worker_bind(), Ipv4Addr::LOCALHOST);
    for kind in [Platform::AppleContainer, Platform::Docker] {
        assert_eq!(kind.worker_bind(), Ipv4Addr::UNSPECIFIED, "{kind:?}");
    }
}

#[test]
fn built_in_created_and_tools() {
    let built_in: Vec<Platform> = Platform::ALL.into_iter().filter(|k| k.built_in()).collect();
    assert_eq!(built_in, [Platform::Native, Platform::Docker]);
    let created: Vec<Platform> = Platform::ALL.into_iter().filter(|k| k.created()).collect();
    assert_eq!(created, [Platform::AppleContainer, Platform::Lima]);
    assert_eq!(Platform::Native.tool(), None);
    assert_eq!(Platform::Lima.tool(), Some("limactl"));
    assert_eq!(Platform::AppleContainer.tool(), Some("container"));
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

#[test]
fn each_profile_names_its_platform_and_how() {
    use layerfs_platform::How;
    for kind in Platform::ALL {
        let profile = kind.profile();
        assert_eq!(profile.name, kind.name());
        assert!(!profile.description.is_empty(), "{kind:?}");
        let how = match kind {
            Platform::Native | Platform::Docker => How::BuiltIn,
            _ => How::Created,
        };
        assert_eq!(profile.how, how, "{kind:?}");
    }
    assert_eq!(How::BuiltIn.as_str(), "built_in");
    assert_eq!(How::Created.as_str(), "created");
}

#[test]
fn kernel_routes_follow_the_backend() {
    use layerfs_platform::KernelRoute;
    assert_eq!(
        Platform::Native.kernel(HostOs::Macos),
        Some(KernelRoute::Nfs)
    );
    assert_eq!(
        Platform::Native.kernel(HostOs::Linux),
        Some(KernelRoute::Fuse)
    );
    assert_eq!(Platform::Native.kernel(HostOs::Windows), None);
    for host in HostOs::NAMED {
        for kind in Platform::ALL.into_iter().skip(1) {
            assert_eq!(kind.kernel(host), Some(KernelRoute::Fuse), "{kind:?}");
        }
    }
    assert_eq!(KernelRoute::Fuse.as_str(), "fuse");
    assert_eq!(KernelRoute::Nfs.as_str(), "nfs");
}

#[test]
fn hosts_follow_todays_host_rules() {
    let hosts = |kind: Platform| kind.profile().hosts.to_vec();
    let any = HostOs::NAMED.to_vec();
    assert_eq!(hosts(Platform::Native), any);
    assert_eq!(hosts(Platform::Docker), [HostOs::Macos, HostOs::Linux]);
    for kind in [Platform::AppleContainer, Platform::Lima] {
        assert_eq!(hosts(kind), [HostOs::Macos], "{kind:?}");
    }
    let names: Vec<&str> = HostOs::NAMED.iter().map(|h| h.as_str()).collect();
    assert_eq!(names, ["macos", "linux", "windows"]);
}

#[test]
fn options_are_what_create_takes() {
    let options = |kind: Platform| -> Vec<(&str, &str)> {
        kind.profile()
            .options
            .iter()
            .map(|o| (o.name, o.kind.as_str()))
            .collect()
    };
    let vm = [
        ("cpus", "int"),
        ("memory", "size"),
        ("disk", "size"),
        ("image", "text"),
        ("os_image", "text"),
    ];
    assert_eq!(options(Platform::Native), []);
    assert_eq!(options(Platform::Docker), []);
    assert_eq!(options(Platform::AppleContainer), vm);
    assert_eq!(options(Platform::Lima), vm);
}
