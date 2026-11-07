//! One step on a sqlite Store's file (layerfs-project#109): the typed
//! request [`steps::run`](super::steps::run) takes, and the `store.file`
//! frame it travels in to a worker, `{ op, args: { store, store_record?,
//! ... } }`, encoded by the caller and decoded once in the worker.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A step and its arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum StoreStep {
    /// Whether the file is there, its size and the free bytes.
    Stat,
    /// The idle Store's bytes, sha256 and heads.
    Snapshot,
    /// A consistent snapshot of the live Store, staged beside it.
    Stage,
    /// Drops the staged snapshot.
    Unstage,
    /// A piece of the file (or, `staged`, of its staged snapshot).
    Read {
        /// Where the piece starts.
        offset: u64,
        /// Its length, at most [`CHUNK`](super::steps::CHUNK).
        len: u64,
        /// From the staged snapshot.
        staged: bool,
    },
    /// A piece of the transfer's staging file.
    Write {
        /// Where the piece goes; 0 starts the file again.
        offset: u64,
        /// The piece.
        bytes: Vec<u8>,
    },
    /// The staged copy is the snapshot: its `bytes`, `sha256` and
    /// `heads` (each null when the snapshot lacked it).
    Verify {
        /// The snapshot's length.
        bytes: Option<i64>,
        /// Its sha256, as hex.
        sha256: Option<String>,
        /// Its heads, as hex.
        heads: Option<String>,
    },
    /// Renames the verified copy into place.
    Install,
    /// Deletes a copy the record does not name.
    Retire,
    /// Deletes the Store's whole directory on the machine.
    Remove,
}

/// A step on Store `store`, with the Store record a stage opens it by.
#[derive(Clone, Debug)]
pub struct StepFrame {
    /// The Store.
    pub store: String,
    /// Its record, sent with every step of a backup.
    pub store_record: Option<SdkValue>,
    /// The step.
    pub step: StoreStep,
}

impl StoreStep {
    /// Its name in the frame.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stat => "stat",
            Self::Snapshot => "snapshot",
            Self::Stage => "stage",
            Self::Unstage => "unstage",
            Self::Read { .. } => "read",
            Self::Write { .. } => "write",
            Self::Verify { .. } => "verify",
            Self::Install => "install",
            Self::Retire => "retire",
            Self::Remove => "remove",
        }
    }
}

/// A non-negative whole `name` of the frame's args.
fn int(args: &SdkValue, name: &str) -> Result<u64> {
    match args.get(name) {
        Some(SdkValue::Int(n)) if *n >= 0 => Ok(*n as u64),
        _ => Err(refused(
            "InvalidInput",
            format!("`{name}` must be a whole number"),
        )),
    }
}

impl StepFrame {
    /// Step `step` on Store `store`, without its record.
    pub fn on(store: &str, step: StoreStep) -> Self {
        Self {
            store: store.to_owned(),
            store_record: None,
            step,
        }
    }

    /// The `store.file` frame.
    pub fn encode(&self) -> SdkValue {
        let int = |n: u64| SdkValue::Int(n as i64);
        let mut fields = vec![("store", SdkValue::text(&self.store))];
        if let Some(record) = &self.store_record {
            fields.push(("store_record", record.clone()));
        }
        match &self.step {
            StoreStep::Read {
                offset,
                len,
                staged,
            } => {
                fields.extend([("offset", int(*offset)), ("len", int(*len))]);
                if *staged {
                    fields.push(("staged", SdkValue::Bool(true)));
                }
            }
            StoreStep::Write { offset, bytes } => {
                fields.extend([
                    ("offset", int(*offset)),
                    ("bytes", SdkValue::Bytes(bytes.clone())),
                ]);
            }
            StoreStep::Verify {
                bytes,
                sha256,
                heads,
            } => {
                let text = |t: &Option<String>| t.as_deref().map(SdkValue::text);
                fields.extend([
                    ("bytes", bytes.map(SdkValue::Int).unwrap_or_default()),
                    ("sha256", text(sha256).unwrap_or_default()),
                    ("heads", text(heads).unwrap_or_default()),
                ]);
            }
            _ => {}
        }
        let args = fields.into_iter().map(|(n, v)| (n.to_owned(), v));
        SdkValue::map([
            ("op", SdkValue::text(self.step.name())),
            ("args", SdkValue::Map(args.collect())),
        ])
    }

    /// A `store.file` frame, refused in the order the worker always
    /// refused it: no step name, no Store, a bad Store name, an unknown
    /// step, then each step's arguments.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let op = frame
            .get_text("op")
            .ok_or_else(|| refused("InvalidInput", "Store step name"))?;
        let args = frame.get("args").unwrap_or(&SdkValue::Null);
        let store = args
            .get_text("store")
            .ok_or_else(|| refused("InvalidInput", "a Store step names its Store"))?;
        layerfs_home::names::StoreName::parse(store)?;
        let step = match op {
            "stat" => StoreStep::Stat,
            "snapshot" => StoreStep::Snapshot,
            "stage" => StoreStep::Stage,
            "unstage" => StoreStep::Unstage,
            "read" => StoreStep::Read {
                offset: int(args, "offset")?,
                len: int(args, "len")?,
                staged: matches!(args.get("staged"), Some(SdkValue::Bool(true))),
            },
            "write" => {
                let offset = int(args, "offset")?;
                let Some(SdkValue::Bytes(bytes)) = args.get("bytes") else {
                    return Err(refused("InvalidInput", "`bytes` must be bytes"));
                };
                StoreStep::Write {
                    offset,
                    bytes: bytes.clone(),
                }
            }
            "verify" => StoreStep::Verify {
                bytes: args.get_int("bytes"),
                sha256: args.get_text("sha256").map(str::to_owned),
                heads: args.get_text("heads").map(str::to_owned),
            },
            "install" => StoreStep::Install,
            "retire" => StoreStep::Retire,
            "remove" => StoreStep::Remove,
            _ => return Err(refused("InvalidInput", format!("no Store step {op}"))),
        };
        Ok(Self {
            store: store.to_owned(),
            store_record: args.get("store_record").cloned(),
            step,
        })
    }
}
