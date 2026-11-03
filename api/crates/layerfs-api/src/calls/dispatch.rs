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
use layerfs_bridge::control::SdkValue;
pub use layerfs_control::common::context::Context;
use layerfs_control::common::outcome::{refused, Outcome};
use std::path::{Path, PathBuf};

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
    call_scoped(home_path, None, name, args, events)
}

/// [`call`] for a client with a folder scope (O32): the scope guards and
/// rewrites the checked arguments before any effect.
pub fn call_scoped(
    home_path: Option<&Path>,
    scope: Option<&super::Scope>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    let spec = table::find(name)
        .ok_or_else(|| refused("InvalidInput", format!("no call named `{name}`")))?;
    let mut args = SdkValue::Map(check::check(spec, args)?);
    if let Some(scope) = scope {
        args = scope.apply(name, args)?;
    }
    let home = home(home_path)?;
    let id = crate::home::open(&home)
        .inspect_err(|refusal| crate::home::note_failure(&home, refusal))?
        .id;
    // `machine/native/` and `machine/docker/` exist from the first call;
    // checked once per process for each home (R6)
    #[cfg(feature = "machine")]
    crate::machine::ensure_built_ins(&home, &id)?;
    #[cfg(not(feature = "machine"))]
    let _ = id;
    if let Some(scope) = scope {
        scope.owns(&home, name, &args)?;
    }
    let mut context = Context::new(home, events);
    let value = route::run(spec, &mut context, &args)?;
    // a scoped `branch.list` names its Branches relative to the namespace
    // in the family (it pages through them there)
    let value = match scope {
        Some(scope) if name == "branch.get" => {
            let prefix = format!("{}.", scope.branch.as_deref().unwrap_or(""));
            match value
                .get_text("name")
                .and_then(|n| n.strip_prefix(&prefix))
                .map(str::to_owned)
            {
                Some(name) => crate::home::with_field(value, "name", SdkValue::Text(name)),
                None => value,
            }
        }
        _ => value,
    };
    Ok(super::keys::snake(value))
}
