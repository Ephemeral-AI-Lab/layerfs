//! A Store as its record names it, parsed once (#535 §6.1, R4): its kind,
//! the machine a sqlite Store is on, and a PostgreSQL Store's server. Every
//! decision on a Store's kind matches on [`StoreRef`] or [`StoreKind`]; no
//! caller compares a record's `kind` text.
use crate::host::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
pub(crate) use layerfs_bridge::StoreKind;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The kind a Store record names. A record with no `kind` is a sqlite
/// Store (the records #506 wrote before PostgreSQL); any other text is
/// refused, never read as sqlite.
pub(crate) fn kind_of(record: &SdkValue) -> Result<StoreKind> {
    match record.get("kind") {
        None => Ok(StoreKind::Sqlite),
        Some(SdkValue::Text(text)) => {
            StoreKind::parse(text).ok_or_else(|| refused("InvalidInput", "Store record kind"))
        }
        Some(_) => Err(refused("InvalidInput", "Store record kind")),
    }
}

/// Where a sqlite Store's record places it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// On this machine (`native` when the record names none).
    Machine(String),
    /// A 1.1.0 home's Store whose machine the conversion could not tell.
    Unknown,
}

/// A PostgreSQL Store's server, as its record names it (no password: the
/// record holds only a reference, resolved per call). A field the record
/// lacks is refused only by the step that needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PgServer {
    pub(crate) schema: Option<String>,
    pub(crate) synchronous_commit: Option<String>,
}

/// One Store, decided once from `store/<name>/record.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StoreRef {
    Sqlite { name: String, machine: Placement },
    Postgres { name: String, server: PgServer },
}

impl StoreRef {
    /// Parses Store `name`'s record; only its kind can refuse it.
    pub(crate) fn from_record(name: &str, record: &SdkValue) -> Result<Self> {
        let name = name.to_owned();
        Ok(match kind_of(record)? {
            StoreKind::Sqlite => Self::Sqlite {
                name,
                machine: match record.get("machine") {
                    None => Placement::Machine("native".to_owned()),
                    Some(SdkValue::Text(machine)) => Placement::Machine(machine.clone()),
                    Some(_) => Placement::Unknown,
                },
            },
            StoreKind::Postgresql => Self::Postgres {
                name,
                server: PgServer {
                    schema: record.get_text("schema").map(str::to_owned),
                    synchronous_commit: record.get_text("synchronous_commit").map(str::to_owned),
                },
            },
        })
    }

    pub(crate) fn kind(&self) -> StoreKind {
        match self {
            Self::Sqlite { .. } => StoreKind::Sqlite,
            Self::Postgres { .. } => StoreKind::Postgresql,
        }
    }

    /// The machine a sqlite Store is on. A PostgreSQL Store is on none, and
    /// a 1.1.0 Store whose machine is unknown is refused naming the fix.
    pub(crate) fn machine(&self) -> Result<&str> {
        match self {
            Self::Sqlite {
                machine: Placement::Machine(machine),
                ..
            } => Ok(machine),
            Self::Sqlite {
                name,
                machine: Placement::Unknown,
            } => Err(refused(
                "Unavailable",
                format!(
                    "Store {name} came from a 1.1.0 home and its machine is not known; \
                     run `layerfs import-env` or `layerfs store move {name} <machine>`"
                ),
            )),
            Self::Postgres { name, .. } => Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it is on no machine"),
            )),
        }
    }
}

/// A field a step needs, refused as a malformed Store record when absent.
pub(crate) fn required(field: Option<String>, key: &str) -> Result<String> {
    field.ok_or_else(|| refused("InvalidInput", format!("Store record {key}")))
}
