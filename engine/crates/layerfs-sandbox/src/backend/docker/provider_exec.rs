//! Ordinary noninteractive provider execution, with streamed output and exact failure custody.
use super::{ProviderFailure, Sandbox};
use crate::{provider::ExecExit, ExecRequest, RuntimeError, WireFailure};
use std::io::Write;
fn failure(container: crate::ContainerId, cause: ProviderFailure) -> Box<WireFailure> {
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
pub(super) fn run(
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
