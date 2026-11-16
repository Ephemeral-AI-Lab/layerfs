//! `logs.machine` and `logs.sandbox`: a machine's worker log and a
//! Sandbox's daemon log, one window per call (layerfs-project#178), read
//! with `layerfs_home::tail` on the side that holds the file. They read
//! `worker.log` with `worker.1.log`, and `daemon.log`, and nothing else:
//! never `log/uncertain.jsonl` (#179 D8) or any other file.
use super::Side;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::tail::{self, Pair};
use layerfs_store::place::Executor;
use std::path::Path;

/// The file a Sandbox's daemon writes in its run folder (the backends'
/// `LOG`): made when the daemon starts, never rotated, removed with the
/// folder when the Sandbox closes.
const DAEMON_LOG: &str = "daemon.log";

/// Which window of a log a call asks for.
#[derive(Clone, Debug, Default)]
pub struct Window {
    /// An answer's `cursor`: the lines after it.
    pub cursor: Option<String>,
    /// An answer's `before`: the lines before it.
    pub before: Option<String>,
    /// At most this many lines.
    pub limit: Option<i64>,
}

impl Window {
    /// The window `args` name.
    pub fn of(args: &SdkValue) -> Self {
        Self {
            cursor: args.get_text("cursor").map(str::to_owned),
            before: args.get_text("before").map(str::to_owned),
            limit: args.get_int("limit"),
        }
    }

    /// The request of inspect `kind` for this window, with `more` fields.
    fn request(&self, more: Option<(&str, &str)>) -> SdkValue {
        let mut fields = Vec::new();
        fields.extend(more.map(|(name, value)| (name.to_owned(), SdkValue::text(value))));
        let text = |value: &Option<String>| value.clone().map(SdkValue::Text);
        fields.extend(text(&self.cursor).map(|value| ("cursor".to_owned(), value)));
        fields.extend(text(&self.before).map(|value| ("before".to_owned(), value)));
        fields.extend(
            self.limit
                .map(|limit| ("limit".to_owned(), SdkValue::Int(limit))),
        );
        SdkValue::Map(fields)
    }

    fn read(&self, pair: &Pair, call: &str) -> Outcome {
        tail::read(
            pair,
            call,
            self.cursor.as_deref(),
            self.before.as_deref(),
            self.limit,
        )
    }
}

/// `Unsupported` `{reason: 'no_worker'}`: the machine's calls run in the
/// caller's process, so it has no worker and no worker log.
fn no_worker(machine: &str) -> SdkRefusal {
    SdkRefusal {
        detail: SdkValue::map([("reason", SdkValue::text("no_worker"))]),
        ..refused(
            "Unsupported",
            format!(
                "machine {machine} has no worker (its calls run in the caller's process), so it has no worker log"
            ),
        )
    }
}

/// `logs.machine(machine, { cursor, before, limit })`: one window of the
/// machine's worker log, `{ lines: [{ text, continued? }], cursor,
/// before?, has_more }` (`layerfs_home::tail::read`). A machine with no
/// worker (every `native`) is `Unsupported`; one whose worker does not
/// answer is `Unavailable`; nothing is started.
#[cfg(unix)]
pub fn machine_log(home: &Path, machine: &MachineName, window: &Window) -> Outcome {
    let machine = machine.as_str();
    tail::check(
        "logs.machine",
        window.cursor.is_some(),
        window.before.is_some(),
        window.limit,
    )?;
    if let Executor::Local = super::on_owner(home, machine)? {
        return Err(no_worker(machine));
    }
    super::ask(home, machine, MACHINE_LOG, window.request(None))
}

/// `logs.sandbox(sandbox, { cursor, before, limit })`: one window of the
/// log of Sandbox `name`'s daemon, shaped as [`machine_log`]'s. A name
/// with no record is `NoSuchSandbox`, and so is a Sandbox whose machine
/// holds no run folder for it; a machine whose worker does not answer is
/// `Unavailable`. A daemon that has ended leaves its log readable while
/// its run folder is there.
#[cfg(unix)]
pub fn sandbox_log(home: &Path, name: &str, window: &Window) -> Outcome {
    tail::check(
        "logs.sandbox",
        window.cursor.is_some(),
        window.before.is_some(),
        window.limit,
    )?;
    let record = crate::family::sandbox_of(home, name)?;
    let request = window.request(Some(("sandbox", name)));
    super::ask(home, record.machine(), SANDBOX_LOG, request)
}

/// The inspect kind of [`machine_log`].
pub(super) const MACHINE_LOG: &str = "machine_log";
/// The inspect kind of [`sandbox_log`].
pub(super) const SANDBOX_LOG: &str = "sandbox_log";

/// [`machine_log`] on the side that holds the file: a worker's root. A
/// worker started by a build that wrote no `log/worker.log` answers an
/// empty log.
pub(super) fn machine(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let Side::Worker(root) = side else {
        return Err(no_worker("native"));
    };
    Window::of(args).read(&Pair::of(paths::worker_log(root)), "logs.machine")
}

/// [`sandbox_log`] on the side that holds the Sandbox's run folder.
#[cfg(unix)]
pub(super) fn sandbox(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let folder = super::run_folder(side, args.get_text("sandbox").unwrap_or(""))?;
    Window::of(args).read(&Pair::of(folder.join(DAEMON_LOG)), "logs.sandbox")
}
