//! Init's rolling four-worker acknowledgement window over sealed bodies.
use super::state::State;
use crate::{location::PackDomain, StorageError, StorageResult};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
impl State<'_> {
    pub(super) fn upload_ready(&self) -> StorageResult<()> {
        let _work = self.storage.work.span(super::Stage::Upload);
        let packs: Vec<_> = self
            .packer
            .ready
            .iter()
            .filter(|pack| pack.info.domain == PackDomain::Payload)
            .map(|pack| (pack.info.key, pack.body.as_slice()))
            .collect();
        let next = AtomicUsize::new(0);
        let stopped = AtomicBool::new(false);
        let attempts = AtomicU64::new(0);
        let bytes = AtomicU64::new(0);
        let objects = &self.storage.source.objects;
        let result = std::thread::scope(|scope| {
            let mut jobs = Vec::with_capacity(packs.len().min(4));
            let mut failure = None;
            for _ in 0..packs.len().min(4) {
                let packs = &packs;
                let next = &next;
                let stopped = &stopped;
                let attempts = &attempts;
                let bytes = &bytes;
                let job = std::thread::Builder::new()
                    .name("layerfs-init-upload".to_owned())
                    .spawn_scoped(scope, move || -> StorageResult<()> {
                        while !stopped.load(Ordering::Acquire) {
                            let Some(&(key, body)) =
                                packs.get(next.fetch_add(1, Ordering::Relaxed))
                            else {
                                break;
                            };
                            attempts.fetch_add(1, Ordering::Relaxed);
                            bytes.fetch_add(body.len() as u64, Ordering::Relaxed);
                            if let Err(error) = objects.put_if_absent(key, body) {
                                stopped.store(true, Ordering::Release);
                                return Err(StorageError::from(error));
                            }
                        }
                        Ok(())
                    });
                match job {
                    Ok(job) => jobs.push(job),
                    Err(error) => {
                        stopped.store(true, Ordering::Release);
                        failure = Some(StorageError::Io(error));
                        break;
                    }
                }
            }
            // Every admitted request joins, even after one definite/unknown error.
            // No parent metadata is registered until the entire window succeeds.
            for job in jobs {
                let result = job.join().unwrap_or_else(|_| {
                    stopped.store(true, Ordering::Release);
                    Err(StorageError::Integrity("upload worker panicked"))
                });
                if let Err(error) = result {
                    failure.get_or_insert(error);
                }
            }
            failure.map_or(Ok(()), Err)
        });
        self.storage.source.note(|counts| {
            counts.puts += attempts.load(Ordering::Relaxed);
            counts.put_bytes += bytes.load(Ordering::Relaxed);
        });
        result
    }
}
