//! The one operation a Store's record holds (`op`, #536 §7.1; call surface
//! 2.0): a move, a remove, a compaction, a backup or a restore, recorded so
//! every client reads the Store as `busy` while it runs and as `broken`,
//! `unresolved` once a gone owner left its outcome unknown.
//!
//! A move and a remove hold the Store to themselves and are written by
//! their own calls (`sqlite::family::begin`). The others record a marker
//! with the functions here: [`begin`] (refused while another operation
//! holds the Store) or [`mark`] (never refused: the Store's own admission
//! decides), then [`advance`] as the phases go, and [`clear`] when the
//! outcome is known or [`leave`] when it is not. A marker refuses no lease
//! and no Store call (`StoreRecord::unblocked`).
use crate::record::{self, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::with_field;
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::time;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A sqlite Store's move between this computer and a machine.
pub const MOVE: &str = "store.move";
/// A sqlite Store's removal.
pub const REMOVE: &str = "store.remove";
/// A sqlite Store made on a machine.
pub const CREATE: &str = "store.create";
/// A compaction.
pub const COMPACT: &str = "store.compact";
/// A backup to a bucket.
pub const BACKUP: &str = "backup.create";
/// A restore from a bucket over the Store.
pub const RESTORE: &str = "backup.restore";

/// Whether `name` is a move, `store.move`.
pub fn is_move(name: &str) -> bool {
    name == MOVE
}

/// Whether operation `name` holds the Store to itself, so no lease is
/// granted and no Store call runs beside it: every operation but a
/// compaction and a backup, which run beside the Store's readers under the
/// Store's own admission.
pub fn exclusive(name: &str) -> bool {
    !matches!(name, COMPACT | BACKUP)
}

/// The word a state's `op.name` shows for operation `name`: `move`,
/// `remove`, `compact`, `backup` or `restore` (`create` while a Store is
/// made on a machine); any other name by its last word.
pub fn word(name: &str) -> &str {
    match name {
        _ if is_move(name) => "move",
        BACKUP => "backup",
        RESTORE => "restore",
        other => other.rsplit('.').next().unwrap_or(other),
    }
}

/// `op` as a refusal's next step and a state's companion show it:
/// `{ name, phase, started_at }`.
pub fn shown(op: &Op) -> SdkValue {
    let started = op.to_record().get_int("started");
    SdkValue::map([
        ("name", SdkValue::text(word(op.name()))),
        ("phase", SdkValue::text(op.phase())),
        (
            "started_at",
            started.map_or(SdkValue::Null, |ms| SdkValue::Text(time::iso(ms))),
        ),
    ])
}

/// The call that clears Store `store`'s operation `op` once its owner is
/// gone and its outcome unknown, with its arguments and without `layerfs`:
/// the same `store move <store> <machine>` for a move, `store remove` for a
/// remove or a create, `store compact` for a compaction (the next run
/// adopts and settles what the earlier one left), and for anything else
/// the live read that settles it, `store get <store> --with live`.
pub fn clearing(op: &Op, store: &str) -> String {
    match (op.name(), op.args().get_text("to")) {
        (name, Some(to)) if is_move(name) => format!("store move {store} {to}"),
        (REMOVE | CREATE, _) => format!("store remove {store}"),
        (COMPACT, _) => format!("store compact {store}"),
        _ => format!("store get {store} --with live"),
    }
}

/// Whether a live read settles Store operation `name` once its owner is
/// gone: every operation but those a call of their own clears (a move, a
/// remove, a create and a compaction).
pub fn settled_by_read(name: &str) -> bool {
    !is_move(name) && !matches!(name, REMOVE | CREATE | COMPACT)
}

/// Records operation `name` of this process on Store `store`, at `phase`,
/// with `args` (what a state and the doctor need; `Null` for none), under
/// the Store's lock. Refused `Busy` while a live process runs an operation
/// on the Store and `Unresolved` while a gone one left another call's
/// outcome unknown; an unresolved operation of the same `name` is replaced
/// (the new run is the call that settles it). Answers the operation, for
/// [`advance`], [`clear`] and [`leave`].
pub fn begin(home: &Path, store: &str, name: &str, phase: &str, args: SdkValue) -> Result<Op> {
    let op = Op::begin(name, phase, args)?;
    let _store = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    let same = |held: &Op| held.name() == name && held.standing() == Standing::Unresolved;
    if !record.op().is_some_and(|held| same(&held)) {
        record.unheld()?;
    }
    record.with(op::FIELD, op.to_record().clone()).write(home)?;
    Ok(op)
}

/// [`begin`] that never refuses: the operation is recorded unless a live
/// process's operation is on the Store (none is answered then, and the
/// Store's own admission refuses what cannot run beside it). An operation
/// a gone process left is replaced.
pub fn mark(
    home: &Path,
    store: &str,
    name: &str,
    phase: &str,
    args: SdkValue,
) -> Result<Option<Op>> {
    let op = Op::begin(name, phase, args)?;
    let _store = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    if record.op().map(|held| held.standing()) == Some(Standing::Running) {
        return Ok(None);
    }
    record.with(op::FIELD, op.to_record().clone()).write(home)?;
    Ok(Some(op))
}

/// `op` moved to `phase`, in Store `store`'s record too while the record
/// still holds it.
pub fn advance(home: &Path, store: &str, op: &mut Op, phase: &str) -> Result<()> {
    *op = op.clone().at(phase);
    replace(home, store, op.id(), Some(op))
}

/// Drops `op` from Store `store`'s record: its outcome is known. Nothing
/// is written when the record holds another operation or none.
pub fn clear(home: &Path, store: &str, op: &Op) -> Result<()> {
    replace(home, store, op.id(), None)
}

/// Keeps `op` in Store `store`'s record with its outcome unknown and no
/// owner, so the Store reads `broken`, `unresolved` at once, in this
/// process too (the call returned; nothing runs), and logs it to
/// `log/uncertain.jsonl` with `message`. A record that cannot be written
/// changes no outcome.
pub fn leave(home: &Path, store: &str, op: &Op, message: &str) {
    let left = unowned(op.clone().as_uncertain());
    let _ = replace(home, store, op.id(), Some(&left));
    left.log_uncertain(home, "store", store, message);
}

/// `op` with no owner: no process answers for it any more.
fn unowned(op: Op) -> Op {
    let fields = with_field(op.to_record().clone(), "owner", SdkValue::Null);
    Op::decode(&fields).unwrap_or(op)
}

/// Under the Store's lock, replaces the record's `op` with `op` (`None`
/// drops it) while it is still the operation `id`: an operation never
/// overwrites another's.
pub fn replace(home: &Path, store: &str, id: &str, op: Option<&Op>) -> Result<()> {
    let _store = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    if record.op().is_none_or(|held| held.id() != id) {
        return Ok(());
    }
    match op {
        Some(op) => record.with(op::FIELD, op.to_record().clone()),
        None => record.without(op::FIELD),
    }
    .write(home)
}
