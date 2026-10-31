//! One ordinary command in a container: one non-TTY Exec creation with exact
//! acknowledged runtime custody, one structurally decoded inspection that
//! keeps no Arguments, and the provider's noninteractive run over them with
//! streamed output and exact failure custody.
use super::{Docker, ProviderFailure, Sandbox};
use crate::json::quoted;
use crate::json::{once, Json};
pub use crate::ExecRequest;
use crate::{ContainerId, ExecExit, ExecId, ExecInspection, RuntimeError, WireFailure};
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
impl Docker {
    /// Reads one coherent runtime snapshot. PID is diagnostic, never a signal target.
    pub fn inspect_exec(
        &self,
        container: ContainerId,
        exec: ExecId,
    ) -> Result<ExecInspection, Box<WireFailure>> {
        let mut response = self
            .exchange_kept("GET", &format!("/exec/{exec}/json"), |_| Ok(()))
            .map_err(|mut failure| {
                failure.requested_container = Some(container);
                failure.requested_exec = Some(exec);
                failure
            })?;
        if response.status != 200 {
            let mut failure = response.fail(RuntimeError::Http(response.status), None);
            failure.requested_container = Some(container);
            failure.requested_exec = Some(exec);
            return Err(failure);
        }
        let mut id = None;
        let mut owner = None;
        let parsed = (|| {
            let mut running = None;
            let mut exit = None;
            let mut pid = None;
            let mut fields = 0;
            let mut j = Json::new(&mut response.body);
            j.object(|j, key| {
                if key.equals("ID") {
                    once(&mut fields, 1)?;
                    let value = j.string(true)?;
                    id = Some(ExecId::parse(value.text())?);
                    if value.overflow {
                        return Err(RuntimeError::Protocol("runtime ID width"));
                    }
                } else if key.equals("ContainerID") {
                    once(&mut fields, 2)?;
                    let value = j.string(true)?;
                    owner = Some(ContainerId::parse(value.text())?);
                    if value.overflow {
                        return Err(RuntimeError::Protocol("container ID width"));
                    }
                } else if key.equals("Running") {
                    once(&mut fields, 4)?;
                    running = Some(j.boolean()?);
                } else if key.equals("ExitCode") {
                    once(&mut fields, 8)?;
                    if !j.null()? {
                        exit = Some(
                            i32::try_from(j.integer()?)
                                .map_err(|_| RuntimeError::Protocol("runtime exit width"))?,
                        );
                    }
                } else if key.equals("Pid") {
                    once(&mut fields, 16)?;
                    pid = Some(
                        u32::try_from(j.integer()?)
                            .map_err(|_| RuntimeError::Protocol("runtime PID width"))?,
                    );
                } else {
                    j.skip(1)?;
                }
                Ok(())
            })?;
            j.finish()?;
            if fields & 15 != 15 || id != Some(exec) || owner != Some(container) {
                return Err(RuntimeError::Protocol("runtime identity/required fields"));
            }
            Ok(ExecInspection {
                container,
                exec,
                running: running.expect("required"),
                exit_code: exit,
                pid,
            })
        })();
        match parsed {
            Ok(inspection) => {
                response.release();
                Ok(inspection)
            }
            Err(cause) => {
                let mut failure = response.fail(cause, id);
                failure.observed_container = owner;
                failure.requested_container = Some(container);
                failure.requested_exec = Some(exec);
                Err(failure)
            }
        }
    }
}
fn failure(container: ContainerId, cause: ProviderFailure) -> Box<WireFailure> {
    // Scalar facts describe only the deciding exchange. Owned pending bytes,
    // diagnostics and fence errors remain in the original nested receipt.
    let wire = match &cause {
        ProviderFailure::ExecCreate(f) => Some(f.transfer.as_ref()),
        ProviderFailure::ExecStart(f) => Some(f.transfer.as_ref()),
        ProviderFailure::ExecInspect { failure, .. } => Some(failure.as_ref()),
        _ => None,
    };
    let mut outer = WireFailure::local(RuntimeError::Protocol("provider execution"));
    if let Some(wire) = wire {
        outer.attempted = wire.attempted;
        outer.sent_bytes = wire.sent_bytes;
        outer.request_header_bytes = wire.request_header_bytes;
        outer.sent_body_bytes = wire.sent_body_bytes;
        outer.status = wire.status;
        outer.observed_exec = wire.observed_exec;
        outer.observed_container = wire.observed_container;
        outer.requested_exec = wire.requested_exec;
    } else {
        // Attachment/splitting, output and exit observation follow an attempted Start.
        outer.attempted = true;
    }
    outer.requested_container = Some(container);
    outer.requested_exec = outer.requested_exec.or_else(|| match &cause {
        ProviderFailure::ExecStart(f) => Some(f.created.identity().1),
        ProviderFailure::ExecOutput { handle, .. }
        | ProviderFailure::ExecInspect { handle, .. }
        | ProviderFailure::ExecExitUnknown { handle, .. } => Some(handle.identity().1),
        _ => None,
    });
    outer.cause = RuntimeError::Docker(Box::new(cause));
    outer
}
pub(crate) fn run(
    sandbox: &Sandbox,
    request: &ExecRequest,
    mut stdout: &mut (dyn Write + Send),
    mut stderr: &mut (dyn Write + Send),
) -> Result<ExecExit, Box<WireFailure>> {
    if request.stdin {
        return Err(
            sandbox.invalid("provider run has no stdin source; use ordinary Docker streams")
        );
    }
    let id = sandbox.identity();
    let created = sandbox
        .create_exec(request.clone())
        .map_err(|f| failure(id, ProviderFailure::ExecCreate(f)))?;
    let attached = created
        .start()
        .map_err(|f| failure(id, ProviderFailure::ExecStart(f)))?;
    let (input, output, handle) = attached
        .into_parts()
        .map_err(|f| failure(id, ProviderFailure::ExecSplit(f)))?;
    // Stdin was disabled in the creation request; dropping it sends no cancellation.
    drop(input);
    let progress = match output.copy_to(&mut stdout, &mut stderr) {
        Ok(progress) => progress,
        Err(f) => {
            return Err(failure(
                id,
                ProviderFailure::ExecOutput { failure: f, handle },
            ))
        }
    };
    let inspection = match handle.inspect() {
        Ok(inspection) => inspection,
        Err(f) => {
            return Err(failure(
                id,
                ProviderFailure::ExecInspect {
                    failure: f,
                    handle,
                    progress,
                },
            ))
        }
    };
    let Some(code) = inspection.known_root_exit() else {
        return Err(failure(
            id,
            ProviderFailure::ExecExitUnknown {
                inspection,
                handle,
                progress,
            },
        ));
    };
    Ok(ExecExit {
        code,
        pid: inspection.pid,
        stdout_bytes: progress.stdout,
        stderr_bytes: progress.stderr,
    })
}
