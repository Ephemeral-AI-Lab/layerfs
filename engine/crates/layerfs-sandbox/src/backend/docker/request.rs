//! One ordinary non-TTY Exec creation; exact acknowledged runtime custody.
use super::Docker;
use crate::backend::json::{once, Json};
use crate::backend::strings::quoted;
pub use crate::ExecRequest;
use crate::{ContainerId, ExecId, RuntimeError, WireFailure};
use std::io::{self, Write};
impl ExecRequest {
    fn encode(&self, out: &mut dyn Write) -> io::Result<()> {
        out.write_all(b"{\"AttachStdout\":true,\"AttachStderr\":true,\"AttachStdin\":")?;
        out.write_all(if self.stdin { b"true" } else { b"false" })?;
        out.write_all(b",\"Tty\":false,\"Privileged\":false,\"User\":")?;
        quoted(out, &format!("{}:{}", self.identity.uid, self.identity.gid))?;
        out.write_all(b",\"WorkingDir\":")?;
        quoted(out, &self.directory)?;
        for (name, items) in [
            (b",\"Cmd\":".as_slice(), &self.arguments),
            (b",\"Env\":".as_slice(), &self.environment),
        ] {
            out.write_all(name)?;
            out.write_all(b"[")?;
            for (n, item) in items.iter().enumerate() {
                if n != 0 {
                    out.write_all(b",")?;
                }
                quoted(out, item)?;
            }
            out.write_all(b"]")?;
        }
        out.write_all(b"}")
    }
}
/// One acknowledged Exec creation. Starting consumes it; no implicit replay.
#[derive(Debug)]
pub struct ExecCreated {
    pub(super) docker: Docker,
    pub(super) container: ContainerId,
    pub(super) exec: ExecId,
    pub(super) request: ExecRequest,
    pub(super) start_attempted: bool,
}
/// Original request and transfer failure. A known partial ID is not a completed acknowledgment.
#[derive(Debug)]
pub struct ExecCreateFailure {
    pub container: ContainerId,
    pub request: ExecRequest,
    pub transfer: Box<WireFailure>,
}
impl Docker {
    /// Creates one actual ordinary runtime Exec, once. This does not launch it yet.
    pub fn create_exec(
        &self,
        container: ContainerId,
        request: ExecRequest,
    ) -> Result<ExecCreated, Box<ExecCreateFailure>> {
        let mut observed = None;
        let result = (|| {
            request.check().map_err(|cause| {
                Box::new(WireFailure {
                    attempted: false,
                    sent_bytes: 0,
                    request_header_bytes: 0,
                    sent_body_bytes: 0,
                    pending_request: Default::default(),
                    status: None,
                    cause,
                    fence_error: None,
                    observed_exec: None,
                    observed_container: None,
                    requested_container: None,
                    requested_exec: None,
                    error_body: None,
                })
            })?;
            let mut response =
                self.exchange_kept("POST", &format!("/containers/{container}/exec"), |w| {
                    request.encode(w)
                })?;
            if response.status != 201 {
                return Err(response.fail(RuntimeError::Http(response.status), None));
            }
            let parsed = (|| {
                let mut json = Json::new(&mut response.body);
                let mut fields = 0;
                json.object(|j, key| {
                    if key.equals("Id") {
                        once(&mut fields, 1)?;
                        let value = j.string(true)?;
                        if value.overflow {
                            return Err(RuntimeError::Protocol("Exec ID length"));
                        }
                        observed = Some(ExecId::parse(value.text())?);
                    } else {
                        j.skip(1)?;
                    }
                    Ok(())
                })?;
                json.finish()?;
                observed.ok_or(RuntimeError::Protocol("missing Exec ID"))
            })();
            match parsed {
                Ok(exec) => {
                    response.release();
                    Ok(exec)
                }
                Err(cause) => Err(response.fail(cause, observed)),
            }
        })();
        match result {
            Ok(exec) => Ok(ExecCreated {
                docker: self.clone(),
                container,
                exec,
                request,
                start_attempted: false,
            }),
            Err(mut transfer) => {
                transfer.requested_container = Some(container);
                Err(Box::new(ExecCreateFailure {
                    container,
                    request,
                    transfer,
                }))
            }
        }
    }
}

impl ExecCreated {
    /// Original acknowledged runtime identity, immutable through this owner.
    pub fn identity(&self) -> (ContainerId, ExecId) {
        (self.container, self.exec)
    }
    /// Original submitted command metadata; never a later inspection/refresh.
    pub fn request(&self) -> &ExecRequest {
        &self.request
    }
}
