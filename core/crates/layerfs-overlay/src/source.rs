//! Exact short base-source custody, with provider IO outside the SQL owner.
use crate::{
    db::integer, sql, BaseSource, Inode, Overlay, OverlayError, OverlayResult, Route,
    StatementKind, WorkspaceState,
};

impl Overlay {
    /// Acquires the current base for one bounded request window. Duplicate
    /// operation IDs fail atomically. Native service fences later acquisitions
    /// behind known install readiness, without attempting/retrying install.
    pub fn acquire_base_source(&self, route: Route, owner: u64) -> OverlayResult<BaseSource> {
        let key = integer(owner)?;
        if key == 0 {
            return Err(OverlayError::Invalid("zero base-source owner"));
        }
        self.atomic(|| {
            let state = self.live(route)?;
            self.execute(
                StatementKind::Lease,
                sql::BASE_SOURCE_INSERT,
                &[&route.ns, &key, &state.base_root.as_slice()],
                48,
            )?;
            self.execute(
                StatementKind::Workspace,
                sql::BASE_SOURCE_INCREMENT,
                &[&route.ns],
                8,
            )?;
            Ok(BaseSource {
                route,
                owner,
                root: state.base_root,
                installed: state.installed,
            })
        })
    }
    /// Bounded observation of original custody after a lost completion. Missing
    /// is absence, never authority to replay an acquisition or infer its fence.
    pub fn retained_base_source(
        &self,
        route: Route,
        owner: u64,
    ) -> OverlayResult<Option<BaseSource>> {
        let installed = self.state(route)?.installed;
        self.base_source_by_owner(route, owner, installed)
    }
    fn base_source_by_owner(
        &self,
        route: Route,
        owner: u64,
        installed: i64,
    ) -> OverlayResult<Option<BaseSource>> {
        self.query(
            StatementKind::Lease,
            sql::BASE_SOURCE_LOOKUP,
            &[&route.ns, &integer(owner)?],
            16,
            |row| {
                let root: Vec<u8> = row.get(0)?;
                Ok(BaseSource {
                    route,
                    owner,
                    root: root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
                    installed,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub(crate) fn source_state(&self, source: BaseSource) -> OverlayResult<WorkspaceState> {
        let state = self.state(source.route)?;
        if state.base_root != source.root
            || self.base_source_by_owner(source.route, source.owner, state.installed)?
                != Some(source)
        {
            return Err(OverlayError::Stale);
        }
        Ok(state)
    }
    /// Current overlay metadata over an exact still-owned unchanged base.
    /// Existing request custody stays usable during logical close.
    pub fn source_inode(&self, source: BaseSource, serial: u64) -> OverlayResult<Option<Inode>> {
        let state = self.source_state(source)?;
        self.inode_at(source.route, serial, state.active, state.installed)
    }
    /// One exact release after the request/provider continuation is fenced.
    /// Original errors leave this source in custody; no hidden Drop SQL/retry.
    pub fn release_base_source(&self, source: BaseSource) -> OverlayResult<()> {
        self.atomic(|| {
            self.source_state(source)?;
            let changed = self.execute(
                StatementKind::Lease,
                sql::BASE_SOURCE_DELETE,
                &[
                    &source.route.ns,
                    &integer(source.owner)?,
                    &source.root.as_slice(),
                ],
                48,
            )?;
            if changed != 1 {
                return Err(OverlayError::Stale);
            }
            self.execute(
                StatementKind::Workspace,
                sql::BASE_SOURCE_DECREMENT,
                &[&source.route.ns],
                8,
            )?;
            self.queue_closed(source.route)
        })
    }
    /// Production-template plan for one source-owner point observation.
    pub fn explain_base_source(&self, route: Route, owner: u64) -> OverlayResult<Vec<String>> {
        self.state(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::BASE_SOURCE_LOOKUP),
            &[&route.ns, &integer(owner)?],
            16,
            |row| row.get(3),
        )
    }
}
