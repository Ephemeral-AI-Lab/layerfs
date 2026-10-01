use crate::{minio::Minio, pending::Pending, read_window};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::encoding::CompressionWorkspace;
use std::sync::{Arc, Mutex};
pub const ROLES: [ObjectRole; 13] = [
    ObjectRole::WholeFile,
    ObjectRole::Chunk,
    ObjectRole::ExtentLeaf,
    ObjectRole::ExtentBranch,
    ObjectRole::FileState,
    ObjectRole::InodeLeaf,
    ObjectRole::DirectoryLeaf,
    ObjectRole::DirectoryBranch,
    ObjectRole::InodeBranch,
    ObjectRole::FilesystemRoot,
    ObjectRole::AttributeLeaf,
    ObjectRole::AttributeBranch,
    ObjectRole::Symlink,
];
#[derive(Clone, Debug)]
pub struct Locator {
    pub id: ObjectId,
    pub role: ObjectRole,
    pub length: usize,
    pub pack: [u8; 32],
    pub group: u32,
    pub record: u32,
    pub pack_id: u64,
}
pub trait Locators: Send + Sync {
    fn lookup_many(&self, ids: &[ObjectId]) -> Result<Vec<Option<Locator>>, String>;
    fn register_many(&self, rows: &[Locator]) -> Result<Vec<Locator>, String>;
    fn lookup(&self, id: ObjectId) -> Result<Option<Locator>, String> {
        self.lookup_many(&[id])?
            .pop()
            .ok_or("lookup cardinality".into())
    }
    fn register(&self, row: &Locator) -> Result<(), String> {
        self.register_many(std::slice::from_ref(row)).map(|_| ())
    }
}
#[derive(Clone)]
pub struct Reader {
    pub s3: Minio,
    pub locators: Arc<dyn Locators>,
    pub pending: Option<Arc<Mutex<Pending>>>,
}
fn err(message: String) -> ContentError {
    eprintln!("object provider: {message}");
    ContentError::Io
}
pub fn read_locator(s3: &Minio, row: &Locator) -> Result<Vec<u8>, String> {
    read_window::read(s3, std::slice::from_ref(row))?
        .pop()
        .ok_or("read cardinality".into())
}
impl Reader {
    pub fn new(s3: Minio, locators: Arc<dyn Locators>) -> Self {
        Self {
            s3,
            locators,
            pending: None,
        }
    }
}
impl AuthenticatedObjects for Reader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > 4096 {
            return Err(ContentError::Io);
        }
        let mut locations: Vec<Option<Locator>> = (0..ids.len()).map(|_| None).collect();
        let mut private = vec![false; ids.len()];
        let mut total = 0usize;
        for (page_no, page) in ids.chunks(read_window::PAGE).enumerate() {
            let mut missing = Vec::new();
            let mut positions = Vec::new();
            for (j, id) in page.iter().enumerate() {
                let index = page_no * read_window::PAGE + j;
                let length = match &self.pending {
                    Some(p) => p
                        .lock()
                        .map_err(|_| ContentError::Io)?
                        .get(*id)
                        .map(|o| o.canonical_len()),
                    None => None,
                };
                if let Some(n) = length {
                    private[index] = true;
                    total = total.checked_add(n).ok_or(ContentError::Io)?;
                } else {
                    missing.push(*id);
                    positions.push(index);
                }
            }
            if !missing.is_empty() {
                let rows = self.locators.lookup_many(&missing).map_err(err)?;
                if rows.len() != positions.len() {
                    return Err(ContentError::Io);
                }
                for (index, row) in positions.into_iter().zip(rows) {
                    let row = row.ok_or(ContentError::MissingObject)?;
                    if row.id != ids[index] {
                        return Err(ContentError::Io);
                    }
                    total = total.checked_add(row.length).ok_or(ContentError::Io)?;
                    locations[index] = Some(row);
                }
            }
        }
        if total > read_window::BYTES {
            return Err(err("read wave canonical byte admission".into()));
        }
        let mut out: Vec<Option<Vec<u8>>> = (0..ids.len()).map(|_| None).collect();
        for (index, id) in ids.iter().enumerate() {
            if private[index] {
                let p = self
                    .pending
                    .as_ref()
                    .ok_or(ContentError::Io)?
                    .lock()
                    .map_err(|_| ContentError::Io)?;
                out[index] = Some(p.get(*id).ok_or(ContentError::Io)?.canonical().to_vec());
            }
        }
        for start in (0..ids.len()).step_by(read_window::PAGE) {
            let end = (start + read_window::PAGE).min(ids.len());
            let mut rows = Vec::new();
            let mut positions = Vec::new();
            for (index, location) in locations.iter().enumerate().take(end).skip(start) {
                if let Some(row) = location {
                    rows.push(row.clone());
                    positions.push(index);
                }
            }
            let values = read_window::read(&self.s3, &rows).map_err(err)?;
            for (index, value) in positions.into_iter().zip(values) {
                out[index] = Some(value);
            }
        }
        out.into_iter().map(|v| v.ok_or(ContentError::Io)).collect()
    }
    fn read_canonical(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        if let Some(p) = &self.pending {
            if let Some(object) = p.lock().map_err(|_| ContentError::Io)?.get(id) {
                return Ok(object.canonical().to_vec());
            }
        }
        let row = self
            .locators
            .lookup(id)
            .map_err(err)?
            .ok_or(ContentError::MissingObject)?;
        read_locator(&self.s3, &row).map_err(err)
    }
}
pub struct Consumer {
    pub reader: Reader,
    compression: CompressionWorkspace,
    pub objects: u64,
    pub bytes: u64,
    pub duplicates: u64,
    pub packs: u64,
    pub peak_objects: usize,
    pub peak_bytes: usize,
    failed: bool,
}
impl Consumer {
    pub fn new(mut reader: Reader) -> Result<Self, String> {
        reader.pending = Some(Arc::new(Mutex::new(Pending::default())));
        Ok(Self {
            reader,
            compression: CompressionWorkspace::new().map_err(|e| e.to_string())?,
            objects: 0,
            bytes: 0,
            duplicates: 0,
            packs: 0,
            peak_objects: 0,
            peak_bytes: 0,
            failed: false,
        })
    }
    pub fn finish(&mut self) -> Result<(), String> {
        if self.failed {
            return Err("construction consumer failed; no resend".into());
        }
        let result = self.flush();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn flush(&mut self) -> Result<(), String> {
        let pending = self.reader.pending.as_ref().ok_or("private owner")?.clone();
        let objects = {
            let mut p = pending.lock().map_err(|_| "pending owner")?;
            self.peak_objects = self.peak_objects.max(p.peak_objects);
            self.peak_bytes = self.peak_bytes.max(p.peak_bytes);
            p.drain()
        };
        if objects.is_empty() {
            return Ok(());
        }
        let mut new = Vec::new();
        let mut iter = objects.into_iter();
        loop {
            let page: Vec<_> = iter.by_ref().take(read_window::PAGE).collect();
            if page.is_empty() {
                break;
            }
            let ids: Vec<_> = page.iter().map(|o| o.id()).collect();
            let locations = self.reader.locators.lookup_many(&ids)?;
            if locations.len() != page.len() {
                return Err("membership cardinality".into());
            }
            for (object, row) in page.iter().zip(&locations) {
                if let Some(row) = row {
                    if row.id != object.id()
                        || row.role != object.role()
                        || row.length != object.canonical_len()
                    {
                        return Err("membership exact role/length".into());
                    }
                }
            }
            let existing: Vec<_> = locations.iter().flatten().cloned().collect();
            let mut existing_bytes = read_window::read(&self.reader.s3, &existing)?.into_iter();
            for (object, row) in page.into_iter().zip(locations) {
                match row {
                    Some(row) => {
                        if row.id != object.id()
                            || row.role != object.role()
                            || row.length != object.canonical_len()
                            || existing_bytes.next().ok_or("exact result cardinality")?
                                != object.canonical()
                        {
                            return Err("exact CAS mismatch".into());
                        }
                        self.duplicates += 1;
                    }
                    None => new.push(object),
                }
            }
        }
        for pack in crate::packing::build(new, &mut self.compression)? {
            self.reader.s3.put(&pack.rows[0].pack, &pack.bytes)?;
            for page in pack.rows.chunks(read_window::PAGE) {
                let normalized = self.reader.locators.register_many(page)?;
                if normalized.len() != page.len()
                    || normalized.iter().zip(page).any(|(a, b)| {
                        a.id != b.id || a.role != b.role || a.length != b.length || a.pack_id == 0
                    })
                {
                    return Err("registration identity/cardinality".into());
                }
            }
            self.objects += pack.rows.len() as u64;
            self.bytes += pack.bytes.len() as u64;
            self.packs += 1;
        }
        Ok(())
    }
}
impl FinalizedConsumer for Consumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if self.failed {
            return Err(ContentError::Io);
        }
        if object.canonical_len() > crate::pending::BYTES {
            self.failed = true;
            return Err(err("single pending object admission".into()));
        }
        let p = self
            .reader
            .pending
            .as_ref()
            .ok_or(ContentError::Io)?
            .clone();
        let fits = {
            let window = p.lock().map_err(|_| ContentError::Io)?;
            window.get(object.id()).is_some() || window.fits(object.canonical_len())
        };
        if !fits {
            self.finish().map_err(err)?
        }
        let inserted = p
            .lock()
            .map_err(|_| ContentError::Io)?
            .push(object)
            .map_err(err)?;
        if !inserted {
            self.duplicates += 1;
        }
        Ok(())
    }
}
