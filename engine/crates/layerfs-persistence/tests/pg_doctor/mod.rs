#![allow(dead_code)]
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PostgresHandles, PostgresSelection,
};
use postgres::{Client, NoTls};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
pub mod namespace;
pub mod proxy;
pub const BINDING: &[u8] = b"doctor-postgres-acceptance";
pub struct Fixture {
    pub selection: PostgresSelection,
    pub writer: Option<PostgresHandles>,
    pub admin: Client,
    pub roles: Vec<String>,
}
impl Fixture {
    pub fn new(tag: &str) -> Option<Self> {
        Self::at(tag, std::env::var("LAYERFS_TEST_POSTGRES_URL").ok()?)
    }
    pub fn at(tag: &str, url: String) -> Option<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let schema = format!(
            "doctor_{tag}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let selection = PostgresSelection::new(url, &schema);
        let writer = PostgresHandles::create(
            &selection,
            layerfs_storage::StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: [71; 32],
                incarnation: 7,
            },
        )
        .unwrap();
        assert_eq!(
            selection.synchronous_commit,
            layerfs_persistence::SynchronousCommit::Off,
            "Disposable fixture"
        );
        let admin = Client::connect(&selection.url, NoTls).unwrap();
        Some(Self {
            selection,
            writer: Some(writer),
            admin,
            roles: Vec::new(),
        })
    }
    pub fn reader(&self) -> PostgresHandles {
        PostgresHandles::open_diagnostic(
            &self.selection,
            BINDING,
            [71; 32],
            Some(Instant::now() + Duration::from_secs(20)),
            &mut resolver,
        )
        .unwrap()
    }
    pub fn role(&mut self) -> String {
        let role = self.selection.schema.clone();
        self.admin.batch_execute(&format!("CREATE ROLE {role}; GRANT USAGE ON SCHEMA {role} TO {role}; GRANT SELECT ON ALL TABLES IN SCHEMA {role} TO {role}")).unwrap();
        self.roles.push(role.clone());
        role
    }
    pub fn restricted(&self, role: &str) -> PostgresSelection {
        let mut result = self.selection.clone();
        result.url.push_str(&format!("&options=-c%20role%3D{role}"));
        result
    }
    pub fn fingerprint(&mut self) -> Vec<(String, i64, String)> {
        let schema = &self.selection.schema;
        let rows=self.admin.query("SELECT table_name FROM information_schema.tables WHERE table_schema=$1 ORDER BY table_name",&[schema]).unwrap();
        rows.into_iter()
            .map(|r| {
                let table: String = r.get(0);
                let row = self.admin.query_one(&format!("SELECT count(*),md5(COALESCE(string_agg(md5(row_to_json(t)::text),'' ORDER BY md5(row_to_json(t)::text)),'')) FROM {schema}.{table} t"),&[]).unwrap();
                (table,row.get(0),row.get(1))
            })
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.writer.take();
        let _ = self
            .admin
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.selection.schema));
        for role in &self.roles {
            let _ = self.admin.batch_execute(&format!("DROP ROLE {role}"));
        }
    }
}
pub fn resolver(host: &str, port: u16, _until: Instant) -> std::io::Result<Vec<SocketAddr>> {
    host.parse()
        .map(|ip| vec![SocketAddr::new(ip, port)])
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "fixture accepts numeric endpoints",
            )
        })
}
pub fn audit(
    reader: &PostgresHandles,
    quick: bool,
) -> (
    Result<(), layerfs_storage::port::PersistenceError>,
    Vec<Observation>,
) {
    let mut out = Vec::new();
    let before = reader.diagnostics().unwrap();
    let result = reader.diagnose(
        Options {
            quick,
            deadline: Some(Instant::now() + Duration::from_secs(20)),
            cancelled: None,
        },
        &mut |o| out.push(o),
    );
    let after = reader.diagnostics().unwrap();
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.write_commits, before.write_commits);
    (result, out)
}
pub fn connection(selection: &PostgresSelection, binding: &[u8]) -> Vec<Observation> {
    let mut out = Vec::new();
    layerfs_persistence::diagnostic::postgresql_connection(
        selection,
        binding,
        Options {
            quick: true,
            deadline: Some(Instant::now() + Duration::from_secs(2)),
            cancelled: None,
        },
        &mut resolver,
        &mut |o| out.push(o),
    )
    .unwrap();
    out
}
pub fn has(out: &[Observation], id: &str, outcome: Outcome) -> bool {
    out.iter().any(|o| o.id == id && o.outcome == outcome)
}
pub fn no_corruption(out: &[Observation]) {
    assert!(!out.iter().any(|o| o.outcome == Outcome::Fail), "{out:?}");
}
