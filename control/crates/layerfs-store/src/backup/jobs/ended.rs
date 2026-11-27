//! How a backup or a restore recorded as its Store's `op` ends, through the
//! Store's one operation helper ([`crate::intent`]): a transfer that worked
//! drops `op` and sets its link in one record write; a certain failure
//! drops `op` and writes what was tried to the link's `attempt`
//! ([`crate::get::ATTEMPT`]), which `store.get`'s `backups` section reads;
//! an `Unknown` one leaves `op` unresolved for `store get --with live` to
//! settle.
use super::super::bucket::record::StoreLink;
use crate::admin::record::intent;
use crate::admin::record::store::{self, StoreRecord};
use layerfs_home::op::{self, Op};
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Under Store `store`'s lock, applies `change` with `op` dropped, while
/// the record still holds `op`.
fn without_op(
    home: &Path,
    store: &str,
    op: &Op,
    change: impl FnOnce(StoreRecord) -> StoreRecord,
) -> Result<()> {
    let _store = store::lock(home, store)?;
    let found = StoreRecord::of(home, store)?;
    if found.op().is_none_or(|held| held.id() != op.id()) {
        return Ok(());
    }
    change(found.without(op::FIELD)).write(home)
}

/// The transfer `op` worked: `op` dropped and the link to `bucket` set to
/// `link`, in one record write.
pub fn finished(home: &Path, store: &str, op: &Op, bucket: &str, link: &StoreLink) -> Result<()> {
    without_op(home, store, op, |found| found.with_link(bucket, link))
}

/// The transfer `op` (`call` of `file` through `bucket`) failed with
/// `refusal`. A record that cannot be written changes no outcome.
pub fn failed(
    home: &Path,
    store: &str,
    op: &Op,
    (bucket, file, call): (&str, &str, &str),
    refusal: &SdkRefusal,
) {
    if refusal.code == "Unknown" {
        intent::leave(home, store, op, &refusal.message);
        return;
    }
    let _ = without_op(home, store, op, |found| {
        let link = StoreLink::with_attempt(found.link(bucket), file, call, &refusal.message);
        found.with_link(bucket, &link)
    });
}
