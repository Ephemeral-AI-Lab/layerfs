//! Telemetry names every kernel opcode once, at its own index.
#![cfg(target_os = "linux")]
use layerfs_fuse::request::{Opcode, OPCODES};
use std::collections::BTreeSet;

#[test]
fn every_opcode_has_its_own_snake_case_name() {
    use Opcode::*;
    let all = [
        (Lookup, "lookup"),
        (Forget, "forget"),
        (BatchForget, "batch_forget"),
        (Getattr, "getattr"),
        (Setattr, "setattr"),
        (Readlink, "readlink"),
        (Mknod, "mknod"),
        (Mkdir, "mkdir"),
        (Unlink, "unlink"),
        (Rmdir, "rmdir"),
        (Symlink, "symlink"),
        (Rename, "rename"),
        (Link, "link"),
        (Open, "open"),
        (Read, "read"),
        (Write, "write"),
        (Flush, "flush"),
        (Release, "release"),
        (Fsync, "fsync"),
        (Opendir, "opendir"),
        (Readdir, "readdir"),
        (Readdirplus, "readdirplus"),
        (Releasedir, "releasedir"),
        (Fsyncdir, "fsyncdir"),
        (Statfs, "statfs"),
        (Setxattr, "setxattr"),
        (Getxattr, "getxattr"),
        (Listxattr, "listxattr"),
        (Removexattr, "removexattr"),
        (Access, "access"),
        (Create, "create"),
        (Getlk, "getlk"),
        (Setlk, "setlk"),
        (Bmap, "bmap"),
        (Ioctl, "ioctl"),
        (Poll, "poll"),
        (Fallocate, "fallocate"),
        (Lseek, "lseek"),
        (CopyFileRange, "copy_file_range"),
    ];
    assert_eq!(all.len(), OPCODES);
    for (opcode, name) in all {
        assert_eq!(Opcode::NAMES[opcode as usize], name);
    }
    assert_eq!(Opcode::NAMES.iter().collect::<BTreeSet<_>>().len(), OPCODES);
}
