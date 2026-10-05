//! One atomic bounded namespace job: several inode/name finals, one ticket.
use crate::{
    db::integer,
    inode::{check, check_name, optional_serial},
    sql, BaseSource, Binding, Changes, Generation, Inode, NameLayers, Overlay, OverlayError,
    OverlayResult, Publication, StatementKind, WorkspaceState, COMPOUND_INODES, COMPOUND_NAMES,
};

/// Consistent local rows for one owner job over an exact owned base source.
/// The single owner runs a job to completion, so these point reads and the
/// job's one `apply` observe the same state without holding a transaction
/// across caller logic. It must not outlive its job.
pub struct SourceRows<'a> {
    db: &'a Overlay,
    source: BaseSource,
    state: WorkspaceState,
}
impl Overlay {
    /// Checks source custody once for the bounded point reads of one job.
    pub fn source_rows(&self, source: BaseSource) -> OverlayResult<SourceRows<'_>> {
        let state = self.source_state(source)?;
        Ok(SourceRows {
            db: self,
            source,
            state,
        })
    }
    /// Publishes every final value of one namespace operation or none of them.
    /// Values are semantically checked by Workspace against rows read in this
    /// same owner job. A closed Workspace refuses; a failed job changes nothing
    /// and leaves no ticket. Exactly one reply-attempt ticket covers the job.
    pub fn apply(&self, source: BaseSource, changes: &Changes) -> OverlayResult<Publication> {
        if changes.inodes.len() > COMPOUND_INODES
            || changes.names.len() > COMPOUND_NAMES
            || (changes.inodes.is_empty() && changes.names.is_empty())
        {
            return Err(OverlayError::Invalid("compound window"));
        }
        for (index, inode) in changes.inodes.iter().enumerate() {
            check(inode)?;
            if changes.inodes[..index]
                .iter()
                .any(|other| other.serial == inode.serial)
            {
                return Err(OverlayError::Invalid("duplicate compound inode"));
            }
        }
        let mut keys = Vec::with_capacity(changes.names.len());
        for (index, change) in changes.names.iter().enumerate() {
            let parent = check_name(change.parent, &change.name)?;
            let target = match change.binding {
                Binding::Bound(0) => return Err(OverlayError::Invalid("zero inode serial")),
                Binding::Bound(serial) => Some(integer(serial)?),
                Binding::Removed { .. } => None,
            };
            if changes.names[..index]
                .iter()
                .any(|other| other.parent == change.parent && other.name == change.name)
            {
                return Err(OverlayError::Invalid("duplicate compound name"));
            }
            keys.push((parent, target));
        }
        if let Some((serial, cell)) = &changes.cell {
            crate::payload::check(cell)?;
            if !changes.inodes.iter().any(|inode| inode.serial == *serial) {
                return Err(OverlayError::Invalid("compound cell owner"));
            }
        }
        if let Some(write) = &changes.write {
            // The owner's final value must already cover the written window.
            let end = write.offset.checked_add(write.data.len() as u64);
            let owner = changes
                .inodes
                .iter()
                .find(|inode| inode.serial == write.serial && inode.kind == crate::InodeKind::File);
            if write.data.is_empty()
                || write.data.len() > crate::WRITE_WINDOW
                || owner.is_none_or(|inode| end.is_none_or(|end| end > inode.size))
            {
                return Err(OverlayError::Invalid("compound write window"));
            }
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let route = source.route;
            let mut inodes = 0_i64;
            let mut layers = Vec::with_capacity(changes.inodes.len());
            for inode in &changes.inodes {
                let (added, layer) = self.put_inode(route, &state, inode)?;
                inodes += i64::from(added);
                layers.push((inode.serial, layer));
            }
            let layer = |serial: u64| {
                layers
                    .iter()
                    .find(|(owner, _)| *owner == serial)
                    .map(|(_, layer)| layer)
                    .ok_or(OverlayError::Invalid("compound payload owner"))
            };
            let mut names = 0_i64;
            for (change, (parent, target)) in changes.names.iter().zip(&keys) {
                let whiteout = match change.binding {
                    Binding::Bound(_) => true,
                    Binding::Removed { inherited } => self
                        .lower_name(&state, route.ns, *parent, &change.name)?
                        .map_or(inherited, |row| row.is_some()),
                };
                if whiteout {
                    names += i64::from(self.put_name(
                        route,
                        state.active,
                        *parent,
                        &change.name,
                        *target,
                    )?);
                } else {
                    // Nothing below binds this name: no row is the final state.
                    names -= self.execute(
                        StatementKind::Dentry,
                        sql::DENTRY_DROP,
                        &[&route.ns, parent, &change.name, &state.active.0],
                        24 + change.name.len() as u64,
                    )? as i64;
                }
            }
            if let Some((serial, cell)) = &changes.cell {
                self.put_cell(route, integer(*serial)?, layer(*serial)?, cell)?;
            }
            if let Some(write) = &changes.write {
                self.write_cells(
                    route.ns,
                    integer(write.serial)?,
                    layer(write.serial)?,
                    write.offset,
                    &write.data,
                )?;
            }
            self.settle(route, &state, inodes, names)
        })
    }
    fn lower_name(
        &self,
        state: &WorkspaceState,
        ns: i64,
        parent: i64,
        name: &[u8],
    ) -> OverlayResult<Option<Option<u64>>> {
        Ok(self
            .query(
                StatementKind::Dentry,
                sql::DENTRY_LOOKUP,
                &[&ns, &parent, &name, &(state.active.0 - 1), &state.installed],
                32 + name.len() as u64,
                |row| optional_serial(row.get(0)?, 0),
            )?
            .pop())
    }
    /// Plans of the compound job's exact statements: lower-name seek, active
    /// row probes, upserts and the no-row delete. INSERT plans are VM programs.
    pub fn explain_compound(&self, source: BaseSource) -> OverlayResult<Vec<String>> {
        let state = self.source_state(source)?;
        let ns = source.route.ns;
        let name: &[u8] = b"n";
        let none: Option<i64> = None;
        let mut plans = Vec::new();
        for (label, statement, params) in [
            (
                "lower-name",
                sql::DENTRY_LOOKUP,
                vec![
                    &ns as &dyn rusqlite::ToSql,
                    &1_i64,
                    &name,
                    &state.active.0,
                    &state.installed,
                ],
            ),
            (
                "active-name",
                sql::DENTRY_ACTIVE,
                vec![&ns, &1_i64, &name, &state.active.0],
            ),
            (
                "drop-name",
                sql::DENTRY_DROP,
                vec![&ns, &1_i64, &name, &state.active.0],
            ),
            (
                "active-inode",
                sql::LAYER_ACTIVE,
                vec![&ns, &1_i64, &state.active.0],
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                &params,
                32,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        let programs: [(&str, &str, Vec<&dyn rusqlite::ToSql>); 2] = [
            (
                "put-name",
                sql::DENTRY_PUT,
                vec![&ns, &1_i64, &name, &state.active.0, &none],
            ),
            (
                "put-inode",
                sql::INODE_PUT,
                vec![
                    &ns,
                    &1_i64,
                    &state.active.0,
                    &1_i64,
                    &0_i64,
                    &0_i64,
                    &0_i64,
                    &1_i64,
                    &0_i64,
                    &0_i64,
                    &0_i64,
                    &0_i64,
                    &0_i64,
                    &0_i64,
                ],
            ),
        ];
        for (label, statement, params) in programs {
            // (opcode, cursor) rows of the actual program. Constant CHECK IN-lists
            // iterate ephemeral cursors; only persistent b-tree cursors can scan.
            let program = self.query(
                StatementKind::Explain,
                &format!("EXPLAIN {statement}"),
                &params,
                32,
                |row| Ok((row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
            )?;
            let tables: Vec<i64> = program
                .iter()
                .filter(|(op, _)| op == "OpenRead" || op == "OpenWrite")
                .map(|(_, cursor)| *cursor)
                .collect();
            let scans = program
                .iter()
                .filter(|(op, cursor)| {
                    matches!(op.as_str(), "Rewind" | "Next" | "Prev" | "Last")
                        && tables.contains(cursor)
                })
                .count();
            plans.push(format!(
                "{label}: vm-program opcodes={} btree-cursors={} btree-scan-opcodes={scans}",
                program.len(),
                tables.len()
            ));
        }
        Ok(plans)
    }
}
impl SourceRows<'_> {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    /// Active generation for values this job will publish.
    pub const fn active(&self) -> Generation {
        self.state.active
    }
    /// Latest local inode row of the current view; None delegates to the base.
    pub fn inode(&self, serial: u64) -> OverlayResult<Option<Inode>> {
        self.db.inode_at(
            self.source.route,
            serial,
            self.state.active,
            self.state.installed,
        )
    }
    /// Two point seeks: the active row and the latest lower row of one name.
    pub fn name(&self, parent: u64, name: &[u8]) -> OverlayResult<NameLayers> {
        let key = check_name(parent, name)?;
        let ns = self.source.route.ns;
        let active = self
            .db
            .query(
                StatementKind::Dentry,
                sql::DENTRY_ACTIVE,
                &[&ns, &key, &name, &self.state.active.0],
                24 + name.len() as u64,
                |row| optional_serial(row.get(0)?, 0),
            )?
            .pop();
        let lower = self.db.lower_name(&self.state, ns, key, name)?;
        Ok(NameLayers { active, lower })
    }
}
