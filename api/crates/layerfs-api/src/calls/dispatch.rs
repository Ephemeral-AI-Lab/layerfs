//! The one entry of the library: `call(home, name, args, events)`. Every
//! front end (the `layerfs` binary, Node and Python) makes exactly this
//! call, so results and errors are the same by construction. A machine's
//! worker serves its requests through `worker_server` (`ops::run`), not here, so
//! one call is dispatched, and logged, once.
//!
//! A call runs once. Its arguments are checked against the table before any
//! effect, then its family's function runs with a [`Context`]: the home and
//! the call's one event stream (O22), which carries `workspace.exec`'s
//! output and the progress of `project.init`, `project.export`, `store.move`,
//! `store.compact`, `backup.create` and `backup.restore`. Result keys are
//! snake_case in every front end. When it ends, one line goes to the home's
//! call log (#659), except for a call its table entry marks `quiet`, and
//! with `LAYERFS_LOGS=0`.
//!
//! Three arguments belong to no table entry and are taken out before the
//! check: `log_tag`, and the global options of the call surface (2.0),
//! `detail` (the long form) and `with` (the sections to add). A migrated
//! call's result is shaped by [`shape`](super::shape) into the form asked;
//! a refusal leaves with one of the thirteen public codes
//! ([`refusal`](super::refusal)).
use super::form::Asked;
use super::table::Call;
use super::{check, refusal, render, shape, table, table::Family, Context};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
/// The global option that asks for the long form; no table entry declares it.
pub const DETAIL: &str = "detail";
/// The global option that names the sections to add; no table entry
/// declares it. The shell takes `--with a,b`, Node `with: ["a", "b"]`,
/// Python `with_=["a", "b"]`.
pub const WITH: &str = "with";

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
/// call's log line, else `LAYERFS_LOGS_TAG` does; `detail` and `with` are
/// taken out the same way and say which form answers.
pub fn call_scoped(
    families: &'static [Family],
    home_path: Option<&Path>,
    scope: Option<&super::Scope>,
    name: &str,
    mut args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
    client: Client,
) -> Outcome {
    let mut log = Log::start(families, name, client, take_tag(&mut args));
    let outcome = layerfs_home::trace::stepped(name, || {
        scoped(families, home_path, scope, name, args, events, &mut log)
    });
    // a call refused for a nameless machine names the chain of what is on
    // it, read on the refusal path alone (UI2-8); a build without the
    // machine calls (a worker) has no machines
    #[cfg(feature = "machine")]
    let outcome = outcome.map_err(|refusal| match home(home_path) {
        Ok(home) => layerfs_control::machine::nameless_chain(&home, refusal),
        Err(_) => refusal,
    });
    // the log keeps the refusing code as it was raised; the caller gets its
    // public one
    log.finish(home_path, &outcome);
    outcome.map_err(refusal::public)
}

/// Logs `refusal` for a call refused before its arguments could be read
/// (a front end's bad input), as dispatch would have.
pub fn refuse_early(
    families: &'static [Family],
    home_path: Option<&Path>,
    name: &str,
    client: Client,
    refusal: SdkRefusal,
) -> Outcome {
    let outcome = Err(refusal);
    Log::start(families, name, client, None).finish(home_path, &outcome);
    outcome.map_err(refusal::public)
}

fn take(args: &mut SdkValue, name: &str) -> Option<SdkValue> {
    let SdkValue::Map(fields) = args else {
        return None;
    };
    let at = fields.iter().position(|(held, _)| held == name)?;
    Some(fields.remove(at).1)
}

/// The `log_tag` argument, taken out of `args`; given but unusable, the
/// call has no tag.
fn take_tag(args: &mut SdkValue) -> Option<Option<String>> {
    Some(match take(args, LOG_TAG)? {
        SdkValue::Text(tag) => layerfs_home::logs::tag(&tag),
        _ => None,
    })
}

/// Whether `args` asks for the long form (`detail: true`), without taking
/// it out: what a front end reads to print a refusal in the form asked.
pub fn asks_detail(args: &SdkValue) -> bool {
    args.get(DETAIL) == Some(&SdkValue::Bool(true))
}

/// The global options `detail` and `with`, taken out of `args` and checked
/// against the entry: `detail` is true or false; `with` is a list of
/// section names (or one text, names joined by `,`), each one the call
/// declares.
fn take_asked(spec: &Call, args: &mut SdkValue) -> Result<Asked, SdkRefusal> {
    let invalid = |why: String| refused("InvalidInput", format!("{}: {why}", spec.name()));
    let detail = match take(args, DETAIL) {
        None | Some(SdkValue::Null) => false,
        Some(SdkValue::Bool(detail)) => detail,
        Some(_) => return Err(invalid("`detail` must be true or false".to_owned())),
    };
    let named = |text: &str| -> Vec<String> {
        let names = text.split(',').map(str::trim);
        names.filter(|n| !n.is_empty()).map(str::to_owned).collect()
    };
    let mut with: Vec<String> = Vec::new();
    match take(args, WITH) {
        None | Some(SdkValue::Null) => {}
        Some(SdkValue::Text(text)) => with = named(&text),
        Some(SdkValue::List(items)) => {
            for item in items {
                match item {
                    SdkValue::Text(text) => with.extend(named(&text)),
                    _ => return Err(invalid("`with` must be a list of section names".to_owned())),
                }
            }
        }
        Some(_) => return Err(invalid("`with` must be a list of section names".to_owned())),
    }
    with.dedup();
    if let Some(unknown) = with.iter().find(|name| spec.section(name).is_none()) {
        let takes: Vec<&str> = spec.with.iter().map(|section| section.name).collect();
        return Err(invalid(match takes.is_empty() {
            true => format!("`with` names `{unknown}`, and this call takes no section"),
            false => format!(
                "`with` names `{unknown}`; this call takes {}",
                takes.join(", ")
            ),
        }));
    }
    Ok(Asked { detail, with })
}

/// One call's log line in the making: started when the call starts.
struct Log<'n> {
    name: &'n str,
    client: Client,
    tag: Option<String>,
    /// The call as the caller asked it, once its arguments were checked.
    argv: Option<Vec<SdkValue>>,
    /// The word and saved version its full result named.
    saved: Vec<(String, SdkValue)>,
    at: i64,
    started: Instant,
    on: bool,
}

impl<'n> Log<'n> {
    /// A call named `name` writes its line unless the log is off or its
    /// entry in `families` is `quiet`.
    fn start(
        families: &'static [Family],
        name: &'n str,
        client: Client,
        tag: Option<Option<String>>,
    ) -> Self {
        let quiet = table::find(families, name).is_some_and(|call| call.quiet);
        let on = layerfs_home::logs::enabled() && !quiet;
        let tag = tag.unwrap_or_else(|| {
            std::env::var("LAYERFS_LOGS_TAG")
                .ok()
                .and_then(|tag| layerfs_home::logs::tag(&tag))
        });
        Log {
            name,
            client,
            tag,
            argv: None,
            saved: Vec::new(),
            at: layerfs_home::home::now_ms(),
            started: Instant::now(),
            on,
        }
    }

    /// Appends the line once the call has its outcome; its `argv` is none
    /// for a call refused before its arguments were checked.
    fn finish(self, home_path: Option<&Path>, outcome: &Outcome) {
        if !self.on {
            return;
        }
        let Ok(home) = home(home_path) else {
            return;
        };
        let ms = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let argv = (self.argv).unwrap_or_else(|| render::bare(self.name));
        let line = render::line(
            self.at,
            ms,
            self.client.name(),
            argv,
            outcome,
            self.saved,
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
    log: &mut Log<'_>,
) -> Outcome {
    let spec = table::find(families, name)
        .ok_or_else(|| refused("InvalidInput", format!("no call named `{name}`")))?;
    let mut args = args;
    let asked = take_asked(spec, &mut args)?;
    let mut args = SdkValue::Map(check::check(spec, args)?);
    // logged as the caller asked, before the scope rewrites the arguments
    log.argv = Some(render::argv(spec, &args, &asked, scope));
    if let Some(scope) = scope {
        args = scope.apply(spec, args)?;
    }
    let home = home(home_path)?;
    super::platform_policy::check(&home, spec.policy, &args)?;
    if spec.opens {
        let id = layerfs_home::home::open(&home)?.id;
        // `machine/native/` and `machine/docker/` exist from the first call;
        // checked once per process for each home (R6)
        #[cfg(feature = "machine")]
        layerfs_machine::ensure_built_ins(&home, &id)?;
        #[cfg(not(feature = "machine"))]
        let _ = id;
    }
    if let Some(scope) = scope {
        scope.owns(&home, spec, &args)?;
    }
    layerfs_home::trace::step("dispatch");
    let mut context = Context::new(home, log.tag.clone(), asked.clone(), events);
    let value = (spec.run)(&mut context, &args)?;
    let value = match scope {
        Some(scope) => scope.answer(spec, value),
        None => value,
    };
    let value = super::keys::snake(value);
    // the line names what the full result said, before a form leaves it out
    log.saved = render::saved(spec, &value);
    Ok(match spec.form {
        Some(form) => shape::shape(&spec.name(), form, spec.with, value, &asked),
        None => value,
    })
}

/// What a process calls before it exits: closes every Store it holds open
/// with no call running (`close_idle`), so a sqlite Store leaves no
/// write-ahead log behind, and ends its idle Sandbox daemon sessions inside
/// a short bound (layerfs-project#181), so its exit is not a dropped
/// connection in a daemon's log. Answers how many Stores it closed. With
/// `LAYERFS_TRACE_STEP` set its steps are traced as op `close`: `stores`
/// and `sessions`.
pub fn close() -> usize {
    layerfs_home::trace::stepped("close", || {
        let closed = layerfs_control::store::close_idle(std::time::Duration::ZERO);
        layerfs_home::trace::step("stores");
        layerfs_sandbox::registry::end_idle_sessions(layerfs_sandbox::registry::SESSION_END);
        layerfs_home::trace::step("sessions");
        closed
    })
}
