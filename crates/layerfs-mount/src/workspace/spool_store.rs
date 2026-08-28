struct Spool {
    path: PathBuf,
    marker: [u8; SPOOL_MARKER_BYTES as usize],
    file: Option<File>,
    appended: u64,
    total_appended: u64,
    live: u64,
}

impl Spool {
    fn new(
        path: PathBuf,
        store_id: [u8; 32],
        owner_id: [u8; 32],
        session_id: [u8; 16],
    ) -> Result<Self, MountedError> {
        let mut marker = [0_u8; SPOOL_MARKER_BYTES as usize];
        marker[..8].copy_from_slice(SPOOL_MAGIC);
        marker[8..40].copy_from_slice(&store_id);
        marker[40..72].copy_from_slice(&owner_id);
        marker[72..88].copy_from_slice(&session_id);
        let compact = Self::compaction_path(&path);
        if compact.exists() {
            let mut prior = OpenOptions::new().read(true).open(&compact)?;
            let mut actual = [0_u8; SPOOL_MARKER_BYTES as usize];
            prior.read_exact(&mut actual)?;
            if actual != marker {
                return Err(MountedError::Corrupt);
            }
            return Err(MountedError::Busy);
        }
        if path.exists() {
            let mut prior = OpenOptions::new().read(true).open(&path)?;
            let mut actual = [0_u8; SPOOL_MARKER_BYTES as usize];
            prior.read_exact(&mut actual)?;
            if actual != marker {
                return Err(MountedError::Corrupt);
            }
            return Err(MountedError::Busy);
        }
        Ok(Self {
            path,
            marker,
            file: None,
            appended: 0,
            total_appended: 0,
            live: 0,
        })
    }

    fn next_offset(&self, bytes: usize) -> Result<u64, MountedError> {
        let bytes = u64::try_from(bytes).map_err(|_| MountedError::NoSpace)?;
        if self
            .appended
            .checked_add(bytes)
            .is_none_or(|value| value > SPOOL_QUOTA_BYTES)
        {
            return Err(MountedError::NoSpace);
        }
        Ok(SPOOL_MARKER_BYTES + self.appended)
    }

    fn append(&mut self, bytes: &[u8]) -> Result<u64, MountedError> {
        let offset = self.next_offset(bytes.len())?;
        if self.file.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&self.path)?;
            file.write_all(&self.marker)?;
            self.file = Some(file);
        }
        let file = self.file.as_mut().ok_or(MountedError::Indeterminate)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        let length = u64::try_from(bytes.len()).map_err(|_| MountedError::NoSpace)?;
        self.appended = self
            .appended
            .checked_add(length)
            .ok_or(MountedError::Indeterminate)?;
        self.total_appended = self
            .total_appended
            .checked_add(length)
            .ok_or(MountedError::Indeterminate)?;
        Ok(offset)
    }

    fn read_exact_at(&mut self, offset: u64, output: &mut [u8]) -> Result<(), MountedError> {
        let file = self.file.as_mut().ok_or(MountedError::Corrupt)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(output)?;
        Ok(())
    }

    fn slice(&mut self, offset: u64, len: u64) -> Result<SpoolSlice<'_>, MountedError> {
        let file = self.file.as_mut().ok_or(MountedError::Corrupt)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(SpoolSlice {
            file,
            remaining: len,
        })
    }

    fn reset(&mut self) -> Result<bool, MountedError> {
        self.file.take();
        let existed = self.path.exists();
        if existed {
            std::fs::remove_file(&self.path)?;
        }
        self.appended = 0;
        self.live = 0;
        Ok(existed)
    }

    fn physical(&self) -> u64 {
        self.appended
    }

    fn compaction_path(path: &Path) -> PathBuf {
        let mut value = path.as_os_str().to_os_string();
        value.push(".compact");
        PathBuf::from(value)
    }
}

struct SpoolSlice<'a> {
    file: &'a mut File,
    remaining: u64,
}

impl Read for SpoolSlice<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let allowed = output.len().min(self.remaining as usize);
        let read = self.file.read(&mut output[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}
