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
                &[&route.ns, &key, &state.base_root.as_slice(), &0_i64],
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
                class: 0,
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
        self.base_source_by_owner(route, owner, installed, 0)
    }
    fn base_source_by_owner(
        &self,
        route: Route,
        owner: u64,
        installed: i64,
        class: i64,
    ) -> OverlayResult<Option<BaseSource>> {
        self.query(
            StatementKind::Lease,
            sql::BASE_SOURCE_LOOKUP,
            &[&route.ns, &integer(owner)?, &class],
            16,
            |row| {
                let root: Vec<u8> = row.get(0)?;
                Ok(BaseSource {
                    route,
                    owner,
                    class,
                    root: root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
                    installed,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub(crate) fn source_state(&self, source: BaseSource) -> OverlayResult<WorkspaceState> {
        let state = self.state(source.route)?;
        self.source_held(source, state)
    }
    /// The same check against a Workspace row this transaction already read.
    pub(crate) fn source_held(
        &self,
        source: BaseSource,
        state: WorkspaceState,
    ) -> OverlayResult<WorkspaceState> {
        // A visit's own source names no row: it is the current base itself.
        if source.class == 3 {
            return if state.base_root == source.root && state.installed == source.installed {
                Ok(state)
            } else {
                Err(OverlayError::Stale)
            };
        }
        if state.base_root != source.root
            || self.base_source_by_owner(
                source.route,
                source.owner,
                state.installed,
                source.class,
            )? != Some(source)
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
        self.release_class(source)?;
        self.atomic_cleanup(|| self.release_source_inner(source))
    }
    pub(crate) fn release_class(&self, source: BaseSource) -> OverlayResult<()> {
        if source.class != 0 {
            return Err(OverlayError::Invalid(
                "file read source requires file-read release",
            ));
        }
        Ok(())
    }
    pub(crate) fn release_source_inner(&self, source: BaseSource) -> OverlayResult<()> {
        self.source_state(source)?;
        let changed = self.execute(
            StatementKind::Lease,
            sql::BASE_SOURCE_DELETE,
            &[
                &source.route.ns,
                &integer(source.owner)?,
                &source.root.as_slice(),
                &source.class,
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
    }
    /// Production-template plan for one source-owner point observation.
    pub fn explain_base_source(&self, route: Route, owner: u64) -> OverlayResult<Vec<String>> {
        self.state(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::BASE_SOURCE_LOOKUP),
            &[&route.ns, &integer(owner)?, &0_i64],
            16,
            |row| row.get(3),
        )
    }
}
