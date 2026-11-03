//! The Store a daemon opens, beside its location.
use crate::{
    control::ControlError,
    provision::{StoreManifest, StoreProfile},
};
use std::fmt;
/// What any opener of a Store needs besides its location.
#[derive(Clone, Eq, PartialEq)]
pub struct StoreAuthority {
    /// The Store's binding key, from the home's Store record.
    pub binding: Vec<u8>,
    /// The Store's cursor key, from the home's Store record.
    pub cursor_key: [u8; 32],
}
impl fmt::Debug for StoreAuthority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreAuthority")
            .field("binding", &self.binding)
            .field("cursor_key", &"[redacted]")
            .finish()
    }
}
/// The Store a daemon serves; the spec's `SqliteInstall` is the existing
/// `StoreManifest`, so no new type names it.
#[derive(Clone, Eq, PartialEq)]
pub enum DaemonStore {
    /// The Store file at `locator` on this kernel. `install: None` waits for
    /// the one-time install over the control channel; `Some(manifest)` opens
    /// the installed Store at startup with its retained authority (never
    /// selected after an install error).
    Sqlite {
        /// The Store file's absolute path on the named in-VM volume.
        locator: String,
        /// An installed Store's manifest, or `None` for the installer.
        install: Option<StoreManifest>,
    },
    /// A PostgreSQL Store (#506); the URL may hold a password and lives only
    /// in this private setup, never in env, argv, records or `Debug`.
    Postgres {
        /// libpq-style connection string or URL, as the daemon dials it.
        url: String,
        /// Schema holding the Store: `[a-z_][a-z0-9_]*`, at most 63 bytes.
        schema: String,
        /// `off`, `local`, `remote_write`, `on` or `remote_apply`.
        synchronous_commit: String,
        /// The Store's binding and cursor keys, from the home's record.
        authority: StoreAuthority,
    },
}
impl DaemonStore {
    /// Whether the daemon opens an existing Store at startup (a file or
    /// PostgreSQL), so it starts ready with no install.
    pub fn opens_existing(&self) -> bool {
        match self {
            Self::Sqlite { install, .. } => install.is_some(),
            Self::Postgres { .. } => true,
        }
    }
    fn check(&self) -> Result<(), ControlError> {
        match self {
            Self::Sqlite { locator, install } => {
                let p = std::path::Path::new(locator);
                if !p.is_absolute()
                    || locator.len() > 4096
                    || locator.as_bytes().contains(&0)
                    || p.components().any(|c| {
                        matches!(
                            c,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )
                    })
                {
                    return Err(ControlError("daemon path"));
                }
                if let Some(m) = install {
                    m.check().map_err(ControlError)?;
                    if &m.locator != locator || m.profile != StoreProfile::Disposable {
                        return Err(ControlError("explicit existing Disposable Store"));
                    }
                }
                Ok(())
            }
            Self::Postgres {
                url,
                schema,
                synchronous_commit,
                authority,
            } => {
                let schema = schema.as_bytes();
                let valid = !url.is_empty()
                    && url.len() <= 4096
                    && !schema.is_empty()
                    && schema.len() <= 63
                    && !schema[0].is_ascii_digit()
                    && schema
                        .iter()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
                    && matches!(
                        synchronous_commit.as_str(),
                        "off" | "local" | "remote_write" | "on" | "remote_apply"
                    )
                    && !authority.binding.is_empty()
                    && authority.binding.len() <= 128
                    && authority.cursor_key != [0; 32];
                valid
                    .then_some(())
                    .ok_or(ControlError("daemon PostgreSQL Store"))
            }
        }
    }
    /// Encodes kind byte + fields after `DaemonSetup`'s fixed fields.
    pub(super) fn encode(&self, out: &mut crate::wire::Writer) -> Result<(), ControlError> {
        match self {
            Self::Sqlite { locator, install } => {
                out.byte(0)?;
                out.blob(locator.as_bytes())?;
                out.byte(u8::from(install.is_some()))?;
                if let Some(m) = install {
                    out.blob(&m.encode()?)?;
                }
            }
            Self::Postgres {
                url,
                schema,
                synchronous_commit,
                authority,
            } => {
                out.byte(1)?;
                for v in [url, schema, synchronous_commit] {
                    out.blob(v.as_bytes())?;
                }
                out.blob(&authority.binding)?;
                out.put(&authority.cursor_key)?;
            }
        }
        Ok(())
    }
    /// Decodes one store kind byte and its fields.
    pub(super) fn decode(input: &mut crate::wire::Reader) -> Result<Self, ControlError> {
        match input.byte()? {
            0 => Ok(Self::Sqlite {
                locator: input.text(4096)?,
                install: match input.byte()? {
                    0 => None,
                    1 => Some(StoreManifest::decode(input.blob(crate::wire::LIMIT)?)?),
                    _ => return Err(ControlError("daemon config boolean")),
                },
            }),
            1 => Ok(Self::Postgres {
                url: input.text(4096)?,
                schema: input.text(63)?,
                synchronous_commit: input.text(16)?,
                authority: StoreAuthority {
                    binding: input.blob(128)?.to_vec(),
                    cursor_key: input.array()?,
                },
            }),
            _ => Err(ControlError("daemon Store kind")),
        }
    }
    /// The whole `check` for the store, run by `DaemonSetup::check`.
    pub(super) fn valid(&self) -> Result<(), ControlError> {
        self.check()
    }
}
impl fmt::Debug for DaemonStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite { locator, install } => f
                .debug_struct("Sqlite")
                .field("locator", locator)
                .field("install", install)
                .finish(),
            Self::Postgres {
                schema,
                synchronous_commit,
                authority,
                ..
            } => f
                .debug_struct("Postgres")
                .field("url", &"[redacted]")
                .field("schema", schema)
                .field("synchronous_commit", synchronous_commit)
                .field("authority", authority)
                .finish(),
        }
    }
}
