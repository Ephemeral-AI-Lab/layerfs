//! Store database failures in plain words, for a refusal's message: the
//! engine's own `Display` of a `PersistenceError` prints Rust `Debug` text.
//! It lives here, below the Store crate, so a backup's own failures use it
//! too; the Store crate's `words` module re-exports it.
use layerfs_storage::port::PersistenceError;

/// A Store database operation that failed: each outcome in words, a
/// refusal with the backend's own status text.
pub fn persistence(error: &PersistenceError) -> String {
    match error {
        PersistenceError::Busy => "the Store is held by another writer; nothing was changed".into(),
        PersistenceError::BackendUnavailable => {
            "the Store's database is not available to this build".into()
        }
        PersistenceError::Missing => "the Store holds no record of what was asked".into(),
        PersistenceError::Refused { status } => format!("the Store refused it: {status}"),
        PersistenceError::Malformed => "the Store answered something malformed".into(),
        PersistenceError::Uncertain => {
            "the Store's answer was lost; whether it changed is not known".into()
        }
    }
}
