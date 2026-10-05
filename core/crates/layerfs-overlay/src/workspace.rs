//! Indexed Workspace routing and maintained bounded observation.
use crate::{
    db::unsigned, Generation, Overlay, OverlayError, OverlayResult, Route, StatementKind,
    WorkspaceState,
};

impl Overlay {
    /// Binds one complete root without opening another database or scanning it.
    /// Authority/root validation is performed by the Workspace/runtime caller.
    pub fn open_workspace(
        &self,
        incarnation: [u8; 32],
        base_root: [u8; 32],
    ) -> OverlayResult<Route> {
        if incarnation == [0; 32] {
            return Err(OverlayError::Invalid("zero incarnation"));
        }
        self.atomic(|| {
            let ns = self.query(
                StatementKind::Workspace,
                "INSERT INTO workspace(incarnation,base_root,active) VALUES(?1,?2,1) RETURNING ns",
                &[&incarnation.as_slice(), &base_root.as_slice()],
                64,
                |r| r.get(0),
            )?[0];
            Ok(Route { ns, incarnation })
        })
    }
    /// One route-qualified point observation. It does not enumerate/count payload.
    pub fn state(&self, route: Route) -> OverlayResult<WorkspaceState> {
        self.available()?;
        self.query(
            StatementKind::Workspace,
            "SELECT active,captured,revision,base_root,dirty_inodes,dirty_names,lifecycle,installed
             FROM workspace WHERE ns=?1 AND incarnation=?2",
            &[&route.ns, &route.incarnation.as_slice()],
            40,
            |r| {
                let root: Vec<u8> = r.get(3)?;
                let base_root = root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(WorkspaceState {
                    active: Generation(r.get(0)?),
                    captured: r.get::<_, Option<i64>>(1)?.map(Generation),
                    installed: r.get(7)?,
                    revision: r.get(2)?,
                    base_root,
                    dirty_inodes: unsigned(r, 4)?,
                    dirty_names: unsigned(r, 5)?,
                    closed: r.get::<_, i64>(6)? != 0,
                })
            },
        )?
        .pop()
        .ok_or(OverlayError::Stale)
    }
    pub(crate) fn live(&self, route: Route) -> OverlayResult<WorkspaceState> {
        let state = self.state(route)?;
        if state.closed {
            Err(OverlayError::Closed)
        } else {
            Ok(state)
        }
    }
}
