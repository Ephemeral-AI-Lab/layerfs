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
//! The change log exists only in an export base. A Store that was never marked
//! has the schema of a build without this module, logs nothing and pays
//! nothing on its writes. [`mark_base`] adds the `change_state` and
//! `change_log` tables and their triggers and records the schema with them;
//! [`unmark_base`] drops them again. While a Store is a base, every write
//! appends to its log; [`trim_changes`] deletes the entries every followed
//! replica has applied. A copy of a base, such as a backup or a Store restored
//! from one, is itself a base and logs every write until it is unmarked.
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
//! A bundle records the identity of its source Store's schema: its version and
//! every table, index and trigger definition. A replica whose schema differs,
//! for example after a migration on one side only, refuses the bundle with
//! [`ChangeError::Schema`] before it installs a row.
//!
//! Every call that writes a Store first takes the Store's writer lane, the lane
//! each of its writers takes, in one non-blocking attempt: while another writer
//! holds it, the call returns [`ChangeError::Busy`] and changes nothing. An
//! export reads one snapshot and takes no lane.
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
mod log;
mod lookup;
mod records;
mod tables;

pub use apply::apply_changes;
pub use base::{mark_base, read_state, trim_changes, unmark_base};
pub use error::ChangeError;
pub use export::export_changes;
pub use records::{ApplyReceipt, ChangeCursor, ExportReceipt};
pub use tables::BUNDLE_FORMAT;
