//! #63: what each PostgreSQL open check refuses (`PostgresCheck`). Both
//! refuse a wrong binding, incarnation, identity format or schema version;
//! only the full check reads `information_schema`, so only it refuses a
//! column changed by hand. Each open costs one round trip after connecting.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresCheck, PostgresHandles, PostgresSelection};
use layerfs_storage::{port::PersistenceError, StoragePolicy};

const BINDING: &[u8] = b"postgresql-open-check";
const CURSOR: [u8; 32] = [61; 32];

struct Schema {
    url: String,
    selection: PostgresSelection,
}
impl Schema {
    fn new(url: String, tag: &str) -> Self {
        let name = format!("lfs63_{tag}_{}", std::process::id());
        let schema = Self {
            selection: PostgresSelection::new(&url, &name),
            url,
        };
        schema.sql(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
        PostgresHandles::create(
            &schema.selection,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: CURSOR,
                incarnation: 3,
            },
        )
        .unwrap();
        schema
    }
    fn sql(&self, sql: &str) {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client.batch_execute(sql).unwrap();
    }
    fn open(&self, check: PostgresCheck, binding: &[u8]) -> Result<(), PersistenceError> {
        let selection = self.selection.clone().with_check(check);
        PostgresHandles::open_read_only(&selection, binding, CURSOR).map(|handles| {
            let work = handles.diagnostics().unwrap();
            assert_eq!(
                (work.connects, work.round_trips, work.prepares),
                (1, 1, 0),
                "{check:?}: one connection, one round trip, nothing prepared"
            );
        })
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        let _ = postgres::Client::connect(&self.url, postgres::NoTls).and_then(|mut c| {
            c.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE",
                self.selection.schema
            ))
        });
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

const CHECKS: [PostgresCheck; 2] = [PostgresCheck::Full, PostgresCheck::Light];

#[test]
fn both_checks_open_a_store_and_refuse_another_binding() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "binding");
    for check in CHECKS {
        schema.open(check, BINDING).unwrap();
        assert!(
            schema.open(check, b"another-binding").is_err(),
            "{check:?} refuses another Store's binding"
        );
    }
    // the reach check's attach reads the binding without knowing it
    let light = schema.selection.clone().with_check(PostgresCheck::Light);
    let (found, handles) = PostgresHandles::attach_read_only(&light, CURSOR).unwrap();
    assert_eq!(found, BINDING);
    assert!(!handles.storage.server_version().is_empty());
}

#[test]
fn both_checks_refuse_a_changed_incarnation_or_schema_version() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "meta");
    let s = &schema.selection.schema;
    // the rows' own CHECK constraints would refuse these hand changes
    schema.sql(&format!(
        "ALTER TABLE {s}.history_meta DROP CONSTRAINT history_meta_catalog_incarnation_check, \
         DROP CONSTRAINT history_meta_schema_version_check, \
         DROP CONSTRAINT history_meta_identity_format_check; \
         ALTER TABLE {s}.store_policy DROP CONSTRAINT store_policy_schema_version_check"
    ));
    schema.sql(&format!(
        "UPDATE {s}.history_meta SET catalog_incarnation=0"
    ));
    for check in CHECKS {
        assert!(
            schema.open(check, BINDING).is_err(),
            "{check:?}: incarnation"
        );
    }
    schema.sql(&format!(
        "UPDATE {s}.history_meta SET catalog_incarnation=3, schema_version=1"
    ));
    for check in CHECKS {
        // a schema version 1 schema (rows, before #91's blocks) is refused
        assert!(schema.open(check, BINDING).is_err(), "{check:?}: version");
    }
    schema.sql(&format!(
        "UPDATE {s}.history_meta SET schema_version=2, identity_format=2"
    ));
    for check in CHECKS {
        assert!(schema.open(check, BINDING).is_err(), "{check:?}: format");
    }
    schema.sql(&format!("UPDATE {s}.history_meta SET identity_format=3"));
    for check in CHECKS {
        schema.open(check, BINDING).unwrap();
    }
    // a policy row that does not validate is refused at open
    schema.sql(&format!("UPDATE {s}.store_policy SET schema_version=9"));
    for check in CHECKS {
        assert_eq!(
            schema.open(check, BINDING).err(),
            Some(PersistenceError::Malformed),
            "{check:?}: policy"
        );
    }
}

#[test]
fn only_the_full_check_reads_the_live_definition() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "columns");
    let s = &schema.selection.schema;
    schema.sql(&format!("ALTER TABLE {s}.reach ADD COLUMN extra bigint"));
    assert!(schema.open(PostgresCheck::Full, BINDING).is_err());
    schema.open(PostgresCheck::Light, BINDING).unwrap();
    // a version 1 wide location column (#90 narrowed `role` to smallint):
    // only the full check sees the type
    schema.sql(&format!(
        "ALTER TABLE {s}.reach DROP COLUMN extra; \
         ALTER TABLE {s}.object_location ALTER COLUMN role TYPE bigint"
    ));
    assert!(schema.open(PostgresCheck::Full, BINDING).is_err());
    schema.open(PostgresCheck::Light, BINDING).unwrap();
    schema.sql(&format!(
        "ALTER TABLE {s}.object_location ALTER COLUMN role TYPE smallint; \
         ALTER TABLE {s}.reach ADD COLUMN extra bigint"
    ));
    schema.open(PostgresCheck::Light, BINDING).unwrap();
    // an acquisition table gone: the full check sees the table set, the
    // light one the three names, so both refuse
    schema.sql(&format!(
        "ALTER TABLE {s}.reach DROP COLUMN extra; DROP TABLE {s}.init_entry"
    ));
    for check in CHECKS {
        assert!(schema.open(check, BINDING).is_err(), "{check:?}: tables");
    }
}
