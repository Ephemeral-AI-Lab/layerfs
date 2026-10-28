//! Incremental change export between replicas of one Store.
//!
//! A Store marked as an export base logs the primary key of every row its
//! writers insert, update or delete in the logged tables. Schema triggers fill
//! `change_log`, so every opener's writes are logged without a code path of
//! its own, in the single writer's commit order. A Store that was never marked
//! logs nothing.
//!
//! [`export_changes`] reads one snapshot of a live Store and writes every row
//! changed after a cursor, at its current value, into a new bundle file: a
//! SQLite file with the logged tables' exact definitions, the deleted keys and
//! the bundle's two cursors. [`apply_changes`] installs a bundle into an
//! offline replica at the bundle's starting cursor, in one transaction.
//! [`mark_base`] makes an offline Store (normally a sealed file) the start of a
//! chain.
//!
//! A cursor names one replica state: a 32-byte state identity and the change
//! sequence it reaches. Each bundle's ending state derives from its starting
//! state and the digest of its contents, so two replicas that diverge from one
//! state never share a later state, and a bundle applies only to the state it
//! was exported from. The digest and states detect accidental damage and
//! misordering; they are not a signature, so a caller that moves bundles over
//! an untrusted path authenticates them itself.
//!
//! An export writes the bundle under a temporary name beside the bundle path
//! and links it into place, so the path names a complete bundle or nothing,
//! also after a process crash. Like the Store under the Disposable profile, a
//! bundle claims process-crash survival only.
//!
//! [`mark_base`] and [`apply_changes`] checkpoint the Store after they commit,
//! so that the Store file alone holds the change. When another connection's
//! read blocks that checkpoint, they return [`ChangeError::NotCheckpointed`]:
//! the change committed, and the file holds it only together with its
//! write-ahead log.
mod apply;
mod base;
mod connection;
mod digest;
mod error;
mod export;
mod lookup;
mod records;
mod tables;

pub use apply::apply_changes;
pub use base::{mark_base, read_state};
pub use error::ChangeError;
pub use export::export_changes;
pub use records::{ApplyReceipt, ChangeCursor, ExportReceipt};
pub use tables::BUNDLE_FORMAT;
