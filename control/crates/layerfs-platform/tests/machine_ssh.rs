//! The ssh machine: strict host keys copied to the work folder, the checks
//! before any setup, the host identity, and disconnect, through stand-in
//! `ssh` and `ssh-keygen`. No host is reached.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, Platform, Spec};
use standin::{read, Scratch};
use std::os::unix::fs::PermissionsExt;

const KEY: &str = "[example.test]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample";

/// `ssh-keygen -F` finds the key; `-l -F` prints its fingerprint.
const KEYGEN: &str = r##"if [ "$1" = -l ]; then
  echo "# Host [example.test]:2222 found: line 1"
  echo "[example.test]:2222 ED25519 SHA256:abcdefFingerprint"
else
  echo "# Host [example.test]:2222 found: line 1"
  echo "[example.test]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample"
fi"##;

fn ssh_spec(scratch: &Scratch) -> Spec {
    std::fs::write(
        scratch.path("known_hosts"),
        format!("other.test ssh-rsa AAAA\n{KEY}\n"),
    )
    .unwrap();
    let mut spec = scratch.spec(Platform::Ssh);
    spec.ssh.as_mut().unwrap().port = 2222;
    spec
}

#[test]
#[ignore = "retained adapter qualification; platform permanently disabled by layerfs-project#7"]
fn connect_checks_the_host_key_kernel_and_data_root_before_setup() {
    let scratch = Scratch::new("ssh-connect");
    scratch.tool("ssh-keygen", KEYGEN);
    scratch.tool(
        "ssh",
        r#"case "$*" in
  *uname*) echo Linux ;;
  *findmnt*) echo "the data root /layerfs/data is on tmpfs, not ext4, xfs or btrfs" >&2; exit 4 ;;
esac"#,
    );
    let spec = ssh_spec(&scratch);
    let error = machine::connect(&spec, &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::Unsupported, "{error}");
    assert!(error.message.contains("tmpfs"));
    assert!(!error.vm_exists);
    let accepted = spec.work_dir.join("known_hosts");
    assert_eq!(
        read(&accepted),
        format!("{KEY}\n"),
        "only the host's own line is copied"
    );
    let mode = std::fs::metadata(&accepted).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let log = scratch.log();
    assert!(log.starts_with(&format!(
        "ssh-keygen -F [example.test]:2222 -f {}\n",
        scratch.path("known_hosts").display()
    )));
    let uname = log.lines().find(|l| l.contains("uname")).unwrap();
    assert_eq!(
        uname,
        format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=15 -o StrictHostKeyChecking=yes -o UserKnownHostsFile={} -p 2222 root@example.test -- uname -s",
            accepted.display()
        )
    );
    assert!(
        !log.contains(" -L "),
        "no forward before the data root passes"
    );
}

#[test]
#[ignore = "retained adapter qualification; platform permanently disabled by layerfs-project#7"]
fn an_unknown_host_key_is_denied() {
    let scratch = Scratch::new("ssh-denied");
    scratch.tool("ssh-keygen", "exit 1");
    scratch.tool("ssh", "echo Linux");
    let error = machine::connect(&ssh_spec(&scratch), &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::Denied, "{error}");
    assert!(
        !scratch.log().contains("\nssh "),
        "nothing reached the host"
    );
}

#[test]
#[ignore = "retained adapter qualification; platform permanently disabled by layerfs-project#7"]
fn a_host_that_is_not_linux_is_unsupported() {
    let scratch = Scratch::new("ssh-bsd");
    scratch.tool("ssh-keygen", KEYGEN);
    scratch.tool("ssh", "echo FreeBSD");
    let mut spec = ssh_spec(&scratch);
    spec.ssh.as_mut().unwrap().identity = Some(scratch.path("id_ed25519"));
    let error = machine::connect(&spec, &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::Unsupported);
    assert!(error.message.contains("FreeBSD"));
    assert!(scratch
        .log()
        .contains(&format!("-i {}", scratch.path("id_ed25519").display())));
}

#[test]
fn identity_is_the_host_key_fingerprint() {
    let scratch = Scratch::new("ssh-identity");
    scratch.tool("ssh-keygen", KEYGEN);
    let spec = ssh_spec(&scratch);
    assert_eq!(
        machine::host_identity(&spec).unwrap(),
        "SHA256:abcdefFingerprint"
    );
    let lima = scratch.spec(Platform::Lima);
    assert_eq!(
        machine::host_identity(&lima).unwrap_err().code,
        Code::InvalidInput
    );
}

#[test]
fn identity_falls_back_to_the_machine_id() {
    let scratch = Scratch::new("ssh-machine-id");
    scratch.tool("ssh-keygen", "exit 0");
    scratch.tool(
        "ssh",
        r#"case "$*" in *machine-id*) echo 0123456789abcdef ;; esac"#,
    );
    let spec = ssh_spec(&scratch);
    assert_eq!(
        machine::host_identity(&spec).unwrap(),
        "machine-id:0123456789abcdef"
    );
}

#[test]
fn disconnect_removes_bundle_and_state_but_never_the_data_root() {
    let scratch = Scratch::new("ssh-disconnect");
    scratch.tool("ssh", "exit 0");
    let mut spec = ssh_spec(&scratch);
    spec.ssh.as_mut().unwrap().data_root = "/srv/layerfs".into();
    machine::remove(&spec).unwrap();
    let log = scratch.log();
    assert!(
        log.contains("exit 0' sh /layerfs/bundle /layerfs/state /srv/layerfs\n"),
        "{log}"
    );
    assert!(!log.contains("rm -rf /srv"));
}

#[test]
fn status_without_a_running_forward_is_stopped() {
    let scratch = Scratch::new("ssh-status");
    scratch.tool("ssh", "echo 'OpenSSH_9.9p1' >&2");
    let status = machine::status(&ssh_spec(&scratch)).unwrap();
    assert_eq!(status.state, machine::State::Stopped);
    assert_eq!(status.tool_version.as_deref(), Some("OpenSSH_9.9p1"));
    assert!(status.down_reason.unwrap().contains("forward"));
}
