//! SQLite's wide-directory placement statements (#818 I2b): the shared
//! window rule and outcome decoding (`placement.rs`) over fixed 32-row
//! inputs, inside the caller's transaction.
//!
//! A set window is one dependency check, one multi-row upsert of its bound
//! children (`bind_host_files.sql`, when it has one) and one multi-row
//! update (`place_children.sql`, one primary-key probe per child). Neither
//! can return the input's columns, so a placed row is matched to its child
//! by its new position, and a binding by its identity; both are distinct in
//! a window.
//! A one-child window and a dependent child run `bind_host_file.sql` and
//! `place_entry.sql`, the per-row loop's statements.
use super::accounting::charge;
use super::statements::{
    BIND_HOST_FILE, BIND_HOST_FILES, PLACEMENT_DEPENDENCIES, PLACE_CHILDREN, PLACE_ENTRY,
};
use super::writes::bind;
use crate::acquisition::failure::{signed, Failure};
use crate::acquisition::placement::{plan, Placement};
use crate::backend::sqlite::session::{prepared::Prepared, rows};
use crate::backend::{error::BackendError, Transaction};
use layerfs_storage::port::acquisition::{Owner, Placed};

/// Statement input slots, not a limit on the caller's public write window.
const INPUT_ROWS: usize = 32;

/// Assigns positions to one window of unpositioned children of `parent`.
pub(crate) fn place_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    placed: &[Placed],
) -> Result<Vec<u64>, Failure> {
    let (operation, parent) = (signed(owner.operation)?, signed(parent)?);
    let statements = std::cell::RefCell::new(Statements {
        bind_one: tx.prepare(BIND_HOST_FILE)?,
        place_one: tx.prepare(PLACE_ENTRY)?,
        dependencies: tx.prepare(PLACEMENT_DEPENDENCIES)?,
        bind: tx.prepare(BIND_HOST_FILES)?,
        place: tx.prepare(PLACE_CHILDREN)?,
    });
    let placement = std::cell::RefCell::new(Placement::default());
    plan(
        placed,
        INPUT_ROWS,
        |children| statements.borrow_mut().dependent(operation, children),
        |child| {
            let mut statements = statements.borrow_mut();
            statements.one(operation, parent, child, &mut placement.borrow_mut())
        },
        |children| {
            let mut statements = statements.borrow_mut();
            statements.window(operation, parent, children, &mut placement.borrow_mut())
        },
    )?;
    let placement = placement.into_inner();
    charge(tx, owner, placement.rows, placement.bytes)?;
    Ok(placement.canonical)
}

struct Statements<'t> {
    bind_one: Prepared<'t>,
    place_one: Prepared<'t>,
    dependencies: Prepared<'t>,
    bind: Prepared<'t>,
    place: Prepared<'t>,
}
impl Statements<'_> {
    /// The per-row bind and place, in the original order.
    fn one(
        &mut self,
        operation: i64,
        parent: i64,
        child: &Placed,
        placement: &mut Placement,
    ) -> Result<(), Failure> {
        let position = signed(child.position)?;
        let mut first = None;
        if let Some((host, path)) = &child.host {
            let (bound, _) = bind(&mut self.bind_one, operation, child.position, host, path)?;
            first = Some(bound);
            placement.bound(child, bound)?;
        }
        let updated = self.place_one.borrowed(
            &[
                &operation,
                &parent,
                &child.name.as_slice(),
                &position,
                &first,
            ],
            32 + child.name.len() as u64,
        )?;
        let kind = match updated.first() {
            Some(row) => Some(row.get::<i64>(0)?),
            None => None,
        };
        Placement::placed(child, kind)
    }

    /// The number of leading children no stored row can refuse.
    fn dependent(&mut self, operation: i64, children: &[Placed]) -> Result<usize, Failure> {
        let positions: [Option<i64>; INPUT_ROWS] =
            std::array::from_fn(|i| children.get(i).map(|c| c.position as i64));
        let hosts: [Option<i64>; INPUT_ROWS] =
            std::array::from_fn(|i| children.get(i).and_then(|c| c.host.as_ref()).map(|_| 1_i64));
        let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 2 * INPUT_ROWS);
        values.push(&operation);
        for i in 0..INPUT_ROWS {
            values.extend([&positions[i] as &dyn rusqlite::ToSql, &hosts[i]]);
        }
        let bytes = 8 + 16 * children.len() as u64;
        let first = self.dependencies.mapped(&values, bytes, 1, |row| {
            row.get::<_, Option<i64>>(0).map_err(rows::error)
        })?;
        match first.first().ok_or(BackendError::Integrity)? {
            None => Ok(children.len()),
            Some(slot) => usize::try_from(*slot)
                .ok()
                .filter(|slot| *slot < children.len())
                .ok_or(Failure::Backend(BackendError::Integrity)),
        }
    }

    /// Binds, then places, one set window; returned rows are matched by
    /// identity and by new position.
    fn window(
        &mut self,
        operation: i64,
        parent: i64,
        children: &[Placed],
        placement: &mut Placement,
    ) -> Result<(), Failure> {
        let absent: Option<&[u8]> = None;
        let identities: Vec<Option<([u8; 8], [u8; 8])>> = children
            .iter()
            .map(|c| {
                c.host
                    .as_ref()
                    .map(|(h, _)| (h.device.to_be_bytes(), h.inode.to_be_bytes()))
            })
            .collect();
        let positions: Vec<i64> = children.iter().map(|c| c.position as i64).collect();
        let mut bound: Vec<Option<i64>> = vec![None; children.len()];
        if children.iter().any(|c| c.host.is_some()) {
            let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 5 * INPUT_ROWS);
            values.push(&operation);
            for (index, child) in children.iter().enumerate() {
                if let (Some((host, path)), Some((device, inode))) =
                    (&child.host, &identities[index])
                {
                    values.extend([
                        &positions[index] as &dyn rusqlite::ToSql,
                        device,
                        inode,
                        &host.evidence,
                        path,
                    ]);
                }
            }
            values.resize(1 + 5 * INPUT_ROWS, &absent);
            let bytes = 8 + children
                .iter()
                .filter_map(|c| c.host.as_ref())
                .map(|(_, path)| 68 + path.len() as u64)
                .sum::<u64>();
            let returned = self.bind.mapped(&values, bytes, children.len(), |row| {
                let device: Vec<u8> = row.get(0).map_err(rows::error)?;
                let inode: Vec<u8> = row.get(1).map_err(rows::error)?;
                let canonical: i64 = row.get(2).map_err(rows::error)?;
                Ok((device, inode, canonical))
            })?;
            for (device, inode, canonical) in returned {
                let slot = identities.iter().position(|identity| {
                    identity.is_some_and(|(d, i)| d[..] == device[..] && i[..] == inode[..])
                });
                let slot = slot.ok_or(BackendError::Integrity)?;
                if bound[slot].replace(canonical).is_some() {
                    return Err(BackendError::Integrity.into());
                }
            }
        }
        let names: Vec<Option<&[u8]>> = (0..INPUT_ROWS)
            .map(|i| children.get(i).map(|c| c.name.as_slice()))
            .collect();
        let places: Vec<Option<i64>> = (0..INPUT_ROWS).map(|i| positions.get(i).copied()).collect();
        let canonical: Vec<Option<i64>> = (0..INPUT_ROWS)
            .map(|i| bound.get(i).copied().flatten())
            .collect();
        let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + 3 * INPUT_ROWS);
        values.extend([&operation as &dyn rusqlite::ToSql, &parent]);
        for i in 0..INPUT_ROWS {
            values.extend([&names[i] as &dyn rusqlite::ToSql, &places[i], &canonical[i]]);
        }
        let bytes = 16
            + children
                .iter()
                .map(|c| 24 + c.name.len() as u64)
                .sum::<u64>();
        let returned = self.place.mapped(&values, bytes, children.len(), |row| {
            let position: i64 = row.get(0).map_err(rows::error)?;
            let kind: i64 = row.get(1).map_err(rows::error)?;
            Ok((position, kind))
        })?;
        let mut kinds: Vec<Option<i64>> = vec![None; children.len()];
        for (position, kind) in returned {
            let slot = positions.iter().position(|p| *p == position);
            let slot = slot.ok_or(BackendError::Integrity)?;
            if kinds[slot].replace(kind).is_some() {
                return Err(BackendError::Integrity.into());
            }
        }
        placement.window(children, &bound, &kinds)
    }
}
