use crate::{CliResult, Command, CommandResult};
use std::path::Path;

pub(crate) fn run(
    context: &Path,
    client: Option<layerfs_sdk::Client>,
    command: Command,
    error_context: String,
    emit: &mut dyn FnMut(crate::CliEvent) -> CliResult<()>,
) -> CliResult<CommandResult> {
    crate::host::execute(context, client, command, error_context, emit)
}
