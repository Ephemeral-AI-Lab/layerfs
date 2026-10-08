#![cfg(target_os = "linux")]
use fuser::{Errno, FileType, INodeNo};
use layerfs_content::{filesystem::attributes::PortableMetadata, object::inode_leaf::InodeKind};
use layerfs_fuse::attributes::Identity;
use layerfs_workspace::ViewStat;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn root_projection_is_bijective_and_does_not_assume_serial_one() {
    let identity = Identity {
        root: 71,
        uid: 501,
        gid: 20,
    };
    assert_eq!(identity.inode(71).unwrap(), INodeNo(1));
    for serial in [1, 2, 70, 71, 72, u64::MAX - 1] {
        assert_eq!(
            identity.serial(identity.inode(serial).unwrap()).unwrap(),
            serial
        );
    }
    assert_eq!(identity.serial(INodeNo(0)), Err(Errno::ESTALE));
    assert_eq!(identity.serial(INodeNo(72)), Err(Errno::ESTALE));
    assert_eq!(identity.inode(0), Err(Errno::ESTALE));
    assert_eq!(identity.inode(u64::MAX), Err(Errno::EOVERFLOW));
}
#[test]
fn attributes_preserve_signed_fraction_identity_mode_and_link_count() {
    let identity = Identity {
        root: 71,
        uid: 501,
        gid: 20,
    };
    let mut value = ViewStat {
        serial: 9,
        kind: InodeKind::RegularFile,
        metadata: PortableMetadata {
            mode: 0o751,
            mtime_seconds: -2,
            mtime_nanoseconds: 750_000_000,
        },
        namespace_refs: 3,
        logical_len: 513,
    };
    let attr = identity.attributes(&value).unwrap();
    assert_eq!(attr.ino, INodeNo(10));
    assert_eq!(
        (attr.uid, attr.gid, attr.perm, attr.nlink),
        (501, 20, 0o751, 3)
    );
    assert_eq!((attr.size, attr.blocks, attr.blksize), (513, 2, 4096));
    assert_eq!(attr.mtime, UNIX_EPOCH - Duration::new(1, 250_000_000));
    assert_eq!(attr.atime, attr.mtime);
    assert_eq!(attr.ctime, attr.mtime);
    value.kind = InodeKind::Directory;
    value.namespace_refs = u64::MAX;
    let attr = identity.attributes(&value).unwrap();
    assert_eq!((attr.kind, attr.nlink), (FileType::Directory, 2));
    value.kind = InodeKind::RegularFile;
    assert_eq!(identity.attributes(&value), Err(Errno::EOVERFLOW));
    value.namespace_refs = 1;
    value.metadata.mtime_nanoseconds = 1_000_000_000;
    assert_eq!(identity.attributes(&value), Err(Errno::EOVERFLOW));
}
