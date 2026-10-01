use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use layerfs_content::{
    read_all_bounded, AuthenticatedObjects, ContentError, ContentResult, ObjectId, ObjectRole,
};
use layerfs_storage::encoding::{decode_canonical, DecompressionWorkspace, GroupCache};
use layerfs_storage::sqlite::lookup::ObjectLocation;
use layerfs_storage::{StorageCapacities, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use rusqlite::{params, params_from_iter, types::Value, Connection};

use super::Result;
struct Cache {
    packs: BTreeMap<i64, Vec<u8>>,
    workspace: DecompressionWorkspace,
    groups: GroupCache,
    group_decodes: u64,
}
struct Provider {
    db: Connection,
    root: PathBuf,
    capacities: StorageCapacities,
    cache: RefCell<Cache>,
}
impl Provider {
    fn batch(&self, ids: &[ObjectId]) -> Result<Vec<Vec<u8>>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > 128 {
            return Err("lookup width exceeds 128".into());
        }
        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let values = ids
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let mut stmt = self.db.prepare_cached(&format!("SELECT id,role,canonical_length,pack_id,group_no,record_no FROM objects WHERE id IN({placeholders})"))?;
        let mut rows = stmt.query(params_from_iter(&values))?;
        let mut locations = BTreeMap::new();
        while let Some(r) = rows.next()? {
            let raw: Vec<u8> = r.get(0)?;
            let id = ObjectId::from_bytes(&raw)?;
            locations.insert(
                id,
                ObjectLocation {
                    object_id: id,
                    role: ObjectRole::from_code(r.get(1)?)?,
                    canonical_length: usize::try_from(r.get::<_, i64>(2)?)?,
                    pack_id: r.get(3)?,
                    group_number: usize::try_from(r.get::<_, i64>(4)?)?,
                    record_number: usize::try_from(r.get::<_, i64>(5)?)?,
                },
            );
        }
        let mut cache = self.cache.borrow_mut();
        let Cache {
            packs,
            workspace,
            groups,
            group_decodes,
        } = &mut *cache;
        let mut output = Vec::with_capacity(ids.len());
        for id in ids {
            let loc = locations.get(id).ok_or("missing requested identity")?;
            if !packs.contains_key(&loc.pack_id) {
                if packs.len() == 8 {
                    packs.clear();
                }
                let bytes = fs::read(self.root.join(format!("pack-{:08}.bin", loc.pack_id)))?;
                if bytes.len() > 256 * 1024 {
                    return Err("pack read admission".into());
                }
                packs.insert(loc.pack_id, bytes);
            }
            let canonical = decode_canonical(
                &packs[&loc.pack_id],
                loc,
                &self.capacities,
                None,
                workspace,
                groups,
                group_decodes,
            )?;
            if ObjectId::for_bytes(&canonical) != *id {
                return Err("canonical identity mismatch".into());
            }
            output.push(canonical);
        }
        Ok(output)
    }
}
impl AuthenticatedObjects for Provider {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.batch(ids).map_err(|error| {
            eprintln!("authenticated read failed: {error}");
            ContentError::ProviderFailure {
                what: "standalone repository authenticated provider",
            }
        })
    }
}
struct Comparator {
    source: File,
    bytes: u64,
    scratch: [u8; 65536],
}
impl Write for Comparator {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        for part in input.chunks(self.scratch.len()) {
            self.source.read_exact(&mut self.scratch[..part.len()])?;
            if self.scratch[..part.len()] != *part {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "original file byte mismatch",
                ));
            }
            self.bytes += part.len() as u64;
        }
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn verify(master: &Path, stage: &Path, downloaded: &Path) -> Result<()> {
    selected(master, stage, downloaded, None)
}

pub fn verify_batch(
    master: &Path,
    stage: &Path,
    downloaded: &Path,
    cursor: &Path,
    count: usize,
    receipt: &Path,
) -> Result<()> {
    if count == 0 || count > 4096 {
        return Err("batch count admission".into());
    }
    selected(
        master,
        stage,
        downloaded,
        Some((fs::read(cursor)?, count, receipt)),
    )
}

fn selected(
    master: &Path,
    stage: &Path,
    downloaded: &Path,
    batch: Option<(Vec<u8>, usize, &Path)>,
) -> Result<()> {
    let started = Instant::now();
    let db = Connection::open(stage.join("catalog.sqlite"))?;
    db.execute_batch(
        "PRAGMA query_only=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0; PRAGMA busy_timeout=0;",
    )?;
    if batch.is_none() {
        db.execute(
            "ATTACH DATABASE ?1 AS original",
            [master
                .join("manifest.sqlite")
                .to_str()
                .ok_or("non-UTF8 database path")?],
        )?;
        for table in ["entries", "xattrs"] {
            let columns = if table == "entries" {
                "path,kind,mode,mtime,size,dev,ino,uid,gid,flags,link,sha256"
            } else {
                "path,name,value"
            };
            for (a, b) in [("main", "original"), ("original", "main")] {
                let count: i64 = db.query_row(&format!("SELECT count(*) FROM(SELECT {columns} FROM {a}.{table} EXCEPT SELECT {columns} FROM {b}.{table})"), [], |r| r.get(0))?;
                if count != 0 {
                    return Err("metadata comparison mismatch".into());
                }
            }
        }
    }
    let ready: u8 = db.query_row("SELECT ready FROM publication", [], |r| r.get(0))?;
    if ready != 1 {
        return Err("catalog not published READY".into());
    }
    let provider = Provider {
        db,
        root: downloaded.to_path_buf(),
        capacities: StorageCapacities::from_policy(StoragePolicy::default())?,
        cache: RefCell::new(Cache {
            packs: BTreeMap::new(),
            workspace: DecompressionWorkspace::new()?,
            groups: GroupCache::new(),
            group_decodes: 0,
        }),
    };
    let (cursor, limit, receipt) = match &batch {
        Some((cursor, limit, receipt)) => (cursor.clone(), i64::try_from(*limit)?, *receipt),
        None => (Vec::new(), i64::MAX, stage),
    };
    let mut query = provider.db.prepare("SELECT path,size,content_root FROM entries WHERE kind='file' AND path>?1 ORDER BY path LIMIT ?2")?;
    let mut rows = query.query(params![cursor, limit])?;
    let mut last = Vec::new();
    let (mut files, mut bytes) = (0_u64, 0_u64);
    while let Some(r) = rows.next()? {
        let path: Vec<u8> = r.get(0)?;
        let size = u64::try_from(r.get::<_, i64>(1)?)?;
        let root: Vec<u8> = r.get(2)?;
        let mut sink = Comparator {
            source: File::open(master.join("tree").join(std::ffi::OsStr::from_bytes(&path)))?,
            bytes: 0,
            scratch: [0; 65536],
        };
        let (result, _) = Timing::disabled("repository.read", |scope| {
            read_all_bounded(
                &provider,
                ObjectId::from_bytes(&root)?,
                size,
                &mut sink,
                scope.child("file"),
            )
        });
        result?;
        let mut eof = [0];
        if sink.bytes != size || sink.source.read(&mut eof)? != 0 {
            return Err("original EOF/length mismatch".into());
        }
        files += 1;
        bytes += size;
        last = path;
        if batch.is_none() && files % 1024 == 0 {
            fs::write(stage.join("reconstruction-progress.json"), format!("{{\"files\":{files},\"bytes\":{bytes},\"metadata\":\"PASS\",\"complete\":false}}\n"))?;
        }
    }
    if batch.is_some() {
        if files != limit as u64 {
            return Err("batch coverage cardinality".into());
        }
        fs::write(receipt.join("next-cursor.bin"), &last)?;
        fs::write(receipt.join("batch-proof.json"),format!("{{\"proof\":\"PASS\",\"files\":{files},\"bytes\":{bytes},\"exact_eof\":true,\"next_cursor_blake3\":\"{}\",\"wall_ns\":{}}}\n",blake3::hash(&last).to_hex(),started.elapsed().as_nanos()))?;
    } else {
        fs::write(stage.join("reconstruction.json"),format!("{{\"proof\":\"PASS\",\"files\":{files},\"bytes\":{bytes},\"metadata\":\"PASS\",\"exact_eof\":true,\"wall_ns\":{}}}\n",started.elapsed().as_nanos()))?;
    }
    eprintln!("exact reconstruction PASS: {files} files, {bytes} bytes");
    Ok(())
}
