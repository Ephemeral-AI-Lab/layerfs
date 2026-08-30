use layerfs_monitor::OperationId;
use layerfs_storage::DiffEntry;
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
        producer: impl FnOnce(
            &mut dyn FnMut(DiffEntry) -> layerfs_storage::Result<()>,
        ) -> crate::Result<()>,
    ) -> crate::Result<Self> {
        let path = spool_path();
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|_| crate::SdkError::InvalidRequest("Diff spool"))?;
        let mut emit = |entry: DiffEntry| {
            let bytes = layerfs_storage::encode_diff_entry(&entry)?;
            file.write_all(&(bytes.len() as u32).to_be_bytes())
                .and_then(|_| file.write_all(&bytes))
                .map_err(layerfs_storage::StorageError::Io)
        };
        if let Err(error) = producer(&mut emit) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        file.flush()
            .map_err(|_| crate::SdkError::InvalidRequest("Diff spool"))?;
        drop(file);
        let file = File::open(&path).map_err(|_| crate::SdkError::InvalidRequest("Diff spool"))?;
        Ok(Self {
            id,
            reader: Mutex::new(DiffReader {
                file,
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
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| crate::SdkError::InvalidRequest("operation lock"))?;
        reader.next_page()
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
            entries.push(layerfs_storage::decode_diff_entry(&bytes)?);
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

fn read_record(file: &mut File) -> crate::Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    match file.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("one-byte read buffer"),
        Err(_) => return Err(crate::SdkError::InvalidRequest("Diff spool")),
    }
    file.read_exact(&mut length[1..])
        .map_err(|_| crate::SdkError::InvalidRequest("Diff spool"))?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 16 * 1024 {
        return Err(crate::SdkError::InvalidRequest("Diff spool entry"));
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)
        .map_err(|_| crate::SdkError::InvalidRequest("Diff spool"))?;
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
