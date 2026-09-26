//! Independent full native-import oracle over public C1, C2 and C5 reads.
use layerfs_content::{
    filesystem::attributes::portable::PortableMetadata,
    filesystem::attributes::read::{read_portable, AttributeReadWork},
    inode_leaf::{InodeKind, InodeValue},
    read_all, FilesystemRead, LogicalPath, ObjectId,
};
use layerfs_history::{sqlite::open_read_only, HistoryCatalog, LayerStackId};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::Timing;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    fs,
    io::{self, Write},
    path::Path,
    sync::Mutex,
};

const VERIFY_WORKERS: usize = 4;
const SAMPLE_TARGET: usize = 64;
const SAMPLE_POLICY: &str = "stride64-size-log2-endpoints-v1";
type MetadataMemo = Option<((ObjectId, InodeKind), PortableMetadata)>;

#[derive(Clone)]
struct Expected {
    kind: char,
    mode: u32,
    mtime_ns: i128,
    size: u64,
    sha256: String,
}

struct DigestWriter {
    hash: Sha256,
    bytes: u64,
}
impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("string write");
    }
    out
}

fn unhex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 {
        return Err("odd hex width".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).map_err(Into::into))
        .collect()
}

fn manifest(bytes: &[u8]) -> Result<BTreeMap<String, Expected>, Box<dyn std::error::Error>> {
    let mut expected = BTreeMap::new();
    for line in std::str::from_utf8(bytes)?.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 6 {
            return Err("manifest row width".into());
        }
        let path = if parts[0] == "." { "" } else { parts[0] };
        let value = Expected {
            kind: parts[1].parse()?,
            mode: parts[2].parse()?,
            mtime_ns: parts[3].parse()?,
            size: parts[4].parse()?,
            sha256: parts[5].into(),
        };
        if expected.insert(path.into(), value).is_some() {
            return Err("duplicate expected path".into());
        }
    }
    if expected.get("").is_none_or(|root| root.kind != 'd') {
        return Err("missing expected root".into());
    }
    Ok(expected)
}

/// A fixed-size cross-tree sample plus both endpoints of each occupied size band.
fn sampled_paths(expected: &BTreeMap<String, Expected>) -> BTreeSet<String> {
    let count = expected.values().filter(|row| row.kind == 'f').count();
    let stride = count.div_ceil(SAMPLE_TARGET).max(1);
    let mut first = [None; 65];
    let mut last = [None; 65];
    let mut sampled = BTreeSet::new();
    for (index, (path, row)) in expected
        .iter()
        .filter(|(_, row)| row.kind == 'f')
        .enumerate()
    {
        if index % stride == 0 || index + 1 == count {
            sampled.insert(path.clone());
        }
        let bucket = if row.size == 0 {
            0
        } else {
            1 + row.size.ilog2() as usize
        };
        first[bucket].get_or_insert(path.as_str());
        last[bucket] = Some(path.as_str());
    }
    for path in first.into_iter().chain(last).flatten() {
        sampled.insert(path.to_owned());
    }
    sampled
}

fn check_metadata(
    provider: &StoreProvider<'_>,
    value: InodeValue,
    wanted: &Expected,
    path: &str,
    work: &mut AttributeReadWork,
    memo: &mut MetadataMemo,
) -> Result<(), String> {
    let kind = match value.kind {
        InodeKind::Directory => 'd',
        InodeKind::RegularFile => 'f',
        InodeKind::Symlink => 's',
    };
    let key = (value.metadata_root, value.kind);
    let portable = match *memo {
        Some((previous, portable)) if previous == key => portable,
        _ => {
            let portable = read_portable(provider, value.metadata_root, value.kind, work)
                .map_err(|error| format!("metadata read {path}: {error}"))?;
            *memo = Some((key, portable));
            portable
        }
    };
    if kind != wanted.kind
        || portable.mode != wanted.mode
        || i128::from(portable.mtime_seconds) * 1_000_000_000
            + i128::from(portable.mtime_nanoseconds)
            != wanted.mtime_ns
    {
        return Err(format!("metadata mismatch: {path}"));
    }
    Ok(())
}

fn verify_files(
    store: &Store,
    expected: &BTreeMap<String, Expected>,
    jobs: VecDeque<(String, InodeValue)>,
) -> Result<(u64, u64, u64), String> {
    let queue = Mutex::new(jobs);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..VERIFY_WORKERS {
            let queue = &queue;
            workers.push(scope.spawn(move || -> Result<(u64, u64, u64), String> {
                let provider = StoreProvider::new(store);
                let mut attributes = AttributeReadWork::default();
                let mut memo = None;
                let mut files = 0u64;
                let mut bytes = 0u64;
                loop {
                    let job = queue
                        .lock()
                        .map_err(|_| "verifier queue poisoned")?
                        .pop_front();
                    let Some((path, value)) = job else { break };
                    let wanted = expected.get(&path).ok_or("unexpected actual path")?;
                    check_metadata(&provider, value, wanted, &path, &mut attributes, &mut memo)?;
                    let mut output = DigestWriter {
                        hash: Sha256::new(),
                        bytes: 0,
                    };
                    Timing::disabled("read", |timer| {
                        read_all(
                            &provider,
                            value.content_root,
                            &mut output,
                            timer.child("file"),
                        )
                    })
                    .0
                    .map_err(|error| format!("file read {path}: {error}"))?;
                    if output.bytes != wanted.size || hex(&output.hash.finalize()) != wanted.sha256
                    {
                        return Err(format!("content mismatch: {path}"));
                    }
                    files += 1;
                    bytes += output.bytes;
                    if files % 500 == 0 {
                        eprintln!("LFS237 verifier worker files={files} bytes={bytes} metadata_attribute_waves={}", attributes.read_waves);
                    }
                }
                Ok((files, bytes, attributes.read_waves))
            }));
        }
        let (mut files, mut bytes, mut metadata_waves) = (0u64, 0u64, 0u64);
        for worker in workers {
            let (count, length, waves) = worker.join().map_err(|_| "verifier worker panic")??;
            files += count;
            bytes += length;
            metadata_waves += waves;
        }
        Ok((files, bytes, metadata_waves))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 7 {
        return Err(
            "store history root-hex stack-body-hex manifest-tsv manifest-sha256 required".into(),
        );
    }
    let raw = fs::read(&args[5])?;
    let manifest_digest = hex(&Sha256::digest(&raw));
    if manifest_digest != args[6] {
        return Err("sealed manifest digest mismatch".into());
    }
    let expected = manifest(&raw)?;
    let sample = sampled_paths(&expected);
    let manifest_files = expected.values().filter(|row| row.kind == 'f').count();
    let manifest_bytes = expected
        .values()
        .filter(|row| row.kind == 'f')
        .try_fold(0_u64, |total, row| total.checked_add(row.size))
        .ok_or("manifest byte count overflow")?;
    let root = ObjectId::from_bytes(&unhex(&args[3])?)?;
    let stack: [u8; 16] = unhex(&args[4])?
        .try_into()
        .map_err(|_| "stack body width")?;
    let cursor: [u8; 32] = unhex(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?
        .try_into()
        .map_err(|_| "cursor key width")?;
    let history = open_read_only(Path::new(&args[2]), b"layerfs-bench-pro", cursor)?;
    let record = history
        .layer_stack(LayerStackId::from_authority(stack))?
        .ok_or("missing stack")?;
    let layer = history
        .layer(record.head_layer)?
        .ok_or("missing genesis layer")?;
    if layer.root != root {
        return Err("history root differs from returned root".into());
    }
    let store = Timing::disabled("open", |scope| Store::open(&args[1], scope.child("store"))).0?;
    let provider = StoreProvider::new(&store);
    let mut fs = FilesystemRead::new(
        &provider,
        layerfs_content::filesystem::root::FilesystemRootId(root),
    )?;
    let root_value = fs.resolve(&LogicalPath::root())?.value;
    let mut pending = VecDeque::from([(String::new(), root_value)]);
    let mut seen = BTreeSet::from([String::new()]);
    let mut attributes = AttributeReadWork::default();
    let mut memo = None;
    let mut jobs = VecDeque::new();
    let mut directories = 0u64;
    let mut discovered_files = 0usize;
    while let Some((path, value)) = pending.pop_front() {
        let wanted = expected.get(&path).ok_or("unexpected actual path")?;
        let logical = LogicalPath::new(&path)?;
        check_metadata(&provider, value, wanted, &path, &mut attributes, &mut memo)?;
        if value.kind != InodeKind::Directory {
            return Err(format!("listed non-directory for traversal: {path}").into());
        }
        directories += 1;
        let mut after = None;
        loop {
            let page = fs.list(&logical, after.as_ref(), 128, 16_384)?;
            let serials: Vec<_> = page.entries.iter().map(|(_, serial)| *serial).collect();
            let inodes = fs.lookup_inodes(&serials)?;
            for ((name, _), inode) in page.entries.into_iter().zip(inodes) {
                let child = if path.is_empty() {
                    name.as_str().to_owned()
                } else {
                    format!("{path}/{}", name.as_str())
                };
                if !seen.insert(child.clone()) {
                    return Err(format!("duplicate actual path: {child}").into());
                }
                let value = inode.ok_or("missing listed inode")?;
                let wanted = expected.get(&child).ok_or("unexpected actual path")?;
                match value.kind {
                    InodeKind::Directory => pending.push_back((child, value)),
                    InodeKind::RegularFile => {
                        if wanted.kind != 'f' {
                            return Err(format!("file kind mismatch: {child}").into());
                        }
                        discovered_files += 1;
                        if sample.contains(&child) {
                            jobs.push_back((child, value));
                        }
                    }
                    InodeKind::Symlink => return Err("unexpected actual symlink".into()),
                }
            }
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
    }
    if seen.len() != expected.len()
        || discovered_files != manifest_files
        || jobs.len() != sample.len()
    {
        return Err("missing output paths or sample".into());
    }
    eprintln!("LFS237 lite verifier traversal paths={} directories={} discovered_files={} sampled_files={} metadata_attribute_waves={}", seen.len(), directories, discovered_files, jobs.len(), attributes.read_waves);
    let (sampled_files, sampled_bytes, file_metadata_waves) =
        verify_files(&store, &expected, jobs).map_err(io::Error::other)?;
    if sampled_files as usize != sample.len() {
        return Err("sampled file count mismatch".into());
    }
    println!("{{\"status\":\"PASS\",\"paths\":{},\"discovered_files\":{},\"directories\":{},\"manifest_bytes\":{},\"sampled_files\":{},\"sampled_bytes\":{},\"sample_policy\":\"{}\",\"workers\":{},\"metadata_attribute_waves\":{},\"root\":\"{}\",\"manifest_sha256\":\"{}\"}}",
        seen.len(), discovered_files, directories, manifest_bytes, sampled_files, sampled_bytes,
        SAMPLE_POLICY, VERIFY_WORKERS, attributes.read_waves + file_metadata_waves, root, manifest_digest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_sample_covers_anchors_empty_sizes_and_tree_tail() {
        let mut expected = BTreeMap::new();
        for index in 0..256 {
            let size = match index {
                0 | 1 => 100_000_000,
                10 => 0,
                128 => 4_000,
                255 => 50_000,
                _ => 50,
            };
            expected.insert(
                format!("f{index:03}"),
                Expected {
                    kind: 'f',
                    mode: 0o640,
                    mtime_ns: 0,
                    size,
                    sha256: String::new(),
                },
            );
        }
        let sample = sampled_paths(&expected);
        for path in ["f000", "f001", "f010", "f128", "f255"] {
            assert!(sample.contains(path), "missing {path}");
        }
        assert!(sample.len() <= 195);
        assert!(sample.len() < expected.len());
    }
}
