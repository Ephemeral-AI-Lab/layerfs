use layerfs_content::file::cdc::{FastCdc, TARGET_CHUNK_BYTES};
use layerfs_content::{encode_bytes_object, ObjectId};
use std::io::Cursor;

const SOURCE_BYTES: usize = 4 * 1024 * 1024;
const PREFIX_BYTES: usize = 4_093;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Chunk {
    bytes: usize,
    id: ObjectId,
}

#[test]
fn shifted_stream_retains_a_frozen_canonical_suffix_that_fixed_blocks_lose() {
    let source = fixture(SOURCE_BYTES, 0x4c41_5945_5246_5346);
    let prefix = fixture(PREFIX_BYTES, 0x5348_4946_5445_4421);
    let mut shifted = prefix.clone();
    shifted.extend_from_slice(&source);

    let original = fastcdc(&source);
    let inserted = fastcdc(&shifted);
    let shared = common_suffix(&original, &inserted);
    let shared_bytes = original[original.len() - shared..]
        .iter()
        .map(|chunk| chunk.bytes)
        .sum::<usize>();

    let fixed_original = fixed(&source);
    let fixed_inserted = fixed(&shifted);
    let fixed_shared = common_suffix(&fixed_original, &fixed_inserted);

    let shared_ids = original[original.len() - shared..]
        .iter()
        .flat_map(|chunk| chunk.id.as_bytes())
        .copied()
        .collect::<Vec<_>>();
    let shared_digest = blake3::hash(&shared_ids).to_hex().to_string();
    eprintln!(
        "FASTCDC_SHIFTED_STREAM source={} prefix={} original_chunks={} shifted_chunks={} shared_suffix_chunks={} shared_suffix_bytes={} shared_suffix_digest={} fixed_shared_suffix_chunks={}",
        blake3::hash(&source).to_hex(),
        blake3::hash(&prefix).to_hex(),
        original.len(),
        inserted.len(),
        shared,
        shared_bytes,
        shared_digest,
        fixed_shared,
    );

    assert_eq!(
        blake3::hash(&source).to_hex().to_string(),
        "216e99ff21dfd04b8f759392a9921584d7a4641ad8667ae38123ea8742eb045f"
    );
    assert_eq!(
        blake3::hash(&prefix).to_hex().to_string(),
        "edb531680306a36957ab75e11afef04abde6cb00d8881477158fc4a7f8cad60e"
    );
    assert_eq!((original.len(), inserted.len()), (220, 220));
    assert_eq!(shared, 219);
    assert_eq!(shared_bytes, 4_175_722);
    assert_eq!(
        shared_digest,
        "61c91a10ca34fae310f4ab363fc5bfe35aa57a5fa2961d9fea3a11777509de2f"
    );
    assert_eq!(fixed_shared, 0);
}

fn fastcdc(bytes: &[u8]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    FastCdc::new()
        .scan(Cursor::new(bytes), |chunk| {
            chunks.push(Chunk {
                bytes: chunk.len(),
                id: ObjectId::for_bytes(&encode_bytes_object(chunk)?),
            });
            Ok(())
        })
        .unwrap();
    chunks
}

fn fixed(bytes: &[u8]) -> Vec<Chunk> {
    bytes
        .chunks(TARGET_CHUNK_BYTES)
        .map(|chunk| Chunk {
            bytes: chunk.len(),
            id: ObjectId::for_bytes(&encode_bytes_object(chunk).unwrap()),
        })
        .collect()
}

fn common_suffix(left: &[Chunk], right: &[Chunk]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

fn fixture(length: usize, mut state: u64) -> Vec<u8> {
    (0..length)
        .map(|index| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            (state as u8) ^ (index as u8).rotate_left((index % 7) as u32)
        })
        .collect()
}
