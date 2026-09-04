//! Independent CDC and flat extent oracle for preparation and verification.
//! Never called inside a measured product operation.
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Extent {
    pub id: ObjectId,
    pub source_offset: u64,
    pub len: u64,
    pub payload_len: u64,
}

// The frozen profile's format constants are shared; this window scanner does
// not call FastCdc, the rope builder, or the product mutation implementation.
pub(crate) fn scan(mut input: impl Read, mut emit: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
    let mut window = Vec::with_capacity(32768);
    let mut scratch = [0u8; 32768];
    let mut eof = false;
    loop {
        while window.len() < 32768 && !eof {
            let room = 32768 - window.len();
            match input.read(&mut scratch[..room]) {
                Ok(0) => eof = true,
                Ok(n) => window.extend_from_slice(&scratch[..n]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if window.is_empty() {
            break;
        }
        let mut cut = window.len();
        let mut position = 8192;
        let mut hash = 0u64;
        while position + 1 < window.len() {
            let small = position < 16384;
            hash = hash
                .wrapping_mul(4)
                .wrapping_add(GEAR[window[position] as usize].wrapping_mul(2));
            if hash
                & if small {
                    0x0001_b206_06a6_e000
                } else {
                    0x0001_b202_06a6_0000
                }
                == 0
            {
                cut = position;
                break;
            }
            hash = hash.wrapping_add(GEAR[window[position + 1] as usize]);
            if hash
                & if small {
                    0x0000_d903_0353_7000
                } else {
                    0x0000_d901_0353_0000
                }
                == 0
            {
                cut = position + 1;
                break;
            }
            position += 2;
        }
        emit(&window[..cut])?;
        window.drain(..cut);
    }
    Ok(())
}

pub(crate) fn transcript(input: impl Read) -> Result<Vec<Extent>> {
    let mut result = Vec::new();
    scan(input, |chunk| {
        let canonical = layerfs_content::file::extent_codec::encode_chunk_object(chunk)?;
        result.push(Extent {
            id: ObjectId::for_bytes(&canonical),
            source_offset: 0,
            len: chunk.len() as u64,
            payload_len: chunk.len() as u64,
        });
        Ok(())
    })?;
    Ok(result)
}

pub(crate) fn normalize(extents: &[Extent]) -> Result<Vec<Extent>> {
    let mut result: Vec<Extent> = Vec::with_capacity(extents.len());
    for extent in extents {
        if extent.len == 0
            || extent
                .source_offset
                .checked_add(extent.len)
                .ok_or("extent overflow")?
                > extent.payload_len
        {
            return Err("invalid payload extent".into());
        }
        if let Some(last) = result.last_mut() {
            if last.id == extent.id && last.payload_len != extent.payload_len {
                return Err("one ID has conflicting payload lengths".into());
            }
            if last.id == extent.id
                && last.source_offset.checked_add(last.len) == Some(extent.source_offset)
            {
                last.len = last.len.checked_add(extent.len).ok_or("extent overflow")?;
                continue;
            }
        }
        result.push(extent.clone());
    }
    Ok(result)
}

fn clip(extents: &[Extent], start: u64, end: u64) -> Result<Vec<Extent>> {
    let mut result = Vec::new();
    let mut position = 0u64;
    for extent in extents {
        let next = position
            .checked_add(extent.len)
            .ok_or("logical length overflow")?;
        let from = start.max(position);
        let to = end.min(next);
        if from < to {
            result.push(Extent {
                id: extent.id,
                source_offset: extent.source_offset + (from - position),
                len: to - from,
                payload_len: extent.payload_len,
            });
        }
        position = next;
    }
    if start > end || end > position {
        return Err("oracle clip range".into());
    }
    Ok(result)
}

pub(crate) fn splice_model(
    previous: &[Extent],
    start: u64,
    delete: u64,
    replacement: impl Read,
) -> Result<Vec<Extent>> {
    let previous = normalize(previous)?;
    let size = previous.iter().try_fold(0u64, |n, e| {
        n.checked_add(e.len).ok_or("logical length overflow")
    })?;
    let end = start.checked_add(delete).ok_or("edit range overflow")?;
    let mut result = clip(&previous, 0, start)?;
    result.extend(transcript(replacement)?);
    result.extend(clip(&previous, end, size)?);
    normalize(&result)
}

pub(crate) fn compare(expected: &[Extent], actual: &[Extent]) -> Result<()> {
    if normalize(expected)? != normalize(actual)? {
        return Err("actual payload transcript differs from independent oracle".into());
    }
    Ok(())
}

pub(crate) fn unique_payloads(extents: &[Extent]) -> Result<BTreeMap<ObjectId, u64>> {
    let mut result = BTreeMap::new();
    for e in normalize(extents)? {
        if let Some(previous) = result.insert(e.id, e.payload_len) {
            if previous != e.payload_len {
                return Err("conflicting canonical payload length".into());
            }
        }
    }
    Ok(result)
}

#[derive(Debug)]
pub(crate) struct Sharing {
    pub shared_ids: BTreeSet<ObjectId>,
    pub shared_logical_bytes: u64,
    pub prefix_chunks: usize,
    pub prefix_bytes: u64,
    pub suffix_chunks: usize,
    pub suffix_bytes: u64,
    pub suffix_base_position: Option<u64>,
    pub suffix_variant_position: Option<u64>,
}
pub(crate) fn sharing(base: &[Extent], variant: &[Extent]) -> Result<Sharing> {
    let base = normalize(base)?;
    let variant = normalize(variant)?;
    let ids = unique_payloads(&base)?;
    let shared_ids = variant
        .iter()
        .filter(|e| ids.contains_key(&e.id))
        .map(|e| e.id)
        .collect();
    let shared_logical_bytes = variant
        .iter()
        .filter(|e| ids.contains_key(&e.id))
        .map(|e| e.len)
        .sum();
    let prefix_chunks = base
        .iter()
        .zip(&variant)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix_chunks = base
        .iter()
        .rev()
        .zip(variant.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let prefix_bytes = variant[..prefix_chunks].iter().map(|e| e.len).sum();
    let suffix_bytes = variant[variant.len() - suffix_chunks..]
        .iter()
        .map(|e| e.len)
        .sum();
    let suffix_base_position = (suffix_chunks > 0).then(|| {
        base[..base.len() - suffix_chunks]
            .iter()
            .map(|e| e.len)
            .sum()
    });
    let suffix_variant_position = (suffix_chunks > 0).then(|| {
        variant[..variant.len() - suffix_chunks]
            .iter()
            .map(|e| e.len)
            .sum()
    });
    Ok(Sharing {
        shared_ids,
        shared_logical_bytes,
        prefix_chunks,
        prefix_bytes,
        suffix_chunks,
        suffix_bytes,
        suffix_base_position,
        suffix_variant_position,
    })
}

#[cfg(test)]
pub(crate) fn self_check() -> Result<()> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let data: Vec<_> = (0..100000)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect();
    let chunks = transcript(&data[..])?;
    if chunks.iter().map(|e| e.len).collect::<Vec<_>>()
        != [16396, 17093, 16413, 20273, 19016, 10809]
    {
        return Err("independent CDC frozen vector".into());
    }
    struct Fragment<'a>(&'a [u8]);
    impl Read for Fragment<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = out.len().min(self.0.len()).min(3);
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    compare(&chunks, &transcript(Fragment(&data))?)?;
    for len in [0, 1, 8191, 8192, 16384, 32768, 32769] {
        let actual = transcript(&data[..len])?;
        let mut product = Vec::new();
        layerfs_content::file::cdc::FastCdc::new().scan(&data[..len], |part| {
            product.push(part.len() as u64);
            Ok(())
        })?;
        if actual.iter().map(|e| e.len).collect::<Vec<_>>() != product
            || actual.iter().map(|e| e.len).sum::<u64>() != len as u64
        {
            return Err("independent CDC boundary qualification".into());
        }
    }
    let base = transcript(&data[..100])?;
    let next = splice_model(&base, 40, 20, &[0u8; 64][..])?;
    if next.len() != 3
        || next[0].len != 40
        || next[1].len != 64
        || next[2].source_offset != 60
        || next[2].len != 40
    {
        return Err("flat extent splice oracle".into());
    }
    if splice_model(&base, 101, 0, &[][..]).is_ok() {
        return Err("invalid flat extent range accepted".into());
    }
    let mut damaged = next.clone();
    damaged[2].source_offset -= 1;
    if compare(&next, &damaged).is_ok() {
        return Err("transcript mismatch accepted".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn independent_cdc_and_extent_oracle() {
        super::self_check().unwrap();
    }
}

// Frozen released GEAR table; data shared by format, scanner above independent.
const GEAR: [u64; 256] = [
    0x3b5d3c7d207e37dc,
    0x784d68ba91123086,
    0xcd52880f882e7298,
    0xeacf8e4e19fdcca7,
    0xc31f385dfbd1632b,
    0x1d5f27001e25abe6,
    0x83130bde3c9ad991,
    0xc4b225676e9b7649,
    0xaa329b29e08eb499,
    0xb67fcbd21e577d58,
    0x0027baaada2acf6b,
    0xe3ef2d5ac73c2226,
    0x0890f24d6ed312b7,
    0xa809e036851d7c7e,
    0xf0a6fe5e0013d81b,
    0x1d026304452cec14,
    0x03864632648e248f,
    0xcdaacf3dcd92b9b4,
    0xf5e012e63c187856,
    0x8862f9d3821c00b6,
    0xa82f7338750f6f8a,
    0x1e583dc6c1cb0b6f,
    0x7a3145b69743a7f1,
    0xabb20fee404807eb,
    0xb14b3cfe07b83a5d,
    0xb9dc27898adb9a0f,
    0x3703f5e91baa62be,
    0xcf0bb866815f7d98,
    0x3d9867c41ea9dcd3,
    0x1be1fa65442bf22c,
    0x14300da4c55631d9,
    0xe698e9cbc6545c99,
    0x4763107ec64e92a5,
    0xc65821fc65696a24,
    0x76196c064822f0b7,
    0x485be841f3525e01,
    0xf652bc9c85974ff5,
    0xcad8352face9e3e9,
    0x2a6ed1dceb35e98e,
    0xc6f483badc11680f,
    0x3cfd8c17e9cf12f1,
    0x89b83c5e2ea56471,
    0xae665cfd24e392a9,
    0xec33c4e504cb8915,
    0x3fb9b15fc9fe7451,
    0xd7fd1fd1945f2195,
    0x31ade0853443efd8,
    0x255efc9863e1e2d2,
    0x10eab6008d5642cf,
    0x46f04863257ac804,
    0xa52dc42a789a27d3,
    0xdaaadf9ce77af565,
    0x6b479cd53d87febb,
    0x6309e2d3f93db72f,
    0xc5738ffbaa1ff9d6,
    0x6bd57f3f25af7968,
    0x67605486d90d0a4a,
    0xe14d0b9663bfbdae,
    0xb7bbd8d816eb0414,
    0xdef8a4f16b35a116,
    0xe7932d85aaaffed6,
    0x08161cbae90cfd48,
    0x855507beb294f08b,
    0x91234ea6ffd399b2,
    0xad70cf4b2435f302,
    0xd289a97565bc2d27,
    0x8e558437ffca99de,
    0x96d2704b7115c040,
    0x0889bbcdfc660e41,
    0x5e0d4e67dc92128d,
    0x72a9f8917063ed97,
    0x438b69d409e016e3,
    0xdf4fed8a5d8a4397,
    0x00f41dcf41d403f7,
    0x4814eb038e52603f,
    0x9dafbacc58e2d651,
    0xfe2f458e4be170af,
    0x4457ec414df6a940,
    0x06e62f1451123314,
    0xbd1014d173ba92cc,
    0xdef318e25ed57760,
    0x9fea0de9dfca8525,
    0x459de1e76c20624b,
    0xaeec189617e2d666,
    0x126a2c06ab5a83cb,
    0xb1321532360f6132,
    0x65421503dbb40123,
    0x2d67c287ea089ab3,
    0x6c93bff5a56bd6b6,
    0x4ffb2036cab6d98d,
    0xce7b785b1be7ad4f,
    0xedb42ef6189fd163,
    0xdc905288703988f6,
    0x365f9c1d2c691884,
    0xc640583680d99bfe,
    0x3cd4624c07593ec6,
    0x7f1ea8d85d7c5805,
    0x014842d480b57149,
    0x0b649bcb5a828688,
    0xbcd5708ed79b18f0,
    0xe987c862fbd2f2f0,
    0x982731671f0cd82c,
    0xbaf13e8b16d8c063,
    0x8ea3109cbd951bba,
    0xd141045bfb385cad,
    0x2acbc1a0af1f7d30,
    0xe6444d89df03bfdf,
    0xa18cc771b8188ff9,
    0x9834429db01c39bb,
    0x214add07fe086a1f,
    0x8f07c19b1f6b3ff9,
    0x56a297b1bf4ffe55,
    0x94d558e493c54fc7,
    0x40bfc24c764552cb,
    0x931a706f8a8520cb,
    0x32229d322935bd52,
    0x2560d0f5dc4fefaf,
    0x9dbcc48355969bb6,
    0x0fd81c3985c0b56a,
    0xe03817e1560f2bda,
    0xc1bb4f81d892b2d5,
    0xb0c4864f4e28d2d7,
    0x3ecc49f9d9d6c263,
    0x51307e99b52ba65e,
    0x8af2b688da84a752,
    0xf5d72523b91b20b6,
    0x6d95ff1ff4634806,
    0x562f21555458339a,
    0xc0ce47f889336346,
    0x487823e5089b40d8,
    0xe4727c7ebc6d9592,
    0x5a8f7277e94970ba,
    0xfca2f406b1c8bb50,
    0x5b1f8a95f1791070,
    0xd304af9fc9028605,
    0x5440ab7fc930e748,
    0x312d25fbca2ab5a1,
    0x10f4a4b234a4d575,
    0x90301d55047e7473,
    0x3b6372886c61591e,
    0x293402b77c444e06,
    0x451f34a4d3e97dd7,
    0x3158d814d81bc57b,
    0x034942425b9bda69,
    0xe2032ff9e532d9bb,
    0x62ae066b8b2179e5,
    0x9545e10c2f8d71d8,
    0x7ff7483eb2d23fc0,
    0x00945fcebdc98d86,
    0x8764bbbe99b26ca2,
    0x1b1ec62284c0bfc3,
    0x58e0fcc4f0aa362b,
    0x5f4abefa878d458d,
    0xfd74ac2f9607c519,
    0xa4e3fb37df8cbfa9,
    0xbf697e43cac574e5,
    0x86f14a3f68f4cd53,
    0x24a23d076f1ce522,
    0xe725cd8048868cc8,
    0xbf3c729eb2464362,
    0xd8f6cd57b3cc1ed8,
    0x6329e52425541577,
    0x62aa688ad5ae1ac0,
    0x0a242566269bf845,
    0x168b1a4753aca74b,
    0xf789afefff2e7e3c,
    0x6c3362093b6fccdb,
    0x4ce8f50bd28c09b2,
    0x006a2db95ae8aa93,
    0x975b0d623c3d1a8c,
    0x18605d3935338c5b,
    0x5bb6f6136cad3c71,
    0x0f53a20701f8d8a6,
    0xab8c5ad2e7e93c67,
    0x40b5ac5127acaa29,
    0x8c7bf63c2075895f,
    0x78bd9f7e014a805c,
    0xb2c9e9f4f9c8c032,
    0xefd6049827eb91f3,
    0x2be459f482c16fbd,
    0xd92ce0c5745aaa8c,
    0x0aaa8fb298d965b9,
    0x2b37f92c6c803b15,
    0x8c54a5e94e0f0e78,
    0x95f9b6e90c0a3032,
    0xe7939faa436c7874,
    0xd16bfe8f6a8a40c9,
    0x44982b86263fd2fa,
    0xe285fb39f984e583,
    0x779a8df72d7619d3,
    0xf2d79a8de8d5dd1e,
    0xd1037354d66684e2,
    0x004c82a4e668a8e5,
    0x31d40a7668b044e6,
    0xd70578538bd02c11,
    0xdb45431078c5f482,
    0x977121bb7f6a51ad,
    0x73d5ccbd34eff8dd,
    0xe437a07d356e17cd,
    0x47b2782043c95627,
    0x9fb251413e41d49a,
    0xccd70b60652513d3,
    0x1c95b31e8a1b49b2,
    0xcae73dfd1bcb4c1b,
    0x34d98331b1f5b70f,
    0x784e39f22338d92f,
    0x18613d4a064df420,
    0xf1d8dae25f0bcebe,
    0x33f77c15ae855efc,
    0x3c88b3b912eb109c,
    0x956a2ec96bafeea5,
    0x1aa005b5e0ad0e87,
    0x5500d70527c4bb8e,
    0xe36c57196421cc44,
    0x13c4d286cc36ee39,
    0x5654a23d818b2a81,
    0x77b1dc13d161abdc,
    0x734f44de5f8d5eb5,
    0x60717e174a6c89a2,
    0xd47d9649266a211e,
    0x5b13a4322bb69e90,
    0xf7669609f8b5fc3c,
    0x21e6ac55bedcdac9,
    0x9b56b62b61166dea,
    0xf48f66b939797e9c,
    0x35f332f9c0e6ae9a,
    0xcc733f6a9a878db0,
    0x3da161e41cc108c2,
    0xb7d74ae535914d51,
    0x4d493b0b11d36469,
    0xce264d1dfba9741a,
    0xa9d1f2dc7436dc06,
    0x70738016604c2a27,
    0x231d36e96e93f3d5,
    0x7666881197838d19,
    0x4a2a83090aaad40c,
    0xf1e761591668b35d,
    0x7363236497f730a7,
    0x301080e37379dd4d,
    0x502dea2971827042,
    0xc2c5eb858f32625f,
    0x786afb9edfafbdff,
    0xdaee0d868490b2a4,
    0x617366b3268609f6,
    0xae0e35a0fe46173e,
    0xd1a07de93e824f11,
    0x079b8b115ea4cca8,
    0x93a99274558faebb,
    0xfb1e6e22e08a03b3,
    0xea635fdba3698dd0,
    0xcf53659328503a5c,
    0xcde3b31e6fd5d780,
    0x8e3e4221d3614413,
    0xef14d0d86bf1a22c,
    0xe1d830d3f16c5ddb,
    0xaabd2b2a451504e1,
];

pub(crate) fn expected_transcripts(
    case: &super::workload_source::workspace_common::Case,
    seed: u8,
    completed_steps: usize,
) -> Result<BTreeMap<String, Vec<Extent>>> {
    use super::workload_source::{self as w, workspace_common::EntryKind};
    let entries = if case.family == "dedup_branch_history" {
        w::dedup_branch_history::fixture(case, seed)?
    } else {
        w::workspace_registry::expected(case, seed, completed_steps)?
    };
    let mut result = BTreeMap::new();
    for entry in &entries {
        if let EntryKind::File(content) = &entry.kind {
            result.insert(entry.path.clone(), transcript(content.reader())?);
        }
    }
    if case.family == "dedup_branch_history" {
        if completed_steps > case.tier {
            return Err("history oracle steps".into());
        }
        if w::dedup_workloads::is_sdk(case) {
            for step in 0..completed_steps {
                for edit in w::dedup_workloads::sdk_edits(case, seed, step)? {
                    let previous = result
                        .get(&edit.path)
                        .ok_or("history oracle missing file")?;
                    let next =
                        splice_model(previous, edit.start, edit.delete_len, &edit.replacement[..])?;
                    result.insert(edit.path, next);
                }
            }
        } else if case.kind == "unrelated" && completed_steps > 0 {
            for entry in w::dedup_branch_history::expected(case, seed, completed_steps)? {
                if let EntryKind::File(content) = entry.kind {
                    result.insert(entry.path, transcript(content.reader())?);
                }
            }
        }
    }
    Ok(result)
}

fn union(files: &BTreeMap<String, Vec<Extent>>) -> Result<BTreeMap<ObjectId, u64>> {
    let mut result = BTreeMap::new();
    for extents in files.values() {
        for (id, len) in unique_payloads(extents)? {
            if result.insert(id, len).is_some_and(|old| old != len) {
                return Err("payload identity length mismatch across files".into());
            }
        }
    }
    Ok(result)
}

/// Authenticate the generated source and qualify independent sharing contracts
/// before any measured import. This runs once in disposable preparation.
pub(crate) fn qualify_import_input(
    case: &super::workload_source::workspace_common::Case,
    seed: u8,
    input: &std::path::Path,
) -> Result<super::workload_source::workspace_common::Receipt> {
    if !super::workload_source::workspace_registry::is_import(case) {
        return Err("input qualification is for import families".into());
    }
    let expected = expected_transcripts(case, seed, 0)?;
    for (path, want) in &expected {
        let got = transcript(std::fs::File::open(input.join(path))?)?;
        compare(want, &got)?;
    }
    let mut receipt = if case.kind == "boundaries" {
        verify_boundary_contract(&expected)?
    } else {
        verify_expected_contract(case, seed, 0, &expected, None)?
    };
    let mut hash = super::workload_source::Sha256::new();
    for (path, extents) in &expected {
        hash.update(path.as_bytes());
        hash.update(&[0]);
        for extent in extents {
            hash.update(extent.id.as_bytes());
            hash.update(&extent.source_offset.to_le_bytes());
            hash.update(&extent.len.to_le_bytes());
            hash.update(&extent.payload_len.to_le_bytes());
        }
    }
    receipt.insert(
        "input_transcript_sha256".into(),
        super::workload_source::hex(&hash.finish()),
    );
    receipt.insert(
        "scope".into(),
        "independent-source-transcripts-and-sharing-before-admission;not-product-verification"
            .into(),
    );
    Ok(receipt)
}

pub(crate) fn verify_transcripts(
    case: &super::workload_source::workspace_common::Case,
    seed: u8,
    completed_steps: usize,
    actual: &super::workspace_verify::SnapshotEvidence,
) -> Result<super::workload_source::workspace_common::Receipt> {
    verify_file_transcripts(
        case,
        seed,
        completed_steps,
        &actual.extents,
        &actual.file_roots,
    )
}

/// File-level transcript checks also serve fast verification. The caller records
/// which extents came from current byte reads versus qualified root references;
/// this function makes no exhaustive namespace/object-census claim.
pub(crate) fn verify_file_transcripts(
    case: &super::workload_source::workspace_common::Case,
    seed: u8,
    completed_steps: usize,
    actual_extents: &BTreeMap<String, Vec<super::workspace_verify::Extent>>,
    actual_file_roots: &BTreeMap<String, ObjectId>,
) -> Result<super::workload_source::workspace_common::Receipt> {
    let expected = expected_transcripts(case, seed, completed_steps)?;
    if actual_extents.len() != expected.len() {
        return Err("dedup regular-file transcript cardinality".into());
    }
    for (path, want) in &expected {
        let got = actual_extents
            .get(path)
            .ok_or("missing actual regular-file transcript")?;
        let got: Vec<_> = got
            .iter()
            .map(|e| Extent {
                id: e.id,
                source_offset: e.source_offset,
                len: e.len,
                payload_len: e.payload_len,
            })
            .collect();
        compare(want, &got)?;
    }
    verify_expected_contract(
        case,
        seed,
        completed_steps,
        &expected,
        Some(actual_file_roots),
    )
}

fn verify_expected_contract(
    case: &super::workload_source::workspace_common::Case,
    seed: u8,
    completed_steps: usize,
    expected: &BTreeMap<String, Vec<Extent>>,
    actual_file_roots: Option<&BTreeMap<String, ObjectId>>,
) -> Result<super::workload_source::workspace_common::Receipt> {
    use super::workload_source::{
        self as w,
        workspace_common::{EntryKind, Receipt},
    };
    let mut receipt = Receipt::new();
    let u = union(expected)?;
    let logical: u64 = expected.values().flatten().map(|e| e.len).sum();
    let unique: u64 = u.values().sum();
    receipt.insert("dedup_transcript_status".into(), "pass".into());
    receipt.insert("regular_file_logical_bytes".into(), logical.to_string());
    receipt.insert("distinct_payload_bytes".into(), unique.to_string());
    receipt.insert("distinct_payload_count".into(), u.len().to_string());
    receipt.insert("regular_file_count".into(), expected.len().to_string());
    if matches!(case.family, "dedup_cross_file" | "dedup_cdc_locality") {
        if expected
            .values()
            .flatten()
            .any(|e| e.source_offset != 0 || e.len != e.payload_len)
        {
            return Err("fresh import has partial payload extent".into());
        }
        receipt.insert("new_payload_bytes".into(), unique.to_string());
        receipt.insert("preexisting_payload_bytes".into(), "0".into());
    }
    if case.family == "dedup_cross_file" {
        let anchor = expected.get("files/f0000.dat").ok_or("cross-file anchor")?;
        if unique_payloads(anchor)?.len() != anchor.len() {
            return Err("unintended anchor internal duplicate chunk".into());
        }
        if case.kind == "identical" && expected.values().any(|e| e != anchor) {
            return Err("identical file transcripts differ".into());
        }
        if case.kind == "identical"
            && actual_file_roots.is_some_and(|roots| {
                roots
                    .values()
                    .any(|root| Some(root) != roots.get("files/f0000.dat"))
            })
        {
            return Err("identical imports have different content roots".into());
        }
        let mut bases = BTreeMap::new();
        for (path, extents) in expected {
            let i = path
                .strip_prefix("files/f")
                .and_then(|p| p.strip_suffix(".dat"))
                .ok_or("cross-file path")?
                .parse::<usize>()?;
            let ordinal = match case.kind {
                "anchor" | "identical" => 0,
                "unique" => i,
                "mixed" => 3 * (i / 4) + (i % 4).saturating_sub(1),
                _ => return Err("cross-file oracle profile".into()),
            };
            if let Some(old) = bases.insert(ordinal, extents) {
                if old != extents {
                    return Err("mixed exact pair differs".into());
                }
            }
        }
        let mut distinct = BTreeSet::new();
        for extents in bases.values() {
            for extent in *extents {
                if !distinct.insert(extent.id) {
                    return Err("unintended duplicate chunk in distinct cross-file bases".into());
                }
            }
        }
        receipt.insert(
            "distinct_complete_file_count".into(),
            bases.len().to_string(),
        );
    }
    if case.family == "dedup_cdc_locality" {
        let base = expected
            .get("reference.dat")
            .ok_or("CDC reference transcript")?;
        for i in 0..case.tier {
            let variant = expected
                .get(&format!("variants/v{i:04}.dat"))
                .ok_or("CDC variant transcript")?;
            let stats = sharing(base, variant)?;
            let prefix = format!("variant_{i:04}_");
            for (key, value) in [
                ("shared_chunk_count", stats.shared_ids.len() as u64),
                ("shared_logical_bytes", stats.shared_logical_bytes),
                ("prefix_chunk_count", stats.prefix_chunks as u64),
                ("prefix_bytes", stats.prefix_bytes),
                ("suffix_chunk_count", stats.suffix_chunks as u64),
                ("suffix_bytes", stats.suffix_bytes),
            ] {
                receipt.insert(format!("{prefix}{key}"), value.to_string());
            }
            receipt.insert(
                format!("{prefix}suffix_base_position"),
                stats
                    .suffix_base_position
                    .map_or("not-found".into(), |n| n.to_string()),
            );
            receipt.insert(
                format!("{prefix}suffix_variant_position"),
                stats
                    .suffix_variant_position
                    .map_or("not-found".into(), |n| n.to_string()),
            );
            if matches!(case.kind, "insert" | "delete") {
                let at = w::dedup_workloads::offset(case.family, case.kind, seed, i)?;
                let base_end = at + if case.kind == "delete" { 4096 } else { 0 };
                let variant_end = at + if case.kind == "insert" { 4096 } else { 0 };
                for (label, position, boundary) in [
                    (
                        "base_resynchronization_distance_bytes",
                        stats.suffix_base_position,
                        base_end,
                    ),
                    (
                        "variant_resynchronization_distance_bytes",
                        stats.suffix_variant_position,
                        variant_end,
                    ),
                ] {
                    let distance = position
                        .map(|position| {
                            position
                                .checked_sub(boundary)
                                .ok_or("suffix resumes before edit boundary")
                        })
                        .transpose()?;
                    receipt.insert(
                        format!("{prefix}{label}"),
                        distance.map_or("not-found".into(), |n| n.to_string()),
                    );
                }
            }
            if case.kind == "common-body" {
                let mut position = 0;
                let mut middle = BTreeSet::new();
                for extent in base {
                    if position >= 131072 && position + extent.len <= 917504 {
                        middle.insert(extent.id);
                    }
                    position += extent.len;
                }
                let mut position = 0;
                let mut coverage = 0;
                for extent in variant {
                    if middle.contains(&extent.id) {
                        coverage += (position + extent.len)
                            .min(917504)
                            .saturating_sub(position.max(131072));
                    }
                    position += extent.len;
                }
                receipt.insert(
                    format!("{prefix}shared_middle_logical_bytes"),
                    coverage.to_string(),
                );
            }
        }
    }
    if case.family == "dedup_workspace_reuse" {
        let base_files = w::dedup_workspace_reuse::base_file_count(case)?;
        let base: BTreeMap<_, _> = expected
            .iter()
            .filter(|(path, _)| path.starts_with("base/"))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        let added: BTreeMap<_, _> = expected
            .iter()
            .filter(|(path, _)| path.starts_with("added/"))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        if base.len() != base_files {
            return Err("workspace reuse base file count".into());
        }
        let p = union(&base)?;
        let a = union(&added)?;
        let base_occurrences = base.values().map(Vec::len).sum::<usize>();
        if p.len() != base_occurrences {
            return Err("unintended duplicate chunk in workspace unique base".into());
        }
        if case.kind == "unique" && a.len() != added.values().map(Vec::len).sum::<usize>() {
            return Err("unintended duplicate chunk in unique workspace additions".into());
        }
        if case.kind == "local" {
            let mut distinct = BTreeSet::new();
            for extents in added.values() {
                let identity: Vec<_> = extents
                    .iter()
                    .map(|e| (e.id, e.source_offset, e.len, e.payload_len))
                    .collect();
                if !distinct.insert(identity) {
                    return Err("duplicate local addition content".into());
                }
            }
        }
        let new_bytes: u64 = a
            .iter()
            .filter(|(id, _)| !p.contains_key(id))
            .map(|(_, len)| len)
            .sum();
        if case.kind == "exact" && new_bytes != 0 {
            return Err("exact additions inserted new regular payload".into());
        }
        if case.kind == "unique" && a.keys().any(|id| p.contains_key(id)) {
            return Err("unique additions intersect base payload".into());
        }
        for i in 0..if completed_steps == 0 { 0 } else { case.tier } {
            if case.kind == "exact"
                && added.get(&format!("added/a{i:04}.dat"))
                    != base.get(&format!("base/b{:04}.dat", i % base_files))
            {
                return Err("exact addition differs from selected base".into());
            }
            if case.kind == "exact"
                && actual_file_roots.is_some_and(|roots| {
                    roots.get(&format!("added/a{i:04}.dat"))
                        != roots.get(&format!("base/b{:04}.dat", i % base_files))
                })
            {
                return Err("exact addition did not reuse base content root".into());
            }
        }
        receipt.insert("base_file_count".into(), base_files.to_string());
        receipt.insert(
            "addition_logical_bytes".into(),
            added
                .values()
                .flatten()
                .map(|e| e.len)
                .sum::<u64>()
                .to_string(),
        );
        receipt.insert("addition_new_payload_bytes".into(), new_bytes.to_string());
        receipt.insert(
            "base_distinct_payload_bytes".into(),
            p.values().sum::<u64>().to_string(),
        );
    }
    // Digests below qualify deterministic distinct inputs without actual-output
    // bytes influencing any expected descriptor or schedule.
    if case.family == "dedup_cdc_locality" {
        let mut digests = BTreeSet::new();
        for entry in w::dedup_cdc_locality::fixture(case, seed)? {
            if let EntryKind::File(content) = entry.kind {
                if !digests.insert(content.digest()?) {
                    return Err("duplicate CDC reference/variant fixture".into());
                }
            }
        }
    }
    Ok(receipt)
}

#[derive(Default)]
pub(crate) struct HistoryAccounting {
    payloads: BTreeMap<ObjectId, u64>,
    canonical_objects: BTreeMap<ObjectId, super::workspace_verify::CanonicalObject>,
    recurring_roots: BTreeMap<usize, ObjectId>,
    snapshots: usize,
}
impl HistoryAccounting {
    // Feed genesis (step zero), then every already-verified retained Commit.
    pub(crate) fn observe(
        &mut self,
        case: &super::workload_source::workspace_common::Case,
        step: usize,
        actual: &super::workspace_verify::SnapshotEvidence,
        canonical_rows: &mut dyn Write,
    ) -> Result<super::workload_source::workspace_common::Receipt> {
        use super::workload_source::{dedup_workloads, workspace_common::Receipt};
        if case.family != "dedup_branch_history" || step != self.snapshots || step > case.tier {
            return Err("retained history accounting order".into());
        }
        let mut current = BTreeMap::new();
        let mut logical = 0u64;
        for extents in actual.extents.values() {
            for e in extents {
                logical = logical
                    .checked_add(e.len)
                    .ok_or("history logical overflow")?;
                if current
                    .insert(e.id, e.payload_len)
                    .is_some_and(|n| n != e.payload_len)
                {
                    return Err("history payload identity length".into());
                }
            }
        }
        if logical != 1_048_576 {
            return Err("history state is not one MiB".into());
        }
        if actual
            .canonical_objects
            .iter()
            .filter(|(_, object)| object.regular_payload())
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>()
            != current.keys().copied().collect()
        {
            return Err("typed regular payload union differs from verified extents".into());
        }
        let new_bytes: u64 = current
            .iter()
            .filter(|(id, _)| !self.payloads.contains_key(id))
            .map(|(_, n)| n)
            .sum();
        if step > 0 && case.kind == "metadata" && new_bytes != 0 {
            return Err("metadata history introduced payload".into());
        }
        if step > 0
            && case.kind == "unrelated"
            && current.keys().any(|id| self.payloads.contains_key(id))
        {
            return Err("unrelated history payload intersects earlier states".into());
        }
        if case.kind == "recurring" {
            let root = *actual
                .file_roots
                .get(&dedup_workloads::shard_path(192))
                .ok_or("recurring root missing")?;
            if let Some(prior) = self.recurring_roots.insert(step % 2, root) {
                if prior != root {
                    return Err("recurring full replacement changed canonical content root".into());
                }
            }
            if step >= 2 && new_bytes != 0 {
                return Err("recurring A/B state inserted new payload".into());
            }
        }
        for (id, len) in current {
            if self.payloads.insert(id, len).is_some_and(|n| n != len) {
                return Err("retained payload identity length changed".into());
            }
        }
        let canonical_root = actual
            .receipt
            .get("canonical_root")
            .ok_or("history canonical root")?;
        let mut new_canonical_objects = 0u64;
        let mut new_canonical_bytes = 0u64;
        for (id, object) in &actual.canonical_objects {
            let changed;
            let retained = match self.canonical_objects.entry(*id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    new_canonical_objects += 1;
                    new_canonical_bytes = new_canonical_bytes
                        .checked_add(object.canonical_bytes)
                        .ok_or("new canonical bytes overflow")?;
                    changed = true;
                    entry.insert(*object)
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    let retained = entry.into_mut();
                    if retained.role != object.role
                        || retained.canonical_bytes != object.canonical_bytes
                    {
                        return Err("retained canonical identity role/length changed".into());
                    }
                    let merged_regular = retained.regular_file || object.regular_file;
                    let merged_metadata = retained.metadata_value || object.metadata_value;
                    changed = merged_regular != retained.regular_file
                        || merged_metadata != retained.metadata_value;
                    retained.regular_file = merged_regular;
                    retained.metadata_value = merged_metadata;
                    retained
                }
            };
            if changed {
                writeln!(
                    canonical_rows,
                    "{step}\t{canonical_root}\t{id}\t{:?}\t{}\t{}\t{}",
                    retained.role,
                    retained.canonical_bytes,
                    u8::from(retained.regular_file),
                    u8::from(retained.metadata_value)
                )?;
            }
        }
        canonical_rows.flush()?;
        self.snapshots += 1;
        let mut receipt = Receipt::new();
        receipt.insert("canonical_root".into(), canonical_root.clone());
        receipt.insert("canonical_union_status".into(), "pass".into());
        receipt.insert(
            "canonical_union_encoding".into(),
            "first-seen-or-usage-expansion-tsv-gzip-v1".into(),
        );
        receipt.insert("canonical_usage_scope".into(), "regular_payload=Chunk+regular_file; non_payload=complement; metadata_value may overlap either".into());
        let mut role_totals = BTreeMap::new();
        let mut totals = [0u64; 8];
        for object in self.canonical_objects.values() {
            let role = role_totals.entry(object.role).or_insert((0u64, 0u64));
            role.0 += 1;
            role.1 = role
                .1
                .checked_add(object.canonical_bytes)
                .ok_or("retained role bytes overflow")?;
            for (index, included) in [
                true,
                object.regular_payload(),
                !object.regular_payload(),
                object.metadata_value,
            ]
            .into_iter()
            .enumerate()
            {
                if included {
                    totals[index * 2] += 1;
                    totals[index * 2 + 1] = totals[index * 2 + 1]
                        .checked_add(object.canonical_bytes)
                        .ok_or("retained canonical bytes overflow")?;
                }
            }
        }
        for (index, prefix) in [
            "retained_canonical",
            "retained_regular_payload_canonical",
            "retained_non_payload_canonical",
            "retained_metadata_value_canonical",
        ]
        .into_iter()
        .enumerate()
        {
            receipt.insert(format!("{prefix}_objects"), totals[index * 2].to_string());
            receipt.insert(format!("{prefix}_bytes"), totals[index * 2 + 1].to_string());
        }
        for (role, (objects, bytes)) in role_totals {
            receipt.insert(
                format!("retained_canonical_{role:?}_objects"),
                objects.to_string(),
            );
            receipt.insert(
                format!("retained_canonical_{role:?}_bytes"),
                bytes.to_string(),
            );
        }
        let current_canonical_bytes =
            actual
                .canonical_objects
                .values()
                .try_fold(0u64, |total, object| {
                    total
                        .checked_add(object.canonical_bytes)
                        .ok_or("current canonical bytes overflow")
                })?;
        for (key, value) in [
            ("step_new_canonical_objects", new_canonical_objects),
            ("step_new_canonical_bytes", new_canonical_bytes),
            (
                "current_canonical_objects",
                actual.canonical_objects.len() as u64,
            ),
            ("current_canonical_bytes", current_canonical_bytes),
            ("retained_snapshot_count", self.snapshots as u64),
            ("created_commit_count", step as u64),
            (
                "retained_logical_snapshot_bytes",
                self.snapshots as u64 * 1_048_576,
            ),
            (
                "distinct_retained_payload_bytes",
                self.payloads.values().sum(),
            ),
            (
                "distinct_retained_payload_count",
                self.payloads.len() as u64,
            ),
            ("step_new_payload_bytes", new_bytes),
        ] {
            receipt.insert(key.into(), value.to_string());
        }
        Ok(receipt)
    }
}

pub(crate) fn verify_boundaries(
    actual: &super::workspace_verify::SnapshotEvidence,
) -> Result<super::workload_source::workspace_common::Receipt> {
    use super::workload_source::{dedup_cdc_locality, workspace_common::EntryKind};
    let mut expected = BTreeMap::new();
    for entry in dedup_cdc_locality::boundaries()? {
        if let EntryKind::File(content) = entry.kind {
            expected.insert(entry.path, transcript(content.reader())?);
        }
    }
    if expected.len() != 60 || actual.extents.len() != 60 {
        return Err("CDC boundary transcript cardinality".into());
    }
    for (path, want) in &expected {
        let got = actual
            .extents
            .get(path)
            .ok_or("CDC boundary actual path missing")?;
        let got: Vec<_> = got
            .iter()
            .map(|e| Extent {
                id: e.id,
                source_offset: e.source_offset,
                len: e.len,
                payload_len: e.payload_len,
            })
            .collect();
        compare(want, &got)?;
    }
    verify_boundary_contract(&expected)
}

fn verify_boundary_contract(
    expected: &BTreeMap<String, Vec<Extent>>,
) -> Result<super::workload_source::workspace_common::Receipt> {
    use super::workload_source::workspace_common::Receipt;
    if expected.len() != 60 {
        return Err("CDC boundary input cardinality".into());
    }
    let mut receipt = Receipt::new();
    for seed in 1..=3 {
        for len in [0, 1, 8191, 8192, 16384, 32768, 32769] {
            let root = format!("boundary/s{seed}/n{len}");
            let a = &expected[&format!("{root}/a.dat")];
            let b = &expected[&format!("{root}/b.dat")];
            compare(a, b)?;
            if a.iter().any(|e| e.len == 0 || e.len > 32768)
                || a.iter().map(|e| e.len).sum::<u64>() != len
                || len == 0 && !a.is_empty()
                || len == 32769 && a.len() < 2
            {
                return Err("CDC boundary shape".into());
            }
            if len > 0 && len <= 8192 {
                let changed = &expected[&format!("{root}/changed.dat")];
                if a.len() != 1 || changed.iter().any(|e| e.id == a[0].id) {
                    return Err("CDC short mutation retained sole chunk".into());
                }
            }
        }
        receipt.insert(format!("seed_{seed}_boundary_status"), "pass".into());
    }
    receipt.insert("boundary_file_count".into(), "60".into());
    receipt.insert("boundary_seed_count".into(), "3".into());
    receipt.insert("dedup_transcript_status".into(), "pass".into());
    Ok(receipt)
}

/// Independent capped SDK splice oracle; never regenerate the final transcript
/// with full-file CDC because unchanged base slices must retain their payload IDs.
pub(crate) fn verify_capped(
    case: &super::workload_source::workspace_common::Case,
    actual: &super::workspace_verify::SnapshotEvidence,
) -> Result<super::workload_source::workspace_common::Receipt> {
    use super::workload_source::{
        edit_length_changing_capped as capped, sdk_edit_common,
        workspace_common::{EntryKind, Receipt},
    };
    let scenario = capped::sdk_scenario(case)?;
    let fixture = capped::fixture(case, 1)?;
    let initial = fixture
        .iter()
        .find(|entry| entry.path == "payload.bin")
        .ok_or("capped oracle fixture path")?;
    let EntryKind::File(content) = &initial.kind else {
        return Err("capped oracle needs regular input".into());
    };
    if actual
        .receipt
        .get("verification_status")
        .map(String::as_str)
        != Some("pass")
        || actual.extents.len() != 1
        || actual.file_roots.len() != 1
    {
        return Err(
            "capped transcript requires complete canonical verification of one file".into(),
        );
    }
    let before = transcript(content.reader())?;
    let replacement = match scenario.replacement_kind {
        sdk_edit_common::ReplacementKind::Inline => sdk_edit_common::replacement_bytes(&scenario),
        sdk_edit_common::ReplacementKind::Zero => vec![0; scenario.replacement_len as usize],
    };
    if sdk_edit_common::sha256_hex(&replacement) != scenario.payload_sha256 {
        return Err("capped oracle original replacement digest".into());
    }
    let expected = splice_model(
        &before,
        scenario.start,
        scenario.delete_len,
        replacement.as_slice(),
    )?;
    let observed = actual
        .extents
        .get("payload.bin")
        .ok_or("capped actual file transcript")?;
    let observed = observed
        .iter()
        .map(|extent| Extent {
            id: extent.id,
            source_offset: extent.source_offset,
            len: extent.len,
            payload_len: extent.payload_len,
        })
        .collect::<Vec<_>>();
    compare(&expected, &observed)?;
    let total = expected.iter().try_fold(0_u64, |sum, extent| {
        sum.checked_add(extent.len)
            .ok_or("capped oracle length overflow")
    })?;
    if total != scenario.final_bytes || total != 524_288_000 {
        return Err("capped oracle final byte cap".into());
    }
    let before_ids = unique_payloads(&before)?;
    let after_ids = unique_payloads(&expected)?;
    let retained = after_ids
        .keys()
        .filter(|id| before_ids.contains_key(*id))
        .count();
    let canonical_root = actual
        .file_roots
        .get("payload.bin")
        .ok_or("capped canonical file root")?;
    Ok(Receipt::from([
        ("capped_sdk_transcript_status".into(), "pass".into()),
        ("scenario_id".into(), scenario.id),
        ("input_sha256".into(), capped::fixture_sha256(case)?.into()),
        ("plan_sha256".into(), scenario.plan_sha256),
        (
            "replacement_kind".into(),
            scenario.replacement_kind.name().into(),
        ),
        ("replacement_sha256".into(), scenario.payload_sha256.into()),
        (
            "initial_file_bytes".into(),
            scenario.fixture_bytes.to_string(),
        ),
        ("final_file_bytes".into(), total.to_string()),
        ("initial_extent_count".into(), before.len().to_string()),
        (
            "expected_final_extent_count".into(),
            normalize(&expected)?.len().to_string(),
        ),
        (
            "actual_final_extent_count".into(),
            normalize(&observed)?.len().to_string(),
        ),
        ("retained_payload_object_count".into(), retained.to_string()),
        (
            "new_payload_object_count".into(),
            (after_ids.len() - retained).to_string(),
        ),
        (
            "actual_canonical_file_root".into(),
            canonical_root.to_string(),
        ),
        (
            "oracle_scope".into(),
            "independent-initial-cdc-plus-flat-sdk-splice".into(),
        ),
    ]))
}
