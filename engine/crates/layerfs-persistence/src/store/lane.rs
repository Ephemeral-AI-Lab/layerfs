//! The Store directory's writer lane, held for a coordinated close.
use std::path::Path;

/// Held for a coordinated close: the connection teardown's close-time
/// checkpoint runs inside it, never beside another opener's first attach.
/// Best-effort - a failed acquisition closes bare, as an uncoordinated drop
/// always has.
pub struct StoreLane(Option<crate::backend::sqlite::writer_admission::Writer>);

impl Drop for StoreLane {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take() {
            let _ = writer.release();
        }
    }
}

/// Holds the writer lane of the Store directory holding `store`.
pub fn store_lane(store: &Path) -> StoreLane {
    StoreLane(crate::backend::sqlite::writer_admission::Writer::acquire(store).ok())
}
