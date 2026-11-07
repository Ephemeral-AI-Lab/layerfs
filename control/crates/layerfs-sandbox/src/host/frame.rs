//! The host operation's values (layerfs-project#108), the one place its
//! typed request meets them: the `sandbox.host` frame a caller sends a
//! machine's worker ([`Sent`], [`encode`], [`decode`]), and the one args
//! map ([`Host::to_args`]) for what still takes a map: the daemon's
//! `sandbox.list_workspaces`. Both
//! are built by the same field encoder ([`fields`]), in the order the
//! untyped request had. What opens its Store is [`Host::open_args`].
use super::call::{Call, Deploy, Host, Message, Op, Started};
use crate::deploy::Choice;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use layerfs_store::open_args::{OpenArgs, PgSelection};
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;

/// One host operation as a caller sends it to its machine's worker.
pub struct Sent {
    /// The operation and its fields.
    pub call: Call,
    /// Its Commit message.
    pub message: Message,
    /// The Sandbox it names.
    pub sandbox: Option<String>,
    /// The Store's name, as sent (the worker parses it).
    pub store: String,
    /// The project's name, as sent (the worker parses it).
    pub project: String,
    /// The Store's record; the worker drops its URL and password reference.
    pub store_record: StoreRecord,
    /// The project's record (from nothing when the caller has none).
    pub project_record: ProjectRecord,
    /// What the machine's record chooses for its Sandboxes.
    pub choice: Choice,
    /// A PostgreSQL Store's server, resolved by the caller (#535 §7.4).
    pub postgresql: Option<PgSelection>,
    /// The machine a PostgreSQL reach check names.
    pub machine: Option<String>,
    /// What a failed reach check suggests.
    pub reach_hint: Option<String>,
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn opt(value: &Option<String>) -> SdkValue {
    SdkValue::opt_text(value.clone())
}

fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

fn message(message: &Message) -> Option<SdkValue> {
    match message {
        Message::Absent => None,
        Message::Text(text) => Some(SdkValue::text(text)),
        Message::Null => Some(SdkValue::Null),
        // never in a frame ([`encode`] refuses it); the args map's
        // layerfs-store calls read no message
        Message::NotText => Some(SdkValue::Null),
    }
}

/// An operation's own fields, its message and the Sandbox it names, in the
/// order the untyped request had them.
fn fields(call: &Call, sent: &Message, sandbox: Option<&str>) -> Vec<(&'static str, SdkValue)> {
    let mut fields = Vec::new();
    let mut said = false;
    match call {
        Call::Create { platform } => {
            fields.extend(platform.as_deref().map(|p| ("platform", text(p))))
        }
        Call::Close { force, held } => {
            fields.push(("force", SdkValue::Bool(*force)));
            if let Some(held) = held {
                let held = held.iter().map(|path| text(path)).collect();
                fields.push(("held", SdkValue::List(held)));
            }
        }
        Call::ListWorkspaces { path } | Call::Status { path } => {
            fields.extend(path.as_deref().map(|p| ("path", text(p))))
        }
        Call::Mount { branch, name } => {
            fields.push(("branch", opt(branch)));
            fields.extend(name.as_deref().map(|n| ("name", text(n))));
        }
        Call::Exec {
            path,
            directory,
            command,
            spool,
        } => {
            fields.push(("path", opt(path)));
            fields.extend(directory.as_deref().map(|d| ("directory", text(d))));
            fields.push(("command", opt(command)));
            fields.extend(message(sent).map(|m| ("message", m)));
            said = true;
            if *spool {
                fields.push(("spool", SdkValue::Bool(true)));
            }
        }
        Call::Commit { path, force } => {
            fields.push(("path", opt(path)));
            fields.extend(message(sent).map(|m| ("message", m)));
            said = true;
            fields.extend(force.map(|f| ("force", SdkValue::Bool(f))));
        }
        Call::Unmount {
            path,
            discard,
            force,
        } => {
            fields.push(("path", opt(path)));
            fields.extend(discard.map(|d| ("discard", SdkValue::Bool(d))));
            fields.extend(force.map(|f| ("force", SdkValue::Bool(f))));
        }
    }
    if !said {
        fields.extend(message(sent).map(|m| ("message", m)));
    }
    fields.extend(sandbox.map(|s| ("sandbox", text(s))));
    fields
}

/// The `sandbox.host` frame's arguments for `sent`, its message checked
/// first as the worker checks it (`dispatch::checked`, every operation but
/// a status): a message that is not text, or not a valid Commit message, is
/// refused before it is sent.
pub fn encode(sent: &Sent) -> Result<SdkValue, SdkRefusal> {
    if sent.call.op() != Op::Status {
        crate::dispatch::checked(&sent.message)?;
    }
    let mut frame = vec![
        ("op", text(sent.call.op().name())),
        ("store", text(&sent.store)),
        ("project", text(&sent.project)),
        ("store_record", sent.store_record.to_record().clone()),
        ("project_record", sent.project_record.to_record().clone()),
        ("deploy", choice(&sent.choice)),
    ];
    if let Some(postgresql) = &sent.postgresql {
        frame.push(("postgresql", postgresql.encode()));
        // what the worker's reach check names and suggests
        frame.extend(sent.machine.as_deref().map(|m| ("machine", text(m))));
        frame.extend(sent.reach_hint.as_deref().map(|h| ("reach_hint", text(h))));
    }
    let more = fields(&sent.call, &sent.message, sent.sandbox.as_deref());
    frame.push(("more", map(more)));
    Ok(map(frame))
}

/// What machine `choice` encodes to: `{ backend: "local", image?,
/// platform? | archive? }`.
fn choice(choice: &Choice) -> SdkValue {
    let mut fields = vec![("backend", text("local"))];
    fields.extend(choice.image.as_deref().map(|i| ("image", text(i))));
    fields.extend(choice.platform.as_deref().map(|p| ("platform", text(p))));
    fields.extend(choice.archive.as_deref().map(|a| ("archive", text(a))));
    map(fields)
}

/// The host operation a worker is sent (`sandbox.host`'s arguments
/// `frame`): refused before any effect when it names no operation the
/// worker runs. Every other field is read as sent and checked where it is
/// used.
pub fn decode(frame: &SdkValue) -> Result<Sent, SdkRefusal> {
    let op = frame
        .get_text("op")
        .ok_or_else(|| refused("InvalidInput", "operation name"))?;
    let Some(op) = Op::parse(op) else {
        return Err(refused(
            "InvalidInput",
            format!("the worker runs no Sandbox operation {op}"),
        ));
    };
    let store = frame.get_text("store").unwrap_or("").to_owned();
    let project = frame.get_text("project").unwrap_or("").to_owned();
    // a PostgreSQL Store's URL and password reference stay with the caller:
    // the request's `postgresql` carries what this side dials (#535 §7.4)
    let record = match frame.get("store_record").cloned().unwrap_or_default() {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(n, _)| !matches!(n.as_str(), "url" | "password"))
                .collect(),
        ),
        other => other,
    };
    let sent_project = frame.get("project_record").cloned().unwrap_or_default();
    let more = frame.get("more").cloned().unwrap_or_default();
    let deploy = frame.get("deploy").cloned().unwrap_or_default();
    Ok(Sent {
        call: call(op, &more),
        message: Message::read(more.get("message")),
        sandbox: more.get_text("sandbox").map(str::to_owned),
        store_record: StoreRecord::from_record(&store, record),
        project_record: ProjectRecord::from_record(&store, &project, sent_project),
        store,
        project,
        choice: Choice {
            image: deploy.get_text("image").map(str::to_owned),
            platform: deploy.get_text("platform").map(str::to_owned),
            archive: deploy.get_text("archive").map(str::to_owned),
        },
        postgresql: frame.get("postgresql").map(PgSelection::read),
        machine: frame.get_text("machine").map(str::to_owned),
        reach_hint: frame.get_text("reach_hint").map(str::to_owned),
    })
}

/// Operation `op`'s fields from a request's `more`, read as the untyped
/// request read them.
fn call(op: Op, more: &SdkValue) -> Call {
    let text = |name: &str| more.get_text(name).map(str::to_owned);
    let flag = |name: &str| more.get(name).map(|_| more.get_bool(name));
    match op {
        Op::SandboxCreate => Call::Create {
            platform: text("platform"),
        },
        Op::SandboxClose => Call::Close {
            force: more.get_bool("force"),
            held: match more.get("held") {
                Some(SdkValue::List(items)) => Some(
                    (items.iter())
                        .filter_map(|item| match item {
                            SdkValue::Text(path) => Some(path.clone()),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            },
        },
        Op::ListWorkspaces => Call::ListWorkspaces { path: text("path") },
        Op::Mount => Call::Mount {
            branch: text("branch"),
            name: text("name"),
        },
        Op::Exec => Call::Exec {
            path: text("path"),
            directory: text("directory"),
            command: text("command"),
            spool: more.get_bool("spool"),
        },
        Op::Commit => Call::Commit {
            path: text("path"),
            force: flag("force"),
        },
        Op::Unmount => Call::Unmount {
            path: text("path"),
            discard: flag("discard"),
            force: flag("force"),
        },
        Op::Status => Call::Status { path: text("path") },
    }
}

/// A deployment as a map, as the untyped request kept it.
fn deploy(deploy: &Deploy) -> SdkValue {
    let mut fields = Vec::new();
    for (name, value) in [
        ("sandbox", &deploy.sandbox),
        ("localDir", &deploy.local_dir),
        ("containerPrefix", &deploy.container_prefix),
        ("daemon", &deploy.daemon),
        ("imageArchive", &deploy.image_archive),
        ("image", &deploy.image),
        ("platform", &deploy.platform),
    ] {
        fields.extend(value.as_deref().map(|v| (name, text(v))));
    }
    map(fields)
}

impl Message {
    /// A request's `message` field as sent (`None`: not sent).
    pub fn read(value: Option<&SdkValue>) -> Self {
        match value {
            None => Self::Absent,
            Some(SdkValue::Null) => Self::Null,
            Some(SdkValue::Text(text)) => Self::Text(text.clone()),
            Some(_) => Self::NotText,
        }
    }
}

impl Host {
    /// What opens its Store in this process (layerfs-project#109): the
    /// fields [`Host::to_args`] gives them.
    pub fn open_args(&self) -> OpenArgs {
        OpenArgs {
            home: Some(self.home.clone()),
            store: Some(self.store.clone()),
            project: Some(self.project.clone()),
            store_path: self.store_path.clone(),
            store_record: self.store_record.as_ref().map(|r| r.to_record().clone()),
            postgresql: self.postgresql.clone(),
        }
    }

    /// The host operation `call` as one args map, for what still takes
    /// one: the daemon's `sandbox.list_workspaces`. Its fields are the ones
    /// the untyped request had.
    pub fn to_args(&self, call: &Call) -> SdkValue {
        let mut fields = vec![
            ("home", text(&self.home.display().to_string())),
            ("store", text(&self.store)),
            ("project", text(&self.project)),
        ];
        if let Some(record) = &self.store_record {
            fields.push(("store_record", record.to_record().clone()));
        }
        fields.extend(self.store_path.as_deref().map(|p| ("storePath", text(p))));
        fields.extend(self.deploy.as_ref().map(|d| ("deploy", deploy(d))));
        // a PostgreSQL Store's URL with its password, never written (H §7.4)
        if let Some(postgresql) = &self.postgresql {
            fields.push(("postgresql", postgresql.encode()));
        }
        fields.extend(self::fields(call, &self.message, self.sandbox.as_deref()));
        map(fields)
    }
}

impl Started {
    /// A `sandbox.create` answer `value`; a field that is not text is
    /// `None`.
    pub fn read(value: &SdkValue) -> Self {
        let text = |name: &str| value.get_text(name).map(str::to_owned);
        Self {
            sandbox: text("sandbox"),
            name: text("name"),
            image: text("image"),
            platform: text("platform"),
            runtime: text("runtime"),
            route: text("route"),
            runtime_id: text("runtime_id"),
            control: value.get("control").cloned(),
        }
    }

    /// The answer, in the order the untyped answer had, `control` last.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("sandbox", opt(&self.sandbox)),
            ("name", opt(&self.name)),
            ("image", opt(&self.image)),
            ("platform", opt(&self.platform)),
            ("runtime", opt(&self.runtime)),
            ("route", opt(&self.route)),
            ("runtime_id", opt(&self.runtime_id)),
        ];
        fields.extend(self.control.clone().map(|keys| ("control", keys)));
        map(fields)
    }
}
