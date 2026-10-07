//! One exact runtime observation, structurally decoded without retaining Arguments.
use super::{
    json::{once, Json},
    Docker,
};
use crate::{ContainerId, ExecId, ExecInspection, RuntimeError, WireFailure};
impl Docker {
    /// Reads one coherent runtime snapshot. PID is diagnostic, never a signal target.
    pub fn inspect_exec(
        &self,
        container: ContainerId,
        exec: ExecId,
    ) -> Result<ExecInspection, Box<WireFailure>> {
        let mut response = self
            .exchange(
                "GET",
                &format!("/v1.54/exec/{exec}/json"),
                false,
                |_| Ok(()),
            )
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
        parsed.map_err(|cause| {
            let mut failure = response.fail(cause, id);
            failure.observed_container = owner;
            failure.requested_container = Some(container);
            failure.requested_exec = Some(exec);
            failure
        })
    }
}
