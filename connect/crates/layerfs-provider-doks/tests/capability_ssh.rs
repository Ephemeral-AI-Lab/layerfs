//! #554 acceptance 6: the DOKS provider declares `provider: doks`, keeps
//! `kind` an isolation class, and has exactly the built-in ssh machine's
//! capability.  No test touches a cluster.
#![cfg(unix)]

use layerfs_machine::{HostOs, Platform, Tools};
use layerfs_provider_doks::{
    capability, capability_for, declaration, pod_ip, preflight, record_fields, spec, DoksEnv,
};
use layerfs_sandbox::{Backend, ExecMode, IsolationClass, KernelScope};
use std::path::{Path, PathBuf};

fn env() -> DoksEnv {
    DoksEnv::from_pairs([
        ("DOKS_NAMESPACE", "layerfs"),
        ("DOKS_POD", "layerfs-0"),
        ("DOKS_PVC", "layerfs-data"),
        ("DOKS_SSH_HOST", "10.0.0.7"),
        ("DOKS_SSH_USER", "root"),
        ("DOKS_SSH_PORT", "22"),
        ("DOKS_KNOWN_HOSTS", "/tmp/known_hosts"),
    ])
    .unwrap()
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-doks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_kubectl(dir: &Path) -> PathBuf {
    let path = dir.join("kubectl");
    std::fs::write(
        &path,
        "#!/bin/sh\ncase \"$7\" in\n  *podIP*) printf '10.0.0.7' ;;\n  *phase*) case \"$2\" in\n    pod) printf 'Running' ;;\n    pvc) printf 'Bound' ;;\n  esac ;;\n  *) printf 'unknown' ;;\nesac\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn provider_name_is_data_not_a_platform_variant() {
    assert!(Platform::parse("doks").is_none());
    assert!(Platform::parse("digitalocean").is_none());
    let got = declaration();
    assert_eq!(got.provider, "doks");
    assert_eq!(got.platform, Platform::Ssh);
    assert_eq!(got.platform.name(), "ssh");
    assert_eq!(got.kind, IsolationClass::Remote);
    assert_eq!(got.exec_mode, ExecMode::Worker);
    assert_eq!(got.kernel, KernelScope::SeparateHost);
    assert_eq!(got.capability, capability());
}

#[test]
fn capability_is_identical_to_the_ssh_machine() {
    for host in HostOs::NAMED {
        let backend = Platform::Ssh
            .backend(host)
            .expect("the ssh platform declares a backend on every named host");
        assert_eq!(backend, Backend::Namespaces, "{host:?}");
        let ssh = backend.capabilities();
        assert_eq!(capability_for(host), ssh, "{host:?}");
        assert_eq!(ssh.isolation_class(), IsolationClass::Vm);
        assert_eq!(Platform::Ssh.isolation(host), Some(IsolationClass::Remote));
        assert_eq!(ssh.kernel_scope(), KernelScope::Separate);
        assert_eq!(
            Platform::Ssh.kernel_scope(host),
            Some(KernelScope::SeparateHost)
        );
        assert_eq!(Platform::Ssh.exec_mode(host), ExecMode::Worker);
    }
}

#[test]
fn the_spec_is_the_builtin_ssh_machine() {
    let env = env();
    let spec = spec(
        "doks-dev",
        PathBuf::from("/tmp/doks-dev"),
        Tools::from_env(),
        &env,
    );
    assert_eq!(spec.kind, Platform::Ssh);
    assert_eq!(spec.vm_name, "doks-layerfs-layerfs-0");
    assert_eq!(
        spec.kind.isolation(HostOs::current()),
        Some(IsolationClass::Remote)
    );
    let ssh = spec.ssh.as_ref().expect("the ssh route");
    assert_eq!(ssh.host, "10.0.0.7");
    assert_eq!(ssh.user, "root");
    assert_eq!(ssh.port, 22);
    assert_eq!(ssh.data_root, "/layerfs/data");
    assert_eq!(ssh.known_hosts, PathBuf::from("/tmp/known_hosts"));
    assert!(spec.validate().is_ok());
}

#[test]
fn the_record_fields_keep_kind_and_provider_apart() {
    let fields = record_fields(&env());
    assert_eq!(fields.provider, "doks");
    assert_eq!(fields.platform, "ssh");
    assert_eq!(fields.kind, "remote");
    assert_eq!(fields.isolation_class, "remote");
    assert_eq!(fields.exec_mode, "worker");
    assert_eq!(fields.kernel, "separate-host");
    assert_eq!(fields.namespace, "layerfs");
    assert_eq!(fields.pod, "layerfs-0");
    assert_eq!(fields.pvc, "layerfs-data");
    assert_eq!(fields.ssh_port, 22);
    let pairs = fields.pairs();
    assert!(pairs
        .iter()
        .any(|(key, value)| *key == "provider" && value.as_str() == "doks"));
    assert!(pairs
        .iter()
        .any(|(key, value)| *key == "kind" && value.as_str() == "remote"));
}

#[test]
fn preflight_reads_the_pod_and_pvc() {
    let dir = temp_dir("preflight");
    let kubectl = fake_kubectl(&dir);
    let env = env();
    let got = preflight(&kubectl, &env).unwrap();
    assert_eq!(got.pod_phase, "Running");
    assert_eq!(got.pvc_phase, "Bound");
    assert_eq!(
        pod_ip(&kubectl, &env.namespace, &env.pod).unwrap(),
        "10.0.0.7"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_environment_refuses_a_route_it_cannot_map() {
    let missing_host =
        DoksEnv::from_pairs([("DOKS_POD", "layerfs-0"), ("DOKS_PVC", "layerfs-data")]).unwrap_err();
    assert!(missing_host.message().contains("DOKS_SSH_HOST"));

    let suite = [
        ("DOKS_SSH_PORT", "0"),
        ("DOKS_SSH_PORT", "70000"),
        ("DOKS_DATA_ROOT", "layerfs/data"),
        ("DOKS_NAMESPACE", "Bad_Name"),
        ("DOKS_SSH_USER", "-root"),
    ];
    for (key, value) in suite {
        let mut pairs = vec![
            ("DOKS_POD", "layerfs-0"),
            ("DOKS_PVC", "layerfs-data"),
            ("DOKS_SSH_HOST", "10.0.0.7"),
            ("DOKS_KNOWN_HOSTS", "/tmp/known_hosts"),
        ];
        pairs.push((key, value));
        assert!(
            DoksEnv::from_pairs(pairs).is_err(),
            "{key}={value} was accepted"
        );
    }
}
