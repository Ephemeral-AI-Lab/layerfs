//! Bounded append-only records. Identity and input hashes precede product work.
use super::{
    common_streams::{compact_identity, hash_file, Stream},
    digest::{hex, sha256},
    json::Json,
};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

pub struct Input {
    pub invocation: [String; 8],
    pub db: PathBuf,
    pub output: PathBuf,
    pub assignment: PathBuf,
    pub base: PathBuf,
    pub replacements: PathBuf,
    pub identity_path: PathBuf,
}
fn future(path: &Path) -> io::Result<PathBuf> {
    Ok(path
        .parent()
        .ok_or_else(|| io::Error::other("input parent absent"))?
        .canonicalize()?
        .join(
            path.file_name()
                .ok_or_else(|| io::Error::other("input filename absent"))?,
        ))
}
fn text(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("diagnostic path UTF8"))
}
impl Input {
    pub fn parse(args: &[OsString]) -> io::Result<Self> {
        if args.len() != 8 {
            return Err(io::Error::other("E04 requires8arguments"));
        }
        let endpoint = args[1]
            .to_str()
            .ok_or_else(|| io::Error::other("endpoint UTF8"))?
            .to_owned();
        let assignment = Path::new(&args[2]).canonicalize()?;
        let base = Path::new(&args[3]).canonicalize()?;
        let replacements = Path::new(&args[4]).canonicalize()?;
        let db = future(Path::new(&args[5]))?;
        let output = future(Path::new(&args[6]))?;
        let identity_path = Path::new(&args[7]).canonicalize()?;
        if !Path::new(&args[5]).is_absolute()
            || !Path::new(&args[6]).is_absolute()
            || db.starts_with(&output)
            || [
                assignment.as_path(),
                base.as_path(),
                replacements.as_path(),
                identity_path.as_path(),
            ]
            .iter()
            .any(|path| path.starts_with(&output) || *path == db)
        {
            return Err(io::Error::other("separate absolute owned paths required"));
        }
        let invocation = [
            text(&std::env::current_exe()?.canonicalize()?)?,
            endpoint,
            text(&assignment)?,
            text(&base)?,
            text(&replacements)?,
            text(&db)?,
            text(&output)?,
            text(&identity_path)?,
        ];
        Ok(Self {
            invocation,
            db,
            output,
            assignment,
            base,
            replacements,
            identity_path,
        })
    }
}
pub struct Recorder {
    pub root: PathBuf,
    pub identity: String,
    pub identity_sha256: String,
    pub startup_record_id: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub identity_source_sha256: String,
    pub identity_source_bytes: usize,
    pub binary_sha256: String,
    pub invocation: [String; 8],
    pub assignment_sha256: String,
    pub base_sha256: String,
    pub replacements_sha256: String,
    pub acquisition_sha256: String,
    pub acquisition_bytes: u64,
    pub assignment_bytes: u64,
    pub write_attempts: u64,
    pub fixture_context: Option<String>,
    pub oracle_summary: Option<String>,
    pub startup: Stream,
    pub jobs: Stream,
    pub probes: Stream,
    pub trace: Stream,
    pub stdout: Stream,
    pub stderr: Stream,
    pub startup_status: &'static str,
    pub stop_status: &'static str,
    pub global_profile: Option<&'static str>,
    pub start: Instant,
    terminal: bool,
}
impl Recorder {
    pub fn create(input: &Input) -> io::Result<Self> {
        let mut bytes = Vec::new();
        File::open(&input.identity_path)?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err(io::Error::other("identity byte window"));
        }
        let original = std::str::from_utf8(&bytes)
            .map_err(io::Error::other)?
            .trim()
            .to_owned();
        let identity = compact_identity(&original)?;
        if !identity.starts_with('{') || !identity.ends_with('}') {
            return Err(io::Error::other("identity JSON object required"));
        }
        let assignment_sha256 = hash_file(&input.assignment)?;
        let assignment_bytes = std::fs::metadata(&input.assignment)?.len();
        let acquisition_path = input.assignment.with_extension("fixture");
        let acquisition_sha256 = hash_file(&acquisition_path)?;
        let acquisition_bytes = std::fs::metadata(&acquisition_path)?.len();
        let base_sha256 = hash_file(&input.base)?;
        let replacements_sha256 = hash_file(&input.replacements)?;
        let binary_sha256 = hash_file(Path::new(&input.invocation[0]))?;
        let identity_sha256 = hex(&sha256(original.as_bytes()));
        let mint = |role: &str| hex(&sha256(format!("{identity_sha256}:{role}:0").as_bytes()));
        let startup_record_id = mint("startup");
        let attempt_id = mint("attempt");
        let owner_id = mint("owner");
        std::fs::create_dir(&input.output)?;
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(input.output.join("identity.json"))?
            .write_all(original.as_bytes())?;
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(input.output.join("oracle-source.rs"))?
            .write_all(include_str!("oracle.rs").as_bytes())?;
        Ok(Self {
            root: input.output.clone(),
            identity,
            identity_sha256,
            startup_record_id,
            attempt_id,
            owner_id,
            identity_source_sha256: hex(&sha256(&bytes)),
            identity_source_bytes: bytes.len(),
            binary_sha256,
            invocation: input.invocation.clone(),
            assignment_sha256,
            assignment_bytes,
            acquisition_sha256,
            acquisition_bytes,
            base_sha256,
            replacements_sha256,
            write_attempts: 0,
            fixture_context: None,
            oracle_summary: None,
            startup: Stream::open(&input.output, "startup.jsonl")?,
            jobs: Stream::open(&input.output, "jobs.jsonl")?,
            probes: Stream::open(&input.output, "probes.jsonl")?,
            trace: Stream::open(&input.output, "trace.jsonl")?,
            stdout: Stream::open(&input.output, "stdout.txt")?,
            stderr: Stream::open(&input.output, "stderr.txt")?,
            startup_status: "NOT_RUN",
            stop_status: "NOT_RUN",
            global_profile: None,
            start: Instant::now(),
            terminal: false,
        })
    }
    pub fn at(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
    pub fn id(&self, role: &str, index: u64) -> String {
        hex(&sha256(
            format!("{}:{role}:{index}", self.identity_sha256).as_bytes(),
        ))
    }
    pub fn header(&self, out: &mut Json, kind: &str, index: u64) -> io::Result<()> {
        let id = if kind == "startup" {
            self.startup_record_id.clone()
        } else {
            self.id(kind, index)
        };
        self.header_with_id(out, kind, index, &id)
    }
    pub fn header_with_id(
        &self,
        out: &mut Json,
        kind: &str,
        index: u64,
        record_id: &str,
    ) -> io::Result<()> {
        out.raw("{\"schema\":\"cluster-two-job-receipts-v1\",\"case\":\"E04-write-16m\",\"mode\":\"diagnostic\",\"sample_count\":0,\"admission_eligible\":false,\"qualification_status\":\"NOT_EVALUATED\",\"global_persistence\":")?;
        match self.global_profile {
            Some(profile) => out.string(profile)?,
            None => out.raw("null")?,
        }
        out.raw(",")?;
        out.text("kind", kind)?;
        out.raw(",")?;
        out.field("record_index", index)?;
        for (key, value) in [
            ("record_id", record_id.to_owned()),
            ("attempt_id", self.attempt_id.clone()),
            ("owner_id", self.owner_id.clone()),
            ("identity_sha256", self.identity_sha256.clone()),
            ("actual_binary_sha256", self.binary_sha256.clone()),
        ] {
            out.raw(",")?;
            out.text(key, &value)?;
        }
        out.raw(",\"identity\":")?;
        out.raw(&self.identity)?;
        out.raw(",\"source_binding\":\"base-commit-plus-current-inventory\",")?;
        out.text("build_target_os", std::env::consts::OS)?;
        out.raw(",")?;
        out.text("build_target_architecture", std::env::consts::ARCH)?;
        out.raw(",\"invocation\":{\"schema\":\"cluster-two-e2-writes-invocation-v1\",\"path_scope\":\"process-local-canonical-inputs\",\"bound_before_startup\":true,\"argv\":[")?;
        for (index, value) in self.invocation.iter().enumerate() {
            if index != 0 {
                out.raw(",")?;
            }
            out.string(value)?;
        }
        out.raw("],")?;
        out.text("identity_source_sha256", &self.identity_source_sha256)?;
        out.raw(",")?;
        out.field("identity_source_bytes", self.identity_source_bytes)?;
        out.raw(",")?;
        out.text("identity_copy_sha256", &self.identity_sha256)?;
        out.raw(",\"docker_path_mapping\":{\"status\":\"UNAVAILABLE\",\"reason\":\"no-verified-host-container-path-mapping\"}}")
    }
    pub fn append(&mut self, stream: &'static str, body: Vec<u8>) -> io::Result<()> {
        if self.terminal {
            return Err(io::Error::other("original recorder is terminal"));
        }
        let result = match stream {
            "startup" => self.startup.append(&body),
            "jobs" => self.jobs.append(&body),
            "probes" => self.probes.append(&body),
            "trace" => self.trace.append(&body),
            "stdout" => self.stdout.append(&body),
            "stderr" => self.stderr.append(&body),
            _ => Err(io::Error::other("stream kind")),
        };
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
    pub fn healthy(&self) -> bool {
        !self.terminal
    }
    pub fn write_once(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        if !self.healthy() {
            return Err(io::Error::other("original recorder is terminal"));
        }
        let result = (|| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.root.join(name))?
                .write_all(bytes)
        })();
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
}
