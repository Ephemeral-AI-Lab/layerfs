use layerfs_core::content::persistence as file_codec;
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::limits::{
    DIRECTORY_PAGE_CEILING, FILE_BRANCH_CAPACITY, FILE_LEAF_CAPACITY, MAPPING_PROFILE_FIELD_BYTES,
    MAX_DELTA_PAGE_BYTES, MAX_PATH_BYTES,
};
use layerfs_core::object::{
    decode_object, encode_object, DirectoryEntry, Object, ObjectKind, ObjectReference,
};
use layerfs_core::validation::ValidatedSnapshotReceiptV1;
use layerfs_core::{CanonicalName, CoreError, ObjectId};

const SELECTED_PROFILE_ID: &str =
    "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1";
const MANIFEST: &str =
    include_str!("../../../implementation-detail/phase-4/wp4p/selected-goldens-v1.tsv");

#[derive(Clone)]
struct IndependentReference {
    raw_id: [u8; 32],
    raw_length: u32,
    object_id: [u8; 32],
}

#[derive(Clone)]
struct IndependentChild {
    object_id: [u8; 32],
    raw_length: u64,
}

fn object_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/object\0");
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn bytes_object(value: &[u8]) -> Vec<u8> {
    let payload_length = u32::try_from(value.len() + 4).expect("test payload length");
    let value_length = u32::try_from(value.len()).expect("test value length");
    let mut output = Vec::with_capacity(13 + value.len());
    output.extend_from_slice(b"LFSO");
    output.push(1);
    output.extend_from_slice(&payload_length.to_be_bytes());
    output.extend_from_slice(&value_length.to_be_bytes());
    output.extend_from_slice(value);
    output
}

fn mapping(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(11 + body.len());
    inner.extend_from_slice(b"LFS4MAP\0");
    inner.extend_from_slice(&1_u16.to_be_bytes());
    inner.push(tag);
    inner.extend_from_slice(body);
    bytes_object(&inner)
}

fn independent_reference(raw: &[u8]) -> IndependentReference {
    let canonical = bytes_object(raw);
    IndependentReference {
        raw_id: object_digest(raw),
        raw_length: u32::try_from(raw.len()).expect("test chunk length"),
        object_id: object_digest(&canonical),
    }
}

fn file_leaf(references: &[IndependentReference]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + references.len() * 68);
    body.extend_from_slice(
        &u32::try_from(references.len())
            .expect("test reference count")
            .to_be_bytes(),
    );
    for reference in references {
        body.extend_from_slice(&reference.raw_id);
        body.extend_from_slice(&reference.raw_length.to_be_bytes());
        body.extend_from_slice(&reference.object_id);
    }
    mapping(0x02, &body)
}

fn child_descriptors(children: &[IndependentChild]) -> Vec<u8> {
    let mut body = Vec::with_capacity(children.len() * 40);
    let mut cumulative = 0_u64;
    for child in children {
        cumulative = cumulative
            .checked_add(child.raw_length)
            .expect("test total");
        body.extend_from_slice(&cumulative.to_be_bytes());
        body.extend_from_slice(&child.object_id);
    }
    body
}

fn file_branch(level: u8, children: &[IndependentChild]) -> Vec<u8> {
    let mut body = Vec::with_capacity(5 + children.len() * 40);
    body.push(level);
    body.extend_from_slice(
        &u32::try_from(children.len())
            .expect("test child count")
            .to_be_bytes(),
    );
    body.extend_from_slice(&child_descriptors(children));
    mapping(0x07, &body)
}

fn file_root(
    mode: u32,
    total: u64,
    references: u64,
    level: u8,
    children: &[IndependentChild],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(25 + children.len() * 40);
    body.extend_from_slice(&mode.to_be_bytes());
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(&references.to_be_bytes());
    body.push(level);
    body.extend_from_slice(
        &u32::try_from(children.len())
            .expect("test root child count")
            .to_be_bytes(),
    );
    body.extend_from_slice(&child_descriptors(children));
    mapping(0x01, &body)
}

fn production_reference(reference: &IndependentReference) -> file_codec::FileReference {
    file_codec::FileReference {
        raw_id: ObjectId::from_bytes(&reference.raw_id).expect("raw id"),
        raw_length: reference.raw_length,
        object_id: ObjectId::from_bytes(&reference.object_id).expect("object id"),
    }
}

fn production_children(children: &[IndependentChild]) -> Vec<file_codec::FileChild> {
    let mut cumulative = 0_u64;
    children
        .iter()
        .map(|child| {
            cumulative += child.raw_length;
            file_codec::FileChild {
                object_id: ObjectId::from_bytes(&child.object_id).expect("child id"),
                cumulative_end: cumulative,
            }
        })
        .collect()
}

fn assert_leaf_matches_production(references: &[IndependentReference], expected: &[u8]) {
    let references = references
        .iter()
        .map(production_reference)
        .collect::<Vec<_>>();
    let inner = file_codec::encode_file_leaf(&references).expect("production leaf");
    assert_eq!(
        encode_object(&Object::bytes(inner).expect("leaf bytes object")).expect("leaf object"),
        expected
    );
}

fn assert_branch_matches_production(level: u8, children: &[IndependentChild], expected: &[u8]) {
    let inner = file_codec::encode_file_branch(level, &production_children(children))
        .expect("production branch");
    assert_eq!(
        encode_object(&Object::bytes(inner).expect("branch bytes object")).expect("branch object"),
        expected
    );
}

fn assert_root_matches_production(
    total: u64,
    references: u64,
    level: u8,
    children: &[IndependentChild],
    expected: &[u8],
) {
    let inner =
        file_codec::encode_file_root(0, total, references, level, &production_children(children))
            .expect("production root");
    assert_eq!(
        encode_object(&Object::bytes(inner).expect("root bytes object")).expect("root object"),
        expected
    );
}

fn add_object_line(output: &mut String, name: &str, bytes: &[u8], encoding: &str) {
    output.push_str("object\t");
    output.push_str(name);
    output.push('\t');
    output.push_str(&bytes.len().to_string());
    output.push('\t');
    output.push_str(&hex(&object_digest(bytes)));
    output.push('\t');
    output.push_str(encoding);
    output.push('\n');
}

fn add_small_object_line(output: &mut String, name: &str, bytes: &[u8]) {
    let encoding = if bytes.len() <= 512 {
        format!("hex:{}", hex(bytes))
    } else {
        format!("recipe:independent-byte-packer:{name}")
    };
    add_object_line(output, name, bytes, &encoding);
}

fn build_repeated_file(count: usize, manifest: &mut String) -> Vec<u8> {
    let reference = independent_reference(b"abc");
    let references = vec![reference; count];
    let mut nodes = Vec::new();
    for (leaf_index, leaf) in references.chunks(FILE_LEAF_CAPACITY).enumerate() {
        let canonical = file_leaf(leaf);
        assert_leaf_matches_production(leaf, &canonical);
        if matches!(count, 1 | 64) && leaf_index == 0 {
            add_small_object_line(manifest, &format!("file-leaf-{count}"), &canonical);
        }
        nodes.push(IndependentChild {
            object_id: object_digest(&canonical),
            raw_length: u64::try_from(leaf.len() * 3).expect("leaf length"),
        });
    }

    let mut level = 0_u8;
    while nodes.len() > FILE_BRANCH_CAPACITY {
        let mut parents = Vec::new();
        for (branch_index, children) in nodes.chunks(FILE_BRANCH_CAPACITY).enumerate() {
            let branch_level = level + 1;
            let canonical = file_branch(branch_level, children);
            assert_branch_matches_production(branch_level, children, &canonical);
            if count == 4_097 {
                add_small_object_line(
                    manifest,
                    &format!("file-branch-4097-{branch_index}"),
                    &canonical,
                );
            }
            parents.push(IndependentChild {
                object_id: object_digest(&canonical),
                raw_length: children.iter().map(|child| child.raw_length).sum(),
            });
        }
        nodes = parents;
        level += 1;
    }

    let total = u64::try_from(count * 3).expect("file total");
    let root = file_root(0, total, count as u64, level, &nodes);
    assert_root_matches_production(total, count as u64, level, &nodes, &root);
    assert_eq!(
        file_codec::expected_file_level(count as u64).expect("selected level"),
        level
    );
    add_small_object_line(manifest, &format!("file-root-{count}"), &root);
    root
}

fn directory_name(number: usize) -> Vec<u8> {
    let mut value = number;
    let mut bytes = vec![b'x'; 255];
    bytes[8] = b'-';
    for index in (0..8).rev() {
        bytes[index] = b'0' + u8::try_from(value % 10).expect("decimal digit");
        value /= 10;
    }
    assert_eq!(value, 0);
    bytes
}

fn directory_object(entries: &[(Vec<u8>, u8, [u8; 32])]) -> Vec<u8> {
    let payload_length = 4_usize
        + entries
            .iter()
            .map(|(name, _, _)| 4 + name.len() + 1 + 32)
            .sum::<usize>();
    let mut output = Vec::with_capacity(9 + payload_length);
    output.extend_from_slice(b"LFSO");
    output.push(2);
    output.extend_from_slice(
        &u32::try_from(payload_length)
            .expect("directory payload")
            .to_be_bytes(),
    );
    output.extend_from_slice(
        &u32::try_from(entries.len())
            .expect("directory count")
            .to_be_bytes(),
    );
    for (name, kind, id) in entries {
        output.extend_from_slice(
            &u32::try_from(name.len())
                .expect("directory name")
                .to_be_bytes(),
        );
        output.extend_from_slice(name);
        output.push(*kind);
        output.extend_from_slice(id);
    }
    output
}

fn directory_page_entries(
    first: usize,
    count: usize,
    child: [u8; 32],
) -> Vec<(Vec<u8>, u8, [u8; 32])> {
    (first..first + count)
        .map(|number| (directory_name(number), 1, child))
        .collect()
}

fn production_directory_entries(entries: &[(Vec<u8>, u8, [u8; 32])]) -> Vec<DirectoryEntry> {
    entries
        .iter()
        .map(|(name, kind, id)| {
            DirectoryEntry::new(
                CanonicalName::from_bytes(name).expect("canonical name"),
                ObjectReference::new(
                    if *kind == 1 {
                        ObjectKind::Bytes
                    } else {
                        ObjectKind::Directory
                    },
                    ObjectId::from_bytes(id).expect("directory child"),
                ),
            )
        })
        .collect()
}

fn directory_index(total: u32, pages: &[(u32, Vec<u8>, [u8; 32])]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(
        &u32::try_from(pages.len())
            .expect("directory page count")
            .to_be_bytes(),
    );
    for (count, first_name, id) in pages {
        body.extend_from_slice(&count.to_be_bytes());
        body.extend_from_slice(
            &u16::try_from(first_name.len())
                .expect("first name length")
                .to_be_bytes(),
        );
        body.extend_from_slice(first_name);
        body.extend_from_slice(id);
    }
    mapping(0x03, &body)
}

fn directory_wrapper(metadata: [u8; 32], index: [u8; 32]) -> Vec<u8> {
    directory_object(&[(b"m".to_vec(), 1, metadata), (b"t".to_vec(), 1, index)])
}

fn add_directory_vectors(manifest: &mut String) {
    let child = object_digest(b"directory-child");
    let one = directory_page_entries(1, 1, child);
    let one_page = directory_object(&one);
    assert_eq!(
        dir_codec::encode_directory_page(&production_directory_entries(&one))
            .expect("production one-entry page"),
        one_page
    );
    add_small_object_line(manifest, "directory-page-1", &one_page);

    let full = directory_page_entries(1, 897, child);
    let full_page = directory_object(&full);
    assert_eq!(full_page.len(), 261_937);
    assert!(full_page.len() <= DIRECTORY_PAGE_CEILING);
    assert!(full_page.len() + 292 > DIRECTORY_PAGE_CEILING);
    assert_eq!(
        dir_codec::encode_directory_page(&production_directory_entries(&full))
            .expect("production 897-entry page"),
        full_page
    );
    add_object_line(
        manifest,
        "directory-page-897",
        &full_page,
        "recipe:max-name-entries:first=1,count=897,child=directory-child",
    );

    let tail = directory_page_entries(898, 1, child);
    let tail_page = directory_object(&tail);
    let pages = vec![
        (897, directory_name(1), object_digest(&full_page)),
        (1, directory_name(898), object_digest(&tail_page)),
    ];
    let index = directory_index(898, &pages);
    let production_pages = pages
        .iter()
        .map(|(count, first_name, id)| dir_codec::DirectoryPageRef {
            count: *count,
            first_name: first_name.clone(),
            object_id: ObjectId::from_bytes(id).expect("page id"),
        })
        .collect::<Vec<_>>();
    let index_inner = dir_codec::encode_directory_index(898, &production_pages)
        .expect("production directory index");
    assert_eq!(
        encode_object(&Object::bytes(index_inner).expect("index bytes object"))
            .expect("index object"),
        index
    );
    let production_full = production_directory_entries(&full);
    let production_tail = production_directory_entries(&tail);
    dir_codec::validate_directory_partition(
        898,
        &[
            (&production_full, &production_pages[0]),
            (&production_tail, &production_pages[1]),
        ],
    )
    .expect("selected greedy partition");
    add_small_object_line(manifest, "directory-index-898", &index);

    let metadata = mapping(0x04, &0_u32.to_be_bytes());
    let production_metadata = dir_codec::encode_directory_metadata(0).expect("metadata");
    assert_eq!(
        encode_object(&Object::bytes(production_metadata).expect("metadata bytes object"))
            .expect("metadata object"),
        metadata
    );
    add_small_object_line(manifest, "directory-metadata-0", &metadata);
    let wrapper = directory_wrapper(object_digest(&metadata), object_digest(&index));
    assert_eq!(
        dir_codec::encode_directory_wrapper(
            ObjectId::from_bytes(&object_digest(&metadata)).expect("metadata id"),
            ObjectId::from_bytes(&object_digest(&index)).expect("index id"),
        )
        .expect("production wrapper"),
        wrapper
    );
    add_small_object_line(manifest, "directory-wrapper-898", &wrapper);
}

fn delta_page(tag: u8, path: &[u8], ids: &[[u8; 32]], modes: &[u32]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.push(tag);
    body.extend_from_slice(
        &u32::try_from(path.len())
            .expect("delta path length")
            .to_be_bytes(),
    );
    body.extend_from_slice(path);
    for (index, id) in ids.iter().enumerate() {
        body.extend_from_slice(id);
        if let Some(mode) = modes.get(index) {
            body.extend_from_slice(&mode.to_be_bytes());
        }
    }
    mapping(0x06, &body)
}

fn delta_index(
    parent: Option<[u8; 32]>,
    child: [u8; 32],
    count: u32,
    pages: &[[u8; 32]],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(u8::from(parent.is_some()));
    if let Some(parent) = parent {
        body.extend_from_slice(&parent);
    }
    body.extend_from_slice(&child);
    body.extend_from_slice(&count.to_be_bytes());
    body.extend_from_slice(
        &u32::try_from(pages.len())
            .expect("delta page count")
            .to_be_bytes(),
    );
    for page in pages {
        body.extend_from_slice(page);
    }
    mapping(0x05, &body)
}

fn add_delta_vectors(manifest: &mut String) {
    let parent = object_digest(b"delta-parent");
    let child = object_digest(b"delta-child");
    let before = object_digest(b"delta-before");
    let after = object_digest(b"delta-after");
    let genesis = delta_index(None, child, 0, &[]);
    let production_genesis =
        delta_codec::encode_genesis(ObjectId::from_bytes(&child).expect("child"))
            .expect("production genesis");
    assert_eq!(
        encode_object(&Object::bytes(production_genesis).expect("genesis bytes object"))
            .expect("genesis object"),
        genesis
    );
    add_small_object_line(manifest, "delta-genesis", &genesis);

    let cases = [
        (
            "add",
            delta_codec::TransitionOperation::Add {
                path: b"a".to_vec(),
                after: ObjectId::from_bytes(&after).expect("after"),
            },
            delta_page(1, b"a", &[after], &[]),
        ),
        (
            "remove",
            delta_codec::TransitionOperation::Remove {
                path: b"a".to_vec(),
                before: ObjectId::from_bytes(&before).expect("before"),
            },
            delta_page(2, b"a", &[before], &[]),
        ),
        (
            "replace",
            delta_codec::TransitionOperation::Replace {
                path: b"a".to_vec(),
                before: ObjectId::from_bytes(&before).expect("before"),
                after: ObjectId::from_bytes(&after).expect("after"),
            },
            delta_page(3, b"a", &[before, after], &[]),
        ),
        (
            "metadata",
            delta_codec::TransitionOperation::Metadata {
                path: Vec::new(),
                before: ObjectId::from_bytes(&before).expect("before"),
                before_mode: 7,
                after: ObjectId::from_bytes(&after).expect("after"),
                after_mode: 9,
            },
            delta_page(4, b"", &[before, after], &[7, 9]),
        ),
    ];
    for (name, operation, page) in cases {
        let production = delta_codec::encode_delta_page(std::slice::from_ref(&operation))
            .expect("production delta page");
        assert_eq!(
            encode_object(&Object::bytes(production).expect("delta bytes object"))
                .expect("delta object"),
            page
        );
        assert_eq!(
            delta_codec::decode_mapping_delta_page(&page).expect("delta decode"),
            vec![operation]
        );
        add_small_object_line(manifest, &format!("delta-page-{name}"), &page);
        let index = delta_index(Some(parent), child, 1, &[object_digest(&page)]);
        let production_index = delta_codec::encode_change(
            ObjectId::from_bytes(&parent).expect("parent"),
            ObjectId::from_bytes(&child).expect("child"),
            1,
            &[ObjectId::from_bytes(&object_digest(&page)).expect("page")],
        )
        .expect("production delta index");
        assert_eq!(
            encode_object(&Object::bytes(production_index).expect("index bytes object"))
                .expect("index object"),
            index
        );
        add_small_object_line(manifest, &format!("delta-index-{name}"), &index);
    }
}

fn add_delta_page_boundary(manifest: &mut String) {
    let metadata_entry_bytes = 1 + 4 + MAX_PATH_BYTES + 32 + 4 + 32 + 4;
    let page_header_bytes = 11 + 4;
    let fit = page_header_bytes + 2_010 * metadata_entry_bytes;
    let overflow = page_header_bytes + 2_011 * metadata_entry_bytes;
    assert_eq!(metadata_entry_bytes, 4_173);
    assert_eq!(fit, 8_387_745);
    assert!(fit <= MAX_DELTA_PAGE_BYTES);
    assert_eq!(overflow, 8_391_918);
    assert!(overflow > MAX_DELTA_PAGE_BYTES);

    let mut declared_oversize = Vec::new();
    declared_oversize.extend_from_slice(b"LFSO");
    declared_oversize.push(1);
    declared_oversize.extend_from_slice(
        &u32::try_from(overflow + 4)
            .expect("outer payload length")
            .to_be_bytes(),
    );
    declared_oversize.extend_from_slice(
        &u32::try_from(overflow)
            .expect("outer field length")
            .to_be_bytes(),
    );
    assert_eq!(
        decode_object(&declared_oversize),
        Err(CoreError::ObjectLimitExceeded)
    );

    manifest.push_str(
        "recipe\tdelta-page-max-metadata-2010\t8387745\t-\tequation:15+2010*4173=8387745<=8388608\n",
    );
    manifest.push_str(
        "failure\tdelta-page-max-metadata-2011\t8391918\t-\tObjectLimitExceeded:15+2011*4173=8391918>8388608\n",
    );
}

fn receipt_bytes() -> (ValidatedSnapshotReceiptV1, [u8; 32], Vec<u8>) {
    let key = [0x5a; 32];
    let store = [0x11; 16];
    let mut authority_hasher = blake3::Hasher::new();
    authority_hasher.update(b"layerfs/validation-authority/v1\0");
    authority_hasher.update(&store);
    authority_hasher.update(&key);
    let authority = *authority_hasher.finalize().as_bytes();
    let profile = file_codec::selected_mapping_profile_id();
    let receipt = ValidatedSnapshotReceiptV1 {
        store_instance_id: store,
        validation_authority_id: authority,
        integrity_epoch: 7,
        head_generation: 9,
        child_root_id: ObjectId::from_bytes(&object_digest(b"receipt-root")).expect("root"),
        transition_id: ObjectId::from_bytes(&object_digest(b"receipt-transition"))
            .expect("transition"),
        mapping_profile_id: profile,
    };
    let mut inner = Vec::new();
    inner.extend_from_slice(b"LFS4VAL\0");
    inner.extend_from_slice(&1_u16.to_be_bytes());
    inner.push(1);
    inner.extend_from_slice(&store);
    inner.extend_from_slice(&authority);
    inner.extend_from_slice(&7_u64.to_be_bytes());
    inner.extend_from_slice(&9_u64.to_be_bytes());
    inner.extend_from_slice(receipt.child_root_id.as_bytes());
    inner.extend_from_slice(receipt.transition_id.as_bytes());
    inner.extend_from_slice(profile.as_bytes());
    let mut authenticator = blake3::Hasher::new_keyed(&key);
    authenticator.update(b"layerfs/validated-snapshot/v1\0");
    authenticator.update(&inner);
    inner.extend_from_slice(authenticator.finalize().as_bytes());
    (receipt, key, bytes_object(&inner))
}

fn selected_manifest() -> String {
    let mut manifest = String::from("kind\tname\tlength\tid\tencoding-or-result\n");
    manifest.push_str("profile\tselected-k64-f64-dir256k\t32\t");
    manifest.push_str(SELECTED_PROFILE_ID);
    manifest.push_str(
        "\tpreimage:layerfs/mapping-profile/v1\\0,u32be(64),u32be(64),u32be(262144),u32be(8388608)\n",
    );
    for count in [1, 64, 65, 4_096, 4_097] {
        build_repeated_file(count, &mut manifest);
    }
    add_directory_vectors(&mut manifest);
    add_delta_vectors(&mut manifest);
    add_delta_page_boundary(&mut manifest);
    let (receipt, key, bytes) = receipt_bytes();
    assert_eq!(
        receipt.encode(&key).expect("production receipt").as_slice(),
        bytes
    );
    add_small_object_line(&mut manifest, "selected-profile-receipt", &bytes);
    for (name, result) in [
        ("mapping-version-2", "UnsupportedMappingVersion"),
        ("mapping-unknown-tag", "InvalidMappingTag"),
        ("mapping-wrong-role", "WrongLogicalRole"),
        ("file-nonfinal-63", "NonCanonicalPagePartition"),
        ("branch-nonfinal-63", "NonCanonicalPagePartition"),
        ("branch-descending-end", "NonCanonicalOrdering"),
        ("file-root-length-mismatch", "LengthMismatch"),
        ("directory-cross-page-duplicate", "NameCollision"),
        ("directory-non-greedy-split", "NonCanonicalPagePartition"),
        ("delta-empty-page", "NonCanonicalPagePartition"),
        ("delta-invalid-operation", "InvalidMappingDiscriminator"),
        ("delta-trailing-byte", "TrailingBytes"),
        ("receipt-wrong-profile", "InvalidValidationReceipt"),
    ] {
        manifest.push_str(&format!("failure\t{name}\t0\t-\t{result}\n"));
    }
    manifest
}

#[test]
fn selected_profile_success_vectors_match_independent_bytes_and_ids() {
    assert_eq!(FILE_LEAF_CAPACITY, 64);
    assert_eq!(FILE_BRANCH_CAPACITY, 64);
    assert_eq!(DIRECTORY_PAGE_CEILING, 262_144);
    assert_eq!(MAPPING_PROFILE_FIELD_BYTES, 8_388_608);
    assert_eq!(
        file_codec::selected_mapping_profile_id().to_string(),
        SELECTED_PROFILE_ID
    );
    assert_eq!(selected_manifest(), MANIFEST);
}

#[test]
fn selected_profile_rejects_representative_malformed_records() {
    let reference = independent_reference(b"abc");
    let leaf = file_leaf(std::slice::from_ref(&reference));
    let mut version = leaf.clone();
    version[21..23].copy_from_slice(&2_u16.to_be_bytes());
    assert_eq!(
        file_codec::decode_mapping(&version, file_codec::FILE_LEAF_TAG),
        Err(CoreError::UnsupportedMappingVersion { version: 2 })
    );
    let mut tag = leaf.clone();
    tag[23] = 0xff;
    assert_eq!(
        file_codec::decode_mapping(&tag, file_codec::FILE_LEAF_TAG),
        Err(CoreError::InvalidMappingTag { tag: 0xff })
    );
    assert_eq!(
        file_codec::decode_mapping(&leaf, file_codec::FILE_ROOT_TAG),
        Err(CoreError::WrongLogicalRole)
    );

    let references = vec![production_reference(&reference); 63];
    assert_eq!(
        file_codec::validate_file_leaf(&references, false),
        Err(CoreError::NonCanonicalPagePartition)
    );
    let children = (0_u64..63)
        .map(|index| file_codec::FileChild {
            object_id: ObjectId::for_bytes(&index.to_be_bytes()),
            cumulative_end: index + 1,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        file_codec::validate_file_children(&children, false),
        Err(CoreError::NonCanonicalPagePartition)
    );
    let mut descending = Vec::new();
    descending.push(1);
    descending.extend_from_slice(&2_u32.to_be_bytes());
    file_codec::FileChild {
        object_id: ObjectId::for_bytes(b"a"),
        cumulative_end: 2,
    }
    .encode(&mut descending);
    file_codec::FileChild {
        object_id: ObjectId::for_bytes(b"b"),
        cumulative_end: 1,
    }
    .encode(&mut descending);
    assert_eq!(
        file_codec::parse_file_children(&descending, true),
        Err(CoreError::NonCanonicalOrdering)
    );

    let child = IndependentChild {
        object_id: object_digest(&leaf),
        raw_length: 3,
    };
    let wrong_total = file_root(0, 4, 1, 0, &[child]);
    let payload = file_codec::decode_mapping(&wrong_total, file_codec::FILE_ROOT_TAG)
        .expect("wrong-total role");
    assert!(matches!(
        file_codec::parse_file_root(payload),
        Err(CoreError::LengthMismatch {
            expected: 4,
            actual: 3
        })
    ));

    let duplicate_child = ObjectId::for_bytes(b"duplicate");
    let first = vec![DirectoryEntry::new(
        CanonicalName::from_bytes(b"a").expect("name"),
        ObjectReference::new(ObjectKind::Bytes, duplicate_child),
    )];
    let descriptor = dir_codec::DirectoryPageRef {
        count: 1,
        first_name: b"a".to_vec(),
        object_id: ObjectId::for_bytes(b"page"),
    };
    let mut partition = dir_codec::DirectoryPartitionValidator::new();
    partition.push(&first, &descriptor).expect("first page");
    assert_eq!(
        partition.push(&first, &descriptor),
        Err(CoreError::NameCollision)
    );
    let mut slack = dir_codec::DirectoryPartitionValidator::new();
    slack.push(&first, &descriptor).expect("first slack page");
    let second = vec![DirectoryEntry::new(
        CanonicalName::from_bytes(b"b").expect("name"),
        ObjectReference::new(ObjectKind::Bytes, duplicate_child),
    )];
    let second_descriptor = dir_codec::DirectoryPageRef {
        count: 1,
        first_name: b"b".to_vec(),
        object_id: ObjectId::for_bytes(b"page-2"),
    };
    assert_eq!(
        slack.push(&second, &second_descriptor),
        Err(CoreError::NonCanonicalPagePartition)
    );

    assert_eq!(
        delta_codec::decode_delta_page(&0_u32.to_be_bytes()),
        Err(CoreError::NonCanonicalPagePartition)
    );
    let mut bad_operation = Vec::new();
    bad_operation.extend_from_slice(&1_u32.to_be_bytes());
    bad_operation.push(0);
    bad_operation.extend_from_slice(&0_u32.to_be_bytes());
    assert_eq!(
        delta_codec::decode_delta_page(&bad_operation),
        Err(CoreError::InvalidMappingDiscriminator { value: 0 })
    );
    let add = delta_codec::TransitionOperation::Add {
        path: b"a".to_vec(),
        after: ObjectId::for_bytes(b"after"),
    };
    let mut trailing = delta_codec::encode_delta_page(&[add]).expect("delta page");
    trailing.push(0);
    assert_eq!(
        delta_codec::decode_delta_page(&trailing[11..]),
        Err(CoreError::TrailingBytes)
    );

    let (receipt, key, bytes) = receipt_bytes();
    assert_eq!(
        ValidatedSnapshotReceiptV1::decode(
            &bytes,
            &key,
            ObjectId::for_bytes(b"wrong profile"),
            receipt.validation_authority_id,
        ),
        Err(CoreError::InvalidValidationReceipt)
    );
}

#[test]
#[ignore = "prints the independently generated normative TSV"]
fn print_selected_golden_manifest() {
    print!("{}", selected_manifest());
}
