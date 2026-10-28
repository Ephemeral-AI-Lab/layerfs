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
        subdirs: 0,
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
    // R7 update, 2026-10-09: a named directory used to report 2 whatever it
    // held. It reports 2 plus its child directories; with none that is still
    // 2, and the namespace reference count is still ignored for a directory.
    value.kind = InodeKind::Directory;
    value.namespace_refs = u64::MAX;
    let attr = identity.attributes(&value).unwrap();
    assert_eq!((attr.kind, attr.nlink), (FileType::Directory, 2));
    value.subdirs = 5;
    assert_eq!(identity.attributes(&value).unwrap().nlink, 7);
    value.namespace_refs = 1;
    assert_eq!(identity.attributes(&value).unwrap().nlink, 7);
    value.subdirs = 0;
    value.namespace_refs = u64::MAX;
    value.kind = InodeKind::RegularFile;
    assert_eq!(identity.attributes(&value), Err(Errno::EOVERFLOW));
    value.namespace_refs = 1;
    value.metadata.mtime_nanoseconds = 1_000_000_000;
    assert_eq!(identity.attributes(&value), Err(Errno::EOVERFLOW));
}
#[test]
fn a_directory_reports_two_links_plus_its_child_directories_and_none_once_removed() {
    let identity = Identity {
        root: 71,
        uid: 501,
        gid: 20,
    };
    let directory = |serial, namespace_refs, subdirs| ViewStat {
        serial,
        kind: InodeKind::Directory,
        metadata: PortableMetadata {
            mode: 0o755,
            mtime_seconds: 1,
            mtime_nanoseconds: 0,
        },
        namespace_refs,
        logical_len: 0,
        subdirs,
    };
    let links = |value: &ViewStat| identity.attributes(value).map(|attr| attr.nlink);
    // A named directory: its name, its `.`, and one `..` per child directory.
    assert_eq!(links(&directory(9, 1, 0)), Ok(2));
    assert_eq!(links(&directory(9, 1, 1)), Ok(3));
    assert_eq!(links(&directory(9, 1, 300)), Ok(302));
    // A removed directory that is still referenced reports none. Its row
    // holds no child, and no count could bring the links back.
    assert_eq!(links(&directory(9, 0, 0)), Ok(0));
    assert_eq!(links(&directory(9, 0, 4)), Ok(0));
    // The root has no namespace reference by canonical grammar and is never
    // removed: it follows the rule of a named directory.
    assert_eq!(links(&directory(71, 0, 0)), Ok(2));
    assert_eq!(links(&directory(71, 0, 4)), Ok(6));
    // The kernel's count is 32 bits: the largest representable directory,
    // then the first that is not, then the checked addition itself.
    let most = u64::from(u32::MAX) - 2;
    assert_eq!(links(&directory(9, 1, most)), Ok(u32::MAX));
    assert_eq!(links(&directory(9, 1, most + 1)), Err(Errno::EOVERFLOW));
    assert_eq!(links(&directory(71, 0, most + 1)), Err(Errno::EOVERFLOW));
    assert_eq!(links(&directory(9, 1, u64::MAX - 1)), Err(Errno::EOVERFLOW));
    assert_eq!(links(&directory(9, 1, u64::MAX)), Err(Errno::EOVERFLOW));
    // A file or a symlink reports its reference count, whatever the field.
    let mut file = directory(9, 3, 7);
    file.kind = InodeKind::RegularFile;
    assert_eq!(links(&file), Ok(3));
    file.kind = InodeKind::Symlink;
    file.namespace_refs = 1;
    assert_eq!(links(&file), Ok(1));
}
