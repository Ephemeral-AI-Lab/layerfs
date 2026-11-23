//! A payload record's edge read from bounded ranges agrees with the reader
//! that borrows the complete record.
//!
//! `header_base` must accept and refuse exactly the records `stored_base`
//! does, and `sparse_edge` must report the base and width the complete group
//! gives, whatever pieces of the pack the caller happens to hold. A group's
//! directory is parsed once, so asking for every record of a group costs one
//! pass over it and not one pass per record.
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::encoding::delta::edge::{
    has_sparse_edges, sparse_directory, sparse_edge, RecordDirectory, Sparse, HEAD_BYTES,
};
use layerfs_storage::encoding::delta::read::ChainEdge;
use layerfs_storage::encoding::delta::record::{
    header_base, stored_base, FULL_TAG, HEADER_BYTES, PREFIX_TAG, STORED_TAG,
};
use layerfs_storage::encoding::framed_record;
use layerfs_storage::location::ObjectLocation;
use layerfs_storage::pack::layout::{GroupCodec, GroupView, PackLane};
use layerfs_storage::policy::WHOLE_FILE_CANONICAL_OVERHEAD;

const PAYLOAD_LANES: [PackLane; 3] = [PackLane::WholeFile, PackLane::Chunk, PackLane::Singleton];

fn base() -> ObjectId {
    ObjectId::for_bytes(b"sparse-edges/base")
}

/// One record of `lane` as the encoder frames it, with every field free so a
/// case can break exactly one of them.
fn record(lane: PackLane, tag: u8, raw: u32, declared_frame: u32, frame: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    if lane != PackLane::WholeFile {
        out.extend_from_slice(&raw.to_le_bytes());
    }
    if lane == PackLane::Singleton {
        out.extend_from_slice(&declared_frame.to_le_bytes());
    }
    if tag == PREFIX_TAG {
        out.extend_from_slice(&base().to_bytes());
    }
    out.extend_from_slice(frame);
    out
}

fn header_of(record: &[u8]) -> &[u8] {
    &record[..record.len().min(HEADER_BYTES)]
}

#[test]
fn the_header_reader_accepts_and_refuses_what_the_record_reader_does() {
    let mut accepted = 0;
    let mut refused = 0;
    for lane in PAYLOAD_LANES {
        for tag in [FULL_TAG, PREFIX_TAG, STORED_TAG, 3, 255] {
            for raw in [0u32, 1, 70_000] {
                for frame_length in [0usize, 1, 7, 64] {
                    for declared in [frame_length as u32, frame_length as u32 + 1] {
                        for canonical in [
                            0,
                            WHOLE_FILE_CANONICAL_OVERHEAD,
                            WHOLE_FILE_CANONICAL_OVERHEAD + 1,
                            WHOLE_FILE_CANONICAL_OVERHEAD + 70_000,
                        ] {
                            let frame = vec![0xA5; frame_length];
                            let whole = record(lane, tag, raw, declared, &frame);
                            // Every truncation too: a short record must be
                            // refused by both, never read past by either.
                            for cut in 0..=whole.len().min(HEADER_BYTES + 2) {
                                let record = if cut == whole.len().min(HEADER_BYTES + 2) {
                                    &whole[..]
                                } else {
                                    &whole[..cut]
                                };
                                let complete =
                                    stored_base(lane, record, canonical, ObjectRole::WholeFile);
                                let header =
                                    header_base(lane, header_of(record), record.len(), canonical);
                                match (&complete, &header) {
                                    (Ok(left), Ok(right)) => {
                                        assert_eq!(left, right);
                                        accepted += 1;
                                    }
                                    (Err(_), Err(_)) => refused += 1,
                                    _ => panic!(
                                        "{lane:?} tag {tag} raw {raw} frame {frame_length} \
                                         declared {declared} canonical {canonical} cut {cut}: \
                                         complete {complete:?}, header {header:?}"
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(accepted > 100 && refused > 100, "{accepted} {refused}");
}

#[test]
fn the_header_reader_refuses_a_header_of_the_wrong_width_and_the_ordinary_lane() {
    let whole = record(PackLane::Chunk, PREFIX_TAG, 9, 0, &[1; 200]);
    assert_eq!(
        header_base(PackLane::Chunk, header_of(&whole), whole.len(), 0).unwrap(),
        Some(base())
    );
    // One byte short or long of the declared prefix is not the record's header.
    assert!(header_base(PackLane::Chunk, &whole[..HEADER_BYTES - 1], whole.len(), 0).is_err());
    assert!(header_base(PackLane::Chunk, &whole[..HEADER_BYTES + 1], whole.len(), 0).is_err());
    assert!(header_base(PackLane::Ordinary, &[FULL_TAG, 1], 2, 0).is_err());
}

/// A Raw group body: the record count, each record's end offset, the records.
fn group(records: &[Vec<u8>]) -> Vec<u8> {
    let mut out = (records.len() as u32).to_le_bytes().to_vec();
    let mut end = 0u32;
    for record in records {
        end += record.len() as u32;
        out.extend_from_slice(&end.to_le_bytes());
    }
    for record in records {
        out.extend_from_slice(record);
    }
    out
}

fn location(record_number: usize, canonical_length: usize) -> ObjectLocation {
    ObjectLocation {
        object_id: ObjectId::for_bytes(&record_number.to_le_bytes()),
        role: ObjectRole::Chunk,
        canonical_length,
        pack_id: 7,
        group_number: 0,
        record_number,
        base: None,
    }
}

/// Runs the sparse readers the way a walk does: start with nothing held,
/// acquire each range they name, and count the acquisitions.
fn walk(
    lane: PackLane,
    view: GroupView,
    pack: &[u8],
    location: &ObjectLocation,
) -> (ChainEdge, usize, usize) {
    let mut held: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut acquired = 0;
    for round in 0..8 {
        let have = |offset: usize, length: usize| {
            held.iter().find_map(|(at, bytes)| {
                let from = offset.checked_sub(*at)?;
                bytes.get(from..from + length)
            })
        };
        let range = match sparse_directory(view, have).unwrap() {
            Sparse::Need(range) => range,
            Sparse::Ready(directory) => {
                match sparse_edge(lane, view, &directory, location, have).unwrap() {
                    Sparse::Ready(edge) => return (edge, round, acquired),
                    Sparse::Need(range) => range,
                }
            }
        };
        assert!(range.offset >= view.start && range.offset + range.length <= view.end);
        acquired += range.length;
        held.push((
            range.offset,
            pack[range.offset..range.offset + range.length].to_vec(),
        ));
    }
    panic!("the walk never settled");
}

#[test]
fn a_sparse_walk_reports_the_complete_groups_edge_without_its_payload() {
    for lane in [PackLane::WholeFile, PackLane::Chunk] {
        for count in [1usize, 2, 5, 130, 400] {
            let canonical = WHOLE_FILE_CANONICAL_OVERHEAD + 9_000;
            let records: Vec<Vec<u8>> = (0..count)
                .map(|index| {
                    let tag = [FULL_TAG, PREFIX_TAG, STORED_TAG][index % 3];
                    // Widths from one byte of frame to several kilobytes.
                    let frame = vec![index as u8; 1 + (index * 37) % 9_000];
                    record(lane, tag, 9_000, 0, &frame)
                })
                .collect();
            let body = group(&records);
            let start = 40;
            let mut pack = vec![0xEE; start];
            pack.extend_from_slice(&body);
            pack.extend_from_slice(&[0xEE; 13]);
            let view = GroupView {
                start,
                end: start + body.len(),
                decoded_length: body.len(),
                codec: GroupCodec::Raw,
            };
            let directory = 4 + 4 * count;
            for number in [0, count / 2, count - 1] {
                let location = location(number, canonical);
                let complete = framed_record(&body, number).unwrap();
                let expected = stored_base(lane, complete, canonical, location.role).unwrap();
                let (edge, rounds, acquired) = walk(lane, view, &pack, &location);
                assert_eq!(edge.base, expected, "{lane:?} {count} {number}");
                assert_eq!(edge.width, complete.len() as u64);
                assert!(rounds <= 3, "{rounds} rounds");
                // The head, at most a wider directory, at most one header:
                // never the payload behind them.
                assert!(
                    acquired <= HEAD_BYTES + directory + 2 * HEADER_BYTES,
                    "{lane:?} {count} {number}: {acquired} of {}",
                    body.len()
                );
            }
            // An ordinal the directory does not hold is refused, as it is by
            // the complete reader.
            let held = |offset: usize, length: usize| pack.get(offset..offset + length);
            let Sparse::Ready(directory) = sparse_directory(view, held).unwrap() else {
                panic!("the whole pack is held");
            };
            assert_eq!(directory, RecordDirectory::of(&body).unwrap());
            assert_eq!(directory.count(), count);
            // Every record's range is the framing reader's.
            for number in 0..count {
                let (from, to) = directory.range(number).unwrap();
                assert_eq!(&body[from..to], framed_record(&body, number).unwrap());
            }
            let past = location(count, canonical);
            assert!(sparse_edge(lane, view, &directory, &past, held).is_err());
            assert!(directory.range(count).is_err());
            assert!(framed_record(&body, count).is_err());
        }
    }
}

#[test]
fn a_broken_directory_is_refused_and_an_encoded_group_is_left_to_the_decoder() {
    let records = vec![
        record(PackLane::Chunk, FULL_TAG, 5, 0, &[1; 30]),
        record(PackLane::Chunk, FULL_TAG, 5, 0, &[2; 30]),
    ];
    let body = group(&records);
    let view = |codec| GroupView {
        start: 0,
        end: body.len(),
        decoded_length: body.len(),
        codec,
    };
    let location = location(1, 0);
    let raw = view(GroupCodec::Raw);
    let held = |offset: usize, length: usize| body.get(offset..offset + length);
    let directory = RecordDirectory::of(&body).unwrap();
    assert!(matches!(
        sparse_edge(PackLane::Chunk, raw, &directory, &location, held).unwrap(),
        Sparse::Ready(_)
    ));
    // A compressed group and the Ordinary lane have no edge to read this way:
    // the caller asks first, and the reader refuses rather than guess.
    assert!(has_sparse_edges(PackLane::Chunk, raw));
    assert!(!has_sparse_edges(
        PackLane::Chunk,
        view(GroupCodec::Zstandard)
    ));
    assert!(!has_sparse_edges(PackLane::Ordinary, raw));
    let compressed = view(GroupCodec::Zstandard);
    assert!(sparse_edge(PackLane::Chunk, compressed, &directory, &location, held).is_err());
    assert!(sparse_edge(PackLane::Ordinary, raw, &directory, &location, held).is_err());
    // A record count of zero, one past the body, and an end that runs backwards.
    for (at, value) in [(0usize, 0u32), (0, 9_999), (4, 0), (8, 3)] {
        let mut broken = body.clone();
        broken[at..at + 4].copy_from_slice(&value.to_le_bytes());
        let held = |offset: usize, length: usize| broken.get(offset..offset + length);
        assert!(
            sparse_directory(raw, held).is_err(),
            "field at {at} = {value}"
        );
        assert!(
            RecordDirectory::of(&broken).is_err(),
            "field at {at} = {value}"
        );
        assert!(
            framed_record(&broken, 1).is_err(),
            "field at {at} = {value}"
        );
    }
    // A group shorter than its count field, and one truncated inside its
    // directory, are refused by both readers.
    for cut in [0, 3, 7, 11] {
        assert!(RecordDirectory::of(&body[..cut]).is_err(), "cut {cut}");
        assert!(framed_record(&body[..cut], 0).is_err(), "cut {cut}");
    }
    // A singleton pack holds one record: any other ordinal is refused.
    assert!(sparse_edge(PackLane::Singleton, raw, &directory, &location, held).is_err());
}

#[test]
fn every_record_of_a_full_group_is_answered_in_one_pass_over_its_directory() {
    // The widest group the format admits. One directory parse answers all of
    // its records; asking the framing reader for each one scans the directory
    // once per record, which is what this reader exists to avoid.
    let count = layerfs_storage::policy::RECORD_COUNT_LIMIT;
    let records: Vec<Vec<u8>> = (0..count)
        .map(|index| {
            let tag = [FULL_TAG, PREFIX_TAG][index % 2];
            record(PackLane::Chunk, tag, 7, 0, &[index as u8; 3])
        })
        .collect();
    let body = group(&records);
    let view = GroupView {
        start: 0,
        end: body.len(),
        decoded_length: body.len(),
        codec: GroupCodec::Raw,
    };
    let held = |offset: usize, length: usize| body.get(offset..offset + length);
    let Sparse::Ready(directory) = sparse_directory(view, held).unwrap() else {
        panic!("the whole group is held");
    };
    assert_eq!(directory.count(), count);
    let mut bases = 0;
    let mut width = 0;
    for number in 0..count {
        let location = location(number, 0);
        let Sparse::Ready(edge) =
            sparse_edge(PackLane::Chunk, view, &directory, &location, held).unwrap()
        else {
            panic!("record {number}");
        };
        bases += usize::from(edge.base == Some(base()));
        width += edge.width as usize;
    }
    assert_eq!(bases, count / 2);
    assert_eq!(width, body.len() - 4 - 4 * count);
}
