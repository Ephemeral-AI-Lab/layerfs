//! The flat image tar copied from `docker export`: the init layer's files and
//! device/FIFO entries are left out, everything else passes byte for byte.
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
use fixture::{pax_record, Tar};
use layerfs_sandbox::image_tar::{dropped, Filtered};
use std::io::Read;

fn filtered(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // small reads exercise every buffer boundary
    let mut reader = Filtered::new(bytes);
    let mut chunk = [0u8; 7];
    loop {
        let n = reader.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    out
}

#[test]
fn the_init_layer_and_devices_are_dropped() {
    let export = Tar::new()
        .file(".dockerenv", 0o755, b"")
        .directory("bin/", 0o755)
        .file("bin/tool", 0o755, b"#!/bin/sh\necho hi\n")
        .directory("dev/", 0o755)
        .directory("dev/pts/", 0o755)
        .file("dev/console", 0o600, b"")
        .directory("etc/", 0o755)
        .file("etc/hosts", 0o644, b"")
        .file("etc/hostname", 0o644, b"")
        .file("etc/resolv.conf", 0o644, b"")
        .symlink("etc/mtab", "/proc/mounts")
        .file("etc/os-release", 0o644, b"ID=debian\n")
        .raw("null", b'3', 0o666, b"")
        .raw("fifo", b'6', 0o644, b"")
        .finish();
    let expected = Tar::new()
        .directory("bin/", 0o755)
        .file("bin/tool", 0o755, b"#!/bin/sh\necho hi\n")
        .directory("dev/", 0o755)
        .directory("etc/", 0o755)
        .file("etc/os-release", 0o644, b"ID=debian\n")
        .finish();
    assert_eq!(filtered(&export), expected);
}

#[test]
fn a_pax_named_entry_is_decided_by_its_long_name() {
    let long = format!("usr/share/{}", "x".repeat(120));
    let header = |name: &str| pax_record("path", name);
    let export = Tar::new()
        .raw("PaxHeaders/1", b'x', 0o644, &header(&long))
        .file("placeholder-1", 0o644, b"kept")
        .raw("PaxHeaders/2", b'x', 0o644, &header("./etc/hosts"))
        .file("placeholder-2", 0o644, b"")
        .finish();
    let expected = Tar::new()
        .raw("PaxHeaders/1", b'x', 0o644, &header(&long))
        .file("placeholder-1", 0o644, b"kept")
        .finish();
    assert_eq!(filtered(&export), expected);
}

#[test]
fn which_paths_are_dropped() {
    for path in [".dockerenv", "./etc/hosts", "/etc/resolv.conf", "dev/shm", "dev/pts/0"] {
        assert!(dropped(path, b'0'), "{path}");
    }
    for path in ["dev", "dev/", "etc/hosts.allow", "usr/dev/x", "etc"] {
        assert!(!dropped(path, b'5'), "{path}");
    }
    assert!(dropped("anything", b'4'));
}

#[test]
fn two_exports_differing_only_in_the_init_layer_filter_to_the_same_bytes() {
    let base = |hostname: &[u8]| {
        Tar::new()
            .file(".dockerenv", 0o755, b"")
            .file("etc/hostname", 0o644, hostname)
            .file("etc/os-release", 0o644, b"ID=debian\n")
            .finish()
    };
    assert_eq!(filtered(&base(b"abc\n")), filtered(&base(b"defg\n")));
}
