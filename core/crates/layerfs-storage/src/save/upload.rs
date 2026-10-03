//! Init's bounded parallel acknowledgement window over already sealed bodies.
use super::state::State;
use crate::{location::PackDomain, StorageError, StorageResult};
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
        for window in packs.chunks(4) {
            let results = std::thread::scope(|scope| {
                let mut jobs = Vec::with_capacity(window.len());
                for &(key, body) in window {
                    let objects = &self.storage.source.objects;
                    self.storage.source.note(|c| {
                        c.puts += 1;
                        c.put_bytes += body.len() as u64;
                    });
                    jobs.push(
                        std::thread::Builder::new()
                            .name("layerfs-init-upload".to_owned())
                            .spawn_scoped(scope, move || objects.put_if_absent(key, body))
                            .map_err(StorageError::Io)?,
                    );
                }
                jobs.into_iter()
                    .map(|job| {
                        job.join()
                            .map_err(|_| StorageError::Integrity("upload worker panicked"))?
                            .map(|_| ())
                            .map_err(StorageError::from)
                    })
                    .collect::<StorageResult<Vec<_>>>()
            });
            results?;
        }
        Ok(())
    }
}
