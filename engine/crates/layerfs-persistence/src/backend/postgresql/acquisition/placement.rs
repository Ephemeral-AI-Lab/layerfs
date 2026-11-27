//! Wide-directory placement on PostgreSQL: one statement per set window
//! (#818 I2a), cut and decoded by the shared algorithm
//! (`sqlite/acquisition/placement.rs`).
//!
//! A unit's input is one write window (at most `WRITE_WINDOW_ROWS` rows or
//! `WRITE_WINDOW_BYTES`, checked by the provider), so a set window is never
//! larger than a `PUT_ENTRIES` statement.
use super::{
    reads::get,
    unit::{values, Writes},
    writes::{bind, charge},
};
use crate::{
    acquisition::placement::{plan, Placement},
    backend::{
        error::BackendError,
        postgresql::session::transaction::Transaction,
        sqlite::acquisition::statements::{signed, Failure},
    },
};
use layerfs_storage::port::acquisition::{HostIdentity, Owner, Placed};
use postgres::Row;
use std::cell::RefCell;

pub(super) const PLACE_ENTRY: &str = "UPDATE {s}.init_entry SET position=$4::bigint,canonical_position=$5::bigint,host_identity=NULL WHERE operation_id=$1::bigint AND parent_position=$2::bigint AND name=$3::bytea AND position IS NULL RETURNING kind";
/// The first slot of a set window whose row a stored row could refuse: a
/// bound child at a stored identity's canonical position (the identity key),
/// or any child at a stored directory's position (`init_entry_directory`).
pub(super) const DEPENDENCIES: &str = "SELECT min(s.slot) FROM unnest($2::bigint[],$3::bigint[],$4::boolean[]) AS s(slot,position,host) \
 WHERE (s.host AND EXISTS(SELECT 1 FROM {s}.init_host_file h WHERE h.operation_id=$1::bigint AND h.canonical_position=s.position)) \
 OR EXISTS(SELECT 1 FROM {s}.init_entry e WHERE e.operation_id=$1::bigint AND e.kind=2 AND e.position=s.position)";
/// Binds a set window's regular children (`BIND_HOST_FILE` over its rows),
/// places every child (`PLACE_ENTRY` over its rows, with its binding's
/// canonical position) and answers each slot's binding and kind in slot
/// order; a slot with no bound or placed row answers NULL there. Names,
/// positions and identities in the window are distinct, so no row is
/// touched twice.
pub(super) const PLACE_CHILDREN: &str = "WITH s AS (SELECT * FROM unnest($3::bigint[],$4::bytea[],$5::bigint[],$6::bytea[],$7::bytea[],$8::bytea[],$9::bytea[]) AS s(slot,name,position,device,inode,evidence,path)), \
 b AS (INSERT INTO {s}.init_host_file(operation_id,canonical_position,device,inode,evidence,host_path) \
  SELECT $1::bigint,s.position,s.device,s.inode,s.evidence,s.path FROM s WHERE s.device IS NOT NULL ORDER BY s.slot \
  ON CONFLICT(operation_id,device,inode) DO UPDATE SET aliases=init_host_file.aliases+1 WHERE init_host_file.evidence=excluded.evidence \
  RETURNING device,inode,canonical_position), \
 p AS (UPDATE {s}.init_entry AS e SET position=s.position,canonical_position=b.canonical_position,host_identity=NULL \
  FROM s LEFT JOIN b ON b.device=s.device AND b.inode=s.inode \
  WHERE e.operation_id=$1::bigint AND e.parent_position=$2::bigint AND e.name=s.name AND e.position IS NULL \
  RETURNING s.slot,e.kind) \
 SELECT s.slot,b.canonical_position,p.kind FROM s LEFT JOIN b ON b.device=s.device AND b.inode=s.inode \
 LEFT JOIN p ON p.slot=s.slot ORDER BY s.slot";

/// Assigns positions to one window of unpositioned children of `parent`.
pub(super) fn place_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    placed: &[Placed],
) -> Result<Vec<u64>, Failure> {
    let (operation, parent) = (signed(owner.operation)?, signed(parent)?);
    let placement = RefCell::new(Placement::default());
    place(&mut &*tx, operation, parent, placed, &placement)?;
    let placement = placement.into_inner();
    charge(tx, owner, placement.rows, placement.bytes)?;
    Ok(placement.canonical)
}

/// `place_children` without its charge, on either route (`unit::Writes`):
/// the dependency check and a dependent child's binding are its waits; the
/// placements are read back in order before it answers, since the charge
/// depends on them.
pub(super) fn place<'a>(
    w: &mut impl Writes<'a>,
    operation: i64,
    parent: i64,
    placed: &'a [Placed],
    placement: &'a RefCell<Placement>,
) -> Result<(), Failure> {
    let route = RefCell::new(w);
    plan(
        placed,
        usize::MAX,
        |children| first_dependent(&mut **route.borrow_mut(), operation, children),
        |child| {
            one(
                &mut **route.borrow_mut(),
                operation,
                parent,
                child,
                placement,
            )
        },
        |children| {
            window(
                &mut **route.borrow_mut(),
                operation,
                parent,
                children,
                placement,
            )
        },
    )?;
    route.into_inner().settle()
}

/// The per-row bind and place, in the original order.
fn one<'a>(
    w: &mut impl Writes<'a>,
    operation: i64,
    parent: i64,
    child: &'a Placed,
    placement: &'a RefCell<Placement>,
) -> Result<(), Failure> {
    let position = signed(child.position)?;
    let mut first = None;
    if let Some((host, path)) = &child.host {
        let (bound, _) = bind(w, operation, child.position, host, path)?;
        first = Some(bound);
        placement.borrow_mut().bound(child, bound)?;
    }
    w.queue(
        PLACE_ENTRY,
        values![operation, parent, child.name.clone(), position, first],
        32 + child.name.len() as u64,
        Some(Box::new(move |updated: &[Row]| {
            let kind = updated.first().map(|row| get::<i64>(row, 0)).transpose()?;
            Placement::placed(child, kind)
        })),
    )
}

/// The number of leading children no stored row can refuse, by one indexed
/// check in this transaction.
fn first_dependent<'a>(
    w: &mut impl Writes<'a>,
    operation: i64,
    children: &[Placed],
) -> Result<usize, Failure> {
    let slots: Vec<i64> = (0..children.len() as i64).collect();
    let positions: Vec<i64> = children.iter().map(|c| c.position as i64).collect();
    let hosts: Vec<bool> = children.iter().map(|c| c.host.is_some()).collect();
    let found = w.ask(
        DEPENDENCIES,
        &[&operation, &slots, &positions, &hosts],
        8 + 17 * children.len() as u64,
    )?;
    let first: Option<i64> = get(found.first().ok_or(BackendError::Integrity)?, 0)?;
    match first {
        None => Ok(children.len()),
        Some(slot) => usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < children.len())
            .ok_or(Failure::Backend(BackendError::Integrity)),
    }
}

/// Binds and places one set window in one statement, decoded when its
/// reply is back.
fn window<'a>(
    w: &mut impl Writes<'a>,
    operation: i64,
    parent: i64,
    children: &'a [Placed],
    placement: &'a RefCell<Placement>,
) -> Result<(), Failure> {
    let slots: Vec<i64> = (0..children.len() as i64).collect();
    let names: Vec<Vec<u8>> = children.iter().map(|c| c.name.clone()).collect();
    let positions: Vec<i64> = children.iter().map(|c| c.position as i64).collect();
    let host = |field: fn(&HostIdentity, &[u8]) -> Vec<u8>| -> Vec<Option<Vec<u8>>> {
        children
            .iter()
            .map(|c| c.host.as_ref().map(|(h, path)| field(h, path)))
            .collect()
    };
    let devices = host(|h, _| h.device.to_be_bytes().to_vec());
    let inodes = host(|h, _| h.inode.to_be_bytes().to_vec());
    let evidence = host(|h, _| h.evidence.to_vec());
    let paths = host(|_, path| path.to_vec());
    let bytes = 16
        + children
            .iter()
            .map(|c| {
                24 + c.name.len() as u64 + c.host.as_ref().map_or(0, |(_, p)| 60 + p.len() as u64)
            })
            .sum::<u64>();
    let decode = move |returned: &[Row]| -> Result<(), Failure> {
        if returned.len() != children.len() {
            return Err(BackendError::Integrity.into());
        }
        let (mut bound, mut kinds) = (Vec::new(), Vec::new());
        for (slot, row) in returned.iter().enumerate() {
            if get::<i64>(row, 0)? != slot as i64 {
                return Err(BackendError::Integrity.into());
            }
            bound.push(get::<Option<i64>>(row, 1)?);
            kinds.push(get::<Option<i64>>(row, 2)?);
        }
        placement.borrow_mut().window(children, &bound, &kinds)
    };
    w.queue(
        PLACE_CHILDREN,
        values![operation, parent, slots, names, positions, devices, inodes, evidence, paths],
        bytes,
        Some(Box::new(decode)),
    )
}

/// Every statement `place` may send, for a unit's one prepare.
pub(super) const TEXTS: &[&str] = &[
    DEPENDENCIES,
    PLACE_CHILDREN,
    super::writes::BIND_HOST_FILE,
    PLACE_ENTRY,
];
