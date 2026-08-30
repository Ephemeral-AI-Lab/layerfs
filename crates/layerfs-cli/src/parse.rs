use crate::command::Invocation;
use crate::{CliError, CliResult, Command};
use clap::Parser;
use std::ffi::OsString;

pub(crate) enum CliArgv {
    Command(Command, bool),
    Display(String),
}

pub(crate) fn argv(arguments: impl IntoIterator<Item = OsString>) -> CliResult<(Command, bool)> {
    let invocation = invocation(arguments).map_err(|error| CliError::Parse(error.to_string()))?;
    Ok((invocation.command, invocation.json))
}

pub(crate) fn cli(arguments: Vec<OsString>) -> CliResult<CliArgv> {
    match invocation(arguments) {
        Ok(invocation) => Ok(CliArgv::Command(invocation.command, invocation.json)),
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            Ok(CliArgv::Display(error.to_string()))
        }
        Err(error) => Err(CliError::Parse(error.to_string())),
    }
}

fn invocation(arguments: impl IntoIterator<Item = OsString>) -> Result<Invocation, clap::Error> {
    let invocation =
        Invocation::try_parse_from(std::iter::once(OsString::from("layerfs")).chain(arguments))?;
    if matches!(
        invocation.command,
        Command::Db {
            command: crate::DbCommand::Create {
                role: crate::StoreRole::Layerstack,
                parent: Some(_),
                ..
            } | crate::DbCommand::Connect {
                role: crate::StoreRole::Layerstack,
                parent: Some(_),
                ..
            }
        }
    ) {
        return Err(clap::Error::raw(
            clap::error::ErrorKind::ArgumentConflict,
            "--parent is only valid for a BranchStore",
        ));
    }
    Ok(invocation)
}

pub(crate) fn line(input: &str) -> CliResult<Command> {
    argv(words(input)?.into_iter().map(OsString::from)).map(|value| value.0)
}

fn words(input: &str) -> CliResult<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            word.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                word.push(character);
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(character);
        }
    }
    if escaped || quote.is_some() {
        return Err(CliError::Parse("unterminated quote or escape".to_owned()));
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}
