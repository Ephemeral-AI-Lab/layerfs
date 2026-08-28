use super::*;

impl NamePreflight for Preflight {
    fn add(&mut self, name: &[u8]) -> Result<()> {
        let started = Instant::now();
        let result = super::ffi::create_regular_at(&self.directory, name);
        self.wall_ns = self.wall_ns.saturating_add(elapsed_ns(started));
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if !self.observed {
                    self.facts.update(|facts| {
                        finish_call(&mut facts.name_preflight, self.wall_ns, false)
                    });
                    self.observed = true;
                }
                Err(error.into())
            }
        }
    }
    fn finish(mut self: Box<Self>) -> Result<()> {
        let cleanup_start = Instant::now();
        let result = super::ffi::remove_owned_tree(
            &self.directory,
            &self.staging,
            &self.name,
            &self.identity,
        );
        finish_cleanup(&self.facts, cleanup_start, result.is_ok());
        if !self.observed {
            self.facts.update(|facts| {
                finish_call(&mut facts.name_preflight, self.wall_ns, result.is_ok())
            });
            self.observed = true;
        }
        if result.is_ok() {
            self.active = false;
        }
        result.map_err(Into::into)
    }
}

impl Drop for Preflight {
    fn drop(&mut self) {
        if !self.observed {
            self.facts
                .update(|facts| finish_call(&mut facts.name_preflight, self.wall_ns, false));
            self.observed = true;
        }
        if self.active {
            let start = Instant::now();
            let removed = super::ffi::remove_owned_tree(
                &self.directory,
                &self.staging,
                &self.name,
                &self.identity,
            );
            finish_cleanup(&self.facts, start, removed.is_ok());
        }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let start = Instant::now();
        let removed = super::ffi::unlink_if_identity_at(&self.staging, &self.name, &self.identity);
        finish_cleanup(&self.facts, start, removed.is_ok());
    }
}

impl DirectoryHandle for Dir {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Read for Regular {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(bytes)
    }
}
impl Write for Regular {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        let result = self.0.write(bytes);
        let elapsed = elapsed_ns(start);
        let written = result.as_ref().ok().copied();
        self.1.update(|facts| {
            finish_write(&mut facts.content_write, elapsed, written);
            finish_write(&mut facts.aggregate_native_write, elapsed, written);
        });
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let start = Instant::now();
        let result = self.0.flush();
        let elapsed = elapsed_ns(start);
        self.1
            .update(|facts| finish_call(&mut facts.content_flush, elapsed, result.is_ok()));
        result
    }
}
impl Seek for Regular {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(position)
    }
}
impl RegularFileHandle for Regular {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Read for Temp {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl Write for Temp {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        let result = self.file.write(bytes);
        let elapsed = elapsed_ns(start);
        let written = result.as_ref().ok().copied();
        self.facts.update(|facts| {
            finish_write(&mut facts.content_write, elapsed, written);
            finish_write(&mut facts.aggregate_native_write, elapsed, written);
        });
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let start = Instant::now();
        let result = self.file.flush();
        let elapsed = elapsed_ns(start);
        self.facts
            .update(|facts| finish_call(&mut facts.content_flush, elapsed, result.is_ok()));
        result
    }
}
impl Seek for Temp {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}
impl OwnedTempHandle for Temp {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn set_len(&mut self, len: u64) -> Result<()> {
        self.file.set_len(len).map_err(Into::into)
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
