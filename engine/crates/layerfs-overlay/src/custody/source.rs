//! Exact short base-source custody, with provider IO outside the SQL owner.
use crate::{db::integer, BaseSource, Inode, Overlay, OverlayError, OverlayResult, WorkspaceState};

impl Overlay {
    /// A diagnostic uses its own minted source class, disjoint from kernel request ids.
    pub fn acquire_diagnostic_source(&self) -> OverlayResult<BaseSource> {
        self.atomic(|| {
            let state = self.live()?;
            let source = BaseSource {
                engine: self.identity,
                owner: self.mint_owner()?,
                class: 4,
                root: state.base_root,
                installed: state.installed,
            };
            self.insert_source(source)?;
            self.source_count(true)?;
            Ok(source)
        })
    }

    /// Acquires the current base for one bounded request window. Duplicate
    /// operation IDs fail atomically. Kernel service fences later acquisitions
    /// behind known install readiness, without attempting/retrying install.
    pub fn acquire_base_source(&self, owner: u64) -> OverlayResult<BaseSource> {
        let key = integer(owner)?;
        if key == 0 {
            return Err(OverlayError::Invalid("zero base-source owner"));
        }
        self.atomic(|| {
            let state = self.live()?;
            let source = BaseSource {
                engine: self.identity,
                owner,
                class: 0,
                root: state.base_root,
                installed: state.installed,
            };
            self.insert_source(source)?;
            self.source_count(true)?;
            Ok(source)
        })
    }
    /// Bounded observation of original custody after a lost completion. Missing
    /// is absence, never authority to replay an acquisition or infer its fence.
    pub fn retained_base_source(&self, owner: u64) -> OverlayResult<Option<BaseSource>> {
        let installed = self.state()?.installed;
        self.base_source_by_owner(owner, installed, 0)
    }
    fn base_source_by_owner(
        &self,
        owner: u64,
        installed: i64,
        class: i64,
    ) -> OverlayResult<Option<BaseSource>> {
        integer(owner)?;
        Ok(self
            .open
            .borrow()
            .sources
            .rows
            .get(&(owner, class))
            .copied()
            .filter(|source| source.installed == installed))
    }
    pub(crate) fn insert_source(&self, source: BaseSource) -> OverlayResult<()> {
        let key = (source.owner, source.class);
        let mut open = self.open.borrow_mut();
        if open.sources.rows.contains_key(&key) {
            return Err(OverlayError::Invalid("duplicate base source"));
        }
        open.sources.put(key, source);
        Ok(())
    }
    pub(crate) fn source_state(&self, source: BaseSource) -> OverlayResult<WorkspaceState> {
        let state = self.state()?;
        self.source_held(source, state)
    }
    /// The same check against runtime Workspace state this job already read.
    pub(crate) fn source_held(
        &self,
        source: BaseSource,
        state: WorkspaceState,
    ) -> OverlayResult<WorkspaceState> {
        if source.engine != self.identity {
            return Err(OverlayError::Stale);
        }
        // A visit's own source names no row: it is the current base itself.
        if source.class == 3 {
            return if state.base_root == source.root && state.installed == source.installed {
                Ok(state)
            } else {
                Err(OverlayError::Stale)
            };
        }
        if state.base_root != source.root
            || self.base_source_by_owner(source.owner, state.installed, source.class)?
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
        self.inode_at(serial, state.active, state.installed)
    }
    /// One exact release after the request/provider continuation is fenced.
    /// Original errors leave this source in custody; no hidden Drop SQL/retry.
    pub fn release_base_source(&self, source: BaseSource) -> OverlayResult<()> {
        self.release_class(source)?;
        self.atomic_cleanup(|| self.release_source_inner(source))
    }
    pub(crate) fn release_class(&self, source: BaseSource) -> OverlayResult<()> {
        if source.class != 0 && source.class != 4 {
            return Err(OverlayError::Invalid(
                "file read source requires file-read release",
            ));
        }
        Ok(())
    }
    pub(crate) fn release_source_inner(&self, source: BaseSource) -> OverlayResult<()> {
        self.source_state(source)?;
        self.open
            .borrow_mut()
            .sources
            .remove((source.owner, source.class));
        self.source_count(false)?;
        Ok(())
    }
}
