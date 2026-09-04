//! Workspace error boundary for shared anonymous fixed-record sorting.
use layerfs_content::filesystem::delta_spool as shared;
use layerfs_layerstack_store::{Result, StoreError};
pub(crate) use shared::{reset_metrics, take_metrics, SORT_BYTES};
use std::path::Path;
fn error(error: shared::SpoolError) -> StoreError {
    match error {
        shared::SpoolError::Io(error) => StoreError::Io(error),
        shared::SpoolError::Core(layerfs_content::CoreError::InvalidRecord(message)) => {
            StoreError::InvalidInput(message)
        }
        shared::SpoolError::Core(error) => error.into(),
    }
}
pub(crate) struct Run<const N: usize>(shared::Run<N>);
impl<const N: usize> std::ops::Deref for Run<N> {
    type Target = shared::Run<N>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<const N: usize> Run<N> {
    pub(crate) fn create(dir: &Path) -> Result<Self> {
        shared::Run::create(dir).map(Self).map_err(error)
    }
    pub(crate) fn push(&mut self, record: &[u8; N]) -> Result<()> {
        self.0.push(record).map_err(error)
    }
    pub(crate) fn truncate(&mut self, count: u64) -> Result<()> {
        self.0.truncate(count).map_err(error)
    }
    pub(crate) fn rewind(&mut self) -> Result<()> {
        self.0.rewind().map_err(error)
    }
    pub(crate) fn next(&mut self) -> Result<Option<[u8; N]>> {
        self.0.next().map_err(error)
    }
    pub(crate) fn at(&self, index: u64) -> Result<[u8; N]> {
        self.0.at(index).map_err(error)
    }
    pub(crate) fn remove(self) -> Result<()> {
        self.0.remove().map_err(error)
    }
}
pub(crate) struct Sorter<const N: usize>(shared::Sorter<N>);
impl<const N: usize> Sorter<N> {
    pub(crate) fn new(dir: &Path, disk: u64, memory: u64) -> Result<Self> {
        shared::Sorter::new(dir, disk, memory)
            .map(Self)
            .map_err(error)
    }
    pub(crate) fn capacity(&self) -> u64 {
        self.0.capacity()
    }
    pub(crate) fn push(&mut self, record: [u8; N]) -> Result<()> {
        self.0.push(record).map_err(error)
    }
    pub(crate) fn finish(self) -> Result<Run<N>> {
        self.0.finish().map(Run).map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_spool_preserves_workspace_os_errors_and_content_errors() {
        // Actual open failure crosses shared storage and the Workspace wrapper.
        let absent = std::env::temp_dir()
            .join(format!("layerfs-absent-spool-{}", std::process::id()))
            .join("missing-parent");
        let failure = match Run::<8>::create(&absent) {
            Ok(_) => panic!("missing directory must fail"),
            Err(error) => error,
        };
        let StoreError::Io(error) = failure else {
            panic!("OS error was erased at shared spool boundary");
        };
        assert_eq!(error.raw_os_error(), Some(2)); // ENOENT on supported Unix hosts.
        let StoreError::Io(error) = super::error(shared::SpoolError::Io(
            std::io::Error::from_raw_os_error(28),
        )) else {
            panic!("ENOSPC must retain its OS error");
        };
        assert_eq!(error.raw_os_error(), Some(28));
        assert!(matches!(
            super::error(shared::SpoolError::Core(
                layerfs_content::CoreError::InvalidRecord("workspace spool limit")
            )),
            StoreError::InvalidInput("workspace spool limit")
        ));
        assert!(matches!(
            layerfs_content::CoreError::from(shared::SpoolError::Io(
                std::io::Error::from_raw_os_error(28)
            )),
            layerfs_content::CoreError::Io
        ));
    }
}
