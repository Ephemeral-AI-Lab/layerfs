use layerfs_storage::format::{
    ValidatedComponent, ValidatedSymlinkTarget, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use layerfs_storage::identity::{
    derive_explicit_directory_v1, derive_file_node_v1, derive_implicit_root_directory_v1,
    derive_logical_chunk_v1, derive_logical_file_v1, derive_physical_chunk_id_v1,
    derive_physical_file_id_v1, derive_symlink_node_v1, derive_version_v1, LogicalChildIdV1,
    LogicalChunkRefV1, LogicalDirectoryEntryV1,
};
use layerfs_storage::CoreError;
use std::process::Command;

fn expected(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64);
    let mut bytes = [0_u8; 32];
    for (slot, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        let high = (pair[0] as char).to_digit(16).expect("high nibble");
        let low = (pair[1] as char).to_digit(16).expect("low nibble");
        *slot = ((high << 4) | low) as u8;
    }
    bytes
}

#[test]
fn all_frozen_m60_logical_vectors_are_exact() {
    let chunk = derive_logical_chunk_v1(b"abc").expect("logical chunk");
    assert_eq!(
        chunk.id().as_bytes(),
        &expected("1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164")
    );
    let file = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(chunk)])
        .expect("logical file");
    assert_eq!(
        file.id().as_bytes(),
        &expected("c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d32")
    );
    let file_node = derive_file_node_v1(0o644, file).expect("file node");
    assert_eq!(
        file_node.as_bytes(),
        &expected("82204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee")
    );

    let target = ValidatedSymlinkTarget::new(b"file.txt").expect("target");
    let symlink = derive_symlink_node_v1(target).expect("symlink node");
    assert_eq!(
        symlink.as_bytes(),
        &expected("b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236")
    );

    let data = ValidatedComponent::new(b"data").expect("component");
    let nested = derive_explicit_directory_v1(
        0o755,
        &[LogicalDirectoryEntryV1::new(
            data,
            LogicalChildIdV1::File(file_node),
        )],
    )
    .expect("nested directory");
    assert_eq!(
        nested.id().as_bytes(),
        &expected("00768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466")
    );

    let empty_root = derive_implicit_root_directory_v1(&[]).expect("empty implicit root");
    assert_eq!(
        empty_root.id().as_bytes(),
        &expected("b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09")
    );
    let empty_version = derive_version_v1(empty_root);
    assert_eq!(
        empty_version.as_bytes(),
        &expected("44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963")
    );

    let composite_root = derive_implicit_root_directory_v1(&[
        LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(b"file.txt").expect("component"),
            LogicalChildIdV1::File(file_node),
        ),
        LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(b"link").expect("component"),
            LogicalChildIdV1::Symlink(symlink),
        ),
        LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(b"nested").expect("component"),
            LogicalChildIdV1::Directory(nested),
        ),
    ])
    .expect("composite root");
    assert_eq!(
        composite_root.id().as_bytes(),
        &expected("70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26")
    );
    assert_eq!(
        derive_version_v1(composite_root).as_bytes(),
        &expected("f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa")
    );
}

#[test]
fn root_sentinel_and_explicit_modes_are_separate_domains() {
    assert_eq!(ROOT_DIRECTORY_MODE_SENTINEL_V1, 0x1000);
    assert_eq!(
        derive_explicit_directory_v1(ROOT_DIRECTORY_MODE_SENTINEL_V1, &[]),
        Err(CoreError::ChildMode)
    );
    let explicit = derive_explicit_directory_v1(0, &[]).expect("explicit empty directory");
    let implicit = derive_implicit_root_directory_v1(&[]).expect("implicit empty root");
    assert_ne!(explicit.id(), implicit.id());
}

#[test]
fn ordering_lengths_endian_and_domains_change_or_reject_identity() {
    let empty = derive_logical_chunk_v1(&[]).expect("empty logical chunk");
    let one = derive_logical_chunk_v1(&[0]).expect("one-byte logical chunk");
    assert_ne!(empty.id(), one.id(), "length is in the canonical preimage");

    let file =
        derive_logical_file_v1(1, &[LogicalChunkRefV1::from_identity(one)]).expect("logical file");
    let file_node = derive_file_node_v1(0o644, file).expect("file node");
    assert_ne!(
        file.id().as_bytes(),
        file_node.as_bytes(),
        "logical-file and file-node domain separators differ"
    );

    let unordered = [
        LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(b"z").expect("z"),
            LogicalChildIdV1::File(file_node),
        ),
        LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(b"a").expect("a"),
            LogicalChildIdV1::File(file_node),
        ),
    ];
    assert_eq!(
        derive_implicit_root_directory_v1(&unordered),
        Err(CoreError::NonCanonicalOrder)
    );
}

#[test]
fn physical_domains_are_separate_and_repacking_does_not_change_logical_identity() {
    let logical = derive_logical_chunk_v1(b"abc").expect("logical chunk");
    let physical_chunk =
        derive_physical_chunk_id_v1(b"same canonical envelope").expect("physical chunk digest");
    let physical_file =
        derive_physical_file_id_v1(b"same canonical envelope").expect("physical file digest");
    assert_ne!(physical_chunk.as_bytes(), physical_file.as_bytes());
    assert_ne!(logical.id().as_bytes(), physical_chunk.as_bytes());

    let before = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(logical)])
        .expect("logical file before repack");
    let _first_pack_placement = derive_physical_chunk_id_v1(b"physical encoding A").unwrap();
    let _second_pack_placement = derive_physical_chunk_id_v1(b"physical encoding B").unwrap();
    let after = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(logical)])
        .expect("logical file after repack");
    assert_eq!(
        before, after,
        "pack placement is absent from logical identity"
    );
}

#[test]
fn repeated_derivation_is_process_stable_and_unkeyed() {
    let expected = expected("1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164");
    if std::env::var_os("LAYERFS_IDENTITY_REPEAT_CHILD").is_none() {
        let executable = std::env::current_exe().expect("current test executable");
        for _ in 0..2 {
            let status = Command::new(&executable)
                .arg("--exact")
                .arg("repeated_derivation_is_process_stable_and_unkeyed")
                .arg("--nocapture")
                .env("LAYERFS_IDENTITY_REPEAT_CHILD", "1")
                .status()
                .expect("spawn repeatability child");
            assert!(status.success(), "repeatability child failed");
        }
    }
    for _ in 0..1_024 {
        assert_eq!(
            derive_logical_chunk_v1(b"abc").unwrap().id().as_bytes(),
            &expected
        );
    }
}
