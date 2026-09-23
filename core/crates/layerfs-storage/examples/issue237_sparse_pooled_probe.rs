//! #237 sparse-pack diagnostic: 17 separate public Saves, then full reopened readback.
//!
//! Usage: issue237_sparse_pooled_probe --output FRESH_DIR
//! The fixed input and default policy are shared by the C3 and C5 release arms.

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
    LEAF_ROW_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

const SAVES: u64 = 17;
const VALUES_PER_LEAF: u64 = 4;

fn output_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--output")) {
        return Err("usage: issue237_sparse_pooled_probe --output FRESH_DIR".into());
    }
    let output = PathBuf::from(args.next().ok_or("--output needs a directory")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    std::fs::create_dir(&output)?; // Refuse an existing result path, including failed attempts.
    Ok(output)
}

fn value(seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

fn leaf(round: u64) -> Result<FinalizedObject, Box<dyn Error>> {
    let rows = (0..VALUES_PER_LEAF)
        .map(|index| InodeLeafRow {
            serial: index + 1,
            value: value(round * 100 + index),
        })
        .collect();
    let canonical = InodeLeaf {
        subtree_bytes: VALUES_PER_LEAF * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()?;
    Ok(FinalizedObject::new(ObjectRole::InodeLeaf, canonical)?)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = output_path()?;
    let path = output.join("store.sqlite");
    let store = Timing::disabled("sparse.create", |scope| {
        Store::create(&path, StoragePolicy::frozen_default(), scope.child("store"))
    })
    .0?;
    let mut expected = Vec::with_capacity(SAVES as usize);
    let mut records = Vec::with_capacity(SAVES as usize);
    for round in 0..SAVES {
        let object = leaf(round)?;
        let id = object.id();
        let canonical = object.canonical().to_vec();
        if ObjectId::for_bytes(&canonical) != id {
            return Err("fixture canonical identity".into());
        }
        let started = Instant::now();
        let outcome = Timing::disabled("sparse.save", |scope| {
            let mut save = store.begin_save(scope.child("begin"))?;
            save.accept(object)?;
            save.finish(scope.child("finish"))
        })
        .0?;
        let operation_ns = started.elapsed().as_nanos();
        if outcome.inserted != 1 || outcome.pool.leaves != 1 || outcome.pool.new_values != 4 {
            return Err(format!("round {round}: unexpected Save outcome {outcome:?}").into());
        }
        records.push(format!(
            "{{\"round\":{round},\"operation_ns\":{operation_ns},\"object_id\":\"{id}\",\"canonical_hex\":\"{}\",\"inserted\":{},\"pooled_groups\":{}}}",
            hex(&canonical), outcome.inserted, outcome.pool.groups
        ));
        expected.push((id, canonical));
    }
    drop(store);

    let started = Instant::now();
    let reopened = Timing::disabled("sparse.reopen", |scope| {
        Store::open(&path, scope.child("store"))
    })
    .0?;
    let ids: Vec<_> = expected.iter().map(|(id, _)| *id).collect();
    let (actual, _) = Timing::disabled("sparse.readback", |scope| {
        reopened.read_batch(&ids, scope.child("read"))
    })
    .0?;
    if actual.len() != expected.len() {
        return Err("reopened readback count".into());
    }
    for ((id, bytes), found) in expected.iter().zip(actual) {
        if found != *bytes || ObjectId::for_bytes(&found) != *id {
            return Err(format!("reopened readback mismatch: {id}").into());
        }
    }
    println!(
        "{{\"schema\":\"issue237-sparse-pooled-probe-v1\",\"policy\":\"frozen-default\",\"store_file\":\"store.sqlite\",\"save_count\":{SAVES},\"values_per_leaf\":{VALUES_PER_LEAF},\"saves\":[{}],\"readback_status\":\"PASS\",\"readback_objects\":{},\"readback_ns\":{}}}",
        records.join(","), expected.len(), started.elapsed().as_nanos()
    );
    Ok(())
}
