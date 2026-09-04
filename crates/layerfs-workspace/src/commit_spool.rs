//! Fixed-record private Commit sorting. No namespace-sized allocation or worker.
use layerfs_layerstack_store::{Result, StoreError};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub(crate) const SORT_BYTES: u64 = 256 * 1024;

// File API attempts and successfully completed bytes, not kernel syscall counts.
// Collection starts at Commit entry so ordinary mutation I/O is excluded.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunMetrics {
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
pub(crate) fn reset_metrics() {
    METRICS.with(|m| m.set(Some(RunMetrics::default())));
}
pub(crate) fn take_metrics() -> Option<RunMetrics> {
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

pub(crate) struct Run<const N: usize> {
    file: File,
    pub(crate) count: u64,
}
impl<const N: usize> Run<N> {
    pub(crate) fn create(dir: &Path) -> Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = dir.join(format!(
            "commit-run-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)?;
        // Keep only the descriptor; failed later construction cannot orphan a
        // named run. A failed unlink is reported before any record is written.
        std::fs::remove_file(&path)?;
        Ok(Self { file, count: 0 })
    }
    pub(crate) fn push(&mut self, record: &[u8; N]) -> Result<()> {
        note(|m| m.write_calls += 1);
        self.file.write_all(record)?;
        note(|m| m.write_bytes += N as u64);
        self.count += 1;
        Ok(())
    }
    pub(crate) fn truncate(&mut self, count: u64) -> Result<()> {
        self.file.set_len(count * N as u64)?;
        self.file.seek(SeekFrom::End(0))?;
        self.count = count;
        Ok(())
    }
    pub(crate) fn rewind(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }
    pub(crate) fn next(&mut self) -> Result<Option<[u8; N]>> {
        let mut record = [0; N];
        note(|m| m.sequential_read_calls += 1);
        let read = self.file.read(&mut record[..1])?;
        note(|m| m.sequential_read_bytes += read as u64);
        if read == 0 {
            return Ok(None);
        }
        note(|m| m.sequential_read_calls += 1);
        self.file.read_exact(&mut record[1..])?;
        note(|m| m.sequential_read_bytes += (N - 1) as u64);
        Ok(Some(record))
    }
    pub(crate) fn at(&self, index: u64) -> Result<[u8; N]> {
        use std::os::unix::fs::FileExt;
        let mut record = [0; N];
        note(|m| m.positional_read_calls += 1);
        self.file.read_exact_at(&mut record, index * N as u64)?;
        note(|m| m.positional_read_bytes += N as u64);
        Ok(record)
    }
    pub(crate) fn remove(self) -> Result<()> {
        drop(self);
        Ok(())
    }
}

/// At most 32 run descriptors, one fixed buffer and three record cursors.
/// Output plus all inputs is <= twice the raw bytes. Caller reserves that
/// simultaneous disk capacity before append; failures keep original mutation facts.
pub(crate) struct Sorter<const N: usize> {
    dir: PathBuf,
    records: Vec<[u8; N]>,
    levels: [Option<Run<N>>; 32],
    count: u64,
    max_bytes: u64,
}
impl<const N: usize> Sorter<N> {
    pub(crate) fn new(dir: &Path, max_bytes: u64, memory_bytes: u64) -> Result<Self> {
        let capacity = (memory_bytes.min(SORT_BYTES) / N as u64) as usize;
        if capacity == 0 {
            return Err(StoreError::InvalidInput("workspace final-delta limit"));
        }
        let mut records = Vec::new();
        records
            .try_reserve_exact(capacity)
            .map_err(|_| StoreError::InvalidInput("workspace final-delta allocation"))?;
        Ok(Self {
            dir: dir.to_owned(),
            records,
            levels: std::array::from_fn(|_| None),
            count: 0,
            max_bytes,
        })
    }
    pub(crate) fn capacity(&self) -> u64 {
        (self.records.capacity() * N) as u64
    }
    pub(crate) fn push(&mut self, record: [u8; N]) -> Result<()> {
        if (self.count + 1)
            .checked_mul(2 * N as u64)
            .is_none_or(|n| n > self.max_bytes)
        {
            return Err(StoreError::InvalidInput("workspace spool limit"));
        }
        if self.records.len() == self.records.capacity() {
            self.flush()?;
        }
        self.records.push(record);
        self.count += 1;
        Ok(())
    }
    fn merge(dir: &Path, mut a: Run<N>, mut b: Run<N>) -> Result<Run<N>> {
        note(|m| m.merge_passes += 1);
        a.rewind()?;
        b.rewind()?;
        let mut out = Run::create(dir)?;
        let mut left = a.next()?;
        let mut right = b.next()?;
        while left.is_some() || right.is_some() {
            if right.is_none()
                || left
                    .as_ref()
                    .zip(right.as_ref())
                    .is_some_and(|(a, b)| a <= b)
            {
                out.push(&left.take().unwrap())?;
                left = a.next()?;
            } else {
                out.push(&right.take().unwrap())?;
                right = b.next()?;
            }
        }
        a.remove()?;
        b.remove()?;
        Ok(out)
    }
    fn flush(&mut self) -> Result<()> {
        if self.records.is_empty() {
            return Ok(());
        }
        self.records.sort_unstable();
        let mut run = Run::create(&self.dir)?;
        for record in &self.records {
            run.push(record)?;
        }
        self.records.clear();
        for level in &mut self.levels {
            match level.take() {
                None => {
                    *level = Some(run);
                    return Ok(());
                }
                Some(before) => run = Self::merge(&self.dir, before, run)?,
            }
        }
        Err(StoreError::InvalidInput("workspace sort run limit"))
    }
    pub(crate) fn finish(mut self) -> Result<Run<N>> {
        self.flush()?;
        let mut result = None;
        for level in &mut self.levels {
            if let Some(run) = level.take() {
                result = Some(match result {
                    None => run,
                    Some(previous) => Self::merge(&self.dir, previous, run)?,
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
