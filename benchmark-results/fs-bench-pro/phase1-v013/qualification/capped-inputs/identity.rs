#[allow(dead_code)]
#[path = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark/fs-bench-pro/workload.rs"]
mod workload;
fn main() {
    let lengths = [524_283_904_u64, 524_285_952_u64, 524_288_000_u64];
    let mut hashes = [Some(workload::Sha256::new()), Some(workload::Sha256::new()), Some(workload::Sha256::new())];
    let mut state = 0x4c41_5945_5246_5331_u64;
    let mut offset = 0_u64;
    let mut buffer = vec![0_u8; 1_048_576];
    while offset < lengths[2] {
        let next = *lengths.iter().find(|length| **length > offset).unwrap();
        let count = buffer.len().min((next - offset) as usize);
        workload::sdk_edit_common::fixture_block(&mut state, &mut buffer[..count]);
        for hash in hashes.iter_mut().flatten() { hash.update(&buffer[..count]); }
        offset += count as u64;
        for (index, length) in lengths.iter().enumerate() {
            if offset == *length {
                println!("{} {}",length,workload::hex(&hashes[index].take().unwrap().finish()));
            }
        }
    }
}
