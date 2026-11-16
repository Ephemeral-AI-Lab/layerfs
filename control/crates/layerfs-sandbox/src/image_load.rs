//! Copying a Sandbox image to a machine (`sandbox.create --image`,
//! `machine.load`; R1, R3): the image named by digest, Docker Id or Docker
//! reference is resolved, and when the machine does not hold it yet, it is
//! exported from the host's Docker as one flat root filesystem tar
//! ([`crate::image_tar`]), sent in 1 MiB frames and unpacked once there
//! ([`crate::image_artifact`]). The machine is asked first, so an image it
//! holds needs neither Docker nor a byte sent. One copy per image and machine
//! at a time: the second one waits in [`ImageLock`], and a copy whose owner
//! died is taken over at once (#175 R01). Progress phases are `inspect`,
//! `lock` (`waiting: "lock"` while another copy holds it), `export`, `send`
//! (`done`/`total` bytes) and `unpack`.
use crate::deploy::Side;
use crate::docker::{self, Docker, Failure, Inspected};
use crate::image_artifact::{self as cache, CHUNK};
use crate::image_tar::Filtered;
use crate::images::{refused_with, stopped};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, private_dir, read_record, write_record};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::process;
use layerfs_store::place::Worker;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A leftover export container older than this is removed by the next copy.
const STALE_SECS: u64 = 3600;
/// How often a waiting copy looks at the lock again: one look every 200 ms
/// reacts to a released or taken-over lock, and to a killed waiter, at once.
const LOCK_POLL: Duration = Duration::from_millis(200);
/// How often a waiting copy says so: the longest silence of a wait is this.
const WAIT_REPORT: Duration = Duration::from_secs(1);

/// The side an image is copied to: the image cache of `machine`, reached
/// through its worker or in this process (native Linux), and the `home` that
/// keeps the copy's lock.
pub struct Target<'a> {
    home: &'a Path,
    machine: &'a str,
    place: Place<'a>,
}

/// Where the bytes are written.
pub enum Place<'a> {
    /// This process, whose root is the home at the path.
    Local,
    /// The machine's worker.
    Worker(&'a mut Worker),
}

impl<'a> Target<'a> {
    /// A copy into this process's own image cache (`native` on Linux).
    pub fn local(home: &'a Path, machine: &'a str) -> Self {
        Target {
            home,
            machine,
            place: Place::Local,
        }
    }

    /// A copy into `machine`'s image cache through its worker.
    pub fn worker(home: &'a Path, machine: &'a str, worker: &'a mut Worker) -> Self {
        Target {
            home,
            machine,
            place: Place::Worker(worker),
        }
    }

    /// The home that keeps this copy's lock.
    pub fn home(&self) -> &Path {
        self.home
    }

    /// The machine whose image cache the copy fills.
    pub fn machine(&self) -> &str {
        self.machine
    }

    /// One `image.artifact` step there.
    fn step(&mut self, args: SdkValue) -> Outcome {
        match &mut self.place {
            Place::Worker(worker) => worker
                .attempt("image.artifact", args, &mut |_| {})
                .map_err(|unreached| stopped(unreached.0))?,
            Place::Local => cache::step(&Side::Home(self.home), &args),
        }
    }

    /// `probe` of image `id` (or of the image a sidecar names `reference`).
    fn probe(
        &mut self,
        id: Option<&str>,
        reference: Option<&str>,
        archive: Option<&str>,
    ) -> Outcome {
        let text = |v: Option<&str>| v.map_or(SdkValue::Null, SdkValue::text);
        self.step(SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("id", text(id)),
            ("ref", text(reference)),
            ("archive", text(archive)),
        ]))
    }
}

/// An image a machine holds after [`load`].
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    /// Its id there: the Docker image Id, or an archive's config digest.
    pub id: String,
    /// Its references.
    pub refs: Vec<String>,
    /// The bytes sent, else the sidecar's size, else what Docker's layers
    /// unpack to when nothing was sent.
    pub size: Option<u64>,
    /// Whether the machine held it already (nothing was sent).
    pub reused: bool,
}

impl Loaded {
    /// `machine.load`'s answer, `{id, refs, size, reused}`.
    pub fn to_value(&self) -> SdkValue {
        SdkValue::map([
            ("id", SdkValue::text(&self.id)),
            (
                "refs",
                SdkValue::List(self.refs.iter().cloned().map(SdkValue::Text).collect()),
            ),
            (
                "size",
                self.size
                    .map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            ("reused", SdkValue::Bool(self.reused)),
        ])
    }
}

fn phase(events: &mut Events<'_>, name: &str, bytes: Option<(u64, Option<u64>)>) {
    let mut fields = vec![("phase".to_owned(), SdkValue::text(name))];
    if let Some((done, total)) = bytes {
        fields.push(("done".to_owned(), SdkValue::Int(done as i64)));
        if let Some(total) = total {
            fields.push(("total".to_owned(), SdkValue::Int(total as i64)));
        }
    }
    events.progress(SdkValue::Map(fields));
}

/// Makes `image` (a digest, Docker Id or Docker reference) present on the
/// machine `target` reaches, whose Sandboxes read `archive` (its own image
/// archive, else the bundle's), copying it from `docker` when needed.
pub fn load(
    docker: &Docker,
    events: &mut Events<'_>,
    target: &mut Target<'_>,
    archive: Option<&str>,
    image: &str,
) -> Result<Loaded> {
    if image.trim().is_empty() {
        return Err(refused("InvalidInput", "the image is empty"));
    }
    phase(events, "inspect", None);
    let held = |probe: &SdkValue| probe.get_bool("present");
    // the size the machine's sidecar records, as `machine.images` lists it
    let kept = |probe: &SdkValue| probe.get_int("size").map(|n| n.max(0) as u64);
    // 1. the machine holds that Id already: Docker is not needed
    if cache::digest(image) {
        let probe = target.probe(Some(image), None, archive)?;
        if held(&probe) {
            return Ok(reused(image, Vec::new(), kept(&probe)));
        }
    }
    // 2. Docker's answer; 3. without Docker, a cached image of that ref
    let mut inspected = match docker.inspect(&[image]) {
        Ok(mut found) if !found.is_empty() => found.remove(0),
        Ok(_) => return Err(Failure::Missing(String::new()).refusal(image)),
        Err(Failure::Missing(m)) => return Err(Failure::Missing(m).refusal(image)),
        Err(failure) => {
            if !cache::digest(image) {
                let probe = target.probe(None, Some(image), archive)?;
                if let (true, Some(id)) = (held(&probe), probe.get_text("id")) {
                    return Ok(reused(id, vec![image.to_owned()], kept(&probe)));
                }
            }
            return Err(failure.refusal(image));
        }
    };
    docker.measure(std::slice::from_mut(&mut inspected));
    let Inspected { id, refs, .. } = &inspected;
    let mut probe = target.probe(Some(id), None, archive)?;
    let machine_arch = probe.get_text("architecture").unwrap_or("").to_owned();
    if inspected.os != "linux" {
        return Err(refused_with(
            "Unsupported",
            format!("image {image} is for {}, not linux", inspected.os),
            SdkValue::map([
                ("reason", SdkValue::text("os")),
                ("value", SdkValue::text(&inspected.os)),
            ]),
        ));
    }
    if inspected.architecture != machine_arch {
        return Err(refused_with(
            "Unsupported",
            format!(
                "image {image} is for {}; this machine runs {machine_arch}",
                inspected.architecture
            ),
            SdkValue::map([
                ("reason", SdkValue::text("arch")),
                ("value", SdkValue::text(&inspected.architecture)),
            ]),
        ));
    }
    if held(&probe) {
        let size = kept(&probe).or(Some(inspected.size));
        return Ok(reused(id, refs.clone(), size));
    }
    // one copy at a time of this image on this machine (#175 R01): a second
    // copy waits here, and takes over at once from one whose owner died
    let machine = target.machine().to_owned();
    let _lock = ImageLock::take(target.home(), &machine, id, events)?;
    // under the lock: the copy that held it may have published the image
    probe = target.probe(Some(id), None, archive)?;
    if held(&probe) {
        let size = kept(&probe).or(Some(inspected.size));
        return Ok(reused(id, refs.clone(), size));
    }
    // capacity before the first effect on the machine (#175 R03): no export
    // container, no staged byte and no cache folder; a resumed copy counts
    // the bytes it already staged once
    capacity(&machine, &probe, id, inspected.size)?;
    let container = container(&machine, id);
    phase(events, "export", None);
    remove_stale(docker, &container);
    created(docker, &container, id)?;
    let unpacked = docker
        .read(
            &[
                "container",
                "inspect",
                "--size",
                "--format",
                "{{.SizeRootFs}}",
                &container,
            ],
            docker::READ,
        )
        .ok()
        .and_then(|out| out.trim().parse::<u64>().ok())
        .filter(|n| *n > 0);
    // the container's root refines the progress estimate only: the capacity
    // decision above is taken before anything is created
    let estimate = Some(unpacked.unwrap_or(0).max(inspected.size)).filter(|n| *n > 0);
    let staged = probe.get_int("offset").unwrap_or(0).max(0) as u64;
    let prefix = probe.get_text("prefix_sha256").map(str::to_owned);
    let (sent, sha256) = match send(
        docker,
        events,
        target,
        &container,
        id,
        (staged, prefix),
        estimate,
    )? {
        Some(done) => done,
        // the staged bytes are not this export's prefix: start over
        None => send(docker, events, target, &container, id, (0, None), estimate)?
            .ok_or_else(|| refused("Failed", "the image export changed while it was sent"))?,
    };
    phase(events, "unpack", None);
    target.step(SdkValue::map([
        ("step", SdkValue::text("publish")),
        ("id", SdkValue::text(id)),
        ("size", SdkValue::Int(sent as i64)),
        ("sha256", SdkValue::text(&sha256)),
        (
            "refs",
            SdkValue::List(refs.iter().cloned().map(SdkValue::Text).collect()),
        ),
        ("architecture", SdkValue::text(&inspected.architecture)),
        ("config", inspected.config.clone()),
    ]))?;
    let _ = docker.read(&["rm", "-f", &container], docker::READ);
    Ok(Loaded {
        id: id.clone(),
        refs: refs.clone(),
        size: Some(sent),
        reused: false,
    })
}

fn reused(id: &str, refs: Vec<String>, size: Option<u64>) -> Loaded {
    Loaded {
        id: id.to_owned(),
        refs,
        size,
        reused: true,
    }
}

/// The lock of one image on one machine, in the home:
/// `<home>/run/image-lock/<machine>/<hex>.lock`, an exclusive `flock` held
/// for the whole copy, with its owner recorded beside it in `<hex>.json`
/// (pid, its kernel start time and boot, the machine and the image; #175
/// R01 and #176's intent).
///
/// The lock is the exclusion and the record is not: a waiter reads the record
/// only to say whom it waits for. An owner that dies releases the kernel
/// lock, so the next copy takes it over and resumes the staged part; a record
/// whose pid another process now holds does not keep the lock, because the
/// lock is not that process's. There is no deadline: a live owner is waited
/// for as long as it lives and the wait says `waiting: "lock"` every second.
/// Interrupting a waiting copy today means killing it, which leaves nothing
/// to undo — a released kernel lock and at most its record, which the next
/// copy replaces; a wait a shared Cancel can end is X01's, not this card's.
pub struct ImageLock {
    file: File,
    record: PathBuf,
}

impl Drop for ImageLock {
    fn drop(&mut self) {
        // the copy is over; a copy that died left its record behind for the
        // next owner to replace
        let _ = std::fs::remove_file(&self.record);
        layerfs_home::home::flock::unlock(&self.file);
    }
}

impl ImageLock {
    /// The folder of machine `machine`'s image locks in `home`.
    pub fn dir(home: &Path, machine: &str) -> PathBuf {
        home.join("run").join("image-lock").join(machine)
    }

    /// The lock file of `hex` on `machine`.
    pub fn path(home: &Path, machine: &str, hex: &str) -> PathBuf {
        Self::dir(home, machine).join(format!("{hex}.lock"))
    }

    /// The owner record beside it.
    pub fn record_path(home: &Path, machine: &str, hex: &str) -> PathBuf {
        Self::dir(home, machine).join(format!("{hex}.json"))
    }

    /// Takes the lock of image `id` on `machine`, waiting while a live copy
    /// holds it and recording this copy as the owner. `events` say
    /// `{phase: "lock", waiting: "lock", holder}` once a second meanwhile.
    pub fn take(home: &Path, machine: &str, id: &str, events: &mut Events<'_>) -> Result<Self> {
        let hex = hex_of(id).ok_or_else(|| refused("InvalidInput", "image id"))?;
        let dir = Self::dir(home, machine);
        private_dir(&dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(Self::path(home, machine, hex))
            .map_err(failed)?;
        let mut reported: Option<Instant> = None;
        loop {
            if layerfs_home::home::flock::try_lock(&file, true).map_err(failed)? {
                break;
            }
            if reported.is_none_or(|at| at.elapsed() >= WAIT_REPORT) {
                let record = Self::record_path(home, machine, hex);
                events.progress(SdkValue::map([
                    ("phase", SdkValue::text("lock")),
                    ("waiting", SdkValue::text("lock")),
                    (
                        "holder",
                        read_record(&record)
                            .ok()
                            .flatten()
                            .unwrap_or(SdkValue::Null),
                    ),
                ]));
                reported = Some(Instant::now());
            }
            std::thread::sleep(LOCK_POLL);
        }
        let lock = Self {
            file,
            record: Self::record_path(home, machine, hex),
        };
        lock.record(machine, id)?;
        Ok(lock)
    }

    /// Records this copy as the lock's owner before it does anything else.
    fn record(&self, machine: &str, id: &str) -> Result<()> {
        let now = home::now_ms();
        let mut fields = process::identity();
        fields.extend([
            ("machine", SdkValue::text(machine)),
            ("image", SdkValue::text(id)),
            ("since", SdkValue::text(cache::utc(now))),
            ("since_ms", SdkValue::Int(now)),
        ]);
        let record = SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        );
        write_record(&self.record, &record)
    }
}

/// The export container of image `id` for `machine`: one per image and
/// machine, so two machines copying the same image neither share nor remove
/// each other's container. Nothing runs in it.
fn container(machine: &str, id: &str) -> String {
    format!("layerfs-export-{machine}-{}", &id["sha256:".len()..])
}

/// `id`'s 64 hex digits, the name of its files under the cache and the lock.
fn hex_of(id: &str) -> Option<&str> {
    cache::digest(id).then(|| &id["sha256:".len()..])
}

/// Refuses `Capacity` before the copy creates anything on the machine or in
/// the home (#175 R03): `needed` is what the copy still has to find room for
/// — the bytes already staged for a resumed copy are on the machine's
/// filesystem and are counted once — `free` is what it has, and `where` names
/// the filesystem that was measured.
fn capacity(machine: &str, probe: &SdkValue, id: &str, size: u64) -> Result<()> {
    // a machine that does not say how much room it has cannot be refused
    let Some(free) = probe.get_int("free").map(|n| n.max(0) as u64) else {
        return Ok(());
    };
    let staged = probe.get_int("offset").unwrap_or(0).max(0) as u64;
    let needed = docker::need(size).saturating_sub(staged);
    if needed <= free {
        return Ok(());
    }
    let at = probe.get_text("root").unwrap_or(machine);
    let staged_note = if staged > 0 {
        format!(" ({staged} bytes are already staged and are not counted again)")
    } else {
        String::new()
    };
    let int = |n: u64| SdkValue::Int(n.min(i64::MAX as u64) as i64);
    Err(refused_with(
        "Capacity",
        format!(
            "machine {machine} needs {needed} more bytes in {at} to copy image {id}; {free} are free{staged_note}"
        ),
        SdkValue::map([
            ("needed", int(needed)),
            ("free", int(free)),
            ("where", SdkValue::text(at)),
        ]),
    ))
}

/// The export container `name` of image `id`: the one a failed copy kept
/// (so a retry exports identical bytes), else a new one. Nothing runs in it.
fn created(docker: &Docker, name: &str, id: &str) -> Result<()> {
    if docker
        .read(
            &["container", "inspect", "--format", "{{.Id}}", name],
            docker::READ,
        )
        .is_ok()
    {
        return Ok(());
    }
    let started = format!(
        "layerfs.export.started={}",
        layerfs_home::home::now_ms() / 1000
    );
    docker
        .read(
            &[
                "create",
                "--name",
                name,
                "--label",
                &started,
                "--network",
                "none",
                "--entrypoint",
                "/layerfs-export",
                id,
            ],
            Duration::from_secs(60),
        )
        .map(drop)
        .map_err(|f| f.refusal(id))
}

/// Removes export containers other copies left more than an hour ago.
fn remove_stale(docker: &Docker, keep: &str) {
    let Ok(listed) = docker.read(
        &[
            "ps",
            "-a",
            "--filter",
            "label=layerfs.export.started",
            "--format",
            "{{.Names}} {{.Label \"layerfs.export.started\"}}",
        ],
        docker::READ,
    ) else {
        return;
    };
    let now = (layerfs_home::home::now_ms() / 1000) as u64;
    for line in listed.lines() {
        let mut words = line.split_whitespace();
        let (Some(name), Some(started)) = (words.next(), words.next()) else {
            continue;
        };
        let old = started
            .parse::<u64>()
            .is_ok_and(|t| now.saturating_sub(t) > STALE_SECS);
        if name != keep && name.starts_with("layerfs-export-") && old {
            let _ = docker.read(&["rm", "-f", name], docker::READ);
        }
    }
}

/// Exports `container` and sends the filtered tar to the machine from byte
/// `staged` on, when the machine's staged bytes hash to `prefix` (else
/// `None`, nothing sent): the bytes in all and their sha256.
fn send(
    docker: &Docker,
    events: &mut Events<'_>,
    target: &mut Target<'_>,
    container: &str,
    id: &str,
    (staged, prefix): (u64, Option<String>),
    total: Option<u64>,
) -> Result<Option<(u64, String)>> {
    let mut child = docker
        .stream(&["export", container])
        .map_err(|f| f.refusal(id))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| refused("Failed", "docker export gave no output"))?;
    let mut tar = Filtered::new(stdout);
    let mut whole = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    // the staged prefix: hashed, and kept only when it matches this export
    if staged > 0 {
        let mut left = staged;
        let mut head = Sha256::new();
        while left > 0 {
            let want = (left as usize).min(CHUNK);
            let n = fill(&mut tar, &mut buffer[..want])?;
            if n == 0 {
                break;
            }
            head.update(&buffer[..n]);
            whole.update(&buffer[..n]);
            left -= n as u64;
        }
        let matched =
            left == 0 && Some(layerfs_bridge::sdk_values::hex(&head.finalize())) == prefix;
        if !matched {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
    }
    let mut offset = staged;
    let mut last = Instant::now();
    // `total` is an estimate: an event never shows more done than total
    let shown = |offset: u64| Some((offset, total.map(|t| t.max(offset))));
    phase(events, "send", shown(offset));
    loop {
        let n = fill(&mut tar, &mut buffer)?;
        if n == 0 {
            break;
        }
        whole.update(&buffer[..n]);
        target.step(SdkValue::map([
            ("step", SdkValue::text("write")),
            ("id", SdkValue::text(id)),
            ("offset", SdkValue::Int(offset as i64)),
            ("bytes", SdkValue::Bytes(buffer[..n].to_vec())),
        ]))?;
        offset += n as u64;
        if last.elapsed() >= Duration::from_millis(200) {
            phase(events, "send", shown(offset));
            last = Instant::now();
        }
    }
    let status = child.wait().map_err(failed)?;
    if !status.success() {
        return Err(refused(
            "Failed",
            format!("docker export {container} failed"),
        ));
    }
    // the last event is exact, with or without an estimate: done == total
    // == the bytes sent
    phase(events, "send", Some((offset, Some(offset))));
    Ok(Some((
        offset,
        layerfs_bridge::sdk_values::hex(&whole.finalize()),
    )))
}

/// Reads until `buffer` is full or the input ends.
fn fill(input: &mut impl Read, buffer: &mut [u8]) -> Result<usize> {
    let mut read = 0;
    while read < buffer.len() {
        let n = input.read(&mut buffer[read..]).map_err(failed)?;
        if n == 0 {
            break;
        }
        read += n;
    }
    Ok(read)
}

/// `machine.load(name, image)`: R1's copy without a Sandbox, `{id, refs,
/// size, reused}`.
pub fn machine_load(
    home: &Path,
    events: &mut Events<'_>,
    name: &layerfs_home::names::MachineName,
    image: &str,
) -> Outcome {
    use layerfs_store::place::{self, Executor};
    let machine = name.as_str();
    let choice = crate::deploy::choose(home, machine, None)?;
    let mut executor = place::on_machine(home, machine)?;
    if crate::images::host_mode(&executor) {
        return Err(crate::images::host_only(machine));
    }
    let docker = Docker::of(home)?;
    let archive = choice.archive.as_deref();
    let loaded = match &mut executor {
        Executor::Local => load(
            &docker,
            events,
            &mut Target::local(home, machine),
            archive,
            image,
        )?,
        Executor::Worker(worker) => load(
            &docker,
            events,
            &mut Target::worker(home, machine, worker),
            archive,
            image,
        )?,
    };
    Ok(loaded.to_value())
}
