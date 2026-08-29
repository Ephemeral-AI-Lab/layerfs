use crate::{Command, CommandSummary};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    Read,
    Mutate,
    Execute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub command: CommandSummary,
    pub effect: CommandEffect,
    pub route: Vec<String>,
    pub confirmation_required: bool,
}

pub(crate) fn effect(command: &Command) -> CommandEffect {
    match command {
        Command::Db {
            command: crate::DbCommand::List,
        }
        | Command::Layer {
            command: crate::LayerCommand::List | crate::LayerCommand::Show { .. },
        }
        | Command::Stack {
            command: crate::StackCommand::List | crate::StackCommand::Show { .. },
        }
        | Command::Branch {
            command:
                crate::BranchCommand::List
                | crate::BranchCommand::Show { .. }
                | crate::BranchCommand::Diff { .. },
        }
        | Command::Workspace {
            command:
                crate::WorkspaceCommand::List
                | crate::WorkspaceCommand::Show { .. }
                | crate::WorkspaceCommand::Diff { .. }
                | crate::WorkspaceCommand::Output { .. },
        } => CommandEffect::Read,
        Command::Workspace {
            command: crate::WorkspaceCommand::Exec { .. } | crate::WorkspaceCommand::Shell { .. },
        } => CommandEffect::Execute,
        Command::Monitor { .. } => CommandEffect::Read,
        _ => CommandEffect::Mutate,
    }
}

pub(crate) fn put_plan(
    output: &mut crate::control::WireWriter,
    plan: &CommandPlan,
) -> crate::CliResult<()> {
    output.string(&plan.command.0)?;
    output.byte(match plan.effect {
        CommandEffect::Read => 0,
        CommandEffect::Mutate => 1,
        CommandEffect::Execute => 2,
    });
    output.u32(
        plan.route
            .len()
            .try_into()
            .map_err(|_| crate::CliError::Context("plan route length".to_owned()))?,
    );
    plan.route
        .iter()
        .try_for_each(|value| output.string(value))?;
    output.bool(plan.confirmation_required);
    Ok(())
}

pub(crate) fn get_plan(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<CommandPlan> {
    let command = crate::CommandSummary(input.string()?);
    let effect = match input.byte()? {
        0 => CommandEffect::Read,
        1 => CommandEffect::Mutate,
        2 => CommandEffect::Execute,
        _ => return Err(crate::CliError::Context("plan effect".to_owned())),
    };
    let route = (0..input.count()?)
        .map(|_| input.string())
        .collect::<crate::CliResult<Vec<_>>>()?;
    Ok(CommandPlan {
        command,
        effect,
        route,
        confirmation_required: input.bool()?,
    })
}
