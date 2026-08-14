pub struct CountingSink {
    bytes: Vec<u8>,
    maximum_bytes: usize,
    writes: u64,
    file_active: bool,
    active: bool,
    finished: bool,
    aborted: bool,
}

impl CountingSink {
    pub fn new(maximum_bytes: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(maximum_bytes),
            maximum_bytes,
            ..Self::default()
        }
    }

    pub fn writes(&self) -> u64 {
        self.writes
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl Default for CountingSink {
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            maximum_bytes: 32 * 1024 * 1024,
            writes: 0,
            file_active: false,
            active: false,
            finished: false,
            aborted: false,
        }
    }
}

impl CountingSink {
    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn aborted(&self) -> bool {
        self.aborted
    }
}

impl CountingSink {
    pub fn begin(&mut self) {
        self.bytes.clear();
        self.writes = 0;
        self.file_active = false;
        self.active = true;
        self.finished = false;
        self.aborted = false;
    }

    pub fn write(&mut self, bytes: &[u8]) -> bool {
        if !self.active {
            return false;
        }
        self.file_active = true;
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .map_or(true, |length| length > self.maximum_bytes)
        {
            return false;
        }
        self.writes += 1;
        self.bytes.extend_from_slice(bytes);
        true
    }

    pub fn finish_file(&mut self) -> bool {
        if !self.file_active {
            return false;
        }
        self.file_active = false;
        true
    }

    pub fn finish(&mut self) -> bool {
        if !self.active || self.file_active {
            return false;
        }
        self.active = false;
        self.finished = true;
        true
    }

    pub fn abort(&mut self) {
        self.active = false;
        self.file_active = false;
        self.bytes.clear();
        self.aborted = true;
    }
}

impl std::io::Write for CountingSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        CountingSink::write(self, bytes)
            .then_some(bytes.len())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::WriteZero))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
