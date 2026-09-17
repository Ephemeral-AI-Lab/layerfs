//! Attribute storage: reference parity, generic domains, patches and values.
//!
//! The sealed reference cases cover the domains the pinned validator accepts. The
//! replacement deliberately accepts every structurally valid domain; that change
//! is exercised separately here, together with value preservation, extent-only
//! value roots, exact page sizing and explicit refusals.

#![allow(dead_code)]

mod support;

use layerfs_content::filesystem::attributes::build::{build_attribute_tree, AttributeTreeBuilder};
use layerfs_content::filesystem::attributes::codec::{
    decode_attribute_page, encode_attribute_page, AttributeEntry, AttributePage,
};
use layerfs_content::filesystem::attributes::keys::AttributeKey;
use layerfs_content::filesystem::attributes::patch::{apply_patches, AttributePatch};
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::attributes::read::{
    lookup_many, read_opaque, read_portable, AttributeReadWork,
};
use layerfs_content::filesystem::attributes::value::{emit_value, read_value};
use layerfs_content::filesystem::limits::{
    MAXIMUM_ATTRIBUTE_DOMAIN_BYTES, MAXIMUM_ATTRIBUTE_KEY_BYTES, MAXIMUM_PAGE_BYTES,
    MINIMUM_FILLED_PAGE_BYTES,
};
use layerfs_content::filesystem::FilesystemObjects;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ContentError, ObjectId};
use support::filesystem::{synthetic, with_objects, TreeStore};

mod manifest {
    include!("fixtures/filesystem/manifest.rs");
}

fn key(domain: &str, name: &[u8]) -> AttributeKey {
    AttributeKey::new(domain.to_owned(), name.to_vec()).expect("attribute key")
}

fn entry(domain: &str, name: &[u8], label: &str) -> AttributeEntry {
    AttributeEntry {
        key: key(domain, name),
        value_root: synthetic(label),
    }
}

#[test]
fn reference_attribute_trees_are_reproduced_exactly() {
    for (name, root, objects) in manifest::ATTRIBUTE_CASES {
        let count = objects
            .iter()
            .filter(|object| object.role == 9)
            .map(|object| object.count)
            .sum::<u64>();
        let path = format!("tests/fixtures/filesystem/{name}.bin");
        let sealed = std::fs::read(path).expect("sealed attribute fixture");
        let mut store = TreeStore::new();
        let entries = match *name {
            "attr-single" => vec![entry("portable", b"mode", "attr-value/attr-single/000")],
            "attr-pair" => vec![
                entry("portable", b"mode", "attr-value/attr-pair/000"),
                entry("portable", b"mtime", "attr-value/attr-pair/001"),
            ],
            _ => (0..count)
                .map(|index| {
                    entry(
                        "apple.xattr",
                        format!("key-{index:03}").as_bytes(),
                        &format!("attr-value/attr-wide/{index:03}"),
                    )
                })
                .collect(),
        };
        let (built, _) = with_objects(&mut store, |objects| {
            build_attribute_tree(objects, entries.iter().cloned().map(Ok))
        })
        .expect("builds");
        let expected_root = root.parse::<ObjectId>().expect("fixture root");
        assert_eq!(
            built, expected_root,
            "case {name}: attribute root identity differs from the reference"
        );
        let observed = store.canonical(built).expect("root bytes");
        assert_eq!(observed, sealed, "case {name}: attribute root bytes");
        let mut observed_objects = store.order().iter().map(|(id, _)| *id).collect::<Vec<_>>();
        observed_objects.sort();
        observed_objects.dedup();
        let mut expected = objects
            .iter()
            .map(|object| object.id.parse::<ObjectId>().expect("fixture id"))
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(observed_objects, expected, "case {name}: object set");
    }
}

#[test]
fn generic_domains_are_accepted_without_platform_dispatch() {
    // The pinned validator accepted only its own whitelist; this replacement
    // accepts any structurally valid domain and treats the value as data.
    for domain in ["user.example", "com.example.thing", "afs", "x"] {
        let parsed = AttributeKey::new(domain.to_owned(), b"k".to_vec()).expect("domain");
        assert_eq!(parsed.domain(), domain);
    }
    assert!(AttributeKey::new(String::new(), b"k".to_vec()).is_err());
    assert!(AttributeKey::new("a".repeat(MAXIMUM_ATTRIBUTE_DOMAIN_BYTES + 1), Vec::new()).is_err());
    assert!(
        AttributeKey::new("d".to_owned(), vec![b'k'; MAXIMUM_ATTRIBUTE_KEY_BYTES + 1]).is_err()
    );
    assert!(AttributeKey::new("d".to_owned(), b"a\0b".to_vec()).is_err());
    // The reserved portable domain keeps its typed grammar.
    assert!(AttributeKey::new("portable".to_owned(), b"mode".to_vec()).is_ok());
    assert!(AttributeKey::new("portable".to_owned(), b"uid".to_vec()).is_err());
}

#[test]
fn opaque_values_survive_patches_without_being_decoded() {
    let mut store = TreeStore::new();
    let entries = (0..24)
        .map(|index| {
            entry(
                "user.example",
                format!("key-{index:02}").as_bytes(),
                &format!("patch/value-{index:02}"),
            )
        })
        .collect::<Vec<_>>();
    let (base, _) = with_objects(&mut store, |objects| {
        build_attribute_tree(objects, entries.iter().cloned().map(Ok))
    })
    .expect("base tree");
    let reader = store.clone();
    let patches = vec![
        AttributePatch::Set {
            key: key("user.example", b"key-01"),
            value: b"new-value".to_vec(),
        },
        AttributePatch::Remove {
            key: key("user.example", b"key-07"),
        },
    ];
    let (updated, work) = with_objects(&mut store, |objects| {
        apply_patches(&reader, objects, base, &patches)
    })
    .expect("patch applies");
    assert_eq!(work.set, 1);
    assert_eq!(work.removed, 1);
    assert_eq!(work.preserved, 22);
    let mut read = AttributeReadWork::default();
    let reader = store.clone();
    assert_eq!(
        read_opaque(
            &reader,
            updated,
            &key("user.example", b"key-01"),
            64,
            &mut read
        )
        .unwrap()
        .unwrap(),
        b"new-value"
    );
    assert_eq!(
        read_opaque(
            &reader,
            updated,
            &key("user.example", b"key-07"),
            64,
            &mut read
        )
        .unwrap(),
        None
    );
    // Every untouched key keeps its exact stored value root, byte for byte, and
    // the patch never had to decode it.
    let untouched = entries
        .iter()
        .filter(|entry| entry.key.key() != b"key-01" && entry.key.key() != b"key-07")
        .cloned()
        .collect::<Vec<_>>();
    let keys = untouched
        .iter()
        .map(|entry| entry.key.clone())
        .collect::<Vec<_>>();
    let found = lookup_many(&reader, updated, &keys, &mut read).unwrap();
    for (expected, value) in untouched.iter().zip(found) {
        let stored = value.expect("untouched key survives the patch");
        assert_eq!(stored.key, expected.key);
        assert_eq!(
            stored.value_root, expected.value_root,
            "an untouched key keeps its stored value root"
        );
    }
}

#[test]
fn portable_fields_round_trip_and_are_checked() {
    let mut store = TreeStore::new();
    let metadata = PortableMetadata {
        mode: 0o644,
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 123,
    };
    let (root, _) = with_objects(&mut store, |objects| {
        let mode = emit_value(objects, &metadata.mode_bytes(InodeKind::RegularFile)?)?;
        let mtime = emit_value(objects, &metadata.mtime_bytes()?)?;
        build_attribute_tree(
            objects,
            vec![
                Ok(AttributeEntry {
                    key: key("portable", b"mode"),
                    value_root: mode,
                }),
                Ok(AttributeEntry {
                    key: key("portable", b"mtime"),
                    value_root: mtime,
                }),
            ]
            .into_iter(),
        )
    })
    .expect("portable metadata");
    let reader = store.clone();
    let mut work = AttributeReadWork::default();
    let observed =
        read_portable(&reader, root, InodeKind::RegularFile, &mut work).expect("reads portable");
    assert_eq!(observed, metadata);
    assert_eq!(work.values_read, 2);
    // The value ropes stay extent-only: each is a file state over an extent leaf.
    let mode_root = lookup_many(&reader, root, &[key("portable", b"mode")], &mut work).unwrap()[0]
        .as_ref()
        .unwrap()
        .value_root;
    assert_eq!(
        read_value(&reader, mode_root, 4).expect("mode value"),
        0o644_u32.to_be_bytes()
    );
    let state = layerfs_content::file::mapping::decode_file_state(
        store.canonical(mode_root).expect("state bytes"),
    )
    .expect("file state");
    assert_eq!(state.logical_len, 4);
    assert_eq!(state.extent_count, 1);
    assert_eq!(state.tree_level, 0);
    assert_eq!(
        store.role(mode_root),
        Some(layerfs_content::ObjectRole::FileState),
        "the value rope is emitted through the same boundary as any file state"
    );

    // A mode outside the kind's mask, and a fractional second past one billion,
    // are refused by the typed grammar.
    assert!(PortableMetadata {
        mode: 0o7777,
        mtime_seconds: 0,
        mtime_nanoseconds: 0
    }
    .validate(InodeKind::RegularFile)
    .is_err());
    assert!(PortableMetadata {
        mode: 0o777,
        mtime_seconds: 0,
        mtime_nanoseconds: 1_000_000_000
    }
    .mtime_bytes()
    .is_err());
    assert!(PortableMetadata {
        mode: 0o755,
        mtime_seconds: 0,
        mtime_nanoseconds: 0
    }
    .validate(InodeKind::Symlink)
    .is_err());
}

#[test]
fn exact_sizing_partitions_match_the_encoder_and_the_tail_rebalances() {
    // Entries are sized exactly, so the builder's decision to seal a page is the
    // same decision the encoder's own byte total would make.
    let mut builder = AttributeTreeBuilder::new();
    let mut store = TreeStore::new();
    let mut pages = Vec::new();
    with_objects(&mut store, |objects| {
        for index in 0..200 {
            let entry = entry(
                "generic",
                format!("k{index:04}").as_bytes(),
                &format!("sized/{index:04}"),
            );
            let width = 37 + entry.key.domain().len() + entry.key.key().len();
            pages.push(width);
            builder.push(objects, entry)?;
        }
        let root = builder.finish(objects)?;
        Ok(root)
    })
    .expect("sized tree");
    let total: usize = pages.iter().sum();
    assert_eq!(total, 200 * (37 + 7 + 5));
    let mut observed = store
        .order()
        .iter()
        .filter(|(_, role)| *role == layerfs_content::ObjectRole::AttributeLeaf)
        .map(|(id, _)| decode_attribute_page(store.canonical(*id).expect("bytes")).expect("page"))
        .collect::<Vec<_>>();
    observed.sort_by_key(|page| match page {
        AttributePage::Leaf { entries, .. } => entries.first().map(|entry| entry.key.clone()),
        AttributePage::Branch { .. } => None,
    });
    let sealed = observed
        .iter()
        .map(|page| page.bytes().expect("size"))
        .collect::<Vec<_>>();
    for (index, size) in sealed.iter().enumerate() {
        assert!(
            *size <= MAXIMUM_PAGE_BYTES,
            "page {index} exceeds the page ceiling"
        );
        if index + 1 != sealed.len() {
            assert!(
                *size >= MINIMUM_FILLED_PAGE_BYTES,
                "non-final page {index} is below the fill rule"
            );
        }
    }
    // A failure inside the encoder is not a page-full signal: an oversized row is
    // a real error, and the builder must not swallow it.
    let oversized = AttributeEntry {
        key: key("d", &vec![b'k'; MAXIMUM_ATTRIBUTE_KEY_BYTES]),
        value_root: synthetic("sized/oversized"),
    };
    let mut store = TreeStore::new();
    let outcome = with_objects(&mut store, |objects| {
        let mut builder = AttributeTreeBuilder::new();
        builder.push(objects, oversized)?;
        Ok(())
    });
    assert!(outcome.is_ok(), "a row that fits alone is accepted");
}

#[test]
fn attribute_pages_reject_malformed_rows_and_reserved_fields() {
    let page = AttributePage::Leaf {
        subtree_bytes: 37 + 8 + 4,
        entries: vec![entry("portable", b"mode", "codec/mode-value")],
    };
    let bytes = encode_attribute_page(&page).expect("encodes");
    assert_eq!(decode_attribute_page(&bytes).expect("decodes"), page);
    for length in 0..bytes.len() {
        assert!(decode_attribute_page(&bytes[..length]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode_attribute_page(&trailing),
        Err(ContentError::TrailingBytes)
    ));
    let mut flags = bytes.clone();
    flags[13 + 12] = 1;
    assert!(matches!(
        decode_attribute_page(&flags),
        Err(ContentError::InvalidRecord("attribute flags"))
    ));
    let mut required = bytes.clone();
    required[13 + 31 + 2 + 8 + 2 + 4] = 0;
    assert!(matches!(
        decode_attribute_page(&required),
        Err(ContentError::InvalidRecord("attribute required flag"))
    ));
    let mut total = bytes.clone();
    total[13 + 23..13 + 31].copy_from_slice(&1_u64.to_be_bytes());
    assert!(matches!(
        decode_attribute_page(&total),
        Err(ContentError::LengthMismatch { .. })
    ));
}

#[test]
fn values_are_extent_only_at_every_size() {
    for size in [1_usize, 4, 12, 4096] {
        let mut store = TreeStore::new();
        let bytes = vec![0x5a; size];
        let root = with_objects(&mut store, |objects| emit_value(objects, &bytes)).expect("value");
        let reader = store.clone();
        assert_eq!(read_value(&reader, root, size).expect("reads"), bytes);
        let state =
            layerfs_content::file::mapping::decode_file_state(store.canonical(root).unwrap())
                .expect("state");
        assert_eq!(
            state.extent_count, 1,
            "size {size} must stay an extent rope"
        );
        assert_eq!(state.logical_len, size as u64);
    }
    let mut store = TreeStore::new();
    let outcome = with_objects(&mut store, |objects| emit_value(objects, &[]));
    assert!(outcome.is_err(), "an empty attribute value is refused");
}

#[test]
fn a_missing_or_foreign_attribute_tree_fails_explicitly() {
    let store = TreeStore::new();
    let mut work = AttributeReadWork::default();
    let outcome = read_portable(
        &store,
        synthetic("absent"),
        InodeKind::RegularFile,
        &mut work,
    );
    assert!(matches!(outcome, Err(ContentError::MissingObject)));

    let mut store = TreeStore::new();
    let (page_bytes, root) = {
        let mut store = TreeStore::new();
        let root = with_objects(&mut store, |objects| {
            build_attribute_tree(
                objects,
                vec![Ok(entry("generic", b"k", "foreign/value"))].into_iter(),
            )
        })
        .expect("tree")
        .0;
        (store.canonical(root).expect("bytes").to_vec(), root)
    };
    let digest = ObjectId::for_bytes(b"layerfs/not-an-attribute-page");
    store.insert(
        layerfs_content::ObjectRole::AttributeLeaf,
        page_bytes.clone(),
    );
    let _ = digest;
    assert!(decode_attribute_page(&page_bytes).is_ok());
    let mut corrupted = page_bytes.clone();
    corrupted[13 + 12] = 3;
    assert!(decode_attribute_page(&corrupted).is_err());
    let _ = root;
}

#[test]
fn a_reader_boundary_is_not_a_store_and_writes_are_rejected_once() {
    struct Refusing;
    impl layerfs_content::FinalizedConsumer for Refusing {
        fn accept(
            &mut self,
            _object: layerfs_content::FinalizedObject,
        ) -> layerfs_content::ContentResult<()> {
            Err(ContentError::OutputRejected)
        }
    }
    let reader = TreeStore::new();
    let mut refusing = Refusing;
    let outcome = {
        let mut objects = FilesystemObjects::new(&reader, &mut refusing);
        emit_value(&mut objects, b"value")
    };
    assert!(matches!(outcome, Err(ContentError::OutputRejected)));
}

/// R32/R34: an inode's real attribute tree, read through a path.
///
/// `read_portable`, `read_attribute` and `attribute_keys` had no caller anywhere -
/// no case bound a real attribute tree to an inode and read it through a path, so
/// the whole path-level attribute surface was unexercised. Resolving a path also
/// charged nothing: the walk used the uncounted directory lookup and threw its work
/// away, so a reader could stat twice and still report `directory.pages_read = 0`.
/// This case binds a three-key tree to a file inode, reads it three ways through
/// the path, and asserts the work each call charges.
#[test]
fn an_inode_attribute_tree_is_read_through_a_path() {
    use layerfs_content::filesystem::limits::MAXIMUM_ATTRIBUTE_KEYS;
    use layerfs_content::filesystem::{
        build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemRead,
        FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
    };
    use layerfs_content::object::inode_leaf::InodeValue;

    let metadata = PortableMetadata {
        mode: 0o644,
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 7,
    };
    let mut store = TreeStore::new();
    let attribute_root = with_objects(&mut store, |objects| {
        let mode = emit_value(objects, &metadata.mode_bytes(InodeKind::RegularFile)?)?;
        let mtime = emit_value(objects, &metadata.mtime_bytes()?)?;
        let note = emit_value(objects, b"opaque")?;
        let rows = vec![
            AttributeEntry {
                key: key("portable", b"mode"),
                value_root: mode,
            },
            AttributeEntry {
                key: key("portable", b"mtime"),
                value_root: mtime,
            },
            AttributeEntry {
                key: key("user", b"note"),
                value_root: note,
            },
        ];
        build_attribute_tree(objects, rows.into_iter().map(Ok)).map(|(root, _)| root)
    })
    .expect("attribute tree");

    let scope = scope_for_seed([0x33; 32]);
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(PathName::new("f").expect("name"), Some(2))],
    }];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("root-content"),
                metadata_root: synthetic("root-meta"),
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic("file-content"),
                metadata_root: attribute_root,
            },
        },
    ];
    let new_inodes = [1_u64, 2];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let built = with_objects(&mut store, |objects| {
        build_filesystem(objects, &input, None)
    })
    .expect("filesystem build");
    let mut read = FilesystemRead::new(&store, FilesystemRootId(built.root.0)).expect("reader");
    let path = LogicalPath::new("f").expect("path");

    // A path resolve charges the directory pages it walks, once per stat.
    assert_eq!(read.work().directory.pages_read, 0);
    let stat = read.stat(&path).expect("first stat");
    let after_first = read.work().directory.pages_read;
    assert!(
        after_first > 0,
        "resolving a path must charge the directory pages it read"
    );
    assert_eq!(stat.kind, InodeKind::RegularFile);
    assert_eq!(stat.metadata_root, attribute_root);
    let _ = read.stat(&path).expect("second stat");
    assert!(
        read.work().directory.pages_read > after_first,
        "a second stat charges its own read"
    );

    // The typed portable fields, read through the path.
    assert_eq!(
        read.read_portable(&path).expect("portable"),
        metadata,
        "portable mode and mtime come back through the inode's own tree"
    );
    assert!(read.work().attributes.values_read >= 2);

    // One generic value, bounded, present and absent.
    assert_eq!(
        read.read_attribute(&path, &key("user", b"note"), 64)
            .expect("attribute"),
        Some(b"opaque".to_vec())
    );
    assert_eq!(
        read.read_attribute(&path, &key("user", b"absent"), 64)
            .expect("absent attribute"),
        None
    );
    // The bound is the caller's: a value longer than it is refused, not truncated.
    assert!(read
        .read_attribute(&path, &key("user", b"note"), 3)
        .is_err());

    // Every key, in key order, with the pages charged.
    let before_keys = read.work().attributes.pages_read;
    assert_eq!(
        read.attribute_keys(&path).expect("keys"),
        vec![
            key("portable", b"mode"),
            key("portable", b"mtime"),
            key("user", b"note")
        ]
    );
    assert!(
        read.work().attributes.pages_read > before_keys,
        "listing keys charges the pages it walked"
    );

    // R32: the listing is bounded, and a tree past the bound is refused instead of
    // collected. The keys are short so one tree holds them in a bounded page set.
    const OVER: usize = MAXIMUM_ATTRIBUTE_KEYS + 1;
    let mut wide = TreeStore::new();
    let wide_root = with_objects(&mut wide, |objects| {
        let mut rows = Vec::with_capacity(OVER);
        for index in 0..OVER {
            let value_root = emit_value(objects, b"v")?;
            rows.push(AttributeEntry {
                key: AttributeKey::new("user".to_owned(), format!("k{index:05}").into_bytes())
                    .expect("key"),
                value_root,
            });
        }
        build_attribute_tree(objects, rows.into_iter().map(Ok)).map(|(root, _)| root)
    })
    .expect("wide attribute tree");
    let wide_inodes = [
        inodes[0],
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                metadata_root: wide_root,
                ..inodes[1].value
            },
        },
    ];
    let wide_input = FilesystemInput {
        inodes: &wide_inodes,
        ..input
    };
    let wide_built = with_objects(&mut wide, |objects| {
        build_filesystem(objects, &wide_input, None)
    })
    .expect("wide filesystem build");
    let mut wide_read =
        FilesystemRead::new(&wide, FilesystemRootId(wide_built.root.0)).expect("reader");
    match wide_read.attribute_keys(&path) {
        Err(ContentError::ObjectLimitExceeded { limit, actual }) => {
            assert_eq!(limit, MAXIMUM_ATTRIBUTE_KEYS);
            assert_eq!(actual, OVER);
        }
        other => panic!("expected the key bound to refuse, got {other:?}"),
    }
}
