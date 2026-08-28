use super::paths::ControlPaths;
use super::receipt::{write_control_failure, write_control_success};
use super::request::{read_splice_request, CONTROL_DECODE_Q_BYTES};
use layerfs_mount::workspace::{ByteBudget, MountedLifecycle, MountedWorkspace};

pub(in crate::process) fn execute_splice_control(
    workspace: &std::sync::Arc<std::sync::Mutex<MountedWorkspace>>,
    budget: &std::sync::Arc<ByteBudget>,
    paths: &ControlPaths,
) -> Result<(), String> {
    budget.pause_and_wait().map_err(|error| error.to_string())?;
    let decode_reservation = budget
        .try_reserve(CONTROL_DECODE_Q_BYTES)
        .map_err(|error| error.to_string())?;
    let request = match read_splice_request(&paths.request) {
        Ok(request) => request,
        Err(error) => {
            drop(decode_reservation);
            if let Ok(mut workspace) = workspace.lock() {
                workspace.mark_incomplete();
            }
            let _ = budget.close_and_wait();
            let _ = write_control_failure(&paths.receipt, MountedLifecycle::Incomplete, &error);
            return Err(error);
        }
    };
    drop(decode_reservation);
    let result = workspace
        .lock()
        .map_err(|_| "workspace lock poisoned".to_owned())?
        .splice_path(
            &request.path,
            request.start,
            request.delete_len,
            &request.replacement,
        );
    match result {
        Ok(receipt) => {
            if let Err(error) = write_control_success(&paths.receipt, &request, &receipt) {
                if let Ok(mut workspace) = workspace.lock() {
                    workspace.mark_incomplete();
                }
                return Err(error);
            }
            Ok(())
        }
        Err(error) => {
            let lifecycle = workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_owned())?
                .lifecycle();
            let _ = budget.close_and_wait();
            let message = error.to_string();
            write_control_failure(&paths.receipt, lifecycle, &message)
                .map_err(|receipt_error| format!("{message}; {receipt_error}"))?;
            Err(message)
        }
    }
}
