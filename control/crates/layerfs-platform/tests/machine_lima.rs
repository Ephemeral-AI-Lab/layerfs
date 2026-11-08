//! Lima commands and the generated template, through a stand-in `limactl`
//! (and a stand-in `ssh` for guest commands). No VM is made.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, HostOs, Platform, State};
use standin::{read, Scratch};

/// A `limactl` that knows no instance or disk, saves `create`'s stdin as
/// `template.json`, and fails `create` when `$SCRATCH/fail-create` exists.
const LIMACTL: &str = r#"case "$1" in
  list) exit 0 ;;
  disk) exit 0 ;;
  create) cat > "$SCRATCH/template.json"; [ -e "$SCRATCH/fail-create" ] && { echo "no image" >&2; exit 1; }; exit 0 ;;
  --version) echo "limactl version 9.9.9" ;;
esac
exit 0"#;

fn lima(label: &str) -> Scratch {
    let scratch = Scratch::new(label);
    scratch.tool("limactl", LIMACTL);
    scratch.tool("ssh", "exit 255");
    scratch
}

#[test]
fn create_writes_a_vz_template_with_the_data_disk_and_no_home_mount() {
    let scratch = lima("lima-create");
    let spec = scratch.spec(Platform::Lima);
    let error = machine::create(&spec, &scratch.bundle()).unwrap_err();
    if HostOs::current() != HostOs::Macos {
        assert_eq!(error.code, Code::Unsupported);
        assert!(error.missing.contains(&"host:macos".to_owned()));
        assert!(scratch.log().is_empty(), "nothing runs off macOS");
        return;
    }
    // the stand-in's `list` never shows the instance, so after the boot the guest is unreachable
    assert_eq!(error.code, Code::Unavailable, "{error}");
    assert!(error.vm_exists, "a failure after the VM exists keeps it");
    let template = read(&scratch.path("template.json"));
    assert!(template.contains("\"vmType\": \"vz\""), "{template}");
    assert!(template.contains("\"mounts\": []"));
    assert!(!template.contains("/layerfs/home"));
    assert!(!template.contains("nestedVirtualization"));
    assert!(template.contains("\"cpus\": 2"));
    assert!(template.contains("\"memory\": \"2048MiB\""));
    assert!(template.contains("\"guestIP\": \"127.0.0.1\", \"guestPort\": 7070, \"hostIP\": \"127.0.0.1\", \"hostPort\": 45678"));
    assert!(template.contains("\"ignore\": true"));
    assert!(template.contains("ubuntu-24.04-server-cloudimg-arm64.img"));
    let disk = template
        .split("\"additionalDisks\": [{\"name\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    assert_eq!(disk.len(), 11, "lima-{disk} must fit an ext4 label");
    assert!(disk.starts_with("lfs"));
    assert!(template.contains("\"format\": true, \"fsType\": \"ext4\""));
    let log = scratch.log();
    let calls: Vec<&str> = log.lines().collect();
    assert_eq!(
        calls[0],
        "limactl list --format {{.Status}}\t{{.Dir}} lfs-dev"
    );
    assert_eq!(calls[1], "limactl disk list --json");
    assert_eq!(
        calls[2],
        format!("limactl disk create {disk} --size 10240MiB --format raw")
    );
    assert_eq!(calls[3], "limactl create --name lfs-dev --tty=false -");
    assert_eq!(calls[4], "limactl start --tty=false lfs-dev");
    assert!(!log.contains("disk delete"), "a disk under a kept VM stays");
}

#[test]
fn a_create_that_fails_before_the_vm_exists_removes_its_disk() {
    if HostOs::current() != HostOs::Macos {
        return;
    }
    let scratch = lima("lima-fail");
    std::fs::write(scratch.path("fail-create"), b"").unwrap();
    let error = machine::create(&scratch.spec(Platform::Lima), &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::Failed);
    assert!(!error.vm_exists);
    assert!(error.message.contains("no image"), "{error}");
    let log = scratch.log();
    assert!(
        log.lines()
            .any(|l| l.starts_with("limactl disk delete lfs")),
        "{log}"
    );
    assert!(!log.contains("limactl start"));
}

#[test]
fn an_existing_instance_is_refused_and_left_alone() {
    let scratch = Scratch::new("lima-exists");
    scratch.tool(
        "limactl",
        "[ \"$1\" = list ] && printf 'Running\\t/tmp/lima/lfs-dev\\n'; exit 0",
    );
    let error = machine::create(&scratch.spec(Platform::Lima), &scratch.bundle()).unwrap_err();
    if HostOs::current() != HostOs::Macos {
        assert_eq!(error.code, Code::Unsupported);
        return;
    }
    assert_eq!(error.code, Code::Exists);
    assert_eq!(scratch.log().lines().count(), 1, "only the list ran");
}

/// A `limactl` whose instance runs once `create` made it.
const LIMACTL_MADE: &str = r#"case "$1" in
  list) [ -e "$SCRATCH/created" ] && printf 'Running\t%s\n' "$SCRATCH/lima-host"; exit 0 ;;
  create) cat > "$SCRATCH/template.json"; touch "$SCRATCH/created"; exit 0 ;;
  --version) echo "limactl version 9.9.9" ;;
esac
exit 0"#;

#[test]
fn a_restarted_guest_ends_only_the_bundle_s_worker() {
    // the terminal pass: after an idle stop, the old worker.pid named the
    // launch script's own shell, whose text holds "layerfs", and it killed
    // itself (exit 143)
    if HostOs::current() != HostOs::Macos {
        return;
    }
    let scratch = Scratch::new("lima-relaunch");
    scratch.tool("limactl", LIMACTL_MADE);
    scratch.tool(
        "ssh",
        r#"case "$*" in
  *"tar -C"*) cat > /dev/null ;;
  *max_user_namespaces*) echo 0 ;;
  *"setsid nohup"*) exit 7 ;;
esac
exit 0"#,
    );
    scratch.tool("tar", "exit 0");
    machine::create(&scratch.spec(Platform::Lima), &scratch.bundle()).unwrap_err();
    let log = scratch.log();
    let launch = log
        .split("ssh ")
        .find(|call| call.contains("setsid nohup"))
        .unwrap_or_else(|| panic!("no launch in:\n{log}"));
    assert!(!launch.contains("grep -q layerfs"), "{launch}");
    // the stand-in logs through `echo`, which turns the script's \0 into a NUL
    assert!(
        launch.contains(r#"[ "$p" != $$ ] && [ "$(tr '"#),
        "{launch}"
    );
    assert!(
        launch.contains(r#"< /proc/$p/cmdline 2>/dev/null)" = "$2/layerfs worker " ]"#),
        "{launch}"
    );
}

#[test]
fn an_image_url_or_path_replaces_the_default_image() {
    if HostOs::current() != HostOs::Macos {
        return;
    }
    let scratch = lima("lima-image");
    let mut spec = scratch.spec(Platform::Lima);
    spec.os_image = Some("/images/debian \"13\".img".into());
    let _ = machine::create(&spec, &scratch.bundle());
    let template = read(&scratch.path("template.json"));
    assert!(
        template.contains(r#""images": [{"location": "/images/debian \"13\".img"}]"#),
        "{template}"
    );
}

#[test]
fn status_reports_a_stopped_instance_and_the_tool_version() {
    let scratch = Scratch::new("lima-status");
    scratch.tool(
        "limactl",
        "case \"$1\" in list) printf 'Stopped\\t/tmp/x\\n';; --version) echo 'limactl version 9.9.9';; esac",
    );
    let status = machine::status(&scratch.spec(Platform::Lima)).unwrap();
    assert_eq!(status.state, State::Stopped);
    assert_eq!(
        status.tool_version.as_deref(),
        Some("limactl version 9.9.9")
    );
    assert_eq!(status.free_disk, None);
    assert!(status.down_reason.is_some());
}

#[test]
fn start_needs_the_recorded_port_and_an_existing_instance() {
    let scratch = lima("lima-start");
    let mut spec = scratch.spec(Platform::Lima);
    let error = machine::start(&spec, &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::NotFound, "{error}");
    spec.state.port = None;
    let error = machine::start(&spec, &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::InvalidInput);
}

#[test]
fn remove_deletes_the_instance_then_its_disk() {
    let scratch = lima("lima-remove");
    // the disk's name comes from the template a create writes (macOS); elsewhere none is listed
    let disk = if HostOs::current() == HostOs::Macos {
        let _ = machine::create(&scratch.spec(Platform::Lima), &scratch.bundle());
        let template = read(&scratch.path("template.json"));
        let rest = template
            .split("\"additionalDisks\": [{\"name\": \"")
            .nth(1)
            .unwrap();
        rest.split('"').next().unwrap().to_owned()
    } else {
        "lfsnone".to_owned()
    };
    std::fs::remove_file(scratch.log_file()).unwrap_or_default();
    scratch.tool(
        "limactl",
        &format!(
            r#"case "$1 $2" in
  "list --format") printf 'Stopped\t/tmp/x\n' ;;
  "disk list") echo '{{"name":"other"}}'; [ {mac} = 1 ] && echo '{{"name":"{disk}"}}' ;;
esac
exit 0"#,
            mac = u8::from(HostOs::current() == HostOs::Macos),
        ),
    );
    machine::remove(&scratch.spec(Platform::Lima)).unwrap();
    let log = scratch.log();
    let calls: Vec<&str> = log.lines().collect();
    assert_eq!(calls[1], "limactl delete --force lfs-dev", "{log}");
    assert_eq!(calls[2], "limactl disk list --json");
    if HostOs::current() == HostOs::Macos {
        assert_eq!(calls[3], format!("limactl disk delete {disk}"));
    } else {
        assert_eq!(calls.len(), 3, "no disk of ours is listed");
    }
}
