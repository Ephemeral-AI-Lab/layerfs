//! Standalone full-corpus import through public C1/C2 APIs; no product changes.
use std::error::Error;
use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Instant;

use layerfs_content::{construct_stream, ConstructionPolicy};
use layerfs_telemetry::timer::Timing;
use rusqlite::{params, Connection};

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
        let input = File::open(master.join("tree").join(std::ffi::OsStr::from_bytes(&path)))?;
        let (result, _) = Timing::disabled("repository.construct", |scope| {
            construct_stream(policy, &capacities, input, &mut packer, scope.child("file"))
        });
        let built = result?;
        if built.logical_len != size {
            return Err("fixture length changed".into());
        }
        packer.db.execute(
            "UPDATE entries SET content_root=?1 WHERE path=?2",
            params![built.root.as_bytes().as_slice(), path],
        )?;
        files += 1;
        logical += size;
        if files % 256 == 0 {
            packer.db.execute_batch("COMMIT; BEGIN;")?;
        }
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
        _ => Err("usage: minio_repository_probe prepare MASTER OUTPUT | verify MASTER STAGE DOWNLOADED_PACKS".into()),
    }
}
