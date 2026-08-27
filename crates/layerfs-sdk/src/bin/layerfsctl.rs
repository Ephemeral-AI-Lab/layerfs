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
    if arguments.len() != 6 || arguments[0] != "init" {
        return Err(usage().into());
    }
    let integrity = match arguments[2].as_str() {
        "trusted" => IntegrityMode::TrustedLocalDev,
        "verified" => IntegrityMode::Verified,
        _ => return Err(usage().into()),
    };
    let branch_id = BranchId::from_bytes(id(&arguments[3])?);
    let stack_id = LayerStackId::from_bytes(id(&arguments[4])?);
    let layer_id = LayerId::from_bytes(id(&arguments[5])?);
    let fs = LayerFs::open(Path::new(&arguments[1]), integrity)?;
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
    "usage: layerfsctl init WORKING_ROOT trusted|verified BRANCH_ID LAYER_STACK_ID LAYER_ID"
}
