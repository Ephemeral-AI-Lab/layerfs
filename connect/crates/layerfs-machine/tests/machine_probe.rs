//! Probes name what each kind is missing; a stand-in tool reports its version.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_machine::{native_missing, nested_missing, probe, HostOs, Platform};
use standin::Scratch;

#[test]
fn an_empty_search_path_names_every_missing_tool() {
    let scratch = Scratch::new("probe-empty");
    let tools = scratch.tools();
    for (kind, tool) in [
        (Platform::Docker, "tool:docker"),
        (Platform::AppleContainer, "tool:container"),
        (Platform::Microsandbox, "tool:msb"),
        (Platform::Lima, "tool:limactl"),
        (Platform::Firecracker, "tool:limactl"),
        (Platform::Ssh, "tool:ssh"),
    ] {
        let found = probe(kind, &tools);
        assert_eq!(found.kind, kind);
        assert!(!found.ready, "{kind:?}");
        assert!(
            found.missing.contains(&tool.to_owned()),
            "{kind:?}: {:?}",
            found.missing
        );
        assert_eq!(found.version, None);
    }
}

#[test]
fn macos_only_kinds_name_the_host_elsewhere() {
    let scratch = Scratch::new("probe-host");
    let tools = scratch.tools();
    let on_macos = HostOs::current() == HostOs::Macos;
    for kind in [
        Platform::Lima,
        Platform::Firecracker,
        Platform::AppleContainer,
    ] {
        let found = probe(kind, &tools);
        assert_eq!(
            found.missing.contains(&"host:macos".to_owned()),
            !on_macos,
            "{kind:?}"
        );
    }
    let firecracker = probe(Platform::Firecracker, &tools);
    for fact in nested_missing(HostOs::current()) {
        assert!(firecracker.missing.contains(&fact));
    }
}

#[test]
fn a_stand_in_tool_reports_its_version() {
    let scratch = Scratch::new("probe-version");
    scratch.tool("limactl", "echo 'limactl version 9.9.9'");
    scratch.tool("msb", "echo 'msb 0.7.8'");
    scratch.tool("ssh", "echo 'OpenSSH_9.9p1' >&2");
    let tools = scratch.tools();
    let lima = probe(Platform::Lima, &tools);
    assert!(!lima.missing.contains(&"tool:limactl".to_owned()));
    assert_eq!(lima.version.as_deref(), Some("limactl version 9.9.9"));
    assert_eq!(lima.ready, HostOs::current() == HostOs::Macos);
    let msb = probe(Platform::Microsandbox, &tools);
    assert!(msb.ready);
    assert_eq!(msb.version.as_deref(), Some("msb 0.7.8"));
    let ssh = probe(Platform::Ssh, &tools);
    assert!(ssh.ready);
    assert_eq!(ssh.version.as_deref(), Some("OpenSSH_9.9p1"));
}

#[test]
fn docker_without_an_engine_names_its_socket() {
    let scratch = Scratch::new("probe-docker");
    scratch.tool(
        "docker",
        "echo 'Cannot connect to the Docker daemon' >&2; exit 1",
    );
    let mut tools = scratch.tools();
    tools.docker_socket = scratch.path("docker.sock");
    let found = probe(Platform::Docker, &tools);
    assert!(!found.ready);
    assert_eq!(
        found.missing,
        [format!("host:{}", tools.docker_socket.display())]
    );
    assert!(scratch.log().contains(&format!(
        "-H unix://{} version",
        tools.docker_socket.display()
    )));
}

#[test]
fn native_names_only_linux_host_facts() {
    let found = probe(Platform::Native, &Scratch::new("probe-native").tools());
    assert_eq!(found.missing, native_missing(HostOs::current()));
    match HostOs::current() {
        HostOs::Macos => assert!(found.ready),
        HostOs::Linux => {
            for fact in &found.missing {
                assert!(
                    ["host:root", "host:/dev/fuse", "host:cgroup2"].contains(&fact.as_str()),
                    "{fact}"
                );
            }
        }
        _ => assert!(!found.ready),
    }
    assert_eq!(native_missing(HostOs::Windows), ["host:macos-or-linux"]);
    assert!(native_missing(HostOs::Macos).is_empty());
    assert!(nested_missing(HostOs::Linux).is_empty());
}
