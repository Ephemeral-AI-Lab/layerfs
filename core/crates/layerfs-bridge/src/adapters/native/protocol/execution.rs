use super::{Decoder, Encoder};
use crate::contract::{Code, Failure, WorkspaceExecWire, WORKSPACE_EXEC_OUTPUT_BYTES};

pub(super) fn put_exec(e: &mut Encoder, result: &WorkspaceExecWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.u8(u8::from(result.exit_status.is_some())
        | (u8::from(result.stdout_truncated) << 1)
        | (u8::from(result.stderr_truncated) << 2))?;
    if let Some(status) = result.exit_status {
        e.u32(status as u32)?;
    }
    e.blob(&result.stdout)?;
    e.blob(&result.stderr)
}

pub(super) fn take_exec(d: &mut Decoder<'_>) -> Result<WorkspaceExecWire, Failure> {
    let workspace = d.blob(crate::contract::WORKSPACE_ID_BYTES)?;
    let incarnation = d.root()?;
    let flags = d.u8()?;
    if flags & !7 != 0 {
        return Err(Code::InvalidInput.into());
    }
    let result = WorkspaceExecWire {
        workspace,
        incarnation,
        exit_status: if flags & 1 != 0 {
            Some(d.u32()? as i32)
        } else {
            None
        },
        stdout: d.blob(WORKSPACE_EXEC_OUTPUT_BYTES)?,
        stderr: d.blob(WORKSPACE_EXEC_OUTPUT_BYTES)?,
        stdout_truncated: flags & 2 != 0,
        stderr_truncated: flags & 4 != 0,
    };
    result.validate()?;
    Ok(result)
}
