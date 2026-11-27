//! F-043: encode and decode regions are reused by later Saves and reads
//! without zeroing. Neither the frames (so the stored bytes) nor the decoded
//! bytes may depend on what a region held before: the same input gives the
//! same output from a fresh workspace and from a reused region, even one
//! filled with a poison pattern.
use layerfs_storage::encoding::{
    CodecProfile, CompressionWorkspace, DecompressionWorkspace, DECODE_WORKSPACE_BYTES,
};
use layerfs_storage::{StorageCapacities, StoragePolicy};

fn inputs() -> Vec<Vec<u8>> {
    let mut state = 0x0f04_3f04_3f04_3f04_u64;
    let mut random = |n: usize| -> Vec<u8> {
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    };
    let text: Vec<u8> = b"export function f(x) { return x + 1; }\n"
        .iter()
        .copied()
        .cycle()
        .take(60_000)
        .collect();
    vec![
        random(5_000),
        text.clone(),
        random(120_000),
        text[..777].to_vec(),
    ]
}

/// Every frame kind a Save makes: whole, against a prefix, and group bodies.
fn frames(workspace: &mut CompressionWorkspace, profile: CodecProfile) -> Vec<Vec<u8>> {
    let inputs = inputs();
    let mut out = Vec::new();
    for raw in &inputs {
        out.push(workspace.compress(profile, raw).unwrap());
    }
    out.push(
        workspace
            .compress_prefix(profile, &inputs[1][..50_000], &inputs[1])
            .unwrap(),
    );
    out.push(
        workspace
            .compress_prefix(profile, &inputs[3], &inputs[0])
            .unwrap(),
    );
    out.push(workspace.compress_group(&inputs[0]).unwrap());
    out.push(workspace.compress_group(&inputs[1][..20_000]).unwrap());
    out
}

#[test]
fn reused_and_poisoned_encode_regions_give_byte_identical_frames() {
    let capacities = StorageCapacities::from_policy(StoragePolicy::new(1, 131_072, 8, 4)).unwrap();
    let profile = CodecProfile::whole_file(&capacities);
    let fresh = frames(&mut CompressionWorkspace::new().unwrap(), profile);
    // A region that has encoded other data, several times over, as Saves do.
    let mut region = CompressionWorkspace::new().unwrap().into_region();
    for round in 0..4 {
        let mut workspace = CompressionWorkspace::in_region(region).unwrap();
        assert_eq!(
            frames(&mut workspace, profile),
            fresh,
            "reused round {round}"
        );
        region = workspace.into_region();
    }
    // A region holding none of zstd's own state at all.
    for pattern in [u64::MAX, 0xa5a5_a5a5_a5a5_a5a5, 0x0123_4567_89ab_cdef] {
        region.fill(pattern);
        let mut workspace = CompressionWorkspace::in_region(region).unwrap();
        assert_eq!(
            frames(&mut workspace, profile),
            fresh,
            "poison {pattern:#x}"
        );
        region = workspace.into_region();
    }
}

#[test]
fn a_region_of_another_size_is_refused() {
    assert!(CompressionWorkspace::in_region(vec![0; 1024]).is_err());
}

/// Every decode a read or Save makes, of the frames `frames` encodes.
fn decoded(
    workspace: &mut DecompressionWorkspace,
    profile: CodecProfile,
    frames: &[Vec<u8>],
) -> Vec<Vec<u8>> {
    let inputs = inputs();
    let mut out = Vec::new();
    for (raw, frame) in inputs.iter().zip(frames) {
        out.push(workspace.decompress(profile, frame, raw.len()).unwrap());
    }
    out.push(
        workspace
            .decompress_prefix(profile, &frames[4], 50_000, &inputs[1])
            .unwrap(),
    );
    out.push(
        workspace
            .decompress_prefix(profile, &frames[5], inputs[3].len(), &inputs[0])
            .unwrap(),
    );
    out.push(
        workspace
            .decompress_group(&frames[6], inputs[0].len())
            .unwrap(),
    );
    out.push(workspace.decompress_group(&frames[7], 20_000).unwrap());
    out
}

#[test]
fn reused_and_poisoned_decode_regions_give_byte_identical_output() {
    let capacities = StorageCapacities::from_policy(StoragePolicy::new(1, 131_072, 8, 4)).unwrap();
    let profile = CodecProfile::whole_file(&capacities);
    let frames = frames(&mut CompressionWorkspace::new().unwrap(), profile);
    let fresh = decoded(
        &mut DecompressionWorkspace::new().unwrap(),
        profile,
        &frames,
    );
    let inputs = inputs();
    assert_eq!(&fresh[..4], &inputs[..], "a fresh decode is exact");
    let mut workspace = DecompressionWorkspace::new().unwrap();
    assert!(workspace
        .decompress(profile, &frames[0], inputs[0].len())
        .is_ok());
    let mut region = workspace.into_region().expect("a materialised region");
    for round in 0..4 {
        let mut workspace = DecompressionWorkspace::in_region(region).unwrap();
        assert_eq!(workspace.workspace_bytes(), DECODE_WORKSPACE_BYTES);
        assert_eq!(
            decoded(&mut workspace, profile, &frames),
            fresh,
            "reused round {round}"
        );
        region = workspace.into_region().unwrap();
    }
    for pattern in [u64::MAX, 0xa5a5_a5a5_a5a5_a5a5, 0x0123_4567_89ab_cdef] {
        region.fill(pattern);
        let mut workspace = DecompressionWorkspace::in_region(region).unwrap();
        assert_eq!(
            decoded(&mut workspace, profile, &frames),
            fresh,
            "poison {pattern:#x}"
        );
        region = workspace.into_region().unwrap();
    }
    assert!(
        DecompressionWorkspace::new()
            .unwrap()
            .into_region()
            .is_none(),
        "a workspace that decoded nothing has no region to give back"
    );
    assert!(DecompressionWorkspace::in_region(vec![0; 1024]).is_err());
    // The cost of zeroing a reused decode region on hand-out (diagnostic).
    let start = std::time::Instant::now();
    for _ in 0..64 {
        region = DecompressionWorkspace::in_region(region)
            .unwrap()
            .into_region()
            .unwrap();
    }
    println!(
        "DECODE_REGION_HANDOUT mean_us={:.1}",
        start.elapsed().as_secs_f64() * 1e6 / 64.0
    );
}
