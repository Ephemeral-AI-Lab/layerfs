use layerfs_content::filesystem::{DiffAspects, DiffEntry, NodeSummary};
use layerfs_content::tree::inode::InodeKind;
use layerfs_content::{CanonicalPath, ObjectId};
use layerfs_monitor::OperationId;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffPage {
    pub entries: Vec<DiffEntry>,
    pub continuation: Option<u64>,
}

pub struct OperationHandle {
    id: OperationId,
    reader: Mutex<DiffReader>,
}

struct DiffReader {
    file: File,
    path: PathBuf,
    pending: Option<Vec<u8>>,
    page: u64,
    finished: bool,
}

impl OperationHandle {
    pub(crate) fn build(
        id: OperationId,
        producer: impl FnOnce(&mut dyn FnMut(DiffEntry) -> crate::Result<()>) -> crate::Result<()>,
    ) -> crate::Result<Self> {
        let path = spool_path();
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let mut emit = |entry: DiffEntry| {
            let bytes = encode_entry(&entry)?;
            file.write_all(&(bytes.len() as u32).to_be_bytes())?;
            file.write_all(&bytes)?;
            Ok(())
        };
        if let Err(error) = producer(&mut emit) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        file.flush()?;
        drop(file);
        Ok(Self {
            id,
            reader: Mutex::new(DiffReader {
                file: File::open(&path)?,
                path,
                pending: None,
                page: 0,
                finished: false,
            }),
        })
    }

    pub fn id(&self) -> OperationId {
        self.id
    }

    pub fn next_diff_page(&self) -> crate::Result<Option<DiffPage>> {
        self.reader
            .lock()
            .map_err(|_| crate::SdkError::InvalidRequest("operation lock"))?
            .next_page()
    }
}

impl DiffReader {
    fn next_page(&mut self) -> crate::Result<Option<DiffPage>> {
        if self.finished {
            return Ok(None);
        }
        let mut entries = Vec::with_capacity(128);
        while entries.len() < 128 {
            let bytes = match self.pending.take() {
                Some(bytes) => Some(bytes),
                None => read_record(&mut self.file)?,
            };
            let Some(bytes) = bytes else {
                self.finished = true;
                break;
            };
            entries.push(decode_entry(&bytes)?);
        }
        if !self.finished {
            self.pending = read_record(&mut self.file)?;
            self.finished = self.pending.is_none();
        }
        if entries.is_empty() {
            return Ok(None);
        }
        self.page += 1;
        Ok(Some(DiffPage {
            entries,
            continuation: (!self.finished).then_some(self.page),
        }))
    }
}

impl Drop for DiffReader {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn encode_entry(entry: &DiffEntry) -> crate::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    match entry {
        DiffEntry::Add { path, after } => {
            bytes.push(0);
            encode_path(&mut bytes, path)?;
            encode_summary(&mut bytes, *after);
        }
        DiffEntry::Remove { path, before } => {
            bytes.push(1);
            encode_path(&mut bytes, path)?;
            encode_summary(&mut bytes, *before);
        }
        DiffEntry::Modify {
            path,
            before,
            after,
            aspects,
        } => {
            bytes.push(2);
            encode_path(&mut bytes, path)?;
            encode_summary(&mut bytes, *before);
            encode_summary(&mut bytes, *after);
            bytes.push(
                u8::from(aspects.node_type)
                    | (u8::from(aspects.content) << 1)
                    | (u8::from(aspects.metadata) << 2)
                    | (u8::from(aspects.directory_membership) << 3)
                    | (u8::from(aspects.hard_links) << 4),
            );
        }
    }
    Ok(bytes)
}

fn decode_entry(bytes: &[u8]) -> crate::Result<DiffEntry> {
    let mut cursor = Cursor { bytes, offset: 0 };
    let tag = cursor.u8()?;
    let path_length = cursor.u32()? as usize;
    let path = CanonicalPath::from_bytes(cursor.bytes(path_length)?)
        .map_err(|_| crate::SdkError::InvalidRequest("Diff path"))?;
    let entry = match tag {
        0 => DiffEntry::Add {
            path,
            after: cursor.summary()?,
        },
        1 => DiffEntry::Remove {
            path,
            before: cursor.summary()?,
        },
        2 => {
            let before = cursor.summary()?;
            let after = cursor.summary()?;
            let flags = cursor.u8()?;
            DiffEntry::Modify {
                path,
                before,
                after,
                aspects: DiffAspects {
                    node_type: flags & 1 != 0,
                    content: flags & 2 != 0,
                    metadata: flags & 4 != 0,
                    directory_membership: flags & 8 != 0,
                    hard_links: flags & 16 != 0,
                },
            }
        }
        _ => return Err(crate::SdkError::InvalidRequest("Diff entry")),
    };
    if cursor.offset != bytes.len() {
        return Err(crate::SdkError::InvalidRequest("Diff entry"));
    }
    Ok(entry)
}

fn encode_path(bytes: &mut Vec<u8>, path: &CanonicalPath) -> crate::Result<()> {
    let length: u32 = path
        .as_bytes()
        .len()
        .try_into()
        .map_err(|_| crate::SdkError::InvalidRequest("Diff path"))?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(path.as_bytes());
    Ok(())
}

fn encode_summary(bytes: &mut Vec<u8>, summary: NodeSummary) {
    bytes.push(match summary.kind {
        InodeKind::RegularFile => 0,
        InodeKind::Directory => 1,
        InodeKind::Symlink => 2,
    });
    bytes.extend_from_slice(summary.content_root.as_bytes());
    bytes.extend_from_slice(summary.metadata_root.as_bytes());
    bytes.extend_from_slice(&summary.namespace_ref_count.to_be_bytes());
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Cursor<'_> {
    fn bytes(&mut self, length: usize) -> crate::Result<&[u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(crate::SdkError::InvalidRequest("Diff entry"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(crate::SdkError::InvalidRequest("Diff entry"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> crate::Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> crate::Result<u32> {
        Ok(u32::from_be_bytes(
            self.bytes(4)?.try_into().expect("four bytes"),
        ))
    }

    fn summary(&mut self) -> crate::Result<NodeSummary> {
        let kind = match self.u8()? {
            0 => InodeKind::RegularFile,
            1 => InodeKind::Directory,
            2 => InodeKind::Symlink,
            _ => return Err(crate::SdkError::InvalidRequest("Diff node kind")),
        };
        let content_root = ObjectId::from_bytes(self.bytes(32)?)
            .map_err(|_| crate::SdkError::InvalidRequest("Diff ObjectId"))?;
        let metadata_root = ObjectId::from_bytes(self.bytes(32)?)
            .map_err(|_| crate::SdkError::InvalidRequest("Diff ObjectId"))?;
        let namespace_ref_count =
            u64::from_be_bytes(self.bytes(8)?.try_into().expect("eight bytes"));
        Ok(NodeSummary {
            kind,
            content_root,
            metadata_root,
            namespace_ref_count,
        })
    }
}

fn read_record(file: &mut File) -> crate::Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    match file.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("one-byte read buffer"),
        Err(error) => return Err(error.into()),
    }
    file.read_exact(&mut length[1..])?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 16 * 1024 {
        return Err(crate::SdkError::InvalidRequest("Diff spool entry"));
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

fn spool_path() -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "layerfs-diff-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
