//! Strict canonical-v2 mapping codecs shared by every v2 content path.

pub mod file_codec {
    use crate::cdc::MAXIMUM_CHUNK_BYTES;
    use crate::content::persistence as v1;
    use crate::limits::{FILE_BRANCH_CAPACITY, FILE_LEAF_CAPACITY, MAX_CHILD_REFERENCES};
    use crate::{decode_bytes_object, CoreError, CoreResult, ObjectId};

    pub use v1::{
        expected_file_level, parse_file_children, parse_file_root, validate_file_children,
        validate_file_root_summary, FileChild, DIR_INDEX_TAG, DIR_METADATA_TAG, FILE_BRANCH_TAG,
        FILE_DESCRIPTOR_BYTES, FILE_LEAF_TAG, FILE_ROOT_TAG,
    };

    pub const MAPPING_VERSION: u16 = 2;
    pub const FILE_REF_BYTES: usize = 36;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FileReference {
        pub raw_length: u32,
        pub object_id: ObjectId,
    }

    pub fn selected_mapping_profile_id() -> ObjectId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/mapping-profile/v2\0");
        for value in [64_u32, 64, 262_144, 8_388_608] {
            hasher.update(&value.to_be_bytes());
        }
        ObjectId::from_bytes(hasher.finalize().as_bytes()).expect("BLAKE3 is 32 bytes")
    }

    pub fn mapping_bytes(tag: u8, body: &[u8]) -> CoreResult<Vec<u8>> {
        let capacity = 11usize
            .checked_add(body.len())
            .ok_or(CoreError::LengthOverflow)?;
        let mut output = Vec::with_capacity(capacity);
        output.extend_from_slice(&v1::MAPPING_MAGIC);
        output.extend_from_slice(&MAPPING_VERSION.to_be_bytes());
        output.push(tag);
        output.extend_from_slice(body);
        Ok(output)
    }

    pub fn decode_mapping(canonical: &[u8], expected_tag: u8) -> CoreResult<&[u8]> {
        let inner = decode_bytes_object(canonical)?;
        if inner.len() < 11 {
            return Err(CoreError::UnexpectedEof);
        }
        if inner[..8] != v1::MAPPING_MAGIC {
            return Err(CoreError::InvalidMappingTag { tag: 0 });
        }
        let version = u16::from_be_bytes([inner[8], inner[9]]);
        if version != MAPPING_VERSION {
            return Err(CoreError::UnsupportedMappingVersion { version });
        }
        if inner[10] != expected_tag {
            return Err(match inner[10] {
                v1::FILE_ROOT_TAG
                | v1::FILE_LEAF_TAG
                | v1::FILE_BRANCH_TAG
                | v1::DIR_INDEX_TAG
                | v1::DIR_METADATA_TAG
                | v1::DELTA_INDEX_TAG
                | v1::DELTA_PAGE_TAG => CoreError::WrongLogicalRole,
                tag => CoreError::InvalidMappingTag { tag },
            });
        }
        Ok(&inner[11..])
    }

    pub fn encode_file_leaf(references: &[FileReference]) -> CoreResult<Vec<u8>> {
        validate_file_leaf(references, true)?;
        let capacity = 4usize
            .checked_add(
                references
                    .len()
                    .checked_mul(FILE_REF_BYTES)
                    .ok_or(CoreError::LengthOverflow)?,
            )
            .ok_or(CoreError::LengthOverflow)?;
        let mut body = Vec::with_capacity(capacity);
        body.extend_from_slice(
            &u32::try_from(references.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for reference in references {
            body.extend_from_slice(&reference.raw_length.to_be_bytes());
            body.extend_from_slice(reference.object_id.as_bytes());
        }
        mapping_bytes(FILE_LEAF_TAG, &body)
    }

    pub fn parse_file_leaf(payload: &[u8]) -> CoreResult<Vec<FileReference>> {
        let count_bytes = payload.get(..4).ok_or(CoreError::UnexpectedEof)?;
        let count = usize::try_from(u32::from_be_bytes(
            count_bytes
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        ))
        .map_err(|_| CoreError::LengthOverflow)?;
        if count > MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let expected = 4usize
            .checked_add(
                count
                    .checked_mul(FILE_REF_BYTES)
                    .ok_or(CoreError::LengthOverflow)?,
            )
            .ok_or(CoreError::LengthOverflow)?;
        if payload.len() != expected {
            return Err(if payload.len() < expected {
                CoreError::UnexpectedEof
            } else {
                CoreError::TrailingBytes
            });
        }
        payload[4..]
            .chunks_exact(FILE_REF_BYTES)
            .map(|bytes| {
                let raw_length = u32::from_be_bytes(
                    bytes[..4]
                        .try_into()
                        .map_err(|_| CoreError::UnexpectedEof)?,
                );
                if usize::try_from(raw_length).map_err(|_| CoreError::LengthOverflow)?
                    > MAXIMUM_CHUNK_BYTES
                {
                    return Err(CoreError::ObjectLimitExceeded);
                }
                Ok(FileReference {
                    raw_length,
                    object_id: ObjectId::from_bytes(&bytes[4..])?,
                })
            })
            .collect()
    }

    pub fn validate_file_leaf(
        references: &[FileReference],
        final_leaf: bool,
    ) -> CoreResult<(u64, u64)> {
        if references.is_empty()
            || references.len() > FILE_LEAF_CAPACITY
            || (!final_leaf && references.len() != FILE_LEAF_CAPACITY)
        {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        let mut total = 0_u64;
        for reference in references {
            if usize::try_from(reference.raw_length).map_err(|_| CoreError::LengthOverflow)?
                > MAXIMUM_CHUNK_BYTES
            {
                return Err(CoreError::ObjectLimitExceeded);
            }
            total = total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
        }
        Ok((
            u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?,
            total,
        ))
    }

    fn strict_children(children: &[FileChild]) -> CoreResult<()> {
        if children.is_empty() || children.len() > FILE_BRANCH_CAPACITY {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        if children
            .windows(2)
            .any(|pair| pair[0].cumulative_end > pair[1].cumulative_end)
        {
            return Err(CoreError::NonCanonicalOrdering);
        }
        Ok(())
    }

    pub fn encode_file_branch(level: u8, children: &[FileChild]) -> CoreResult<Vec<u8>> {
        if level == 0 {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        strict_children(children)?;
        let mut body = Vec::with_capacity(
            5usize
                .checked_add(
                    children
                        .len()
                        .checked_mul(FILE_DESCRIPTOR_BYTES)
                        .ok_or(CoreError::LengthOverflow)?,
                )
                .ok_or(CoreError::LengthOverflow)?,
        );
        body.push(level);
        body.extend_from_slice(
            &u32::try_from(children.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for child in children {
            child.encode(&mut body);
        }
        mapping_bytes(FILE_BRANCH_TAG, &body)
    }

    pub fn encode_file_root(
        mode: u32,
        total_raw: u64,
        reference_count: u64,
        level: u8,
        children: &[FileChild],
    ) -> CoreResult<Vec<u8>> {
        if reference_count == 0 {
            if total_raw != 0 || level != 0 || !children.is_empty() {
                return Err(CoreError::NonCanonicalPagePartition);
            }
        } else {
            strict_children(children)?;
            if level != expected_file_level(reference_count)? {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            let final_end = children.last().map_or(0, |child| child.cumulative_end);
            if final_end != total_raw {
                return Err(CoreError::LengthMismatch {
                    expected: total_raw,
                    actual: final_end,
                });
            }
        }
        let mut body = Vec::with_capacity(
            25usize
                .checked_add(
                    children
                        .len()
                        .checked_mul(FILE_DESCRIPTOR_BYTES)
                        .ok_or(CoreError::LengthOverflow)?,
                )
                .ok_or(CoreError::LengthOverflow)?,
        );
        body.extend_from_slice(&mode.to_be_bytes());
        body.extend_from_slice(&total_raw.to_be_bytes());
        body.extend_from_slice(&reference_count.to_be_bytes());
        body.push(level);
        body.extend_from_slice(
            &u32::try_from(children.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for child in children {
            child.encode(&mut body);
        }
        mapping_bytes(FILE_ROOT_TAG, &body)
    }
}

pub mod dir_codec {
    use crate::canonical_v2::file_codec::{mapping_bytes, DIR_INDEX_TAG, DIR_METADATA_TAG};
    use crate::cow::persistence as v1;
    use crate::{CoreError, CoreResult};

    pub use v1::{
        encode_directory_page, encode_directory_wrapper, parse_directory_index, DirectoryPageRef,
        DirectoryPartitionValidator,
    };

    pub fn encode_directory_metadata(mode: u32) -> CoreResult<Vec<u8>> {
        mapping_bytes(DIR_METADATA_TAG, &mode.to_be_bytes())
    }

    pub fn encode_directory_index(
        total_entries: u32,
        pages: &[DirectoryPageRef],
    ) -> CoreResult<Vec<u8>> {
        let mut body = Vec::new();
        body.extend_from_slice(&total_entries.to_be_bytes());
        body.extend_from_slice(
            &u32::try_from(pages.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for page in pages {
            body.extend_from_slice(&page.count.to_be_bytes());
            body.extend_from_slice(
                &u16::try_from(page.first_name.len())
                    .map_err(|_| CoreError::LengthOverflow)?
                    .to_be_bytes(),
            );
            body.extend_from_slice(&page.first_name);
            body.extend_from_slice(page.object_id.as_bytes());
        }
        parse_directory_index(&body)?;
        mapping_bytes(DIR_INDEX_TAG, &body)
    }
}

pub mod delta_codec {
    use crate::canonical_v2::file_codec::{decode_mapping, mapping_bytes};
    use crate::content::persistence::{DELTA_INDEX_TAG, DELTA_PAGE_TAG};
    use crate::delta::codec as v1;
    use crate::{CanonicalPath, CoreError, CoreResult, ObjectId};

    pub use v1::{replay_durable_transition, DecodedTransition, TransitionOperation};

    pub fn encode_genesis(child: ObjectId) -> CoreResult<Vec<u8>> {
        encode_transition(None, child, 0, &[])
    }

    pub fn encode_change(
        parent: ObjectId,
        child: ObjectId,
        entry_count: u32,
        pages: &[ObjectId],
    ) -> CoreResult<Vec<u8>> {
        encode_transition(Some(parent), child, entry_count, pages)
    }

    fn encode_transition(
        parent: Option<ObjectId>,
        child: ObjectId,
        entry_count: u32,
        pages: &[ObjectId],
    ) -> CoreResult<Vec<u8>> {
        if (entry_count == 0) != pages.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        if parent.is_none() && (entry_count != 0 || !pages.is_empty()) {
            return Err(CoreError::DeltaConflict);
        }
        let mut body = Vec::new();
        body.push(u8::from(parent.is_some()));
        if let Some(parent) = parent {
            body.extend_from_slice(parent.as_bytes());
        }
        body.extend_from_slice(child.as_bytes());
        body.extend_from_slice(&entry_count.to_be_bytes());
        body.extend_from_slice(
            &u32::try_from(pages.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for page in pages {
            body.extend_from_slice(page.as_bytes());
        }
        v1::decode_transition(&body)?;
        mapping_bytes(DELTA_INDEX_TAG, &body)
    }

    pub fn decode_mapping_transition(canonical: &[u8]) -> CoreResult<DecodedTransition> {
        v1::decode_transition(decode_mapping(canonical, DELTA_INDEX_TAG)?)
    }

    pub fn measure_mapping_transition_pages(canonical: &[u8]) -> CoreResult<usize> {
        Ok(decode_mapping_transition(canonical)?.pages.len())
    }

    pub fn encode_delta_page(entries: &[TransitionOperation]) -> CoreResult<Vec<u8>> {
        if entries.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        let mut body = Vec::new();
        body.extend_from_slice(
            &u32::try_from(entries.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for entry in entries {
            encode_entry(entry, &mut body)?;
        }
        v1::decode_delta_page(&body)?;
        mapping_bytes(DELTA_PAGE_TAG, &body)
    }

    pub fn decode_mapping_delta_page(canonical: &[u8]) -> CoreResult<Vec<TransitionOperation>> {
        v1::decode_delta_page(decode_mapping(canonical, DELTA_PAGE_TAG)?)
    }

    pub fn measure_mapping_delta_page(canonical: &[u8]) -> CoreResult<(usize, usize)> {
        let entries = decode_mapping_delta_page(canonical)?;
        let path_bytes = entries.iter().try_fold(0usize, |sum, entry| {
            let path = match entry {
                TransitionOperation::Add { path, .. }
                | TransitionOperation::Remove { path, .. }
                | TransitionOperation::Replace { path, .. }
                | TransitionOperation::Metadata { path, .. } => path,
            };
            sum.checked_add(path.len()).ok_or(CoreError::LengthOverflow)
        })?;
        Ok((entries.len(), path_bytes))
    }

    fn encode_entry(entry: &TransitionOperation, output: &mut Vec<u8>) -> CoreResult<()> {
        match entry {
            TransitionOperation::Add { path, after } => {
                encode_path(output, 0x01, path)?;
                output.extend_from_slice(after.as_bytes());
            }
            TransitionOperation::Remove { path, before } => {
                encode_path(output, 0x02, path)?;
                output.extend_from_slice(before.as_bytes());
            }
            TransitionOperation::Replace {
                path,
                before,
                after,
            } => {
                encode_path(output, 0x03, path)?;
                output.extend_from_slice(before.as_bytes());
                output.extend_from_slice(after.as_bytes());
            }
            TransitionOperation::Metadata {
                path,
                before,
                before_mode,
                after,
                after_mode,
            } => {
                encode_path(output, 0x04, path)?;
                output.extend_from_slice(before.as_bytes());
                output.extend_from_slice(&before_mode.to_be_bytes());
                output.extend_from_slice(after.as_bytes());
                output.extend_from_slice(&after_mode.to_be_bytes());
            }
        }
        Ok(())
    }

    fn encode_path(output: &mut Vec<u8>, tag: u8, path: &[u8]) -> CoreResult<()> {
        CanonicalPath::from_bytes(path)?;
        output.push(tag);
        output.extend_from_slice(
            &u32::try_from(path.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        output.extend_from_slice(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{delta_codec, dir_codec, file_codec};
    use crate::{encode_bytes_object, validate_bytes_identity, CoreError, ObjectId};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn native_v2_codecs_are_strict_and_never_translate_v1() {
        let chunk = encode_bytes_object(b"payload").unwrap();
        let reference = file_codec::FileReference {
            raw_length: 7,
            object_id: ObjectId::for_bytes(&chunk),
        };
        let leaf =
            encode_bytes_object(&file_codec::encode_file_leaf(&[reference]).unwrap()).unwrap();
        assert_eq!(
            file_codec::parse_file_leaf(
                file_codec::decode_mapping(&leaf, file_codec::FILE_LEAF_TAG).unwrap()
            )
            .unwrap(),
            vec![reference]
        );
        assert_eq!(
            file_codec::decode_mapping(&leaf, file_codec::FILE_ROOT_TAG),
            Err(CoreError::WrongLogicalRole)
        );

        let metadata =
            encode_bytes_object(&dir_codec::encode_directory_metadata(0o644).unwrap()).unwrap();
        assert_eq!(
            file_codec::decode_mapping(&metadata, file_codec::DIR_METADATA_TAG).unwrap(),
            0o644_u32.to_be_bytes()
        );

        let child = ObjectId::for_bytes(b"child");
        let genesis = encode_bytes_object(&delta_codec::encode_genesis(child).unwrap()).unwrap();
        assert_eq!(
            delta_codec::decode_mapping_transition(&genesis)
                .unwrap()
                .child,
            child
        );
        assert_eq!(
            file_codec::decode_mapping(&genesis, file_codec::FILE_ROOT_TAG),
            Err(CoreError::WrongLogicalRole)
        );
    }

    #[test]
    fn literal_vectors_and_error_precedence_are_exact() {
        let abc = encode_bytes_object(b"abc").unwrap();
        let reference = file_codec::FileReference {
            raw_length: 3,
            object_id: ObjectId::for_bytes(&abc),
        };
        assert_eq!(
            hex(&encode_bytes_object(&file_codec::encode_file_leaf(&[reference]).unwrap()).unwrap()),
            "4c46534f0100000037000000334c4653344d415000000202000000010000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b"
        );
        assert_eq!(
            hex(&encode_bytes_object(&file_codec::encode_file_root(0, 0, 0, 0, &[]).unwrap()).unwrap()),
            "4c46534f0100000028000000244c4653344d41500000020100000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            file_codec::selected_mapping_profile_id().to_string(),
            "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
        );
        assert_eq!(
            hex(
                &encode_bytes_object(&dir_codec::encode_directory_metadata(0x1234_5678).unwrap())
                    .unwrap()
            ),
            "4c46534f01000000130000000f4c4653344d41500000020412345678"
        );
        assert_eq!(
            hex(&encode_bytes_object(&delta_codec::encode_genesis(ObjectId::from_bytes(&[0; 32]).unwrap()).unwrap()).unwrap()),
            "4c46534f0100000038000000344c4653344d4150000002050000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );

        let mut wrong_version = file_codec::encode_file_root(0, 0, 0, 0, &[]).unwrap();
        wrong_version[8..10].copy_from_slice(&1_u16.to_be_bytes());
        let wrong_version = encode_bytes_object(&wrong_version).unwrap();
        assert_eq!(
            file_codec::decode_mapping(&wrong_version, file_codec::FILE_ROOT_TAG),
            Err(CoreError::UnsupportedMappingVersion { version: 1 })
        );
        assert_eq!(
            file_codec::decode_mapping(
                &encode_bytes_object(
                    &file_codec::mapping_bytes(file_codec::DIR_INDEX_TAG, &[]).unwrap()
                )
                .unwrap(),
                file_codec::FILE_ROOT_TAG,
            ),
            Err(CoreError::WrongLogicalRole)
        );
        assert_eq!(
            validate_bytes_identity(&wrong_version, ObjectId::for_bytes(b"wrong")),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(
            file_codec::encode_file_leaf(&[file_codec::FileReference {
                raw_length: 32_769,
                object_id: ObjectId::for_bytes(b"oversized"),
            }]),
            Err(CoreError::ObjectLimitExceeded)
        );
        assert_eq!(
            file_codec::encode_file_branch(
                0,
                &[file_codec::FileChild {
                    object_id: ObjectId::for_bytes(b"child"),
                    cumulative_end: 1,
                }],
            ),
            Err(CoreError::NonCanonicalPagePartition)
        );
        assert_eq!(
            file_codec::encode_file_root(
                0,
                2,
                1,
                0,
                &[file_codec::FileChild {
                    object_id: ObjectId::for_bytes(b"child"),
                    cumulative_end: 1,
                }],
            ),
            Err(CoreError::LengthMismatch {
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(
            delta_codec::encode_change(
                ObjectId::for_bytes(b"parent"),
                ObjectId::for_bytes(b"child"),
                1,
                &[],
            ),
            Err(CoreError::NonCanonicalPagePartition)
        );
        let mut bad_magic = file_codec::encode_file_root(0, 0, 0, 0, &[]).unwrap();
        bad_magic[0] ^= 1;
        assert_eq!(
            file_codec::decode_mapping(
                &encode_bytes_object(&bad_magic).unwrap(),
                file_codec::FILE_ROOT_TAG,
            ),
            Err(CoreError::InvalidMappingTag { tag: 0 })
        );
        assert_eq!(
            file_codec::decode_mapping(
                &encode_bytes_object(&file_codec::mapping_bytes(0xff, &[]).unwrap()).unwrap(),
                file_codec::FILE_ROOT_TAG,
            ),
            Err(CoreError::InvalidMappingTag { tag: 0xff })
        );
        let mut transition = vec![0];
        transition.extend_from_slice(&[0; 32]);
        transition.extend_from_slice(&100_001_u32.to_be_bytes());
        transition.extend_from_slice(&100_001_u32.to_be_bytes());
        let transition = encode_bytes_object(
            &file_codec::mapping_bytes(crate::content::persistence::DELTA_INDEX_TAG, &transition)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            delta_codec::decode_mapping_transition(&transition),
            Err(CoreError::ObjectLimitExceeded)
        );
    }
}
