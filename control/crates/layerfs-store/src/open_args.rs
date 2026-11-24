//! What a Store call opens its Store with (layerfs-project#109 R8): the
//! Store file or key, the Store's identity sources and a PostgreSQL Store's
//! server. Read from an args map only at a map's edge, a worker's
//! `store.call` frame ([`CallFrame::open`](crate::call::CallFrame::open));
//! the engine call gets the call's own arguments without these keys
//! ([`ROUTING`]).
use crate::postgresql::open::{schema_of, setting_of, url_of};
use crate::record::{StoreRecord, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::refused;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The args-map keys that route a call and never reach the engine.
pub const ROUTING: [&str; 9] = [
    "storePath",
    "sandbox",
    "deploy",
    "home",
    "store",
    "postgresql",
    "store_record",
    "project_record",
    "place_view",
];

/// The Store a call opens and how. Each field is named for the frame field
/// it travels as.
#[derive(Clone, Default)]
pub struct OpenArgs {
    /// `home`; `None` takes `LAYERFS_HOME`.
    pub home: Option<PathBuf>,
    /// `store`.
    pub store: Option<String>,
    /// `project`.
    pub project: Option<String>,
    /// `storePath`: the Store file, or a PostgreSQL Store's key.
    pub store_path: Option<String>,
    /// `store_record`, as sent with the call.
    pub store_record: Option<SdkValue>,
    /// `postgresql`, when the call has one.
    pub postgresql: Option<PgSelection>,
}

/// A PostgreSQL Store's server as a call dials it (`postgresql: { url,
/// schema, synchronousCommit }`). No `Debug`: the URL holds the password.
#[derive(Clone, Default)]
pub struct PgSelection {
    /// `url`, password included.
    pub url: Option<String>,
    /// `schema`.
    pub schema: Option<String>,
    /// `synchronousCommit`.
    pub synchronous_commit: Option<String>,
}

impl OpenArgs {
    /// Store `store`'s file under `root` (a home, or a worker's own root),
    /// with the Store record `store_record` the call was sent or read.
    pub fn in_root(root: &Path, store: &str, store_record: Option<SdkValue>) -> Self {
        Self {
            home: Some(root.to_path_buf()),
            store: Some(store.to_owned()),
            store_path: Some(paths::store_file(root, store).display().to_string()),
            store_record,
            ..Self::default()
        }
    }

    /// Store `store` of `home` as its record `record` places it: a sqlite
    /// Store's file ([`in_root`](Self::in_root)), or a PostgreSQL Store's
    /// key dialled with its saved login
    /// ([`selection`](crate::postgresql::connect::selection)).
    pub fn of_record(home: &Path, store: &str, record: &StoreRecord) -> Result<Self> {
        let fields = Some(record.to_record().clone());
        match record.store_ref()? {
            StoreRef::Sqlite { .. } => Ok(Self::in_root(home, store, fields)),
            StoreRef::Postgres { .. } => {
                let postgresql = crate::postgresql::connect::selection(record)?;
                Ok(Self::dialled(home, store, record, postgresql))
            }
        }
    }

    /// PostgreSQL Store `store` of `home`, at the key its record `record`
    /// gives it, dialled with `postgresql`.
    pub fn dialled(
        home: &Path,
        store: &str,
        record: &StoreRecord,
        postgresql: PgSelection,
    ) -> Self {
        let key = record.postgresql_key(home, store);
        Self {
            home: Some(home.to_path_buf()),
            store: Some(store.to_owned()),
            store_path: Some(key.display().to_string()),
            store_record: Some(record.to_record().clone()),
            postgresql: Some(postgresql),
            ..Self::default()
        }
    }

    /// These arguments for a call on project `project`.
    pub fn with_project(self, project: &str) -> Self {
        Self {
            project: Some(project.to_owned()),
            ..self
        }
    }

    /// The Store file a call names (`storePath`).
    pub fn store_path(&self) -> Result<PathBuf> {
        match self.store_path.as_deref() {
            Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
            _ => Err(refused(
                "InvalidInput",
                "a Store call needs storePath, the Store volume's Store file",
            )),
        }
    }

}

impl PgSelection {
    /// The selection a `postgresql` value holds.
    pub fn read(value: &SdkValue) -> Self {
        let text = |name: &str| value.get_text(name).map(str::to_owned);
        Self {
            url: text("url"),
            schema: text("schema"),
            synchronous_commit: text("synchronousCommit"),
        }
    }

    /// The `postgresql` value [`read`](Self::read) reads: `{ url, schema,
    /// synchronousCommit }` in that order, a field it lacks left out. The
    /// one encoder, used only where a frame or args map carries it.
    pub fn encode(&self) -> SdkValue {
        let fields = [
            ("url", &self.url),
            ("schema", &self.schema),
            ("synchronousCommit", &self.synchronous_commit),
        ];
        SdkValue::Map(
            (fields.into_iter())
                .filter_map(|(name, value)| {
                    Some((name.to_owned(), SdkValue::text(value.as_deref()?)))
                })
                .collect(),
        )
    }

    /// Its URL, required.
    pub fn url(&self) -> Result<&str> {
        url_of(self.url.as_deref())
    }

    /// Its schema, checked.
    pub fn schema(&self) -> Result<String> {
        schema_of(self.schema.as_deref())
    }

    /// Its `synchronous_commit`, `local` by default.
    pub fn setting(&self) -> Result<String> {
        setting_of(self.synchronous_commit.as_deref())
    }
}
