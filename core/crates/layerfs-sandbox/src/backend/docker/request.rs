//! One ordinary non-TTY Exec creation; exact acknowledged runtime custody.
use super::{
    json::{once, Json},
    strings::quoted,
    Docker,
};
use crate::{CommandIdentity, ContainerId, ExecId, RuntimeError, WireFailure};
use std::{
    fmt,
    io::{self, Write},
};
/// Caller-owned ordinary command metadata. No hidden shell/classifier/preparation.
#[derive(Clone)]
pub struct ExecRequest {
    pub identity: CommandIdentity,
    pub arguments: Vec<String>,
    pub environment: Vec<String>,
    pub directory: String,
    pub stdin: bool,
}
impl fmt::Debug for ExecRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecRequest")
            .field("identity", &self.identity)
            .field("arguments_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("directory", &self.directory)
            .field("stdin", &self.stdin)
            .finish()
    }
}
impl ExecRequest {
    fn check(&self) -> Result<(), RuntimeError> {
        self.identity.check()?;
        if self.arguments.is_empty()
            || self.arguments[0].is_empty()
            || !std::path::Path::new(&self.directory).is_absolute()
            || self
                .arguments
                .iter()
                .chain(&self.environment)
                .chain(std::iter::once(&self.directory))
                .any(|v| v.as_bytes().contains(&0))
        {
            return Err(RuntimeError::Protocol("ordinary command fields"));
        }
        if self
            .environment
            .iter()
            .any(|v| !v.contains('=') || v.starts_with('='))
        {
            return Err(RuntimeError::Protocol("ordinary environment field"));
        }
        Ok(())
    }
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
            let mut response = self.exchange(
                "POST",
                &format!("/v1.54/containers/{container}/exec"),
                false,
                |w| request.encode(w),
            )?;
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
            parsed.map_err(|cause| response.fail(cause, observed))
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
