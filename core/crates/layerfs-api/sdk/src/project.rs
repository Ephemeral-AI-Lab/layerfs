//! Authorized Project Init and Branch fork through the composed Server.
use layerfs_api_core::{Branch, Error, Project};
use layerfs_bridge::contract::{
    Failure, HistoryCommand, HistoryForkSource, HistoryResult, Operation, Request, Response,
    HISTORY_PROFILE, HISTORY_RESULT_BYTES, MAX_OPERATION_MS,
};
use layerfs_server::Server;
use std::io::Cursor;

/// The authority-supplied Branch body; the published identity adds its tag.
pub const BRANCH_BODY_BYTES: usize = 16;

pub struct ProjectApi<'a> {
    server: &'a Server,
}

impl<'a> ProjectApi<'a> {
    /// Bind the SDK to one composed host authority.
    pub fn new(server: &'a Server) -> Self {
        Self { server }
    }

    /// Import one host-visible directory and return its published genesis.
    pub fn init(&self, name: &str, path: &std::path::Path) -> Result<Project, Error> {
        let created =
            self.service()
                .init_project(&self.peer()?, self.server.store(), name, path)?;
        Ok(Project {
            id: created.stack.stack,
            genesis_layer: created.stack.head_layer,
            root: created.root,
            root_serial: created.root_serial,
        })
    }

    /// Fork one named Branch from a project's genesis Layer.
    ///
    /// The fork is an ordinary authorized history command: it runs through the
    /// same admission, transaction and publication path as any other caller, and
    /// returns the published Branch snapshot rather than a benchmark identity.
    pub fn fork(
        &self,
        project: &Project,
        branch: [u8; BRANCH_BODY_BYTES],
        name: &str,
    ) -> Result<Branch, Error> {
        if branch.iter().all(|byte| *byte == 0) {
            return Err(Error::Backend(Failure::from(
                layerfs_bridge::contract::Code::InvalidInput,
            )));
        }
        let request = Request {
            id: 1,
            generation: 1,
            store: self.server.store(),
            profile: HISTORY_PROFILE,
            deadline_ms: MAX_OPERATION_MS,
            response_bytes: HISTORY_RESULT_BYTES as u64,
            operation: Operation::HistoryCommand(HistoryCommand::Fork {
                stack: project.id,
                branch,
                name: name.as_bytes().to_vec(),
                source: HistoryForkSource::Layer(project.genesis_layer),
            }),
        };
        let (result, _) = self.service().handle(
            &self.peer()?,
            &request,
            &mut Cursor::new([]),
            &mut std::io::sink(),
        );
        match result? {
            Response::History(result) => match *result {
                HistoryResult::BranchSnapshot(snapshot) => Ok(Branch {
                    id: snapshot.branch.branch,
                    name: String::from_utf8_lossy(&snapshot.branch.name).into_owned(),
                    base_layer: snapshot.branch.base_layer,
                    head_commit: snapshot.branch.head_commit,
                    head_root: snapshot.head_root,
                    base_root: snapshot.base_root,
                    effective_root: snapshot.effective_root,
                    root_serial: snapshot.root_serial,
                    scope: snapshot.scope,
                }),
                _ => Err(Error::Backend(
                    layerfs_bridge::contract::Code::Integrity.into(),
                )),
            },
            _ => Err(Error::Backend(
                layerfs_bridge::contract::Code::Integrity.into(),
            )),
        }
    }

    fn service(&self) -> &layerfs_server::Service {
        self.server.service()
    }

    fn peer(&self) -> Result<layerfs_bridge::adapters::native::connection::VerifiedPeer, Error> {
        self.server.peer().map_err(Error::Backend)
    }
}
