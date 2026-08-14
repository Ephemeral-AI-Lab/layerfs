pub struct CountingSource<'a> {
    bytes: &'a [u8],
    offset: usize,
    reads: u64,
    bytes_read: u64,
}

impl<'a> CountingSource<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            reads: 0,
            bytes_read: 0,
        }
    }

    pub fn read(&mut self, destination: &mut [u8]) -> usize {
        self.reads += 1;
        let amount = destination
            .len()
            .min(self.bytes.len().saturating_sub(self.offset));
        destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
        self.offset += amount;
        self.bytes_read += amount as u64;
        amount
    }

    pub fn reads(&self) -> u64 {
        self.reads
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

impl std::io::Read for CountingSource<'_> {
    fn read(&mut self, destination: &mut [u8]) -> std::io::Result<usize> {
        Ok(CountingSource::read(self, destination))
    }
}
