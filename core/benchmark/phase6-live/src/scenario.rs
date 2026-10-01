//! Generic sealed command/manifest input for the experimental coordinator.
use crate::objects::Reader;
use layerfs_content::{
    filesystem::FilesystemRootId, inode_leaf::InodeKind, FilesystemRead, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Write},
    path::Path,
};
pub struct Step {
    pub name: String,
    pub command: String,
    pub manifest: String,
}
pub struct Scenario {
    pub id: String,
    pub steps: Vec<Step>,
}
fn number(r: &mut impl Read) -> Result<u32, String> {
    let mut b = [0; 4];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(u32::from_be_bytes(b))
}
fn text(r: &mut impl Read, max: usize) -> Result<String, String> {
    let n = number(r)? as usize;
    if n > max {
        return Err("scenario text admission".into());
    }
    let mut b = vec![0; n];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    String::from_utf8(b).map_err(|e| e.to_string())
}
fn label(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
impl Scenario {
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let mut header = [0; 8];
        f.read_exact(&mut header).map_err(|e| e.to_string())?;
        if &header != b"P6CASE1\0" {
            return Err("scenario framing".into());
        }
        let id = text(&mut f, 128)?;
        if !label(&id) {
            return Err("scenario label".into());
        }
        let n = number(&mut f)?;
        if n == 0 || n > 8 {
            return Err("scenario steps admission".into());
        }
        let mut steps = Vec::new();
        for _ in 0..n {
            let name = text(&mut f, 128)?;
            if !label(&name) {
                return Err("scenario step label".into());
            }
            let command = text(&mut f, 4096)?;
            let manifest = text(&mut f, 1024 * 1024)?;
            parse(&manifest)?;
            steps.push(Step {
                name,
                command,
                manifest,
            });
        }
        if f.read(&mut [0]).map_err(|e| e.to_string())? != 0 {
            return Err("scenario trailing bytes".into());
        }
        Ok(Self { id, steps })
    }
}
#[derive(Clone)]
struct Expected {
    kind: String,
    mode: u32,
    size: u64,
    sha: String,
}
fn parse(text: &str) -> Result<BTreeMap<String, Expected>, String> {
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5 || result.len() >= 512 {
            return Err("manifest framing/admission".into());
        }
        let kind = fields[1];
        if !["f", "d"].contains(&kind) {
            return Err("manifest kind unsupported".into());
        }
        let size = fields[3].parse().map_err(|_| "manifest size")?;
        if (kind == "d" && (size != 0 || fields[4] != "-"))
            || (kind == "f"
                && (fields[4].len() != 64 || !fields[4].bytes().all(|b| b.is_ascii_hexdigit())))
        {
            return Err("manifest digest/length".into());
        }
        if result
            .insert(
                fields[0].into(),
                Expected {
                    kind: kind.into(),
                    mode: fields[2].parse().map_err(|_| "manifest mode")?,
                    size,
                    sha: fields[4].into(),
                },
            )
            .is_some()
        {
            return Err("duplicate manifest path".into());
        }
    }
    if !result.contains_key(".") {
        return Err("manifest root absent".into());
    }
    Ok(result)
}
struct Hash {
    hash: Sha256,
    bytes: u64,
}
impl Write for Hash {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.hash.update(b);
        self.bytes += b.len() as u64;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn verify(reader: &Reader, root: [u8; 32], manifest: &str) -> Result<(usize, u64), String> {
    let mut expected = parse(manifest)?;
    let mut fs = FilesystemRead::new(
        reader,
        FilesystemRootId(ObjectId::from_bytes(&root).map_err(|e| e.to_string())?),
    )
    .map_err(|e| e.to_string())?;
    let mut pending = vec![(String::new(), fs.root().root_inode().serial())];
    let mut directories = BTreeSet::new();
    let mut files = 0;
    let mut bytes = 0;
    while let Some((path, id)) = pending.pop() {
        let key = if path.is_empty() { "." } else { &path };
        let e = expected
            .remove(key)
            .ok_or_else(|| format!("unexpected committed path {key}"))?;
        let value = fs.resolve_inode(id).map_err(|e| e.to_string())?.value;
        let portable = fs.read_portable_inode(id).map_err(|e| e.to_string())?;
        if portable.mode != e.mode {
            return Err(format!("mode mismatch {key}"));
        }
        match value.kind {
            InodeKind::Directory => {
                if e.kind != "d" || !directories.insert(id) {
                    return Err(format!("directory graph/kind mismatch {key}"));
                }
                let mut after = None;
                loop {
                    let page = fs
                        .list_inode(id, after.as_ref(), 128, 16384)
                        .map_err(|e| e.to_string())?;
                    for (name, child) in page.entries {
                        let name = name.as_str();
                        let child_path = if path.is_empty() {
                            name.to_owned()
                        } else {
                            format!("{path}/{name}")
                        };
                        pending.push((child_path, child));
                        if pending.len() > 512 {
                            return Err("proof pending admission".into());
                        }
                    }
                    after = page.continuation;
                    if after.is_none() {
                        break;
                    }
                }
            }
            InodeKind::RegularFile => {
                if e.kind != "f" {
                    return Err(format!("file kind mismatch {key}"));
                }
                let mut sink = Hash {
                    hash: Sha256::new(),
                    bytes: 0,
                };
                let (r, _) = Timing::disabled("full semantic manifest", |t| {
                    layerfs_content::read_all_bounded(
                        reader,
                        value.content_root,
                        e.size,
                        &mut sink,
                        t.child("file"),
                    )
                });
                r.map_err(|e| e.to_string())?;
                if sink.bytes != e.size || crate::minio::hex(&sink.hash.finalize()) != e.sha {
                    return Err(format!("content digest/size mismatch {key}"));
                }
                files += 1;
                bytes += sink.bytes;
            }
            _ => return Err("proof symlink capability unqualified".into()),
        }
    }
    if !expected.is_empty() {
        return Err(format!("missing committed paths {}", expected.len()));
    }
    Ok((files, bytes))
}
