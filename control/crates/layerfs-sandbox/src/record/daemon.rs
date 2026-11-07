//! What reopens a started Sandbox: its record's `daemon` (layerfs-project
//! #108, typed). On the side that runs it (`native`'s home, a machine's
//! worker) it names the local daemon or init process, its directory, port,
//! identity, image and labels (`files::save_sandbox`); in a home that
//! reaches a worker's Sandbox directly (#535 G4) it names the forward of
//! its daemon's port and what its create answered (`direct::open`). A
//! record of the retired docker backend has no `runtime` (D-8). As the
//! Sandbox record does, it keeps unknown, null and mistyped fields as read,
//! in file order.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::home::fields::{self, Fields};
use std::net::SocketAddr;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The fields a daemon record knows.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    Platform(String),
    Name(String),
    Container(String),
    Runtime(String),
    Uid(i64),
    Gid(i64),
    Directory(String),
    Pid(i64),
    Start(String),
    Boot(String),
    Port(i64),
    Image(String),
    Labels(Vec<(String, String)>),
    Forward(Forward),
}

/// Labels stored as `[{ key, value }]`, each exactly those two texts.
fn labels(value: &SdkValue) -> Option<Vec<(String, String)>> {
    let SdkValue::List(items) = value else {
        return None;
    };
    (items.iter())
        .map(|item| match item {
            SdkValue::Map(fields) => match fields.as_slice() {
                [(k, SdkValue::Text(key)), (v, SdkValue::Text(value))]
                    if k == "key" && v == "value" =>
                {
                    Some((key.clone(), value.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

impl fields::Field for Field {
    fn decode(name: &str, value: &SdkValue) -> Option<Self> {
        let text = || fields::text(value);
        let int = || fields::int(value);
        Some(match name {
            "platform" => Self::Platform(text()?),
            "name" => Self::Name(text()?),
            "container" => Self::Container(text()?),
            "runtime" => Self::Runtime(text()?),
            "uid" => Self::Uid(int()?),
            "gid" => Self::Gid(int()?),
            "directory" => Self::Directory(text()?),
            "pid" => Self::Pid(int()?),
            "start" => Self::Start(text()?),
            "boot" => Self::Boot(text()?),
            "port" => Self::Port(int()?),
            "image" => Self::Image(text()?),
            "labels" => Self::Labels(labels(value)?),
            "forward" => Self::Forward(Forward::decode(value)?),
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Platform(_) => "platform",
            Self::Name(_) => "name",
            Self::Container(_) => "container",
            Self::Runtime(_) => "runtime",
            Self::Uid(_) => "uid",
            Self::Gid(_) => "gid",
            Self::Directory(_) => "directory",
            Self::Pid(_) => "pid",
            Self::Start(_) => "start",
            Self::Boot(_) => "boot",
            Self::Port(_) => "port",
            Self::Image(_) => "image",
            Self::Labels(_) => "labels",
            Self::Forward(_) => "forward",
        }
    }

    fn encode(&self) -> SdkValue {
        match self {
            Self::Uid(n) | Self::Gid(n) | Self::Pid(n) | Self::Port(n) => SdkValue::Int(*n),
            Self::Labels(labels) => SdkValue::List(
                (labels.iter())
                    .map(|(key, value)| {
                        SdkValue::map([
                            ("key", SdkValue::text(key)),
                            ("value", SdkValue::text(value)),
                        ])
                    })
                    .collect(),
            ),
            Self::Forward(forward) => forward.fields.encode(),
            Self::Platform(text)
            | Self::Name(text)
            | Self::Container(text)
            | Self::Runtime(text)
            | Self::Directory(text)
            | Self::Start(text)
            | Self::Boot(text)
            | Self::Image(text) => SdkValue::text(text),
        }
    }
}

/// A local daemon or init process as its Sandbox's side records it.
pub struct Local {
    /// The platform the call named (#529).
    pub platform: String,
    /// Its directory's name, `<prefix><id>`.
    pub name: String,
    /// Its runtime id.
    pub container: String,
    /// `native` or `namespaces`.
    pub runtime: &'static str,
    /// Its commands' identity.
    pub uid: u32,
    /// Its commands' group.
    pub gid: u32,
    /// Its directory.
    pub directory: String,
    /// Its process id, start time and boot (32 hex digits).
    pub process: (u32, u64, String),
    /// Its listener port.
    pub port: u16,
    /// The image it runs (`local` for native).
    pub image: String,
    /// Its labels.
    pub labels: Vec<(String, String)>,
}

/// One Sandbox's daemon record.
#[derive(Clone, Debug, PartialEq)]
pub struct DaemonRecord {
    fields: Fields<Field>,
}

impl DaemonRecord {
    /// The daemon record `value` holds, when it is a map (the codec's read).
    pub fn decode(value: &SdkValue) -> Option<Self> {
        matches!(value, SdkValue::Map(_)).then(|| Self::from_record(value))
    }

    /// Daemon record `value` (a map; anything else has no fields).
    pub fn from_record(value: &SdkValue) -> Self {
        Self {
            fields: Fields::decode(value),
        }
    }

    /// The record as written (the codec's write).
    pub fn to_record(&self) -> SdkValue {
        self.fields.encode()
    }

    /// A local daemon (native) or init (namespaces) process.
    pub fn local(local: Local) -> Self {
        let (pid, start, boot) = local.process;
        Self {
            fields: Fields::of([
                Field::Platform(local.platform),
                Field::Name(local.name),
                Field::Container(local.container),
                Field::Runtime(local.runtime.to_owned()),
                Field::Uid(i64::from(local.uid)),
                Field::Gid(i64::from(local.gid)),
                Field::Directory(local.directory),
                Field::Pid(i64::from(pid)),
                Field::Start(start.to_string()),
                Field::Boot(boot),
                Field::Port(i64::from(local.port)),
                Field::Image(local.image),
                Field::Labels(local.labels),
            ]),
        }
    }

    /// A worker's Sandbox reached through `forward`, with what its create
    /// answered (`started`): its `name`, `runtime`, `platform` and `image`,
    /// each null when it gave none.
    pub fn forwarded(forward: Forward, started: &SdkValue) -> Self {
        let mut fields = Fields::of([Field::Forward(forward)]);
        for name in ["name", "runtime", "platform", "image"] {
            fields.put(name, started.get(name).cloned().unwrap_or_default());
        }
        Self { fields }
    }

    fn text(&self, pick: fn(&Field) -> Option<&String>) -> Option<&str> {
        self.fields.find(pick).map(String::as_str)
    }

    fn int(&self, pick: fn(&Field) -> Option<i64>) -> Option<i64> {
        self.fields.get(pick)
    }

    /// The platform it names (#529).
    pub fn platform(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Platform(text) => Some(text),
            _ => None,
        })
    }

    /// Its directory's (or the docker backend's container's) name.
    pub fn name(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Name(text) => Some(text),
            _ => None,
        })
    }

    /// Its runtime id.
    pub fn container(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Container(text) => Some(text),
            _ => None,
        })
    }

    /// Its runtime: `native`, `namespaces`, or none for the retired docker
    /// backend's.
    pub fn runtime(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Runtime(text) => Some(text),
            _ => None,
        })
    }

    /// Its commands' uid.
    pub fn uid(&self) -> Option<i64> {
        self.int(|f| match f {
            Field::Uid(n) => Some(*n),
            _ => None,
        })
    }

    /// Its commands' gid.
    pub fn gid(&self) -> Option<i64> {
        self.int(|f| match f {
            Field::Gid(n) => Some(*n),
            _ => None,
        })
    }

    /// Its directory.
    pub fn directory(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Directory(text) => Some(text),
            _ => None,
        })
    }

    /// Its process id.
    pub fn pid(&self) -> Option<i64> {
        self.int(|f| match f {
            Field::Pid(n) => Some(*n),
            _ => None,
        })
    }

    /// Its process's start time, as recorded.
    pub fn start(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Start(text) => Some(text),
            _ => None,
        })
    }

    /// Its process's boot, 32 hex digits, as recorded.
    pub fn boot(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Boot(text) => Some(text),
            _ => None,
        })
    }

    /// Its listener port.
    pub fn port(&self) -> Option<i64> {
        self.int(|f| match f {
            Field::Port(n) => Some(*n),
            _ => None,
        })
    }

    /// The image it runs.
    pub fn image(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Image(text) => Some(text),
            _ => None,
        })
    }

    /// Its labels; a record written before #373 has none.
    pub fn labels(&self) -> Result<Vec<(String, String)>> {
        let known = self.fields.find(|f| match f {
            Field::Labels(labels) => Some(labels),
            _ => None,
        });
        if let Some(labels) = known {
            return Ok(labels.clone());
        }
        // a list of another shape, read as before: each item's key and value
        match self.fields.kept("labels") {
            Some(SdkValue::List(items)) => (items.iter())
                .map(|item| {
                    Ok((
                        helpers::text(item, "key")?.to_owned(),
                        helpers::text(item, "value")?.to_owned(),
                    ))
                })
                .collect(),
            _ => Ok(Vec::new()),
        }
    }

    /// The forward of its daemon's port to this host (#535 G4), if any.
    pub fn forward(&self) -> Option<&Forward> {
        self.fields.find(|f| match f {
            Field::Forward(forward) => Some(forward),
            _ => None,
        })
    }

    /// Whether it names the retired docker backend: `runtime` `docker`, or
    /// none at all, which only a docker record lacked.
    pub fn retired(&self) -> bool {
        matches!(self.runtime(), None | Some("docker"))
    }
}

/// The fields a forward record knows.
#[derive(Clone, Debug, PartialEq)]
enum ForwardField {
    Address(String),
    Machine(String),
    Pid(i64),
    PidStart(String),
    PidBoot(String),
}

impl fields::Field for ForwardField {
    fn decode(name: &str, value: &SdkValue) -> Option<Self> {
        let text = || fields::text(value);
        Some(match name {
            "address" => Self::Address(text()?),
            "machine" => Self::Machine(text()?),
            "pid" => Self::Pid(fields::int(value)?),
            "pid_start" => Self::PidStart(text()?),
            "pid_boot" => Self::PidBoot(text()?),
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Address(_) => "address",
            Self::Machine(_) => "machine",
            Self::Pid(_) => "pid",
            Self::PidStart(_) => "pid_start",
            Self::PidBoot(_) => "pid_boot",
        }
    }

    fn encode(&self) -> SdkValue {
        match self {
            Self::Pid(n) => SdkValue::Int(*n),
            Self::Address(text)
            | Self::Machine(text)
            | Self::PidStart(text)
            | Self::PidBoot(text) => SdkValue::text(text),
        }
    }
}

/// A forward's process as observed: its start time and boot, when known.
pub type Carrier = Option<(u64, Option<u128>)>;

/// The forward of a worker's Sandbox daemon's port to this host: its host
/// address, the machine and the process that carries it, by id, start time
/// and boot.
#[derive(Clone, Debug, PartialEq)]
pub struct Forward {
    fields: Fields<ForwardField>,
}

impl Forward {
    fn decode(value: &SdkValue) -> Option<Self> {
        matches!(value, SdkValue::Map(_)).then(|| Self {
            fields: Fields::decode(value),
        })
    }

    /// A forward to `address` that machine `machine` carries, by process
    /// `pid` (with its start time and boot, when known).
    pub fn new(address: SocketAddr, machine: &str, pid: Option<(u32, Carrier)>) -> Self {
        let mut fields = vec![
            ForwardField::Address(address.to_string()),
            ForwardField::Machine(machine.to_owned()),
        ];
        if let Some((pid, process)) = pid {
            fields.push(ForwardField::Pid(i64::from(pid)));
            // the forward's identity, so a reused id is never taken for it
            if let Some((start, boot)) = process {
                fields.push(ForwardField::PidStart(start.to_string()));
                if let Some(boot) = boot {
                    fields.push(ForwardField::PidBoot(format!("{boot:032x}")));
                }
            }
        }
        Self {
            fields: Fields::of(fields),
        }
    }

    /// The host address it reaches the daemon at.
    pub fn address(&self) -> Option<SocketAddr> {
        let text = self.fields.find(|f| match f {
            ForwardField::Address(text) => Some(text),
            _ => None,
        })?;
        text.parse().ok()
    }

    /// The machine that carries it.
    pub fn machine(&self) -> Option<&str> {
        self.fields
            .find(|f| match f {
                ForwardField::Machine(text) => Some(text),
                _ => None,
            })
            .map(String::as_str)
    }

    /// The process that carries it, as recorded.
    pub fn pid(&self) -> Option<i64> {
        self.fields.get(|f| match f {
            ForwardField::Pid(n) => Some(*n),
            _ => None,
        })
    }

    /// Whether the process it names is alive, the same process when its
    /// start time and boot were recorded (`layerfs_home::process`).
    pub fn alive(&self) -> bool {
        let Some(pid) = self.pid().and_then(|p| u32::try_from(p).ok()) else {
            return false;
        };
        let start = self.fields.find(|f| match f {
            ForwardField::PidStart(text) => Some(text),
            _ => None,
        });
        let boot = self.fields.find(|f| match f {
            ForwardField::PidBoot(text) => Some(text),
            _ => None,
        });
        layerfs_home::process::alive_as(
            pid,
            start.and_then(|s| s.parse().ok()),
            boot.and_then(|b| u128::from_str_radix(b, 16).ok()),
        )
    }
}
