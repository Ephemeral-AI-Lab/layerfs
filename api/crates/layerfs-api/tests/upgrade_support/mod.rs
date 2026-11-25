//! What the `store upgrade` tests (#804) share: a home holding a frozen
//! fixture Store (written by a real old build, never faked), calls through
//! the call surface, and the comparison of a Store with its fixture's
//! manifest (every Branch head, every Commit id, every file at every head).
#![allow(dead_code)]
use layerfs_api::json;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The repository's `fixtures/store-formats`.
pub const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../fixtures/store-formats"
);

/// The SQLite schema-2 fixture file.
pub fn sqlite_v2() -> PathBuf {
    Path::new(FIXTURES).join("sqlite/v2/store.sqlite")
}

/// The PostgreSQL schema-3 fixture dump.
pub fn postgresql_v3() -> PathBuf {
    Path::new(FIXTURES).join("postgresql/v3/store.sql")
}

/// A fixture's manifest.
pub fn manifest(engine_version: &str) -> SdkValue {
    let path = Path::new(FIXTURES)
        .join(engine_version)
        .join("manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    json::parse(&text).unwrap()
}

pub fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

pub const DETAIL: (&str, SdkValue) = ("detail", SdkValue::Bool(true));

pub fn with(names: &[&str]) -> (&'static str, SdkValue) {
    ("with", SdkValue::List(names.iter().map(|n| t(n)).collect()))
}

/// A fresh private folder.
pub fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lfs-upgrade-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// One call, its events kept.
pub fn call_events(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
) -> (Result<SdkValue, SdkRefusal>, Vec<SdkValue>) {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    let mut events = Vec::new();
    let outcome = layerfs_api::call(Some(home), name, args, &mut |event| events.push(event));
    (outcome, events)
}

pub fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    call_events(home, name, args).0
}

pub fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// The phases of the progress events, in order.
pub fn phases(events: &[SdkValue]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| event.get("progress"))
        .filter_map(|progress| progress.get_text("phase").map(str::to_owned))
        .collect()
}

pub fn list(value: Option<&SdkValue>) -> Vec<SdkValue> {
    match value {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// Whether this build has the upgrade (the skeleton answers "not built yet"
/// and `--with format` adds nothing); says NOT_RUN when it does not.
pub fn built(home: &Path, store: &str) -> bool {
    let got = call(home, "store.get", &[("store", t(store)), with(&["format"])]);
    let built = got
        .as_ref()
        .is_ok_and(|value| value.get("format").is_some());
    if !built {
        eprintln!("NOT_RUN: this build has no store upgrade yet ({got:?})");
    }
    built
}

/// The SQLite fixture's binding (`history_meta.binding_key`), read on an
/// immutable connection: the record of the Store that holds it names it.
fn binding_of(file: &Path) -> Vec<u8> {
    let uri = format!("file:{}?immutable=1", file.display());
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let db = rusqlite::Connection::open_with_flags(uri, flags).unwrap();
    db.query_row("SELECT binding_key FROM history_meta", [], |row| row.get(0))
        .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    layerfs_bridge::sdk_values::hex(bytes)
}

/// `record.json` of Store `store` with its binding replaced by `binding`.
fn rebind(home: &Path, store: &str, binding: &[u8]) {
    let path = home.join("store").join(store).join("record.json");
    let record = std::fs::read_to_string(&path).unwrap();
    let key = r#""binding":{"hex":""#;
    let start = record.find(key).unwrap() + key.len();
    let end = start + record[start..].find('"').unwrap();
    let record = format!("{}{}{}", &record[..start], hex(binding), &record[end..]);
    std::fs::write(path, record).unwrap();
}

/// The minimal record each fixture project needs for a Store call.
fn project_records(home: &Path, store: &str, manifest: &SdkValue) {
    for (project, _) in fields(manifest.get("projects").unwrap()) {
        let dir = home.join("store").join(store).join("project").join(project);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("record.json"), "{\"v\":1}\n").unwrap();
    }
}

pub fn fields(value: &SdkValue) -> Vec<(String, SdkValue)> {
    match value {
        SdkValue::Map(fields) => fields.clone(),
        _ => Vec::new(),
    }
}

/// A home whose Store `fx` on `native` is the frozen SQLite schema-2
/// fixture: `store create fx`, its record bound to the fixture's binding,
/// the fixture copied over the file (closed), and each project's minimal
/// record. `None` (NOT_RUN) when the fixture is missing.
pub fn sqlite_fixture_home(tag: &str) -> Option<PathBuf> {
    let fixture = sqlite_v2();
    if !fixture.exists() {
        eprintln!("NOT_RUN: {fixture:?} is missing");
        return None;
    }
    let home = tempdir(tag).join("home");
    ok(&home, "home.init", &[]);
    ok(&home, "store.create", &[("name", t("fx"))]);
    layerfs_api::close_idle(std::time::Duration::ZERO);
    rebind(&home, "fx", &binding_of(&fixture));
    let file = home.join("store/fx/store.sqlite");
    std::fs::copy(&fixture, &file).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    project_records(&home, "fx", &manifest("sqlite/v2"));
    Some(home)
}

/// The PostgreSQL server of the tests: `LAYERFS_TEST_POSTGRES_URL` (an
/// admin URL of a trust-auth server, `postgresql://user@host:port/db?...`)
/// and `LAYERFS_TEST_PSQL`, the words of a `psql` command that reaches it
/// (for example `docker exec -i lfs-804-w3-pg psql -U postgres`): the dump
/// holds `COPY ... FROM stdin` and `\restrict`, which only psql reads.
pub struct Postgres {
    pub admin: String,
    psql: Vec<String>,
}

impl Postgres {
    pub fn from_env() -> Option<Self> {
        let (Ok(admin), Ok(psql)) = (
            std::env::var("LAYERFS_TEST_POSTGRES_URL"),
            std::env::var("LAYERFS_TEST_PSQL"),
        ) else {
            eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL / LAYERFS_TEST_PSQL unset");
            return None;
        };
        let psql = psql.split_whitespace().map(str::to_owned).collect();
        Some(Self { admin, psql })
    }

    pub fn client(&self, db: &str) -> postgres::Client {
        postgres::Client::connect(&self.url_of(db), postgres::NoTls).unwrap()
    }

    /// The admin URL with database `db`.
    pub fn url_of(&self, db: &str) -> String {
        let (base, query) = self.admin.split_once('?').unwrap_or((&self.admin, ""));
        let base = &base[..base.rfind('/').unwrap()];
        match query {
            "" => format!("{base}/{db}"),
            query => format!("{base}/{db}?{query}"),
        }
    }

    /// The Store URL of database `db`: the admin URL without its user.
    pub fn store_url(&self, db: &str) -> (String, String) {
        let url = self.url_of(db);
        let (scheme, rest) = url.split_once("://").unwrap();
        let (user, host) = rest.split_once('@').unwrap();
        (format!("{scheme}://{host}"), user.to_owned())
    }

    /// Restores the fixture dump into database `db`.
    pub fn restore(&self, db: &str, dump: &Path) {
        let file = std::fs::File::open(dump).unwrap();
        let status = std::process::Command::new(&self.psql[0])
            .args(&self.psql[1..])
            .args(["-d", db, "-q", "-v", "ON_ERROR_STOP=1"])
            .stdin(file)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "psql restore of {dump:?} into {db}");
    }

    pub fn drop_database(&self, db: &str) {
        let mut client = self.client("postgres");
        client
            .batch_execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .unwrap();
    }
}

/// A home whose Store `fx` is the frozen PostgreSQL schema-3 fixture in
/// database `db` of the test server: connected while the database was
/// empty (so the record and login are this build's), then the schema
/// replaced by the restored dump and the record bound to its binding.
/// `store connect` itself refuses a schema-3 database (it opens it to list
/// its projects).
pub fn postgresql_fixture_home(server: &Postgres, tag: &str, db: &str) -> PathBuf {
    server.drop_database(db);
    server
        .client("postgres")
        .batch_execute(&format!("CREATE DATABASE {db}"))
        .unwrap();
    let home = tempdir(tag).join("home");
    ok(&home, "home.init", &[]);
    let (url, user) = server.store_url(db);
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("fx")),
            ("url", t(&url)),
            ("user", t(&user)),
            ("password", t("unused")),
        ],
    );
    layerfs_api::close_idle(std::time::Duration::ZERO);
    server
        .client(db)
        .batch_execute("DROP SCHEMA layerfs CASCADE")
        .unwrap();
    server.restore(db, &postgresql_v3());
    let binding: Vec<u8> = server
        .client(db)
        .query_one("SELECT binding_key FROM layerfs.history_meta", &[])
        .unwrap()
        .get(0);
    rebind(&home, "fx", &binding);
    project_records(&home, "fx", &manifest("postgresql/v3"));
    home
}

fn sha256(path: &Path) -> String {
    hex(&Sha256::digest(std::fs::read(path).unwrap()))
}

/// Every page of a list call.
fn pages(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Vec<SdkValue> {
    let mut items = Vec::new();
    let mut cursor: Option<SdkValue> = None;
    loop {
        let mut asked: Vec<(&str, SdkValue)> = args.to_vec();
        asked.push(("limit", SdkValue::Int(1000)));
        if let Some(cursor) = cursor.take() {
            asked.push(("cursor", cursor));
        }
        let page = ok(home, name, &asked);
        items.extend(list(page.get("items")));
        match page.get("next_cursor") {
            Some(next @ SdkValue::Text(_)) => cursor = Some(next.clone()),
            _ => return items,
        }
    }
}

/// One Branch head's tree as the manifest writes it: per path its kind and
/// mode (`file stat`), content digest and size or link target (`project
/// export`), and the paths it shares an inode with.
pub fn tree(
    home: &Path,
    project: &str,
    branch: &str,
    scratch: &Path,
) -> BTreeMap<String, SdkValue> {
    let out = scratch.join(format!("{}-{branch}", project.replace('/', "-")));
    ok(
        home,
        "project.export",
        &[
            ("project", t(project)),
            ("destination", t(&out.display().to_string())),
            ("branch", t(branch)),
        ],
    );
    let mut files = BTreeMap::new();
    let mut inodes: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut stack = vec![out.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = format!("/{}", path.strip_prefix(&out).unwrap().display());
            let stat = ok(
                home,
                "file.stat",
                &[
                    ("project", t(project)),
                    ("branch", t(branch)),
                    ("path", t(&rel)),
                    DETAIL,
                ],
            );
            let kind = stat.get_text("type").unwrap().to_owned();
            let mut entry = vec![
                ("kind".to_owned(), t(&kind)),
                ("mode".to_owned(), stat.get("mode").cloned().unwrap()),
            ];
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                entry.push(("target".to_owned(), t(&target.display().to_string())));
            } else if meta.is_file() {
                entry.push(("sha256".to_owned(), t(&sha256(&path))));
                entry.push(("size".to_owned(), SdkValue::Int(meta.size() as i64)));
            } else if meta.is_dir() {
                stack.push(path.clone());
            }
            if kind == "file" {
                let inode = stat.get_int("inode").unwrap();
                inodes.entry(inode).or_default().push(rel.clone());
            }
            files.insert(rel, entry);
        }
    }
    for group in inodes.values().filter(|group| group.len() > 1) {
        let mut sorted = group.clone();
        sorted.sort();
        for rel in group {
            let links = SdkValue::List(sorted.iter().map(|p| t(p)).collect());
            files
                .get_mut(rel)
                .unwrap()
                .push(("hard_links".to_owned(), links));
        }
    }
    std::fs::remove_dir_all(&out).unwrap();
    files
        .into_iter()
        .map(|(path, mut entry)| {
            entry.sort_by(|a, b| a.0.cmp(&b.0));
            (path, SdkValue::Map(entry))
        })
        .collect()
}

/// The manifest's tree of one Branch, its keys sorted as [`tree`] sorts.
fn manifest_tree(files: &SdkValue) -> BTreeMap<String, SdkValue> {
    fields(files)
        .into_iter()
        .map(|(path, entry)| {
            let mut entry = fields(&entry);
            entry.sort_by(|a, b| a.0.cmp(&b.0));
            (path, SdkValue::Map(entry))
        })
        .collect()
}

/// Checks Store `fx` in `home` against `manifest`: the Branches and their
/// heads, every Branch's Commit ids in order, each project's Commit count,
/// and, with `files`, every path at every head (content byte-identical by
/// sha256, kind, mode, link targets and hard links).
pub fn matches_manifest(home: &Path, manifest: &SdkValue, files: bool) {
    let scratch = tempdir("export");
    for (name, project) in fields(manifest.get("projects").unwrap()) {
        let project_name = format!("fx/{name}");
        let said = project.get("branches").unwrap();
        let branches: BTreeMap<String, String> =
            pages(home, "branch.list", &[("project", t(&project_name))])
                .iter()
                .map(|b| {
                    let branch = b.get_text("branch").unwrap().to_owned();
                    (branch, b.get_text("commit").unwrap().to_owned())
                })
                .collect();
        let want: BTreeMap<String, String> = fields(said)
            .iter()
            .map(|(branch, b)| (branch.clone(), b.get_text("head").unwrap().to_owned()))
            .collect();
        assert_eq!(branches, want, "{project_name}: Branches and heads");
        let mut all = BTreeSet::new();
        for (branch, b) in fields(said) {
            let mut ids: Vec<String> = pages(
                home,
                "commit.list",
                &[("project", t(&project_name)), ("branch", t(&branch))],
            )
            .iter()
            .map(|c| c.get_text("commit").unwrap().to_owned())
            .collect();
            ids.reverse();
            let want: Vec<String> = list(b.get("commits"))
                .iter()
                .map(|c| match c {
                    SdkValue::Text(id) => id.clone(),
                    other => panic!("{other:?}"),
                })
                .collect();
            assert_eq!(ids, want, "{project_name} {branch}: Commit ids");
            all.extend(ids);
            if files {
                let have = tree(home, &project_name, &branch, &scratch);
                let want = manifest_tree(b.get("files").unwrap());
                let paths = |m: &BTreeMap<String, SdkValue>| m.keys().cloned().collect::<Vec<_>>();
                assert_eq!(paths(&have), paths(&want), "{project_name} {branch}: paths");
                for (path, entry) in &want {
                    assert_eq!(
                        have.get(path),
                        Some(entry),
                        "{project_name} {branch} {path}"
                    );
                }
            }
        }
        assert_eq!(
            Some(all.len() as i64),
            project.get_int("commit_count"),
            "{project_name}: Commit count"
        );
    }
    let _ = std::fs::remove_dir_all(scratch);
}
