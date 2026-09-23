//! Independent full native-import oracle over public C1, C2 and C5 reads.
use layerfs_content::{
    filesystem::attributes::read::{read_portable, AttributeReadWork},
    inode_leaf::InodeKind,
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
};

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
    let mut seen = BTreeSet::new();
    let mut attributes = AttributeReadWork::default();
    let mut files = 0u64;
    let mut directories = 0u64;
    let mut bytes = 0u64;
    while let Some((path, value)) = pending.pop_front() {
        if !seen.insert(path.clone()) {
            return Err("duplicate actual path".into());
        }
        let wanted = expected.get(&path).ok_or("unexpected actual path")?;
        let logical = LogicalPath::new(&path)?;
        let portable = read_portable(&provider, value.metadata_root, value.kind, &mut attributes)?;
        let actual_kind = match value.kind {
            InodeKind::Directory => 'd',
            InodeKind::RegularFile => 'f',
            InodeKind::Symlink => 's',
        };
        if actual_kind != wanted.kind
            || portable.mode != wanted.mode
            || i128::from(portable.mtime_seconds) * 1_000_000_000
                + i128::from(portable.mtime_nanoseconds)
                != wanted.mtime_ns
        {
            return Err(format!("metadata mismatch: {path}").into());
        }
        if actual_kind == 'd' {
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
                    pending.push_back((child, inode.ok_or("missing listed inode")?));
                }
                after = page.continuation;
                if after.is_none() {
                    break;
                }
            }
        } else if actual_kind == 'f' {
            files += 1;
            let mut output = DigestWriter {
                hash: Sha256::new(),
                bytes: 0,
            };
            Timing::disabled("read", |scope| {
                read_all(
                    &provider,
                    value.content_root,
                    &mut output,
                    scope.child("file"),
                )
            })
            .0?;
            if output.bytes != wanted.size || hex(&output.hash.finalize()) != wanted.sha256 {
                return Err(format!("content mismatch: {path}").into());
            }
            bytes += output.bytes;
        } else {
            return Err(format!("unsupported actual symlink: {path}").into());
        }
    }
    if seen.len() != expected.len() {
        return Err("missing output paths".into());
    }
    println!("{{\"status\":\"PASS\",\"paths\":{},\"files\":{},\"directories\":{},\"bytes\":{},\"root\":\"{}\",\"manifest_sha256\":\"{}\"}}",
        seen.len(), files, directories, bytes, root, manifest_digest);
    Ok(())
}
