//! Sizes and spec validation, refused before any effect.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{parse_size, Code, Platform, Spec};
use standin::Scratch;

#[test]
fn sizes_are_bytes_or_binary_units() {
    assert_eq!(parse_size("8G").unwrap(), 8 << 30);
    assert_eq!(parse_size("512M").unwrap(), 512 << 20);
    assert_eq!(parse_size("2GiB").unwrap(), 2 << 30);
    assert_eq!(parse_size("2GB").unwrap(), 2 << 30);
    assert_eq!(parse_size("1T").unwrap(), 1 << 40);
    assert_eq!(parse_size("64k").unwrap(), 64 << 10);
    assert_eq!(parse_size("1024").unwrap(), 1024);
    assert_eq!(parse_size("100B").unwrap(), 100);
    assert_eq!(parse_size(" 4G ").unwrap(), 4 << 30);
    for bad in [
        "",
        "0",
        "0G",
        "G",
        "8X",
        "8iB",
        "-1G",
        "1.5G",
        "99999999999999T",
    ] {
        let error = parse_size(bad).unwrap_err();
        assert_eq!(error.code, Code::InvalidInput, "{bad:?}");
    }
}

fn refused(spec: &Spec) -> String {
    let error = spec.validate().unwrap_err();
    assert_eq!(error.code, Code::InvalidInput);
    assert!(!error.vm_exists);
    error.message
}

#[test]
fn a_plain_vm_spec_is_valid() {
    let scratch = Scratch::new("spec-ok");
    for kind in Platform::ALL {
        if kind != Platform::Firecracker {
            scratch.spec(kind).validate().unwrap();
        }
    }
}

#[test]
fn names_sizes_and_paths_are_checked() {
    let scratch = Scratch::new("spec-bad");
    let mut spec = scratch.spec(Platform::Lima);
    spec.name = "-dev".into();
    assert!(refused(&spec).contains("machine name"));
    let mut spec = scratch.spec(Platform::Lima);
    spec.vm_name = "lfs dev".into();
    assert!(refused(&spec).contains("VM name"));
    let mut spec = scratch.spec(Platform::Docker);
    spec.vm_name = "--privileged".into();
    assert!(refused(&spec).contains("VM name"));
    let mut spec = scratch.spec(Platform::Lima);
    spec.memory = 100 << 20;
    assert!(refused(&spec).contains("memory"));
    let mut spec = scratch.spec(Platform::Lima);
    spec.disk = 1 << 20;
    assert!(refused(&spec).contains("disk"));
    let mut spec = scratch.spec(Platform::Lima);
    spec.cpus = 0;
    assert!(refused(&spec).contains("CPU"));
    let mut spec = scratch.spec(Platform::Lima);
    spec.work_dir = "relative/work".into();
    assert!(refused(&spec).contains("absolute"));
}

#[test]
fn kind_specific_fields_belong_to_their_kind() {
    let scratch = Scratch::new("spec-kinds");
    let mut spec = scratch.spec(Platform::AppleContainer);
    spec.nested = true;
    assert!(refused(&spec).contains("nested"));

    let spec = scratch.spec(Platform::Firecracker);
    assert!(refused(&spec).contains("lima host"));

    let mut spec = scratch.spec(Platform::Firecracker);
    spec.host = Some(Box::new(scratch.spec(Platform::Lima)));
    assert!(refused(&spec).contains("not a nested lima"));

    let mut host = scratch.spec(Platform::Lima);
    host.nested = true;
    let mut spec = scratch.spec(Platform::Firecracker);
    spec.host = Some(Box::new(host.clone()));
    spec.validate().unwrap();
    spec.state.network = Some(251);
    assert!(refused(&spec).contains("251"));

    let mut spec = scratch.spec(Platform::Lima);
    spec.host = Some(Box::new(host));
    assert!(refused(&spec).contains("only a firecracker"));
}
