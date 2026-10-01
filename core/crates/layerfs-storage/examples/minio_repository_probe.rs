//! Standalone full-corpus import through public C1/C2 APIs; no product changes.
use std::error::Error;
use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Instant;

use layerfs_content::{construct_bytes, construct_stream, ConstructionPolicy};
use layerfs_telemetry::timer::Timing;
use rusqlite::Connection;

#[path = "repository_probe/attribution.rs"]
mod attribution;
#[path = "repository_probe/metrics.rs"]
mod metrics;
#[path = "repository_probe/optimize.rs"]
mod optimize;
#[path = "repository_probe/packs.rs"]
mod packs;
#[path = "repository_probe/read.rs"]
mod read;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn prepare(master: &Path, output: &Path) -> Result<()> {
    let started = Instant::now();
    fs::create_dir(output)?;
    fs::create_dir(output.join("packs"))?;
    fs::copy(
        master.join("manifest.sqlite"),
        output.join("catalog.sqlite"),
    )?;
    let source = Connection::open(master.join("manifest.sqlite"))?;
    source.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0;")?;
    let mut packer = packs::Packer::new(output)?;
    let policy = ConstructionPolicy::default();
    let capacities = policy.capacities();
    let mut query =
        source.prepare("SELECT path,size FROM entries WHERE kind='file' ORDER BY path")?;
    let mut rows = query.query([])?;
    let (mut files, mut logical) = (0_u64, 0_u64);
    while let Some(row) = rows.next()? {
        let path: Vec<u8> = row.get(0)?;
        let size = u64::try_from(row.get::<_, i64>(1)?)?;
        let mut input = File::open(master.join("tree").join(std::ffi::OsStr::from_bytes(&path)))?;
        let built = if size < 131072 {
            let bytes = optimize::sized_bytes(&mut input, usize::try_from(size)?)?;
            let (result, _) = Timing::disabled("repository.construct", |scope| {
                construct_bytes(
                    policy,
                    &capacities,
                    &bytes,
                    &mut packer,
                    scope.child("file"),
                )
            });
            result?
        } else {
            let (result, _) = Timing::disabled("repository.construct", |scope| {
                construct_stream(policy, &capacities, input, &mut packer, scope.child("file"))
            });
            result?
        };
        if built.logical_len != size {
            return Err("fixture length changed".into());
        }
        files += 1;
        logical += size;
        packer.update_file(&path, built.root, files)?;
        if files % 4096 == 0 {
            eprintln!("constructed files={files} logical_bytes={logical}");
        }
    }
    packer.finish()?;
    let missing: i64 = packer.db.query_row(
        "SELECT count(*) FROM entries WHERE kind='file' AND content_root IS NULL",
        [],
        |r| r.get(0),
    )?;
    if missing != 0 {
        return Err("unconstructed files".into());
    }
    let engine: String = packer
        .db
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let summary = format!("{{\"files\":{files},\"logical_bytes\":{logical},\"unique_objects\":{},\"duplicate_emissions\":{},\"packs\":{},\"pack_bytes\":{},\"wall_ns\":{},\"sqlite_version\":\"{engine}\",\"producer_count\":1,\"cache_verdict\":\"INELIGIBLE\",\"performance_claim\":false}}\n", packer.objects, packer.duplicates, packer.packs, packer.bytes, started.elapsed().as_nanos());
    fs::write(output.join("preparation.json"), summary)?;
    eprintln!(
        "prepared {files} files; {} packs, {} bytes",
        packer.packs, packer.bytes
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.as_slice() {
        [_, action, master, output] if action == "prepare" => prepare(Path::new(master), Path::new(output)),
        [_, action, master, output, downloaded] if action == "verify" => read::verify(Path::new(master), Path::new(output), Path::new(downloaded)),
        [_, action, master, output, downloaded, cursor, count, receipt] if action == "verify-batch" => read::verify_batch(Path::new(master), Path::new(output), Path::new(downloaded), Path::new(cursor), count.parse()?, Path::new(receipt)),
        [_, action, master, output] if action == "diagnose" => attribution::diagnose(Path::new(master), Path::new(output)),
        [_, action] if action == "opt-selfcheck" => optimize::selfcheck(),
        [_, action, master, reference, output, mode] if action == "opt-root" && matches!(mode.as_str(),"cached"|"uncached") => optimize::root_updates(Path::new(master),Path::new(reference),Path::new(output),mode=="cached"),
        [_, action, reference, fixture, output, mode] if action == "opt-small" && matches!(mode.as_str(),"sized"|"stream") => optimize::small_sources(Path::new(reference),Path::new(fixture),Path::new(output),mode=="sized"),
        _ => Err("usage: minio_repository_probe prepare|diagnose MASTER OUTPUT | verify MASTER STAGE DOWNLOADED_PACKS | verify-batch MASTER STAGE DOWNLOADED_PACKS CURSOR COUNT RECEIPT".into()),
    }
}
