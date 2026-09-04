//! Fixed-record private Commit sorting. No namespace-sized allocation or worker.
use crate::{CoreError, CoreResult as Result};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SORT_BYTES: u64 = 256 * 1024;

// File API attempts and successfully completed bytes, not kernel syscall counts.
// Collection starts at Commit entry so ordinary mutation I/O is excluded.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunMetrics {
    pub sequential_read_calls: u64,
    pub sequential_read_bytes: u64,
    pub positional_read_calls: u64,
    pub positional_read_bytes: u64,
    pub write_calls: u64,
    pub write_bytes: u64,
    pub merge_passes: u64,
}
thread_local! {
    static METRICS: std::cell::Cell<Option<RunMetrics>> = const { std::cell::Cell::new(None) };
}
pub fn reset_metrics() {
    METRICS.with(|m| m.set(Some(RunMetrics::default())));
}
pub fn take_metrics() -> Option<RunMetrics> {
    METRICS.with(|m| m.take())
}
fn note(update: impl FnOnce(&mut RunMetrics)) {
    METRICS.with(|cell| {
        if let Some(mut metrics) = cell.get() {
            update(&mut metrics);
            cell.set(Some(metrics));
        }
    });
}

struct CountedFile<'a>(&'a mut File);
impl Read for CountedFile<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        note(|m| m.sequential_read_calls += 1);
        let n = self.0.read(bytes)?;
        note(|m| m.sequential_read_bytes += n as u64);
        Ok(n)
    }
}
impl Write for CountedFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        note(|m| m.write_calls += 1);
        let n = self.0.write(bytes)?;
        note(|m| m.write_bytes += n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> { self.0.flush() }
}
fn read_record<const N: usize>(reader: &mut impl Read) -> Result<Option<[u8; N]>> {
    let mut record = [0; N];
    if reader.read(&mut record[..1]).map_err(|_| CoreError::Io)? == 0 { return Ok(None); }
    reader.read_exact(&mut record[1..]).map_err(|_| CoreError::Io)?;
    Ok(Some(record))
}

pub struct Run<const N: usize> {
    file: File,
    pub count: u64,
}
impl<const N: usize> Run<N> {
    pub fn create(dir: &Path) -> Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = dir.join(format!(
            "commit-run-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|_| CoreError::Io)?;
        // Keep only the descriptor; failed later construction cannot orphan a
        // named run. A failed unlink is reported before any record is written.
        std::fs::remove_file(&path).map_err(|_| CoreError::Io)?;
        Ok(Self { file, count: 0 })
    }
    pub fn push(&mut self, record: &[u8; N]) -> Result<()> {
        note(|m| m.write_calls += 1);
        self.file.write_all(record).map_err(|_| CoreError::Io)?;
        note(|m| m.write_bytes += N as u64);
        self.count += 1;
        Ok(())
    }
    pub fn truncate(&mut self, count: u64) -> Result<()> {
        self.file
            .set_len(count * N as u64)
            .map_err(|_| CoreError::Io)?;
        self.file
            .seek(SeekFrom::End(0))
            .map_err(|_| CoreError::Io)?;
        self.count = count;
        Ok(())
    }
    pub fn rewind(&mut self) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|_| CoreError::Io)?;
        Ok(())
    }
    pub fn next(&mut self) -> Result<Option<[u8; N]>> {
        let mut record = [0; N];
        note(|m| m.sequential_read_calls += 1);
        let read = self
            .file
            .read(&mut record[..1])
            .map_err(|_| CoreError::Io)?;
        note(|m| m.sequential_read_bytes += read as u64);
        if read == 0 {
            return Ok(None);
        }
        note(|m| m.sequential_read_calls += 1);
        self.file
            .read_exact(&mut record[1..])
            .map_err(|_| CoreError::Io)?;
        note(|m| m.sequential_read_bytes += (N - 1) as u64);
        Ok(Some(record))
    }
    pub fn at(&self, index: u64) -> Result<[u8; N]> {
        use std::os::unix::fs::FileExt;
        let mut record = [0; N];
        note(|m| m.positional_read_calls += 1);
        self.file
            .read_exact_at(&mut record, index * N as u64)
            .map_err(|_| CoreError::Io)?;
        note(|m| m.positional_read_bytes += N as u64);
        Ok(record)
    }
    pub fn remove(self) -> Result<()> {
        drop(self);
        Ok(())
    }
}

/// At most 32 run descriptors, one fixed buffer and three record cursors.
/// Output plus all inputs is <= twice the raw bytes. Caller reserves that
/// simultaneous disk capacity before append; failures keep original mutation facts.
pub struct Sorter<const N: usize> {
    dir: PathBuf,
    records: Vec<[u8; N]>,
    levels: [Option<Run<N>>; 32],
    count: u64,
    max_bytes: u64,
    buffer_bytes: usize,
    record_limit: usize,
}
impl<const N: usize> Sorter<N> {
    pub fn new(dir: &Path, max_bytes: u64, memory_bytes: u64) -> Result<Self> {
        let memory_bytes = memory_bytes.min(SORT_BYTES);
        let buffer_bytes = (memory_bytes / 16).min(8192) as usize;
        let capacity = ((memory_bytes - 3 * buffer_bytes as u64) / N as u64) as usize;
        if capacity == 0 {
            return Err(CoreError::InvalidRecord("workspace final-delta limit"));
        }
        let records = Vec::new();
        Ok(Self {
            dir: dir.to_owned(),
            records,
            levels: std::array::from_fn(|_| None),
            count: 0,
            max_bytes,
            buffer_bytes,
            record_limit: capacity,
        })
    }
    pub fn capacity(&self) -> u64 {
        if self.records.capacity() == 0 { 0 } else { (self.records.capacity() * N + 3 * self.buffer_bytes) as u64 }
    }
    pub fn push(&mut self, record: [u8; N]) -> Result<()> {
        if (self.count + 1)
            .checked_mul(2 * N as u64)
            .is_none_or(|n| n > self.max_bytes)
        {
            return Err(CoreError::InvalidRecord("workspace spool limit"));
        }
        if self.records.capacity() == 0 {
            self.records.try_reserve_exact(self.record_limit).map_err(|_| CoreError::InvalidRecord("workspace final-delta allocation"))?;
        }
        if self.records.len() == self.records.capacity() { self.flush()?; }
        self.records.push(record);
        self.count += 1;
        Ok(())
    }
    fn merge(dir: &Path, mut a: Run<N>, mut b: Run<N>, buffer: usize) -> Result<Run<N>> {
        note(|m| m.merge_passes += 1);
        a.rewind()?; b.rewind()?;
        let mut out = Run::create(dir)?;
        out.count = a.count.checked_add(b.count).ok_or(CoreError::LengthOverflow)?;
        {
            let mut a = BufReader::with_capacity(buffer, CountedFile(&mut a.file));
            let mut b = BufReader::with_capacity(buffer, CountedFile(&mut b.file));
            let mut output = BufWriter::with_capacity(buffer, CountedFile(&mut out.file));
            let mut left = read_record::<N>(&mut a)?;
            let mut right = read_record::<N>(&mut b)?;
            while left.is_some() || right.is_some() {
                if right.is_none() || left.as_ref().zip(right.as_ref()).is_some_and(|(a,b)| a <= b) {
                    output.write_all(&left.take().unwrap()).map_err(|_| CoreError::Io)?;
                    left = read_record(&mut a)?;
                } else {
                    output.write_all(&right.take().unwrap()).map_err(|_| CoreError::Io)?;
                    right = read_record(&mut b)?;
                }
            }
            output.flush().map_err(|_| CoreError::Io)?;
        }
        a.remove()?; b.remove()?;
        Ok(out)
    }
    fn flush(&mut self) -> Result<()> {
        if self.records.is_empty() {
            return Ok(());
        }
        self.records.sort_unstable();
        let mut run = Run::create(&self.dir)?;
        {
            let mut output = BufWriter::with_capacity(self.buffer_bytes, CountedFile(&mut run.file));
            for record in &self.records { output.write_all(record).map_err(|_| CoreError::Io)?; }
            output.flush().map_err(|_| CoreError::Io)?;
        }
        run.count = self.records.len() as u64;
        self.records.clear();
        for level in &mut self.levels {
            match level.take() {
                None => {
                    *level = Some(run);
                    return Ok(());
                }
                Some(before) => run = Self::merge(&self.dir, before, run, self.buffer_bytes)?,
            }
        }
        Err(CoreError::InvalidRecord("workspace sort run limit"))
    }
    pub fn finish(mut self) -> Result<Run<N>> {
        self.flush()?;
        let mut result = None;
        for level in &mut self.levels {
            if let Some(run) = level.take() {
                result = Some(match result {
                    None => run,
                    Some(previous) => Self::merge(&self.dir, previous, run, self.buffer_bytes)?,
                });
            }
        }
        let mut result = match result {
            Some(run) => run,
            None => Run::create(&self.dir)?,
        };
        result.rewind()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_sort_merges_multiple_levels_and_cleans_all_runs() {
        let dir = std::env::temp_dir().join(format!("layerfs-commit-sort-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        reset_metrics();
        let mut sorter = Sorter::<8>::new(&dir, 4096, 32).unwrap();
        for i in (0..100u64).rev() {
            sorter.push(i.to_be_bytes()).unwrap();
        }
        let mut run = sorter.finish().unwrap();
        assert_eq!(run.at(0).unwrap(), 0u64.to_be_bytes());
        for i in 0..100u64 {
            assert_eq!(run.next().unwrap(), Some(i.to_be_bytes()));
        }
        assert_eq!(run.next().unwrap(), None);
        run.remove().unwrap();
        let metrics = take_metrics().unwrap();
        assert!(
            metrics.merge_passes > 0
                && metrics.write_calls > 100
                && metrics.sequential_read_calls > 200
        );
        assert_eq!(
            (metrics.positional_read_calls, metrics.positional_read_bytes),
            (1, 8)
        );
        assert!(take_metrics().is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }
}
