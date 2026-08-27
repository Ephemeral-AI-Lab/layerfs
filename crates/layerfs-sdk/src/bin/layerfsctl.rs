use layerfs_sdk::{BranchId, IntegrityMode, LayerFs, LayerId, LayerStackId};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, root, integrity] if command == "compact" => {
            let fs = LayerFs::open(Path::new(root), integrity_mode(integrity)?)?;
            let storage_id = fs.storage_id();
            let started = std::time::Instant::now();
            let fs = fs.compact()?;
            let compact_wall_ns = u64::try_from(started.elapsed().as_nanos())?;
            if fs.storage_id() != storage_id {
                return Err("compaction changed StorageId".into());
            }
            let observation = fs
                .last_compaction_observation()
                .ok_or("missing compaction observation")?;
            println!(
                "{{\"status\":\"PASS\",\"storage_id\":\"{}\",\"source_indexed_objects\":{},\"source_indexed_canonical_bytes\":{},\"retained_objects\":{},\"retained_canonical_bytes\":{},\"reclaimed_objects\":{},\"reclaimed_canonical_bytes\":{},\"candidate_indexed_objects\":{},\"candidate_indexed_canonical_bytes\":{},\"old_generation_apparent_bytes\":{},\"new_generation_apparent_bytes\":{},\"mark_database_apparent_bytes\":{},\"candidate_journal_temp_peak_apparent_bytes\":{},\"verification_scratch_peak_apparent_bytes\":{},\"selector_temporary_apparent_bytes\":{},\"total_peak_apparent_bytes\":{},\"compact_wall_ns\":{}}}",
                hex(&storage_id),
                observation.source_indexed_objects,
                observation.source_indexed_canonical_bytes,
                observation.retained_objects,
                observation.retained_canonical_bytes,
                observation.reclaimed_objects,
                observation.reclaimed_canonical_bytes,
                observation.candidate_indexed_objects,
                observation.candidate_indexed_canonical_bytes,
                observation.old_generation_bytes,
                observation.new_generation_bytes,
                observation.mark_database_bytes,
                observation.candidate_journal_temp_peak_bytes,
                observation.verification_scratch_peak_bytes,
                observation.selector_temporary_bytes,
                observation.total_peak_bytes,
                compact_wall_ns,
            );
            Ok(())
        }
        [command, root, integrity, branch, stack, layer] if command == "init" => {
            let branch_id = BranchId::from_bytes(id(branch)?);
            let stack_id = LayerStackId::from_bytes(id(stack)?);
            let layer_id = LayerId::from_bytes(id(layer)?);
            let fs = LayerFs::open(Path::new(root), integrity_mode(integrity)?)?;
            let root = fs.initialize_empty_root()?;
            let stack = fs.create_layer_stack(stack_id, layer_id, "main", root)?;
            let branch = fs.create_top_level_branch(branch_id, Some("main"), stack)?;
            println!(
                "{{\"storage_id\":\"{}\",\"branch_id\":\"{}\",\"generation\":{},\"root\":\"{}\"}}",
                hex(&fs.storage_id()),
                hex(branch.branch_id.as_bytes()),
                branch.generation,
                branch.root,
            );
            Ok(())
        }
        _ => Err(usage().into()),
    }
}

fn integrity_mode(value: &str) -> Result<IntegrityMode, Box<dyn std::error::Error>> {
    match value {
        "trusted" => Ok(IntegrityMode::TrustedLocalDev),
        "verified" => Ok(IntegrityMode::Verified),
        _ => Err(usage().into()),
    }
}

fn id(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 {
        return Err("identity must contain 64 hexadecimal characters".into());
    }
    let mut decoded = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Ok(decoded)
}

fn digit(value: u8) -> Result<u8, Box<dyn std::error::Error>> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("identity must be lowercase hexadecimal".into()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn usage() -> &'static str {
    "usage:\n  layerfsctl init WORKING_ROOT trusted|verified BRANCH_ID LAYER_STACK_ID LAYER_ID\n  layerfsctl compact WORKING_ROOT trusted|verified"
}
