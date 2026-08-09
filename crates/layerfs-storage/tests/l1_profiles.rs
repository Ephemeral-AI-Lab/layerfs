use layerfs_storage::profile::{
    ChunkerSpecV1, DigestSpecV1, ProfileSpecV1, CHUNKER_SPEC_BYTES, DIGEST_SPEC_BYTES,
    PROFILE_SPEC_BYTES,
};
use layerfs_storage::CoreError;

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
fn frozen_profile_records_have_exact_lengths_seals_and_ids() {
    let digest = DigestSpecV1::frozen();
    let chunker = ChunkerSpecV1::frozen();
    let profile = ProfileSpecV1::frozen();

    assert_eq!(digest.canonical_bytes().len(), DIGEST_SPEC_BYTES);
    assert_eq!(chunker.canonical_bytes().len(), CHUNKER_SPEC_BYTES);
    assert_eq!(profile.canonical_bytes().len(), PROFILE_SPEC_BYTES);
    assert_eq!(
        sha256(digest.canonical_bytes()),
        expected("2d9ac18268aeffbf22c8a75391d4fe3d6e384334309a241f89f0276b4022e515")
    );
    assert_eq!(
        sha256(chunker.canonical_bytes()),
        expected("6c4fe77a0d015024d2d4dc60f0186343c0214dbf5c681d3446785b68fd479f84")
    );
    assert_eq!(
        sha256(profile.canonical_bytes()),
        expected("726b30797b6eac121dd2ccd65e48a8632e38a04a8157750bcfec08e58797fc94")
    );
    assert_eq!(
        digest.id().as_bytes(),
        &expected("ea17622d03b3baaacf09dff8877df6d8834306c32d9ed5d950a6a15ef01ad5cb")
    );
    assert_eq!(
        chunker.id().as_bytes(),
        &expected("88b1ca8e3b1d9076916818a484b907e9bc6913fe54013ced176c6d9eb23408e7")
    );
    assert_eq!(
        profile.id().as_bytes(),
        &expected("3d372f239a0e55b7001f0cb89648de46650de4c43421d645c927a2f7d0d8702b")
    );
}

#[test]
fn frozen_gear_and_gear_ls_table_seals_are_exact() {
    let chunker = ChunkerSpecV1::frozen();
    let raw_table = &chunker.canonical_bytes()[68..];
    assert_eq!(raw_table.len(), 256 * 8);
    assert_eq!(
        sha256(raw_table),
        expected("9df0a720752a7d211fdebaf39bed01610983756fc340a1cfef41052b7356ae73")
    );

    let mut shifted = Vec::with_capacity(raw_table.len());
    let mut interleaved = Vec::with_capacity(raw_table.len() * 2);
    for word in raw_table.chunks_exact(8) {
        let gear = u64::from_be_bytes(word.try_into().expect("eight-byte GEAR word"));
        let gear_ls = gear.wrapping_shl(1).to_be_bytes();
        shifted.extend_from_slice(&gear_ls);
        interleaved.extend_from_slice(word);
        interleaved.extend_from_slice(&gear_ls);
    }
    assert_eq!(
        sha256(&shifted),
        expected("93123c215ae531383c1b660bb185d4013ba3c87faa99796879f97c4076bdfce2")
    );
    assert_eq!(
        sha256(&interleaved),
        expected("0ff906fefd2f6ce85c431130c1146e62746dc6e984d96d8d13ce7a55359d113a")
    );
}

#[test]
fn profile_decoders_fail_closed_on_mutations_and_non_exact_input() {
    let digest = *DigestSpecV1::frozen().canonical_bytes();
    assert_eq!(
        DigestSpecV1::decode_exact(&digest),
        Ok(DigestSpecV1::frozen())
    );
    assert_eq!(
        DigestSpecV1::decode_exact(&digest[..15]),
        Err(CoreError::Truncated)
    );
    let mut digest_trailing = digest.to_vec();
    digest_trailing.push(0);
    assert_eq!(
        DigestSpecV1::decode_exact(&digest_trailing),
        Err(CoreError::TrailingBytes)
    );
    let mut digest_reserved = digest;
    digest_reserved[15] = 1;
    assert_eq!(
        DigestSpecV1::decode_exact(&digest_reserved),
        Err(CoreError::Reserved)
    );

    let chunker = *ChunkerSpecV1::frozen().canonical_bytes();
    assert_eq!(
        ChunkerSpecV1::decode_exact(&chunker),
        Ok(ChunkerSpecV1::frozen())
    );
    assert_eq!(
        ChunkerSpecV1::decode_exact(&chunker[..2_115]),
        Err(CoreError::Truncated)
    );
    let mut chunker_mask = chunker;
    chunker_mask[39] ^= 1;
    assert_eq!(
        ChunkerSpecV1::decode_exact(&chunker_mask),
        Err(CoreError::TypeDomain)
    );
    let mut chunker_table = chunker;
    chunker_table[68] ^= 1;
    assert_eq!(
        ChunkerSpecV1::decode_exact(&chunker_table),
        Err(CoreError::TypeDomain)
    );

    let profile = *ProfileSpecV1::frozen().canonical_bytes();
    assert_eq!(
        ProfileSpecV1::decode_exact(&profile),
        Ok(ProfileSpecV1::frozen())
    );
    assert_eq!(
        ProfileSpecV1::decode_exact(&profile[..135]),
        Err(CoreError::Truncated)
    );
    let mut profile_flags = profile;
    profile_flags[11] = 1;
    assert_eq!(
        ProfileSpecV1::decode_exact(&profile_flags),
        Err(CoreError::Flags)
    );
    let mut profile_reserved = profile;
    profile_reserved[99] = 1;
    assert_eq!(
        ProfileSpecV1::decode_exact(&profile_reserved),
        Err(CoreError::Reserved)
    );
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = u64::try_from(input.len())
        .expect("test input fits u64")
        .checked_mul(8)
        .expect("test bit length fits u64");
    let padded_len = input
        .len()
        .checked_add(9)
        .and_then(|len| len.checked_add((64 - (len % 64)) % 64))
        .expect("test padding length fits usize");
    let mut padded = vec![0_u8; padded_len];
    padded[..input.len()].copy_from_slice(input);
    padded[input.len()] = 0x80;
    padded[padded_len - 8..].copy_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for block in padded.chunks_exact(64) {
        let mut schedule = [0_u32; 64];
        for (index, word) in block.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes(word.try_into().expect("four-byte SHA word"));
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temporary1 = h
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(ROUND[index])
                .wrapping_add(schedule[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary1);
            d = c;
            c = b;
            b = a;
            a = temporary1.wrapping_add(temporary2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut output = [0_u8; 32];
    for (slot, word) in output.chunks_exact_mut(4).zip(state) {
        slot.copy_from_slice(&word.to_be_bytes());
    }
    output
}
