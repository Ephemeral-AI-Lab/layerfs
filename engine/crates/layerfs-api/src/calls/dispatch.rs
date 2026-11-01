//! The one entry of the library: `call(home, name, args, events)`. Every
//! front end (the `layerfs` binary, Node, Python and the guest worker) makes
//! exactly this call; so results and errors are the same by construction.
//!
//! A call runs once. Its arguments are checked against the table before any
//! effect, then its family's function runs with a [`Context`]: the home and
//! the call's one event stream (O22), which carries `workspace.exec`'s
//! output and the progress of `project.init`, `store.move`, `backup.create`
//! and `backup.restore`. Result keys are snake_case in every front end.
use super::{check, route, table};
use crate::host::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

/// What one running call can reach: its home and its event stream.
pub struct Context<'a> {
    /// The home every record lives in.
    pub home: PathBuf,
    events: &'a mut dyn FnMut(SdkValue),
}

impl Context<'_> {
    /// Sends one progress update, `{ progress: value }`.
    pub fn progress(&mut self, value: SdkValue) {
        (self.events)(SdkValue::map([("progress", value)]));
    }

    /// Sends one piece of a command's output, `{ stream, bytes }`.
    pub fn output(&mut self, stream: &str, bytes: &[u8]) {
        (self.events)(SdkValue::map([
            ("stream", SdkValue::text(stream)),
            ("bytes", SdkValue::Bytes(bytes.to_vec())),
        ]));
    }
}

/// The home a call uses: `explicit` when given, else `LAYERFS_HOME`, else
/// `~/.layerfs` (O28). It must be an absolute path.
pub fn home(explicit: Option<&Path>) -> Result<PathBuf, layerfs_bridge::control::SdkRefusal> {
    let home = match explicit {
        Some(path) => path.to_owned(),
        None => match std::env::var_os("LAYERFS_HOME") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".layerfs"))
                .ok_or_else(|| refused("InvalidInput", "LAYERFS_HOME is unset and so is HOME"))?,
        },
    };
    if !home.is_absolute() {
        return Err(refused(
            "InvalidInput",
            "LAYERFS_HOME must be an absolute path",
        ));
    }
    Ok(home)
}

/// Runs the call `name` (`family.call`) once with named `args`.
pub fn call(
    home_path: Option<&Path>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    let spec = table::find(name)
        .ok_or_else(|| refused("InvalidInput", format!("no call named `{name}`")))?;
    let args = SdkValue::Map(check::check(spec, args)?);
    let mut context = Context {
        home: home(home_path)?,
        events,
    };
    let value = route::run(spec, &mut context, &args)?;
    Ok(super::keys::snake(value))
}
