//! Probes name what each kind is missing; a stand-in tool reports its version.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{native_missing, probe, HostOs, Platform};
use standin::Scratch;

#[test]
fn an_empty_search_path_names_every_missing_tool() {
    let scratch = Scratch::new("probe-empty");
    let tools = scratch.tools();
    for (kind, tool) in [
        (Platform::Docker, "tool:docker"),
        (Platform::AppleContainer, "tool:container"),
        (Platform::Lima, "tool:limactl"),
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
    for kind in [Platform::Lima, Platform::AppleContainer] {
        let found = probe(kind, &tools);
        assert_eq!(
            found.missing.contains(&"host:macos".to_owned()),
            !on_macos,
            "{kind:?}"
        );
    }
}

#[test]
fn a_stand_in_tool_reports_its_version() {
    let scratch = Scratch::new("probe-version");
    scratch.tool("limactl", "echo 'limactl version 9.9.9'");
    let tools = scratch.tools();
    let lima = probe(Platform::Lima, &tools);
    assert!(!lima.missing.contains(&"tool:limactl".to_owned()));
    assert_eq!(lima.version.as_deref(), Some("limactl version 9.9.9"));
    assert_eq!(lima.ready, HostOs::current() == HostOs::Macos);
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
}

/// A provider call is answered when its command ends, not at the next
/// fixed 20 ms poll step (#66): a stand-in that exits at once is seen well
/// inside one step. Prints the median of each stand-in's probes (one
/// provider call each) with the host's load; only the quick one is judged.
#[test]
fn a_provider_call_ends_when_its_command_does() {
    let scratch = Scratch::new("probe-latency");
    let tools = scratch.tools();
    let median = |body: &str| {
        scratch.tool("limactl", body);
        let mut ms: Vec<f64> = (0..9)
            .map(|_| {
                let started = std::time::Instant::now();
                let found = probe(Platform::Lima, &tools);
                assert_eq!(found.version.as_deref(), Some("limactl version 9.9.9"));
                started.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        ms.sort_by(f64::total_cmp);
        ms[ms.len() / 2]
    };
    let quick = median("echo 'limactl version 9.9.9'");
    let slow = median("sleep 0.026; echo 'limactl version 9.9.9'");
    let load = std::process::Command::new("uptime")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default();
    println!(
        "PROVIDER_CALL {{\"quick_ms\":{quick:.2},\"sleep26_ms\":{slow:.2},\"uptime\":{load:?}}}"
    );
    assert!(quick < 20.0, "a quick provider call took {quick:.2} ms");
}
