//! Probes name what each kind is missing; a stand-in tool reports its version.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{native_missing, nested_missing, probe, HostOs, Platform, Tools};
use layerfs_sandbox_backend::platform_policy::disabled_reason;
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
            found
                .missing
                .contains(&disabled_reason(kind.name()).unwrap_or(tool).to_owned()),
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
        if let Some(reason) = disabled_reason(kind.name()) {
            assert_eq!(found.missing, [reason]);
            continue;
        }
        assert_eq!(
            found.missing.contains(&"host:macos".to_owned()),
            !on_macos,
            "{kind:?}"
        );
    }
    let firecracker = probe(Platform::Firecracker, &tools);
    if disabled_reason("firecracker").is_none() {
        for fact in nested_missing(HostOs::current()) {
            assert!(firecracker.missing.contains(&fact));
        }
    } else {
        assert_eq!(
            firecracker.missing,
            [disabled_reason("firecracker").unwrap()]
        );
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
    assert!(!ssh.ready);
    assert_eq!(ssh.version, None);
    assert_eq!(ssh.missing, [disabled_reason("ssh").unwrap()]);
}

#[test]
fn msb_is_found_where_its_installer_puts_it() {
    let scratch = Scratch::new("probe-msb-home");
    let home = scratch.dir.join("home");
    let installed = home.join(".microsandbox/bin");
    std::fs::create_dir_all(&installed).unwrap();
    scratch.tool("msb", "echo 'msb 0.7.11'");
    std::fs::rename(scratch.bin().join("msb"), installed.join("msb")).unwrap();
    let mut tools = scratch.tools();
    assert!(!probe(Platform::Microsandbox, &tools).ready);
    tools.path = Tools::search_path(&tools.path, Some(&home));
    let dirs: Vec<_> = std::env::split_paths(&tools.path).collect();
    assert_eq!(dirs, [scratch.bin(), installed]);
    let msb = probe(Platform::Microsandbox, &tools);
    assert!(msb.ready, "{:?}", msb.missing);
    assert_eq!(msb.version.as_deref(), Some("msb 0.7.11"));
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

/// A provider call is answered when its command ends, not at the next
/// fixed 20 ms poll step (#66): a stand-in that exits at once is seen well
/// inside one step. Prints the median of each stand-in's probes (one
/// provider call each) with the host's load; only the quick one is judged.
#[test]
fn a_provider_call_ends_when_its_command_does() {
    let scratch = Scratch::new("probe-latency");
    let tools = scratch.tools();
    let median = |body: &str| {
        scratch.tool("msb", body);
        let mut ms: Vec<f64> = (0..9)
            .map(|_| {
                let started = std::time::Instant::now();
                let found = probe(Platform::Microsandbox, &tools);
                assert_eq!(found.version.as_deref(), Some("msb 0.7.8"));
                started.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        ms.sort_by(f64::total_cmp);
        ms[ms.len() / 2]
    };
    if disabled_reason("microsandbox").is_some() {
        return;
    }
    let quick = median("echo 'msb 0.7.8'");
    let slow = median("sleep 0.026; echo 'msb 0.7.8'");
    let load = std::process::Command::new("uptime")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default();
    println!(
        "PROVIDER_CALL {{\"quick_ms\":{quick:.2},\"sleep26_ms\":{slow:.2},\"uptime\":{load:?}}}"
    );
    assert!(quick < 20.0, "a quick provider call took {quick:.2} ms");
}
