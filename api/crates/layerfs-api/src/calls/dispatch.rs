//! The one entry of the library: `call(home, name, args, events)`. Every
//! front end (the `layerfs` binary, Node and Python) makes exactly this
//! call, so results and errors are the same by construction. A machine's
//! worker serves its requests through `worker_server::run`, not here, so
//! one call is dispatched, and logged, once.
//!
//! A call runs once. Its arguments are checked against the table before any
//! effect, then its family's function runs with a [`Context`]: the home and
//! the call's one event stream (O22), which carries `workspace.exec`'s
//! output and the progress of `project.init`, `project.export`, `store.push`,
//! `store.pull`, `backup.create` and `backup.restore`. Result keys are
//! snake_case in every front end. When it ends, one line goes to the home's
//! call log (#659), except for the `logs` calls and with `LAYERFS_LOGS=0`.
use super::{check, render, table, table::Family, Context};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Which front end made a call, as its call log line names it (#659 L8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    /// The `layerfs` command.
    Cli,
    /// The Node binding.
    Node,
    /// The Python binding.
    Python,
    /// A direct library call (`layerfs_api::call`, tests, embedders).
    Rust,
}

impl Client {
    /// Its name in a log line: `cli`, `node`, `python` or `rust`.
    pub fn name(self) -> &'static str {
        match self {
            Client::Cli => "cli",
            Client::Node => "node",
            Client::Python => "python",
            Client::Rust => "rust",
        }
    }
}

/// The argument that tags one call's log line; no table entry declares it.
const LOG_TAG: &str = "log_tag";

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

/// Runs the call `name` (`family.call`) of `families` once with named
/// `args`, as a direct library call.
pub fn call(
    families: &'static [Family],
    home_path: Option<&Path>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    call_scoped(families, home_path, None, name, args, events, Client::Rust)
}

/// [`call`] for a client with a folder scope (O32): the scope guards and
/// rewrites the checked arguments before any effect. With
/// `LAYERFS_TRACE_STEP` set, its steps are traced (`trace::stepped`). A
/// top-level `log_tag` argument is taken out before checking and tags the
/// call's log line, else `LAYERFS_LOGS_TAG` does.
pub fn call_scoped(
    families: &'static [Family],
    home_path: Option<&Path>,
    scope: Option<&super::Scope>,
    name: &str,
    mut args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
    client: Client,
) -> Outcome {
    let log = Log::start(name, client, take_tag(&mut args));
    let mut argv = None;
    let outcome = layerfs_home::trace::stepped(name, || {
        scoped(families, home_path, scope, name, args, events, &mut argv)
    });
    log.finish(home_path, argv, &outcome);
    outcome
}

/// Logs `refusal` for a call refused before its arguments could be read
/// (a front end's bad input), as dispatch would have.
pub fn refuse_early(
    home_path: Option<&Path>,
    name: &str,
    client: Client,
    refusal: layerfs_bridge::control::SdkRefusal,
) -> Outcome {
    let outcome = Err(refusal);
    Log::start(name, client, None).finish(home_path, None, &outcome);
    outcome
}

/// The `log_tag` argument, taken out of `args`; given but unusable, the
/// call has no tag.
fn take_tag(args: &mut SdkValue) -> Option<Option<String>> {
    let SdkValue::Map(fields) = args else {
        return None;
    };
    let at = fields.iter().position(|(name, _)| name == LOG_TAG)?;
    Some(match fields.remove(at).1 {
        SdkValue::Text(tag) => layerfs_home::logs::tag(&tag),
        _ => None,
    })
}

/// One call's log line in the making: started when the call starts.
struct Log<'n> {
    name: &'n str,
    client: Client,
    tag: Option<String>,
    at: i64,
    started: Instant,
    on: bool,
}

impl<'n> Log<'n> {
    fn start(name: &'n str, client: Client, tag: Option<Option<String>>) -> Self {
        let on = layerfs_home::logs::enabled() && name.split('.').next() != Some("logs");
        let tag = tag.unwrap_or_else(|| {
            std::env::var("LAYERFS_LOGS_TAG")
                .ok()
                .and_then(|tag| layerfs_home::logs::tag(&tag))
        });
        Log {
            name,
            client,
            tag,
            at: layerfs_home::home::now_ms(),
            started: Instant::now(),
            on,
        }
    }

    /// Appends the line once the call has its outcome; `argv` is none for a
    /// call refused before its arguments were checked.
    fn finish(self, home_path: Option<&Path>, argv: Option<Vec<SdkValue>>, outcome: &Outcome) {
        if !self.on {
            return;
        }
        let Ok(home) = home(home_path) else {
            return;
        };
        let ms = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let argv = argv.unwrap_or_else(|| render::bare(self.name));
        let line = render::line(
            self.at,
            ms,
            self.client.name(),
            argv,
            self.name,
            outcome,
            self.tag.as_deref(),
        );
        layerfs_home::logs::append(&home, &line);
    }
}

fn scoped(
    families: &'static [Family],
    home_path: Option<&Path>,
    scope: Option<&super::Scope>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
    argv: &mut Option<Vec<SdkValue>>,
) -> Outcome {
    let spec = table::find(families, name)
        .ok_or_else(|| refused("InvalidInput", format!("no call named `{name}`")))?;
    let mut args = SdkValue::Map(check::check(spec, args)?);
    // logged as the caller asked, before the scope rewrites the arguments
    *argv = Some(render::argv(spec, &args, scope));
    if let Some(scope) = scope {
        args = scope.apply(name, args)?;
    }
    let home = home(home_path)?;
    super::platform_policy::check(&home, name, &args)?;
    let id = layerfs_control::migrate::open(&home)
        .inspect_err(|refusal| layerfs_control::migrate::note_failure(&home, refusal))?
        .id;
    // `machine/native/` and `machine/docker/` exist from the first call;
    // checked once per process for each home (R6)
    #[cfg(feature = "machine")]
    layerfs_machine::ensure_built_ins(&home, &id)?;
    #[cfg(not(feature = "machine"))]
    let _ = id;
    if let Some(scope) = scope {
        scope.owns(&home, name, &args)?;
    }
    layerfs_home::trace::step("dispatch");
    let mut context = Context::new(home, events);
    let value = (spec.run)(&mut context, &args)?;
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
                Some(name) => layerfs_home::home::with_field(value, "name", SdkValue::Text(name)),
                None => value,
            }
        }
        _ => value,
    };
    Ok(super::keys::snake(value))
}
